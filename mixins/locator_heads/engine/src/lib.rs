//! Locator Heads for Horizon: the heads of the other players on the
//! locator bar, with a compass, team-coloured borders, name modes and
//! include/exclude filters.
//!
//! The game thread calls [`marks`] once per HUD frame (after the
//! contextual bar layer) with the camera and the loaded players, and
//! [`compass_letters`] for the compass overlay. All state lives behind
//! one mutex; the hook adapters in `../` do the game's I/O (settings
//! files, the atlas skin lookup, drawing).
//!
//! An independent Rust reimplementation of the idea of Haage001's
//! Locator Heads (Fabric, LGPL-3.0) — no Java source was copied. It
//! differs in how it gets its players: the original only redecorates
//! the waypoints the server tracks, while this mod also shows loaded
//! player mobs (a Starling server need not send a waypoint for a
//! player standing next to you) and takes waypoint players past
//! loaded range from the server's `TrackedWaypoint` packets, so a
//! head replaces the vanilla dot at every distance.

use std::sync::{LazyLock, Mutex, MutexGuard};

pub const ID: &str = "locator_heads";
pub const NAME: &str = "Locator Heads";
pub const VERSION: &str = "1.0.0";
pub const DESCRIPTION: &str = "Shows the heads of nearby players on the locator bar, with a compass, team-coloured borders and name options. An independent Rust reimplementation of Haage001's Locator Heads idea.";

/// The head-size steps, as the original's 1-9 slider: 5 is 1.0x.
const HEAD_MULTIPLIERS: [f32; 9] = [0.5, 0.6, 0.7, 0.8, 1.0, 1.1, 1.2, 1.3, 1.5];

/// The border thickness choices, in GUI pixels (the original's
/// THIN/MEDIUM/NORMAL/THICK).
const BORDER_THICKNESSES: [f32; 4] = [0.3, 0.5, 1.0, 2.0];
const BORDER_THICKNESS_NAMES: [&str; 4] = ["Thin", "Medium", "Normal", "Thick"];

/// The compass colours the settings page cycles through.
const COMPASS_COLOURS: [(&str, u32); 6] = [
    ("White", 0xFFFFFF),
    ("Yellow", 0xFFFF55),
    ("Aqua", 0x55FFFF),
    ("Green", 0x55FF55),
    ("Red", 0xFF5555),
    ("Light purple", 0xFF55FF),
];

/// Settings keys, one file per key under the mod's data folder.
const KEY_ENABLED: &str = "enabled";
const KEY_HEADS: &str = "heads";
const KEY_HEAD_SIZE: &str = "head_size";
const KEY_COMPASS: &str = "compass";
const KEY_COMPASS_COLOUR: &str = "compass_color";
const KEY_BORDER: &str = "border";
const KEY_BORDER_STYLE: &str = "border_style";
const KEY_BORDER_THICKNESS: &str = "border_thickness";
const KEY_NAMES: &str = "names";
const KEY_FILTER_MODE: &str = "filter_mode";
const KEY_INCLUDE: &str = "include";
const KEY_EXCLUDE: &str = "exclude";
const KEY_COMPASS_COORDS: &str = "compass_coords";
const KEY_COMPASS_SHADOW: &str = "compass_shadow";
const KEY_STATIC_BORDER: &str = "static_border";

/// The preset colours the static-border button cycles through.
const STATIC_COLOURS: [(&str, u32); 6] = COMPASS_COLOURS;

/// Distance scaling for heads, in blocks, matching vanilla's waypoint
/// style defaults (near_distance 128, far_distance 332): full size up
/// to 128 blocks, shrinking linearly to 5/9 at 332 blocks and beyond —
/// the original lerps head size from 9 px at near to 5 px at far. The
/// port's base head size stays 8 px at 1.0x; only the range and the far
/// ratio follow vanilla.
const SCALE_NEAR: f64 = 128.0;
const SCALE_FAR: f64 = 332.0;
const SCALE_FAR_FACTOR: f32 = 5.0 / 9.0;

/// The name slide-in: 20 px up over 150 ms, as the original. At
/// 60 fps that is 9 frames; the engine counts `marks` calls as frames.
const NAME_SLIDE_PX: f32 = 20.0;
const NAME_SLIDE_FRAMES: u64 = 9;

/// A server-tracked waypoint's id, mirroring `horizon_protocol`'s
/// `WaypointId` (the engine is a plain crate and cannot depend on it).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum WaypointId {
    Uuid(u128),
    Name(String),
}

/// What a waypoint points at, mirroring `WaypointTarget`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WaypointTarget {
    Empty,
    Block([i32; 3]),
    Chunk([i32; 2]),
    Azimuth(f32),
}

/// One server-tracked waypoint, as the packet hook recorded it.
#[derive(Clone, Debug, PartialEq)]
pub struct WaypointIn {
    pub id: WaypointId,
    pub style: String,
    pub color: Option<i32>,
    pub target: WaypointTarget,
}

/// `waypoint_operation` values, mirroring the protocol crate.
pub const WAYPOINT_TRACK: i32 = 0;
pub const WAYPOINT_UNTRACK: i32 = 1;
pub const WAYPOINT_UPDATE: i32 = 2;

/// A waypoint player's position, as `View` resolves it: the tracked
/// entity's eyes when it is loaded and within 3 blocks (Manhattan) of
/// the waypoint, else the block's centre; a chunk's middle at the
/// camera's height; an azimuth's direction (placed 400 blocks out, so
/// it reads as far away, as the game's `INFINITY` distance does).
pub fn waypoint_position(target: &WaypointTarget, cam_pos: [f64; 3]) -> [f64; 3] {
    match *target {
        WaypointTarget::Empty => cam_pos,
        WaypointTarget::Block(v) => v.map(|c| f64::from(c) + 0.5),
        WaypointTarget::Chunk(c) => [
            f64::from(c[0] * 16 + 8) + 0.5,
            (cam_pos[1] as i32) as f64 + 0.5,
            f64::from(c[1] * 16 + 8) + 0.5,
        ],
        WaypointTarget::Azimuth(a) => {
            let a = f64::from(a);
            [
                cam_pos[0] + 400.0 * -a.sin(),
                cam_pos[1],
                cam_pos[2] + 400.0 * a.cos(),
            ]
        }
    }
}

/// When to show a player's name above their head.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameMode {
    Off,
    Always,
    LookingAt,
    PlayerList,
}

impl NameMode {
    fn from_stored(v: &str) -> Self {
        match v {
            "always" => Self::Always,
            "looking_at" => Self::LookingAt,
            "player_list" => Self::PlayerList,
            _ => Self::Off,
        }
    }
    fn stored(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Always => "always",
            Self::LookingAt => "looking_at",
            Self::PlayerList => "player_list",
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Always => "Always",
            Self::LookingAt => "Looking at",
            Self::PlayerList => "Player list",
        }
    }
    fn next(&self) -> Self {
        match self {
            Self::Off => Self::Always,
            Self::Always => Self::LookingAt,
            Self::LookingAt => Self::PlayerList,
            Self::PlayerList => Self::Off,
        }
    }
}

