# Windows capture without Npcap

The experimental `pktmon` module uses the **Windows inbox Packet Monitor** API.
It installs no third-party capture driver and does not change the game's
connections, inject code, decrypt traffic, or modify firewall rules. Windows'
own Packet Monitor driver is still involved. Npcap remains the established
backend until the new path has live traffic evidence.

## Feasible choices

| Mechanism | Payload and operational limits | Decision |
| --- | --- | --- |
| Packet Monitor user-mode API | Independent capture sessions and real-time packet bytes; administrator privileges; runtime API availability varies; documented snapshot maximum 9000 bytes. | Implemented as an experimental, explicitly selected backend. |
| Raw sockets / `SIO_RCVALL` | Requires administrator privileges and explicit interface binding; IPv4 and IPv6 setup differ. Loopback, proxies and offload need dedicated validation. | Not selected as the general replacement. |
| WFP deep inspection | Reading packet/stream data uses a kernel-mode callout driver. User-mode management functions alone do not provide arbitrary TCP payloads. | An unsigned custom kernel driver is not an appropriate substitute. |
| `Microsoft-Windows-TCPIP` connection telemetry | Connection activity is useful for attribution. It is not evidence that the events contain the full application byte stream. | Keep separate from packet acquisition. |
| Global `pktmon.exe` / `netsh trace` capture | Useful diagnostics, but global session/filter changes can interfere with other tools. Text parsing, file conversion and locale-dependent output complicate live tracking. | The application does not invoke global start/stop commands. |

Primary contracts:

