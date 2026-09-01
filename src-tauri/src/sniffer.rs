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
    /// The driver is installed and will not let this process use it.
    ///
    /// Distinct from `NpcapMissing`: Npcap installed with "Restrict Npcap
    /// driver's access to Administrators only" — a box in its own installer —
    /// looks like an absent driver, and telling that player to install what
    /// they already have is a dead end.
    NoAccess,
    NoInterface,
    WaitingForGame,
    Capturing { iface: String, hosts: usize, dropped: u32, packets: u32, deaf: Deaf },
}

/// Whether the capture has gone the whole of `DEAF_AFTER` without decoding a
/// single message, and whether there is anything left to try.
///
/// The panel needs both: "nothing has been counted for a minute and a half, and
/// there is a setting that would help" is a different thing to say from
/// "nothing has been counted and everything is already being read".
#[derive(Clone, Copy, PartialEq)]
pub enum Deaf {
    /// hearing things, or not silent long enough to say so
    No,
    /// Silent because every frame that arrived was encrypted.
    ///
    /// A client at the login screen talks to the account service over TLS and
    /// to nothing else, so it is silent in exactly the way a broken capture is.
    /// Kept apart from a fault: this state is not one, and no setting would
    /// change it, so the panel must not offer a fix for it.
    Encrypted,
    /// silent, and only the game's own connections are being read
    Narrow,
    /// silent with every connection on the machine already being read
    Wide,
}

impl Status {
    pub fn text(&self) -> String {
        match self {
            #[cfg(windows)]
            Status::NoCapture => "npcap-missing".into(),
            #[cfg(not(windows))]
            Status::NoCapture => "no-capture".into(),
            Status::NoAccess => "no-access".into(),
            Status::NoInterface => "no-interface".into(),
            Status::WaitingForGame => "waiting-for-game".into(),
            Status::Capturing { iface, hosts, dropped, packets, deaf } => {
                let deaf = match deaf {
                    Deaf::No => 0,
                    Deaf::Narrow => 1,
                    Deaf::Wide => 2,
                    Deaf::Encrypted => 3,
                };
                format!("capturing|{iface}|{hosts}|{dropped}|{packets}|{deaf}")
            }
        }
    }
}

/// How long a capture may hear nothing before that is worth writing down. Long
/// enough that a player sitting in a menu does not trip it, short enough to be
/// in the log by the time they think to send it.
const DEAF_AFTER: Duration = Duration::from_secs(90);
/// The setting behind `wide_capture`. The environment variable still works and
/// still wins: it is what a maintainer reaches for on a machine that is not
/// theirs to configure.
static WIDE: AtomicBool = AtomicBool::new(false);

pub fn set_wide_capture(on: bool) {
    WIDE.store(on, Ordering::Relaxed);
}
/// and how long before saying it again, if it is still true
const DEAF_AGAIN: Duration = Duration::from_secs(300);

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
    /// Frames that got past the filter, whether or not anything was decoded
    /// from them. Without this, "nothing is being recorded" has two very
    /// different causes that look identical: a filter matching no traffic, and
    /// traffic that no longer parses.
    packets: Arc<AtomicU32>,
    /// How many of those were on port 443 and skipped unread.
    ///
    /// This is what separates two states that look identical from outside:
    /// "240 frames, all encrypted" is a game that has not joined a server yet,
    /// and "240 frames, none encrypted, none decoded" is a protocol this app no
    /// longer understands.
    tls: Arc<AtomicU32>,
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