/// Which players get a head on the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    All,
    Include,
    Exclude,
}

impl FilterMode {
    fn from_stored(v: &str) -> Self {
        match v {
            "include" => Self::Include,
            "exclude" => Self::Exclude,
            _ => Self::All,
        }
    }
    fn stored(&self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Include => "include",
            Self::Exclude => "exclude",
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Self::All => "All players",
            Self::Include => "Include only",
            Self::Exclude => "Exclude only",
        }
    }
    fn next(&self) -> Self {
        match self {
            Self::All => Self::Include,
            Self::Include => Self::Exclude,
            Self::Exclude => Self::All,
        }
    }
}

/// Whether the border follows the player's team colour or one fixed colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BorderStyle {
    Team,
    Static,
}

impl BorderStyle {
    fn from_stored(v: &str) -> Self {
        match v {
            "static" => Self::Static,
            _ => Self::Team,
        }
    }
    fn stored(&self) -> &'static str {
        match self {
            Self::Team => "team",
            Self::Static => "static",
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Self::Team => "Team colour",
            Self::Static => "Static colour",
        }
    }
}

/// The mod's settings.
#[derive(Clone, Debug)]
pub struct Config {
    pub enabled: bool,
    pub render_heads: bool,
    /// 1-9, as the original's slider; 5 is 1.0x.
    pub head_size: usize,
    pub compass: bool,
    /// Index into [`COMPASS_COLOURS`].
    pub compass_colour: usize,
    /// Coordinates notation for the compass (`+X`/`-X`/`+Z`/`-Z`
    /// instead of N/E/S/W).
    pub compass_coords: bool,
    /// Shadow under the compass letters.
    pub compass_shadow: bool,
    pub border: bool,
    pub border_style: BorderStyle,
    /// Index into [`BORDER_THICKNESSES`].
    pub border_thickness: usize,
    /// The static border colour as `0xRRGGBB`.
    pub static_border: u32,
    pub names: NameMode,
    pub filter_mode: FilterMode,
    /// Player names, as typed (comma/semicolon/colon/period separated).
    pub include: String,
    pub exclude: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            render_heads: true,
            head_size: 5,
            compass: false,
            compass_colour: 0,
            compass_coords: false,
            compass_shadow: true,
            border: false,
            border_style: BorderStyle::Team,
            border_thickness: 2,
            static_border: 0xFFFFFF,
            names: NameMode::Off,
            filter_mode: FilterMode::All,
            include: String::new(),
            exclude: String::new(),
        }
    }
}

impl Config {
    fn load(get: &dyn Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        let bool = |k: &str, default: bool| match get(k).as_deref() {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        };
        let num = |k: &str, default: usize| {
            get(k).and_then(|v| v.parse::<usize>().ok()).unwrap_or(default)
        };
        let text = |k: &str| get(k).unwrap_or_default();
        let hex = |k: &str, default: u32| {
            get(k)
                .and_then(|v| parse_hex_color(&v))
                .unwrap_or(default)
        };
        Self {
            enabled: bool(KEY_ENABLED, d.enabled),
            render_heads: bool(KEY_HEADS, d.render_heads),
            head_size: num(KEY_HEAD_SIZE, d.head_size).clamp(1, 9),
            compass: bool(KEY_COMPASS, d.compass),
            compass_colour: num(KEY_COMPASS_COLOUR, d.compass_colour) % COMPASS_COLOURS.len(),
            compass_coords: bool(KEY_COMPASS_COORDS, d.compass_coords),
            compass_shadow: bool(KEY_COMPASS_SHADOW, d.compass_shadow),
            border: bool(KEY_BORDER, d.border),
            border_style: get(KEY_BORDER_STYLE)
                .map_or(d.border_style, |v| BorderStyle::from_stored(&v)),
            border_thickness: num(KEY_BORDER_THICKNESS, d.border_thickness) % BORDER_THICKNESSES.len(),
            static_border: hex(KEY_STATIC_BORDER, d.static_border),
            names: get(KEY_NAMES).map_or(d.names, |v| NameMode::from_stored(&v)),
            filter_mode: get(KEY_FILTER_MODE)
                .map_or(d.filter_mode, |v| FilterMode::from_stored(&v)),
            include: text(KEY_INCLUDE),
            exclude: text(KEY_EXCLUDE),
        }
    }

    fn stored(&self) -> Vec<(&'static str, String)> {
        vec![
            (KEY_ENABLED, self.enabled.to_string()),
            (KEY_HEADS, self.render_heads.to_string()),
            (KEY_HEAD_SIZE, self.head_size.to_string()),
            (KEY_COMPASS, self.compass.to_string()),
            (KEY_COMPASS_COLOUR, self.compass_colour.to_string()),
            (KEY_COMPASS_COORDS, self.compass_coords.to_string()),
            (KEY_COMPASS_SHADOW, self.compass_shadow.to_string()),
            (KEY_BORDER, self.border.to_string()),
            (KEY_BORDER_STYLE, self.border_style.stored().to_string()),
            (KEY_BORDER_THICKNESS, self.border_thickness.to_string()),
            (KEY_STATIC_BORDER, format!("{:06X}", self.static_border)),
            (KEY_NAMES, self.names.stored().to_string()),
            (KEY_FILTER_MODE, self.filter_mode.stored().to_string()),
            (KEY_INCLUDE, self.include.clone()),
            (KEY_EXCLUDE, self.exclude.clone()),
        ]
    }

    /// The head's side in GUI pixels at the current size step.
    pub fn head_size_px(&self) -> f32 {
        8.0 * HEAD_MULTIPLIERS[self.head_size - 1]
    }

    /// The border's thickness in GUI pixels, at least one whole pixel
    /// once it is on (the original's THIN is 0.3, which no fill can draw).
    pub fn border_px(&self) -> f32 {
        BORDER_THICKNESSES[self.border_thickness].max(1.0)
    }

    /// The compass colour as `0xRRGGBB`.
    pub fn compass_rgb(&self) -> u32 {
        COMPASS_COLOURS[self.compass_colour].1
    }

