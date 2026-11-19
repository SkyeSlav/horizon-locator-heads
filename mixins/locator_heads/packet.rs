// Every play packet, first: the waypoints the server tracks and the
// player list, for heads past loaded range. The game keeps the same
// two tables itself (`ClientWaypoints` and the Tab list); this mod
// keeps its own mirror in the engine, because the game's tables are
// private. The injection mirrors `../autologin/packet.rs`.
//
// `TrackedWaypoint` carries a position for every waypoint the server
// tracks — loaded or not — so a player past loaded range still gets a
// head, at the waypoint's block centre. `PlayerInfoUpdate` carries
// the UUID-to-name list, which the mixin cannot read from the game
// (`Remote::players` is private); a waypoint player whose name never
// arrived still gets a head, with no name. `Login` joins a new world,
// where the game clears its own waypoint table, so this mirror goes
// too.

fn locator_heads_packet(
    packet: &horizon_protocol::packets::play::Clientbound,
    game: &mut crate::game::Game<'_>,
) {
    use horizon_protocol::packets::play::{Clientbound as C, WaypointId, WaypointTarget};
    use locator_heads_engine as lh;
    let _ = game;
    match packet {
        C::TrackedWaypoint(p) => {
            let id = match &p.id {
                WaypointId::Uuid(u) => lh::WaypointId::Uuid(*u),
                WaypointId::Name(n) => lh::WaypointId::Name(n.clone()),
            };
            let target = match p.target {
                WaypointTarget::Empty => lh::WaypointTarget::Empty,
                WaypointTarget::Block(v) => lh::WaypointTarget::Block(v),
                WaypointTarget::Chunk(c) => lh::WaypointTarget::Chunk(c),
                WaypointTarget::Azimuth(a) => lh::WaypointTarget::Azimuth(a),
            };
            lh::note_waypoint(p.operation, id, p.style.clone(), p.color, target);
        }
        C::PlayerInfoUpdate(p) => {
            for e in &p.entries {
                if let Some(name) = &e.name {
                    lh::note_player_name(e.uuid, name.clone());
                }
            }
        }
        C::PlayerInfoRemove(p) => {
            for uuid in &p.uuids {
                lh::forget_player(*uuid);
            }
        }
        C::Login(_) => {
            lh::clear_waypoints();
            lh::clear_player_names();
        }
        _ => {}
    }
}