impl Shared {
    /// The statistics, whatever happened to the last thread that held them.
    ///
    /// A poisoned `Mutex` hands every later caller an error instead of the
    /// data, so one panic anywhere — a packet parse as easily as a command —
    /// would turn every subsequent click into a panic of its own and leave the
    /// window on screen but unresponsive.
    ///
    /// What this guards is a session's counters, not a file: the worst a
    /// half-written one can do is count a drop twice, and a reset clears it. So
    /// the poison is stepped over. The panic that caused it is already in the
    /// log, where the panic hook put it.
    pub fn stats(&self) -> std::sync::MutexGuard<'_, GameStats> {
        self.stats.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The sniffer's own status, on the same terms.
    pub fn status(&self) -> std::sync::MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(|e| e.into_inner())
    }
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
/// privileges — but *opening* one does, and that is what is missing without
/// `cap_net_raw`, or after any rebuild, since the capability lives on the inode
/// and every relink drops it.
///
/// Checked here rather than in the capture threads, which run only while the
/// game does: a machine with no capture rights at all would otherwise sit on
/// "waiting for Hero Siege" and never say what was wrong. One device is opened
/// and closed again to find out.
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
/// The Linux build opens IPv6 sockets and talks IPv4 over them, so endpoints
/// arrive v4-mapped. Left mapped, `scope_for` emits `host ::ffff:10.8.1.8`,
/// libpcap compiles it as an IPv6 test, and no plain-IPv4 packet can satisfy
/// it — a capture that stays up with the counters at zero.
fn unmap(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    }
}

/// The far ends of the game's own connections, read out of the operating
/// system's socket table.
///
/// The near ends were collected too and are not any more: see `scope_for` for
/// why naming this machine's own address in the filter did more harm than the
/// narrowing was worth.
fn game_endpoints(pids: &[u32]) -> BTreeSet<IpAddr> {
    let mut remote = BTreeSet::new();
    if pids.is_empty() {
        return remote;
    }
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    if let Ok(sockets) = netstat2::get_sockets_info(af, ProtocolFlags::TCP) {
        for s in sockets {
            if !s.associated_pids.iter().any(|p| pids.contains(p)) {
                continue;
            }
            if let ProtocolSocketInfo::Tcp(t) = &s.protocol_socket_info {
                let far = unmap(t.remote_addr);
                if far.is_unspecified() || far.is_loopback() {
                    continue;
                }
                remote.insert(far);
            }
        }
    }
    remote
}

/// Every adapter worth listening on. A split-tunnel engine (WireSock and the
/// like) implements the tunnel in user space and re-injects packets, so the
/// game's traffic can surface on the physical adapter, on the tunnel adapter,
/// or on both — picking one by address misses half of it.
fn capture_devices() -> Vec<pcap::Device> {
    let all = pcap::Device::list().unwrap_or_default();
    // Skip loopback, and nothing else.
    //
    // Not `any(|a| !a.addr.is_loopback())`, which keeps a device that has at
    // least one non-loopback address: a device with NO addresses has none that
    // qualify and would be dropped. The adapter Npcap offers for dialup and VPN
    // capture is exactly that, and on a machine whose traffic goes through a
    // Windows VPN it is the only place the game can be seen — on the physical
    // card the same traffic is inside the tunnel.
    let kept: Vec<pcap::Device> = all
        .iter()
        .filter(|d| worth_capturing(&d.addresses) && is_a_network(&d.name))
        .cloned()
        .collect();

    // Which adapters exist, and which were passed over. Every report of nothing
    // being counted has turned on this list and none of them arrived with it.
    crate::log::once(
        "devices",
        "info",
        format!(
            "adapters: {}{}",
            kept.iter()
                .map(|d| d.desc.clone().unwrap_or_else(|| d.name.clone()))
                .collect::<Vec<_>>()
                .join(", "),
            {
                let skipped: Vec<String> = all
                    .iter()
                    .filter(|d| !kept.iter().any(|k| k.name == d.name))
                    .map(|d| d.desc.clone().unwrap_or_else(|| d.name.clone()))
                    .collect();
                if skipped.is_empty() {
                    String::new()
                } else {
                    format!(" | not networks, or loopback: {}", skipped.join(", "))
                }
            }
        ),
    );
    kept
}

