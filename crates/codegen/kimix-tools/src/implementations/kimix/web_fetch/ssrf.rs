//! SSRF (Server-Side Request Forgery) protection for `web_fetch`.
//!
//! Validates that resolved IP addresses are not in private, link-local, or
//! cloud metadata ranges before allowing outbound HTTP requests.
//!
//! ## Proxy / fake-ip note
//!
//! When an HTTP(S)/SOCKS proxy is active, the client dials the **proxy**,
//! not the destination IP DNS returned. Pre-resolving the hostname and
//! treating Clash/Mihomo **fake-ip** (`198.18.0.0/15`,
//! `fdfe:dcba:9876::/48`) as private ULA produces false SSRF blocks on
//! ordinary public sites. Proxy mode therefore skips destination DNS
//! checks for hostnames; literal private IPs in the URL are still blocked.
//!
//! Reference: [IANA IPv4 Special-Purpose Address Registry](https://www.iana.org/assignments/iana-ipv4-special-registry/)
use std::net::IpAddr;

use url::Url;

use super::error::WebFetchError;

/// Whether the process has a configured egress proxy for `web_fetch`.
///
/// Order: explicit `proxy_endpoint` (caller), then standard env vars.
pub(crate) fn egress_proxy_active(explicit: Option<&str>) -> bool {
    if explicit.is_some_and(|s| !s.trim().is_empty()) {
        return true;
    }
    env_proxy_url().is_some()
}

/// First non-empty proxy URL from common environment variables.
pub(crate) fn env_proxy_url() -> Option<String> {
    const KEYS: &[&str] = &[
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ];
    for key in KEYS {
        if let Ok(v) = std::env::var(key) {
            let t = v.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// Clash/Mihomo fake-ip ranges used for transparent proxy DNS hijacking.
///
/// These are **not** real internal hosts — they only work when the packet
/// path goes through the local proxy/TUN. Treating them as SSRF private
/// addresses incorrectly blocks every public URL under fake-ip mode.
fn is_proxy_fake_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            // 198.18.0.0/15 — RFC 2544 benchmarking; Clash default fake-ip.
            let o = v4.octets();
            o[0] == 198 && (o[1] == 18 || o[1] == 19)
        }
        IpAddr::V6(v6) => {
            // Common Clash fake-ip6 prefix: fdfe:dcba:9876::/48
            let s = v6.segments();
            s[0] == 0xfdfe && s[1] == 0xdcba && s[2] == 0x9876
        }
    }
}

/// Returns `true` if an IP address is in a private, link-local, or cloud
/// metadata range that should be blocked to prevent SSRF attacks.
///
/// **Allowed:** loopback (`127.x` / `::1`) for local development;
/// Clash/Mihomo fake-ip ranges (see [`is_proxy_fake_ip`]).
/// **Blocked:** RFC 1918, link-local, CGNAT/cloud metadata, unspecified,
/// other ULA (except known fake-ip6).
pub(crate) fn is_blocked_ip(ip: &IpAddr) -> bool {
    if is_proxy_fake_ip(ip) {
        return false;
    }
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            // Loopback (127.0.0.0/8) — allowed for local dev servers.
            if octets[0] == 127 {
                return false;
            }
            // RFC 1918: 10.0.0.0/8 — private network.
            if octets[0] == 10 {
                return true;
            }
            // RFC 1918: 172.16.0.0/12 — private network.
            if octets[0] == 172 && (16..=31).contains(&octets[1]) {
                return true;
            }
            // RFC 1918: 192.168.0.0/16 — private network.
            if octets[0] == 192 && octets[1] == 168 {
                return true;
            }
            // RFC 3927: 169.254.0.0/16 — link-local.
            // Includes AWS/GCP/Azure metadata endpoint 169.254.169.254.
            if octets[0] == 169 && octets[1] == 254 {
                return true;
            }
            // RFC 6598: 100.64.0.0/10 — CGNAT / shared address space.
            // Used by some cloud providers for internal metadata services.
            if octets[0] == 100 && (64..=127).contains(&octets[1]) {
                return true;
            }
            // 0.0.0.0 — unspecified address.
            if v4.is_unspecified() {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => {
            // ::1 — loopback, allowed for local dev.
            if v6.is_loopback() {
                return false;
            }
            // :: — unspecified.
            if v6.is_unspecified() {
                return true;
            }
            // IPv4-mapped IPv6 (::ffff:x.x.x.x) — delegate to v4 checks.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked_ip(&IpAddr::V4(v4));
            }
            let segments = v6.segments();
            // RFC 4291: fe80::/10 — link-local unicast.
            if segments[0] & 0xffc0 == 0xfe80 {
                return true;
            }
            // RFC 4193: fc00::/7 — unique local address (ULA).
            // Known proxy fake-ip6 already allowed above.
            if segments[0] & 0xfe00 == 0xfc00 {
                return true;
            }
            false
        }
    }
}

