//! Opt-in Packet Monitor watcher. The default pcap path does not enter here.
use super::*;
use crate::pktmon::{self, Filter, Session};
use std::time::Instant;

static DIAGNOSTICS: Mutex<Option<pktmon::Diagnostics>> = Mutex::new(None);

pub(super) fn diagnostics() -> serde_json::Value {
    let last = *DIAGNOSTICS.lock().unwrap_or_else(|e| e.into_inner());
    last.map(|d| {
        serde_json::json!({
            "observed":d.observed,"accepted":d.accepted,"filtered":d.filtered,
            "truncated":d.truncated,"malformed":d.malformed,"unsupported":d.unsupported,
            "queue_full":d.queue_full,"missed_write":d.missed_write,"missed_read":d.missed_read,
            "stream_events":d.stream_events,
        })
    })
    .unwrap_or(serde_json::Value::Null)
}

fn endpoints_from_rows(
    pids: &[u32],
    rows: impl IntoIterator<Item = (Vec<u32>, IpAddr, u16)>,
) -> BTreeSet<(IpAddr, u16)> {
    rows.into_iter()
        .map(|(owners, address, port)| (owners, unmap(address), port))
        .filter(|(owners, address, port)| {
            *port != 0 && !address.is_unspecified() && owners.iter().any(|pid| pids.contains(pid))
        })
        .map(|(_, address, port)| (address, port))
        .collect()
}

/// Keep the game's exact TCP endpoints, including its local cleartext proxy
/// endpoint. Never follow other processes or widen an empty socket table.
fn game_tcp_endpoints(pids: &[u32]) -> BTreeSet<(IpAddr, u16)> {
    let mut endpoints = BTreeSet::new();
    for (name, family) in [
        ("IPv4", AddressFamilyFlags::IPV4),
        ("IPv6", AddressFamilyFlags::IPV6),
    ] {
        match netstat2::iterate_sockets_info(family, ProtocolFlags::TCP) {
            Ok(rows) => endpoints.extend(endpoints_from_rows(
                pids,
                rows.flatten().filter_map(|row| {
                    if let ProtocolSocketInfo::Tcp(tcp) = row.protocol_socket_info {
                        Some((row.associated_pids, tcp.remote_addr, tcp.remote_port))
                    } else {
                        None
                    }
                }),
            )),
            Err(error) => crate::log::once(
                &format!("packet-monitor-sockets-{name}"),
                "warn",
                format!("Packet Monitor cannot read the {name} socket table: {error}"),
            ),
        }
    }
    endpoints
}

fn report_error(status: &Arc<Mutex<Status>>, error: impl std::fmt::Display) {
    let message = error.to_string();
    crate::log::once(
        "packet-monitor-error",
        "warn",
        format!("Packet Monitor: {message}"),
    );
    set_status(
        status,
        Status::BackendError {
            backend: "packet-monitor".into(),
            message,
        },
    );
}

fn losses(diagnostics: pktmon::Diagnostics) -> u32 {
    diagnostics
        .queue_full
        .saturating_add(diagnostics.missed_write)
        .saturating_add(diagnostics.missed_read)
        .saturating_add(diagnostics.truncated)
        .min(u32::MAX as u64) as u32
}

