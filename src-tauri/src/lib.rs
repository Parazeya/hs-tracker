mod items;
mod parser;
mod presence;
mod sniffer;
mod stats;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sniffer::Shared;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Alert kinds that own a configurable sound (not item rarities — see stats).
const SOUND_KEYS: [&str; 6] = ["satanic", "set", "heroic", "angelic", "unholy", "mail"];

/// A sound is either one of the six built-in alerts or a list's own file,
/// named `list-<id>`. Anything else must not reach the filesystem.
fn sound_key(key: &str) -> bool {
    SOUND_KEYS.contains(&key)
        || (key.len() <= 40
            && key.starts_with("list-")
            && key[5..].chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
}
const SOUND_EXTS: [(&str, &str); 4] = [
    ("mp3", "audio/mpeg"),
    ("wav", "audio/wav"),
    ("ogg", "audio/ogg"),
    ("flac", "audio/flac"),
];

// The overlay's width never changes; its height is whatever its rows add up to,
// and the web side measures that itself (see `fit_overlay`). The figures here
// are only the opening bid, so the window is about right before the first frame
// rather than resizing in front of the player.
const BASE_W: f64 = 444.0;
const OVERLAY_ROWS: [&str; 6] = ["session", "gold", "xp", "items", "vitals", "zone"];

fn overlay_height(settings: &Settings) -> f64 {
    // what the overlay says it is, and only otherwise what its rows suggest
    let measured = PANEL_H.load(Ordering::Relaxed);
    if measured > 0 {
        return measured as f64;
    }
    let rows = OVERLAY_ROWS.iter().filter(|r| !settings.hidden.iter().any(|h| h == *r)).count();
    34.0 + 33.0 * rows.max(1) as f64
}

const HK_TOGGLE: &str = "ctrl+shift+o";
const HK_LOCK: &str = "ctrl+shift+l";
const HK_RESET: &str = "ctrl+shift+r";
const HK_PAUSE: &str = "ctrl+shift+p";

// lock-button rect in overlay CSS px, with a small margin (see App.svelte .lock)
const LOCK_RECT: (f64, f64, f64, f64) = (BASE_W - 32.0, 0.0, BASE_W, 34.0);

static LOCKED: AtomicBool = AtomicBool::new(false);
static TICKER: AtomicBool = AtomicBool::new(true);
/// The ticker is a transparent window pinned over the game: while it is on
/// screen the compositor keeps blending it, empty or not. It is only shown
/// while an entry is actually visible.
static TICKER_BUSY: AtomicBool = AtomicBool::new(false);
/// Whether the flourish is on at all. Which drops deserve one is the engine's
/// question, and it is asked there — see `set_flourish_filter`.
static FLOURISH: AtomicBool = AtomicBool::new(false);
static SCALE_MILLI: AtomicU32 = AtomicU32::new(1000);
/// The panel's own height in CSS pixels, as the overlay last measured it. Zero
/// until the first frame has been drawn, when the guess below stands in.
static PANEL_H: AtomicU32 = AtomicU32::new(0);
/// How long a run may show no sign of life before the clock stops, in seconds;
/// zero means the player would rather it never did.
pub(crate) static IDLE_AFTER: AtomicU32 = AtomicU32::new(IDLE_DEFAULT);
/// Five minutes: long enough to survive a boss fight, a long walk or a stretch
/// of bad luck, short enough that a tea break does not end up in the rates.
const IDLE_DEFAULT: u32 = 300;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SoundCfg {
    pub enabled: bool,
    pub volume: f32,
}

impl Default for SoundCfg {
    fn default() -> Self {
        Self { enabled: true, volume: 0.7 }
    }
}

/// A named set of items with a sound of its own. It outranks the rarity
/// alerts: an item on a list is announced by that list, whatever the rarity
/// switches and the minimum grade say.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SoundList {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub volume: f32,
    pub items: Vec<String>,
}

impl Default for SoundList {
    fn default() -> Self {
        Self { id: String::new(), name: String::new(), enabled: true, volume: 0.7, items: Vec::new() }
    }
}

/// A pack of lists, the way a loot filter is a pack of rules. One is active at
/// a time, so a farming filter and a trading filter can live side by side.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct SoundFilter {
    pub id: String,
    pub name: String,
    pub lists: Vec<SoundList>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct NotableGroup {
    pub label: String,
    pub names: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub satanic: SoundCfg,
    pub set: SoundCfg,
    pub heroic: SoundCfg,
    pub angelic: SoundCfg,
    pub unholy: SoundCfg,
    pub mail: SoundCfg,
    /// rarities worth announcing at all, and the tier they must reach
    pub alerts: Vec<String>,
    pub min_tier: i64,
    /// named drops that get their own counter: label -> item names
    pub notable: Vec<NotableGroup>,
    /// sound filters, one of which may be switched on
    pub filters: Vec<SoundFilter>,
    pub filter: String,
    pub use_filter: bool,
    /// pre-0.9.4 lists, folded into a filter on load
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lists: Vec<SoundList>,
    pub locked: bool,
    pub opacity: f32,
    pub scale: f32,
    pub auto_show: bool,
    pub autostart: bool,
    pub ticker: bool,
    pub debug_log: bool,
    pub sound_on_ground: bool,
    /// stop the session clock when nothing has happened for a while, so a break
    /// does not quietly halve every per-hour figure
    pub auto_pause: bool,
    /// which skin the windows wear: "default", or a season's own colours
    pub theme: String,
    /// A window that plays the game's own loot pillar when something worth it
    /// drops. Off by default: it is a window over the game, and that is the
    /// player's screen to give away, not ours to take.
    pub flourish: bool,
    /// how big it is drawn, how hard it shades the game behind it, and how long
    /// it stays on screen
    pub flourish_scale: f32,
    pub flourish_shade: f32,
    pub flourish_secs: f32,
    /// which rarities are worth it, and the grade a drop must reach
    pub flourish_rarities: Vec<String>,
    pub flourish_tier: i64,
    /// show the run in Discord while the game is open. Off unless asked for:
    /// it puts what the player is doing in front of everyone on their list.
    pub discord: bool,
    /// which face was up last: the overlay (true) or the dashboard
    pub compact: bool,
    /// Linux only: enter a Wayland session through XWayland, which is what
    /// gives the overlay a display server that lets it float and be clicked
    /// through. Chosen in Settings, applied at the next start.
    pub x11_backend: bool,
    pub hidden: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            satanic: SoundCfg::default(),
            set: SoundCfg::default(),
            heroic: SoundCfg::default(),
            angelic: SoundCfg::default(),
            unholy: SoundCfg::default(),
            mail: SoundCfg::default(),
            alerts: stats::JOURNAL_RARITIES.iter().map(|r| r.to_string()).collect(),
            min_tier: 0,
            notable: stats::default_notable()
                .into_iter()
                .map(|(label, names)| NotableGroup { label, names })
                .collect(),
            filters: Vec::new(),
            filter: String::new(),
            use_filter: true,
            lists: Vec::new(),
            locked: false,
            opacity: 1.0,
            scale: 1.0,
            auto_show: true,
            autostart: false,
            ticker: true,
            debug_log: false,
            sound_on_ground: true,
            auto_pause: true,
            theme: "default".into(),
            flourish: false,
            flourish_scale: 1.0,
            flourish_shade: 0.55,
            flourish_secs: 6.0,
            flourish_rarities: ["Heroic", "Angelic", "Unholy"].iter().map(|r| r.to_string()).collect(),
            flourish_tier: 6,
            discord: false,
            compact: false,
            x11_backend: false,
            hidden: Vec::new(),
        }
    }
}

static DEBUG_LOG: AtomicBool = AtomicBool::new(false);