    /// The static border colour's preset name, when it matches one.
    pub fn static_preset_name(&self) -> &'static str {
        STATIC_COLOURS
            .iter()
            .find(|(_, rgb)| *rgb == self.static_border)
            .map_or("Custom", |(name, _)| *name)
    }

    /// Move the static border colour to the next preset.
    fn cycle_static_preset(&mut self) {
        let cur = STATIC_COLOURS
            .iter()
            .position(|(_, rgb)| *rgb == self.static_border)
            .unwrap_or(usize::MAX);
        let next = if cur == usize::MAX {
            0
        } else {
            (cur + 1) % STATIC_COLOURS.len()
        };
        self.static_border = STATIC_COLOURS[next].1;
    }

    fn buttons(&self) -> Vec<String> {
        vec![
            format!("Locator Heads: {}", on_off(self.enabled)),
            format!("Heads: {}", on_off(self.render_heads)),
            format!(
                "Head size: {:.1}x",
                HEAD_MULTIPLIERS[self.head_size - 1]
            ),
            format!("Compass: {}", on_off(self.compass)),
            format!(
                "Compass colour: {}",
                COMPASS_COLOURS[self.compass_colour].0
            ),
            format!("Border: {}", on_off(self.border)),
            format!("Border style: {}", self.border_style.label()),
            format!(
                "Border thickness: {}",
                BORDER_THICKNESS_NAMES[self.border_thickness]
            ),
            format!("Names: {}", self.names.label()),
            format!("Filter: {}", self.filter_mode.label()),
            format!(
                "Compass labels: {}",
                if self.compass_coords { "Coordinates" } else { "Cardinal" }
            ),
            format!("Compass shadow: {}", on_off(self.compass_shadow)),
            format!(
                "Static border colour: {}",
                self.static_preset_name()
            ),
        ]
    }
}

fn on_off(b: bool) -> &'static str {
    if b { "On" } else { "Off" }
}

#[derive(Default)]
struct State {
    config: Config,
    loaded: bool,
    store: Vec<(String, String)>,
    /// `marks` calls so far; the name slide-in counts frames from it.
    frame: u64,
    /// When each currently shown name first appeared, in frames.
    name_first_seen: std::collections::HashMap<String, u64>,
    /// Server-tracked waypoints, in TRACK order, mirroring the game's
    /// own table (the packet hook keeps it current).
    waypoints: Vec<WaypointIn>,
    /// Player-list names by profile UUID (from `PlayerInfoUpdate`;
    /// `PlayerInfoRemove` takes them out again).
    player_names: std::collections::HashMap<u128, String>,
}

impl State {
    fn queue_config(&mut self) {
        for (k, v) in self.config.stored() {
            self.store.push((k.to_string(), v));
        }
    }
}

/// Parse a hex colour: `RRGGBB`, with an optional `#` or `0x` prefix.
pub fn parse_hex_color(raw: &str) -> Option<u32> {
    let s = raw.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

/// The head-size factor for a distance in blocks: 1.0 up to
/// [`SCALE_NEAR`] blocks, [`SCALE_FAR_FACTOR`] at or past
/// [`SCALE_FAR`] blocks, linear between.
pub fn distance_scale(distance: f64) -> f32 {
    if distance <= SCALE_NEAR {
        1.0
    } else if distance >= SCALE_FAR {
        SCALE_FAR_FACTOR
    } else {
        (1.0 - ((distance - SCALE_NEAR) / (SCALE_FAR - SCALE_NEAR)) as f32
            * (1.0 - SCALE_FAR_FACTOR))
            .clamp(SCALE_FAR_FACTOR, 1.0)
    }
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::default()));

/// Test-only: forget the name slide-in state, so slide tests start clean.
#[cfg(test)]
pub fn reset_name_anim() {
    let mut s = state();
    s.frame = 0;
    s.name_first_seen.clear();
}

/// Test-only: forget the recorded waypoints and player names.
#[cfg(test)]
pub fn reset_waypoints() {
    let mut s = state();
    s.waypoints.clear();
    s.player_names.clear();
}

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Record a `TrackedWaypoint` packet, mirroring
/// `ClientWaypoints::apply`: TRACK inserts or replaces by id,
/// UNTRACK removes, any other operation only moves a waypoint of the
/// same kind.
pub fn note_waypoint(
    operation: i32,
    id: WaypointId,
    style: String,
    color: Option<i32>,
    target: WaypointTarget,
) {
    let mut s = state();
    let at = s.waypoints.iter().position(|w| w.id == id);
    match operation {
        WAYPOINT_TRACK => match at {
            Some(i) => {
                s.waypoints[i] = WaypointIn {
                    id,
                    style,
                    color,
                    target,
                }
            }
            None => s.waypoints.push(WaypointIn {
                id,
                style,
                color,
                target,
            }),
        },
        WAYPOINT_UNTRACK => {
            if let Some(i) = at {
                s.waypoints.remove(i);
            }
        }
        _ => {
            if let Some(w) = at.map(|i| &mut s.waypoints[i]) {
                if matches!(
                    (w.target, target),
                    (WaypointTarget::Block(_), WaypointTarget::Block(_))
                        | (WaypointTarget::Chunk(_), WaypointTarget::Chunk(_))
                        | (WaypointTarget::Azimuth(_), WaypointTarget::Azimuth(_))
                ) {
                    w.target = target;
                }
            }
        }
    }
}

/// The recorded waypoints, in TRACK order.
pub fn waypoints() -> Vec<WaypointIn> {
    state().waypoints.clone()
}

/// A player-list name by profile UUID, from `PlayerInfoUpdate`.
pub fn note_player_name(uuid: u128, name: String) {
    state().player_names.insert(uuid, name);
}

/// Forget a player-list name (`PlayerInfoRemove`).
pub fn forget_player(uuid: u128) {
    state().player_names.remove(&uuid);
}

/// A player-list name by profile UUID, if one was ever listed.
pub fn player_name(uuid: u128) -> Option<String> {
    state().player_names.get(&uuid).cloned()
}

/// Forget the recorded waypoints: joining a new world, where the game
/// clears its own table.
pub fn clear_waypoints() {
    state().waypoints.clear();
}

/// Forget the recorded player-list names: joining a new world.
pub fn clear_player_names() {
    state().player_names.clear();
}

/// Read the settings from the game's files.
pub fn load(get: &dyn Fn(&str) -> Option<String>) {
    let mut s = state();
    s.config = Config::load(get);
    s.loaded = true;
}

/// The settings, read from the game's files on first use and cached
/// after that; the settings page and commands keep the cache current.
pub fn config(get: &dyn Fn(&str) -> Option<String>) -> Config {
    let mut s = state();
    if !s.loaded {
        s.config = Config::load(get);
        s.loaded = true;
    }
    s.config.clone()
}

/// Settings writes the adapter still has to persist.
pub fn take_store() -> Vec<(String, String)> {
    std::mem::take(&mut state().store)
}

/// The settings page's buttons, in order.
pub fn settings_buttons() -> Vec<String> {
    state().config.buttons()
}

