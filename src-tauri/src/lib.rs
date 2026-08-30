mod items;
mod log;
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
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Alert kinds that own a configurable sound (not item rarities — see stats).
const SOUND_KEYS: [&str; 8] =
    ["satanic", "set", "heroic", "angelic", "unholy", "mail", "zone", "relic"];

/// A sound is either one of the built-in alerts or a list's own file,
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
/// The panel's own width, which is the art's — what it is drawn at before
/// anything has measured it.
const PANEL_W: f64 = 444.0;

/// What it actually came out as, once the page has been laid out.
///
/// Reported as "Squished Panel": the chips are fixed widths with `nowrap`
/// inside a fixed 444, so on a machine where the text comes out wider — a
/// substituted font, a system text size, a webview minimum font size — it has
/// nowhere to go and spills over the row. Nothing in Settings could help,
/// because none of those move a CSS pixel.
///
/// The height has been measured from the panel and handed back since the row
/// toggles went in. This is the same arrangement for the other axis.
static PANEL_WIDTH: AtomicU32 = AtomicU32::new(0);

/// The control strip beside it: the lock and four plates, each the height of a
/// chip so the column keeps the panel's own rhythm, and flush against the frame
/// rather than floating clear of it.
///
/// Twice the plates' native 21 would be exactly crisp and is too big — it
/// dwarfed the panel it belongs to. At 28 one source pixel in three is doubled,
/// which at this size costs less than a column nobody wants to look at.
const STRIP_W: f64 = 28.0;
const STRIP_GAP: f64 = 0.0;
/// The window, which is wider than the panel so the strip can sit OUTSIDE it
/// rather than on its frame. A webview cannot paint past its own window, so
/// "beside the panel" and "inside the window" are the same requirement.
const BASE_W: f64 = PANEL_W + STRIP_GAP + STRIP_W;

fn panel_w() -> f64 {
    match PANEL_WIDTH.load(Ordering::Relaxed) {
        0 => PANEL_W,
        w => w as f64,
    }
}

fn base_w() -> f64 {
    panel_w() + STRIP_GAP + STRIP_W
}

/// Take a measurement of the panel, never narrower than it was drawn for.
///
/// A page that has not finished laying out reports a small width for a frame —
/// zero, on the very first — and a window that shrank to it would clip the panel
/// it exists to hold. The ceiling is there for the same reason in reverse:
/// nothing the panel legitimately contains is 1600 CSS px wide.
fn remember_width(w: f64) {
    if !w.is_finite() {
        return;
    }
    let w = w.clamp(PANEL_W, 1600.0);
    PANEL_WIDTH.store(w.round() as u32, Ordering::Relaxed);
}

/// The icon strip's rect, in overlay CSS px, and what it takes to summon it.
///
/// The strip stands beside the panel rather than on it — see .strip in
/// App.svelte — so this is simply everything past the panel's right edge. It
/// used to be two constants reading `x 444..472`; the panel can be wider than
/// 444 now, so the edge is asked for rather than remembered.
///
/// It stood for the lock alone, and twice it stood in the wrong place. It once
/// reached x 412 and y 34, wider and taller than the button, and the corner it
/// left over lay on top of the Reset Stats button below — a locked overlay is
/// click-through everywhere but here, so that corner of the button quietly
/// belonged to the lock. Trimming it to a 24x24 corner then missed the button
/// the other way, because `.lock` was laid out against the panel's PADDING box
/// and sat at x 415..436, not 420..444. A strip fixed to the window has one
/// origin and one set of numbers, which is most of why it is one.
///
/// `held` is whether the strip is already open. Click-through is a whole-window
/// switch — `set_ignore_cursor_events` is one boolean and there is no partial
/// input region in this stack — so every pixel the poller watches is a pixel
/// where the overlay stops passing clicks to the game beneath it. Watching the
/// whole column all the time would mean brushing the right-hand edge of the
/// panel costs the player a click in a fight.
///
/// So the column is entered through the corner the lock has always occupied,
/// and only then does the rest of it start being watched. Reaching for the
/// buttons is deliberate; crossing the edge on the way somewhere else is not.
/// One cell and the gap under it — 31, not the 62 this was first given, which
/// reached over the Dashboard button as well and made a one-click action live
/// before the strip that carries it had appeared.
fn strip_rect(held: bool) -> (f64, f64, f64, f64) {
    let end = if held { STRIP_H } else { STRIP_W + 3.0 };
    (panel_w(), 0.0, base_w(), end)
}

/// The rows that add height to the overlay. "vitals" is not one of them and has
/// no Settings entry any more: magic find had moved into the session row, and
/// then the readout itself came out because the heartbeat that carried it went
/// tens of minutes between packets. A stored `hidden: ["vitals"]` from an older
/// version hides nothing and costs nothing, which is why it is not migrated
/// away.
const OVERLAY_ROWS: [&str; 5] = ["session", "gold", "xp", "items", "zone"];

fn overlay_height(settings: &Settings) -> f64 {
    // what the overlay says it is, and only otherwise what its rows suggest
    let measured = PANEL_H.load(Ordering::Relaxed);
    let panel = if measured > 0 {
        measured as f64
    } else {
        let rows = OVERLAY_ROWS.iter().filter(|r| !settings.hidden.iter().any(|h| h == *r)).count();
        34.0 + 33.0 * rows.max(1) as f64
    };
    // The strip stands beside the panel and can be the taller of the two — a
    // window cut to the panel would clip the last buttons off the bottom. The
    // extra is transparent, so a tall strip costs a little dead space beside a
    // short panel and nothing at all beside a full one.
    panel.max(STRIP_H)
}

const HK_TOGGLE: &str = "ctrl+shift+o";
const HK_LOCK: &str = "ctrl+shift+l";
const HK_RESET: &str = "ctrl+shift+r";
const HK_PAUSE: &str = "ctrl+shift+p";


/// Five cells, 1px between them and 3 more under the lock — see .strip in
/// App.svelte, which is the same arithmetic in the other language.
const STRIP_H: f64 = 5.0 * STRIP_W + 4.0 + 3.0;

static LOCKED: AtomicBool = AtomicBool::new(false);
/// Whether there is a tray icon to hide into. Assumed until proved otherwise:
/// on Linux the indicator library can be missing outright, and a window that
/// hides itself into a tray nobody can see is a window nobody gets back.
static TRAY_OK: AtomicBool = AtomicBool::new(true);
/// Set once any window has told us it painted; see `ui_ready`.
static UI_READY: AtomicBool = AtomicBool::new(false);
/// Set once the event loop is turning. Until then this thread IS the startup,
/// and a window built on it is built the ordinary way; after it, this thread is
/// the event loop, and see `ensure_flourish` for why that matters.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// One builder at a time. Two commands in the same instant both find no window
/// and both ask for one; the label is the same, so the second build fails and
/// logs an error about a window that is in fact fine.
static BUILDING_FLOURISH: AtomicBool = AtomicBool::new(false);
static TICKER: AtomicBool = AtomicBool::new(true);
/// The ticker is a transparent window pinned over the game: while it is on
/// screen the compositor keeps blending it, empty or not. It is only shown
/// while an entry is actually visible.
static TICKER_BUSY: AtomicBool = AtomicBool::new(false);
/// Whether the flourish is on at all. Which drops deserve one is the engine's
/// question, and it is asked there — see `set_flourish_filter`.
static FLOURISH: AtomicBool = AtomicBool::new(false);
/// leave the announcement window up so a capture has something to hold on to
static FLOURISH_ALWAYS: AtomicBool = AtomicBool::new(false);
/// Whether a rotation gets the pillar. Read on the pusher's thread, which has
/// no settings of its own.
static FLOURISH_ZONE: AtomicBool = AtomicBool::new(true);
static SCALE_MILLI: AtomicU32 = AtomicU32::new(1000);
/// The panel's own height in CSS pixels, as the overlay last measured it. Zero
/// until the first frame has been drawn, when the guess below stands in.
static PANEL_H: AtomicU32 = AtomicU32::new(0);

/// Frameless-when-locked: on by default only where the compositor clears the
/// window between frames. See `Settings::ghost`.
fn ghost_default() -> bool {
    cfg!(windows)
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SoundCfg {
    pub enabled: bool,
    pub volume: f32,
}

impl Default for SoundCfg {
    fn default() -> Self {
        Self { enabled: true, volume: 0.5 }
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
    /// Whole categories the list holds — "every Satanic helmet" — as a rule
    /// rather than as the 36 names it stands for today. See `stats::Rule` for
    /// why it is a rule, and `SoundRule` below for what None means.
    ///
    /// No `serde(default)` of its own: the struct already carries one, so a
    /// settings file written before the category picker existed loads with an
    /// empty vector and every list in it goes on meaning exactly what it meant.
    pub rules: Vec<SoundRule>,
}

impl Default for SoundList {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: true,
            volume: 0.5,
            items: Vec::new(),
            rules: Vec::new(),
        }
    }
}

/// A category on a list, as the settings file spells it.
///
/// `null` is "any" on every field, which is why they are Options and not
/// sentinel numbers: a missing `item_type` defaulting to 0 would silently mean
/// Helmet. `weapon` only says anything alongside `item_type` 3, the wire's one
/// type for every weapon there is.
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct SoundRule {
    pub rarity: Option<String>,
    pub item_type: Option<i64>,
    pub weapon: Option<i64>,
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
    /// The satanic zone rotating: its chime, its volume, and — because the two
    /// are one decision — whether the overlay's zone chip pulses with it.
    pub zone: SoundCfg,
    /// Which of the satanic zone's buffs are worth the alert. Empty is every
    /// rotation, not none of them: the list narrows the alert, so a list that
    /// narrows nothing lets everything through. No `serde(default)` of its own
    /// — the struct already carries one, and a second would outlive a change to
    /// `Default` and quietly keep handing old files a list nobody chose.
    pub zone_buffs: Vec<u8>,
    /// The hunted-relic chime: whether it sounds, and how loud. A relic is not
    /// a rarity, so it could not borrow one of the five — every relic in the
    /// game is Common.
    pub relic: SoundCfg,
    /// Which relics are worth the alert, by id-in-type. Empty is NONE, which is
    /// the opposite of `zone_buffs` right above and the one thing about this
    /// pair that has to be said out loud wherever either is shown: that list
    /// narrows an alert the game already makes, so narrowing by nothing lets
    /// everything through; this list IS the alert, so an empty one is silence.
    pub relics: Vec<u16>,
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
    /// Read every connection on the machine, not only the game's own.
    ///
    /// Off, the capture is filtered down to the addresses the operating system
    /// says the game is talking to, which is cheaper and narrower. There are
    /// setups where those are not the addresses on the wire — a route optimiser
    /// such as ExitLag redirects the game's packets in a driver below the TCP
    /// stack — and then the filter contains nothing that ever appears and the
    /// app counts nothing at all. This is the way out of that, and it is a
    /// setting rather than a silent fallback because reading everything is the
    /// player's decision to make about their own machine.
    pub wide_capture: bool,
    pub sound_on_ground: bool,
    /// stop the session clock when nothing has happened for a while, so a break
    /// does not quietly halve every per-hour figure
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
    /// Announce whatever the custom filter's lists match, whatever its rarity
    /// or grade. A list is already a statement that those items matter; saying
    /// it again in the announcement's own switches is how a filter comes to
    /// look as though it does nothing.
    pub flourish_listed: bool,
    /// Announce a rotation with the pillar as well as the chime. Its own
    /// switch, and not one the chime can veto: the player with the game's audio
    /// up wants to be shown, not told.
    pub flourish_zone: bool,
    /// Keep the announcement window on screen between drops, drawing nothing.
    /// OBS can only capture a window that exists, and this one otherwise
    /// appears for a few seconds and is gone again. Off by default: a window
    /// held open is a window the compositor keeps blending, empty or not.
    pub flourish_always: bool,
    /// show the run in Discord while the game is open. Off unless asked for:
    /// it puts what the player is doing in front of everyone on their list.
    pub discord: bool,
    /// which face was up last: the overlay (true) or the dashboard
    pub compact: bool,
    /// Whether a locked overlay drops its frame and lets the numbers float over
    /// the game. It needs the window to clear itself between frames, which
    /// WebKitGTK on a transparent X11 window does not do — so it is off by
    /// default where that is the case, and offered as a switch rather than
    /// taken away: a still overlay smears far less than a busy one, and the
    /// look is worth having for anyone who finds it acceptable.
    #[serde(default = "ghost_default")]
    pub ghost: bool,
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
            // On out of the box: the zone moving is the one thing on this panel
            // the player is meant to act on, and it happens while they are
            // looking at the fight rather than at us.
            zone: SoundCfg::default(),
            // every rotation, until the player narrows it
            zone_buffs: Vec::new(),
            // Off out of the box, unlike `zone` above, and for the reason the
            // field says: an empty relic list is silence either way, so the
            // switch has nothing to announce until a relic is ticked.
            relic: SoundCfg::default(),
            relics: Vec::new(),
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
            wide_capture: false,
            sound_on_ground: true,
            theme: "default".into(),
            // On out of the box. Off, with the narrowest band it has, it
            // announced nothing at all — which reads as a broken feature
            // rather than an unset one, and cost a bug report saying so.
            flourish: true,
            flourish_scale: 1.0,
            flourish_shade: 0.55,
            flourish_secs: 6.0,
            flourish_rarities: ["Satanic", "Set", "Heroic", "Angelic", "Unholy"]
                .iter()
                .map(|r| r.to_string())
                .collect(),
            // grade 1 is D, which this slider reads as "any"
            flourish_tier: 1,
            flourish_listed: false,
            // The rotation is the one thing on this panel the player is meant
            // to act on, and it is rare enough that a pillar for it is not a
            // pillar in the way. On, for the same reason the announcement is.
            flourish_zone: true,
            flourish_always: false,
            discord: false,
            compact: false,
            ghost: ghost_default(),
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
    /// there is a tray icon, so closing a window can mean hiding it
    tray: bool,
}

