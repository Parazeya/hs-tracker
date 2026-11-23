//! Read-only loopback sources rendered by OBS, independent of desktop windows.
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

pub const DEFAULT_PORT: u16 = 17265;
const MAX_WORKERS: usize = 40;
const MAX_CLIENTS: usize = 8;
const MAX_HEADER: usize = 8192;
const FRAME_GAP: Duration = Duration::from_millis(33);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone, Serialize)]
struct Frame {
    id: u64,
    topic: String,
    payload: Value,
    at_ms: u64,
}
#[derive(Default)]
struct State {
    sequence: u64,
    topics: BTreeMap<String, Frame>,
    notifications: Vec<Value>,
    drops: Vec<Value>,
}
struct Server {
    port: u16,
    stop: Arc<AtomicBool>,
    listener: std::thread::JoinHandle<()>,
}
impl Server {
    fn stop(self) {
        self.stop.store(true, Ordering::Relaxed);
        // A stopped flag is not a released socket. Join the accept loop before
        // rebinding, even when the next configuration restores the same port.
        let _ = self.listener.join();
    }
}
struct Hub {
    epoch: String,
    state: Mutex<State>,
    changed: Condvar,
    server: Mutex<Option<Server>>,
    enabled: AtomicBool,
    map_needed: AtomicBool,
    port: AtomicU16,
    clients: AtomicUsize,
    workers: AtomicUsize,
    error: Mutex<Option<String>>,
}
impl Hub {
    fn new() -> Self {
        Self {
            epoch: format!(
                "{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ),
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            server: Mutex::new(None),
            enabled: AtomicBool::new(false),
            map_needed: AtomicBool::new(false),
            port: AtomicU16::new(DEFAULT_PORT),
            clients: AtomicUsize::new(0),
            workers: AtomicUsize::new(0),
            error: Mutex::new(None),
        }
    }
    fn publish(&self, topic: &str, payload: Value) {
        let mut state = lock(&self.state);
        state.sequence += 1;
        let frame = Frame {
            id: state.sequence,
            topic: topic.into(),
            payload,
            at_ms: now_ms(),
        };
        state.topics.insert(topic.into(), frame);
        drop(state);
        self.changed.notify_all();
    }
    fn frames_after(&self, id: u64) -> Vec<Frame> {
        let state = lock(&self.state);
        let mut frames = state
            .topics
            .values()
            .filter(|f| f.id > id)
            .cloned()
            .collect::<Vec<_>>();
        frames.sort_by_key(|f| f.id);
        frames
    }
}
fn hub() -> &'static Arc<Hub> {
    static HUB: OnceLock<Arc<Hub>> = OnceLock::new();
    HUB.get_or_init(|| Arc::new(Hub::new()))
}

#[derive(Clone, Serialize)]
pub struct Status {
    pub enabled: bool,
    pub running: bool,
    pub port: u16,
    pub base_url: String,
    pub clients: usize,
    pub error: Option<String>,
    pub map: Option<Value>,
}
pub fn status() -> Status {
    let hub = hub();
    let port = hub.port.load(Ordering::Relaxed);
    Status {
        enabled: hub.enabled.load(Ordering::Relaxed),
        running: lock(&hub.server)
            .as_ref()
            .is_some_and(|server| !server.listener.is_finished()),
        port,
        base_url: format!("http://127.0.0.1:{port}"),
        clients: hub.clients.load(Ordering::Relaxed),
        error: lock(&hub.error).clone(),
        map: lock(&hub.state)
            .topics
            .get("map")
            .map(|f| json!({"revision": f.id, "state": f.payload})),
    }
}
pub fn has_clients() -> bool {
    hub().enabled.load(Ordering::Relaxed) && hub().clients.load(Ordering::Relaxed) > 0
}

