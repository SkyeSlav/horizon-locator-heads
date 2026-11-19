# Horizon Locator Heads

Shows the heads of nearby players on the locator bar, with a compass,
team-coloured borders, name modes and include/exclude filters.
Version 1.0.0, `.hnmod` only, for Starling 0.8.0 (game 26.3), pinned to
Horizon/Starling revision `ac874422ed`.

An independent Rust reimplementation of the idea of Haage001's
[Locator Heads](https://github.com/Haage001/locator-heads) (Fabric,
LGPL-3.0) — see `NOTICE.md`. No Java source was copied. One deliberate
difference from the original's data source: the original shows only
the waypoints the server tracks for the player. This mod shows every
loaded player mob, and waypoint players past loaded range from the
server's `TrackedWaypoint` packets, so a head replaces the vanilla
dot at every distance.

## What it does

- **Heads on the bar.** Every other player on the server gets a
  head on the XP/locator bar by bearing relative to where you look,
  inside Horizon's ±60° window, using Horizon's own locator math
  (`yaw_to`, `x = middle + floor(angle*173/2/60)`). Farthest first, so
  nearer heads draw over farther ones. Heads shrink with distance,
  matching vanilla's waypoint style defaults (near_distance 128,
  far_distance 332): full size up to 128 blocks, shrinking linearly
  to 5/9 at 332 blocks and beyond — the original lerps head size from
  9 px at near to 5 px at far, and the port follows the same range
  and far ratio (its base head size stays 8 px at 1.0x). At the
  distances you normally see other players, heads stay full size.
  Past loaded range the head comes from the server's waypoint
  packet instead: its position is the waypoint's block centre (or
  chunk middle, or azimuth ray), its name from the player list, its
  team from the scoreboard by name. A loaded player always wins
  over its waypoint entry, so nobody draws twice. A waypoint
  player whose name never arrived still gets a head, with no name
  rather than being skipped; one with no scoreboard team falls back
  to the static/white border.
- **Heads replace the dots entirely.** While the locator bar shows,
  the vanilla dots are suppressed — the pre-hook cancels the layer
  and the post hook redraws what the game would have drawn (the
  locator background, the level number, and the vanilla dots for
  waypoints that are not players, with the game's own draw code, so
  sprites, colours, pitch arrows and blink match exactly) — then
  the heads on top. A location waypoint keeps its dot; a player
  waypoint never shows one. The XP bar itself is never redrawn and
  no other layer is ever cancelled.
- **Heads only when the locator bar shows.** The mod computes the
  current contextual bar the same way the game does
  (`ContextualBar::next`) and draws heads, borders and names only
  when it is the locator bar — exactly like the original, whose
  heads only ever existed inside vanilla's waypoint draw. Gain XP
  and the heads vanish for ~5 s with the dots while the XP bar
  shows, then come back; they never float on top of the XP bar.
  The compass is not gated (it is an independent HUD element in
  the original). Known consequence: on a server that never sends
  waypoint packets the bar is never the locator bar, so no heads
  ever show — exactly like the original mod there.
- **Real heads, with a fallback.** The head is the player's skin from
  the GUI atlas (`object_skins["id:<uuid>"]`, else `name:<lower>`),
  cropped to the face (u 8..16) with the hat layer (u 40..48) over it —
  the same lookup the HUD's own inline player heads use. A player
  whose skin is not in the atlas yet (the list fills when the player
  list, chat or name tags resolve a profile) gets a coloured dot with
  their initials instead. Nothing is faked: no head is drawn unless
  the atlas has that player's skin sprite.
- **Borders.** Around each head, in the player's team colour when they
  have one, else the static colour; or one static colour for all.
  Thickness Thin/Medium/Normal/Thick. Off by default, as in the
  original. The static colour is white by default; set it with
  `/lh bordercolor <hex>` (e.g. `/lh bordercolor FF0000`, `#` or `0x`
  prefix allowed) or cycle the presets on the settings page.
- **Names.** Off (default), Always, Looking at (a 20° cone around
  the view direction), or Player list (names show while the player
  list is open — Tab held on keyboard, Players button held on touch).
  Names are tinted with the head's border colour, as in the original,
  and slide up 20 px over 150 ms when they first appear. No fade:
  the HUD text helper takes opaque colours only, so slide only.
- **Compass.** N/E/S/W letters over a 120° field of view, at
  `screenHeight - 31`, configurable colour, drawn over the heads.
  Off by default, as in the original. Coordinates notation swaps the
  letters for `+Z`/`-X`/`-Z`/`+X`; the shadow under the letters can be
  toggled (on by default, as in the original). (The original fades
  letters towards the edges; this build draws them at full opacity —
  the HUD text helper takes an opaque colour.)
- **Filtering.** All players, Include only, or Exclude only, by name
  list (comma/semicolon/colon/period separated, case-insensitive).
- **Settings.** Options > Mods > Locator Heads 1.0.0, thirteen buttons
  that cycle; the filter lists are free text, set in chat:
  `/lh filter include Alice, Bob`, `/lh filter exclude Carol`,
  `/lh filter clear`; the static border colour is set with
  `/lh bordercolor <hex>`. `/lh` shows the status, `/lh toggle`
  flips the mod on/off. Settings persist in the mod's data folder,
  keyed by mod id, not version — updating the file to a newer 1.0.0
  build keeps them.

