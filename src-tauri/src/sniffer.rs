use std::collections::{BTreeSet, HashMap};
use std::net::IpAddr;
#[cfg(windows)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use etherparse::{NetSlice, SlicedPacket, TransportSlice};
use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::Emitter;

use crate::parser::{self, Reassembler};
use crate::stats::GameStats;

#[derive(Clone, PartialEq)]
pub enum Status {
    /// no way to capture at all: Npcap absent on Windows, libpcap refusing to
    /// hand out a device elsewhere — which on Linux usually means the binary
    /// lacks cap_net_raw rather than that the library is missing
    NoCapture,
    NoInterface,
    WaitingForGame,
    Capturing { iface: String, hosts: usize, dropped: u32 },
}

impl Status {
    pub fn text(&self) -> String {
        match self {
            #[cfg(windows)]
            Status::NoCapture => "npcap-missing".into(),
            #[cfg(not(windows))]
            Status::NoCapture => "no-capture".into(),
            Status::NoInterface => "no-interface".into(),
            Status::WaitingForGame => "waiting-for-game".into(),
            Status::Capturing { iface, hosts, dropped } => format!("capturing|{iface}|{hosts}|{dropped}"),
        }
    }
}

/// Whether Hero Siege is up. The watcher already looks for the process every
/// second; anything else that needs to know reads it here rather than looking
/// again.
static GAME_UP: AtomicBool = AtomicBool::new(false);

pub fn game_running() -> bool {
    GAME_UP.load(Ordering::Relaxed)
}

struct Capture {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
    iface: String,
    /// the filter this capture was built with; it changes when the game's own
    /// addresses do, and the capture is then restarted
    scope: String,
    dropped: Arc<AtomicU32>,
    /// messages this adapter has produced; one that yields nothing is dropped
    /// again, so the usual case costs a single capture
    hits: Arc<AtomicU32>,
    /// set once the device is open and filtered — "the thread has not ended"
    /// is also true of one that is about to fail, and that read the status
    /// green on every spawn
    opened: Arc<AtomicBool>,
    started: std::time::Instant,
}

pub struct Shared {
    pub stats: Arc<Mutex<GameStats>>,
    pub status: Arc<Mutex<Status>>,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            stats: Arc::new(Mutex::new(GameStats::default())),
            status: Arc::new(Mutex::new(Status::WaitingForGame)),
        }
    }
}

#[cfg(windows)]
fn npcap_dir() -> PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    PathBuf::from(root).join("System32").join("Npcap")
}

/// Windows carries the capture driver as a separate install, so its absence is
/// worth reporting before anything else is attempted.
#[cfg(windows)]
pub fn capture_available() -> bool {
    npcap_dir().join("wpcap.dll").exists()
        || npcap_dir().parent().is_some_and(|s| s.join("wpcap.dll").exists())
}

/// Elsewhere libpcap is a package dependency and listing devices needs no
/// privileges — but *opening* one does, and that is exactly what is missing on
/// a fresh install without `cap_net_raw`, or after any rebuild, since the
/// capability lives on the inode and every relink drops it.
///
/// This used to be left to the capture threads, which only ever run while the
/// game does — so a machine with no capture rights at all sat on a friendly
/// blue "waiting for Hero Siege" and never said the one thing that was wrong.
/// One device is opened here and closed again to find out.
#[cfg(not(windows))]
pub fn capture_available() -> bool {
    let Some(dev) = capture_devices().into_iter().next() else {
        return true; // nothing to test against; the threads will say so
    };
    let name = dev.name.clone();
    match pcap::Capture::from_device(dev).and_then(|c| c.immediate_mode(true).timeout(50).open()) {
        Ok(_) => true,
        Err(e) => {
            let refused = denied_open(&e);
            crate::log::once(
                "capture-probe",
                "warn",
                format!(
                    "cannot open {name} for capture: {e}{}",
                    if refused { " - the binary needs cap_net_raw" } else { "" }
                ),
            );
            !refused
        }
    }
}