/// Settings are projected explicitly. A browser source never receives paths,
/// custom audio, filter rules, exported settings, or a native command bridge.
pub fn configure(app: &AppHandle, settings: &crate::Settings) {
    let hub = hub();
    hub.enabled
        .store(settings.stream_enabled, Ordering::Relaxed);
    hub.port.store(
        if settings.stream_port == 0 {
            DEFAULT_PORT
        } else {
            settings.stream_port
        },
        Ordering::Relaxed,
    );
    hub.publish(
        "appearance",
        json!({"theme":settings.theme,"accent":settings.accent,
        "language":settings.language,"slots":settings.overlay_slots,"hidden":settings.hidden,
        "looks":settings.looks,"notification_secs":settings.flourish_secs.clamp(2.0,12.0)}),
    );
    if let Err(error) = ensure_server(app) {
        *lock(&hub.error) = Some(error);
    }
    let _ = app.emit("stream-status-changed", status());
    hub.changed.notify_all();
}
pub fn publish_snapshot(snapshot: &crate::stats::Snapshot) {
    if hub().enabled.load(Ordering::Relaxed) {
        if let Ok(value) = serde_json::to_value(snapshot) {
            hub().publish("snapshot", value);
        }
    }
}

fn add_timed(hub: &Hub, topic: &str, mut payload: Value, duration_ms: u64) {
    let now = now_ms();
    let mut state = lock(&hub.state);
    let id = state.sequence + 1;
    payload["stream_id"] = json!(id);
    let entries = if topic == "drops" {
        &mut state.drops
    } else {
        &mut state.notifications
    };
    entries.retain(|entry| entry["until_ms"].as_u64().unwrap_or(0) > now);
    if topic == "notifications" && entries.len() >= 6 {
        return;
    }
    let start = if topic == "notifications" {
        entries
            .last()
            .and_then(|e| e["until_ms"].as_u64())
            .unwrap_or(now)
            .max(now)
    } else {
        now
    };
    payload["start_ms"] = json!(start);
    payload["until_ms"] = json!(start + duration_ms);
    entries.push(payload);
    if entries.len() > 6 {
        entries.remove(0);
    }
    let payload = json!(entries);
    state.sequence = id;
    state.topics.insert(
        topic.into(),
        Frame {
            id,
            topic: topic.into(),
            payload,
            at_ms: now,
        },
    );
    drop(state);
    hub.changed.notify_all();
}
pub fn publish_drop(drop: &crate::stats::DropEntry, suppress_notification: bool) {
    let hub = hub();
    if !hub.enabled.load(Ordering::Relaxed) {
        return;
    }
    let Ok(value) = serde_json::to_value(drop) else {
        return;
    };
    if drop.announce {
        add_timed(hub, "drops", value.clone(), 8000);
    }
    if drop.flourish && !suppress_notification {
        publish_notification(value);
    }
}
pub fn publish_notification(value: Value) {
    let hub = hub();
    if !hub.enabled.load(Ordering::Relaxed) {
        return;
    }
    let secs = lock(&hub.state)
        .topics
        .get("appearance")
        .and_then(|f| f.payload["notification_secs"].as_f64())
        .unwrap_or(6.0);
    add_timed(
        hub,
        "notifications",
        value,
        (secs.clamp(2.0, 12.0) * 1000.0) as u64,
    );
}