#[tauri::command]
fn session_info() -> SessionInfo {
    #[cfg(windows)]
    {
        SessionInfo {
            overlay: true,
            wayland: false,
            through_x11: false,
            can_switch: false,
            tray: TRAY_OK.load(Ordering::Relaxed),
        }
    }
    #[cfg(not(windows))]
    {
        let wayland = wayland_session();
        SessionInfo {
            overlay: overlay_supported(),
            wayland,
            through_x11: forced_x11(),
            can_switch: wayland && x11_reachable(),
            tray: TRAY_OK.load(Ordering::Relaxed),
        }
    }
}

/// Restart into the other display backend.
///
/// A Wayland session gives an application no overlay, but XWayland does — and
/// the game itself runs through XWayland when it runs through Proton, so the
/// two end up in the same X server where one can sit above the other. Rather
/// than teach the user about `GDK_BACKEND`, the app relaunches itself.
#[tauri::command(async)]
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
        // Hand the single-instance name over before spawning. The parent still
        // holds it until it exits, and the child's own guard — registered
        // first, so it runs before any window exists — sees the name taken and
        // quietly exits: the button appeared to do nothing at all. Both
        // directions took this path, into XWayland and back out of it.
        //
        // The cost, stated plainly: `destroy` does not re-acquire. If the
        // relaunch below fails, this process carries on with no guard, and a
        // second copy could be started until it is restarted.
        tauri_plugin_single_instance::destroy(&app);
        // A breadcrumb before the spawn, not after: if the replacement never
        // paints, the next start finds this and drops the choice rather than
        // repeating it forever. `ui_ready` clears it once a page is up.
        //
        // Only going *to* X11. Its one reader treats it as "the last XWayland
        // start died", so leaving it behind on the way back to Wayland would
        // have the next cold start discard a choice nobody made wrongly.
        let breadcrumb = data_dir().join("x11-attempt");
        if x11 {
            let _ = std::fs::write(&breadcrumb, "");
        }
        // Spawn first: the choice is only worth remembering once a replacement
        // is actually on its way. Written before, a launch that fails leaves an
        // app that relaunches into the same failure at every start, with no
        // window left to undo it in.
        if let Err(e) = relaunch(x11) {
            // Nothing was replaced, so undo both preparations: the breadcrumb
            // would report a failure that never happened, and this process is
            // now running with no single-instance name and nothing to take it
            // back — a second copy started from here on would be a second
            // sniffer writing over the same files.
            let _ = std::fs::remove_file(&breadcrumb);
            log::error(format!("could not restart into the other backend: {e}"));
            return Err(format!("{e} — restart HS Tracker by hand"));
        }
        let mut settings = read_settings();
        settings.x11_backend = x11;
        save_settings(app.clone(), settings)?;
        app.exit(0);
        Ok(())
    }
}

/// A start that hands over to a replacement never meant to draw anything, so it
/// must not leave the mark that says it tried and failed.
///
/// `ease_webkit` writes `no-paint` on the way up and `ui_ready` clears it once a
/// frame exists. A process that relaunches itself does neither: it exits in
/// between, mark still on disk, and the replacement reads it as "the run before
/// me drew nothing" — turns the DMA-BUF renderer off for good and writes a
/// reason that is not true. Choosing the X11 backend on a Wayland session does
/// exactly that, on the very first start after the box is ticked.
#[cfg(not(windows))]
fn handing_over() {
    let _ = std::fs::remove_file(data_dir().join("no-paint"));
}

/// Start a fresh copy of ourselves on the chosen backend.
#[cfg(not(windows))]
fn relaunch(x11: bool) -> Result<(), String> {
    handing_over();
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
    // The file stays open: with the wide capture this runs many times a second.
    // The count beside it is what has been written since it was opened, which is
    // how the roll below knows without asking the filesystem every line.
    static FILE: std::sync::Mutex<Option<(std::io::BufWriter<std::fs::File>, u64)>> =
        std::sync::Mutex::new(None);
    let Ok(mut guard) = FILE.lock() else { return };
    let path = data_dir().join("debug-capture.jsonl");

    // Rolled, because this had no ceiling at all. It is off by default and a
    // player switches it on to catch one thing — and then leaves it on, and with
    // the wide capture it is no longer only the game's traffic being written
    // down. A session is about 25 MB; two of these is as much of a packet log as
    // anyone is going to read, and it stops a debugging aid from quietly filling
    // a disk.
    const CAPTURE_KEEP: u64 = 64 * 1024 * 1024;
    if guard.as_ref().is_some_and(|(_, written)| *written >= CAPTURE_KEEP) {
        if let Some((mut f, _)) = guard.take() {
            let _ = f.flush();
        }
        let _ = std::fs::rename(&path, path.with_extension("old.jsonl"));
    }

    if guard.is_none() {
        let opened = std::fs::OpenOptions::new().create(true).append(true).open(&path);
        let Ok(f) = opened else { return };
        let had = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        *guard = Some((std::io::BufWriter::new(f), had));
    }
    let Some((f, written)) = guard.as_mut() else { return };
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
            *written += line.len() as u64 + 1;
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
            parser::GameEvent::ItemsLetGo(gone) => format!("gone  {} out of the bags", gone.len()),
            parser::GameEvent::WhoseAccount(id) => format!("whom  account {id}"),
            parser::GameEvent::Mail(has) => format!("mail  {has}"),
            parser::GameEvent::Room(room) => format!("room  {room}"),
            parser::GameEvent::ZoneRegion(id) => format!("asks  zone, for region {id}"),
            parser::GameEvent::Vitals { mf, level, hlevel, satanic_here } => {
                let say = |v: &Option<i64>| v.map_or("-".into(), |n| n.to_string());
                let sz = satanic_here.map_or("-".into(), |b| b.to_string());
                format!("vitals  mf {}  lv {level}  hlv {hlevel}  sz {sz}", say(mf))
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
            parser::GameEvent::Found { finder, name } => format!("chat  {finder:?} found {name:?}"),
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
        // The last resort used to be ".", and a process started by the
        // session has "/" for a working directory: every write then failed
        // into a discarded `let _ =` while About cheerfully reported a path
        // that had never been created. The temp directory is a poor home, but
        // it is a real one, and it says so.
        let home = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
        let scratch = home.is_none();
        let dir = home.unwrap_or_else(std::env::temp_dir).join("hs-tracker");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("cannot create {}: {e}", dir.display());
        } else if scratch {
            eprintln!("no HOME: settings and runs are going to {}", dir.display());
        }
        dir
    })
    .clone()
}

fn sounds_dir() -> PathBuf {
    data_dir().join("sounds")
}

/// Write a file so that a reader never sees half of one.
///
/// `fs::write` truncates in place: a crash, a power cut or a full disk between
/// the truncate and the last byte leaves a file that parses as nothing, and
/// every reader here falls back to a default. For runs.json that loss is made
/// permanent by the next `end_run`, which reads the wreck, gets an empty list
/// and writes a one-run history over two hundred.
///
/// The staging name carries the process id and a counter rather than a plain
/// `.tmp`: settings.json has genuinely concurrent writers — the panel, the
/// tray, the hotkey thread — and a shared staging file would turn a power-cut
/// corruption into one reachable on an ordinary afternoon.
fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::sync::atomic::AtomicU64;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let stem = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let staged = path.with_file_name(format!(
        "{stem}.{}.{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    // The rename is atomic, but only over what the disk actually holds: without
    // this the metadata operation can land while the bytes are still in the
    // cache, and a power cut leaves the new name pointing at an empty file —
    // the one outcome the staging was meant to prevent. Saves are debounced at
    // 150 ms, so this costs a flush a few times a second at worst.
    {
        let mut f = std::fs::File::create(&staged)?;
        std::io::Write::write_all(&mut f, bytes)?;
        f.sync_data()?;
    }
    // Windows hands out sharing violations when an antivirus or the indexer
    // has the target open for a moment; a couple of retries covers it.
    let mut last = None;
    for attempt in 0..3 {
        match std::fs::rename(&staged, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                // Not after the last one: there is nothing left to wait for, and
                // the 60 ms it used to sleep there was spent on the way to
                // reporting the failure anyway.
                if attempt < 2 {
                    std::thread::sleep(Duration::from_millis(20 * (attempt + 1)));
                }
            }
        }
    }
    let _ = std::fs::remove_file(&staged);
    Err(last.unwrap_or_else(|| std::io::Error::other("rename failed")))
}

/// The same, for anything that serialises — and it says so when it cannot.
///
/// Four of these writers used to discard their error entirely, so a read-only
/// folder or a full disk looked exactly like a successful save until the next
/// start, when the setting was simply gone.
/// `pretty` is for the files a person opens and edits — settings above all.
/// runs.json holds two hundred runs and is machine-only; pretty-printing it
/// tripled a megabyte for nobody's benefit. The doc said as much while the code
/// had stopped making the distinction, so settings.json went out on one line.
fn write_json<T: Serialize>(path: &std::path::Path, value: &T, pretty: bool) {
    let encode = if pretty { serde_json::to_vec_pretty } else { serde_json::to_vec };
    let json = match encode(value) {
        Ok(j) => j,
        Err(e) => return log::error(format!("cannot encode {}: {e}", path.display())),
    };
    if let Err(e) = write_atomic(path, &json) {
        log::once(&path.display().to_string(), "error", format!("cannot write {}: {e}", path.display()));
    }
}

