use crate::{
    database::{citizen::CitizenQuery, CitizenDB, UniverseDatabase},
    get_conn,
    universe_connection::UniverseConnectionID,
    UniverseServer,
};
use aw_core::*;
use aw_db::DatabaseResult;

pub fn citizen_change(server: &UniverseServer, cid: UniverseConnectionID, packet: &AWPacket) {
    let changed_info = match citizen_from_packet(packet) {
        Ok(changed_info) => changed_info,
        Err(why) => {
            log::trace!("Could not change citizen: {:?}", why);
            return;
        }
    };

    let mut rc = ReasonCode::Success;

    let conn = get_conn!(server, cid, "citizen_change");

    if let Some(player_citizen) = conn.client.as_ref().and_then(|x| x.citizen()) {
        // Client needs to be the user in question or an admin
        if changed_info.id != player_citizen.cit_id && !conn.has_admin_permissions() {
            rc = ReasonCode::Unauthorized;
        } else {
            match server.database.citizen_by_number(changed_info.id) {
                DatabaseResult::Ok(Some(original_info)) => {
                    if let Err(x) = modify_citizen(
                        &original_info,
                        &changed_info,
                        &server.database,
                        conn.has_admin_permissions(),
                    ) {
                        rc = x;
                    }
                }
                DatabaseResult::Ok(None) => {
                    rc = ReasonCode::NoSuchCitizen;
                }
                DatabaseResult::DatabaseError => {
                    rc = ReasonCode::DatabaseError;
                }
            }
        }
    }

    let mut response = AWPacket::new(PacketType::CitizenChangeResult);
    log::trace!("Change citizen: {:?}", rc);
    response.add_int(VarID::ReasonCode, rc as i32);

    conn.send(response);
}

fn citizen_from_packet(packet: &AWPacket) -> Result<CitizenQuery, String> {
    let username = packet
        .get_string(VarID::CitizenName)
        .ok_or_else(|| "No citizen name".to_string())?;
    let citizen_id = packet
        .get_uint(VarID::CitizenNumber)
        .ok_or_else(|| "No citizen number".to_string())?;
    let email = packet
        .get_string(VarID::CitizenEmail)
        .ok_or_else(|| "No citizen email".to_string())?;
    let priv_pass = packet
        .get_string(VarID::CitizenPrivilegePassword)
        .ok_or_else(|| "No citizen privilege password".to_string())?;
    let expiration = packet
        .get_uint(VarID::CitizenExpiration)
        .ok_or_else(|| "No citizen expiration".to_string())?;
    let bot_limit = packet
        .get_uint(VarID::CitizenBotLimit)
        .ok_or_else(|| "No citizen bot limit".to_string())?;
    let beta = packet
        .get_uint(VarID::BetaUser)
        .ok_or_else(|| "No citizen beta user".to_string())?;
    let enabled = packet
        .get_uint(VarID::CitizenEnabled)
        .ok_or_else(|| "No citizen enabled".to_string())?;
    let comment = packet
        .get_string(VarID::CitizenComment)
        .ok_or_else(|| "No citizen comment".to_string())?;
    let password = packet
        .get_string(VarID::CitizenPassword)
        .ok_or_else(|| "No citizen password".to_string())?;
    let url = packet
        .get_string(VarID::CitizenURL)
        .ok_or_else(|| "No citizen url".to_string())?;
    let cav_template = packet
        .get_uint(VarID::CAVTemplate)
        .ok_or_else(|| "No citizen cav template".to_string())?;
    let cav_enabled = packet
        .get_uint(VarID::CAVEnabled)
        .ok_or_else(|| "No citizen cav enabled".to_string())?;
    let privacy = packet
        .get_uint(VarID::CitizenPrivacy)
        .ok_or_else(|| "No citizen privacy".to_string())?;
    let trial = packet
        .get_uint(VarID::TrialUser)
        .ok_or_else(|| "No citizen trial".to_string())?;

    Ok(CitizenQuery {
        id: citizen_id,
        changed: 0,
        name: username,
        password,
        email,
        priv_pass,
        comment,
        url,
        immigration: 0,
        expiration,
        last_login: 0,
        last_address: 0,
        total_time: 0,
        bot_limit,
        beta,
        cav_enabled,
        cav_template,
        enabled,
        privacy,
        trial,
    })
}

fn modify_citizen(
    original: &CitizenQuery,
    changed: &CitizenQuery,
    database: &UniverseDatabase,
    admin: bool,
) -> Result<(), ReasonCode> {
    // Find any citizens with the same name as the new name
    match database.citizen_by_name(&changed.name) {
        DatabaseResult::Ok(Some(matching_cit)) => {
            // If someone already has the name, it needs to be the same user
            if matching_cit.id != original.id {
                return Err(ReasonCode::NameAlreadyUsed);
            }
        }
        DatabaseResult::Ok(None) => { /* No one else with same name */ }
        DatabaseResult::DatabaseError => return Err(ReasonCode::DatabaseError),
    };

    let cit_query = CitizenQuery {
        id: original.id,
        changed: 0,
        name: changed.name.clone(),
        password: changed.password.clone(),
        email: changed.email.clone(),
        priv_pass: changed.priv_pass.clone(),
        comment: if admin {
            changed.comment.clone()
        } else {
            original.comment.clone()
        },
        url: changed.url.clone(),
        immigration: original.immigration,
        expiration: if admin {
            changed.expiration
        } else {
            original.expiration
        },
        last_login: original.last_login,
        last_address: original.last_address,
        total_time: original.total_time,
        bot_limit: if admin {
            changed.bot_limit
        } else {
            original.bot_limit
        },
        beta: if admin { changed.beta } else { original.beta },
        cav_enabled: if admin {
            changed.cav_enabled
        } else {
            original.cav_enabled
        },
        cav_template: changed.cav_template,
        enabled: if admin {
            changed.enabled
        } else {
            original.enabled
        },
        privacy: changed.privacy,
        trial: if admin { changed.trial } else { original.trial },
    };

    match database.citizen_change(&cit_query) {
        DatabaseResult::Ok(_) => Ok(()),
        DatabaseResult::DatabaseError => Err(ReasonCode::UnableToChangeCitizen),
    }
}
