//! `streamboat snapcast-discover` (D-034): find a running snapserver on the
//! LAN via mDNS, so the Snapcast output mode's `host`/`port` can be picked
//! from a list instead of hand-typed. Uses `mdns-sd` 0.21.3 — the version
//! current as of this writing, confirmed against crates.io directly rather
//! than trusted from memory (`headless-and-tidal-connect/references/mpd-and-multiroom.md`
//! §1 already checked and cited this exact version).
//!
//! Service types: `_snapcast._tcp` is what snapserver actually advertises
//! for its streaming port 1704 (`mpd-and-multiroom.md` §2, itself citing
//! `badaix/snapcast`'s own `server/etc/snapserver.conf`). `_snapcast-tcp._tcp`
//! is not in that reference at all — it is searched anyway, defensively,
//! per this task's own brief, in case a build or fork advertises under that
//! name; finding nothing under it is expected and not a bug.

use std::net::IpAddr;
use std::time::Duration;

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};

/// One snapserver-shaped mDNS answer, host/address/port pulled out of
/// `mdns_sd`'s [`ResolvedService`] into a plain, easy-to-test shape.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredSnapserver {
    pub service_type: String,
    pub hostname: String,
    pub addresses: Vec<IpAddr>,
    pub port: u16,
}

/// Pure record-to-address parsing (unit tested with a synthetic
/// `ResolvedService` — no real network needed): everything [`discover`]
/// does with one `ServiceEvent::ServiceResolved` once mDNS itself has run.
pub fn from_resolved(service_type: &str, resolved: &ResolvedService) -> DiscoveredSnapserver {
    DiscoveredSnapserver {
        service_type: service_type.to_string(),
        hostname: resolved.get_hostname().to_string(),
        addresses: resolved
            .get_addresses()
            .iter()
            .map(|a| a.to_ip_addr())
            .collect(),
        port: resolved.get_port(),
    }
}

const SERVICE_TYPES: &[&str] = &["_snapcast._tcp.local.", "_snapcast-tcp._tcp.local."];

/// Browses both service types for `timeout`, returning every resolved
/// instance found. Never panics on a browse failure for one type — logs
/// (via the returned `Vec` simply being shorter) and moves on to the next.
pub async fn discover(timeout: Duration) -> anyhow::Result<Vec<DiscoveredSnapserver>> {
    let daemon = ServiceDaemon::new()?;
    let mut found = Vec::new();
    for &service_type in SERVICE_TYPES {
        let receiver = match daemon.browse(service_type) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(%e, service_type, "snapcast-discover: could not browse");
                continue;
            }
        };
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, receiver_recv_async(&receiver)).await {
                Ok(Some(ServiceEvent::ServiceResolved(resolved))) => {
                    found.push(from_resolved(service_type, &resolved));
                }
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(_) => break, // timed out waiting for the next event
            }
        }
        let _ = daemon.stop_browse(service_type);
    }
    let _ = daemon.shutdown();
    Ok(found)
}

/// `mdns_sd::ServiceDaemon::browse` hands back a plain (non-async)
/// `flume`-style `Receiver`; bridges its blocking `recv` onto a
/// `spawn_blocking` task so [`discover`] can `.await` it alongside the
/// per-type deadline above instead of blocking the async runtime.
async fn receiver_recv_async(receiver: &mdns_sd::Receiver<ServiceEvent>) -> Option<ServiceEvent> {
    let receiver = receiver.clone();
    tokio::task::spawn_blocking(move || receiver.recv().ok())
        .await
        .unwrap_or(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdns_sd::ServiceInfo;
    use std::collections::HashMap;
    use std::net::Ipv4Addr;

    /// `ResolvedService` is `#[non_exhaustive]` (no direct struct literal
    /// outside `mdns_sd`) — the crate's own documented way to build one for
    /// a test is `ServiceInfo::new(...).as_resolved_service()`, so this
    /// still constructs synthetic data with no real network, just through
    /// the public constructor instead of the struct's fields directly.
    fn synthetic_resolved(host: &str, ips: &str, port: u16) -> ResolvedService {
        ServiceInfo::new(
            "_snapcast._tcp.local.",
            "streamboat-test",
            host,
            ips,
            port,
            None::<HashMap<String, String>>,
        )
        .unwrap()
        .as_resolved_service()
    }

    #[test]
    fn from_resolved_extracts_hostname_address_and_port_with_no_network() {
        let resolved = synthetic_resolved("snapserver-box.local.", "192.168.1.42", 1704);
        let d = from_resolved("_snapcast._tcp.local.", &resolved);
        assert_eq!(d.service_type, "_snapcast._tcp.local.");
        assert_eq!(d.hostname, "snapserver-box.local.");
        assert_eq!(d.port, 1704);
        assert_eq!(
            d.addresses,
            vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42))]
        );
    }

    #[test]
    fn from_resolved_handles_multiple_addresses() {
        let resolved = synthetic_resolved("multi.local.", "10.0.0.5,192.168.0.5", 1704);
        let d = from_resolved("_snapcast._tcp.local.", &resolved);
        assert_eq!(d.addresses.len(), 2);
    }
}