/// Read a file of ours, and keep the wreck when it will not parse.
///
/// A file that is not there is a first run and answers with defaults. A file
/// that IS there and does not parse used to answer with defaults too, with no
/// log line and nothing kept — and the callers that read-modify-write commit
/// that answer straight back: the lock hotkey and the tray item rewrite the
/// whole of settings.json, and `end_run` writes a one-run history over two
/// hundred. One unparseable file therefore cost every filter, every list and
/// every notable group, on the next keypress, silently. Moving it aside first
/// leaves the user something to recover and the log something to say.
///
/// Only the parse failure moves the file. An io error is the folder being
/// locked or unreadable, and renaming is the last thing that helps there.
fn read_json_or_default<T: serde::de::DeserializeOwned + Default>(path: &std::path::Path) -> T {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::once(
                    &path.display().to_string(),
                    "error",
                    format!("cannot read {}: {e}", path.display()),
                );
            }
            return T::default();
        }
    };
    match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(e) => {
            let kept = path.with_file_name(format!(
                "{}.bad",
                path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            ));
            let moved = std::fs::rename(path, &kept);
            log::error(format!(
                "{} does not parse ({e}); {}",
                path.display(),
                match &moved {
                    Ok(()) => format!("kept as {}", kept.display()),
                    Err(e) => format!("and it could not be kept aside: {e}"),
                }
            ));
            T::default()
        }
    }
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
    let carried = app.state::<Shared>().stats().carried();
    write_json(&carried_path(), &carried, false);
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

/// Whether a window position means anything in this session.
///
/// GDK's Wayland backend answers every `outer_position` with (0, 0) — the
/// protocol does not tell a client where it is. Recording that overwrites real
/// coordinates with the origin, and the next Xorg start pins the dashboard
/// under the GNOME top bar with its own close button out of reach.
fn can_place_windows() -> bool {
    #[cfg(windows)]
    {
        true
    }
    #[cfg(not(windows))]
    {
        !wayland_session() || forced_x11()
    }
}

fn save_window_positions(app: &AppHandle) {
    if !can_place_windows() {
        return;
    }
    write_json(&positions_path(), &window_positions(app), false);
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
            let revision = app.state::<Shared>().stats().revision();
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
                // Under the overlay, unless there is no room under the
                // overlay. A window placed past the bottom of the screen is
                // not left there: the window manager drags it back on, and it
                // comes to rest across the middle of the overlay it was meant
                // to hang below. An overlay parked near the bottom edge is the
                // normal case, not an odd one.
                // The PANEL's height, not the window's. The window has a floor
                // under it now — the control strip beside the panel can be the
                // taller of the two — so a short panel leaves transparent space
                // below itself, and a ticker measured from the window bottom
                // detached from the overlay by however much that was.
                let panel = (PANEL_H.load(Ordering::Relaxed) as f64 * scale * dpi) as i32;
                let tall = if panel > 0 { panel } else { size.height as i32 };
                let below = pos.y + tall + 4;
                let floor = main
                    .current_monitor()
                    .ok()
                    .flatten()
                    .map(|m| m.position().y + m.size().height as i32);
                let y = match floor {
                    Some(bottom) if below + height as i32 > bottom => pos.y - height as i32 - 4,
                    _ => below,
                };
                // The panel's width, not the window's: the window carries the
                // control strip beside the panel now, and a ticker as wide as
                // the window would hang past the overlay it belongs under.
                let width = (panel_w() * scale * dpi) as u32;
                let want = (
                    tauri::PhysicalPosition::new(pos.x, y),
                    tauri::PhysicalSize::new(width, height),
                );
                // Shown first, then placed — a position handed to an unmapped
                // window is advice the window manager may ignore, which is why
                // `reveal` does it in that order too. Through `reveal` rather
                // than a bare `show`, so the keep-above is asked for again:
                // hiding unmaps the window and a window manager keeps no state
                // for an unmapped one. This window hides and shows dozens of
                // times a run, far more often than the overlay it sits under.
                if !shown {
                    reveal(&app, "ticker", false);
                    let _ = ticker.set_zoom(scale);
                    set_click_through(&ticker, true);
                    shown = true;
                }
                // Checked against where the window actually is, not against
                // what we remember asking for. A move or a resize issued while
                // GTK is still mapping the window is quietly dropped, and
                // remembering the request as if it had worked left the ticker
                // sitting across the middle of the overlay for the rest of the
                // session. Asking again costs nothing when it already agrees.
                let here = (ticker.outer_position().ok(), ticker.outer_size().ok());
                if here != (Some(want.0), Some(want.1)) {
                    let _ = ticker.set_position(want.0);
                    let _ = ticker.set_size(want.1);
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
        let mut extra_at = Instant::now() - EXTRA_MIN_GAP;
        let (mut had_main, mut had_dash) = (false, false);
        // `None` until the server has answered once, so the first answer sets
        // the mark rather than sounding it. Starting at `false` meant every
        // launch announced whatever was already in the box.
        let mut had_mail: Option<bool> = None;
        loop {
            std::thread::sleep(Duration::from_millis(200));
            // The mail chime is announced on its own, before anything about
            // visibility is decided: the counters may be behind a hidden window
            // all run, and the reminder is the point of them.
            // Five quiet minutes stop the clock, so a break does not end up
            // divided into the per-hour figures. This is the only thread with a
            // heartbeat, so it is the one that has to ask.
            app.state::<Shared>().stats().watch_idle();
            let mail = app.state::<Shared>().stats().mail_state();
            if mail == Some(true) && had_mail == Some(false) {
                let _ = app.emit("mail", ());
            }
            if mail.is_some() {
                had_mail = mail;
            }
            // And the same for the zone moving on: the overlay is the window
            // that plays sounds whether it is on screen or not, and a player
            // who has hidden it still wants to know the drops got better.
            // Bound before the `if`, so the stats lock is let go on this line
            // rather than held across the emit and the window work below.
            let rotated = app.state::<Shared>().stats().take_zone_change();
            if let Some(zone) = rotated {
                // The zone travels with the event. The snapshot only goes to
                // windows that are on screen, so a hidden overlay would read
                // the alert off a stale one — and hidden is the case the chime
                // is for.
                let _ = app.emit("zone-changed", &zone);
                maybe_zone_flourish(&app, &zone);
            }

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
            let revision = shared.stats().revision();

            let due = |rev: u64, at: Instant, gap: Duration, beat: Duration| {
                (rev != revision && at.elapsed() >= gap) || at.elapsed() >= beat
            };
            if due(snap_rev, snap_at, SNAP_MIN_GAP, SNAP_HEARTBEAT) {
                let status = shared.status().text();
                let snapshot = shared.stats().snapshot(status);
                for (label, on_screen) in [("main", main), ("dashboard", dashboard)] {
                    if on_screen {
                        let _ = app.emit_to(label, "stats", &snapshot);
                    }
                }
                (snap_rev, snap_at) = (revision, Instant::now());
            }
            // The series and the drop journal are the heaviest payload in the
            // app, so they only travel while the statistics section is open —
            // and only when one of them has actually changed. Every event bumps
            // `revision`, the client's heartbeat arrives every few seconds all
            // run long, and the heartbeat below it fired anyway: a full journal
            // serialises to 129 KB and it was going out at up to one a second
            // for a journal and a series nobody had added to. Its own revision
            // moves when a drop is journalled, a series point is taken or the
            // character changes, which is everything `extra()` carries.
            let reading_stats = dashboard && STATS_SECTION.load(Ordering::Relaxed);
            if reading_stats && extra_at.elapsed() >= EXTRA_MIN_GAP {
                // The clock is reset whether or not anything had changed.
                //
                // It used to move only when something did, so a dashboard with
                // the statistics section open and nothing happening took the
                // heaviest lock in the app on every pass of this loop —
                // contending with the thread that is trying to parse packets,
                // for an answer that was always "no change".
                extra_at = Instant::now();
                let stats = shared.stats();
                let extra_now = stats.extra_revision();
                if extra_rev != extra_now {
                    let extra = stats.extra();
                    drop(stats);
                    let _ = app.emit_to("dashboard", "stats-extra", &extra);
                    extra_rev = extra_now;
                }
            }
        }
    });
}