/// wpcap.dll is delay-loaded; make sure the loader can find it.
#[cfg(windows)]
pub fn prepare_capture() {
    let dir = npcap_dir();
    if dir.exists() {
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{};{}", dir.display(), path));
    }
}

#[cfg(not(windows))]
pub fn prepare_capture() {}

fn game_pids(sys: &mut System) -> Vec<u32> {
    // Only names are wanted here. The default refresh also reads memory, io and
    // the executable path of every process on the box, three times a second,
    // for the whole time the game is not running.
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::OnlyIfNotSet),
    );
    let looks_like_it = |s: &str| {
        let flat: String =
            s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
        flat.starts_with("herosiege")
    };
    sys.processes()
        .iter()
        .filter(|(_, p)| {
            // The comm is enough on Windows and for a native build. Behind a
            // Steam launch wrapper or Proton the recognisable name is on the
            // executable path or the command line instead, and matching the
            // comm alone would find nothing at all.
            looks_like_it(&p.name().to_string_lossy())
                || p.exe()
                    .and_then(|e| e.file_name())
                    .is_some_and(|f| looks_like_it(&f.to_string_lossy()))
                || p.cmd().first().is_some_and(|a| {
                    std::path::Path::new(a)
                        .file_name()
                        .is_some_and(|f| looks_like_it(&f.to_string_lossy()))
                })
        })
        .map(|(pid, _)| pid.as_u32())
        .collect()
}

/// Both ends of every connection the game holds. The local side decides which
/// adapters to watch — with split tunnelling the game talks over the VPN and
/// over the LAN at the same time, and one adapter would only show half of it.
/// The remote side is for the status line.
/// `::ffff:10.8.1.8` and `10.8.1.8` are the same address, and only one of them
/// can be written into a packet filter that will ever match.
///
/// A Linux build of the game opens IPv6 sockets and talks IPv4 over them, so
/// every endpoint arrives v4-mapped. Left that way, `scope_for` produced
/// `host ::ffff:10.8.1.8`, libpcap compiled it as an IPv6 test, and no packet
/// on the wire — all of them plain IPv4 — could satisfy it. The capture stayed
/// up, the counters stayed at zero, and nothing anywhere said why.
fn unmap(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    }
}

fn game_endpoints(pids: &[u32]) -> (BTreeSet<IpAddr>, BTreeSet<IpAddr>) {
    let (mut local, mut remote) = (BTreeSet::new(), BTreeSet::new());
    if pids.is_empty() {
        return (local, remote);
    }
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    if let Ok(sockets) = netstat2::get_sockets_info(af, ProtocolFlags::TCP) {
        for s in sockets {
            if !s.associated_pids.iter().any(|p| pids.contains(p)) {
                continue;
            }
            if let ProtocolSocketInfo::Tcp(t) = &s.protocol_socket_info {
                let (near, far) = (unmap(t.local_addr), unmap(t.remote_addr));
                if far.is_unspecified() || far.is_loopback() {
                    continue;
                }
                remote.insert(far);
                if !near.is_unspecified() && !near.is_loopback() {
                    local.insert(near);
                }
            }
        }
    }
    (local, remote)
}

/// Every adapter worth listening on. A split-tunnel engine (WireSock and the
/// like) implements the tunnel in user space and re-injects packets, so the
/// game's traffic can surface on the physical adapter, on the tunnel adapter,
/// or on both — picking one by address misses half of it.
fn capture_devices() -> Vec<pcap::Device> {
    pcap::Device::list()
        .unwrap_or_default()
        .into_iter()
        .filter(|d| d.addresses.iter().any(|a| !a.addr.is_loopback()))
        .collect()
}

/// Where the IP header starts in a captured frame. Ethernet can carry one or
/// two VLAN tags before the ethertype that matters.
fn ip_offset(data: &[u8], framing: i32) -> Option<usize> {
    match framing {
        1 => {
            let mut at = 12;
            for _ in 0..3 {
                let ty = u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?]);
                match ty {
                    0x8100 | 0x88a8 | 0x9100 => at += 4,
                    0x0800 | 0x86dd => return Some(at + 2),
                    _ => return None,
                }
            }
            None
        }
        0 | 108 => Some(4),
        _ => Some(0),
    }
}

