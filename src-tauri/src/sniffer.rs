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
use sysinfo::{ProcessesToUpdate, System};
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

/// Elsewhere libpcap is a package dependency, and listing devices needs no
/// privileges — so there is nothing to test here. What can be missing is the
/// right to *open* a device, and only the attempt can tell us that; the capture
/// threads report it.
#[cfg(not(windows))]
pub fn capture_available() -> bool {
    true
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
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes()
        .iter()
        .filter(|(_, p)| {
            let name: String = p
                .name()
                .to_string_lossy()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            name.starts_with("herosiege")
        })
        .map(|(pid, _)| pid.as_u32())
        .collect()
}

/// Both ends of every connection the game holds. The local side decides which
/// adapters to watch — with split tunnelling the game talks over the VPN and
/// over the LAN at the same time, and one adapter would only show half of it.
/// The remote side is for the status line.
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
                if t.remote_addr.is_unspecified() || t.remote_addr.is_loopback() {
                    continue;
                }
                remote.insert(t.remote_addr);
                if !t.local_addr.is_unspecified() && !t.local_addr.is_loopback() {
                    local.insert(t.local_addr);
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
    let mut barren: HashMap<String, std::time::Instant> = HashMap::new();
    let mut looked = std::time::Instant::now() - Duration::from_secs(10);

    loop {
        tick += 1;
        if tick.is_multiple_of(30) {
            stats.lock().unwrap().sample();
        }
        // a run nobody is playing should not be dividing its totals by the time
        // it spent standing still
        let idle_after = crate::IDLE_AFTER.load(Ordering::Relaxed);
        stats
            .lock()
            .unwrap()
            .watch_idle((idle_after > 0).then(|| Duration::from_secs(idle_after as u64)));

        if !capture_available() {
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

        let (local, remote) = if running {
            game_endpoints(&pids)
        } else {
            (BTreeSet::new(), BTreeSet::new())
        };
        // adapters are re-checked on a slow beat: a VPN comes and goes, and the
        // game opens its connections a moment after the process appears
        if running && looked.elapsed() >= Duration::from_secs(5) {
            looked = std::time::Instant::now();
            wanted = capture_devices();
            scope = scope_for(&local);
        }
        if !running {
            wanted.clear();
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
                barren.insert(name.clone(), std::time::Instant::now());
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
        barren.retain(|_, at| at.elapsed() < Duration::from_secs(300));

        for dev in &wanted {
            if captures.contains_key(&dev.name) || barren.contains_key(&dev.name) {
                continue;
            }
            let iface = dev.desc.clone().unwrap_or_else(|| dev.name.clone());
            let scope = scope.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let dropped = Arc::new(AtomicU32::new(0));
            let hits = Arc::new(AtomicU32::new(0));
            let handle = {
                let (stop, stats, status, app) = (stop.clone(), stats.clone(), status.clone(), app.clone());
                let (dev, dropped, scope, hits) = (dev.clone(), dropped.clone(), scope.clone(), hits.clone());
                std::thread::spawn(move || {
                    // "no interface" is the wrong story when the adapter is
                    // there and the process simply may not open it — the usual
                    // state of a fresh Linux install without cap_net_raw.
                    if let Err(e) = capture_loop(dev, scope, stop, stats, dropped, hits, &app) {
                        set_status(&status, if denied_open(&e) { Status::NoCapture } else { Status::NoInterface });
                    }
                })
            };
            #[cfg(debug_assertions)]
            println!("[capture] {iface} — filter: tcp and len > 30 and ({scope})");
            captures.insert(
                dev.name.clone(),
                Capture { stop, handle, iface, scope, dropped, hits, started: std::time::Instant::now() },
            );
        }

        // only threads still on their feet count as capturing; one that died on
        // open has already put the reason on the status line and must not be
        // painted over with a green "capturing"
        let alive: Vec<&Capture> = captures.values().filter(|c| !c.handle.is_finished()).collect();
        if !alive.is_empty() {
            let dropped = alive.iter().map(|c| c.dropped.load(Ordering::Relaxed)).sum();
            let mut ifaces: Vec<&str> = alive.iter().map(|c| c.iface.as_str()).collect();
            ifaces.sort_unstable();
            set_status(&status, Status::Capturing { iface: ifaces.join(" + "), hosts: remote.len(), dropped });
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

fn capture_loop(
    dev: pcap::Device,
    scope: String,
    stop: Arc<AtomicBool>,
    stats: Arc<Mutex<GameStats>>,
    dropped: Arc<AtomicU32>,
    hits: Arc<AtomicU32>,
    app: &tauri::AppHandle,
) -> Result<(), pcap::Error> {
    let mut cap = pcap::Capture::from_device(dev)?
        .immediate_mode(true)
        .timeout(400)
        .open()?;
    cap.filter(&format!("tcp and len > 30 and ({scope})"), true)?;

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
            Ok(p) => Some(p.data),
            Err(pcap::Error::TimeoutExpired) => None,
            Err(e) => return Err(e),
        };
        if packet.is_none() || swept.elapsed() >= Duration::from_millis(100) {
            swept = std::time::Instant::now();
            for (src, flushed) in asm.drain_idle() {
                handle_flush(&flushed, src, &stats, &hits, app);
            }
        }
        let Some(data) = packet else { continue };
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
            crate::stream::ticked(&drop);
        }
        if drop.flourish {
            crate::maybe_flourish(app, &drop);
        }
    }
}