/// Whether this session can host the overlay at all.
///
/// The overlay is a click-through, always-on-top window that follows the mouse
/// and answers global hotkeys. Wayland gives an application none of those on
/// purpose: it may not place itself, may not float above another program's
/// fullscreen window, and may not read the pointer outside itself. Rather than
/// draw an overlay that lies where the compositor pleases and cannot be
/// unlocked, the app runs as the dashboard alone there.
///
/// Windows and X11 are unaffected. Forcing the GTK backend to X11 (which runs
/// the app through XWayland) also brings the overlay back, and is honoured here.
#[cfg(windows)]
pub(crate) fn overlay_supported() -> bool {
    true
}

/// GDK_BACKEND is a priority list, not a single choice: "wayland,x11" still
/// lands on Wayland. Only the first entry says what the toolkit will use.
#[cfg(not(windows))]
fn forced_x11() -> bool {
    std::env::var("GDK_BACKEND")
        .is_ok_and(|v| v.to_lowercase().split(',').next().is_some_and(|first| first.trim() == "x11"))
}

#[cfg(not(windows))]
fn wayland_session() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v.eq_ignore_ascii_case("wayland"))
}

/// XWayland's socket, which is what the X11 backend actually needs.
#[cfg(not(windows))]
fn x11_reachable() -> bool {
    std::env::var_os("DISPLAY").is_some()
}

#[cfg(not(windows))]
pub(crate) fn overlay_supported() -> bool {
    forced_x11() || !wayland_session()
}

/// What the windows need to know about the session they are drawn in.
#[derive(Serialize)]
pub struct SessionInfo {
    /// the overlay can exist here
    overlay: bool,
    /// a Wayland session, whichever backend the toolkit ended up using
    wayland: bool,
    /// a Wayland session the app was told to enter through XWayland
    through_x11: bool,
    /// XWayland is there to switch to
    can_switch: bool,
}

#[tauri::command]
fn session_info() -> SessionInfo {
    #[cfg(windows)]
    {
        SessionInfo { overlay: true, wayland: false, through_x11: false, can_switch: false }
    }
    #[cfg(not(windows))]
    {
        let wayland = wayland_session();
        SessionInfo {
            overlay: overlay_supported(),
            wayland,
            through_x11: forced_x11(),
            can_switch: wayland && x11_reachable(),
        }
    }
}

/// Restart into the other display backend.
///
/// A Wayland session gives an application no overlay, but XWayland does — and
/// the game itself runs through XWayland when it runs through Proton, so the
/// two end up in the same X server where one can sit above the other. Rather
/// than teach the user about `GDK_BACKEND`, the app relaunches itself.
#[tauri::command]
fn restart_backend(app: AppHandle, x11: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        let _ = (app, x11);
        Err("Windows has one backend".into())
    }
    #[cfg(not(windows))]
    {
        if x11 && !x11_reachable() {
            return Err("no X server to switch to — this session has no XWayland".into());
        }
        // Spawn first: the choice is only worth remembering once a replacement
        // is actually on its way. Written before, a launch that fails leaves an
        // app that relaunches into the same failure at every start, with no
        // window left to undo it in.
        relaunch(x11)?;
        let mut settings = read_settings();
        settings.x11_backend = x11;
        save_settings(app.clone(), settings)?;
        app.exit(0);
        Ok(())
    }
}

/// Start a fresh copy of ourselves on the chosen backend.
#[cfg(not(windows))]
fn relaunch(x11: bool) -> Result<(), String> {
    // inside an AppImage the mounted binary is not what the user keeps
    let exe = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .ok_or_else(|| "cannot find my own binary".to_string())?;
    let mut cmd = std::process::Command::new(exe);
    if x11 {
        cmd.env("GDK_BACKEND", "x11");
    } else {
        cmd.env_remove("GDK_BACKEND");
    }
    // a marker so the replacement never tries to relaunch itself again
    cmd.env("HS_TRACKER_RELAUNCHED", "1");
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    // spawn only reports that fork and exec worked; a toolkit that cannot open
    // its display dies a moment later, and that must not be mistaken for success
    std::thread::sleep(Duration::from_millis(700));
    match child.try_wait() {
        Ok(Some(status)) => Err(format!("the restarted app exited immediately ({status})")),
        _ => Ok(()),
    }
}

/// Append every parsed message to debug-capture.jsonl so a real session can be
/// replayed against the parser when counters look wrong.
pub(crate) fn debug_log(messages: &[serde_json::Value], src: std::net::IpAddr) {
    use std::io::Write;
    if !DEBUG_LOG.load(Ordering::Relaxed) {
        return;
    }
    // the file stays open: with the wide capture this runs many times a second
    static FILE: std::sync::Mutex<Option<std::io::BufWriter<std::fs::File>>> =
        std::sync::Mutex::new(None);
    let Ok(mut guard) = FILE.lock() else { return };
    if guard.is_none() {
        let opened = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(data_dir().join("debug-capture.jsonl"));
        let Ok(f) = opened else { return };
        *guard = Some(std::io::BufWriter::new(f));
    }
    let Some(f) = guard.as_mut() else { return };
    for m in messages {
        // the sender is what tells a character upload apart from the server's
        // copy of it
        let tagged = match m {
            serde_json::Value::Object(o) => {
                let mut o = o.clone();
                o.insert("_src".into(), serde_json::Value::String(src.to_string()));
                serde_json::Value::Object(o)
            }
            other => other.clone(),
        };
        if let Ok(line) = serde_json::to_string(&tagged) {
            let _ = writeln!(f, "{line}");
        }
    }
    let _ = f.flush();
}

/// `npm start` builds: every parsed event goes to the terminal, and the
/// overlay opens with devtools so the webview console is visible too.
#[cfg(debug_assertions)]
pub(crate) fn dev_log(events: &[parser::GameEvent], src: std::net::IpAddr) {
    for e in events {
        let line = match e {
            parser::GameEvent::Gold(c) => format!("gold  GSS {} GSH {} GNS {} +{}", c.gss, c.gsh, c.gns, c.delta),
            parser::GameEvent::XpGain(xp) => format!("xp    +{xp} (guild share)"),
            parser::GameEvent::Account { experience, kills, name, .. } => {
                format!("save  {name}: xp {experience}, kills {kills}")
            }
            parser::GameEvent::Mail(has) => format!("mail  {has}"),
            parser::GameEvent::Room(room) => format!("room  {room}"),
            parser::GameEvent::Vitals { mf, level, hlevel, satanic_here } => {
                format!("vitals  mf {mf}  lv {level}  hlv {hlevel}  sz {satanic_here}")
            }
            parser::GameEvent::ItemAdded { name, rarity, tier, ground, item_type, item_id, weapon_type, .. } => {
                // an empty name means the item tables predate this item
                let label = if name.is_empty() {
                    format!("unknown {item_type}:{item_id}:{weapon_type}")
                } else {
                    name.clone()
                };
                format!("item  {label:?} rarity {rarity} tier {tier} {}", if *ground { "on the ground" } else { "picked up" })
            }
            parser::GameEvent::SatanicZone { zone, .. } => format!("zone  {zone}"),
        };
        println!("[{src}] {line}");
    }
}

#[cfg(not(debug_assertions))]
pub(crate) fn dev_log(_: &[parser::GameEvent], _: std::net::IpAddr) {}

#[cfg(windows)]
fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default()
}

/// Everything the app writes lives here. On Windows that is the folder the
/// installer put the exe in, which keeps the app portable — copy the folder,
/// keep the settings. Elsewhere the binary lands in /usr/bin or inside a
/// read-only AppImage, so the XDG config directory is the only sane home.
#[cfg(windows)]
fn data_dir() -> PathBuf {
    exe_dir()
}

