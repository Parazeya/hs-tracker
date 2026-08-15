//! The overlay as a page, for OBS.
//!
//! A streamer capturing only the game window does not get the overlay, and a
//! transparent frameless window is not something every OBS capture method
//! handles. Every overlay that works on stream solves this the same way: it
//! serves itself as a web page and the streamer adds a Browser Source, which
//! is a browser and therefore has transparency, scaling and placement for free.
//!
//! The page is the app's own front end — the same Svelte, the same sprites,
//! embedded in this binary already — so what goes on the stream is what is on
//! the screen, not a second implementation of it.
//!
//! It listens on the loopback address and nowhere else: this is a window onto
//! the player's own session, and it has no business being reachable from the
//! network.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::sniffer::Shared;

/// Whether the server should be running, and on which port. A change to either
/// takes effect on the next beat of the accept loop.
static WANTED: AtomicBool = AtomicBool::new(false);
static PORT: AtomicU16 = AtomicU16::new(0);
/// the port a listener is actually standing on, or zero. Wanting a port and
/// having one are different things, and only the second is worth telling a
/// streamer to paste into OBS.
static SERVING: AtomicU16 = AtomicU16::new(0);
/// how often a listening page is sent the current numbers
const BEAT: Duration = Duration::from_millis(500);

/// Drops waiting to be told to the pages. A drop is a moment, not a state, so
/// it cannot ride along with the numbers on the next beat — it is queued here
/// and sent on its own.
static ANNOUNCED: Mutex<Vec<(&'static str, String)>> = Mutex::new(Vec::new());

/// A drop, for any page that is listening. `flourish` is the announcement the
/// window plays; `drop` is the line the ticker adds.
fn queue(kind: &'static str, drop: &crate::stats::DropEntry) {
    let Ok(json) = serde_json::to_string(drop) else { return };
    if let Ok(mut waiting) = ANNOUNCED.lock() {
        // nobody may be listening at all; this is not a backlog to keep
        if waiting.len() > 16 {
            waiting.remove(0);
        }
        waiting.push((kind, json));
    }
}

pub fn announce(drop: &crate::stats::DropEntry) {
    queue("flourish", drop);
}

pub fn ticked(drop: &crate::stats::DropEntry) {
    queue("drop", drop);
}

pub fn configure(on: bool, port: u16) {
    PORT.store(port, Ordering::Relaxed);
    WANTED.store(on, Ordering::Relaxed);
}

pub fn port() -> u16 {
    SERVING.load(Ordering::Relaxed)
}

/// One page listening on the events stream. It stays until its socket refuses
/// a write — a page that is merely slow is still a page.
struct Viewer {
    out: TcpStream,
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let viewers: Arc<Mutex<Vec<Viewer>>> = Arc::new(Mutex::new(Vec::new()));
        pump(app.clone(), viewers.clone());
        let mut bound: Option<(TcpListener, u16)> = None;
        loop {
            let wanted = WANTED.load(Ordering::Relaxed);
            let port = PORT.load(Ordering::Relaxed);
            if !wanted {
                if bound.take().is_some() {
                    SERVING.store(0, Ordering::Relaxed);
                    viewers.lock().unwrap().clear();
                }
            } else if bound.as_ref().is_none_or(|(_, p)| *p != port) {
                match listen(port) {
                    Some(server) => {
                        // the old listener goes only once a new one is standing
                        bound = Some((server, port));
                        SERVING.store(port, Ordering::Relaxed);
                        viewers.lock().unwrap().clear();
                    }
                    None => {
                        // the port is taken; keep serving on the old one rather
                        // than leaving the streamer with nothing
                        eprintln!("stream: port {port} is not free");
                        std::thread::sleep(Duration::from_secs(5));
                    }
                }
            }
            let Some((server, _)) = &bound else {
                std::thread::sleep(Duration::from_millis(400));
                continue;
            };
            match server.accept() {
                Ok((stream, _)) => {
                    let app = app.clone();
                    let viewers = viewers.clone();
                    std::thread::spawn(move || answer(app, stream, viewers));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(120));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(400)),
            }
        }
    });
}

fn listen(port: u16) -> Option<TcpListener> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let server = TcpListener::bind(addr).ok()?;
    server.set_nonblocking(true).ok()?;
    Some(server)
}