## Hook points and pins

- `gui_layer_pre` (API 139, a hook point) for
  `minecraft:contextual_info_bar`: cancels the layer only when the
  locator bar is the current bar, so the vanilla dots are suppressed
  and the post hook redraws the bar with heads in place of them.
- `gui_layer_post` (API 139, a hook point) for
  `minecraft:contextual_info_bar`: redraws the locator background,
  the level number and the non-player waypoint dots, then the heads,
  the compass over the heads.
- `handle_packet` (a target injection, the shape the autologin mod
  uses) on `horizon_client::game::remote::Game::handle_packet`:
  observes `TrackedWaypoint` (the mod's own waypoint table),
  `PlayerInfoUpdate`/`PlayerInfoRemove` (UUID-to-name), and `Login`
  (clears both tables on joining a new world). Observed only; no
  packet is sent, blocked or rewritten.
- `command`, `commands`, `mods`, `settings`, `settings_click` (hook
  points) for `/locatorheads` / `/lh`, the Mods row and the settings page.
- Pinned to Horizon `ac874422ed` anyway, because the HUD adapter reads
  the game's own state, which no hook point's helper offers: the loaded
  player mobs and their teams (`self.mobs`, `remote_player_names`,
  `remote_player_uuid`, `mob_team_of`), the last frame's camera
  (`locator_camera`), the GUI atlas's `object_skins`, the Tab key and
  screen (`keys`, `screen`) for PLAYER_LIST names, the bar state
  (`waypoints`, `experience_recent()`, mount, mode) for the
  locator-bar gate, and the scoreboard by name (`score.board`) plus
  the level's singleplayer UUID (`level.player_uuid`) for waypoint
  players. A Horizon
  commit that renames any of those fails this build with the compiler's
  error, as the Starling build server will report.

## Deferred in v1

- **Always show XP (full).** Deferred: the original renders the locator
  bar over the experience bar when this is on. In Horizon the bar choice
  (`ContextualBar::next`) is computed inline in the HUD draw with no
  hook point over it; forcing Experience would mean cancelling the
  whole `contextual_info_bar` layer and re-drawing the XP bar by hand
  (the pieces exist as public functions, but the mod would then own
  every bar state).
- **Compass edge fade.** Omitted (see above): letters are full opacity.
  Names fade the same way: slide-in only, no alpha fade.

## Build

```sh
python3 tools/package.py            # target/locator-heads-1.0.0.hnmod
python3 tools/submit.py             # compile on the kara-box build server
```

The engine crate (`mixins/locator_heads/engine`) is pure Rust and has
its own tests: `cargo test` in that folder (24 tests: bearings, the
±60° window, the 20° name cone, filters, compass letters and
coordinates notation, distance scaling, hex colours, the bordercolor
command, name modes including Player list, name tint and slide-in,
settings round-trip, commands, plus the waypoint table's TRACK/UNTRACK/
UPDATE semantics, the player-list names, waypoint positions by target
kind, the loaded/waypoint merge and dedupe, and unnamed heads passing
the filter).

## Install and phone test (the gate)

A clean compile is not a phone test. On the phone, in Starling 0.8.0:

1. Native mods card → Build server… → type the kara-box address by
   hand (`https://8787-agq3ej7hk3etn2c3sruyrryuhy.instabox.tech`),
   if it is not already set.
2. Install `locator-heads-1.0.0.hnmod` and Compile. Same version as
   the build before it, so Starling cannot tell them apart: if the
   row still shows the old build, do the two-compile swap — compile
   once with the mod off, then enable the row and compile again.
   The builds are only distinguishable by file hash (see the release
   notes for the new sha256).
3. Join a server with another player online. Their head should appear
   on the bar and slide along it as you turn; it disappears past ±60°.
   Heads stay full size at normal distances and only shrink for far
   players (5/9 at 332+ blocks), so do not expect visible shrinking
   up close — that is vanilla's range, not a bug.
4. Open the player list (Tab) once, then check the head again — the
   skin resolves through the atlas once a profile is listed.
5. Options > Mods > Locator Heads 1.0.0: turn the compass on and check
   N/E/S/W track your yaw; switch Compass labels to Coordinates and
   check `+Z`/`-X`/`-Z`/`+X`; turn borders on and check the outline;
   set Names to Always and check the name above the head is tinted
   the border colour and slides up when it first appears.
6. Set Names to Player list: names appear only while the player list
   is held open, and vanish when you let go.
7. `/lh bordercolor FF0000` — the static border (and the name tint
   with Border style Static) should turn red; `/lh bordercolor xyz`
   should print the usage line and change nothing.
8. Gain XP (pick something up / kill a mob) while another player is
   on the bar: the heads vanish for ~5 s while the XP bar shows, then
   come back when the locator bar returns. They must never sit on
   top of the XP bar. (The compass keeps drawing through this.)
9. `/lh filter exclude <their name>` — their head should vanish;
   `/lh filter clear` brings it back.