#[cfg(not(windows))]
fn data_dir() -> PathBuf {
    // resolved and created once; every settings read would otherwise stat it
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from("."));
        let dir = base.join("hs-tracker");
        let _ = std::fs::create_dir_all(&dir);
        dir
    })
    .clone()
}

fn sounds_dir() -> PathBuf {
    data_dir().join("sounds")
}

fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

fn shopping_path() -> PathBuf {
    data_dir().join("shopping.json")
}

fn positions_path() -> PathBuf {
    data_dir().join("positions.json")
}

fn carried_path() -> PathBuf {
    data_dir().join("carried.json")
}

/// Bank balance, experience and kills as of the last run. The game only sends
/// them when it saves, so without this a restart shows zeros until the next
/// save — which can be a whole farming run away.
fn read_carried() -> stats::Carried {
    std::fs::read_to_string(carried_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_carried(app: &AppHandle) {
    let carried = app.state::<Shared>().stats.lock().unwrap().carried();
    if let Ok(json) = serde_json::to_string(&carried) {
        let _ = std::fs::write(carried_path(), json);
    }
}

// The flourish is here because the player puts it somewhere deliberately: it is
// the one window whose position is a choice rather than a convenience.
const REMEMBERED_WINDOWS: [&str; 3] = ["main", "dashboard", "flourish"];

/// Where each window was, and how big — the dashboard can be resized, so its
/// size is worth remembering too. Only windows that are actually on screen have
/// geometry worth writing down: a hidden one reports (0, 0) on GTK and a
/// minimised one reports the parking lot Windows keeps them in.
fn window_positions(app: &AppHandle) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for label in REMEMBERED_WINDOWS {
        let Some(w) = app.get_webview_window(label) else { continue };
        if !on_screen(&w) {
            // keep whatever the last run knew rather than overwrite it with junk
            if let Some(pos) = parked(label) {
                if let Ok(size) = w.outer_size() {
                    map.insert(label.into(), serde_json::json!([pos.x, pos.y, size.width, size.height]));
                }
            }
            continue;
        }
        if let (Ok(pos), Ok(size)) = (w.outer_position(), w.outer_size()) {
            map.insert(label.into(), serde_json::json!([pos.x, pos.y, size.width, size.height]));
        }
    }
    map
}

fn save_window_positions(app: &AppHandle) {
    if let Ok(json) = serde_json::to_string(&window_positions(app)) {
        let _ = std::fs::write(positions_path(), json);
    }
}

/// A clean exit is not guaranteed (task manager, crash), so positions are also
/// written a few seconds after they stop changing.
fn spawn_position_saver(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last = window_positions(&app);
        let mut dirty_since: Option<Instant> = None;
        let (mut saved_revision, mut saved_at) = (0, Instant::now());
        loop {
            std::thread::sleep(Duration::from_millis(1000));
            let now = window_positions(&app);
            if now != last {
                last = now;
                dirty_since = Some(Instant::now());
                continue;
            }
            if dirty_since.is_some_and(|t| t.elapsed() >= Duration::from_secs(2)) {
                dirty_since = None;
                save_window_positions(&app);
            }
            let revision = app.state::<Shared>().stats.lock().unwrap().revision();
            if revision != saved_revision && saved_at.elapsed() >= Duration::from_secs(20) {
                saved_revision = revision;
                saved_at = Instant::now();
                save_carried(&app);
            }
        }
    });
}

/// Restore saved positions, but only onto a connected monitor.
fn restore_window_positions(app: &AppHandle) {
    let Ok(saved) = std::fs::read_to_string(positions_path()) else { return };
    let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&saved) else { return };
    let monitors = app.available_monitors().unwrap_or_default();
    let on_screen = |x: i32, y: i32| {
        monitors.iter().any(|m| {
            let p = m.position();
            let s = m.size();
            x >= p.x - 50 && x < p.x + s.width as i32 && y >= p.y - 50 && y < p.y + s.height as i32
        })
    };
    for label in REMEMBERED_WINDOWS {
        let Some(pos) = map.get(label).and_then(|v| v.as_array()) else { continue };
        let (Some(x), Some(y)) = (pos.first().and_then(|v| v.as_i64()), pos.get(1).and_then(|v| v.as_i64())) else {
            continue;
        };
        if !on_screen(x as i32, y as i32) {
            continue;
        }
        let Some(w) = app.get_webview_window(label) else { continue };
        // seed the in-memory copy too: a window that starts hidden has no
        // geometry of its own to save later, and this is where it comes from
        park(label, tauri::PhysicalPosition::new(x as i32, y as i32));
        let _ = w.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
        // older files hold just a position; a size only comes back if it fits
        if let (Some(width), Some(height)) = (
            pos.get(2).and_then(|v| v.as_u64()),
            pos.get(3).and_then(|v| v.as_u64()),
        ) {
            if width >= 200 && height >= 200 {
                let _ = w.set_size(tauri::PhysicalSize::new(width as u32, height as u32));
            }
        }
    }
}

/// The drop ticker is a pure display glued right under the overlay: always
/// click-through, follows the overlay, hides with it.
fn spawn_ticker_glue(app: AppHandle) {
    std::thread::spawn(move || {
        let mut shown = false;
        let mut placed: Option<(tauri::PhysicalPosition<i32>, tauri::PhysicalSize<u32>)> = None;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let (Some(main), Some(ticker)) = (app.get_webview_window("main"), app.get_webview_window("ticker"))
            else {
                continue;
            };
            let visible = main.is_visible().unwrap_or(false)
                && TICKER.load(Ordering::Relaxed)
                && TICKER_BUSY.load(Ordering::Relaxed);
            if !visible {
                if shown {
                    let _ = ticker.hide();
                    shown = false;
                    // hiding unmaps it, and a window manager may place it
                    // afresh: the next show has to position it again
                    placed = None;
                }
                continue;
            }
            if let (Ok(pos), Ok(size), Ok(dpi)) = (main.outer_position(), main.outer_size(), main.scale_factor()) {
                let scale = SCALE_MILLI.load(Ordering::Relaxed) as f64 / 1000.0;
                let height = (170.0 * scale * dpi) as u32;
                let want = (
                    tauri::PhysicalPosition::new(pos.x, pos.y + size.height as i32 + 4),
                    tauri::PhysicalSize::new(size.width, height),
                );
                if placed != Some(want) {
                    let _ = ticker.set_position(want.0);
                    let _ = ticker.set_size(want.1);
                    placed = Some(want);
                }
                if !shown {
                    let _ = ticker.show();
                    let _ = ticker.set_ignore_cursor_events(true);
                    shown = true;
                }
            }
        }
    });
}


/// Counters are pushed, not polled: the webviews used to ask for a snapshot
/// twice a second each — the statistics view even asked for the whole graph
/// series and drop journal while hidden. Now one thread coalesces changes and
/// emits only to what is actually on screen. The heartbeats keep the per-hour
/// rates fresh while nothing is dropping.
const SNAP_MIN_GAP: Duration = Duration::from_millis(400);
const SNAP_HEARTBEAT: Duration = Duration::from_millis(2000);
const EXTRA_MIN_GAP: Duration = Duration::from_millis(1000);
const EXTRA_HEARTBEAT: Duration = Duration::from_millis(5000);

/// The dashboard shows one section at a time and says which, so the heavy
/// payload can stay home while the user is on Settings or Sounds.
static STATS_SECTION: AtomicBool = AtomicBool::new(true);

#[tauri::command]
fn viewing(section: String) {
    STATS_SECTION.store(section == "stats", Ordering::Relaxed);
}