/// Sends the current numbers to every page that is listening.
fn pump(app: AppHandle, viewers: Arc<Mutex<Vec<Viewer>>>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(BEAT);
        let mut list = viewers.lock().unwrap();
        if list.is_empty() {
            continue;
        }
        let body = snapshot_json(&app);
        let frame = format!("event: stats\ndata: {body}\n\n");
        list.retain_mut(|v| match v.out.write_all(frame.as_bytes()) {
            Ok(()) => true,
            // a Browser Source that OBS has hidden stops reading; its buffer
            // fills, and that is not the same as it having gone away
            Err(e) => matches!(e.kind(), std::io::ErrorKind::WouldBlock),
        });
    });
}

fn snapshot_json(app: &AppHandle) -> String {
    let shared = app.state::<Shared>();
    let status = shared.status.lock().unwrap().text();
    let snap = shared.stats.lock().unwrap().snapshot(status);
    serde_json::to_string(&snap).unwrap_or_else(|_| "null".into())
}

fn answer(app: AppHandle, mut stream: TcpStream, viewers: Arc<Mutex<Vec<Viewer>>>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Some((path, host)) = request_head(&mut stream) else { return };

    // Binding to loopback keeps other machines out; it does not keep out a page
    // the player is merely visiting. A site can point a name it controls at
    // 127.0.0.1 and have the browser ask us for the settings and the run
    // history — so the name in the request has to be one of ours.
    if !local_host(&host) {
        let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");
        return;
    }
    let route = path.split('?').next().unwrap_or("/");

    match route {
        "/api/events" => {
            let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\
                        Cache-Control: no-store\r\nConnection: keep-alive\r\n\r\n";
            if stream.write_all(head.as_bytes()).is_err() {
                return;
            }
            // the first frame goes out at once, so a source that has just been
            // added is not blank until the next beat
            let first = format!("event: stats\ndata: {}\n\n", snapshot_json(&app));
            if stream.write_all(first.as_bytes()).is_err() {
                return;
            }
            let _ = stream.set_nonblocking(true);
            viewers.lock().unwrap().push(Viewer { out: stream });
        }
        "/api/snapshot" => send(&mut stream, "application/json", snapshot_json(&app).into_bytes()),
        "/api/settings" => {
            let body = serde_json::to_vec(&crate::read_settings()).unwrap_or_else(|_| b"null".into());
            send(&mut stream, "application/json", body);
        }
        "/api/runs" => {
            let body = serde_json::to_vec(&crate::read_runs()).unwrap_or_else(|_| b"[]".into());
            send(&mut stream, "application/json", body);
        }
        _ => {
            // anything else is the app's own front end, served straight out of
            // the binary — no second copy, and never out of step with it
            let asset = if route == "/" { "/index.html" } else { route };
            match app.asset_resolver().get(asset.into()) {
                Some(found) => send(&mut stream, &found.mime_type.clone(), found.bytes),
                None => {
                    let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                }
            }
        }
    }
}

/// The path asked for and the host it was asked of. The rest of the head is of
/// no interest, but all of it has to come off the socket before anything is
/// written back.
fn request_head(stream: &mut TcpStream) -> Option<(String, String)> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let path = parts.next()?.to_string();
    let mut host = String::new();
    let mut header = String::new();
    loop {
        header.clear();
        match reader.read_line(&mut header) {
            Ok(0) => break,
            Ok(_) if header.trim().is_empty() => break,
            Ok(_) if header.len() > 8192 => break,
            Ok(_) => {
                if let Some(value) = header.strip_prefix("Host:").or_else(|| header.strip_prefix("host:")) {
                    host = value.trim().to_string();
                }
            }
            Err(_) => break,
        }
    }
    Some((path, host))
}

/// Only this machine, asked for by an address that means this machine.
fn local_host(host: &str) -> bool {
    let name = host.rsplit_once(':').map_or(host, |(name, _)| name);
    let name = name.trim_matches(['[', ']']);
    matches!(name, "127.0.0.1" | "localhost" | "::1" | "")
}

fn send(stream: &mut TcpStream, mime: &str, body: Vec<u8>) {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
    let _ = stream.flush();
}
