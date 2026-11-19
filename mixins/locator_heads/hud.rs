// `native_gui_layer_post` (a hook point, API 139) for
// `minecraft:contextual_info_bar`: the heads sit on the locator bar,
// the compass above it.
//
// Heads draw only when the locator bar is the current contextual bar
// (the same `ContextualBar::next` the game uses): the original only
// ever drew inside vanilla's waypoint draw, so its heads vanished
// with the dots whenever the XP bar or jump bar took over. The compass
// is an independent HUD element in the original, so it is not gated.
// Known consequence: on a server that never sends TrackedWaypoint
// packets the bar is never Locator, so no heads ever show — exactly
// like the original mod there.
//
// The vanilla dots are suppressed while the locator bar shows: the
// pre-hook (`layer_pre.rs`) cancels this layer, and this post hook
// redraws what the game would have drawn — the locator background,
// the level number, and the vanilla dots for waypoints that are not
// players (location waypoints) — with the game's own draw code, then
// the heads on top. A head therefore replaces its dot at every
// distance: past loaded range the head comes from the server's
// `TrackedWaypoint` packet (the packet hook keeps the engine's mirror
// table current), positioned at the waypoint's block centre. The
// level number is the only piece of the XP bar redrawn here; the XP
// bar itself is never touched, and when any other bar is current the
// layer is not cancelled at all.
//
// Pinned to Horizon ac874422ed (see horizon-mixin.toml): it reads the
// game's own state — the loaded player mobs and their teams, the last
// frame's camera (`locator_camera`), the waypoint table the packet
// hook mirrors, and the GUI atlas's `object_skins` for the players'
// skin sprites — which no hook point's helper offers. It also reads
// `keys` + `screen` for PLAYER_LIST names, and the bar state
// (`waypoints`, `experience_recent()`, mount, mode) for the
// locator-bar gate.
// The skin lookup is the same one the HUD's own inline player heads use
// (`HudBatch::draw_inline_object`): `object_skins["id:<uuid>"]` (or
// `name:<lower>`), checked against the atlas's sprites, then the face
// (u 8..16) and hat (u 40..48) of the 64x64 skin, cropped with `rect_uv`.
// A player whose skin is not in the atlas yet — the list fills when the
// player list, chat or name tags resolve a profile — falls back to a
// coloured dot with their initials, never a faked head.
//
// A helper file holds exactly one private function, so the skin lookup
// and the head drawing below are closures inside it, not free functions.

