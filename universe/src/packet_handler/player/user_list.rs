use crate::{get_conn_mut, universe_connection::UniverseConnectionID, UniverseServer};
use aw_core::*;

pub fn user_list(server: &mut UniverseServer, cid: UniverseConnectionID, packet: &AWPacket) {
    // This is normally based on the time, but it seems easier to just use the IDs we already have.
    let continuation_id = packet.get_uint(VarID::UserListContinuationID).unwrap_or(0);

    let conn = get_conn_mut!(server, cid, "user_list");

    let ip = conn.addr().ip();

    let Some(player) = conn.player_info_mut() else {
        return;
    };

    let name = player.username.clone();

    let player_list = &mut player.tabs.player_list;

    let current_list = player_list.current().clone();

    log::debug!(
        "Sending the full CURRENT player list to {} ({}) current: {:?}",
        ip,
        name,
        current_list
    );

    current_list.send_list_starting_from(conn, continuation_id);
}