pub(crate) fn read_settings() -> Settings {
    let mut settings: Settings = read_json_or_default(&settings_path());
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
    const GUESSED: [&str; 3] = [
        "gul rune,vex rune,qi rune,xo rune,sur rune",
        "ber rune,jah rune,drax rune,zed rune",
        // the SS runes before the game added Sus, Kek and Jord
        "fawn,flo,nju,jol",
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

/// One category rule as the engine wants it, or nothing if it is not a category.
///
/// A rule that names neither a rarity nor a type matches every named drop in
/// the game. Nothing in the picker can build one — it offers the bulk add only
/// once a dropdown is set — but an imported filter or a hand-edited file can,
/// and a list that quietly swallowed every drop would look exactly like the app
/// being broken. It is refused here, where every settings file passes.
fn engine_rule(rule: &SoundRule) -> Option<stats::Rule> {
    let rarity = rule
        .rarity
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_lowercase);
    let item_type = rule.item_type;
    if rarity.is_none() && item_type.is_none() {
        return None;
    }
    // A weapon type says nothing on its own: it numbers the kinds inside item
    // type 3, and 6 means Polearm there and nothing anywhere else.
    Some(stats::Rule { rarity, item_type, weapon: item_type.and(rule.weapon) })
}

fn apply_stats_settings(app: &AppHandle, settings: &Settings) {
    let active = settings
        .use_filter
        .then(|| settings.filters.iter().find(|f| f.id == settings.filter))
        .flatten();
    let mut notable: Vec<(String, Vec<String>)> = settings
        .notable
        .iter()
        .map(|g| (g.label.clone(), g.names.iter().map(|n| n.to_lowercase()).collect()))
        .collect();
    if notable.is_empty() {
        notable = stats::default_notable();
    }
    let prefs = stats::Prefs {
        prefer_ground: settings.sound_on_ground,
        // a rarity dropped from the tracked list must stop alerting even if an
        // older settings file still names it
        alerts: settings
            .alerts
            .iter()
            .filter(|r| stats::JOURNAL_RARITIES.contains(&r.as_str()))
            .cloned()
            .collect(),
        min_tier: settings.min_tier,
        // the flourish asks a different question of the same drop
        fx_rarities: if settings.flourish { settings.flourish_rarities.clone() } else { Vec::new() },
        fx_tier: settings.flourish_tier.clamp(1, 6),
        fx_listed: settings.flourish && settings.flourish_listed && settings.use_filter,
        notable_defs: notable,
        // the rotation asks a different question again, of the zone rather
        // than of a drop
        zone_buffs: settings.zone_buffs.clone(),
        // and this one of an item, but by identity rather than by name — see
        // `hunted_relic`. Emptied when the chime is switched off, so the engine
        // is told the alert is gone rather than being left to fire into a muted
        // channel: a hunted relic also takes the journal and the pillar, and
        // those are not the volume slider's to decide.
        relics: if settings.relic.enabled { settings.relics.clone() } else { Vec::new() },
        sound_lists: active
            .map(|f| {
                f.lists
                    .iter()
                    .filter(|l| l.enabled && !l.id.is_empty())
                    .map(|l| stats::Listed {
                        key: format!("list-{}", l.id),
                        names: l.items.iter().map(|n| n.trim().to_lowercase()).collect(),
                        rules: l.rules.iter().filter_map(engine_rule).collect(),
                    })
                    // An empty list is not a silent one — it is nothing at all,
                    // and it used to be dropped by `items.is_empty()` alone. A
                    // list can now be empty of names and still say something.
                    .filter(|l| !l.names.is_empty() || !l.rules.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
    };
    app.state::<Shared>().stats().set_prefs(prefs);
}

/// Everything a settings change touches outside the webviews.
fn apply_settings_effects(app: &AppHandle, settings: &Settings) {
    let scale = settings.scale.clamp(0.6, 1.5) as f64;
    LOCKED.store(settings.locked, Ordering::Relaxed);
    TICKER.store(settings.ticker, Ordering::Relaxed);
    DEBUG_LOG.store(settings.debug_log, Ordering::Relaxed);
    sniffer::set_wide_capture(settings.wide_capture);
    SCALE_MILLI.store((scale * 1000.0) as u32, Ordering::Relaxed);
    presence::set_enabled(settings.discord);
    FLOURISH.store(settings.flourish, Ordering::Relaxed);
    FLOURISH_ALWAYS.store(settings.flourish_always, Ordering::Relaxed);
    FLOURISH_ZONE.store(settings.flourish_zone, Ordering::Relaxed);
    ensure_flourish(app, settings.flourish, settings.flourish_scale.clamp(0.5, 2.0) as f64);
    if let Some(w) = app.get_webview_window("main") {
        // Click-through is NOT set here, though it used to be, and the comment
        // that stood in its place claimed the poller owned it.
        //
        // Both wrote it and only one remembered. Locking from the strip is the
        // case: the poller has settled on `ignoring = Some(false)` because the
        // cursor is on the strip, this line then makes the window click-through
        // behind its back, and on the next tick `want_ignore` is still false —
        // the cursor is still inside the rect — so the guard sees no change and
        // never puts it back. The strip is left fully lit with every click going
        // through it into the game, which is a click-to-move ARPG, and it stays
        // that way until the cursor leaves the rect and returns.
        //
        // The poller reads LOCKED, which is stored above, and converges within
        // one 50ms tick. One writer.
        let _ = w.set_zoom(scale);
        let _ = w.set_size(LogicalSize::new(BASE_W * scale, overlay_height(settings) * scale));
    }
    apply_autostart(settings.autostart);
}

/// Tell the webview its background is transparent, in so many words.
///
/// A window built with `transparent: true` is given an ARGB visual, but that
/// only says the surface *can* hold alpha — WebKitGTK still has to be told
/// what to clear it to, and without a background colour it clears to nothing
/// at all: each frame is composited over the last. On this desktop that is
/// visible as the previous text still readable under the new, and as the
/// flourish's soft shade thickening into a hard black blob over the twenty
/// frames it fades in across.
///
/// Windows and macOS do their own thing with transparency and are left alone.
#[cfg(not(windows))]
fn clear_to_nothing(w: &tauri::WebviewWindow) {
    use tauri::utils::config::Color;
    if let Err(e) = w.set_background_color(Some(Color(0, 0, 0, 0))) {
        log::warn(format!("{}: no transparent background: {e}", w.label()));
    }
}

#[cfg(windows)]
fn clear_to_nothing(_w: &tauri::WebviewWindow) {}

/// Click-through, asked only of a window that is actually on screen.
///
/// This is not defensive tidiness, it is the difference between running and
/// not. GTK routes the `true` branch to `input_shape_combine_region` on the
/// underlying GdkWindow, and a toplevel that has never been mapped has none —
/// tao unwraps that `None` inside a glib callback, so the panic aborts the
/// process instead of returning an error the `let _ =` could swallow. Turning
/// it back off is widget-level and always safe. Windows has no such rule, but
/// nothing there needs the call on a hidden window either.
fn set_click_through(w: &tauri::WebviewWindow, through: bool) {
    if !through {
        let _ = w.set_ignore_cursor_events(false);
    } else if w.is_visible().unwrap_or(false) {
        let _ = w.set_ignore_cursor_events(true);
    }
}

/// While locked the overlay is click-through EXCEPT the strip of icons down its
/// right-hand edge: a poller re-enables mouse events whenever the cursor is over
/// it. The lock is that strip's top cell, so this is still one rectangle.
fn spawn_strip_poller(app: AppHandle) {
    std::thread::spawn(move || {
        let mut ignoring: Option<bool> = None;
        let mut told: Option<bool> = None;
        loop {
            let locked = LOCKED.load(Ordering::Relaxed);
            // Where the cursor is, reported whether the overlay is locked or
            // not. The web side cannot work this out for itself while the
            // window is click-through, and asking it to use :hover the rest of
            // the time gave the button two different truths and a moment
            // between them where it had neither.
            let held = told == Some(true);
            let over = (|| {
                let w = app.get_webview_window("main")?;
                if !w.is_visible().ok()? {
                    return None;
                }
                let pos = w.outer_position().ok()?;
                let dpi = w.scale_factor().ok()?;
                let cur = app.cursor_position().ok()?;
                let z = dpi * SCALE_MILLI.load(Ordering::Relaxed) as f64 / 1000.0;
                // The corner opens it; the whole column keeps it open.
                let (x0, y0, x1, y1) = strip_rect(held);
                Some(
                    cur.x >= pos.x as f64 + x0 * z
                        && cur.x <= pos.x as f64 + x1 * z
                        && cur.y >= pos.y as f64 + y0 * z
                        && cur.y <= pos.y as f64 + y1 * z,
                )
            })();
            // The overlay is hidden: leave the state unset so the right one is
            // applied the first time it is really shown. Reading `None` as
            // "the cursor is not over the button" is what asked a never-mapped
            // window to go click-through, and that aborts the process.
            let Some(over) = over else {
                ignoring = None;
                told = None;
                std::thread::sleep(std::time::Duration::from_millis(200));
                continue;
            };
            if told != Some(over) {
                told = Some(over);
                let _ = app.emit_to("main", "strip-hover", over);
            }
            // Only a locked overlay is masked, and then only away from the
            // button: that is the whole point of locking it.
            let want_ignore = locked && !over;
            if ignoring != Some(want_ignore) {
                if let Some(w) = app.get_webview_window("main") {
                    set_click_through(&w, want_ignore);
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

/// Turn the wide capture on or off, without the caller holding the settings.
///
/// The panel that offers this is the trouble banner, which appears exactly when
/// nothing is working and has no business loading, editing and writing back the
/// whole settings file to flip one flag.
#[tauri::command(async)]
fn set_wide_capture(app: AppHandle, on: bool) -> Result<(), String> {
    let mut settings = read_settings();
    if settings.wide_capture == on {
        return Ok(());
    }
    settings.wide_capture = on;
    save_settings(app, settings)
}

#[tauri::command(async)]
fn get_settings() -> Settings {
    read_settings()
}

/// `(async)`, like every other command here that touches a file.
///
/// A plain `#[tauri::command]` is run inline on the main thread, which on
/// Windows is the thread that pumps window messages: while one is running,
/// nothing on screen answers a click, a drag or the close button. That is
/// affordable for reading an atomic, and it is not affordable for this — a save
/// is a flush to the device and, when an antivirus or the indexer has the file
/// open for a moment, up to another 60 ms of waiting on top. The word costs
/// nothing and moves all of it to a worker thread.
///
/// Commands that borrow `State` are left as they are: that borrow cannot cross
/// the hop, and none of them do I/O.
#[tauri::command(async)]
fn save_settings(app: AppHandle, mut settings: Settings) -> Result<(), String> {
    for cfg in [
        &mut settings.satanic,
        &mut settings.set,
        &mut settings.heroic,
        &mut settings.angelic,
        &mut settings.unholy,
        &mut settings.mail,
        &mut settings.zone,
    ] {
        cfg.volume = cfg.volume.clamp(0.0, 1.0);
    }
    settings.opacity = settings.opacity.clamp(0.3, 1.0);
    settings.scale = settings.scale.clamp(0.6, 1.5);
    settings.min_tier = settings.min_tier.clamp(0, 20);
    // Applied before it is written. The other way round, a setting that kills
    // the process on the way in is already on disk when it does — and every
    // later start reads it back and dies again, with no way to the panel that
    // would turn it off.
    apply_stats_settings(&app, &settings);
    apply_settings_effects(&app, &settings);
    let json = serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?;
    write_atomic(&settings_path(), &json).map_err(|e| {
        log::error(format!("cannot save settings: {e}"));
        e.to_string()
    })?;
    let _ = app.emit("settings-changed", &settings);
    Ok(())
}

#[tauri::command]
fn snapshot(state: State<Shared>) -> stats::Snapshot {
    let status = state.status().text();
    state.stats().snapshot(status)
}

#[tauri::command]
fn get_extra(state: State<Shared>) -> stats::Extra {
    state.stats().extra()
}

/// Runs are kept next to the settings, newest first, and the file is bounded:
/// this is a record of what happened, not a database.
const RUNS_KEPT: usize = 200;

fn runs_path() -> PathBuf {
    data_dir().join("runs.json")
}

pub(crate) fn read_runs() -> Vec<stats::Run> {
    read_json_or_default(&runs_path())
}

/// End the session and file it away. Everything that ends a run goes through
/// here — the button, the hotkey, the tray, the game closing and the app
/// quitting — so a run is never lost and never counted twice.
pub(crate) fn end_run(app: &AppHandle) {
    let finished = app.state::<Shared>().stats().finish();
    app.state::<Shared>().stats().reset();
    let Some(run) = finished else { return };
    let mut runs = read_runs();
    runs.insert(0, run);
    runs.truncate(RUNS_KEPT);
    write_json(&runs_path(), &runs, false);
    let _ = app.emit("runs-changed", ());
}

fn close_session(app: &AppHandle) {
    end_run(app);
}

#[tauri::command(async)]
fn get_runs() -> Vec<stats::Run> {
    read_runs()
}

#[tauri::command(async)]
fn clear_runs() -> Result<(), String> {
    // through the same staging as every other writer: a truncate in place is
    // what leaves a history that reads as nothing
    write_atomic(&runs_path(), b"[]").map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn reset_stats(app: AppHandle) {
    close_session(&app);
}

/// Stop or restart the session clock. The counters are untouched either way —
/// what a pause changes is what the run is divided by.
#[tauri::command]
fn set_paused(app: AppHandle, paused: bool) {
    app.state::<Shared>().stats().set_paused(paused);
}

fn toggle_pause(app: &AppHandle) {
    let shared = app.state::<Shared>();
    let mut stats = shared.stats();
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
        // Hidden, never destroyed. Tearing a webview down and building another
        // under the same label moments later is what froze the app: switching
        // the announcement off and straight back on did exactly that, and the
        // next thing to touch the window never got an answer. A hidden webview
        // costs a little memory; a window nobody can close costs the session.
        if existing.is_some() {
            PLACING.store(false, Ordering::Relaxed);
            hide_aux(app, "flourish");
        }
        return;
    }
    let size = LogicalSize::new(FLOURISH_W * scale, FLOURISH_H * scale);
    if let Some(w) = existing {
        let _ = w.set_size(size);
        // the setting can be turned on with the window already built
        if FLOURISH_ALWAYS.load(Ordering::Relaxed) {
            show_flourish(app, &w);
        } else if !PLACING.load(Ordering::Relaxed) {
            hide_aux(app, "flourish");
        }
        return;
    }
    // Never built on the thread that asks for it, once the app is up.
    //
    // A window is built by the event loop, and the builder waits for it to be.
    // A synchronous #[tauri::command] IS the event loop on Windows — Tauri runs
    // one inline on the main thread — so a build started from inside one waits
    // for a loop that is waiting for the command to return. Neither ever does.
    // Every window stops answering, including the close button, and the only
    // way out is the task manager. That is the report this app has had from a
    // Windows user on 0.9.93, and it is what this file already records having
    // done to itself once: see the comment above about tearing the window down
    // and building another.
    //
    // The path is not exotic. `save_settings` is a synchronous command; so are
    // `compact_mode` and `full_mode`, which call it through `set_face`; so is
    // the hotkey that toggles the lock. Any of them reaches here, and all it
    // takes is for the window not to exist yet — the player had the
    // announcement off when the app started, or the build failed at startup and
    // left nothing behind.
    //
    // Doing it on a thread of its own costs one thread for a few milliseconds
    // and takes the whole class away, whoever calls it and however they got
    // here. Before the loop is running there is nothing to deadlock against and
    // nothing to gain, so startup builds the window where it stands.
    if RUNNING.load(Ordering::Relaxed) {
        if BUILDING_FLOURISH.swap(true, Ordering::SeqCst) {
            return;
        }
        let app = app.clone();
        std::thread::spawn(move || {
            build_flourish(&app, size);
            BUILDING_FLOURISH.store(false, Ordering::SeqCst);
        });
        return;
    }
    build_flourish(app, size);
}

fn build_flourish(app: &AppHandle, size: LogicalSize<f64>) {
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
            clear_to_nothing(&w);
            // click-through is applied when it is shown; see `set_click_through`
            if FLOURISH_ALWAYS.load(Ordering::Relaxed) {
                show_flourish(app, &w);
            }
        }
        Err(e) => log::error(format!("the flourish window could not be built: {e}")),
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

/// The satanic zone has rotated onto something the player asked to be shown.
///
/// Decided here rather than in the window that draws it: whether a rotation is
/// worth announcing is the same question the chime answers, and asking it twice
/// in two languages is how the two come to disagree. `stats` has already ruled
/// on the buffs; what is left is whether this player wants a pillar for it.
fn maybe_zone_flourish(app: &AppHandle, zone: &stats::SatanicZone) {
    if !FLOURISH.load(Ordering::Relaxed) || !FLOURISH_ZONE.load(Ordering::Relaxed) {
        return;
    }
    let Some(w) = app.get_webview_window("flourish") else { return };
    // The raw zone code, not a name. Turning `Satanic_5_5` into "Act 5 : Temple
    // of Zamjo" needs a table of forty room names that the window already has
    // and this side has no other use for.
    let payload = serde_json::json!({
        "kind": "zone",
        "zone": zone.zone,
        "buffs": zone.buffs,
        "debuffs": zone.debuffs,
    });
    let _ = app.emit_to("flourish", "flourish-play", &payload);
    show_flourish(app, &w);
}

/// On screen without taking the keyboard, click-through, and where the player
/// left it. It hides itself again when the animation is over — the window tells
/// us, because it is the one that knows how long that is.
fn show_flourish(app: &AppHandle, w: &tauri::WebviewWindow) {
    if w.is_visible().unwrap_or(false) {
        return;
    }
    // A first announcement used to appear wherever the window manager felt
    // like putting it, which on one machine was over the dashboard.
    if parked("flourish").is_none() {
        park_below_centre(app, w);
    }
    reveal(app, "flourish", false);
    set_click_through(w, true);
}

/// The window says when it has finished playing, or that the player has parked
/// it and it may go away again.
#[tauri::command]
fn flourish_done(app: AppHandle) {
    if PLACING.load(Ordering::Relaxed) || FLOURISH_ALWAYS.load(Ordering::Relaxed) {
        return;
    }
    hide_aux(&app, "flourish");
}

/// While a flourish is being placed it stays on screen, takes the mouse and
/// loops, so it can be dragged where the player wants it.
static PLACING: AtomicBool = AtomicBool::new(false);
/// Which placement is current. A placement that outlives its welcome is ended
/// from here rather than by the window, because the whole trouble with a window
/// that takes the mouse is that it might be the thing not answering.
static PLACING_GEN: AtomicU32 = AtomicU32::new(0);
/// long enough to drag a box somewhere, short enough not to be stuck with it
const PLACING_LIMIT: Duration = Duration::from_secs(180);

/// Where the announcement goes before anyone has moved it.
///
/// Dead centre is where the fight is: the pillar lands on the character and on
/// whatever is being killed. A fifth of the screen lower clears the action and
/// still reads as the middle — and it is what a stream frames.
fn park_below_centre(app: &AppHandle, w: &tauri::WebviewWindow) {
    let Ok(Some(mon)) = w.current_monitor().or_else(|_| app.primary_monitor()) else {
        let _ = w.center();
        return;
    };
    let (pos, size) = (mon.position(), mon.size());
    let win = w.outer_size().unwrap_or(tauri::PhysicalSize { width: 480, height: 240 });
    let x = pos.x + (size.width as i32 - win.width as i32) / 2;
    let y = pos.y + (size.height as i32 - win.height as i32) / 2 + (size.height as i32) / 5;
    let _ = w.set_position(tauri::PhysicalPosition { x, y });
}

#[tauri::command]
fn place_flourish(app: AppHandle, placing: bool) {
    let Some(w) = app.get_webview_window("flourish") else {
        // Nothing to place. Saying so beats setting the flag and leaving the
        // app in a mode it cannot be talked out of.
        PLACING.store(false, Ordering::Relaxed);
        return;
    };
    PLACING.store(placing, Ordering::Relaxed);
    if placing {
        // It has never been put anywhere, so it goes in the middle rather than
        // wherever the window manager fancies — which on a first run has been
        // on top of the dashboard, over the buttons, invisible.
        if parked("flourish").is_none() {
            park_below_centre(&app, &w);
        }
        reveal(&app, "flourish", false);
        // While it is being placed it is an ordinary window: it takes the
        // mouse, and it has to be able to take the keyboard too, or its own
        // Done button is the one thing on screen that cannot be clicked.
        let _ = w.set_focusable(true);
        set_click_through(&w, false);
        let _ = w.set_focus();
        let _ = app.emit_to("flourish", "flourish-placing", true);
        let mine = PLACING_GEN.fetch_add(1, Ordering::Relaxed) + 1;
        let later = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(PLACING_LIMIT);
            if PLACING.load(Ordering::Relaxed) && PLACING_GEN.load(Ordering::Relaxed) == mine {
                place_flourish(later, false);
            }
        });
    } else {
        PLACING_GEN.fetch_add(1, Ordering::Relaxed);
        let _ = app.emit_to("flourish", "flourish-placing", false);
        set_click_through(&w, true);
        let _ = w.set_focusable(false);
        if FLOURISH_ALWAYS.load(Ordering::Relaxed) {
            show_flourish(&app, &w);
        } else {
            hide_aux(&app, "flourish");
        }
    }
}

/// What the About section shows about the build it is running in.
#[derive(Serialize)]
pub struct About {
    version: String,
    platform: &'static str,
    repo: &'static str,
    /// what the overlay measures, so a capture source can be given the size
    /// instead of it being guessed at
    overlay_w: u32,
    overlay_h: u32,
    /// where this process actually lives, so the setcap line the dashboard
    /// shows is one that can be pasted rather than a placeholder
    binary: String,
    /// Running out of an AppImage, where `setcap` is not merely unnecessary but
    /// harmful.
    ///
    /// A binary carrying a capability is a privileged one, so the loader stops
    /// trusting the library path it was handed — and that path is the whole of
    /// how an AppImage finds its own bundled libraries. Granting the right
    /// stops the app starting at all, with `libpcap.so.0.8: cannot open shared
    /// object file`, which is a worse place to be than counting nothing. The
    /// panel has to know, because it is where the advice is given.
    appimage: bool,
}

const REPO: &str = "https://github.com/Parazeya/hs-tracker";

/// The front end's own errors. A panel that throws while rendering goes blank
/// and says nothing; this is how it says something.
#[tauri::command(async)]
fn report(level: String, message: String) {
    let level = match level.as_str() {
        "warn" => "warn",
        _ => "error",
    };
    // it arrives from the web side, so it is text and nothing else
    let trimmed: String = message.chars().take(2000).collect();
    log::say(level, &format!("ui: {trimmed}"));
}

/// Where the log is, for anyone being asked to send it.
#[tauri::command]
fn log_path() -> String {
    log::path().display().to_string()
}

/// Show it in the file manager, selected.
/// Wait for a helper in the background rather than dropping its handle.
///
/// `xdg-open` returns the moment it has handed the request on, but a `Child`
/// that is dropped without being waited on stays a zombie for the life of the
/// process — one per click on "Show it in the folder".
fn reap(spawned: std::io::Result<std::process::Child>) -> std::io::Result<()> {
    let mut child = spawned?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[tauri::command(async)]
fn show_log() -> Result<(), String> {
    let file = log::path();
    // Explorer's parser, not the one std uses.
    //
    // `arg` quotes the argument it is given, so explorer was handed
    // `"/select,C:\...\HS Tracker\hs-tracker.log"` as one quoted token, did not
    // recognise it, and opened Documents with nothing selected. The product's
    // own name has a space in it, so that is every default install — and the
    // button reported success while doing it, at exactly the moment someone is
    // being asked to find their log.
    #[cfg(windows)]
    let spawned = {
        use std::os::windows::process::CommandExt as _;
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", file.display()))
            .spawn()
    };
    #[cfg(not(windows))]
    let spawned = std::process::Command::new("xdg-open")
        .arg(file.parent().unwrap_or(&file))
        .spawn();
    reap(spawned).map_err(|e| e.to_string())
}

#[tauri::command]
fn about() -> About {
    About {
        // the crate's version is the one the installer and the tag carry, since
        // scripts/set-version.mjs writes all three from package.json
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: std::env::consts::OS,
        repo: REPO,
        // the panel without its strip: the size to give a window capture
        overlay_w: panel_w() as u32,
        overlay_h: {
            let measured = PANEL_H.load(Ordering::Relaxed);
            if measured > 0 { measured } else { 199 }
        },
        // inside an AppImage the running binary is on a mount that will be
        // gone; the file the user keeps is the one to name
        appimage: std::env::var_os("APPIMAGE").is_some(),
        binary: std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok())
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    }
}

/// Open a link in whatever browser the desktop uses.
///
/// Only ever this project's own pages: the address arrives from the web side,
/// and handing an arbitrary string to a shell is how a link becomes a command.
#[tauri::command(async)]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with(REPO) {
        return Err("that is not one of this project's pages".into());
    }
    #[cfg(windows)]
    let spawned = std::process::Command::new("cmd").args(["/C", "start", "", &url]).spawn();
    #[cfg(not(windows))]
    let spawned = std::process::Command::new("xdg-open").arg(&url).spawn();
    reap(spawned).map_err(|e| e.to_string())
}

#[tauri::command]
fn fit_overlay(app: AppHandle, height: f64, width: Option<f64>) {
    let height = height.clamp(60.0, 1200.0);
    // kept for the scale slider: zoom changes the window without changing a
    // single CSS pixel of the panel, so nothing would measure it again
    PANEL_H.store(height.round() as u32, Ordering::Relaxed);
    if let Some(w) = width {
        remember_width(w);
    }
    let Some(w) = app.get_webview_window("main") else { return };
    let scale = SCALE_MILLI.load(Ordering::Relaxed) as f64 / 1000.0;
    let wanted = LogicalSize::new(base_w() * scale, height.max(STRIP_H) * scale);
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
    // An iconified toplevel is still "visible" to GTK, so `show` on a minimised
    // window is a no-op — tray -> Dashboard after clicking minimise did nothing
    // at all, and on Wayland the tray is the only control there is.
    let _ = w.unminimize();
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
        // tao refuses to raise a window it does not yet believe is mapped, and
        // the show above is still queued on the main loop when we get here —
        // so the request is handed back to that loop to run after it.
        let (app, label) = (app.clone(), label.to_string());
        let _ = app.clone().run_on_main_thread(move || {
            if let Some(w) = app.get_webview_window(&label) {
                let _ = w.set_focus();
            }
        });
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
    // With no tray to hide into and no overlay, this is the only window there
    // is and the hotkeys that would bring it back were never registered. The
    // button says "Close to tray"; where there is no tray, closing is what it
    // has to mean.
    if !TRAY_OK.load(Ordering::Relaxed) && !overlay_supported() {
        app.exit(0);
        return;
    }
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

#[tauri::command(async)]
fn compact_mode(app: AppHandle) {
    set_face(&app, true);
}

#[tauri::command(async)]
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
    /// Categories, the same way `SoundList` carries them. A file written before
    /// they existed has none and imports as it always did; one written after
    /// them opens on an older build with its rules skipped rather than
    /// refused, which serde's default gives for free.
    #[serde(default)]
    rules: Vec<SoundRule>,
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
    let key = format!("list-{id}");
    // Every other route to a sound file asks this first; this one did not, and
    // it is the one that builds a path out of an id and then reads whatever is
    // there. An id is only ever minted by this app, but settings.json is a
    // plain file on disk and a hand-edited one could name any path it liked.
    if !sound_key(&key) {
        return None;
    }
    SOUND_EXTS.iter().find_map(|(ext, _)| {
        let path = sounds_dir().join(format!("{key}.{ext}"));
        std::fs::read(&path).ok().map(|bytes| ExportedSound {
            ext: (*ext).to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    })
}

/// The sound filed under any key, not just a list's.
fn sound_by_key(key: &str) -> Option<ExportedSound> {
    if !sound_key(key) {
        return None;
    }
    SOUND_EXTS.iter().find_map(|(ext, _)| {
        let path = sounds_dir().join(format!("{key}.{ext}"));
        std::fs::read(&path).ok().map(|bytes| ExportedSound {
            ext: (*ext).to_string(),
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    })
}

/// Put one back. The key is checked the same way it is on the way out: a
/// settings file is a plain file on disk and could name any path it liked.
fn write_sound(dir: &std::path::Path, key: &str, snd: &ExportedSound) -> Result<(), String> {
    use base64::Engine;
    if !sound_key(key) || !SOUND_EXTS.iter().any(|(e, _)| *e == snd.ext) {
        return Err("not a sound this app files".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&snd.data)
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // One key, one file — the rule `pick_sound` and `clear_sound` both keep.
    // Every reader takes the first extension of SOUND_EXTS that exists, so
    // importing a satanic.wav onto a machine that already had a satanic.mp3
    // left both on disk with the old mp3 still playing: an import that says it
    // replaces every setting, and the one it could not replace was the sound.
    // Written first and cleared after, as there: a write that fails must not
    // leave the key silent.
    let staged = dir.join(format!("{key}.{}.new", snd.ext));
    std::fs::write(&staged, bytes).map_err(|e| e.to_string())?;
    for (e, _) in SOUND_EXTS {
        let _ = std::fs::remove_file(dir.join(format!("{key}.{e}")));
    }
    std::fs::rename(&staged, dir.join(format!("{key}.{}", snd.ext))).map_err(|e| {
        let _ = std::fs::remove_file(&staged);
        e.to_string()
    })
}

/// Give a new list the sound an old one has, for duplicating a filter.
///
/// A list's sound is a file named after its id, and a copy is given fresh ids
/// so it cannot fight with the original — which left the copy mute while the
/// button that made it promised "sounds and all". Silent when there is nothing
/// to copy: most lists have no sound of their own.
#[tauri::command(async)]
fn copy_sound(app: AppHandle, from: String, to: String) -> Result<(), String> {
    if !sound_key(&from) || !sound_key(&to) || from == to {
        return Err("bad list".into());
    }
    for (ext, _) in SOUND_EXTS {
        let source = sounds_dir().join(format!("{from}.{ext}"));
        if source.exists() {
            std::fs::create_dir_all(sounds_dir()).map_err(|e| e.to_string())?;
            std::fs::copy(&source, sounds_dir().join(format!("{to}.{ext}"))).map_err(|e| e.to_string())?;
            let _ = app.emit("sounds-changed", &to);
            break;
        }
    }
    Ok(())
}

// `async` here is not about concurrency: a plain command runs on the main
// thread, and a native file dialog opened from there stops the event loop
// dead — the windows go grey and unclickable, and the app cannot even be
// closed, until the dialog is answered. The plugin says as much of its own
// blocking calls. Marked async, the command runs off the main thread, the
// dialog is dispatched back to it, and the rest of the app keeps drawing.
#[tauri::command(async)]
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
                rules: l.rules,
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&exported).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(Some(path.file_name().unwrap_or_default().to_string_lossy().into_owned()))
}

/// Everything the app remembers, in one file.
///
/// A filter export carries one filter and its sounds; this carries the lot —
/// every switch, every filter, every list, and the sound files themselves,
/// which live outside settings.json and would otherwise arrive as silence on
/// the other machine.
#[derive(serde::Serialize, serde::Deserialize)]
struct ExportedSettings {
    app: String,
    version: u32,
    kind: String,
    settings: Settings,
    /// sound key -> the file, so a restore is not missing its audio
    sounds: std::collections::HashMap<String, ExportedSound>,
}

/// Every custom sound the settings refer to, by the key it is filed under.
fn all_sounds(settings: &Settings) -> std::collections::HashMap<String, ExportedSound> {
    let mut out = std::collections::HashMap::new();
    for rarity in SOUND_KEYS {
        if let Some(snd) = sound_by_key(rarity) {
            out.insert(rarity.to_string(), snd);
        }
    }
    for filter in &settings.filters {
        for list in &filter.lists {
            if let Some(snd) = list_sound(&list.id) {
                out.insert(format!("list-{}", list.id), snd);
            }
        }
    }
    out
}

/// Off the main thread; see `export_filter`.
#[tauri::command(async)]
fn export_settings(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let settings = read_settings();
    let picked = app
        .dialog()
        .file()
        .add_filter("HS Tracker settings", &["json"])
        .set_file_name("hs-tracker-settings.hstracker.json")
        .blocking_save_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let exported = ExportedSettings {
        app: "hs-tracker".into(),
        version: 1,
        kind: "settings".into(),
        sounds: all_sounds(&settings),
        settings,
    };
    let json = serde_json::to_string_pretty(&exported).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(Some(path.file_name().unwrap_or_default().to_string_lossy().into_owned()))
}

/// Off the main thread; see `export_filter`.
#[tauri::command(async)]
fn import_settings(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app
        .dialog()
        .file()
        .add_filter("HS Tracker settings", &["json"])
        .blocking_pick_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let exported: ExportedSettings =
        serde_json::from_str(&text).map_err(|_| "not an HS Tracker settings file".to_string())?;
    if exported.app != "hs-tracker" || exported.kind != "settings" {
        return Err("not an HS Tracker settings file".into());
    }
    // the sounds first: settings that name a file which is not there yet would
    // be saved, applied, and play nothing
    for (key, snd) in &exported.sounds {
        let _ = write_sound(&sounds_dir(), key, snd);
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    save_settings(app, exported.settings)?;
    Ok(Some(name))
}

/// Off the main thread; see `export_filter`.
#[tauri::command(async)]
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
            rules: list.rules,
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

/// The front end reporting that it has actually painted.
///
/// This is the only proof the app has that its renderer works. Windows can be
/// built, shown and left blank — every one of them is transparent, so a dead
/// web process is an *invisible* window, not an empty one — and nothing in
/// WebKitGTK tells us. Two things hang off the signal: the XWayland breadcrumb
/// is only cleared once a page is up, and a watchdog says so in the log when
/// no page ever arrives.
#[tauri::command]
fn ui_ready() {
    if UI_READY.swap(true, Ordering::Relaxed) {
        return;
    }
    #[cfg(not(windows))]
    {
        let _ = std::fs::remove_file(data_dir().join("x11-attempt"));
        // Something was drawn, so this start is not the one that failed. See
        // `ease_webkit`.
        let _ = std::fs::remove_file(data_dir().join("no-paint"));
    }
    log::say("start", "the interface is up");
}

/// What this session actually is, in the log, once.
///
/// Every Linux report so far has turned on facts nobody could see afterwards:
/// which display server, which backend the toolkit chose, which driver. The
/// app knew all of it and wrote none of it down.
#[cfg(not(windows))]
fn log_environment() {
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| "-".into());
    let driver = if std::path::Path::new("/sys/module/nvidia/version").exists() {
        std::fs::read_to_string("/sys/module/nvidia/version")
            .map(|v| format!("nvidia {}", v.trim()))
            .unwrap_or_else(|_| "nvidia".into())
    } else if std::path::Path::new("/sys/module/nouveau").exists() {
        "nouveau".into()
    } else {
        "no nvidia module".into()
    };
    log::say(
        "env",
        &format!(
            "session={} wayland={:?} display={:?} gdk={} desktop={} | gpu: {} | overlay={} x11-forced={}",
            var("XDG_SESSION_TYPE"),
            var("WAYLAND_DISPLAY"),
            var("DISPLAY"),
            var("GDK_BACKEND"),
            var("XDG_CURRENT_DESKTOP"),
            driver,
            overlay_supported(),
            forced_x11(),
        ),
    );
}

/// The same, for the side that has actually had the bug.
///
/// Every Windows report this app has had arrived with two lines of log and
/// nothing about the machine underneath them. The one freeze it has been told
/// about could not be narrowed at all, because nothing written down said which
/// Windows, which WebView2, or where the app was installed from — and the
/// webview runtime is exactly the sort of thing that differs between a machine
/// a bug happens on and one it does not.
#[cfg(windows)]
fn log_environment() {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let windows = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .map(|key| {
            let text = |name: &str| key.get_value::<String, _>(name).unwrap_or_else(|_| "-".into());
            let patch = key
                .get_value::<u32, _>("UBR")
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "-".into());
            format!(
                "{} {} build {}.{}",
                text("ProductName"),
                text("DisplayVersion"),
                text("CurrentBuild"),
                patch
            )
        })
        .unwrap_or_else(|e| format!("Windows (unreadable: {e})"));

    // The runtime the whole interface is drawn by, and the one thing here that
    // is not part of the app: it updates on its own schedule, and a player can
    // be running a version nobody has ever tested against.
    let webview = tauri::webview_version().unwrap_or_else(|e| format!("unreadable ({e})"));

    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "-".into());

    // Written to, not asked about. A folder can exist, list fine and still
    // refuse a write — an install under Program Files does exactly that, and
    // then the log the user is asked for is the one file that was never saved.
    let dir = data_dir();
    let probe = dir.join(".write-probe");
    let writable = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);

    log::say(
        "env",
        &format!(
            "HS Tracker {} | {windows} | WebView2 {webview} | data {} ({}) | exe {exe} | overlay={}",
            env!("CARGO_PKG_VERSION"),
            dir.display(),
            if writable { "writable" } else { "READ-ONLY" },
            overlay_supported(),
        ),
    );
}

/// Nothing was drawn, so try once more without the renderer that usually
/// explains it — rather than printing the remedy and hoping it is read.
///
/// The player this was written for had four EGL failures and a paragraph of
/// advice in front of him and answered "yeah idk what's going on atp". He was
/// one restart away. Restarting is something this app already does to change
/// GTK backends, and `ease_webkit` reads the mark on the way up, so the
/// replacement needs nothing handed to it.
///
/// Once, and the mark is what says so. `HS_TRACKER_RELAUNCHED` cannot be the
/// guard here: on the machine this came from it was already set, because that
/// process was itself the replacement that came up through XWayland.
#[cfg(not(windows))]
fn retry_without_dmabuf() -> bool {
    let soft = data_dir().join("soft-render");
    if soft.exists() || std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some() {
        return false; // this start was already the retry
    }
    if std::fs::write(&soft, "the DMA-BUF renderer drew nothing on this machine\n").is_err() {
        return false;
    }
    // inside an AppImage the mounted binary is not what the user keeps
    let Some(exe) =
        std::env::var_os("APPIMAGE").map(PathBuf::from).or_else(|| std::env::current_exe().ok())
    else {
        return false;
    };
    log::warn("nothing was drawn in 20s - restarting once with the DMA-BUF renderer off");
    // The replacement reads `soft-render`, which is already written; the attempt
    // mark belongs to this process and would only tell it a second time.
    handing_over();
    // The environment carries over as it stands, the GTK backend with it. The
    // marker only stops the backend logic relaunching a second time on top.
    match std::process::Command::new(exe).env("HS_TRACKER_RELAUNCHED", "1").spawn() {
        Ok(_) => true,
        Err(e) => {
            log::error(format!("could not restart: {e}"));
            false
        }
    }
}

/// Nothing painted, and by now something would have. Says so where the user
/// can find it, because on screen there is only an invisible window.
// Windows has nothing to retry: there the handle is only carried, not used.
#[cfg_attr(windows, allow(unused_variables))]
fn spawn_render_watchdog(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        if UI_READY.load(Ordering::Relaxed) {
            return;
        }
        // Do it rather than describe it; see `retry_without_dmabuf`.
        #[cfg(not(windows))]
        if retry_without_dmabuf() {
            app.exit(0);
            return;
        }
        // The advice differs by platform and used to be printed on both: a
        // Windows user was told to try two WebKitGTK environment variables that
        // do not exist on their machine, which reads as the app not knowing
        // what it is running on.
        #[cfg(windows)]
        log::error(
            "no window has drawn anything after 20s - the web process is probably dead. \
             The WebView2 runtime is the usual cause: repair or reinstall the Microsoft Edge \
             WebView2 Runtime. The env line above says which version was in use.",
        );
        #[cfg(not(windows))]
        log::error(
            "no window has drawn anything after 20s, and not with the DMA-BUF renderer \
             off either - so the web process is dying for another reason. Worth trying: \
             WEBKIT_DISABLE_COMPOSITING_MODE=1, then LIBGL_ALWAYS_SOFTWARE=1. Please send \
             this log and whatever the terminal printed alongside it.",
        );
    });
}

#[tauri::command]
fn ticker_busy(active: bool) {
    TICKER_BUSY.store(active, Ordering::Relaxed);
}

#[tauri::command(async)]
fn get_shopping() -> Vec<String> {
    std::fs::read_to_string(shopping_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command(async)]
fn set_shopping(items: Vec<String>) -> Result<(), String> {
    let items: Vec<String> = items.into_iter().filter(|s| !s.trim().is_empty()).take(200).collect();
    let json = serde_json::to_vec_pretty(&items).map_err(|e| e.to_string())?;
    write_atomic(&shopping_path(), &json).map_err(|e| e.to_string())
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
#[tauri::command(async)]
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
#[tauri::command(async)]
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

#[tauri::command(async)]
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
/// unreliable in a frameless always-on-top window. Off the main thread; see
/// `export_filter`.
#[tauri::command(async)]
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
    // The new one is put down before the old one is taken away. The other way
    // round — clear, then copy — a copy that fails for any reason at all
    // leaves the key with nothing, and the player has lost a sound by trying
    // to change it. Copied under a temporary name so a half-written file is
    // never the one left behind.
    let name = format!("{rarity}.{ext}");
    let staged = dir.join(format!("{rarity}.{ext}.new"));
    std::fs::copy(&path, &staged).map_err(|e| e.to_string())?;
    for (e, _) in SOUND_EXTS {
        let _ = std::fs::remove_file(dir.join(format!("{rarity}.{e}")));
    }
    std::fs::rename(&staged, dir.join(&name)).map_err(|e| {
        let _ = std::fs::remove_file(&staged);
        e.to_string()
    })?;
    let _ = app.emit("sounds-changed", &rarity);
    Ok(Some(name))
}

#[tauri::command(async)]
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
    // `is_visible` alone stays true for a minimised window on both toolkits,
    // so clicking the tray icon to bring back a dashboard the player had just
    // minimised hid it instead — and the second click had to undo that first.
    let visible =
        |label: &str| app.get_webview_window(label).is_some_and(|w| on_screen(&w));
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
            // A click is reported twice — once going down and once coming up —
            // so acting on both toggled the window twice for one press: it
            // appeared and vanished again, and only a double click, being an
            // even number of toggles away, appeared to work. The release is
            // the one to act on: it is where the press finished, and it is
            // what a drag off the icon cancels.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
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
        write_json(&settings_path(), &settings, true);
        log::warn("the last start through XWayland failed; coming up on Wayland instead");
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
/// NVIDIA is not the only way it fails, though, and the second way cannot be
/// tested for from here.
///
/// A player on KDE with an AMD card and no NVIDIA module at all got
/// `Could not create default EGL display: EGL_BAD_PARAMETER. Aborting...` four
/// times over and then the same invisible window — through XWayland, with the
/// overlay working and the renderer still unable to find a display to draw on. Whatever the renderer
/// wants there, it does not have, and nothing this process can read says so in
/// advance: the failure happens inside a web process that has not started yet.
///
/// So it is learned instead. Every start leaves `no-paint` behind and `ui_ready`
/// removes it once something has actually been drawn; a start that finds the
/// file knows the run before it drew nothing, and writes `soft-render` to say
/// that this machine does not get the DMA-BUF renderer again. Delete that file
/// to try it once more — after a driver update, say.
///
/// Only machines that need it pay for it, and never one where the user has
/// already made the choice themselves. It has to be set before GTK starts,
/// which is why it lives at the top of `run` — and the process is still
/// single-threaded here, so setting it is safe.
#[cfg(not(windows))]
fn ease_webkit() {
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some() {
        return;
    }
    let nvidia = ["/dev/nvidiactl", "/sys/module/nvidia/version"]
        .iter()
        .any(|p| std::path::Path::new(p).exists());

    let painting = data_dir().join("no-paint");
    let soft = data_dir().join("soft-render");
    // The run before this one got as far as writing the breadcrumb and never
    // as far as drawing. Once is enough to stop trying.
    if painting.exists() && !soft.exists() {
        let _ = std::fs::write(&soft, "the DMA-BUF renderer drew nothing on this machine\n");
        log::say("start", "nothing was drawn last time; the DMA-BUF renderer is off from now on");
    }
    let _ = std::fs::write(&painting, "");

    if nvidia || soft.exists() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
}

#[cfg(windows)]
fn ease_webkit() {}

pub fn run() {
    log::init(env!("CARGO_PKG_VERSION"));
    ease_webkit();
    honour_backend_choice();
    sniffer::prepare_capture();
    let hk_toggle: Shortcut = HK_TOGGLE.parse().unwrap();
    let hk_lock: Shortcut = HK_LOCK.parse().unwrap();
    let hk_reset: Shortcut = HK_RESET.parse().unwrap();
    let hk_pause: Shortcut = HK_PAUSE.parse().unwrap();
    let app = tauri::Builder::default()
        // First, before anything else can be built: a second copy has its own
        // sniffer and its own writes to settings.json and runs.json, so both
        // would count the same session and the loser's file would win. It hands
        // its arguments over and leaves, and the copy already running comes to
        // the front instead.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            let face = if read_settings().compact && overlay_supported() { "main" } else { "dashboard" };
            reveal(app, face, true);
        }))
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
            about,
            report,
            log_path,
            show_log,
            open_url,
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
            export_settings,
            import_settings,
            import_filter,
            ticker_busy,
            ui_ready,
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
            set_wide_capture,
            pick_sound,
            copy_sound,
            clear_sound
        ])
        .setup(|app| {
            let overlay = overlay_supported();
            // The tray is a convenience, not the app. On Linux the indicator
            // library is dlopened and panics outright when it is missing, and
            // this is the first line of setup — one absent .so and there is
            // never a window at all. Note it and carry on; the front end asks
            // `session_info` whether there is a tray to hide into.
            if let Err(e) = build_tray(app.handle()) {
                TRAY_OK.store(false, Ordering::Relaxed);
                log::error(format!("no tray icon: {e}"));
            }
            // hotkeys are the overlay's remote control, and the backend they
            // need is X11's; registering them under Wayland reports success and
            // then nothing ever fires
            if overlay {
                for hk in [HK_TOGGLE, HK_LOCK, HK_RESET, HK_PAUSE] {
                    if let Err(e) = app.global_shortcut().register(hk) {
                        log::warn(format!("hotkey {hk} not registered: {e}"));
                    }
                }
            } else {
                log::say("info", "wayland session: running as the dashboard, without the overlay");
            }
            let settings = read_settings();
            app.state::<Shared>().stats().restore(&read_carried());
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
                set_click_through(&t, true);
            }
            #[cfg(debug_assertions)]
            if let Some(w) = app.get_webview_window("main") {
                w.open_devtools();
            }
            // both of these only ever move or mask the overlay and the ticker
            if overlay {
                spawn_strip_poller(app.handle().clone());
                spawn_ticker_glue(app.handle().clone());
            }
            log_environment();
            for label in ["main", "ticker", "dashboard"] {
                if let Some(w) = app.get_webview_window(label) {
                    clear_to_nothing(&w);
                }
            }
            // The scope in tauri.conf.json cannot match this directory on unix:
            // there the glob refuses to let `**` cross a dot component, and the
            // whole path lives under ~/.config. Every custom sound then 403s
            // and is re-delivered as base64 over IPC. Granted here, by path.
            {
                use tauri::Manager as _;
                let _ = app.asset_protocol_scope().allow_directory(sounds_dir(), false);
            }
            spawn_render_watchdog(app.handle().clone());
            spawn_position_saver(app.handle().clone());
            spawn_stats_pusher(app.handle().clone());
            presence::spawn(app.handle().clone());
            sniffer::spawn(app.state::<Shared>().inner(), app.handle().clone());
            // The breadcrumb that says "we already tried XWayland" used to be
            // dropped here, but `setup` runs before `app.run` and so before a
            // single page has painted: a backend that builds windows and then
            // fails to render them looked like success. It is cleared once the
            // front end says it is up — see the `ui_ready` command.
            
            Ok(())
        })
        .on_window_event(|window, event| {
            // The close button on a frame the window manager draws would destroy
            // the window, and a destroyed dashboard cannot be brought back from
            // the tray — on a Wayland session it is the only face there is.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle().clone();
                let label = window.label().to_string();
                // Closing the placement box has to end the placement, or
                // `PLACING` stays true for the full watchdog and the next real
                // drop re-shows a mouse-grabbing dashed frame over the game.
                if label == "flourish" && PLACING.load(Ordering::Relaxed) {
                    api.prevent_close();
                    place_flourish(app, false);
                    return;
                }
                // Hiding is only kind while there is somewhere to hide into.
                // Without a tray — an indicator library that would not load,
                // a session with no host for one — the dashboard would be gone
                // with no way back, and on Wayland it is the only face there
                // is. Then let it close and take the app with it.
                if label == "dashboard" && !TRAY_OK.load(Ordering::Relaxed) {
                    // Letting it close is not the same as letting it go. The
                    // overlay and the ticker keep the event loop alive, so what
                    // was left was a process with nothing on screen, no tray and
                    // no window to bring back — and the single-instance guard
                    // then swallowed every relaunch, so the only way to start
                    // the app again was to find it in a process list and kill
                    // it. Going out through `exit` also files the run and saves
                    // the window positions, which destroying the window skips.
                    api.prevent_close();
                    app.exit(0);
                    return;
                }
                api.prevent_close();
                hide_aux(&app, &label);
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        if let tauri::RunEvent::Ready = event {
            RUNNING.store(true, Ordering::Relaxed);
        }
        if let tauri::RunEvent::Exit = event {
            save_window_positions(app);
            save_carried(app);
            // quitting mid-run still files it
            end_run(app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The strip follows the panel's edge instead of remembering where it was.
    ///
    /// The panel was a fixed 444 on both sides of the process boundary, and the
    /// strip's column was two constants written from it. Reported as "Squished
    /// Panel": where the text drew wider, the chips spilled over the row and no
    /// setting could help. Now the panel is measured — so the strip has to be
    /// asked, not told, or it would sit on top of a panel that had grown past it.
    ///
    /// One test rather than several: `PANEL_WIDTH` is a process-wide static and
    /// cargo runs tests in parallel, so its value is only safely anyone's while
    /// a single test owns it.
    #[test]
    fn the_strip_stands_where_the_panel_now_ends() {
        let before = PANEL_WIDTH.load(Ordering::Relaxed);
        PANEL_WIDTH.store(0, Ordering::Relaxed);

        // unmeasured, it is exactly what the two constants used to read
        assert_eq!(panel_w(), PANEL_W);
        assert_eq!(base_w(), BASE_W);
        assert_eq!(strip_rect(false), (444.0, 0.0, 472.0, STRIP_W + 3.0));
        assert_eq!(strip_rect(true), (444.0, 0.0, 472.0, STRIP_H));

        // measured wider, the column moves with the edge and keeps its width
        remember_width(520.4);
        assert_eq!(panel_w(), 520.0);
        let (x0, _, x1, _) = strip_rect(false);
        assert_eq!((x0, x1 - x0), (520.0, STRIP_W), "the strip is beside the panel, not on it");

        // a page mid-layout reports nothing useful, and the window must not
        // shrink to it and clip the panel
        for narrow in [0.0, -1.0, 12.0, 443.9, f64::NAN] {
            remember_width(narrow);
            assert_eq!(
                panel_w(),
                PANEL_W,
                "a width of {narrow} narrowed the panel below what it is drawn at"
            );
        }
        remember_width(9000.0);
        assert_eq!(panel_w(), 1600.0, "and nothing legitimate is wider than this");

        PANEL_WIDTH.store(before, Ordering::Relaxed);
    }

    /// Seven commands build a filesystem path out of an id that came from the
    /// web side, and `list_sound` once skipped this check outright — its own
    /// comment records it. This is defence against the app's own bad data
    /// rather than a live exploit, and it is worth a test for exactly that
    /// reason: nothing about a wrong answer here is loud.
    #[test]
    fn a_sound_key_cannot_walk_out_of_its_directory() {
        for good in ["satanic", "set", "heroic", "angelic", "unholy", "mail", "zone", "list-9f3a2b", "list-a-b"] {
            assert!(sound_key(good), "{good} should be allowed");
        }
        for bad in [
            "list-../../etc/passwd",
            "list-a/b",
            "list-a\\b",
            "../satanic",
            "list-a.b",
            "",
            "satanic ",
            "LIST-abc/..",
        ] {
            assert!(!sound_key(bad), "{bad:?} should be refused");
        }
        // and the length ceiling, which is what stops a very long id at all
        assert!(!sound_key(&format!("list-{}", "a".repeat(60))));
    }

    /// The imported sound has to be the one that plays. Every reader takes the
    /// first extension in SOUND_EXTS that exists, and the import used to write
    /// its file beside whatever was already there.
    #[test]
    fn an_imported_sound_replaces_the_one_the_key_already_had() {
        let dir = std::env::temp_dir().join(format!("hs-tracker-sound-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("satanic.mp3"), b"the old one").unwrap();

        let imported = ExportedSound {
            ext: "wav".into(),
            data: base64::engine::general_purpose::STANDARD.encode(b"the imported one"),
        };
        write_sound(&dir, "satanic", &imported).unwrap();

        assert!(!dir.join("satanic.mp3").exists(), "mp3 comes first and would shadow the wav");
        assert_eq!(std::fs::read(dir.join("satanic.wav")).unwrap(), b"the imported one");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// settings.json is a plain file on disk and the code says so twice. When
    /// one comes back unparseable — hand-edited, or half-written by a power cut
    /// — answering with defaults means the next lock toggle writes those
    /// defaults over the only copy of every filter and list the user has.
    /// A settings file written before relics existed opens without complaint.
    ///
    /// The 22 keys below are exactly the ones in the owner's own 0.9.x file,
    /// which is still on disk at `src-tauri/target/release/settings.json` — no
    /// `zone`, no `zone_buffs`, no `theme`, no `flourish_*`, and now no `relic`
    /// or `relics` either. `#[serde(default)]` on the struct is what carries
    /// it, and the thing worth asserting is not that it parses but WHAT the
    /// missing relic fields come back as: an empty hunt list is silence, so an
    /// upgrade must not start chiming at a player who never picked a relic.
    #[test]
    fn a_settings_file_from_before_relics_opens_and_hunts_nothing() {
        let old = r#"{
            "satanic": {"enabled": true, "volume": 0.02},
            "set": {"enabled": true, "volume": 0.02},
            "heroic": {"enabled": true, "volume": 0.18},
            "angelic": {"enabled": true, "volume": 0.2},
            "unholy": {"enabled": true, "volume": 0.24},
            "mail": {"enabled": true, "volume": 0.23},
            "alerts": ["Satanic", "Set", "Heroic", "Angelic", "Unholy"],
            "min_tier": 0, "notable": [], "filters": [], "filter": "",
            "use_filter": true, "locked": false, "opacity": 1.0, "scale": 1.0,
            "auto_show": true, "autostart": false, "ticker": false,
            "debug_log": false, "compact": false, "hidden": ["vitals"],
            "sound_on_ground": true
        }"#;
        let settings: Settings = serde_json::from_str(old).expect("an old file still parses");
        assert!(settings.relics.is_empty(), "no relic is hunted by an upgrade");
        assert!(settings.relic.enabled, "the switch is on, which costs nothing while the list is empty");
        assert_eq!(settings.unholy.volume, 0.24, "and the settings it did carry are untouched");

        // The engine has to agree, not just the file: an empty pick reaches
        // `Prefs` as an empty pick, and `hunted_relic` answers None to every
        // relic there is.
        assert!(
            crate::stats::Prefs::default().relics.is_empty(),
            "and the default the engine starts from hunts nothing either"
        );
    }

    /// A filter saved before categories existed opens meaning what it meant.
    ///
    /// This is the thing a shipping product cannot get wrong. The list below is
    /// the shape 1.0.3 wrote: an id, a name, a tick, a volume and some items,
    /// and no `rules` key at all. It has to load with no rules — never with one
    /// that matches something — and the names it does hold have to survive the
    /// trip through `apply_stats_settings` untouched.
    #[test]
    fn a_filter_from_before_categories_opens_with_none_of_them() {
        let old = r#"{
            "use_filter": true,
            "filter": "f1",
            "filters": [{
                "id": "f1",
                "name": "Chase",
                "lists": [{
                    "id": "abc123",
                    "name": "Vaults",
                    "enabled": true,
                    "volume": 0.7,
                    "items": ["Essence Vault (Angelic)", "AK-47"]
                }]
            }]
        }"#;
        let settings: Settings = serde_json::from_str(old).expect("a 1.0.3 filter still parses");
        let list = &settings.filters[0].lists[0];
        assert!(list.rules.is_empty(), "no category is invented for it");
        assert_eq!(list.items.len(), 2, "and the names it does hold are all there");

        // Round-tripping it must not put a `rules` key in front of an older
        // build either — an empty vector serialises as `[]`, which older serde
        // ignores, but the names are what matter and they come back verbatim.
        let back: Settings =
            serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(back.filters[0].lists[0].items, list.items);
    }

    /// A rule that names neither a rarity nor a type is not a category.
    ///
    /// It would match every named drop in the game, and a list that swallowed
    /// everything reads as the app being broken rather than as a setting. The
    /// picker cannot build one; a hand-edited or imported file can, so the
    /// refusal lives where every settings file passes.
    #[test]
    fn a_rule_that_narrows_nothing_is_refused() {
        assert!(engine_rule(&SoundRule::default()).is_none(), "any rarity, any type: not a rule");
        assert!(
            engine_rule(&SoundRule { rarity: Some("  ".into()), ..Default::default() }).is_none(),
            "and a blank rarity is no rarity"
        );

        let rarity = engine_rule(&SoundRule { rarity: Some("Satanic".into()), ..Default::default() })
            .expect("a rarity on its own is a category");
        assert_eq!(rarity.rarity.as_deref(), Some("satanic"), "lowercased for the engine");
        assert!(rarity.item_type.is_none());

        // A weapon type numbers the kinds inside item type 3 and means nothing
        // without one — 6 is Polearm there and nothing at all anywhere else.
        let stray = engine_rule(&SoundRule {
            rarity: Some("Set".into()),
            item_type: None,
            weapon: Some(6),
        })
        .expect("the rarity still makes it a category");
        assert!(stray.weapon.is_none(), "the weapon type is dropped with no item type to hold it");
    }

    #[test]
    fn a_file_that_will_not_parse_is_kept_rather_than_answered_with_defaults() {
        let dir = std::env::temp_dir().join(format!("hs-tracker-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        let wreck = r#"{"min_tier": 5, "alerts": ["Sat"#;
        std::fs::write(&path, wreck).unwrap();

        let settings: Settings = read_json_or_default(&path);
        assert_eq!(settings.min_tier, Settings::default().min_tier, "defaults, as before");
        assert!(!path.exists(), "and not left where the next save overwrites it");
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.json.bad")).unwrap(),
            wreck,
            "the user's own file, kept whole"
        );

        // a file that is simply not there is a first run, and stays quiet
        let fresh: Settings = read_json_or_default(&dir.join("nothing.json"));
        assert_eq!(fresh.min_tier, Settings::default().min_tier);
        assert!(!dir.join("nothing.json.bad").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