/// A click on the settings page: the button's setting moves to its next value.
pub fn settings_click(button: usize) {
    let mut s = state();
    match button {
        0 => s.config.enabled = !s.config.enabled,
        1 => s.config.render_heads = !s.config.render_heads,
        2 => s.config.head_size = s.config.head_size % 9 + 1,
        3 => s.config.compass = !s.config.compass,
        4 => s.config.compass_colour = (s.config.compass_colour + 1) % COMPASS_COLOURS.len(),
        5 => s.config.border = !s.config.border,
        6 => {
            s.config.border_style = match s.config.border_style {
                BorderStyle::Team => BorderStyle::Static,
                BorderStyle::Static => BorderStyle::Team,
            };
        }
        7 => s.config.border_thickness = (s.config.border_thickness + 1) % BORDER_THICKNESSES.len(),
        8 => s.config.names = s.config.names.next(),
        9 => s.config.filter_mode = s.config.filter_mode.next(),
        10 => s.config.compass_coords = !s.config.compass_coords,
        11 => s.config.compass_shadow = !s.config.compass_shadow,
        12 => s.config.cycle_static_preset(),
        _ => return,
    }
    s.queue_config();
}

// --- Geometry: Horizon's own locator math, as in
// `crates/horizon-client/src/locator_bar.rs` at ac874422ed. ---

/// `Mth.wrapDegrees`.
pub fn wrap_degrees(angle: f32) -> f32 {
    let mut a = angle % 360.0;
    if a >= 180.0 {
        a -= 360.0;
    }
    if a < -180.0 {
        a += 360.0;
    }
    a
}

/// `View::yaw_to`: the bearing of `target` from the camera, in degrees,
/// clockwise from where the camera looks.
pub fn yaw_to(cam: [f64; 3], yaw: f32, target: [f64; 3]) -> f64 {
    // Vec3.rotateClockwise90: (-z, y, x).
    let (x, z) = (-(cam[2] - target[2]), cam[0] - target[0]);
    let angle = (z.atan2(x) as f32) * (180.0 / std::f32::consts::PI);
    f64::from(wrap_degrees(angle - yaw))
}

/// The bar slot a bearing falls in: the left edge of the 9-pixel dot,
/// or `None` outside Horizon's ±60° window (vanilla skips `angle <= -60`
/// and `angle > 60`).
pub fn bar_x(gui_width: i32, angle: f64) -> Option<i32> {
    if angle <= -60.0 || angle > 60.0 {
        return None;
    }
    let middle = ((gui_width - 9) as f32 / 2.0).ceil() as i32;
    Some(middle + (angle * 173.0 / 2.0 / 60.0).floor() as i32)
}

/// Whether the view direction looks at a target: the original's
/// LOOKING_AT, a 20° cone.
pub fn looking_at(view: [f64; 3], to: [f64; 3]) -> bool {
    let dot = |a: [f64; 3], b: [f64; 3]| (0..3).map(|i| a[i] * b[i]).sum::<f64>();
    let norm = |a: [f64; 3]| {
        let l = dot(a, a).sqrt();
        if l == 0.0 { [0.0; 3] } else { a.map(|v| v / l) }
    };
    // cos(20°).
    dot(norm(view), norm(to)) >= 0.939_692_6
}

/// A filter list, split on comma, semicolon, colon or period like the
/// original, lowercased for matching.
pub fn parse_list(raw: &str) -> Vec<String> {
    raw.split([',', ';', ':', '.'])
        .map(|n| n.trim().to_ascii_lowercase())
        .filter(|n| !n.is_empty())
        .collect()
}

/// Whether a player passes the filter.
pub fn filter_pass(mode: FilterMode, name: &str, include: &[String], exclude: &[String]) -> bool {
    let name = name.to_ascii_lowercase();
    match mode {
        FilterMode::All => true,
        FilterMode::Include => include.iter().any(|n| *n == name),
        FilterMode::Exclude => !exclude.iter().any(|n| *n == name),
    }
}

/// A compass letter's screen position: `(letter, x)`, as the original
/// draws it — the direction's angle against the yaw, within a 120°
/// field of view, spread over the 182-pixel bar's half width (91).
/// S is 0°, W 90°, N 180°, E 270°, as Minecraft yaw counts them.
/// With `coordinates`, the letters are `+Z`/`-X`/`-Z`/`+X` instead.
pub fn compass_letters(yaw: f32, gui_width: i32, coordinates: bool) -> Vec<(&'static str, i32)> {
    let centre = gui_width / 2;
    let letters: [(&'static str, f32); 4] = if coordinates {
        [("+Z", 0.0), ("-X", 90.0), ("-Z", 180.0), ("+X", 270.0)]
    } else {
        [("S", 0.0), ("W", 90.0), ("N", 180.0), ("E", 270.0)]
    };
    letters
        .into_iter()
        .filter_map(|(letter, angle)| {
            let normalized = ((yaw % 360.0) + 360.0) % 360.0;
            let diff = wrap_degrees(angle - normalized);
            if diff.abs() > 60.0 {
                return None;
            }
            Some((letter, centre + (diff / 60.0 * 91.0) as i32))
        })
        .collect()
}

// --- Per-frame layout ---

/// One other player, as the HUD adapter hands it in.
pub struct PlayerIn {
    pub name: String,
    pub uuid: Option<u128>,
    /// Eye position, in world coordinates.
    pub pos: [f64; 3],
    /// The team colour as `0xRRGGBB`, when the player has a team with one.
    pub team_rgb: Option<u32>,
}

/// What a frame needs to lay the bar out.
pub struct FrameInput<'a> {
    /// The camera position (the locator bar's own camera).
    pub cam_pos: [f64; 3],
    /// The camera yaw, in degrees.
    pub yaw: f32,
    /// The view direction, for LOOKING_AT names.
    pub view: [f64; 3],
    pub gui_width: i32,
    pub gui_height: i32,
    /// Whether the player list is open (Tab held while playing),
    /// for PLAYER_LIST names.
    pub list_open: bool,
    /// The loaded players, with their eye positions and team colours.
    pub players: &'a [PlayerIn],
    /// Waypoint players past loaded range, already resolved to eye
    /// positions (block centre, chunk middle, or azimuth ray) with
    /// their names and team colours; a loaded player wins on UUID.
    pub waypoint_players: &'a [PlayerIn],
}

/// One head (or fallback dot) to draw on the bar.
pub struct PlayerMark {
    pub name: String,
    pub uuid: Option<u128>,
    /// Left edge of the 9-pixel vanilla dot slot.
    pub x: i32,
    /// Top of the bar row: `gui_height - 32 + 3 - 2`, as vanilla draws it.
    pub top: i32,
    pub head_size: f32,
    /// The border colour as `0xAARRGGBB`, when borders are on.
    pub border_argb: Option<u32>,
    pub show_name: bool,
    /// The name's colour as `0xRRGGBB`: the border colour (team, else
    /// static), as the original tints names with the border style.
    pub name_rgb: u32,
    /// The name's slide-in offset in GUI pixels: 20 when it first
    /// appears, easing to 0 over [`NAME_SLIDE_FRAMES`] frames. No
    /// alpha fade — the HUD text helper takes opaque colours only.
    pub name_dy: f32,
    pub distance: f64,
}