fn spawn_stats_pusher(app: AppHandle) {
    std::thread::spawn(move || {
        // minimised counts as off screen: the dashboard can sit in the taskbar
        // for a whole run, and nothing there needs the numbers
        let visible = |label: &str| {
            app.get_webview_window(label)
                .map(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
                .unwrap_or(false)
        };
        let (mut snap_rev, mut extra_rev) = (u64::MAX, u64::MAX);
        let mut snap_at = Instant::now() - SNAP_HEARTBEAT;
        let mut extra_at = Instant::now() - EXTRA_HEARTBEAT;
        let (mut had_main, mut had_dash) = (false, false);
        let mut had_mail = false;
        loop {
            std::thread::sleep(Duration::from_millis(200));
            // The mail chime is announced on its own, before anything about
            // visibility is decided: the counters may be behind a hidden window
            // all run, and the reminder is the point of them.
            let mail = app.state::<Shared>().stats.lock().unwrap().has_mail();
            if mail && !had_mail {
                let _ = app.emit("mail", ());
            }
            had_mail = mail;

            let (main, dashboard) = (visible("main"), visible("dashboard"));
            // a window that just appeared gets the current numbers at once
            if main && !had_main {
                snap_rev = u64::MAX;
            }
            if dashboard && !had_dash {
                (snap_rev, extra_rev) = (u64::MAX, u64::MAX);
            }
            (had_main, had_dash) = (main, dashboard);
            if !main && !dashboard {
                continue;
            }
            let shared = app.state::<Shared>();
            let revision = shared.stats.lock().unwrap().revision();

            let due = |rev: u64, at: Instant, gap: Duration, beat: Duration| {
                (rev != revision && at.elapsed() >= gap) || at.elapsed() >= beat
            };
            if due(snap_rev, snap_at, SNAP_MIN_GAP, SNAP_HEARTBEAT) {
                let status = shared.status.lock().unwrap().text();
                let snapshot = shared.stats.lock().unwrap().snapshot(status);
                for (label, on_screen) in [("main", main), ("dashboard", dashboard)] {
                    if on_screen {
                        let _ = app.emit_to(label, "stats", &snapshot);
                    }
                }
                (snap_rev, snap_at) = (revision, Instant::now());
            }
            // the series and the drop journal are the heaviest payload in the
            // app, so they only travel while the statistics section is open
            let reading_stats = dashboard && STATS_SECTION.load(Ordering::Relaxed);
            if reading_stats && due(extra_rev, extra_at, EXTRA_MIN_GAP, EXTRA_HEARTBEAT) {
                let extra = shared.stats.lock().unwrap().extra();
                let _ = app.emit_to("dashboard", "stats-extra", &extra);
                (extra_rev, extra_at) = (revision, Instant::now());
            }
        }
    });
}

pub(crate) fn read_settings() -> Settings {
    let mut settings: Settings = std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    migrate_notable(&mut settings);
    migrate_lists(&mut settings);
    settings
}

/// Lists used to live loose in the settings; they are a filter's contents now.
fn migrate_lists(settings: &mut Settings) {
    if settings.lists.is_empty() {
        return;
    }
    let lists = std::mem::take(&mut settings.lists);
    settings.filters.push(SoundFilter { id: "mine".into(), name: "My filter".into(), lists });
    if settings.filter.is_empty() {
        settings.filter = "mine".into();
    }
}

/// The rune groups were guesses until the item tables gained the game's own
/// grades. A settings file still holding the guess is refreshed; anything the
/// user has edited themselves is left alone.
fn migrate_notable(settings: &mut Settings) {
    const GUESSED: [&str; 2] = [
        "gul rune,vex rune,qi rune,xo rune,sur rune",
        "ber rune,jah rune,drax rune,zed rune",
    ];
    for group in &mut settings.notable {
        let joined = group.names.join(",").to_lowercase();
        if GUESSED.contains(&joined.as_str()) {
            if let Some((_, names)) = stats::default_notable().into_iter().find(|(l, _)| *l == group.label) {
                group.names = names;
            }
        }
    }
}

fn apply_stats_settings(app: &AppHandle, settings: &Settings) {
    let shared = app.state::<Shared>();
    let mut stats = shared.stats.lock().unwrap();
    stats.set_prefer_ground(settings.sound_on_ground);
    // a rarity dropped from the tracked list must stop alerting even if an
    // older settings file still names it
    let alerts = settings
        .alerts
        .iter()
        .filter(|r| stats::JOURNAL_RARITIES.contains(&r.as_str()))
        .cloned()
        .collect();
    stats.set_filter(alerts, settings.min_tier);
    // the flourish asks a different question of the same drop
    let fx = if settings.flourish { settings.flourish_rarities.clone() } else { Vec::new() };
    stats.set_flourish_filter(fx, settings.flourish_tier.clamp(1, 6));
    let active = settings
        .use_filter
        .then(|| settings.filters.iter().find(|f| f.id == settings.filter))
        .flatten();
    stats.set_sound_lists(
        active
            .map(|f| {
                f.lists
                    .iter()
                    .filter(|l| l.enabled && !l.id.is_empty() && !l.items.is_empty())
                    .map(|l| (format!("list-{}", l.id), l.items.clone()))
                    .collect()
            })
            .unwrap_or_default(),
    );
    stats.set_notable(
        settings
            .notable
            .iter()
            .map(|g| (g.label.clone(), g.names.iter().map(|n| n.to_lowercase()).collect()))
            .collect(),
    );
}

/// Everything a settings change touches outside the webviews.
fn apply_settings_effects(app: &AppHandle, settings: &Settings) {
    let scale = settings.scale.clamp(0.6, 1.5) as f64;
    LOCKED.store(settings.locked, Ordering::Relaxed);
    TICKER.store(settings.ticker, Ordering::Relaxed);
    DEBUG_LOG.store(settings.debug_log, Ordering::Relaxed);
    SCALE_MILLI.store((scale * 1000.0) as u32, Ordering::Relaxed);
    presence::set_enabled(settings.discord);
    FLOURISH.store(settings.flourish, Ordering::Relaxed);
    ensure_flourish(app, settings.flourish, settings.flourish_scale.clamp(0.5, 2.0) as f64);
    IDLE_AFTER.store(if settings.auto_pause { IDLE_DEFAULT } else { 0 }, Ordering::Relaxed);
    if let Some(w) = app.get_webview_window("main") {
        // the lock poller owns this once the overlay is up; touching a window
        // that has never been shown is what breaks on GTK
        if w.is_visible().unwrap_or(false) {
            let _ = w.set_ignore_cursor_events(settings.locked);
        }
        let _ = w.set_zoom(scale);
        let _ = w.set_size(LogicalSize::new(BASE_W * scale, overlay_height(settings) * scale));
    }
    apply_autostart(settings.autostart);
}

/// While locked the overlay is click-through EXCEPT the lock button: a poller
/// re-enables mouse events whenever the cursor is over the button's corner.
fn spawn_lock_poller(app: AppHandle) {
    std::thread::spawn(move || {
        let mut ignoring: Option<bool> = None;
        loop {
            let locked = LOCKED.load(Ordering::Relaxed);
            if !locked {
                if ignoring != Some(false) {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.set_ignore_cursor_events(false);
                    }
                    ignoring = Some(false);
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            }
            let over = (|| {
                let w = app.get_webview_window("main")?;
                if !w.is_visible().ok()? {
                    return None;
                }
                let pos = w.outer_position().ok()?;
                let dpi = w.scale_factor().ok()?;
                let cur = app.cursor_position().ok()?;
                let z = dpi * SCALE_MILLI.load(Ordering::Relaxed) as f64 / 1000.0;
                let (x0, y0, x1, y1) = LOCK_RECT;
                Some(
                    cur.x >= pos.x as f64 + x0 * z
                        && cur.x <= pos.x as f64 + x1 * z
                        && cur.y >= pos.y as f64 + y0 * z
                        && cur.y <= pos.y as f64 + y1 * z,
                )
            })()
            .unwrap_or(false);
            let want_ignore = !over;
            if ignoring != Some(want_ignore) {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.set_ignore_cursor_events(want_ignore);
                }
                ignoring = Some(want_ignore);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    });
}

#[cfg(windows)]
fn apply_autostart(enabled: bool) {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let Ok(run) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", winreg::enums::KEY_ALL_ACCESS)
    else {
        return;
    };
    let _ = run.delete_value("HS Companion"); // pre-rename entry
    if enabled {
        if let Ok(exe) = std::env::current_exe() {
            let _ = run.set_value("HS Tracker", &format!("\"{}\"", exe.display()));
        }
    } else {
        let _ = run.delete_value("HS Tracker");
    }
}

/// The freedesktop equivalent of the Run key: a .desktop file the session
/// launches on login.
#[cfg(not(windows))]
fn apply_autostart(enabled: bool) {
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|c| c.join("autostart"));
    let Some(dir) = dir else { return };
    let entry = dir.join("hs-tracker.desktop");
    if !enabled {
        let _ = std::fs::remove_file(entry);
        return;
    }
    // Inside an AppImage the running binary lives on a mount that is gone by
    // the next login; $APPIMAGE is the file the user actually keeps.
    let target = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok());
    let Some(target) = target else { return };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    // Exec is parsed as an argv, so a path with a space has to be quoted, and
    // the spec's own escapes have to survive quoting
    let quoted = format!("\"{}\"", target.display().to_string().replace('\\', "\\\\").replace('"', "\\\""));
    let desktop = format!(
        "[Desktop Entry]\nType=Application\nName=HS Tracker\nComment=Hero Siege session tracker\nExec={quoted}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    );
    let _ = std::fs::write(entry, desktop);
}

#[tauri::command]
fn get_settings() -> Settings {
    read_settings()
}

#[tauri::command]
fn save_settings(app: AppHandle, mut settings: Settings) -> Result<(), String> {
    for cfg in [
        &mut settings.satanic,
        &mut settings.set,
        &mut settings.heroic,
        &mut settings.angelic,
        &mut settings.unholy,
        &mut settings.mail,
    ] {
        cfg.volume = cfg.volume.clamp(0.0, 1.0);
    }
    settings.opacity = settings.opacity.clamp(0.3, 1.0);
    settings.scale = settings.scale.clamp(0.6, 1.5);
    settings.min_tier = settings.min_tier.clamp(0, 20);
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(settings_path(), json).map_err(|e| e.to_string())?;
    apply_stats_settings(&app, &settings);
    apply_settings_effects(&app, &settings);
    let _ = app.emit("settings-changed", &settings);
    Ok(())
}

#[tauri::command]
fn snapshot(state: State<Shared>) -> stats::Snapshot {
    let status = state.status.lock().unwrap().text();
    state.stats.lock().unwrap().snapshot(status)
}

#[tauri::command]
fn get_extra(state: State<Shared>) -> stats::Extra {
    state.stats.lock().unwrap().extra()
}

/// Runs are kept next to the settings, newest first, and the file is bounded:
/// this is a record of what happened, not a database.
const RUNS_KEPT: usize = 200;

fn runs_path() -> PathBuf {
    data_dir().join("runs.json")
}

pub(crate) fn read_runs() -> Vec<stats::Run> {
    std::fs::read_to_string(runs_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// End the session and file it away. Everything that ends a run goes through
/// here — the button, the hotkey, the tray, the game closing and the app
/// quitting — so a run is never lost and never counted twice.
pub(crate) fn end_run(app: &AppHandle) {
    let finished = app.state::<Shared>().stats.lock().unwrap().finish();
    app.state::<Shared>().stats.lock().unwrap().reset();
    let Some(run) = finished else { return };
    let mut runs = read_runs();
    runs.insert(0, run);
    runs.truncate(RUNS_KEPT);
    if let Ok(json) = serde_json::to_string(&runs) {
        let _ = std::fs::write(runs_path(), json);
    }
    let _ = app.emit("runs-changed", ());
}

fn close_session(app: &AppHandle) {
    end_run(app);
}

#[tauri::command]
fn get_runs() -> Vec<stats::Run> {
    read_runs()
}

#[tauri::command]
fn clear_runs() -> Result<(), String> {
    std::fs::write(runs_path(), "[]").map_err(|e| e.to_string())
}

#[tauri::command]
fn reset_stats(app: AppHandle) {
    close_session(&app);
}

/// Stop or restart the session clock. The counters are untouched either way —
/// what a pause changes is what the run is divided by.
#[tauri::command]
fn set_paused(app: AppHandle, paused: bool) {
    app.state::<Shared>().stats.lock().unwrap().set_paused(paused);
}

fn toggle_pause(app: &AppHandle) {
    let shared = app.state::<Shared>();
    let mut stats = shared.stats.lock().unwrap();
    let on = !stats.paused();
    stats.set_paused(on);
}

/// The overlay is exactly as tall as its panel, and the panel is drawn by the
/// web side — so that is what knows the height. Working it out here meant a
/// formula kept in step with the CSS by hand, and the row added in 0.9.8 is what
/// that costs: the panel grew, the window did not, and the last row was cut off.
/// The size the flourish window is built at, before the player's scale.
// wide enough for the name with a burst either side of it, and no taller than
// that needs — there is no beam to make room for
const FLOURISH_W: f64 = 560.0;
const FLOURISH_H: f64 = 220.0;

/// Build the flourish window, or take it down again.
///
/// It is not declared in tauri.conf.json on purpose: a window there is created
/// at every start whether it is wanted or not, and this one is a third webview
/// — on Linux a third GL context, which is exactly where the driver trouble we
/// have already been bitten by lives. A player who leaves the feature off never
/// pays for it.
fn ensure_flourish(app: &AppHandle, wanted: bool, scale: f64) {
    let existing = app.get_webview_window("flourish");
    if !wanted || !overlay_supported() {
        if let Some(w) = existing {
            let _ = w.destroy();
        }
        return;
    }
    let size = LogicalSize::new(FLOURISH_W * scale, FLOURISH_H * scale);
    if let Some(w) = existing {
        let _ = w.set_size(size);
        return;
    }
    let built = tauri::WebviewWindowBuilder::new(app, "flourish", tauri::WebviewUrl::default())
        .title("HS Tracker Flourish")
        .inner_size(size.width, size.height)
        .resizable(false)
        .visible(false)
        .focused(false)
        .focusable(false)
        .always_on_top(true)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .build();
    match built {
        Ok(w) => {
            let _ = w.set_ignore_cursor_events(true);
        }
        Err(e) => eprintln!("the flourish window could not be built: {e}"),
    }
}

/// Whether a drop is worth stopping the screen for, and if so, showing it.
pub(crate) fn maybe_flourish(app: &AppHandle, drop: &stats::DropEntry) {
    if !FLOURISH.load(Ordering::Relaxed) {
        return;
    }
    // The server announces a find in chat and the client also rolls it on the
    // ground: one item, two sightings, and nobody wants it announced twice.
    if !drop.name.is_empty() {
        static SHOWN: std::sync::Mutex<Vec<(String, Instant)>> = std::sync::Mutex::new(Vec::new());
        if let Ok(mut seen) = SHOWN.lock() {
            seen.retain(|(_, at)| at.elapsed() < Duration::from_secs(20));
            if seen.iter().any(|(n, _)| n == &drop.name) {
                return;
            }
            seen.push((drop.name.clone(), Instant::now()));
        }
    }
    let Some(w) = app.get_webview_window("flourish") else { return };
    let _ = app.emit_to("flourish", "flourish-play", drop);
    show_flourish(app, &w);
}

/// On screen without taking the keyboard, click-through, and where the player
/// left it. It hides itself again when the animation is over — the window tells
/// us, because it is the one that knows how long that is.
fn show_flourish(app: &AppHandle, w: &tauri::WebviewWindow) {
    if w.is_visible().unwrap_or(false) {
        return;
    }
    reveal(app, "flourish", false);
    let _ = w.set_ignore_cursor_events(true);
}

/// The window says when it has finished playing, or that the player has parked
/// it and it may go away again.
#[tauri::command]
fn flourish_done(app: AppHandle) {
    if PLACING.load(Ordering::Relaxed) {
        return;
    }
    hide_aux(&app, "flourish");
}

/// While a flourish is being placed it stays on screen, takes the mouse and
/// loops, so it can be dragged where the player wants it.
static PLACING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
fn place_flourish(app: AppHandle, placing: bool) {
    PLACING.store(placing, Ordering::Relaxed);
    let Some(w) = app.get_webview_window("flourish") else { return };
    if placing {
        reveal(&app, "flourish", false);
        let _ = w.set_ignore_cursor_events(false);
        let _ = app.emit_to("flourish", "flourish-placing", true);
    } else {
        let _ = app.emit_to("flourish", "flourish-placing", false);
        let _ = w.set_ignore_cursor_events(true);
        hide_aux(&app, "flourish");
    }
}

#[tauri::command]
fn fit_overlay(app: AppHandle, height: f64) {
    let height = height.clamp(60.0, 1200.0);
    // kept for the scale slider: zoom changes the window without changing a
    // single CSS pixel of the panel, so nothing would measure it again
    PANEL_H.store(height.round() as u32, Ordering::Relaxed);
    let Some(w) = app.get_webview_window("main") else { return };
    let scale = SCALE_MILLI.load(Ordering::Relaxed) as f64 / 1000.0;
    let wanted = LogicalSize::new(BASE_W * scale, height * scale);
    // a resize that changes nothing still goes through the window manager, and
    // on X11 that can shift the window out from under the player
    if let (Ok(now), Ok(factor)) = (w.inner_size(), w.scale_factor()) {
        let now = now.to_logical::<f64>(factor);
        if (now.height - wanted.height).abs() < 1.5 && (now.width - wanted.width).abs() < 1.5 {
            return;
        }
    }
    let _ = w.set_size(wanted);
}

#[tauri::command]
fn hide_window(app: AppHandle) {
    hide_aux(&app, "main");
}

/// Where each window stood when it was last hidden. Hiding a window unmaps it,
/// and a window manager is free to place it afresh when it comes back — KWin
/// centres it, which drags the overlay out from the corner the player put it
/// in. Windows keeps the position by itself; restoring it there costs nothing.
static PARKED: std::sync::Mutex<Vec<(String, tauri::PhysicalPosition<i32>)>> =
    std::sync::Mutex::new(Vec::new());

fn park(label: &str, pos: tauri::PhysicalPosition<i32>) {
    let Ok(mut parked) = PARKED.lock() else { return };
    match parked.iter_mut().find(|(l, _)| l == label) {
        Some(slot) => slot.1 = pos,
        None => parked.push((label.to_string(), pos)),
    }
}

fn parked(label: &str) -> Option<tauri::PhysicalPosition<i32>> {
    let parked = PARKED.lock().ok()?;
    parked.iter().find(|(l, _)| l == label).map(|(_, p)| *p)
}

fn show_aux(app: &AppHandle, label: &str) {
    reveal(app, label, true);
}

/// A window comes back where it was, and only takes the keyboard when the user
/// asked for it: the overlay following the game must not pull focus out of the
/// game it is following.
fn reveal(app: &AppHandle, label: &str, focus: bool) {
    let Some(w) = app.get_webview_window(label) else { return };
    let _ = w.show();
    // after the show: a position set on an unmapped window is advice the window
    // manager may ignore. Only somewhere a screen still reaches.
    if let Some(pos) = parked(label) {
        if on_a_monitor(app, pos) {
            let _ = w.set_position(pos);
        }
    }
    // Hiding a window unmaps it, and the state a window manager keeps for an
    // unmapped window is its own business — the position is already restored
    // above for the same reason. Asking again for the one thing an overlay
    // cannot do without costs nothing on a window that already has it.
    if label != "dashboard" {
        let _ = w.set_always_on_top(true);
    }
    if focus {
        let _ = w.set_focus();
    }
}

fn on_a_monitor(app: &AppHandle, pos: tauri::PhysicalPosition<i32>) -> bool {
    let monitors = app.available_monitors().unwrap_or_default();
    monitors.is_empty()
        || monitors.iter().any(|m| {
            let (p, s) = (m.position(), m.size());
            pos.x >= p.x - 50
                && pos.x < p.x + s.width as i32
                && pos.y >= p.y - 50
                && pos.y < p.y + s.height as i32
        })
}

fn hide_aux(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        // a window that is not on screen has no position worth keeping: an
        // unmapped one reports (0, 0), a minimised one reports the far corner
        if on_screen(&w) {
            if let Ok(pos) = w.outer_position() {
                park(label, pos);
            }
        }
        let _ = w.hide();
    }
}

/// Visible and not minimised — the only state whose geometry means anything.
fn on_screen(w: &tauri::WebviewWindow) -> bool {
    w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false)
}

/// The sniffer follows the game with these two. Showing the overlay must leave
/// the keyboard with the game.
pub(crate) fn show_overlay(app: &AppHandle) {
    reveal(app, "main", false);
}

pub(crate) fn hide_overlay(app: &AppHandle) {
    hide_aux(app, "main");
}

#[tauri::command]
fn hide_dashboard(app: AppHandle) {
    hide_aux(&app, "dashboard");
}

/// The two faces of the app: the dashboard to set things up and read the run,
/// the overlay to keep an eye on it while playing. Which one was up is
/// remembered, so the tray and the next launch bring back the same one.
///
/// Where the overlay cannot work there is only one face, and asking for the
/// other one brings the dashboard back instead of hiding everything.
fn set_face(app: &AppHandle, compact: bool) {
    let possible = overlay_supported();
    let shown = compact && possible;
    let (show, hide) = if shown { ("main", "dashboard") } else { ("dashboard", "main") };
    hide_aux(app, hide);
    show_aux(app, show);
    // What the user asked for is what is remembered. A session that cannot host
    // the overlay must not rewrite the preference of one that can — the same
    // settings file travels with a portable install and outlives a login.
    let mut settings = read_settings();
    if (possible || !compact) && settings.compact != compact {
        settings.compact = compact;
        let _ = save_settings(app.clone(), settings);
    }
}

#[tauri::command]
fn compact_mode(app: AppHandle) {
    set_face(&app, true);
}

#[tauri::command]
fn full_mode(app: AppHandle) {
    set_face(&app, false);
}

/// A filter travels as one file: the lists, the items and the sound each list
/// plays, inlined. Without the sounds an exported filter would arrive mute on
/// the other machine, which is half the point of sharing one.
#[derive(Serialize, Deserialize)]
struct ExportedSound {
    ext: String,
    data: String,
}

#[derive(Serialize, Deserialize)]
struct ExportedList {
    name: String,
    #[serde(default = "yes")]
    enabled: bool,
    #[serde(default = "default_volume")]
    volume: f32,
    #[serde(default)]
    items: Vec<String>,
    #[serde(default)]
    sound: Option<ExportedSound>,
}

fn yes() -> bool {
    true
}

fn default_volume() -> f32 {
    0.7
}

#[derive(Serialize, Deserialize)]
struct ExportedFilter {
    app: String,
    version: u32,
    name: String,
    lists: Vec<ExportedList>,
}

fn list_sound(id: &str) -> Option<ExportedSound> {
    SOUND_EXTS.iter().find_map(|(ext, _)| {
        let path = sounds_dir().join(format!("list-{id}.{ext}"));
        std::fs::read(&path).ok().map(|bytes| ExportedSound {
            ext: (*ext).to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    })
}

#[tauri::command]
fn export_filter(app: AppHandle, filter: SoundFilter) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let safe: String = filter.name.chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '-' }).collect();
    let suggested = format!("{safe}.hstracker.json");
    let picked = app
        .dialog()
        .file()
        .add_filter("HS Tracker filter", &["json"])
        .set_file_name(&suggested)
        .blocking_save_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let exported = ExportedFilter {
        app: "hs-tracker".into(),
        version: 1,
        name: filter.name,
        lists: filter
            .lists
            .into_iter()
            .map(|l| ExportedList {
                sound: list_sound(&l.id),
                name: l.name,
                enabled: l.enabled,
                volume: l.volume,
                items: l.items,
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&exported).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(Some(path.file_name().unwrap_or_default().to_string_lossy().into_owned()))
}

#[tauri::command]
fn import_filter(app: AppHandle) -> Result<Option<SoundFilter>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .add_filter("HS Tracker filter", &["json"])
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let exported: ExportedFilter = serde_json::from_str(&text).map_err(|_| "not an HS Tracker filter".to_string())?;
    if exported.app != "hs-tracker" {
        return Err("not an HS Tracker filter".into());
    }
    std::fs::create_dir_all(sounds_dir()).map_err(|e| e.to_string())?;
    let mut lists = Vec::new();
    for list in exported.lists {
        // ids are minted here, so an imported filter never fights with one
        // that is already installed
        let id = format!("{:x}", now_id());
        if let Some(sound) = list.sound {
            if let (true, Ok(bytes)) = (
                SOUND_EXTS.iter().any(|(e, _)| *e == sound.ext),
                base64::engine::general_purpose::STANDARD.decode(sound.data),
            ) {
                if bytes.len() <= 10 << 20 {
                    let _ = std::fs::write(sounds_dir().join(format!("list-{id}.{}", sound.ext)), bytes);
                }
            }
        }
        lists.push(SoundList {
            id,
            name: list.name,
            enabled: list.enabled,
            volume: list.volume.clamp(0.0, 1.0),
            items: list.items,
        });
    }
    Ok(Some(SoundFilter { id: format!("{:x}", now_id()), name: exported.name, lists }))
}

/// Short unique ids without pulling in a crate for it.
fn now_id() -> u64 {
    use std::sync::atomic::AtomicU64;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    nanos.wrapping_add(SEQ.fetch_add(1, Ordering::Relaxed)) & 0xffff_ffff
}

#[tauri::command]
fn ticker_busy(active: bool) {
    TICKER_BUSY.store(active, Ordering::Relaxed);
}

#[tauri::command]
fn get_shopping() -> Vec<String> {
    std::fs::read_to_string(shopping_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn set_shopping(items: Vec<String>) -> Result<(), String> {
    let items: Vec<String> = items.into_iter().filter(|s| !s.trim().is_empty()).take(200).collect();
    let json = serde_json::to_string_pretty(&items).map_err(|e| e.to_string())?;
    std::fs::write(shopping_path(), json).map_err(|e| e.to_string())
}

/// The clipboard handle is kept for the life of the process: on X11 the
/// copying application owns the selection, and dropping the handle hands the
/// text back to nobody unless a clipboard manager happens to be running.
#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    with_clipboard(|c| c.set_text(text))
}

/// One clipboard, opened once. Both the shopping list and the run card use it.
fn with_clipboard<T>(
    job: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
) -> Result<T, String> {
    static CLIPBOARD: std::sync::Mutex<Option<arboard::Clipboard>> = std::sync::Mutex::new(None);
    let mut guard = CLIPBOARD.lock().map_err(|_| "the clipboard is busy".to_string())?;
    if guard.is_none() {
        *guard = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
    }
    job(guard.as_mut().expect("just filled")).map_err(|e| e.to_string())
}

/// Put a picture on the clipboard, ready to be pasted into a chat.
///
/// The card is drawn in the window — that is where the fonts and the game's
/// sprites are — and arrives here as raw pixels, base64'd because the bridge
/// carries JSON and a megabyte of numbers spelled out is not that.
#[tauri::command]
fn copy_image(width: u32, height: u32, rgba: String) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(rgba)
        .map_err(|_| "the picture did not survive the trip".to_string())?;
    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || bytes.len() != w * h * 4 {
        return Err("the picture is not the size it says it is".into());
    }
    with_clipboard(|c| {
        c.set_image(arboard::ImageData { width: w, height: h, bytes: bytes.into() })
    })
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// Custom sound beside the exe: sounds\{satanic|heroic|angelic|mail}.{mp3,wav,ogg,flac}.
#[tauri::command]
fn load_sound(rarity: String) -> Option<String> {
    if !sound_key(&rarity) {
        return None;
    }
    for (ext, mime) in SOUND_EXTS {
        let path = sounds_dir().join(format!("{rarity}.{ext}"));
        if let Ok(bytes) = std::fs::read(&path) {
            let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
            return Some(format!("data:{mime};base64,{b64}"));
        }
    }
    None
}

/// Absolute path of the custom sound, for the asset protocol — streaming the
/// file beats shipping a multi-megabyte data URL through the IPC bridge.
#[tauri::command]
fn sound_path(rarity: String) -> Option<String> {
    if !sound_key(&rarity) {
        return None;
    }
    SOUND_EXTS
        .iter()
        .map(|(ext, _)| sounds_dir().join(format!("{rarity}.{ext}")))
        .find(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
fn sound_status(rarity: String) -> Option<String> {
    if !sound_key(&rarity) {
        return None;
    }
    SOUND_EXTS
        .iter()
        .map(|(ext, _)| format!("{rarity}.{ext}"))
        .find(|name| sounds_dir().join(name).exists())
}

/// Native picker + copy into sounds\; the webview's own file input is
/// unreliable in a frameless always-on-top window.
#[tauri::command]
fn pick_sound(app: AppHandle, rarity: String) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    if !sound_key(&rarity) {
        return Err("bad rarity".into());
    }
    let picked = app
        .dialog()
        .file()
        .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    if !SOUND_EXTS.iter().any(|(e, _)| *e == ext) {
        return Err("unsupported format (mp3/wav/ogg/flac)".into());
    }
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > 10 << 20 {
        return Err("file larger than 10 MB".into());
    }
    let dir = sounds_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (e, _) in SOUND_EXTS {
        let _ = std::fs::remove_file(dir.join(format!("{rarity}.{e}")));
    }
    let name = format!("{rarity}.{ext}");
    std::fs::copy(&path, dir.join(&name)).map_err(|e| e.to_string())?;
    let _ = app.emit("sounds-changed", &rarity);
    Ok(Some(name))
}

#[tauri::command]
fn clear_sound(app: AppHandle, rarity: String) -> Result<(), String> {
    if !sound_key(&rarity) {
        return Err("bad rarity".into());
    }
    for (e, _) in SOUND_EXTS {
        let _ = std::fs::remove_file(sounds_dir().join(format!("{rarity}.{e}")));
    }
    let _ = app.emit("sounds-changed", &rarity);
    Ok(())
}

/// Left-clicking the tray hides whatever is on screen, and brings back the
/// face that was up last — usually the overlay while playing.
fn toggle_window(app: &AppHandle) {
    let visible = |label: &str| {
        app.get_webview_window(label).and_then(|w| w.is_visible().ok()).unwrap_or(false)
    };
    if visible("main") || visible("dashboard") {
        hide_aux(app, "main");
        hide_aux(app, "dashboard");
    } else {
        let compact = read_settings().compact && overlay_supported();
        show_aux(app, if compact { "main" } else { "dashboard" });
    }
}

fn toggle_lock(app: &AppHandle) {
    let mut settings = read_settings();
    settings.locked = !settings.locked;
    let _ = save_settings(app.clone(), settings);
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    // the two overlay entries are greyed out where the session cannot host one
    let overlay = overlay_supported();
    let dashboard = MenuItem::with_id(app, "dashboard", "Dashboard", true, None::<&str>)?;
    let compact = MenuItem::with_id(app, "compact", "Compact overlay", overlay, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", "Lock / Unlock overlay", overlay, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause / Resume session", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset", "Reset stats", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&dashboard, &compact, &lock, &pause, &reset, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .tooltip("HS Tracker")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "dashboard" => full_mode(app.clone()),
            "compact" => compact_mode(app.clone()),
            "lock" => toggle_lock(app),
            "pause" => toggle_pause(app),
            "reset" => close_session(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, .. } = event {
                toggle_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Act on the backend chosen in Settings before a single window exists: the
/// toolkit picks its display server once, at startup, and cannot be talked out
/// of it afterwards.
#[cfg(not(windows))]
fn honour_backend_choice() {
    if std::env::var_os("HS_TRACKER_RELAUNCHED").is_some() {
        return; // this process is already the replacement
    }
    if !wayland_session() || forced_x11() || !read_settings().x11_backend {
        return;
    }
    // A run that never got as far as its windows leaves this behind. Finding it
    // means the last attempt to come up through XWayland died, so the choice is
    // dropped rather than repeated forever — one bad start, not a dead app.
    let breadcrumb = data_dir().join("x11-attempt");
    if breadcrumb.exists() {
        let _ = std::fs::remove_file(&breadcrumb);
        let mut settings = read_settings();
        settings.x11_backend = false;
        if let Ok(json) = serde_json::to_string_pretty(&settings) {
            let _ = std::fs::write(settings_path(), json);
        }
        eprintln!("the last start through XWayland failed; coming up on Wayland instead");
        return;
    }
    if !x11_reachable() {
        return; // no XWayland here at all
    }
    let _ = std::fs::write(&breadcrumb, "");
    let started = std::process::Command::new(
        std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok())
            .unwrap_or_default(),
    )
    .env("GDK_BACKEND", "x11")
    .env("HS_TRACKER_RELAUNCHED", "1")
    .spawn()
    .is_ok();
    if started {
        std::process::exit(0);
    }
    let _ = std::fs::remove_file(&breadcrumb);
}

#[cfg(windows)]
fn honour_backend_choice() {}

/// Keep WebKitGTK away from the renderer NVIDIA's driver cannot survive.
///
/// Since 2.40 WebKitGTK composites through a DMA-BUF renderer. On the
/// proprietary NVIDIA driver its web process segfaults inside
/// `libnvidia-eglcore` while tearing a GL context down — which from the outside
/// looks like the tray icon arriving and the window never following, with a
/// crash reporter naming `WebKitWebProcess`. Every GTK application in the same
/// position turns the renderer off, and the cost here is a little smoothness on
/// a panel that is mostly still pictures.
///
/// Only machines that actually carry the driver pay for it, and never one where
/// the user has already made the choice themselves. It has to be set before GTK
/// starts, which is why it lives at the top of `run` — and the process is still
/// single-threaded here, so setting it is safe.
#[cfg(not(windows))]
fn ease_webkit() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some() {
        return;
    }
    let nvidia = ["/dev/nvidiactl", "/sys/module/nvidia/version"]
        .iter()
        .any(|p| std::path::Path::new(p).exists());
    if nvidia {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

#[cfg(windows)]
fn ease_webkit() {}

pub fn run() {
    ease_webkit();
    honour_backend_choice();
    sniffer::prepare_capture();
    let hk_toggle: Shortcut = HK_TOGGLE.parse().unwrap();
    let hk_lock: Shortcut = HK_LOCK.parse().unwrap();
    let hk_reset: Shortcut = HK_RESET.parse().unwrap();
    let hk_pause: Shortcut = HK_PAUSE.parse().unwrap();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    if *shortcut == hk_toggle {
                        toggle_window(app);
                    } else if *shortcut == hk_lock {
                        toggle_lock(app);
                    } else if *shortcut == hk_reset {
                        close_session(app);
                    } else if *shortcut == hk_pause {
                        toggle_pause(app);
                    }
                })
                .build(),
        )
        .manage(Shared::default())
        .invoke_handler(tauri::generate_handler![
            snapshot,
            get_extra,
            reset_stats,
            set_paused,
            fit_overlay,
            flourish_done,
            place_flourish,
            hide_window,
            hide_dashboard,
            compact_mode,
            full_mode,
            session_info,
            restart_backend,
            viewing,
            export_filter,
            import_filter,
            ticker_busy,
            get_runs,
            clear_runs,
            get_shopping,
            set_shopping,
            copy_text,
            copy_image,
            quit,
            get_settings,
            save_settings,
            load_sound,
            sound_path,
            sound_status,
            pick_sound,
            clear_sound
        ])
        .setup(|app| {
            let overlay = overlay_supported();
            build_tray(app.handle())?;
            // hotkeys are the overlay's remote control, and the backend they
            // need is X11's; registering them under Wayland reports success and
            // then nothing ever fires
            if overlay {
                for hk in [HK_TOGGLE, HK_LOCK, HK_RESET, HK_PAUSE] {
                    if let Err(e) = app.global_shortcut().register(hk) {
                        eprintln!("hotkey {hk} not registered: {e}");
                    }
                }
            } else {
                eprintln!("wayland session: running as the dashboard, without the overlay");
            }
            let settings = read_settings();
            app.state::<Shared>().stats.lock().unwrap().restore(&read_carried());
            apply_stats_settings(app.handle(), &settings);
            apply_settings_effects(app.handle(), &settings);
            restore_window_positions(app.handle());
            if settings.compact && overlay {
                hide_aux(app.handle(), "dashboard");
                show_aux(app.handle(), "main");
            }
            // click-through is set once the window exists on screen: off
            // Windows the call reaches into a native window that an unshown
            // one does not have yet. The ticker gets it when it is shown, the
            // overlay from the lock poller.
            if let Some(t) = app.get_webview_window("ticker") {
                if t.is_visible().unwrap_or(false) {
                    let _ = t.set_ignore_cursor_events(true);
                }
            }
            #[cfg(debug_assertions)]
            if let Some(w) = app.get_webview_window("main") {
                w.open_devtools();
            }
            // both of these only ever move or mask the overlay and the ticker
            if overlay {
                spawn_lock_poller(app.handle().clone());
                spawn_ticker_glue(app.handle().clone());
            }
            spawn_position_saver(app.handle().clone());
            spawn_stats_pusher(app.handle().clone());
            presence::spawn(app.handle().clone());
            sniffer::spawn(app.state::<Shared>().inner(), app.handle().clone());
            // the windows are up: whatever backend this is, it worked
            #[cfg(not(windows))]
            let _ = std::fs::remove_file(data_dir().join("x11-attempt"));
            Ok(())
        })
        .on_window_event(|window, event| {
            // The close button on a frame the window manager draws would destroy
            // the window, and a destroyed dashboard cannot be brought back from
            // the tray — on a Wayland session it is the only face there is.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle().clone();
                let label = window.label().to_string();
                hide_aux(&app, &label);
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            save_window_positions(app);
            save_carried(app);
            // quitting mid-run still files it
            end_run(app);
        }
    });
}
