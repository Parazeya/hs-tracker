//! The map has local window controls and an isolated remote iframe.
//! All lifecycle actions share a worker lock, so a burst of shortcuts can
//! never race two builders or leave a stale visibility flag behind.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

pub const LABEL: &str = "world-map";
pub const HOTKEY: &str = "ctrl+shift+m";
pub static HOTKEY_REGISTERED: AtomicBool = AtomicBool::new(false);
static KEY_DOWN: AtomicBool = AtomicBool::new(false);
static WINDOW_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Request { Toggle, Show, Hide }
#[derive(Clone, Copy, Debug, PartialEq)]
enum Action { Create, Show, Hide, Nothing }

fn action(visible: Option<bool>, request: Request) -> Action {
    match (visible, request) {
        (None, Request::Hide) => Action::Nothing,
        (None, _) => Action::Create,
        (Some(true), Request::Toggle | Request::Hide) => Action::Hide,
        (Some(false), Request::Hide) => Action::Nothing,
        (Some(_), _) => Action::Show,
    }
}

#[cfg(windows)]
fn active_center() -> Option<(f64, f64)> {
    #[repr(C)]
    #[derive(Default)]
    struct Rect { left: i32, top: i32, right: i32, bottom: i32 }
    #[link(name="user32")]
    extern "system" {
        fn GetForegroundWindow() -> *mut std::ffi::c_void;
        fn GetWindowRect(window: *mut std::ffi::c_void, rect: *mut Rect) -> i32;
    }
    let mut rect = Rect::default();
    unsafe {
        let window = GetForegroundWindow();
        if window.is_null() || GetWindowRect(window, &mut rect) == 0 { return None; }
    }
    Some(((rect.left as f64 + rect.right as f64) / 2.0, (rect.top as f64 + rect.bottom as f64) / 2.0))
}
#[cfg(not(windows))]
fn active_center() -> Option<(f64, f64)> { None }

fn contains(position: (i32, i32), size: (u32, u32), point: (f64, f64)) -> bool {
    point.0 >= position.0 as f64 && point.1 >= position.1 as f64
        && point.0 < position.0 as f64 + size.0 as f64
        && point.1 < position.1 as f64 + size.1 as f64
}

fn apply(app: &AppHandle, request: Request) -> Result<(), String> {
    let existing = app.get_webview_window(LABEL);
    let visible = existing.as_ref().map(|window| window.is_visible()).transpose().map_err(|e| e.to_string())?;
    match action(visible, request) {
        Action::Nothing => return Ok(()),
        Action::Hide => {
            let window=existing.unwrap();
            // Hiding only the HWND leaves WebView2's document visible and its
            // remote map animating. Controller visibility suspends those frames.
            let view: &tauri::Webview<_> = window.as_ref();
            view.hide().map_err(|e| e.to_string())?;
            return window.hide().map_err(|e| e.to_string());
        },
        _ => {},
    }
    // Capture the game's monitor before the new window takes focus. On X11
    // the pointer is the available cross-platform indication of the screen.
    let point = active_center().or_else(|| app.cursor_position().ok().map(|p| (p.x, p.y)));
    let monitor = app.available_monitors().ok().and_then(|monitors| {
        point.and_then(|point| monitors.into_iter().find(|m|
            contains((m.position().x, m.position().y), (m.size().width, m.size().height), point)))
    });
    let window = match existing {
        Some(window) => window,
        None => WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("index.html".into()))
            .title("HS Tracker — World map")
            .inner_size(1280.0, 800.0)
            .decorations(false)
            .transparent(false)
            .always_on_top(true)
            .visible(false)
            .build().map_err(|e| e.to_string())?,
    };
    window.set_fullscreen(false).map_err(|e| e.to_string())?;
    if let Some(monitor) = monitor {
        window.set_position(*monitor.position()).map_err(|e| e.to_string())?;
        window.set_size(*monitor.size()).map_err(|e| e.to_string())?;
    }
    window.set_fullscreen(true).map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    let view: &tauri::Webview<_> = window.as_ref();
    view.show().map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

pub fn request(app: &AppHandle, request: Request) -> Result<(), String> {
    let handle = app.clone();
    // WebView2 creation may wait for a message dispatched to the UI thread.
    // Building directly inside its event/IPC callback can block that message.
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = WINDOW_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        if let Err(error) = apply(&handle, request) {
            crate::log::error(format!("world map: {error}"));
            let _ = handle.emit("map-window-error", error);
        }
    });
    Ok(())
}

pub fn hotkey_registered() -> bool { HOTKEY_REGISTERED.load(Ordering::Relaxed) }

pub fn shortcut(app: &AppHandle, pressed: bool) {
    if pressed && !KEY_DOWN.swap(true, Ordering::Relaxed) {
        let _ = request(app, Request::Toggle);
    } else if !pressed { KEY_DOWN.store(false, Ordering::Relaxed); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_toggle_reuses_the_window_and_close_only_hides() {
        assert_eq!(action(None, Request::Toggle), Action::Create);
        assert_eq!(action(Some(true), Request::Toggle), Action::Hide);
        assert_eq!(action(Some(false), Request::Toggle), Action::Show);
        assert_eq!(action(Some(true), Request::Hide), Action::Hide);
        assert_eq!(action(Some(false), Request::Hide), Action::Nothing);
        assert_eq!(action(None, Request::Hide), Action::Nothing);
        assert_eq!(action(Some(true), Request::Show), Action::Show);
    }
    #[test]
    fn current_screen_selection_handles_negative_coordinates_and_edges() {
        assert!(contains((-1920, 0), (1920, 1080), (-1500.0, 600.0)));
        assert!(!contains((-1920, 0), (1920, 1080), (0.0, 600.0)));
        assert!(contains((0, 0), (2560, 1440), (0.0, 600.0)));
    }
}
