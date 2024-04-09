use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    client::ClientInfo,
    database::{contact::ContactOptions, CitizenDB, ContactDB, Database, TelegramDB},
    get_conn, get_conn_mut,
    tabs::{regenerate_contact_list, regenerate_contact_list_and_mutuals},
    telegram,
    universe_connection::UniverseConnectionID,
    UniverseConnection, UniverseServer,
};
use aw_core::*;

pub fn contact_add(server: &mut UniverseServer, cid: UniverseConnectionID, packet: &AWPacket) {
    let mut response = AWPacket::new(PacketType::ContactAdd);
    let conn = get_conn!(server, cid, "contact_add");

    let rc = match try_add_contact(conn, packet, &server.database) {
        Ok((cit_id, cont_id)) => {
            alert_friend_request(cit_id, cont_id, server);
            response.add_uint(VarID::ContactListCitizenID, cont_id);
            // response.add_uint(
            //     VarID::ContactListOptions,
            //     server.database.contact_get_default(cit_id).bits(),
            // );

            ReasonCode::Success
        }
        Err(x) => x,
    };

    log::info!("Contact add: {rc:?}");
    response.add_int(VarID::ReasonCode, rc as i32);

    conn.send(response);
    regenerate_contact_list(server, cid);
}

fn alert_friend_request(from: u32, to: u32, server: &UniverseServer) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Current time is before the unix epoch.")
        .as_secs() as u32;

    let citizen = match server.database.citizen_by_number(from) {
        Ok(x) => x,
        _ => return,
    };

    // Create a telegram to alert user of friend request
    let source_username = citizen.name;
    if server
        .database
        .telegram_add(to, from, now, &format!("\n\x01({from}){source_username}\n"))
        .is_err()
    {
        return;
    }

    // Alert recipient of new telegram
    if let Some(target_cid) = server.connections.get_by_citizen_id(to) {
        telegram::send_telegram_update_available(server, target_cid);
    }
}

fn try_add_contact(
    conn: &UniverseConnection,
    packet: &AWPacket,
    database: &Database,
) -> Result<(u32, u32), ReasonCode> {
    // Must be a player logged in as a citizen
    let client = conn.client.as_ref().ok_or(ReasonCode::NotLoggedIn)?;
    let citizen = client.citizen().ok_or(ReasonCode::NotLoggedIn)?;

    let citizen_id = citizen.cit_id;

    let contact_name = packet
        .get_string(VarID::ContactListName)
        .ok_or(ReasonCode::NoSuchCitizen)?;

    let contact_options = packet
        .get_uint(VarID::ContactListOptions)
        .ok_or(ReasonCode::NoSuchCitizen)?;

    let contact_citizen = database
        .citizen_by_name(&contact_name)
        .map_err(|_| ReasonCode::NoSuchCitizen)?;

    let mut options = ContactOptions::from_bits_truncate(contact_options);
    let other_has_blocked_you = database.contact_blocked(contact_citizen.id, citizen_id);
    if other_has_blocked_you && !options.contains(ContactOptions::ALL_BLOCKED) {
        return Err(ReasonCode::ContactAddBlocked);
    }

    let source_has_contact = database.contact_get(citizen_id, contact_citizen.id).is_ok();

    let target_has_contact = database.contact_get(contact_citizen.id, citizen_id).is_ok();

    // Stop people from adding each other when they are already friends
    if source_has_contact && target_has_contact {
        // Haven't checked if this is the right error code to send
        return Err(ReasonCode::UnableToSetContact);
    }

    options.remove(ContactOptions::FRIEND_REQUEST_ALLOWED);
    options.insert(ContactOptions::FRIEND_REQUEST_BLOCKED);

    database
        .contact_set(citizen_id, contact_citizen.id, options.bits())
        .map_err(|_| ReasonCode::UnableToSetContact)?;

    Ok((citizen_id, contact_citizen.id))
}

pub fn set_afk(server: &mut UniverseServer, cid: UniverseConnectionID, packet: &AWPacket) {
    let conn = get_conn_mut!(server, cid, "set_afk");

    let Some(client) = &mut conn.client else {
        return;
    };

    let Some(citizen) = client.citizen_mut() else {
        return;
    };

    let afk_status = match packet.get_uint(VarID::AFKStatus) {
        Some(x) => x,
        None => return,
    };

    let is_afk = afk_status != 0;
    citizen.base_player.afk = is_afk;
    log::debug!(
        "{:?} AFK: {:?}",
        citizen.base_player.username,
        citizen.base_player.afk
    );

    // Really only need to regenerate mutuals
    regenerate_contact_list_and_mutuals(server, cid);
}

pub fn contact_confirm(server: &mut UniverseServer, cid: UniverseConnectionID, packet: &AWPacket) {
    let conn = get_conn!(server, cid, "contact_confirm");

    let rc = match try_contact_confirm(conn, packet, &server.database) {
        Ok(_) => ReasonCode::Success,
        Err(x) => x,
    };

    let mut response = AWPacket::new(PacketType::ContactConfirm);
    response.add_int(VarID::ReasonCode, rc as i32);
    conn.send(response);
    regenerate_contact_list_and_mutuals(server, cid);
}