pub(super) fn watcher(
    stats: Arc<Mutex<GameStats>>,
    status: Arc<Mutex<Status>>,
    app: tauri::AppHandle,
) {
    let mut sys = System::new();
    let mut game_running = false;
    let mut capture: Option<Session> = None;
    let mut active_filter: Option<Filter> = None;
    let mut probe_error = None;
    let mut last_probe = Instant::now() - Duration::from_secs(60);
    let mut last_scope = Instant::now() - Duration::from_secs(60);
    let mut last_sample = Instant::now();
    let mut started = Instant::now();
    let mut retry_after = Instant::now();
    let mut last_error: Option<String> = None;
    let mut transport = TransportState::default();
    let mut reported_transport = parser::TransportDiagnostics::default();
    let mut reported_at = Instant::now();
    let hits = Arc::new(AtomicU32::new(0));
    let packets = AtomicU32::new(0);
    let tls = AtomicU32::new(0);

    loop {
        if last_sample.elapsed() >= Duration::from_secs(30) {
            last_sample = Instant::now();
            stats.lock().unwrap_or_else(|e| e.into_inner()).sample();
        }
        // Permission problems are explicit even before the game starts. This
        // path never loads wpcap.dll and never asks Windows for elevation.
        if capture.is_none() && last_probe.elapsed() >= Duration::from_secs(15) {
            last_probe = Instant::now();
            probe_error = pktmon::probe().err();
        }
        if let Some(error) = &probe_error {
            report_error(&status, error);
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        if last_scope.elapsed() >= Duration::from_secs(1) {
            last_scope = Instant::now();
            let pids = game_pids(&mut sys);
            let running = !pids.is_empty();
            let was_running = game_running;
            note_game_presence(running, &mut game_running, &stats, &app);
            if running && !was_running {
                transport = TransportState::default();
            }
            if !running {
                capture = None;
                active_filter = None;
                last_error = None;
                set_status(&status, Status::WaitingForGame);
            } else {
                let filter = Filter {
                    endpoints: game_tcp_endpoints(&pids),
                    allow_all_tcp: wide_capture(),
                };
                if filter.endpoints.is_empty() && !filter.allow_all_tcp {
                    capture = None;
                    active_filter = None;
                    last_error = None;
                    set_status(&status, Status::WaitingForConnection);
                } else if active_filter.as_ref() != Some(&filter)
                    || (capture.is_none() && Instant::now() >= retry_after)
                {
                    capture = None; // synchronously close only our private stream/session
                    active_filter = Some(filter.clone());
                    match Session::open(filter) {
                        Ok(opened) => {
                            capture = Some(opened);
                            started = Instant::now();
                            hits.store(0, Ordering::Relaxed);
                            packets.store(0, Ordering::Relaxed);
                            tls.store(0, Ordering::Relaxed);
                            last_error = None;
                        }
                        Err(error) => {
                            report_error(&status, &error);
                            last_error = Some(error.to_string());
                            retry_after = Instant::now() + Duration::from_secs(15);
                        }
                    }
                }
            }
        }

        let Some(session) = capture.as_ref() else {
            if let Some(error) = &last_error {
                report_error(&status, error);
            }
            std::thread::sleep(Duration::from_millis(100));
            continue;
        };
        match session.next_packet(Duration::from_millis(100)) {
            Ok(Some(packet)) => {
                packets.fetch_add(1, Ordering::Relaxed);
                route_packet(
                    &packet.data,
                    packet.framing,
                    packet.whole,
                    wide_capture(),
                    &tls,
                    &transport,
                    |flow, bytes| handle_flush(bytes, flow.0, &stats, &hits, &app),
                );
            }
            Ok(None) => {}
            Err(error) => {
                report_error(&status, &error);
                last_error = Some(error.to_string());
                retry_after = Instant::now() + Duration::from_secs(15);
                capture = None;
                continue;
            }
        }
        drain_ready(&transport, |flow, bytes| {
            handle_flush(bytes, flow.0, &stats, &hits, &app)
        });
        if reported_at.elapsed() >= Duration::from_secs(15) {
            reported_at = Instant::now();
            let current = transport.assembly.lock().unwrap_or_else(|e| e.into_inner()).diagnostics();
            if current.gaps > reported_transport.gaps || current.conflicts > reported_transport.conflicts
                || current.limits > reported_transport.limits || current.evicted_flows > reported_transport.evicted_flows {
                crate::log::warn(format!("Packet Monitor TCP recovery: {} gaps, {} conflicting overlaps, {} buffer-limit refusals, {} flow evictions",
                    current.gaps, current.conflicts, current.limits, current.evicted_flows));
            }
            reported_transport = current;
        }
        let diagnostic = session.diagnostics();
        *DIAGNOSTICS.lock().unwrap_or_else(|e| e.into_inner()) = Some(diagnostic);
        let total = packets.load(Ordering::Relaxed);
        let encrypted = tls.load(Ordering::Relaxed);
        let wide = active_filter
            .as_ref()
            .is_some_and(|filter| filter.allow_all_tcp);
        let deaf = if hits.load(Ordering::Relaxed) == 0 && started.elapsed() >= DEAF_AFTER {
            if total > 0 && encrypted >= total {
                Deaf::Encrypted
            } else if wide {
                Deaf::Wide
            } else {
                Deaf::Narrow
            }
        } else {
            Deaf::No
        };
        let endpoints = &active_filter
            .as_ref()
            .expect("an opened session has a filter")
            .endpoints;
        set_status(
            &status,
            Status::Capturing {
                iface: "Windows Packet Monitor (experimental)".into(),
                hosts: endpoints
                    .iter()
                    .filter(|(ip, _)| !ip.is_loopback())
                    .map(|(ip, _)| ip)
                    .collect::<BTreeSet<_>>()
                    .len(),
                local: endpoints.iter().filter(|(ip, _)| ip.is_loopback()).count(),
                dropped: losses(diagnostic),
                packets: total,
                deaf,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_socket_scope_preserves_loopback_and_normalizes_ipv4_mapping() {
        let endpoints = endpoints_from_rows(
            &[10],
            vec![
                (vec![10], "::ffff:203.0.113.10".parse().unwrap(), 7000),
                (vec![10], "127.0.0.1".parse().unwrap(), 6600),
                (vec![10], "::1".parse().unwrap(), 6601),
                (vec![99], "203.0.113.11".parse().unwrap(), 7000),
                (vec![10], "0.0.0.0".parse().unwrap(), 0),
                (vec![10], "::ffff:0.0.0.0".parse().unwrap(), 7000),
            ],
        );
        assert_eq!(
            endpoints,
            [
                ("203.0.113.10".parse().unwrap(), 7000),
                ("127.0.0.1".parse().unwrap(), 6600),
                ("::1".parse().unwrap(), 6601),
            ]
            .into_iter()
            .collect()
        );
        assert!(
            endpoints_from_rows(&[], vec![(vec![10], "203.0.113.10".parse().unwrap(), 7000)])
                .is_empty()
        );
    }
}