/// A frame the adapter has not cut up yet, with its length field rewritten to
/// describe what was actually captured.
///
/// With Large Send Offload the stack hands the adapter one buffer — up to 64 KB
/// — and the adapter segments it on the way out. A capture sits above that, so
/// it sees the whole buffer while the length field still describes a single
/// segment, or nothing at all. Measured against etherparse 0.16 with the two
/// shapes that occur: a total length of 0 fails the parse outright and the frame
/// is dropped on the floor, and a total length of one MSS returns that many
/// bytes and silently discards the rest.
///
/// Either way no message longer than one segment survives. The character save
/// is about 5 KB and is the only carrier of experience and kills, which is why
/// those were the two counters stuck at zero — but every message over one MSS
/// is affected, and a fair share of a session's inventory syncs are.
///
/// `None` when nothing needs doing, which is almost always.
fn unoffload(data: &[u8], ip_start: usize) -> Option<Vec<u8>> {
    let here = data.len().checked_sub(ip_start)?;
    let version = data.get(ip_start)? >> 4;
    // Where the length lives, and what it would have to say to describe this
    // frame. IPv6 counts from the end of its fixed 40-byte header; IPv4 counts
    // the header in.
    let (at, declared, want) = match version {
        4 => {
            let d = u16::from_be_bytes([*data.get(ip_start + 2)?, *data.get(ip_start + 3)?]) as usize;
            (ip_start + 2, d, here)
        }
        6 => {
            let d = u16::from_be_bytes([*data.get(ip_start + 4)?, *data.get(ip_start + 5)?]) as usize;
            (ip_start + 4, d + 40, here.checked_sub(40)?)
        }
        _ => return None,
    };
    // A short frame is padded out to the 60 bytes ethernet insists on, so bytes
    // past the declared end are ordinary there and must NOT be taken for
    // payload. That is the only case, and it can only happen in a frame of 60
    // bytes or fewer — so the test is the frame's size, not how far it
    // overshoots. Allowing 64 bytes of overshoot anywhere, as this did, left a
    // band in which a genuinely offloaded buffer was still quietly truncated.
    let padded = data.len() <= 60;
    let offloaded = declared == 0 || (here > declared && !padded);
    if !offloaded || want > u16::MAX as usize {
        return None;
    }
    let mut patched = data.to_vec();
    patched[at..at + 2].copy_from_slice(&(want as u16).to_be_bytes());
    Some(patched)
}

/// The filter every capture shares: the addresses the game is actually using,
/// or everything while it has not connected yet.
fn scope_for(local: &BTreeSet<IpAddr>) -> String {
    if local.is_empty() {
        return "tcp".into();
    }
    local.iter().map(|ip| format!("host {ip}")).collect::<Vec<_>>().join(" or ")
}

pub fn spawn(shared: &Shared, app: tauri::AppHandle) {
    let stats = shared.stats.clone();
    let status = shared.status.clone();
    std::thread::spawn(move || watcher(stats, status, app));
}

fn set_status(status: &Arc<Mutex<Status>>, s: Status) {
    *status.lock().unwrap() = s;
}

/// "No suitable interface" is the wrong story when the adapter is there and the
/// process simply may not open it. libpcap hands the reason back as prose, and
/// its wording for a missing capability has changed over the years, so all the
/// spellings it has used are matched.
fn denied_open(e: &pcap::Error) -> bool {
    match e {
        pcap::Error::IoError(kind) => *kind == std::io::ErrorKind::PermissionDenied,
        // EPERM and EACCES
        pcap::Error::ErrnoError(errno) => matches!(errno.0, 1 | 13),
        other => {
            let text = other.to_string().to_lowercase();
            ["permission", "not permitted", "cap_net_raw", "denied", "root"]
                .iter()
                .any(|m| text.contains(m))
        }
    }
}