/// Resolve hostname via DNS and verify none of the resolved addresses are
/// in blocked private/link-local ranges.
///
/// When `via_proxy` is true the HTTP client will dial the proxy, not the
/// destination IP — so destination DNS is not an SSRF surface for hostnames.
/// Literal private IPs in the URL are still rejected.
pub(crate) async fn check_ssrf(url: &Url, via_proxy: bool) -> Result<(), WebFetchError> {
    let host = url
        .host_str()
        .ok_or_else(|| WebFetchError::SingleLabelHost {
            host: String::new(),
        })?;

    // If the host is already a literal IP, check it directly.
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_blocked_ip(&ip) {
            return Err(WebFetchError::SsrfBlocked {
                host: host.to_string(),
                ip,
            });
        }
        return Ok(());
    }

    // Via HTTP(S)/SOCKS proxy: client never connects to the resolved
    // destination IP. Skip DNS SSRF (avoids Clash fake-ip false positives).
    if via_proxy {
        return Ok(());
    }

    // DNS resolution.
    let port = url.port_or_known_default().unwrap_or(443);
    let addr_str = format!("{host}:{port}");
    let addrs: Vec<std::net::SocketAddr> = tokio::net::lookup_host(&addr_str)
        .await
        .map_err(|e| WebFetchError::DnsResolution {
            host: host.to_string(),
            source: e,
        })?
        .collect();

    if addrs.is_empty() {
        return Err(WebFetchError::DnsEmpty(host.to_string()));
    }

    addrs
        .iter()
        .find(|addr| is_blocked_ip(&addr.ip()))
        .map_or(Ok(()), |addr| {
            Err(WebFetchError::SsrfBlocked {
                host: host.to_string(),
                ip: addr.ip(),
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── IPv4 blocking ───────────────────────────────────────────────────

    #[test]
    fn blocks_rfc1918_10x() {
        assert!(is_blocked_ip(&"10.0.0.1".parse().unwrap()));
        assert!(is_blocked_ip(&"10.255.255.255".parse().unwrap()));
    }

    #[test]
    fn blocks_rfc1918_172x() {
        assert!(is_blocked_ip(&"172.16.0.1".parse().unwrap()));
        assert!(is_blocked_ip(&"172.31.255.255".parse().unwrap()));
        assert!(!is_blocked_ip(&"172.15.0.1".parse().unwrap()));
        assert!(!is_blocked_ip(&"172.32.0.1".parse().unwrap()));
    }

    #[test]
    fn blocks_rfc1918_192168() {
        assert!(is_blocked_ip(&"192.168.0.1".parse().unwrap()));
        assert!(is_blocked_ip(&"192.168.255.255".parse().unwrap()));
    }

    #[test]
    fn blocks_link_local() {
        assert!(is_blocked_ip(&"169.254.0.1".parse().unwrap()));
        assert!(is_blocked_ip(&"169.254.169.254".parse().unwrap()));
    }

    #[test]
    fn blocks_cgnat_cloud_metadata() {
        assert!(is_blocked_ip(&"100.64.0.1".parse().unwrap()));
        assert!(is_blocked_ip(&"100.127.255.255".parse().unwrap()));
        assert!(!is_blocked_ip(&"100.63.0.1".parse().unwrap()));
        assert!(!is_blocked_ip(&"100.128.0.1".parse().unwrap()));
    }

    #[test]
    fn blocks_unspecified() {
        assert!(is_blocked_ip(&"0.0.0.0".parse().unwrap()));
        assert!(is_blocked_ip(&"::".parse().unwrap()));
    }

    #[test]
    fn allows_loopback() {
        assert!(!is_blocked_ip(&"127.0.0.1".parse().unwrap()));
        assert!(!is_blocked_ip(&"127.0.0.2".parse().unwrap()));
        assert!(!is_blocked_ip(&"::1".parse().unwrap()));
    }

    #[test]
    fn allows_public_ips() {
        assert!(!is_blocked_ip(&"1.1.1.1".parse().unwrap()));
        assert!(!is_blocked_ip(&"8.8.8.8".parse().unwrap()));
        assert!(!is_blocked_ip(&"142.250.80.46".parse().unwrap()));
    }

    #[test]
    fn allows_clash_fake_ip_v4() {
        assert!(!is_blocked_ip(&"198.18.0.1".parse().unwrap()));
        assert!(!is_blocked_ip(&"198.18.0.10".parse().unwrap()));
        assert!(!is_blocked_ip(&"198.19.255.255".parse().unwrap()));
        // Adjacent real public range still not blocked by private rules:
        assert!(!is_blocked_ip(&"198.17.0.1".parse().unwrap()));
    }

    #[test]
    fn allows_clash_fake_ip6() {
        assert!(!is_blocked_ip(
            &"fdfe:dcba:9876::37".parse::<IpAddr>().unwrap()
        ));
        assert!(!is_blocked_ip(
            &"fdfe:dcba:9876::2b".parse::<IpAddr>().unwrap()
        ));
    }

    // ── IPv6 ────────────────────────────────────────────────────────────

    #[test]
    fn blocks_ipv6_link_local() {
        assert!(is_blocked_ip(&"fe80::1".parse().unwrap()));
    }

    #[test]
    fn blocks_ipv6_unique_local() {
        assert!(is_blocked_ip(&"fc00::1".parse().unwrap()));
        assert!(is_blocked_ip(&"fd00::1".parse().unwrap()));
        // Non-Clash ULA still blocked.
        assert!(is_blocked_ip(
            &"fdfe:dcba:9875::1".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn blocks_ipv4_mapped_ipv6_private() {
        assert!(is_blocked_ip(&"::ffff:10.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(is_blocked_ip(
            &"::ffff:192.168.1.1".parse::<IpAddr>().unwrap()
        ));
    }

    #[test]
    fn allows_ipv4_mapped_ipv6_public() {
        assert!(!is_blocked_ip(&"::ffff:8.8.8.8".parse::<IpAddr>().unwrap()));
    }

    // ── check_ssrf integration ──────────────────────────────────────────

    #[tokio::test]
    async fn ssrf_blocks_ip_literal_private() {
        let url = Url::parse("https://10.0.0.1/secret").unwrap();
        let result = check_ssrf(&url, false).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("private"));
    }

    #[tokio::test]
    async fn ssrf_allows_ip_literal_public() {
        let url = Url::parse("https://1.1.1.1/").unwrap();
        let result = check_ssrf(&url, false).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn ssrf_via_proxy_skips_hostname_dns() {
        // Host that would fail DNS in offline CI still passes with proxy mode.
        let url = Url::parse("https://this-host-does-not-exist.invalid.example/").unwrap();
        let result = check_ssrf(&url, true).await;
        assert!(result.is_ok(), "via_proxy must skip DNS SSRF: {result:?}");
    }

    #[tokio::test]
    async fn ssrf_via_proxy_still_blocks_private_ip_literal() {
        let url = Url::parse("https://10.0.0.1/secret").unwrap();
        let result = check_ssrf(&url, true).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn ssrf_allows_clash_fake_ip_literal() {
        let url = Url::parse("https://198.18.0.10/").unwrap();
        assert!(check_ssrf(&url, false).await.is_ok());
        let url6 = Url::parse("https://[fdfe:dcba:9876::37]/").unwrap();
        assert!(check_ssrf(&url6, false).await.is_ok());
    }
}