fn try_contact_confirm(
    conn: &UniverseConnection,
    packet: &AWPacket,
    database: &Database,
) -> Result<(), ReasonCode> {
    // Must be a player logged in as a citizen
    let client = conn.client.as_ref().ok_or(ReasonCode::NotLoggedIn)?;
    let citizen = client.citizen().ok_or(ReasonCode::NotLoggedIn)?;

    let citizen_id = citizen.cit_id;

    let contact_id = packet
        .get_uint(VarID::ContactListCitizenID)
        .ok_or(ReasonCode::NoSuchCitizen)?;

    if packet.get_int(VarID::ContactListOptions).unwrap_or(-1) == -1 {
        // Contact request denied
        return Ok(());
    }

    let contact_options = packet
        .get_uint(VarID::ContactListOptions)
        .ok_or(ReasonCode::NoSuchCitizen)?;

    let target_options = match database.contact_get(contact_id, citizen_id) {
        Ok(x) => x.options,
        // Fail if the target has no contact for this citizen (i.e. this contact was not requested)
        _ => return Err(ReasonCode::UnableToSetContact),
    };

    if !target_options.is_friend_request_allowed() {
        return Err(ReasonCode::UnableToSetContact);
    }

    // Fail if could not set the contacts
    if database.contact_set(citizen_id, contact_id, 0).is_err()
        || database.contact_set(contact_id, citizen_id, 0).is_err()
    {
        return Err(ReasonCode::UnableToSetContact);
    }

    log::info!(
        "Accepted contact {:?}",
        ContactOptions::from_bits_truncate(contact_options)
    );

    Ok(())
}

pub fn contact_list(server: &mut UniverseServer, cid: UniverseConnectionID, packet: &AWPacket) {
    let Some(starting_id) = packet.get_uint(VarID::ContactListCitizenID) else {
        return;
    };

    regenerate_contact_list(server, cid);

    let conn = get_conn_mut!(server, cid, "contact_list");

    let ip = conn.addr().ip();

    let Some(player) = conn.player_info() else {
        return;
    };

    let name = player.username.clone();

    let current_list = player.tabs.contact_list.current_starting_from(starting_id);

    log::debug!(
        "Sending the CURRENT contact list starting from id {starting_id} to {} ({}) current: {:?}",
        ip,
        name,
        current_list
    );

    current_list.send_limited_list(conn);
}

pub fn contact_delete(server: &mut UniverseServer, cid: UniverseConnectionID, packet: &AWPacket) {
    let Some(other_cit_id) = packet.get_uint(VarID::ContactListCitizenID) else {
        return;
    };

    let conn = get_conn!(server, cid, "contact_delete");

    let Some(self_cit_id) = conn.client.as_ref().and_then(ClientInfo::citizen_id) else {
        return;
    };

    let blocked_by_other_person = server.database.contact_blocked(other_cit_id, self_cit_id);

    let mut rc = ReasonCode::Success;
    match server.database.contact_delete(self_cit_id, other_cit_id) {
        Ok(()) => {
            if !blocked_by_other_person {
                if let Err(why) = server.database.contact_delete(other_cit_id, self_cit_id) {
                    log::warn!("Unable to delete contact (1) {other_cit_id} {self_cit_id} {why:?}");
                }
            }
        }
        Err(why) => {
            log::warn!("Unable to delete contact (2) {self_cit_id} {other_cit_id} {why:?}");
            rc = ReasonCode::UnableToSetContact;
        }
    }

    let mut response = AWPacket::new(PacketType::ContactDelete);
    response.add_uint(VarID::ReasonCode, rc.into());
    conn.send(response);

    regenerate_contact_list(server, cid);

    if let Some(other_cid) = server.connections.get_by_citizen_id(other_cit_id) {
        regenerate_contact_list(server, other_cid);
    }
}

pub fn contact_change(server: &mut UniverseServer, cid: UniverseConnectionID, packet: &AWPacket) {
    let Some(option_changes) = packet
        .get_uint(VarID::ContactListOptions)
        .and_then(ContactOptions::from_bits)
    else {
        return;
    };

    let Some(contact_cit_id) = packet.get_uint(VarID::ContactListCitizenID) else {
        return;
    };

    let Some(self_citizen_id) = get_conn!(server, cid, "contact_change")
        .client
        .as_ref()
        .and_then(ClientInfo::citizen_id)
    else {
        return;
    };

    let original_options = match server.database.contact_get(self_citizen_id, contact_cit_id) {
        Ok(q) => q.options,
        // The user may not have an entry for a contact with 0 yet
        Err(_) if contact_cit_id == 0 => ContactOptions::empty(),
        Err(_) => return,
    };

    let new_options = original_options.apply_changes(option_changes);

    server
        .database
        .contact_set(self_citizen_id, contact_cit_id, new_options.bits())
        .ok();

    if option_changes.contains(ContactOptions::ALL_BLOCKED) {
        server
            .database
            .contact_delete(contact_cit_id, self_citizen_id)
            .ok();
    }

    if contact_cit_id == 0 {
        if let Ok(mut cit_q) = server.database.citizen_by_number(self_citizen_id) {
            cit_q.privacy = new_options.bits();
            server.database.citizen_change(&cit_q).ok();
        }
    }

    regenerate_contact_list_and_mutuals(server, cid);
}