/// The marks for this frame, farthest first so nearer heads draw over
/// farther ones, as vanilla sorts its waypoints.
pub fn marks(cfg: &Config, input: &FrameInput) -> Vec<PlayerMark> {
    if !cfg.enabled {
        return Vec::new();
    }
    let include = parse_list(&cfg.include);
    let exclude = parse_list(&cfg.exclude);
    let top = input.gui_height - 32 + 3 - 2;
    // The name slide-in counts frames from the shared state.
    let frame = {
        let mut s = state();
        s.frame += 1;
        s.frame
    };
    let mut out: Vec<PlayerMark> = Vec::new();
    // Waypoint players past loaded range join the loaded ones; a
    // loaded player wins on UUID, so nobody draws twice.
    let mut merged: Vec<&PlayerIn> = input.players.iter().collect();
    for w in input.waypoint_players {
        let dup = w
            .uuid
            .is_some_and(|u| input.players.iter().any(|p| p.uuid == Some(u)));
        if !dup {
            merged.push(w);
        }
    }
    for p in merged {
        // A head with no name (its player list entry never arrived)
        // cannot be matched by a filter list, so it always passes.
        if !p.name.is_empty() && !filter_pass(cfg.filter_mode, &p.name, &include, &exclude) {
            continue;
        }
        let angle = yaw_to(input.cam_pos, input.yaw, p.pos);
        let Some(x) = bar_x(input.gui_width, angle) else {
            continue;
        };
        let to = [
            p.pos[0] - input.cam_pos[0],
            p.pos[1] - input.cam_pos[1],
            p.pos[2] - input.cam_pos[2],
        ];
        let distance = (0..3).map(|i| to[i] * to[i]).sum::<f64>().sqrt();
        let border_rgb = match cfg.border_style {
            BorderStyle::Team => p.team_rgb.unwrap_or(0xFFFFFF),
            BorderStyle::Static => cfg.static_border,
        };
        let border_argb = cfg
            .border
            .then(|| 0xFF00_0000 | border_rgb);
        let show_name = match cfg.names {
            NameMode::Off => false,
            NameMode::Always => true,
            NameMode::LookingAt => looking_at(input.view, to),
            NameMode::PlayerList => input.list_open,
        };
        // Slide-in: a name starts 20 px low and rises over
        // NAME_SLIDE_FRAMES frames from when it first appears.
        let name_dy = if show_name {
            let first = {
                let mut s = state();
                *s.name_first_seen.entry(p.name.clone()).or_insert(frame)
            };
            let progress = ((frame - first) as f32 / NAME_SLIDE_FRAMES as f32).clamp(0.0, 1.0);
            NAME_SLIDE_PX * (1.0 - progress)
        } else {
            0.0
        };
        out.push(PlayerMark {
            name: p.name.clone(),
            uuid: p.uuid,
            x,
            top,
            head_size: cfg.head_size_px() * distance_scale(distance),
            border_argb,
            show_name,
            name_rgb: border_rgb,
            name_dy,
            distance,
        });
    }
    // Forget names that stopped showing, so they slide in again when
    // they reappear.
    {
        let mut s = state();
        s.name_first_seen
            .retain(|name, _| out.iter().any(|m| m.show_name && &m.name == name));
    }
    out.sort_by(|a, b| b.distance.total_cmp(&a.distance));
    out
}

// --- Commands ---

/// What a command prints in the chat.
pub struct CommandOut {
    pub lines: Vec<String>,
}

fn status_lines(cfg: &Config) -> Vec<String> {
    vec![
        format!(
            "Locator Heads: {} · heads {} · compass {} ({}, shadow {}) · names {} · filter {}",
            on_off(cfg.enabled),
            on_off(cfg.render_heads),
            on_off(cfg.compass),
            if cfg.compass_coords { "coordinates" } else { "cardinal" },
            on_off(cfg.compass_shadow),
            cfg.names.label(),
            cfg.filter_mode.label(),
        ),
        format!(
            "Border: {} · {} · static #{:06X} (set with /lh bordercolor <hex>)",
            on_off(cfg.border),
            cfg.border_style.label(),
            cfg.static_border,
        ),
        format!(
            "Include: {} · Exclude: {} (set with /lh filter include <names> or /lh filter exclude <names>; clear with /lh filter clear)",
            if cfg.include.is_empty() { "—" } else { &cfg.include },
            if cfg.exclude.is_empty() { "—" } else { &cfg.exclude },
        ),
    ]
}