fn watcher(stats: Arc<Mutex<GameStats>>, status: Arc<Mutex<Status>>, app: tauri::AppHandle) {
    let mut sys = System::new();
    let mut captures: HashMap<String, Capture> = HashMap::new();
    let mut game_running = false;
    let mut tick: u64 = 0;
    let mut wanted: Vec<pcap::Device> = Vec::new();
    let mut scope = String::new();
    // adapter -> when it went quiet, and how long to leave it alone
    let mut barren: HashMap<String, (std::time::Instant, Duration)> = HashMap::new();
    let mut looked = std::time::Instant::now() - Duration::from_secs(10);
    // The capture probe opens a device and closes it again — a socket and a
    // ring buffer each time — and this loop runs at 3.3 Hz while the game is
    // down. Rights do not change on that timescale; a minute is plenty.
    let mut probed = std::time::Instant::now() - Duration::from_secs(120);
    let mut can_capture = true;
    // the hosts the game is talking to, kept between the slow endpoint sweeps
    let mut hosts = 0usize;

    loop {
        tick += 1;
        if tick.is_multiple_of(30) {
            stats.lock().unwrap().sample();
        }
        // `!can_capture ||` here re-probed on every pass through exactly the
        // case the cache exists for — a machine with no rights, which fails the
        // probe every time. A failing probe is re-tried sooner than a working
        // one so that granting the capability is noticed within a quarter of a
        // minute rather than a whole one, but it is still a window, not a loop.
        let window = if can_capture { 60 } else { 15 };
        if probed.elapsed() >= Duration::from_secs(window) {
            probed = std::time::Instant::now();
            can_capture = capture_available();
        }
        if !can_capture {
            set_status(&status, Status::NoCapture);
            std::thread::sleep(Duration::from_secs(3));
            continue;
        }

        let pids = game_pids(&mut sys);

        // the overlay follows the game: show on launch, close the farm
        // session and hide when the game exits
        let running = !pids.is_empty();
        GAME_UP.store(running, Ordering::Relaxed);
        if running != game_running {
            game_running = running;
            // nothing to show or hide where the session hosts no overlay
            let auto = crate::read_settings().auto_show && crate::overlay_supported();
            // through the same pair as everywhere else, so the overlay comes
            // back where the player left it rather than where the window
            // manager fancies
            if running {
                // The clock starts when the game does. Left alone it ran from
                // whenever the app was started — so an app on autostart at
                // nine and a game at eight in the evening divided every
                // per-hour figure by eleven idle hours, and filed that as the
                // run's length when the game closed. Outside `if auto` on
                // purpose: the same is true with the overlay switched off.
                stats.lock().unwrap().reset();
                if auto {
                    crate::show_overlay(&app);
                }
            } else {
                // the game closing ends the run, and a closed run is filed
                crate::end_run(&app);
                if auto {
                    crate::hide_overlay(&app);
                }
            }
        }

        // Adapters and endpoints are re-checked on a slow beat: a VPN comes
        // and goes, and the game opens its connections a moment after the
        // process appears. The sweep is inside the beat because on Linux it
        // walks /proc/<pid>/fd for every process on the machine to build an
        // inode-to-pid map — four calls in five were thrown away.
        if running && looked.elapsed() >= Duration::from_secs(5) {
            looked = std::time::Instant::now();
            let (local, remote) = game_endpoints(&pids);
            hosts = remote.len();
            wanted = capture_devices();
            scope = scope_for(&local);
        }
        if !running {
            wanted.clear();
            hosts = 0;
        }

        // An adapter is only judged against one that is working: with the game
        // sitting in a menu nothing arrives anywhere, and retiring every
        // capture then would leave us deaf until the retry window passes.
        let productive = captures.values().any(|c| c.hits.load(Ordering::Relaxed) > 0);
        captures.retain(|name, c| {
            // give a new capture a while to prove itself, then judge it
            let silent = productive
                && c.started.elapsed() >= Duration::from_secs(45)
                && c.hits.load(Ordering::Relaxed) == 0;
            // A thread that ended by itself without a single message could not
            // open the adapter — the usual state of a Linux binary without
            // cap_net_raw. Re-opening it every second forever would be a busy
            // loop that also keeps overwriting the reason on the status line.
            let refused = c.handle.is_finished() && c.hits.load(Ordering::Relaxed) == 0;
            if silent || refused {
                // How long to stay away depends on why. A capture that never
                // opened was refused - no rights, no such device - and asking
                // again in a moment only busies the loop. One that opened and
                // then died lost its adapter underneath it, which is what a
                // VPN does every time it reconnects: that comes back, often
                // within seconds, and five minutes of deafness after every
                // reconnect is not a diagnosis, it is a wait.
                let opened = c.opened.load(Ordering::Relaxed);
                let rest = if opened { Duration::from_secs(10) } else { Duration::from_secs(300) };
                barren.insert(name.clone(), (std::time::Instant::now(), rest));
            }
            let keep = running
                && !silent
                && !c.handle.is_finished()
                && c.scope == scope
                && wanted.iter().any(|d| d.name == *name);
            if !keep {
                c.stop.store(true, Ordering::Relaxed);
            }
            keep
        });
        // a barren adapter is retried now and then: routes change
        barren.retain(|_, (at, rest)| at.elapsed() < *rest);

        for dev in &wanted {
            if captures.contains_key(&dev.name) || barren.contains_key(&dev.name) {
                continue;
            }
            let iface = dev.desc.clone().unwrap_or_else(|| dev.name.clone());
            let scope = scope.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let dropped = Arc::new(AtomicU32::new(0));
            let hits = Arc::new(AtomicU32::new(0));
            let opened = Arc::new(AtomicBool::new(false));
            let handle = {
                let name = dev.name.clone();
                let (stop, stats, status, app) = (stop.clone(), stats.clone(), status.clone(), app.clone());
                let (dev, dropped, scope, hits) = (dev.clone(), dropped.clone(), scope.clone(), hits.clone());
                let opened = opened.clone();
                std::thread::spawn(move || {
                    // "no interface" is the wrong story when the adapter is
                    // there and the process simply may not open it — the usual
                    // state of a fresh Linux install without cap_net_raw.
                    if let Err(e) = capture_loop(dev, scope, stop, stats, dropped, hits, opened, &app) {
                        let refused = denied_open(&e);
                        // The README asks for this log when something is wrong;
                        // until now the whole module never wrote a line to it.
                        crate::log::warn(format!(
                            "capture on {name} ended: {e}{}",
                            if refused { " - the binary needs cap_net_raw" } else { "" }
                        ));
                        set_status(&status, if refused { Status::NoCapture } else { Status::NoInterface });
                    }
                })
            };
            #[cfg(debug_assertions)]
            println!("[capture] {iface} — filter: tcp and len > 30 and ({scope})");
            captures.insert(
                dev.name.clone(),
                Capture {
                    stop,
                    handle,
                    iface,
                    scope,
                    dropped,
                    hits,
                    opened,
                    started: std::time::Instant::now(),
                },
            );
        }

        // Only a capture that actually opened its device counts. "Has not
        // finished yet" is also true of one spawned a moment ago and about to
        // die on a permission error, so the line went green on every spawn and
        // a machine without the right watched it alternate every five minutes.
        let alive: Vec<&Capture> = captures
            .values()
            .filter(|c| c.opened.load(Ordering::Relaxed) && !c.handle.is_finished())
            .collect();
        if !alive.is_empty() {
            let dropped = alive.iter().map(|c| c.dropped.load(Ordering::Relaxed)).sum();
            let mut ifaces: Vec<&str> = alive.iter().map(|c| c.iface.as_str()).collect();
            ifaces.sort_unstable();
            set_status(&status, Status::Capturing { iface: ifaces.join(" + "), hosts, dropped });
        } else if !captures.is_empty() {
            // every capture died: whatever they stored stands
        } else if !running {
            set_status(&status, Status::WaitingForGame);
        } else if wanted.is_empty() && looked.elapsed() < Duration::from_secs(5) {
            set_status(&status, Status::NoInterface);
        }

        // poll briskly while waiting so we attach the moment the game starts
        std::thread::sleep(Duration::from_millis(if running { 1000 } else { 300 }));
    }
}

