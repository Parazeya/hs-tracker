mod items;
mod parser;
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
const SOUND_EXTS: [(&str, &str); 4] = [
    ("mp3", "audio/mpeg"),
    ("wav", "audio/wav"),
    ("ogg", "audio/ogg"),
    ("flac", "audio/flac"),
];

// base overlay size at scale 1.0; keep in sync with App.svelte layout
// (panel chrome 40px, each row 27px, 6px between rows)
const BASE_W: f64 = 444.0;
const OVERLAY_ROWS: [&str; 5] = ["session", "gold", "xp", "items", "zone"];

fn overlay_height(settings: &Settings) -> f64 {
    let rows = OVERLAY_ROWS.iter().filter(|r| !settings.hidden.iter().any(|h| h == *r)).count();
    34.0 + 33.0 * rows.max(1) as f64
}

const HK_TOGGLE: &str = "ctrl+shift+o";
const HK_LOCK: &str = "ctrl+shift+l";
const HK_RESET: &str = "ctrl+shift+r";

// lock-button rect in overlay CSS px, with a small margin (see App.svelte .lock)
const LOCK_RECT: (f64, f64, f64, f64) = (BASE_W - 32.0, 0.0, BASE_W, 34.0);

static LOCKED: AtomicBool = AtomicBool::new(false);
static TICKER: AtomicBool = AtomicBool::new(true);
/// The ticker is a transparent window pinned over the game: while it is on
/// screen the compositor keeps blending it, empty or not. It is only shown
/// while an entry is actually visible.
static TICKER_BUSY: AtomicBool = AtomicBool::new(false);
static SCALE_MILLI: AtomicU32 = AtomicU32::new(1000);

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
    pub locked: bool,
    pub opacity: f32,
    pub scale: f32,
    pub auto_show: bool,
    pub autostart: bool,
    pub ticker: bool,
    pub debug_log: bool,
    pub sound_on_ground: bool,
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
            locked: false,
            opacity: 1.0,
            scale: 1.0,
            auto_show: true,
            autostart: false,
            ticker: true,
            debug_log: false,
            sound_on_ground: true,
            hidden: Vec::new(),
        }
    }
}

static DEBUG_LOG: AtomicBool = AtomicBool::new(false);

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
            .open(exe_dir().join("debug-capture.jsonl"));
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

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default()
}

fn sounds_dir() -> PathBuf {
    exe_dir().join("sounds")
}

fn settings_path() -> PathBuf {
    exe_dir().join("settings.json")
}

fn shopping_path() -> PathBuf {
    exe_dir().join("shopping.json")
}

fn positions_path() -> PathBuf {
    exe_dir().join("positions.json")
}