/// This is data ingress from a local, capability-limited shell, never HTTP.
pub fn publish_map(value: Value) -> Result<(), String> {
    validate_map(&value)?;
    hub().publish("map", value);
    Ok(())
}
pub fn validate_map(value: &Value) -> Result<(), String> {
    let invalid = || "Invalid map view state".to_string();
    let object = value.as_object().ok_or_else(invalid)?;
    if object.len() != 4
        || !["camera", "viewport", "selection", "presentation"]
            .iter()
            .all(|key| object.contains_key(*key))
    {
        return Err(invalid());
    }
    let camera = value["camera"].as_object().ok_or_else(invalid)?;
    let center = value["camera"]["center"].as_array().ok_or_else(invalid)?;
    let number = |v: &Value, lo: f64, hi: f64| {
        v.as_f64()
            .is_some_and(|n| n.is_finite() && (lo..=hi).contains(&n))
    };
    if camera.len() != 2
        || center.len() != 2
        || !center.iter().all(|n| number(n, -4.0, 4.0))
        || !number(&value["camera"]["scale"], 0.000001, 1024.0)
    {
        return Err(invalid());
    }
    let viewport = value["viewport"].as_object().ok_or_else(invalid)?;
    if viewport.len() != 2
        || !number(&value["viewport"]["width"], 1.0, 32768.0)
        || !number(&value["viewport"]["height"], 1.0, 32768.0)
    {
        return Err(invalid());
    }
    let selection = value["selection"].as_object().ok_or_else(invalid)?;
    if selection.len() != 1 || !selection.contains_key("roomId") {
        return Err(invalid());
    }
    let room = &value["selection"]["roomId"];
    if !room.is_null()
        && !room
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
    {
        return Err(invalid());
    }
    let presentation = value["presentation"].as_object().ok_or_else(invalid)?;
    if presentation.len() != 3
        || !["query", "lang", "source"]
            .iter()
            .all(|key| presentation.contains_key(*key))
    {
        return Err(invalid());
    }
    if !value["presentation"]["query"]
        .as_str()
        .is_some_and(|s| s.chars().count() <= 256 && !s.chars().any(char::is_control))
    {
        return Err(invalid());
    }
    if !value["presentation"]["lang"].as_str().is_some_and(|s| {
        [
            "en", "de", "sp", "fi", "fr", "pl", "pt", "ru", "zh", "ja", "ko",
        ]
        .contains(&s)
    }) {
        return Err(invalid());
    }
    let source = &value["presentation"]["source"];
    if !source.is_null()
        && !source
            .as_str()
            .is_some_and(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
    {
        return Err(invalid());
    }
    if !source.is_null() && !room.is_null() {
        return Err(invalid());
    }
    if serde_json::to_vec(value).map_err(|_| invalid())?.len() > 8192 {
        return Err(invalid());
    }
    Ok(())
}

pub fn map_source(app: &AppHandle) -> Result<Status, String> {
    hub().map_needed.store(true, Ordering::Relaxed);
    ensure_server(app)?;
    Ok(status())
}
fn ensure_server(app: &AppHandle) -> Result<(), String> {
    let hub = hub();
    let mut server = lock(&hub.server);
    let wanted = hub.enabled.load(Ordering::Relaxed) || hub.map_needed.load(Ordering::Relaxed);
    let port = hub.port.load(Ordering::Relaxed);
    if server
        .as_ref()
        .is_some_and(|running| !wanted || running.port != port || running.listener.is_finished())
    {
        if let Some(old) = server.take() {
            old.stop.store(true, Ordering::Relaxed);
            hub.changed.notify_all();
            old.stop();
        }
    }
    if !wanted || server.is_some() {
        return Ok(());
    }
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .map_err(|e| format!("Cannot open local streaming port {port}: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let listener_stop = stop.clone();
    let app = app.clone();
    let worker_hub = hub.clone();
    let listener_thread = std::thread::Builder::new()
        .name("obs-loopback".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((socket, peer)) => {
                        if !peer.ip().is_loopback() {
                            continue;
                        }
                        if worker_hub.workers.fetch_add(1, Ordering::Relaxed) >= MAX_WORKERS {
                            worker_hub.workers.fetch_sub(1, Ordering::Relaxed);
                            continue;
                        }
                        let (app, hub, stop) = (app.clone(), worker_hub.clone(), stop.clone());
                        std::thread::spawn(move || {
                            let _ = handle(socket, peer, &app, &hub, &stop, port);
                            hub.workers.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(25))
                    }
                    Err(error) => {
                        *lock(&worker_hub.error) = Some(error.to_string());
                        break;
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    *server = Some(Server {
        port,
        stop: listener_stop,
        listener: listener_thread,
    });
    *lock(&hub.error) = None;
    Ok(())
}

#[derive(Debug)]
struct Request {
    path: String,
    headers: BTreeMap<String, String>,
}
fn read_request(socket: &mut TcpStream) -> Result<Request, String> {
    let mut bytes = Vec::new();
    let mut part = [0; 1024];
    loop {
        let n = socket.read(&mut part).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("Incomplete request".into());
        }
        bytes.extend_from_slice(&part[..n]);
        if bytes.len() > MAX_HEADER {
            return Err("Request too large".into());
        }
        if bytes.windows(4).any(|s| s == b"\r\n\r\n") {
            break;
        }
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "Invalid request")?;
    let mut lines = text.split("\r\n");
    let first = lines.next().unwrap_or("");
    let parts = first.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 3 || parts[0] != "GET" || !matches!(parts[2], "HTTP/1.1" | "HTTP/1.0") {
        return Err("Only GET is supported".into());
    }
    let path = parts[1].split('?').next().unwrap_or("");
    if !safe_path(path) {
        return Err("Invalid path".into());
    }
    let mut headers = BTreeMap::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let (name, value) = line.split_once(':').ok_or("Invalid header")?;
        if headers
            .insert(name.to_ascii_lowercase(), value.trim().into())
            .is_some()
        {
            return Err("Duplicate header".into());
        }
    }
    Ok(Request {
        path: path.into(),
        headers,
    })
}
fn safe_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= 1024
        && !path.split('/').any(|s| s == "." || s == "..")
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
}
fn allowed_request(request: &Request, port: u16) -> bool {
    let host = format!("127.0.0.1:{port}");
    let origin = format!("http://{host}");
    request.headers.get("host") == Some(&host)
        && request
            .headers
            .get("origin")
            .is_none_or(|value| value == &origin)
}
fn response(
    socket: &mut TcpStream,
    code: u16,
    mime: &str,
    body: &[u8],
    port: u16,
) -> std::io::Result<()> {
    let reason = match code {
        200 => "OK",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        _ => "Bad Request",
    };
    write!(socket,"HTTP/1.1 {code} {reason}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: frame-ancestors http://tauri.localhost http://localhost:5176 http://127.0.0.1:{port}; object-src 'none'\r\n\r\n",body.len())?;
    socket.write_all(body)
}
fn handle(
    mut socket: TcpStream,
    _peer: SocketAddr,
    app: &AppHandle,
    hub: &Hub,
    stop: &AtomicBool,
    port: u16,
) -> Result<(), String> {
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    let request = match read_request(&mut socket) {
        Ok(request) => request,
        Err(error) => {
            let _ = response(&mut socket, 400, "text/plain", error.as_bytes(), port);
            return Ok(());
        }
    };
    if !allowed_request(&request, port) {
        let _ = response(&mut socket, 403, "text/plain", b"Local source only", port);
        return Ok(());
    }
    if matches!(request.path.as_str(), "/events" | "/state") {
        if !hub.enabled.load(Ordering::Relaxed) {
            let _ = response(
                &mut socket,
                403,
                "text/plain",
                b"Streaming is disabled",
                port,
            );
            return Ok(());
        }
        if request.path == "/state" {
            let bytes = serde_json::to_vec(
                &json!({"epoch":hub.epoch,"frames":hub.frames_after(0),"server_ms":now_ms()}),
            )
            .unwrap();
            let _ = response(&mut socket, 200, "application/json", &bytes, port);
            return Ok(());
        }
        if hub.clients.fetch_add(1, Ordering::Relaxed) >= MAX_CLIENTS {
            hub.clients.fetch_sub(1, Ordering::Relaxed);
            let _ = response(&mut socket, 429, "text/plain", b"Too many sources", port);
            return Ok(());
        }
        let result = sse(&mut socket, hub, stop);
        hub.clients.fetch_sub(1, Ordering::Relaxed);
        return result.map_err(|e| e.to_string());
    }
    let asset = match request.path.as_str() {
        "/stats" | "/notifications" | "/drops" | "/map" => Some("stream.html".to_string()),
        path if path.starts_with("/assets/") || path.starts_with("/hs-map/") => {
            Some(path.trim_start_matches('/').to_string())
        }
        _ => None,
    };
    if let Some(asset) = asset.and_then(|path| app.asset_resolver().get(path)) {
        response(&mut socket, 200, &asset.mime_type, &asset.bytes, port).map_err(|e| e.to_string())
    } else {
        response(&mut socket, 404, "text/plain", b"Source not found", port)
            .map_err(|e| e.to_string())
    }
}
fn sse(socket: &mut TcpStream, hub: &Hub, stop: &AtomicBool) -> std::io::Result<()> {
    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\nretry: 1000\n\n")?;
    let mut revision = 0;
    let mut heartbeat = std::time::Instant::now();
    while !stop.load(Ordering::Relaxed) && hub.enabled.load(Ordering::Relaxed) {
        let frames = hub.frames_after(revision);
        if !frames.is_empty() {
            revision = frames.last().unwrap().id;
            let data = serde_json::to_string(
                &json!({"epoch":hub.epoch,"frames":frames,"server_ms":now_ms()}),
            )
            .unwrap();
            write!(socket, "id: {revision}\nevent: update\ndata: {data}\n\n")?;
            socket.flush()?;
            heartbeat = std::time::Instant::now();
            // Every topic retains only its latest bounded state. A slow OBS
            // renderer skips obsolete camera positions instead of queueing them.
            std::thread::sleep(FRAME_GAP);
        } else {
            let guard = lock(&hub.state);
            drop(hub.changed.wait_timeout(guard, Duration::from_secs(1)));
            if heartbeat.elapsed() >= Duration::from_secs(5) {
                socket.write_all(b": heartbeat\n\n")?;
                socket.flush()?;
                heartbeat = std::time::Instant::now();
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stopping_a_listener_releases_its_port_before_immediate_rebind() {
        for _ in 0..4 {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let port = listener.local_addr().unwrap().port();
            let stop = Arc::new(AtomicBool::new(false));
            let stopping = stop.clone();
            let (entered, polling) = std::sync::mpsc::sync_channel(0);
            let thread = std::thread::spawn(move || {
                let _socket = listener;
                entered.send(()).unwrap();
                while !stopping.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(25));
                }
            });
            polling.recv().unwrap();
            Server {
                port,
                stop,
                listener: thread,
            }
            .stop();
            let rebound = TcpListener::bind((Ipv4Addr::LOCALHOST, port));
            assert!(rebound.is_ok(), "the old listener still owns {port}");
        }
    }

    #[test]
    fn map_state_is_bounded_and_cannot_carry_commands() {
        let valid = json!({"camera":{"center":[0.3,0.5],"scale":1.25},"viewport":{"width":1920,"height":1080},"selection":{"roomId":"Act_01_01"},"presentation":{"query":"","lang":"en","source":null}});
        assert!(validate_map(&valid).is_ok());
        for bad in [
            json!({"command":"exit"}),
            {
                let mut v = valid.clone();
                v["viewport"]["width"] = json!(0);
                v
            },
            {
                let mut v = valid.clone();
                v["camera"]["center"] = json!([0, 0, 0]);
                v
            },
            {
                let mut v = valid.clone();
                v["selection"]["roomId"] = json!("a".repeat(129));
                v
            },
        ] {
            assert!(validate_map(&bad).is_err())
        }
    }
    #[test]
    fn latest_state_replaces_obsolete_camera_work_and_preserves_bootstrap() {
        let hub = Hub::new();
        hub.publish("snapshot", json!({"gold":1}));
        for i in 0..1000 {
            hub.publish("map", json!({"x":i}));
        }
        let frames = hub.frames_after(0);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].payload["x"], 999);
        assert_eq!(hub.frames_after(1000).len(), 1);
        assert!(hub.frames_after(1001).is_empty());
    }
    #[test]
    fn source_requests_are_local_and_read_only() {
        assert!(!safe_path("/../settings.json"));
        assert!(!safe_path("/%2e%2e/settings.json"));
        assert!(!safe_path("/a\\b"));
        assert!(safe_path("/hs-map/data/source.json"));
        let mut request = Request {
            path: "/state".into(),
            headers: BTreeMap::from([("host".into(), format!("127.0.0.1:{DEFAULT_PORT}"))]),
        };
        assert!(allowed_request(&request, DEFAULT_PORT));
        request
            .headers
            .insert("origin".into(), "https://unrelated.example".into());
        assert!(!allowed_request(&request, DEFAULT_PORT));
        request.headers.remove("origin");
        request
            .headers
            .insert("host".into(), format!("unrelated.example:{DEFAULT_PORT}"));
        assert!(!allowed_request(&request, DEFAULT_PORT));
    }
    #[test]
    fn notification_bootstrap_is_bounded_and_keeps_expiry() {
        let hub = Hub::new();
        for i in 0..100 {
            add_timed(
                &hub,
                "notifications",
                json!({"name":format!("drop{i}")}),
                6000,
            );
        }
        let frames = hub.frames_after(0);
        assert_eq!(frames.len(), 1);
        let entries = frames[0].payload.as_array().unwrap();
        assert_eq!(entries.len(), 6);
        assert_eq!(entries.last().unwrap()["name"], "drop5");
        assert!(entries
            .iter()
            .all(|entry| entry["until_ms"].as_u64().unwrap() > now_ms()));
        for pair in entries.windows(2) {
            assert_eq!(pair[1]["start_ms"], pair[0]["until_ms"]);
        }
    }
}
