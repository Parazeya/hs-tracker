//! Serve the packaged world map on a separate origin from the Tracker webview.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::AppHandle;

const MAX_WORKERS: usize = 12;
const MAX_HEADER: usize = 8192;
static SERVER: Mutex<Option<(u16, JoinHandle<()>)>> = Mutex::new(None);
static WORKERS: AtomicUsize = AtomicUsize::new(0);

pub fn source(app: &AppHandle) -> Result<String, String> {
    let mut server = SERVER.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((port, thread)) = &*server {
        if !thread.is_finished() {
            return Ok(format!("http://127.0.0.1:{port}"));
        }
    }

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let app = app.clone();
    let thread = thread::Builder::new()
        .name("map-assets".into())
        .spawn(move || {
            for socket in listener.incoming() {
                let socket = match socket {
                    Ok(socket) => socket,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        crate::log::warn(format!("Map asset listener: {e}"));
                        break;
                    }
                };
                if WORKERS.fetch_add(1, Ordering::Relaxed) >= MAX_WORKERS {
                    WORKERS.fetch_sub(1, Ordering::Relaxed);
                    continue;
                }
                let app = app.clone();
                if thread::Builder::new()
                    .spawn(move || {
                        let _ = handle(socket, &app, port);
                        WORKERS.fetch_sub(1, Ordering::Relaxed);
                    })
                    .is_err()
                {
                    WORKERS.fetch_sub(1, Ordering::Relaxed);
                }
            }
        })
        .map_err(|e| e.to_string())?;
    *server = Some((port, thread));
    Ok(format!("http://127.0.0.1:{port}"))
}

struct Request {
    path: String,
    headers: BTreeMap<String, String>,
}

fn read_request(socket: &mut TcpStream) -> Result<Request, &'static str> {
    let mut bytes = Vec::new();
    let mut part = [0; 1024];
    let end = loop {
        let n = socket.read(&mut part).map_err(|_| "Cannot read request")?;
        if n == 0 {
            return Err("Incomplete request");
        }
        bytes.extend_from_slice(&part[..n]);
        if bytes.len() > MAX_HEADER {
            return Err("Request too large");
        }
        if let Some(end) = bytes.windows(4).position(|s| s == b"\r\n\r\n") {
            break end;
        }
    };
    let text = std::str::from_utf8(&bytes[..end]).map_err(|_| "Invalid request")?;
    let mut lines = text.split("\r\n");
    let mut parts = lines.next().unwrap_or("").split_whitespace();
    let (Some("GET"), Some(target), Some("HTTP/1.1" | "HTTP/1.0"), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err("Only GET is supported");
    };
    let path = target.split('?').next().unwrap_or("");
    if !path.starts_with('/')
        || path.len() > 1024
        || path.split('/').any(|s| s == "." || s == "..")
        || !path.bytes().all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
    {
        return Err("Invalid path");
    }
    let mut headers = BTreeMap::new();
    for line in lines {
        let (name, value) = line.split_once(':').ok_or("Invalid header")?;
        if headers.insert(name.to_ascii_lowercase(), value.trim().into()).is_some() {
            return Err("Duplicate header");
        }
    }
    Ok(Request { path: path.into(), headers })
}

fn response(socket: &mut TcpStream, status: &str, mime: &str, body: &[u8]) -> io::Result<()> {
    write!(
        socket,
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\n\
         Connection: close\r\nX-Content-Type-Options: nosniff\r\n\
         Content-Security-Policy: frame-ancestors http://tauri.localhost http://localhost:5176 tauri://localhost; object-src 'none'\r\n\r\n",
        body.len()
    )?;
    socket.write_all(body)
}

fn handle(mut socket: TcpStream, app: &AppHandle, port: u16) -> io::Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(2)))?;
    socket.set_write_timeout(Some(Duration::from_secs(3)))?;
    let request = match read_request(&mut socket) {
        Ok(request) => request,
        Err(error) => return response(&mut socket, "400 Bad Request", "text/plain", error.as_bytes()),
    };
    let host = format!("127.0.0.1:{port}");
    if request.headers.get("host") != Some(&host)
        || request.headers.get("origin").is_some_and(|origin| origin != &format!("http://{host}"))
    {
        return response(&mut socket, "403 Forbidden", "text/plain", b"Local map only");
    }
    let asset = request.path.strip_prefix("/hs-map/").and_then(|path| {
        app.asset_resolver().get(format!("hs-map/{path}"))
    });
    match asset {
        Some(asset) => response(&mut socket, "200 OK", &asset.mime_type, &asset.bytes),
        None => response(&mut socket, "404 Not Found", "text/plain", b"Map asset not found"),
    }
}