fn carried_path() -> PathBuf {
    exe_dir().join("carried.json")
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

const REMEMBERED_WINDOWS: [&str; 4] = ["main", "settings", "stats", "shop"];

fn window_positions(app: &AppHandle) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    for label in REMEMBERED_WINDOWS {
        if let Some(w) = app.get_webview_window(label) {
            if let Ok(pos) = w.outer_position() {
                map.insert(label.into(), serde_json::json!([pos.x, pos.y]));
            }
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
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
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
/// twice a second each — the statistics window even asked for the whole graph
/// series and drop journal while hidden. Now one thread coalesces changes and
/// emits only to windows that are actually on screen. The heartbeats keep the
/// per-hour rates fresh while nothing is dropping.
const SNAP_MIN_GAP: Duration = Duration::from_millis(400);
const SNAP_HEARTBEAT: Duration = Duration::from_millis(2000);
const EXTRA_MIN_GAP: Duration = Duration::from_millis(1000);
const EXTRA_HEARTBEAT: Duration = Duration::from_millis(5000);

fn spawn_stats_pusher(app: AppHandle) {
    std::thread::spawn(move || {
        let visible = |label: &str| {
            app.get_webview_window(label)
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(false)
        };
        let (mut snap_rev, mut extra_rev) = (u64::MAX, u64::MAX);
        let mut snap_at = Instant::now() - SNAP_HEARTBEAT;
        let mut extra_at = Instant::now() - EXTRA_HEARTBEAT;
        let (mut had_main, mut had_stats) = (false, false);
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let (main, stats_win) = (visible("main"), visible("stats"));
            // a window that just appeared gets the current numbers at once
            if main && !had_main {
                snap_rev = u64::MAX;
            }
            if stats_win && !had_stats {
                (snap_rev, extra_rev) = (u64::MAX, u64::MAX);
            }
            (had_main, had_stats) = (main, stats_win);
            if !main && !stats_win {
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
                for (label, on_screen) in [("main", main), ("stats", stats_win)] {
                    if on_screen {
                        let _ = app.emit_to(label, "stats", &snapshot);
                    }
                }
                (snap_rev, snap_at) = (revision, Instant::now());
            }
            // the series and the drop journal are the heaviest payload in the
            // app, so they only travel while the window showing them is open
            if stats_win && due(extra_rev, extra_at, EXTRA_MIN_GAP, EXTRA_HEARTBEAT) {
                let extra = shared.stats.lock().unwrap().extra();
                let _ = app.emit_to("stats", "stats-extra", &extra);
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
    settings
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
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_ignore_cursor_events(settings.locked);
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

fn close_session(app: &AppHandle) {
    app.state::<Shared>().stats.lock().unwrap().reset();
}

#[tauri::command]
fn reset_stats(app: AppHandle) {
    close_session(&app);
}

#[tauri::command]
fn hide_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

fn show_aux(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn hide_aux(app: &AppHandle, label: &str) {
    if let Some(w) = app.get_webview_window(label) {
        let _ = w.hide();
    }
}

#[tauri::command]
fn open_settings(app: AppHandle) {
    show_aux(&app, "settings");
}

#[tauri::command]
fn hide_settings(app: AppHandle) {
    hide_aux(&app, "settings");
}

#[tauri::command]
fn open_stats(app: AppHandle) {
    show_aux(&app, "stats");
}

#[tauri::command]
fn hide_stats(app: AppHandle) {
    hide_aux(&app, "stats");
}

#[tauri::command]
fn open_shop(app: AppHandle) {
    show_aux(&app, "shop");
}

#[tauri::command]
fn hide_shop(app: AppHandle) {
    hide_aux(&app, "shop");
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

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// Custom sound beside the exe: sounds\{satanic|heroic|angelic|mail}.{mp3,wav,ogg,flac}.
#[tauri::command]
fn load_sound(rarity: String) -> Option<String> {
    if !SOUND_KEYS.contains(&rarity.as_str()) {
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
    if !SOUND_KEYS.contains(&rarity.as_str()) {
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
    if !SOUND_KEYS.contains(&rarity.as_str()) {
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
    if !SOUND_KEYS.contains(&rarity.as_str()) {
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
    if !SOUND_KEYS.contains(&rarity.as_str()) {
        return Err("bad rarity".into());
    }
    for (e, _) in SOUND_EXTS {
        let _ = std::fs::remove_file(sounds_dir().join(format!("{rarity}.{e}")));
    }
    let _ = app.emit("sounds-changed", &rarity);
    Ok(())
}

fn toggle_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
}

fn toggle_lock(app: &AppHandle) {
    let mut settings = read_settings();
    settings.locked = !settings.locked;
    let _ = save_settings(app.clone(), settings);
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Show / Hide", true, None::<&str>)?;
    let lock = MenuItem::with_id(app, "lock", "Lock / Unlock overlay", true, None::<&str>)?;
    let statistics = MenuItem::with_id(app, "statistics", "Statistics", true, None::<&str>)?;
    let shopping = MenuItem::with_id(app, "shopping", "Shopping List", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset", "Reset stats", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &lock, &statistics, &shopping, &settings, &reset, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .tooltip("HS Tracker")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "toggle" => toggle_window(app),
            "lock" => toggle_lock(app),
            "statistics" => show_aux(app, "stats"),
            "shopping" => show_aux(app, "shop"),
            "settings" => show_aux(app, "settings"),
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

pub fn run() {
    sniffer::add_npcap_to_path();
    let hk_toggle: Shortcut = HK_TOGGLE.parse().unwrap();
    let hk_lock: Shortcut = HK_LOCK.parse().unwrap();
    let hk_reset: Shortcut = HK_RESET.parse().unwrap();
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
                    }
                })
                .build(),
        )
        .manage(Shared::default())
        .invoke_handler(tauri::generate_handler![
            snapshot,
            get_extra,
            reset_stats,
            hide_window,
            open_settings,
            hide_settings,
            open_stats,
            hide_stats,
            open_shop,
            hide_shop,
            ticker_busy,
            get_shopping,
            set_shopping,
            copy_text,
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
            build_tray(app.handle())?;
            for hk in [HK_TOGGLE, HK_LOCK, HK_RESET] {
                if let Err(e) = app.global_shortcut().register(hk) {
                    eprintln!("hotkey {hk} not registered: {e}");
                }
            }
            let settings = read_settings();
            app.state::<Shared>().stats.lock().unwrap().restore(&read_carried());
            apply_stats_settings(app.handle(), &settings);
            apply_settings_effects(app.handle(), &settings);
            restore_window_positions(app.handle());
            if let Some(t) = app.get_webview_window("ticker") {
                let _ = t.set_ignore_cursor_events(true);
            }
            #[cfg(debug_assertions)]
            if let Some(w) = app.get_webview_window("main") {
                w.open_devtools();
            }
            spawn_lock_poller(app.handle().clone());
            spawn_ticker_glue(app.handle().clone());
            spawn_position_saver(app.handle().clone());
            spawn_stats_pusher(app.handle().clone());
            sniffer::spawn(app.state::<Shared>().inner(), app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            save_window_positions(app);
            save_carried(app);
        }
    });
}
