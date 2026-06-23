//! SSRF guard for asset downloads: only public http(s) destinations are allowed,
//! re-validated against every redirect hop by the download loop.

use super::download::DownloadError;
use std::net::IpAddr;

/// SSRF guard. Rejects any URL whose scheme is not http(s), and any URL that
/// resolves to a non-public address (loopback, private, link-local, CGNAT, the
/// cloud metadata IP, etc.). Every resolved address must be public — a host that
/// maps to even one internal address is refused, which blocks DNS-based attacks.
/// The 198.18.0.0/15 benchmarking range is explicitly allowed.
pub(super) async fn validate_download_url(url: &reqwest::Url) -> Result<(), DownloadError> {
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err(DownloadError::BlockedUrl),
    }
    let host = url.host_str().ok_or(DownloadError::BlockedUrl)?;
    let port = url.port_or_known_default().unwrap_or(80);
    let mut resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| DownloadError::BlockedUrl)?
        .peekable();
    if resolved.peek().is_none() {
        return Err(DownloadError::BlockedUrl);
    }
    if allow_private_asset_hosts() {
        return Ok(());
    }
    for addr in resolved {
        if is_blocked_ip(addr.ip()) {
            return Err(DownloadError::BlockedUrl);
        }
    }
    Ok(())
}

/// Opt-in escape hatch (`KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS=1`) for trusted
/// single-user setups that intentionally fetch covers from a LAN/private host.
/// Off by default so the SSRF guard is the safe default.
fn allow_private_asset_hosts() -> bool {
    std::env::var("KIZUNASHELF_ALLOW_PRIVATE_ASSET_HOSTS")
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

/// Whether an IP is in a range we refuse to fetch from. IPv4-mapped IPv6
/// addresses are unwrapped first so `::ffff:127.0.0.1` is treated as loopback.
fn is_blocked_ip(ip: IpAddr) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        v4 => v4,
    };
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // Explicit allowlist: 198.18.0.0/15 (benchmarking range).
            if octets[0] == 198 && (octets[1] == 18 || octets[1] == 19) {
                return false;
            }
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local() // 169.254.0.0/16, incl. the 169.254.169.254 metadata IP
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || octets[0] == 0
                || (octets[0] == 100 && (64..=127).contains(&octets[1])) // CGNAT 100.64.0.0/10
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0) // 192.0.0.0/24
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // unique local fc00::/7
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_internal_ip_ranges() {
        let blocked = [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254", // cloud metadata
            "100.64.0.1",      // CGNAT
            "0.0.0.0",
            "::1",
            "::ffff:127.0.0.1", // IPv4-mapped loopback
            "fe80::1",
            "fc00::1",
        ];
        for ip in blocked {
            assert!(
                is_blocked_ip(ip.parse::<IpAddr>().unwrap()),
                "{ip} should be blocked"
            );
        }
        // Public addresses, and the explicitly allowlisted benchmarking range.
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "198.18.0.1",
            "198.19.255.255",
            "2606:4700::1",
        ] {
            assert!(
                !is_blocked_ip(ip.parse::<IpAddr>().unwrap()),
                "{ip} should be allowed"
            );
        }
    }
}