#[allow(clippy::too_many_arguments)]
fn capture_loop(
    dev: pcap::Device,
    scope: String,
    stop: Arc<AtomicBool>,
    stats: Arc<Mutex<GameStats>>,
    dropped: Arc<AtomicU32>,
    hits: Arc<AtomicU32>,
    opened: Arc<AtomicBool>,
    app: &tauri::AppHandle,
) -> Result<(), pcap::Error> {
    let mut cap = pcap::Capture::from_device(dev)?
        .immediate_mode(true)
        .timeout(400)
        .open()?;
    cap.filter(&format!("tcp and len > 30 and ({scope})"), true)?;
    // past every way this can fail: only now is it a capture
    opened.store(true, Ordering::Relaxed);

    // VPN/tunnel adapters (WireGuard etc.) deliver raw IP or a 4-byte
    // loopback family header instead of Ethernet frames
    let framing = cap.get_datalink().0;
    let mut asm = Reassembler::default();
    let mut swept = std::time::Instant::now();
    let mut counted = std::time::Instant::now();

    while !stop.load(Ordering::Relaxed) {
        if counted.elapsed() >= Duration::from_secs(15) {
            counted = std::time::Instant::now();
            if let Ok(st) = cap.stats() {
                dropped.store(st.dropped + st.if_dropped, Ordering::Relaxed);
                #[cfg(debug_assertions)]
                println!("[capture] {} packets seen, {} dropped, {} dropped by the adapter",
                    st.received, st.dropped, st.if_dropped);
            }
        }
        let packet = match cap.next_packet() {
            // `whole` is false when the capture kept less of the frame than the
            // wire carried. Only a complete frame may have its length rewritten
            // below: on a truncated one the bytes we hold really are fewer than
            // the header says, and telling the parser otherwise would hand the
            // reassembler half a segment as if it were a message.
            Ok(p) => Some((p.data, p.header.caplen >= p.header.len)),
            Err(pcap::Error::TimeoutExpired) => None,
            Err(e) => return Err(e),
        };
        if packet.is_none() || swept.elapsed() >= Duration::from_millis(100) {
            swept = std::time::Instant::now();
            for (src, flushed) in asm.drain_idle() {
                handle_flush(&flushed, src, &stats, &hits, app);
            }
        }
        let Some((data, whole)) = packet else { continue };
        // A segmentation-offloading adapter hands us the whole buffer with a
        // length field describing one segment of it; `unoffload` puts the two
        // back in agreement, and returns nothing at all in the ordinary case.
        let patched =
            whole.then(|| ip_offset(data, framing).and_then(|at| unoffload(data, at))).flatten();
        let data: &[u8] = patched.as_deref().unwrap_or(data);
        let sliced = match framing {
            1 => SlicedPacket::from_ethernet(data), // DLT_EN10MB
            0 | 108 => {
                // DLT_NULL / DLT_LOOP
                if data.len() < 4 {
                    continue;
                }
                SlicedPacket::from_ip(&data[4..])
            }
            _ => SlicedPacket::from_ip(data), // DLT_RAW and friends
        };
        let Ok(pkt) = sliced else { continue };
        let src = match &pkt.net {
            Some(NetSlice::Ipv4(v4)) => IpAddr::V4(v4.header().source_addr()),
            Some(NetSlice::Ipv6(v6)) => IpAddr::V6(v6.header().source_addr()),
            _ => continue,
        };
        let Some(TransportSlice::Tcp(tcp)) = &pkt.transport else { continue };
        // The filter is deliberately wide (everything this machine sends and
        // receives), so TLS is skipped here: an encrypted stream cannot yield
        // the plaintext the parser looks for, and reassembling it is pure cost.
        if tcp.source_port() == 443 || tcp.destination_port() == 443 {
            continue;
        }

        let flow = (src, tcp.source_port(), tcp.destination_port());
        if let Some(flushed) = asm.push(flow, tcp.acknowledgment_number(), tcp.payload()) {
            handle_flush(&flushed, src, &stats, &hits, app);
        }
    }
    Ok(())
}