- [Packet Monitor API overview](https://learn.microsoft.com/en-us/windows/win32/pktmon/pktmon): DLL entry points are resolved dynamically; Microsoft does not list a minimum supported Windows client on this page.
- [PacketMonitorInitialize](https://learn.microsoft.com/en-us/windows/win32/pktmon/packetmonitor/nf-packetmonitor-packetmonitorinitialize): API version and administrator requirement.
- [CreateLiveSession](https://learn.microsoft.com/en-us/windows/win32/pktmon/packetmonitor/nf-packetmonitor-packetmonitorcreatelivesession), [CreateRealtimeStream](https://learn.microsoft.com/en-us/windows/win32/pktmon/packetmonitor/nf-packetmonitor-packetmonitorcreaterealtimestream): private sessions, callback layouts, buffer and truncation limits.
- [SetSessionActive](https://learn.microsoft.com/en-us/windows/win32/pktmon/packetmonitor/nf-packetmonitor-packetmonitorsetsessionactive), [CloseRealtimeStream](https://learn.microsoft.com/en-us/windows/win32/pktmon/packetmonitor/nf-packetmonitor-packetmonitorcloserealtimestream): stop and callback lifetime.
- [Raw sockets](https://learn.microsoft.com/en-us/windows/win32/winsock/tcp-ip-raw-sockets-2), [SIO_RCVALL](https://learn.microsoft.com/en-us/windows/win32/winsock/sio-rcvall): raw socket constraints.
- [WFP operation](https://learn.microsoft.com/en-us/windows/win32/fwp/basic-operation): driver callouts and filtering layers.

## Implementation boundary

The standard build keeps the established backend. To try Packet Monitor, set
the process environment variable `HS_CAPTURE_BACKEND=packet-monitor`. An absent
value, `pcap` or `npcap` selects pcap in the standard build. An invalid value
reports an error; there is no automatic fallback that hides a failed capture.
On Windows the selected backend can be inspected through
`capture_backend_info`.

The Cargo feature `pcap-capture` is enabled by default. A separate experimental
Windows build made with `--no-default-features` excludes the pcap dependency,
Npcap SDK linkage and delay-load import. That build selects Packet Monitor
when the environment variable is absent. Explicit `pcap` or `npcap` selection
reports that the backend was not compiled in. Disabling the feature is a build
choice; setting an environment variable on the standard executable does not
remove its Npcap import. Both variants need their own artifact and import-table
verification before distribution.

The existing Windows installer still describes and offers Npcap for the
standard edition. The monitor-only flavor currently targets a separate
portable executable; building a monitor-only installer requires matching
installer text and hooks before shipping it.

From the Windows developer environment, the portable flavor can be built with:

```cmd
tauri-release.cmd --no-bundle -- --no-default-features --locked
```

Copy that executable to a separate output location before rebuilding the
standard edition: both commands use `src-tauri/target/release/hs-tracker.exe`.

The watcher follows the game's own exact TCP endpoints, including a local
proxy, and waits for a connection when the endpoint table is empty. Only the
existing explicit wide-capture preference allows all TCP. Both sources feed
the same offload normalization, TCP sequence reassembly and game event handler;
the Packet Monitor path cannot independently add earnings or notifications.

`src-tauri/src/pktmon.rs` loads `PktMonApi.dll` with
`LOAD_LIBRARY_SEARCH_SYSTEM32`; it cannot load a same-named DLL beside Tracker.
Missing entry points, unsupported versions and permission errors are distinct
from an absent Npcap installation. The permission probe opens and closes only a
driver handle; it does not create or activate a capture session and never asks
Windows for elevation.

A private session and real-time stream use RAII cleanup. Callbacks finish
before their context is released. The callback validates descriptor bounds,
metadata/payload separation and Ethernet/raw-IP framing, then checks an exact
`(IP address, TCP port)` endpoint before copying packet bytes. Empty endpoint
sets are refused unless the user explicitly selected wide capture. Both TCP
directions are accepted; an adjacent IP address or a different port is not.

The initial implementation filters in user mode. The **OS ring sees all
components' packets**, even though unrelated packet bytes are not retained in
the application's queue. The public documentation omits constants required to
declare its native constraint structure, so this version does not guess that
ABI. Heavy unrelated traffic can therefore cause stream loss. The bounded
queue holds at most 128 packets, and queue/stream loss has explicit counters.

Packet Monitor can show the same packet at several stack components. The
existing sequence-based TCP reassembler remains responsible for repeated
delivery. Packet types without a complete Ethernet/IP endpoint, fragmented IP
packets and ambiguous or truncated snapshots are refused and counted. A
9000-byte snapshot is rejected conservatively because the API does not expose
an independent original packet length. An oversized offload buffer may be
lost; the implementation does not invent its missing tail.

## Evidence and release gate

On the current Windows 26300 host, the signed Microsoft DLL exports the
required functions. The unprivileged probe returns **0x80070005, AccessDenied**.
No live capture was started. Local evidence is stored in ignored
`tools/data/capture-research/packet-monitor-native-probe.json`.

Native tests cover endpoint directionality, IPv6 loopback/extension header
fixtures, VLAN framing, malformed descriptors, snapshot limits and bounded
queue behavior. These are parser/ABI-boundary tests; they do not certify that
the OS captures loopback or both directions of a real game connection.

Before making this backend the default, an authorized elevated test must
demonstrate full payload recovery on real IPv4, IPv6 and local proxy traffic,
clean stop/restart, offload behavior, loss reporting and game counter parity.
Older Windows machines without the API also need a supported path. A separate
privileged helper could reduce the privileges of the dashboard later, but is
not installed by this implementation.

Removing Npcap does not establish that antivirus products will accept the
application. Code signing, build provenance, clear permissions and review of
an actual false-positive detection remain separate release concerns. No
antivirus exclusions, disabling of protections, obfuscation or external binary
submission are part of this change.

## Final local artifact check

The monitor-only portable executable is
`tools/data/releases/hs-tracker-no-npcap.exe`, SHA256
`734b2b4b2f998cbc031a2b03b2d8404a1c9a038a21247e5a812e0d0289308858`.
Its actual PE import tables contain neither `wpcap.dll` nor `Packet.dll`.
The embedded WebView2 dashboard renders without an installed Npcap and reports
the expected Packet Monitor permission error. All 27 dataset references in its
compiled embedded map match final bundle `60e43527`.

The standard executable retains its expected `wpcap.dll` delay import. Both
final executables passed the local Defender custom scan (exit 0, no threats);
both are Authenticode NotSigned. The detailed evidence is in
`tools/data/capture-research/` and `tools/data/windows-release-evidence.json`.
This does not replace the elevated live-capture release gate above.