/// Loopback and nothing else is passed over. A device with no addresses at all
/// is kept: having none is not the same as having only loopback, and the
/// adapter Npcap offers for dialup and VPN capture has none.
fn worth_capturing(addresses: &[pcap::Address]) -> bool {
    addresses.is_empty() || addresses.iter().any(|a| !a.addr.is_loopback())
}

/// Devices libpcap offers that are not networks.
///
/// Keeping address-less devices (see above) brings these along, since they have
/// no addresses either — `bluetooth0`, `nflog`, `nfqueue`, `dbus-system`,
/// `dbus-session` and their like. Each would get a capture thread, each fails
/// with "link-layer type filtering not implemented" or similar, and those
/// failures then take the status line away from the adapter that works.
///
/// `dbus-session` is worse: libpcap runs `dbus-launch` to find the bus, which
/// inside an AppImage resolves against the bundled libdbus and dies with a
/// version error the player has to read past.
///
/// None of them can carry a TCP conversation with a game server, so none is
/// opened. The names are libpcap's own and fixed; anything new that slips
/// through is caught after the fact by `unusable_device`.
fn is_a_network(name: &str) -> bool {
    const PSEUDO: [&str; 5] = ["any", "nflog", "nfqueue", "dbus-system", "dbus-session"];
    !PSEUDO.contains(&name)
        && !name.starts_with("bluetooth")
        && !name.starts_with("usbmon")
        && !name.starts_with("nfqueue:")
}