/// `/locatorheads` or `/lh`, without the slash. `None` when the line is
/// not this mod's command.
pub fn command(line: &str) -> Option<CommandOut> {
    let mut words = line.split_whitespace();
    match words.next()? {
        "locatorheads" | "lh" => {}
        _ => return None,
    }
    let mut s = state();
    let rest: Vec<&str> = words.collect();
    match rest.first().copied() {
        None => Some(CommandOut { lines: status_lines(&s.config) }),
        Some("toggle") => {
            s.config.enabled = !s.config.enabled;
            let lines = vec![format!(
                "Locator Heads: {}",
                on_off(s.config.enabled)
            )];
            s.queue_config();
            Some(CommandOut { lines })
        }
        Some("bordercolor") => {
            let raw = rest.get(1).copied().unwrap_or("");
            match parse_hex_color(raw) {
                Some(rgb) => {
                    s.config.static_border = rgb;
                    s.queue_config();
                    Some(CommandOut {
                        lines: vec![format!(
                            "Locator Heads: static border colour set to #{:06X}.",
                            rgb
                        )],
                    })
                }
                None => Some(CommandOut {
                    lines: vec![
                        "Usage: /lh bordercolor <hex> — e.g. /lh bordercolor FF0000 or /lh bordercolor #00FF00."
                            .to_string(),
                    ],
                }),
            }
        }
        Some("filter") => match rest.get(1).copied() {
            Some("clear") => {
                s.config.include.clear();
                s.config.exclude.clear();
                s.queue_config();
                Some(CommandOut {
                    lines: vec!["Locator Heads: filter lists cleared.".to_string()],
                })
            }
            Some("include" | "exclude") => {
                let list = rest[2..].join(" ");
                if list.trim().is_empty() {
                    return Some(CommandOut {
                        lines: vec![
                            "Usage: /lh filter include <name, name, ...>".to_string(),
                        ],
                    });
                }
                let lines = vec![format!(
                    "Locator Heads: {} list set to {}.",
                    rest[1], list
                )];
                if rest[1] == "include" {
                    s.config.include = list;
                } else {
                    s.config.exclude = list;
                }
                s.queue_config();
                Some(CommandOut { lines })
            }
            _ => Some(CommandOut {
                lines: vec![
                    "Usage: /lh filter include <names> · /lh filter exclude <names> · /lh filter clear"
                        .to_string(),
                ],
            }),
        },
        _ => Some(CommandOut {
            lines: vec![
                "Usage: /lh · /lh toggle · /lh bordercolor <hex> · /lh filter include <names> · /lh filter exclude <names> · /lh filter clear"
                    .to_string(),
            ],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The command and settings-click tests share the global state, so
    /// they take this lock and run one at a time.
    static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn cfg() -> Config {
        Config::default()
    }

    fn frame(players: &[PlayerIn]) -> FrameInput<'_> {
        FrameInput {
            cam_pos: [0.0, 64.0, 0.0],
            yaw: 0.0,
            view: [0.0, 0.0, 1.0],
            gui_width: 320,
            gui_height: 240,
            list_open: false,
            players,
            waypoint_players: &[],
        }
    }

    fn player(name: &str, pos: [f64; 3]) -> PlayerIn {
        PlayerIn {
            name: name.to_string(),
            uuid: Some(1),
            pos,
            team_rgb: None,
        }
    }

    #[test]
    fn wrap_degrees_wraps() {
        assert_eq!(wrap_degrees(190.0), -170.0);
        assert_eq!(wrap_degrees(-190.0), 170.0);
        assert_eq!(wrap_degrees(360.0), 0.0);
        assert_eq!(wrap_degrees(45.0), 45.0);
    }

    #[test]
    fn yaw_to_matches_compass_directions() {
        // Facing south (yaw 0): south is ahead, east is left, north behind.
        let cam = [0.0, 64.0, 0.0];
        assert!((yaw_to(cam, 0.0, [0.0, 64.0, 10.0]) - 0.0).abs() < 0.001);
        assert!((yaw_to(cam, 0.0, [10.0, 64.0, 0.0]) - -90.0).abs() < 0.001);
        assert!((yaw_to(cam, 0.0, [-10.0, 64.0, 0.0]) - 90.0).abs() < 0.001);
        assert!((yaw_to(cam, 0.0, [0.0, 64.0, -10.0]).abs() - 180.0).abs() < 0.001);
        // Facing west (yaw 90): west is ahead.
        assert!((yaw_to(cam, 90.0, [-10.0, 64.0, 0.0]) - 0.0).abs() < 0.001);
    }

    #[test]
    fn bar_x_window_is_vanillas() {
        // gui_width 320: middle is ceil((320-9)/2) = 156.
        assert_eq!(bar_x(320, 0.0), Some(156));
        assert_eq!(bar_x(320, 60.0), Some(156 + 86));
        assert_eq!(bar_x(320, -60.0), None);
        assert_eq!(bar_x(320, 60.5), None);
        assert_eq!(bar_x(320, -59.0), Some(156 - 86));
    }

    #[test]
    fn marks_keep_only_the_window_and_the_filter() {
        let mut c = cfg();
        c.filter_mode = FilterMode::Exclude;
        c.exclude = "bob".to_string();
        let players = [
            player("Alice", [0.0, 64.0, 10.0]),
            player("Bob", [0.0, 64.0, 12.0]),
            player("Carol", [0.0, 64.0, -10.0]), // behind: out of window
        ];
        let got = marks(&c, &frame(&players));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "Alice");
        assert_eq!(got[0].x, 156);
        assert_eq!(got[0].top, 240 - 32 + 3 - 2);
    }

    #[test]
    fn marks_sort_farthest_first() {
        let c = cfg();
        let players = [
            player("Near", [0.0, 64.0, 5.0]),
            player("Far", [0.0, 64.0, 20.0]),
        ];
        let got = marks(&c, &frame(&players));
        assert_eq!(got[0].name, "Far");
        assert_eq!(got[1].name, "Near");
    }

    #[test]
    fn looking_at_is_a_20_degree_cone() {
        assert!(looking_at([0.0, 0.0, 1.0], [0.0, 0.0, 5.0]));
        assert!(!looking_at([0.0, 0.0, 1.0], [5.0, 0.0, 0.0]));
        // Exactly 20° off: still looking at them.
        let a = 20.0f64.to_radians();
        assert!(looking_at([0.0, 0.0, 1.0], [a.sin(), 0.0, a.cos()]));
        assert!(!looking_at([0.0, 0.0, 1.0], [25.0f64.to_radians().sin(), 0.0, 25.0f64.to_radians().cos()]));
    }

    #[test]
    fn filter_lists_split_on_four_separators() {
        assert_eq!(
            parse_list("Alice, bob;Carol:dan.eve "),
            ["alice", "bob", "carol", "dan", "eve"]
        );
        assert!(filter_pass(FilterMode::Include, "Bob", &parse_list("alice, bob"), &[]));
        assert!(!filter_pass(FilterMode::Include, "Carol", &parse_list("alice, bob"), &[]));
        assert!(!filter_pass(FilterMode::Exclude, "Bob", &[], &parse_list("bob")));
        assert!(filter_pass(FilterMode::All, "Anyone", &[], &[]));
    }

    #[test]
    fn compass_letters_follow_yaw() {
        // Facing south: only S shows, dead centre.
        assert_eq!(compass_letters(0.0, 320, false), vec![("S", 160)]);
        // Facing west: only W shows, dead centre.
        assert_eq!(compass_letters(90.0, 320, false), vec![("W", 160)]);
        // Facing north: only N shows.
        assert_eq!(compass_letters(180.0, 320, false), vec![("N", 160)]);
        // Facing east (yaw 270): only E shows.
        assert_eq!(compass_letters(270.0, 320, false), vec![("E", 160)]);
        // Halfway south-west (yaw 45): S and W both show, S left of centre.
        let got = compass_letters(45.0, 320, false);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], ("S", 160 - 68));
        assert_eq!(got[1], ("W", 160 + 68));
    }

    #[test]
    fn compass_coordinates_notation() {
        // Same geometry, coordinate labels: +Z is south, -X is west.
        assert_eq!(compass_letters(0.0, 320, true), vec![("+Z", 160)]);
        assert_eq!(compass_letters(90.0, 320, true), vec![("-X", 160)]);
        assert_eq!(compass_letters(180.0, 320, true), vec![("-Z", 160)]);
        assert_eq!(compass_letters(270.0, 320, true), vec![("+X", 160)]);
        let got = compass_letters(45.0, 320, true);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], ("+Z", 160 - 68));
        assert_eq!(got[1], ("-X", 160 + 68));
    }

    #[test]
    fn distance_scale_shrinks_heads() {
        // Vanilla's waypoint style defaults: full size up to 128
        // blocks, 5/9 at 332 blocks and beyond, linear between.
        assert_eq!(distance_scale(0.0), 1.0);
        assert_eq!(distance_scale(64.0), 1.0);
        assert_eq!(distance_scale(100.0), 1.0);
        assert_eq!(distance_scale(128.0), 1.0);
        assert!((distance_scale(230.0) - 0.7778).abs() < 0.001);
        assert_eq!(distance_scale(332.0), 5.0 / 9.0);
        assert_eq!(distance_scale(400.0), 5.0 / 9.0);
        // Through marks: a player at 100 blocks is still full size;
        // at 230 blocks ~0.78x; at 400 blocks 5/9.
        let c = cfg();
        let near = marks(&c, &frame(&[player("Near", [0.0, 64.0, 5.0])]));
        assert_eq!(near[0].head_size, 8.0);
        let mid = marks(&c, &frame(&[player("Mid", [0.0, 64.0, 100.0])]));
        assert_eq!(mid[0].head_size, 8.0);
        let far = marks(&c, &frame(&[player("Far", [0.0, 64.0, 230.0])]));
        assert!((far[0].head_size - 6.2222).abs() < 0.01);
        let very_far = marks(&c, &frame(&[player("VeryFar", [0.0, 64.0, 400.0])]));
        assert!((very_far[0].head_size - 4.4444).abs() < 0.01);
    }

    #[test]
    fn hex_colours_parse_and_reject() {
        assert_eq!(parse_hex_color("FF0000"), Some(0xFF0000));
        assert_eq!(parse_hex_color("#00ff00"), Some(0x00FF00));
        assert_eq!(parse_hex_color("0x0000FF"), Some(0x0000FF));
        assert_eq!(parse_hex_color("0Xffffff"), Some(0xFFFFFF));
        assert_eq!(parse_hex_color("xyz"), None);
        assert_eq!(parse_hex_color("12345"), None);
        assert_eq!(parse_hex_color("#1234567"), None);
        assert_eq!(parse_hex_color(""), None);
    }

    #[test]
    fn bordercolor_command_sets_and_rejects() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        load(&|_| None);
        let out = command("lh bordercolor #FF0000").unwrap();
        assert!(out.lines[0].contains("#FF0000"));
        assert_eq!(state().config.static_border, 0xFF0000);
        let out = command("lh bordercolor notacolor").unwrap();
        assert!(out.lines[0].contains("Usage: /lh bordercolor"));
        assert_eq!(state().config.static_border, 0xFF0000);
        // The new keys round-trip through the stored files.
        let stored: Vec<(String, String)> = state()
            .config
            .stored()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let back = Config::load(&|k| {
            stored
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        });
        assert_eq!(back.static_border, 0xFF0000);
        assert!(back.compass_shadow);
        assert!(!back.compass_coords);
    }

    #[test]
    fn name_modes_include_player_list() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut c = cfg();
        c.names = NameMode::PlayerList;
        let players = [player("Alice", [0.0, 64.0, 10.0])];
        let mut f = frame(&players);
        f.list_open = false;
        assert!(!marks(&c, &f)[0].show_name);
        f.list_open = true;
        assert!(marks(&c, &f)[0].show_name);
        // The settings button cycles through all four modes.
        load(&|_| None);
        settings_click(8);
        assert_eq!(state().config.names, NameMode::Always);
        settings_click(8);
        assert_eq!(state().config.names, NameMode::LookingAt);
        settings_click(8);
        assert_eq!(state().config.names, NameMode::PlayerList);
        settings_click(8);
        assert_eq!(state().config.names, NameMode::Off);
    }

    #[test]
    fn name_tint_follows_border_style() {
        let mut c = cfg();
        c.names = NameMode::Always;
        c.border_style = BorderStyle::Team;
        c.static_border = 0x123456;
        let mut team = player("Alice", [0.0, 64.0, 10.0]);
        team.team_rgb = Some(0xFFAA00);
        assert_eq!(marks(&c, &frame(&[team]))[0].name_rgb, 0xFFAA00);
        c.border_style = BorderStyle::Static;
        let plain = player("Alice", [0.0, 64.0, 10.0]);
        assert_eq!(marks(&c, &frame(&[plain]))[0].name_rgb, 0x123456);
        // No team: team style falls back to white.
        c.border_style = BorderStyle::Team;
        let plain = player("Alice", [0.0, 64.0, 10.0]);
        assert_eq!(marks(&c, &frame(&[plain]))[0].name_rgb, 0xFFFFFF);
    }

    #[test]
    fn name_slides_in_over_nine_frames() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_name_anim();
        let mut c = cfg();
        c.names = NameMode::Always;
        let players = [player("Alice", [0.0, 64.0, 10.0])];
        let f = frame(&players);
        let first = marks(&c, &f);
        assert_eq!(first[0].name_dy, 20.0);
        for _ in 0..8 {
            marks(&c, &f);
        }
        let settled = marks(&c, &f);
        assert_eq!(settled[0].name_dy, 0.0);
        // Hidden, then shown again: slides in again from the top.
        c.names = NameMode::Off;
        marks(&c, &f);
        c.names = NameMode::Always;
        assert_eq!(marks(&c, &f)[0].name_dy, 20.0);
    }

    #[test]
    fn head_size_steps_match_the_original_slider() {
        let mut c = cfg();
        c.head_size = 1;
        assert_eq!(c.head_size_px(), 4.0);
        c.head_size = 5;
        assert_eq!(c.head_size_px(), 8.0);
        c.head_size = 9;
        assert_eq!(c.head_size_px(), 12.0);
        c.border_thickness = 0;
        assert_eq!(c.border_px(), 1.0); // THIN is 0.3: at least a whole pixel
        c.border_thickness = 3;
        assert_eq!(c.border_px(), 2.0);
    }

    #[test]
    fn settings_round_trip_through_files() {
        let mut c = cfg();
        c.names = NameMode::LookingAt;
        c.filter_mode = FilterMode::Include;
        c.include = "alice, bob".to_string();
        c.head_size = 9;
        c.compass = true;
        c.compass_colour = 2;
        let stored: Vec<(String, String)> = c
            .stored()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let back = Config::load(&|k| {
            stored
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        });
        assert_eq!(back.names, NameMode::LookingAt);
        assert_eq!(back.filter_mode, FilterMode::Include);
        assert_eq!(back.include, "alice, bob");
        assert_eq!(back.head_size, 9);
        assert!(back.compass);
        assert_eq!(back.compass_rgb(), 0x55FFFF);
    }

    #[test]
    fn command_sets_filter_lists() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        load(&|_| None);
        let out = command("lh filter include Alice, Bob").unwrap();
        assert!(out.lines[0].contains("include list set"));
        let out = command("lh filter exclude Carol").unwrap();
        assert!(out.lines[0].contains("exclude list set"));
        let out = command("lh").unwrap();
        assert!(out.lines[2].contains("Alice, Bob"));
        assert!(out.lines[2].contains("Carol"));
        let out = command("lh filter clear").unwrap();
        assert!(out.lines[0].contains("cleared"));
        assert!(command("say hello").is_none());
        assert!(command("locatorheads toggle").is_some());
    }

    #[test]
    fn settings_click_cycles_every_button() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        load(&|_| None);
        settings_click(2);
        assert_eq!(state().config.head_size, 6);
        settings_click(8);
        assert_eq!(state().config.names, NameMode::Always);
        settings_click(8);
        assert_eq!(state().config.names, NameMode::LookingAt);
        settings_click(9);
        assert_eq!(state().config.filter_mode, FilterMode::Include);
        assert!(!take_store().is_empty());
    }

    #[test]
    fn waypoint_table_tracks_updates_and_untracks() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_waypoints();
        // TRACK inserts; a second TRACK for the same id replaces it.
        note_waypoint(
            WAYPOINT_TRACK,
            WaypointId::Uuid(1),
            "minecraft:default".into(),
            None,
            WaypointTarget::Block([1, 2, 3]),
        );
        assert_eq!(waypoints().len(), 1);
        note_waypoint(
            WAYPOINT_TRACK,
            WaypointId::Uuid(1),
            "minecraft:default".into(),
            Some(0xFF0000),
            WaypointTarget::Block([4, 5, 6]),
        );
        assert_eq!(waypoints().len(), 1);
        assert_eq!(waypoints()[0].target, WaypointTarget::Block([4, 5, 6]));
        assert_eq!(waypoints()[0].color, Some(0xFF0000));
        // UPDATE moves a waypoint of the same kind only.
        note_waypoint(
            WAYPOINT_UPDATE,
            WaypointId::Uuid(1),
            "minecraft:default".into(),
            None,
            WaypointTarget::Block([7, 8, 9]),
        );
        assert_eq!(waypoints()[0].target, WaypointTarget::Block([7, 8, 9]));
        note_waypoint(
            WAYPOINT_UPDATE,
            WaypointId::Uuid(1),
            "minecraft:default".into(),
            None,
            WaypointTarget::Azimuth(1.0),
        );
        assert_eq!(waypoints()[0].target, WaypointTarget::Block([7, 8, 9]));
        // UPDATE for an unknown id and UNTRACK for one are no-ops.
        note_waypoint(
            WAYPOINT_UPDATE,
            WaypointId::Uuid(9),
            "minecraft:default".into(),
            None,
            WaypointTarget::Block([0, 0, 0]),
        );
        assert_eq!(waypoints().len(), 1);
        note_waypoint(
            WAYPOINT_UNTRACK,
            WaypointId::Uuid(9),
            "minecraft:default".into(),
            None,
            WaypointTarget::Empty,
        );
        assert_eq!(waypoints().len(), 1);
        note_waypoint(
            WAYPOINT_UNTRACK,
            WaypointId::Uuid(1),
            "minecraft:default".into(),
            None,
            WaypointTarget::Empty,
        );
        assert!(waypoints().is_empty());
        // A Name id tracks beside a UUID id.
        note_waypoint(
            WAYPOINT_TRACK,
            WaypointId::Name("home".into()),
            "minecraft:default".into(),
            None,
            WaypointTarget::Block([0, 64, 0]),
        );
        assert_eq!(waypoints().len(), 1);
        assert_eq!(waypoints()[0].id, WaypointId::Name("home".into()));
        clear_waypoints();
        assert!(waypoints().is_empty());
    }

    #[test]
    fn player_names_come_and_go_with_the_list() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_waypoints();
        assert_eq!(player_name(7), None);
        note_player_name(7, "Alice".into());
        assert_eq!(player_name(7), Some("Alice".to_string()));
        note_player_name(7, "Alicia".into());
        assert_eq!(player_name(7), Some("Alicia".to_string()));
        forget_player(7);
        assert_eq!(player_name(7), None);
        clear_player_names();
    }

    #[test]
    fn waypoint_positions_follow_the_target_kind() {
        let cam = [0.0, 64.0, 0.0];
        assert_eq!(
            waypoint_position(&WaypointTarget::Block([1, 2, 3]), cam),
            [1.5, 2.5, 3.5]
        );
        // A chunk's middle at the camera's height.
        assert_eq!(
            waypoint_position(&WaypointTarget::Chunk([0, 0]), cam),
            [8.5, 64.5, 8.5]
        );
        // An azimuth south (0 rad) reads straight ahead, far away.
        let south = waypoint_position(&WaypointTarget::Azimuth(0.0), cam);
        assert!((yaw_to(cam, 0.0, south) - 0.0).abs() < 0.001);
        assert!((distance_scale(400.0) - 5.0 / 9.0).abs() < 0.001);
        // An empty target sits on the camera: dead centre.
        assert_eq!(waypoint_position(&WaypointTarget::Empty, cam), cam);
    }

    #[test]
    fn marks_merge_loaded_and_waypoint_players() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_name_anim();
        let c = cfg();
        let loaded = [player("Alice", [0.0, 64.0, 10.0])];
        // A waypoint player past loaded range joins the bar.
        let mut f = frame(&loaded);
        let far = [PlayerIn {
            name: "Bob".into(),
            uuid: Some(2),
            pos: [0.0, 64.0, 200.0],
            team_rgb: None,
        }];
        f.waypoint_players = &far;
        let got = marks(&c, &f);
        assert_eq!(got.len(), 2);
        assert!(got.iter().any(|m| m.name == "Bob"));
        // The same UUID loaded wins: one mark, at the mob's position.
        let mut loaded2 = [player("Alice", [0.0, 64.0, 10.0])];
        loaded2[0].uuid = Some(1);
        let mut f2 = frame(&loaded2);
        let wp = [PlayerIn {
            name: "Alice".into(),
            uuid: Some(1),
            pos: [0.0, 64.0, 200.0],
            team_rgb: None,
        }];
        f2.waypoint_players = &wp;
        let got = marks(&c, &f2);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "Alice");
        assert!((got[0].distance - 10.0).abs() < 0.001);
    }

    #[test]
    fn unnamed_waypoint_heads_pass_the_filter() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_name_anim();
        let mut c = cfg();
        c.filter_mode = FilterMode::Exclude;
        c.exclude = "bob".to_string();
        let mut f = frame(&[]);
        let unnamed = [PlayerIn {
            name: String::new(),
            uuid: Some(3),
            pos: [0.0, 64.0, 10.0],
            team_rgb: None,
        }];
        f.waypoint_players = &unnamed;
        let got = marks(&c, &f);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "");
        // A named player in the exclude list still hides.
        let mut f2 = frame(&[]);
        let bob = [player("Bob", [0.0, 64.0, 10.0])];
        f2.waypoint_players = &bob;
        assert!(marks(&c, &f2).is_empty());
    }
}