/// With several adapters listening, a re-injected packet arrives twice. The
/// counters are diff-based and survive that, but a gold deposit is a delta and
/// would count double — so an identical message seen twice is dropped.
fn fresh_messages(messages: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    static SEEN: Mutex<Option<Vec<(u64, std::time::Instant)>>> = Mutex::new(None);
    let Ok(mut guard) = SEEN.lock() else { return messages };
    let seen = guard.get_or_insert_with(Vec::new);
    seen.retain(|(_, at)| at.elapsed() < Duration::from_secs(10));
    messages
        .into_iter()
        .filter(|m| {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            m.to_string().hash(&mut hasher);
            let key = hasher.finish();
            if seen.iter().any(|(h, _)| *h == key) {
                return false;
            }
            seen.push((key, std::time::Instant::now()));
            true
        })
        .collect()
}

fn handle_flush(
    flushed: &[u8],
    src: IpAddr,
    stats: &Arc<Mutex<GameStats>>,
    hits: &Arc<AtomicU32>,
    app: &tauri::AppHandle,
) {
    let messages = fresh_messages(parser::extract_messages(flushed));
    if messages.is_empty() {
        return;
    }
    hits.fetch_add(1, Ordering::Relaxed);
    crate::debug_log(&messages, src);
    let events = parser::events_from_messages(&messages);
    crate::dev_log(&events, src);
    if events.is_empty() {
        return;
    }
    // the engine dedupes and resolves rarities, so it also decides what the
    // ticker and the sounds react to
    let fresh: Vec<_> = {
        let mut stats = stats.lock().unwrap();
        events.iter().filter_map(|e| stats.apply(e)).collect()
    };
    for drop in fresh {
        if let Some(key) = &drop.sound {
            // the rarity travels along as a fallback: a list with no sound of
            // its own still gets announced
            let _ = app.emit("item-drop", (key, &drop.rarity));
        }
        // the ticker and the journal follow the alert rules; the flourish has
        // its own, and a drop can satisfy either, both or only one
        if drop.announce {
            let _ = app.emit("drop-entry", &drop);
        }
        if drop.flourish {
            crate::maybe_flourish(app, &drop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    /// `::ffff:10.8.1.8` and `10.8.1.8` are one address, and only one of them
    /// can be written into a filter that will ever match. A Linux build of the
    /// game hands us the first; leaving it that way kept the capture up and
    /// the counters at zero, with nothing anywhere saying why.
    #[test]
    fn a_mapped_address_comes_back_as_the_address_it_is() {
        let mapped = IpAddr::V6("::ffff:10.8.1.8".parse::<Ipv6Addr>().unwrap());
        assert_eq!(unmap(mapped), "10.8.1.8".parse::<IpAddr>().unwrap());
        // a real IPv6 address is left alone
        let real = IpAddr::V6("2a01:4f8::1".parse::<Ipv6Addr>().unwrap());
        assert_eq!(unmap(real), real);
        // and IPv4 passes through untouched
        let plain: IpAddr = "192.168.0.70".parse().unwrap();
        assert_eq!(unmap(plain), plain);
    }

    /// The filter decides whether anything is captured at all. With no known
    /// address it must stay wide, or a session is deaf until the game happens
    /// to open a socket we can attribute.
    #[test]
    fn the_filter_is_wide_until_the_game_names_itself() {
        assert_eq!(scope_for(&BTreeSet::new()), "tcp");

        let one: BTreeSet<IpAddr> = ["10.8.1.8".parse().unwrap()].into_iter().collect();
        assert_eq!(scope_for(&one), "host 10.8.1.8");

        let two: BTreeSet<IpAddr> =
            ["10.8.1.8".parse().unwrap(), "192.168.0.70".parse().unwrap()].into_iter().collect();
        let filter = scope_for(&two);
        assert!(filter.contains("host 10.8.1.8"), "{filter}");
        assert!(filter.contains("host 192.168.0.70"), "{filter}");
        assert!(filter.contains(" or "), "{filter}");
    }
}


#[cfg(test)]
mod offload_tests {
    use super::*;
    use etherparse::{SlicedPacket, TransportSlice};

    /// An ethernet + IPv4 + TCP frame carrying `payload` bytes, with `total_len`
    /// written into the header's total-length field whatever the truth is.
    fn frame(payload: usize, total_len: u16) -> Vec<u8> {
        let mut v = vec![0u8; 12];
        v.extend_from_slice(&[0x08, 0x00]);
        v.push(0x45);
        v.push(0);
        v.extend_from_slice(&total_len.to_be_bytes());
        v.extend_from_slice(&[0, 0, 0, 0, 64, 6, 0, 0]);
        v.extend_from_slice(&[10, 0, 0, 1]);
        v.extend_from_slice(&[10, 0, 0, 2]);
        v.extend_from_slice(&[0x1f, 0x90, 0x1f, 0x91]);
        v.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 2]);
        v.push(0x50);
        v.push(0x18);
        v.extend_from_slice(&[0xff, 0xff, 0, 0, 0, 0]);
        v.extend(std::iter::repeat(b'x').take(payload));
        v
    }

    /// What the capture loop would end up with for this frame.
    fn payload_seen(f: &[u8]) -> Option<usize> {
        let patched = ip_offset(f, 1).and_then(|at| unoffload(f, at));
        let data: &[u8] = patched.as_deref().unwrap_or(f);
        match &SlicedPacket::from_ethernet(data).ok()?.transport {
            Some(TransportSlice::Tcp(t)) => Some(t.payload().len()),
            _ => None,
        }
    }

    #[test]
    fn an_offloaded_frame_keeps_all_of_its_payload() {
        // The two shapes a segmentation-offloading adapter produces. Before this
        // was handled, the first was dropped by the parser and the second came
        // back one segment long, which is why a 5 KB character save — the only
        // carrier of experience and kills — never arrived on such a machine.
        assert_eq!(payload_seen(&frame(5000, 0)), Some(5000), "a header claiming nothing");
        assert_eq!(payload_seen(&frame(5000, 1500)), Some(5000), "a header claiming one segment");
    }

    #[test]
    fn an_ordinary_frame_is_left_exactly_as_it_was() {
        assert_eq!(payload_seen(&frame(1000, 1040)), Some(1000));
        assert_eq!(payload_seen(&frame(5000, 5040)), Some(5000));
        // and nothing is copied when nothing is wrong
        assert!(unoffload(&frame(1000, 1040), 14).is_none());
    }

    #[test]
    fn ethernet_padding_is_not_mistaken_for_payload() {
        // A frame this short only reaches 60 bytes because the adapter pads it.
        // The header declares 40 bytes of IP and the buffer holds 46 — six real
        // bytes of padding past the declared end, which reading to the end of
        // the buffer would hand the parser as payload.
        let mut f = frame(0, 40);
        f.resize(60, 0);
        assert_eq!(f.len(), 60);
        assert!(unoffload(&f, 14).is_none(), "padding is not an offloaded buffer");
        assert_eq!(payload_seen(&f), Some(0));

        // one byte past the ceiling there is no padding to excuse the overshoot
        let mut big = frame(0, 40);
        big.resize(61, 0);
        assert!(unoffload(&big, 14).is_some(), "and it is read as offloaded again");
    }

    #[test]
    fn a_vlan_tag_does_not_hide_the_header() {
        let plain = frame(5000, 0);
        let mut tagged = plain[..12].to_vec();
        tagged.extend_from_slice(&[0x81, 0x00, 0x00, 0x64]); // one tag, vid 100
        tagged.extend_from_slice(&plain[12..]);
        assert_eq!(ip_offset(&tagged, 1), Some(18));
        assert_eq!(payload_seen(&tagged), Some(5000));
    }
}