/// Whether a capture ended because the device is not one we can read, rather
/// than because something is wrong.
///
/// That is a fact about the device, not a fault: it must not be retried, and it
/// must not put an error on the status line while a real adapter is working
/// perfectly well beside it.
fn unusable_device(e: &pcap::Error) -> bool {
    let text = e.to_string().to_lowercase();
    text.contains("not implemented") || text.contains("link-layer type")
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
/// segment, or nothing at all. Both shapes occur, and etherparse handles
/// neither: a total length of 0 fails the parse and the frame is lost, and a
/// length of one MSS returns that many bytes and discards the rest.
///
/// Either way no message longer than one segment survives — including the
/// character save, which is around 5 KB and the only carrier of experience and
/// kills.
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

/// Take everything on the wire, rather than only what the game is talking to.
///
/// The ordinary filter is built from the game's own sockets, re-read every five
/// seconds, so in principle it cannot miss a server the game uses. In practice
/// a split-tunnel engine re-injects packets whose local address is not the one
/// the socket table names, so when something stops arriving, "the filter is
/// innocent" has to be measured rather than argued: set `HS_WIDE_CAPTURE=1` and
/// compare.
///
/// An environment variable and not a setting, because a wide filter on a busy
/// machine hands this thread every plaintext byte the machine sends.
fn wide_capture() -> bool {
    WIDE.load(Ordering::Relaxed)
        || matches!(std::env::var("HS_WIDE_CAPTURE").as_deref(), Ok("1") | Ok("true"))
}

/// The far end, and only the far end.
///
/// A packet filter cannot ask which process a packet belongs to, so the game's
/// own servers are as close as it gets. Naming this machine's addresses as well
/// buys nothing and can exclude everything — see below.
///
/// While the game has not connected there is nothing to name and the answer is
/// everything. The endpoints are re-read every five seconds and the capture
/// restarted when they change, so that state does not last.
fn scope_for(remote: &BTreeSet<IpAddr>) -> String {
    // The far side is named by its /24 rather than by the one address that
    // happens to be answering: the game moves between servers inside a session,
    // and the endpoints are re-read only every five seconds, so a single
    // address misses the first seconds of a new one. The precision costs
    // nothing — the game's hosts sit in 172.104.128.x and 139.162.166.x.
    //
    // v6 is left as a plain host: those addresses are not handed out in
    // anything as tidy as a /24.
    //
    // The near side is deliberately NOT named. The socket table gives the
    // address a socket is BOUND to, which is not necessarily the one on the
    // frames a capture sees: a split-tunnel VPN binds to the tunnel and puts a
    // different address on the wire, and a machine with more than one route can
    // send from an address the game's socket never mentions. When the two
    // differ, `and (host ...)` matches nothing at all — a green status line
    // with every counter at zero.
    //
    // It buys nothing either way: the capture is not promiscuous, so the
    // adapter only hands over frames addressed to this machine, and the far
    // side is already a /24 belonging to the game's host.
    if remote.is_empty() {
        // Nothing to narrow to yet. Wide, because a filter that matches nothing
        // is worse than one that matches too much: what is not a game message
        // is thrown away a layer up, and a session that starts deaf stays deaf
        // until the game happens to open a socket we can attribute.
        return "tcp".into();
    }
    let mut out: Vec<String> = remote
        .iter()
        .map(|ip| match ip {
            IpAddr::V4(v4) => {
                let [a, b, c, _] = v4.octets();
                format!("net {a}.{b}.{c}.0/24")
            }
            IpAddr::V6(_) => format!("host {ip}"),
        })
        .collect();
    out.sort();
    out.dedup();
    out.join(" or ")
}

pub fn spawn(shared: &Shared, app: tauri::AppHandle) {
    let stats = shared.stats.clone();
    let status = shared.status.clone();
    std::thread::spawn(move || watcher(stats, status, app));
}

fn set_status(status: &Arc<Mutex<Status>>, s: Status) {
    *status.lock().unwrap_or_else(|e| e.into_inner()) = s;
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
    // When silence was last written down. `log::once` cannot do this job: it
    // dedupes on the message, and the message carries a frame count that moves
    // every second — so the first version of this wrote the same warning
    // twenty-four times in two minutes and buried the two lines above it that
    // actually mattered.
    let mut deaf_said: Option<std::time::Instant> = None;

    loop {
        tick += 1;
        if tick.is_multiple_of(30) {
            stats.lock().unwrap_or_else(|e| e.into_inner()).sample();
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
                // The clock starts when the game does, not when the app does:
                // on autostart the idle hours before the game opens would
                // otherwise divide every per-hour figure and be filed as the
                // run's length. Outside `if auto` on purpose — the same holds
                // with the overlay switched off.
                //
                // This is the one reset with a blackout behind it: the zone
                // carried over from the last session, and nothing says it is
                // still the zone.
                stats.lock().unwrap_or_else(|e| e.into_inner()).reset_after_blackout();
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
            let remote = game_endpoints(&pids);
            hosts = remote.len();
            wanted = capture_devices();
            let narrow = scope_for(&remote);
            let next = if wide_capture() { "tcp".to_string() } else { narrow };
            if next != scope {
                crate::log::say("net", &format!("capture filter: {next}"));
                scope = next;
            }
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
            let packets = Arc::new(AtomicU32::new(0));
            let tls = Arc::new(AtomicU32::new(0));
            let opened = Arc::new(AtomicBool::new(false));
            let handle = {
                let name = dev.name.clone();
                let (stop, stats, status, app) = (stop.clone(), stats.clone(), status.clone(), app.clone());
                let (dev, dropped, scope, hits) = (dev.clone(), dropped.clone(), scope.clone(), hits.clone());
                let (packets, tls) = (packets.clone(), tls.clone());
                let opened = opened.clone();
                std::thread::spawn(move || {
                    // "no interface" is the wrong story when the adapter is
                    // there and the process simply may not open it — the usual
                    // state of a fresh Linux install without cap_net_raw.
                    if let Err(e) =
                        capture_loop(dev, scope, stop, stats, dropped, hits, packets, tls, opened, &app)
                    {
                        // Something libpcap lists but cannot filter on. Said
                        // once, at a level nobody has to act on, and left alone
                        // from then on — the status line belongs to the
                        // adapters that can actually carry the game.
                        if unusable_device(&e) {
                            crate::log::once(
                                &format!("unusable:{name}"),
                                "info",
                                format!("{name} is not a device we can read: {e}"),
                            );
                            return;
                        }
                        let refused = denied_open(&e);
                        // The README asks for this log when something is wrong;
                        // until now the whole module never wrote a line to it.
                        crate::log::warn(format!(
                            "capture on {name} ended: {e}{}",
                            if refused { " - the binary needs cap_net_raw" } else { "" }
                        ));
                        // Refused is not the same as absent. If the driver is
                        // there, the answer the user needs is about rights, not
                        // about installing anything.
                        set_status(
                            &status,
                            match (refused, capture_available()) {
                                (true, true) => Status::NoAccess,
                                (true, false) => Status::NoCapture,
                                _ => Status::NoInterface,
                            },
                        );
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
                    packets,
                    tls,
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
            let iface = ifaces.join(" + ");
            let packets: u32 = alive.iter().map(|c| c.packets.load(Ordering::Relaxed)).sum();

            // Capturing and hearing anything are different states, counted
            // apart: frames that got past the filter, and frames that decoded
            // into something. Nothing past the filter means an adapter or a
            // filter the game's traffic never reaches; plenty past it and
            // nothing decoded means traffic that no longer parses. On the
            // status line the two look identical, which leaves a player with
            // nothing to report but "nothing is recorded".
            let heard: u32 = alive.iter().map(|c| c.hits.load(Ordering::Relaxed)).sum();
            if heard > 0 {
                // it came back; a later relapse is worth another line
                deaf_said = None;
            }
            let silent = running
                && heard == 0
                && alive.iter().any(|c| c.started.elapsed() >= DEAF_AFTER);
            let encrypted: u32 = alive.iter().map(|c| c.tls.load(Ordering::Relaxed)).sum();
            let deaf = match (silent, packets > 0 && encrypted == packets, wide_capture()) {
                (false, _, _) => Deaf::No,
                // nothing arrived that was not encrypted: a game at its login
                // screen looks exactly like this, and nothing is wrong
                (true, true, _) => Deaf::Encrypted,
                (true, false, false) => Deaf::Narrow,
                (true, false, true) => Deaf::Wide,
            };
            let due = deaf_said.is_none_or(|at| at.elapsed() >= DEAF_AGAIN);
            if silent && due {
                deaf_said = Some(std::time::Instant::now());
                let verdict = if packets == 0 {
                    "None arrived at all - the game's traffic is not on this adapter, or not matching this filter."
                } else if encrypted == packets {
                    "Every one of them was encrypted, so the game is talking to a web service and not to a game server - it is most likely sitting at the login screen or in a menu."
                } else if encrypted > 0 {
                    "Some were encrypted and the rest did not parse as game messages."
                } else {
                    "They arrived in the clear and none of them was a game message."
                };
                crate::log::warn(format!(
                    "nothing decoded after {}s on {iface}: {packets} frames got past `tcp and len > 30 and ({scope})`, {encrypted} of them on port 443. {verdict}{}",
                    DEAF_AFTER.as_secs(),
                    if deaf == Deaf::Narrow {
                        " Only the game's own connections are being read - turn on \"Read every connection\" in Settings if something is redirecting them."
                    } else {
                        ""
                    }
                ));
            }
            set_status(&status, Status::Capturing { iface, hosts, dropped, packets, deaf });
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
    packets: Arc<AtomicU32>,
    tls: Arc<AtomicU32>,
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
            Ok(p) => {
                packets.fetch_add(1, Ordering::Relaxed);
                Some((p.data, p.header.caplen >= p.header.len))
            }
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
        // Port 443 is skipped as encrypted: a TLS stream cannot yield the
        // plaintext the parser looks for, and reassembling it is pure cost.
        //
        // The assumption holds until something moves the game's traffic onto
        // that port — a route optimiser relaying it does — and then the rule
        // throws everything away without saying so. Under a wide capture the
        // port is read anyway; being wrong that way costs only scanning
        // payloads that never match, which the scan budget bounds.
        if tcp.source_port() == 443 || tcp.destination_port() == 443 {
            tls.fetch_add(1, Ordering::Relaxed);
            if !wide_capture() {
                continue;
            }
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
        let mut stats = stats.lock().unwrap_or_else(|e| e.into_inner());
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

    fn address(ip: &str) -> pcap::Address {
        pcap::Address {
            addr: ip.parse().unwrap(),
            netmask: None,
            broadcast_addr: None,
            dst_addr: None,
        }
    }

    /// The list libpcap hands over is not all networks.
    ///
    /// Keeping address-less devices — which the VPN case needs — brought
    /// Bluetooth, netfilter and D-Bus along with it, and one Linux machine
    /// spent a capture thread on each. Opening the D-Bus one makes libpcap run
    /// `dbus-launch`, which inside an AppImage dies on the bundled libdbus.
    #[test]
    fn the_pseudo_devices_libpcap_lists_are_left_alone() {
        for real in ["enp9s0", "eth0", "wlan0", "virbr0", "tun0", "wg0"] {
            assert!(is_a_network(real), "{real}");
        }
        for pseudo in ["any", "nflog", "nfqueue", "dbus-system", "dbus-session", "bluetooth0", "bluetooth-monitor", "usbmon1"] {
            assert!(!is_a_network(pseudo), "{pseudo}");
        }
    }

    /// An adapter with no addresses is not a loopback adapter.
    ///
    /// This read `any(|a| !a.addr.is_loopback())`, which is false for an empty
    /// list, so every device without addresses of its own was passed over — and
    /// the one Npcap offers for dialup and VPN capture is exactly that. On a
    /// machine whose traffic all goes through a VPN, that was the only adapter
    /// the game could have been seen on.
    #[test]
    fn an_adapter_with_no_addresses_is_still_worth_listening_on() {
        assert!(worth_capturing(&[]), "no addresses is not the same as loopback");
        assert!(worth_capturing(&[address("192.168.0.70")]));
        assert!(worth_capturing(&[address("127.0.0.1"), address("10.250.0.1")]));
        assert!(!worth_capturing(&[address("127.0.0.1")]), "loopback is what this skips");
        assert!(!worth_capturing(&[address("::1")]));
    }

    /// The filter decides whether anything is captured at all. With no known
    /// address it must stay wide, or a session is deaf until the game happens
    /// to open a socket we can attribute.
    #[test]
    fn the_filter_is_wide_until_the_game_names_itself() {
        assert_eq!(scope_for(&BTreeSet::new()), "tcp");
    }

    /// And once it does, the filter names the far end only.
    ///
    /// The far side is what keeps the capture from being every connection the
    /// machine holds. Naming the near side as well adds nothing and can be
    /// wrong: it is the address the socket is bound to, which behind a split
    /// tunnel is not the address on the wire, and when they differ the filter
    /// matches nothing at all.
    #[test]
    fn the_filter_names_the_far_end_and_leaves_this_machine_alone() {
        let remote: BTreeSet<IpAddr> =
            ["172.104.128.178".parse().unwrap(), "139.162.166.201".parse().unwrap()]
                .into_iter()
                .collect();
        let filter = scope_for(&remote);

        assert_eq!(
            filter,
            "net 139.162.166.0/24 or net 172.104.128.0/24",
            "the game's servers and their neighbours, and nothing else"
        );

        // the concern that put the far side there in the first place
        assert!(!filter.contains("192.168.0.226"));
        // and a tunnel address the socket table might report changes nothing
        assert!(!filter.contains("10.8.1.2"), "{filter}");

        // Two servers in one range are one clause, so a third joining it is
        // already covered without waiting for the endpoints to be re-read.
        let pair: BTreeSet<IpAddr> =
            ["172.104.128.178".parse().unwrap(), "172.104.128.9".parse().unwrap()]
                .into_iter()
                .collect();
        assert_eq!(scope_for(&pair), "net 172.104.128.0/24");
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