fn locator_heads_layer(
    game: &crate::game::Game<'_>,
    layer: &str,
    batch: &mut horizon_client::hud::HudBatch,
) {
    use locator_heads_engine as lh;
    if layer != "minecraft:contextual_info_bar" {
        return;
    }
    if !game.native_on(lh::ID) {
        return;
    }
    let cfg = lh::config(&|k| game.native_setting(lh::ID, k));
    if !cfg.enabled {
        return;
    }
    let Some((cam_pos, yaw, cam_pitch, fov)) = game.locator_camera else {
        return;
    };
    // The current contextual bar, exactly as the game computes it
    // (`ContextualBar::next` in locator_bar.rs; the game draws this
    // layer every frame, whichever bar is current). Heads draw only
    // when the locator bar is showing.
    let spectator = game.spectator();
    let contextual = horizon_client::locator_bar::ContextualBar::next(
        game.waypoints.has_waypoints(),
        !spectator && game.jumpable_mount(),
        game.mount_jumping(),
        !game.creative && !spectator,
        game.experience_recent(),
    );
    let show_heads =
        contextual == horizon_client::locator_bar::ContextualBar::Locator;
    // Whether the player list is open: Tab held while playing
    // (`draw_tab_list` in game/scoreboard.rs recomputes exactly this;
    // on a remote server it reduces to Tab-held-while-playing). On
    // Android the Players touch button feeds the same `keys` set, so
    // hold-to-show works there too.
    let list_open = game.screen == crate::game::Screen::Playing
        && game.keys.contains(&winit::keyboard::KeyCode::Tab);
    // The view direction (`view_vector` in game.rs), for LOOKING_AT names.
    let (sy, cy) = (
        f64::from(yaw).to_radians().sin(),
        f64::from(yaw).to_radians().cos(),
    );
    let (sp, cp) = (
        f64::from(game.pitch).to_radians().sin(),
        f64::from(game.pitch).to_radians().cos(),
    );
    let view = [-sy * cp, -sp, cy * cp];
    let (gui_width, gui_height) = (batch.width as i32, batch.height as i32);

    // The skin sprite for a player, as the HUD's inline heads resolve it.
    let skin_sprite = |uuid: Option<u128>, name: &str| -> Option<String> {
        let atlas = &game.gui.atlas;
        let by_id = uuid.and_then(|u| atlas.object_skins.get(&format!("id:{u:032x}")));
        let by_name = atlas
            .object_skins
            .get(&format!("name:{}", name.to_ascii_lowercase()));
        let resolved = if uuid.is_some() {
            by_id.or(by_name)
        } else {
            by_name.or(by_id)
        };
        resolved
            .filter(|skin| atlas.sprites.contains_key(*skin))
            .cloned()
    };
    // A head at `(x, y)`, `size` GUI pixels square: the face, then the
    // hat layer over it, exactly as the HUD draws an inline player head.
    let draw_head = |batch: &mut horizon_client::hud::HudBatch,
                     skin: &str,
                     x: f32,
                     y: f32,
                     size: f32|
     -> bool {
        use horizon_client::hud::Rect;
        let Some(rect) = game.gui.atlas.sprites.get(skin).copied() else {
            return false;
        };
        for u in [8.0, 40.0] {
            let du = (rect.u1 - rect.u0) / 64.0;
            let dv = (rect.v1 - rect.v0) / 64.0;
            batch.rect_uv(
                x,
                y,
                size,
                size,
                Rect {
                    u0: rect.u0 + du * u,
                    v0: rect.v0 + dv * 8.0,
                    u1: rect.u0 + du * (u + 8.0),
                    v1: rect.v0 + dv * 16.0,
                    w: 8,
                    h: 8,
                },
                u32::MAX,
            );
        }
        true
    };

    // The other players: their mobs, with the names the server sent.
    // `remote_player_names` already leaves the local player out.
    let players: Vec<lh::PlayerIn> = game
        .remote_player_names()
        .into_iter()
        .filter_map(|(id, name)| {
            let m = game.mobs.iter().find(|m| m.id == id)?;
            if !m.alive() {
                return None;
            }
            let team_rgb = game
                .mob_team_of(m)
                .and_then(|t| t.color.as_deref())
                .and_then(horizon_client::scoreboard::team_color_rgb);
            let p = m.position;
            let eyes = m.eye_height();
            Some(lh::PlayerIn {
                name,
                uuid: game.remote_player_uuid(id),
                pos: [p.x, p.y + eyes, p.z],
                team_rgb,
            })
        })
        .collect();

    // The local player's UUID, exactly as the game's `own_uuid`:
    // the remote profile, else the singleplayer UUID from the level.
    let local_uuid = match &game.remote {
        Some(r) => r.profile(),
        None => game.level.player_uuid.map_or(0, |ints| {
            ints.iter()
                .fold(0u128, |acc, &i| acc << 32 | u128::from(i as u32))
        }),
    };
    // Waypoint players past loaded range: the server tracks them even
    // when no mob is loaded, so their heads come from the packet
    // hook's table, positioned at the waypoint's block centre (or
    // chunk middle / azimuth ray). A loaded player wins on UUID, and
    // the local player never gets a head. The name comes from the
    // player list the packet hook records; without it the head draws
    // with no name rather than being skipped. The team comes from the
    // scoreboard by name, as the original resolves teams by name; a
    // player with no scoreboard team falls back to the static/white
    // border, as a loaded player with no team does.
    let loaded_uuids: std::collections::HashSet<u128> =
        players.iter().filter_map(|p| p.uuid).collect();
    let waypoint_players: Vec<lh::PlayerIn> = lh::waypoints()
        .iter()
        .filter_map(|w| {
            let lh::WaypointId::Uuid(uuid) = w.id else {
                return None;
            };
            if uuid == local_uuid || loaded_uuids.contains(&uuid) {
                return None;
            }
            let name = lh::player_name(uuid).unwrap_or_default();
            let team_rgb = if name.is_empty() {
                None
            } else {
                game.score
                    .board
                    .players_team(&name)
                    .and_then(|t| t.color.as_deref())
                    .and_then(horizon_client::scoreboard::team_color_rgb)
            };
            Some(lh::PlayerIn {
                name,
                uuid: Some(uuid),
                pos: lh::waypoint_position(&w.target, cam_pos),
                team_rgb,
            })
        })
        .collect();

    let input = lh::FrameInput {
        cam_pos,
        yaw,
        view,
        gui_width,
        gui_height,
        list_open,
        players: &players,
        waypoint_players: &waypoint_players,
    };
    // Gated on the locator bar: while the XP bar (or jump bar) shows,
    // no marks are computed, so no heads, borders or names draw. The
    // name slide-in state simply pauses and resumes with the bar.
    let marks_list = if show_heads {
        lh::marks(&cfg, &input)
    } else {
        Vec::new()
    };
    // The pre-hook cancelled this layer (Locator only), so redraw
    // what the game would have drawn, with its own code, before the
    // heads go on top: the locator background, the level number, and
    // the vanilla dots for waypoints that are not players. The heads
    // replace the dots for players; location waypoints keep theirs.
    if show_heads {
        let (sprite, [bx, by, bw, bh]) =
            horizon_client::locator_bar::background(gui_width, gui_height);
        horizon_client::ui::nine_slice(
            batch,
            &game.gui.atlas,
            sprite,
            bx as f32,
            by as f32,
            bw as f32,
            bh as f32,
            u32::MAX,
        );
        let has_experience = !game.creative && !spectator;
        if has_experience && game.player.is_some() {
            let width = |s: &str| {
                horizon_client::hud::HudBatch::text_width(&game.gui.atlas, s) as i32
            };
            let draws = horizon_client::hud_layout::experience(
                gui_width,
                gui_height,
                game.experience.progress,
                game.experience.level,
                false,
                &width,
            );
            horizon_client::hud_layout::paint(batch, &game.gui.atlas, &draws);
        }
        // The vanilla dots for waypoints that are not players, drawn
        // with the game's own code: a filtered copy of its table, so
        // sprites, colours, arrows and blink match exactly. Player
        // waypoints are left out — their heads replace them — and so
        // is the local player's own waypoint.
        let mut dots = horizon_client::locator_bar::ClientWaypoints::default();
        for w in lh::waypoints() {
            let is_player = match &w.id {
                lh::WaypointId::Uuid(uuid) => {
                    *uuid == local_uuid
                        || loaded_uuids.contains(uuid)
                        || lh::player_name(*uuid).is_some()
                }
                lh::WaypointId::Name(_) => false,
            };
            if is_player {
                continue;
            }
            dots.apply(&horizon_protocol::packets::play::TrackedWaypoint {
                operation: horizon_protocol::packets::play::waypoint_operation::TRACK,
                id: match &w.id {
                    lh::WaypointId::Uuid(u) => {
                        horizon_protocol::packets::play::WaypointId::Uuid(*u)
                    }
                    lh::WaypointId::Name(n) => {
                        horizon_protocol::packets::play::WaypointId::Name(n.clone())
                    }
                },
                style: w.style.clone(),
                color: w.color,
                target: match w.target {
                    lh::WaypointTarget::Empty => {
                        horizon_protocol::packets::play::WaypointTarget::Empty
                    }
                    lh::WaypointTarget::Block(v) => {
                        horizon_protocol::packets::play::WaypointTarget::Block(v)
                    }
                    lh::WaypointTarget::Chunk(c) => {
                        horizon_protocol::packets::play::WaypointTarget::Chunk(c)
                    }
                    lh::WaypointTarget::Azimuth(a) => {
                        horizon_protocol::packets::play::WaypointTarget::Azimuth(a)
                    }
                },
            });
        }
        if let Some(p) = &game.player {
            // The game's own `View`, as `locator_bar_draws` builds
            // it: the locator camera, the player's position, and
            // interpolated mob eyes for loaded waypoint entities
            // (with the LAN-guest fallback, `PLAYER_EYES` 1.62).
            let alpha = game.clock.camera_fraction();
            let (dsy, dcy) = (
                f64::from(yaw.to_radians()).sin(),
                f64::from(yaw.to_radians()).cos(),
            );
            let (dsp, dcp) = (
                f64::from(cam_pitch.to_radians()).sin(),
                f64::from(cam_pitch.to_radians()).cos(),
            );
            let forward = [-dsy * dcp, -dsp, dcy * dcp];
            let up = [-dsy * dsp, dcp, dcy * dsp];
            let focal = 1.0 / f64::from((fov / 2.0).to_radians()).tan();
            let project = |o: [f64; 3]| {
                let dot = |a: [f64; 3]| a[0] * o[0] + a[1] * o[1] + a[2] * o[2];
                let depth = dot(forward);
                let y = dot(up) * focal / depth;
                [0.0, y, if depth < 0.0 { 2.0 } else { 0.5 }]
            };
            let entity = |id: u128| -> Option<([i32; 3], [f64; 3])> {
                let at = |p: [f64; 3], q: [f64; 3], eyes: f64| {
                    let pos: [f64; 3] =
                        std::array::from_fn(|i| q[i] + (p[i] - q[i]) * alpha);
                    (
                        p.map(|c| c.floor() as i32),
                        [pos[0], pos[1] + eyes, pos[2]],
                    )
                };
                if game.remote.is_some() {
                    let entity = game.remote.as_ref()?.player_entity(id)?;
                    let m = game.mobs.iter().find(|m| m.id == entity)?;
                    let pos = [m.position.x, m.position.y, m.position.z];
                    let prev = [m.previous.x, m.previous.y, m.previous.z];
                    return Some(at(pos, prev, m.eye_height()));
                }
                let host = game.lan.as_ref()?;
                let g = host
                    .hub
                    .guests
                    .iter()
                    .find(|g| g.uuid == id && g.placed)?;
                Some(at(g.position, g.previous_position, 1.62))
            };
            let view = horizon_client::locator_bar::View {
                gui_width,
                gui_height,
                camera_uuid: local_uuid,
                camera_entity_pos: [p.position.x, p.position.y, p.position.z],
                camera_pos: cam_pos,
                yaw,
                pitch: cam_pitch,
                fov,
                project: &project,
                entity: &entity,
                styles: &game.waypoint_styles,
                tick: game.ticks,
            };
            let draws = dots.draw(&view);
            horizon_client::hud_layout::paint(batch, &game.gui.atlas, &draws);
        }
    }
    for mark in marks_list {
        let size = mark.head_size;
        // Centred on vanilla's 9x9 dot slot.
        let x = mark.x as f32 + 4.5 - size / 2.0;
        let y = mark.top as f32 + 4.5 - size / 2.0;
        if let Some(argb) = mark.border_argb {
            let t = cfg.border_px();
            game.native_fill(batch, [x - t, y - t, size + 2.0 * t, t], argb);
            game.native_fill(batch, [x - t, y + size, size + 2.0 * t, t], argb);
            game.native_fill(batch, [x - t, y, t, size], argb);
            game.native_fill(batch, [x + size, y, t, size], argb);
        }
        let headed = cfg.render_heads
            && skin_sprite(mark.uuid, &mark.name)
                .is_some_and(|skin| draw_head(batch, &skin, x, y, size));
        if !headed {
            // No skin in the atlas (yet): a coloured dot with initials.
            let dot = mark.border_argb.unwrap_or(0xFF_FFFFFF);
            game.native_fill(batch, [x, y, size, size], dot);
            let initials: String =
                mark.name.chars().take(2).collect::<String>().to_ascii_uppercase();
            let w = game.native_text_width(&initials);
            game.native_text(
                batch,
                &initials,
                [x + size / 2.0 - w / 2.0, y + size / 2.0 - 4.0],
                0xFFFFFF,
                true,
            );
        }
        if mark.show_name {
            let w = game.native_text_width(&mark.name);
            game.native_text(
                batch,
                &mark.name,
                [mark.x as f32 + 4.5 - w / 2.0, y - 10.0 + mark.name_dy],
                mark.name_rgb,
                true,
            );
        }
    }

    // The compass, over the heads, as the original draws it last.
    if cfg.compass {
        for (letter, x) in lh::compass_letters(yaw, gui_width, cfg.compass_coords) {
            game.native_text(
                batch,
                letter,
                [x as f32 - 2.0, (gui_height - 31) as f32],
                cfg.compass_rgb(),
                cfg.compass_shadow,
            );
        }
    }
}
