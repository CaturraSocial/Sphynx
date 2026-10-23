use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::Url;

use crate::error::ProxyError;

pub fn is_private_or_restricted_ipv4(ip: &Ipv4Addr) -> bool {
    let octets = ip.octets();

    // 0.0.0.0/8 (Current network)
    if octets[0] == 0 {
        return true;
    }
    // 127.0.0.0/8 (Loopback)
    if ip.is_loopback() {
        return true;
    }
    // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 (Private)
    if ip.is_private() {
        return true;
    }
    // 169.254.0.0/16 (Link-local)
    if ip.is_link_local() {
        return true;
    }
    // 100.64.0.0/10 (Carrier-grade NAT)
    if octets[0] == 100 && (octets[1] & 0b1100_0000) == 64 {
        return true;
    }
    // 192.0.0.0/24 (IETF Protocol Assignments)
    if octets[0] == 192 && octets[1] == 0 && octets[2] == 0 {
        return true;
    }
    // 192.0.2.0/24 (TEST-NET-1)
    if octets[0] == 192 && octets[1] == 0 && octets[2] == 2 {
        return true;
    }
    // 198.51.100.0/24 (TEST-NET-2)
    if octets[0] == 198 && octets[1] == 51 && octets[2] == 100 {
        return true;
    }
    // 203.0.113.0/24 (TEST-NET-3)
    if octets[0] == 203 && octets[1] == 0 && octets[2] == 113 {
        return true;
    }
    // 198.18.0.0/15 (Benchmarking)
    if octets[0] == 198 && (octets[1] & 0b1111_1110) == 18 {
        return true;
    }
    // 224.0.0.0/4 (Multicast) & 240.0.0.0/4 (Reserved) & 255.255.255.255 (Broadcast)
    if ip.is_multicast() || ip.is_broadcast() || octets[0] >= 240 {
        return true;
    }

    false
}

pub fn is_private_or_restricted_ipv6(ip: &Ipv6Addr) -> bool {
    let segments = ip.segments();

    // ::1 (Loopback)
    if ip.is_loopback() {
        return true;
    }
    // :: (Unspecified)
    if ip.is_unspecified() {
        return true;
    }
    // ff00::/8 (Multicast)
    if ip.is_multicast() {
        return true;
    }
    // fe80::/10 (Link-local unicast)
    if (segments[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    // fc00::/7 (Unique local address / private)
    if (segments[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // 2001:db8::/32 (Documentation)
    if segments[0] == 0x2001 && segments[1] == 0x0db8 {
        return true;
    }
    // IPv4-mapped IPv6 (::ffff:x.x.x.x)
    if segments[0] == 0
        && segments[1] == 0
        && segments[2] == 0
        && segments[3] == 0
        && segments[4] == 0
        && segments[5] == 0xffff
    {
        let v4 = Ipv4Addr::new(
            (segments[6] >> 8) as u8,
            (segments[6] & 0xff) as u8,
            (segments[7] >> 8) as u8,
            (segments[7] & 0xff) as u8,
        );
        return is_private_or_restricted_ipv4(&v4);
    }

    false
}

pub fn is_ip_restricted(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_private_or_restricted_ipv4(v4),
        IpAddr::V6(v6) => is_private_or_restricted_ipv6(v6),
    }
}

pub async fn validate_url(raw_url: &str) -> Result<Url, ProxyError> {
    let parsed = Url::parse(raw_url).map_err(|e| ProxyError::InvalidUrl(e.to_string()))?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(ProxyError::UnsupportedProtocol);
    }

    let host = parsed.host_str().ok_or_else(|| {
        ProxyError::InvalidUrl("Missing hostname in target URL".to_string())
    })?;

    // Check if host is direct IP literal
    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_ip_restricted(&ip) {
            return Err(ProxyError::SsrfBlocked(format!(
                "Access to restricted IP {ip} is forbidden"
            )));
        }
        return Ok(parsed);
    }

    // Resolve domain to IPs and ensure none point to restricted networks
    let port = parsed.port_or_known_default().unwrap_or(80);
    let socket_addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| ProxyError::RemoteServer(format!("DNS resolution failed for {host}: {e}")))?;

    let mut resolved_any = false;
    for addr in socket_addrs {
        resolved_any = true;
        if is_ip_restricted(&addr.ip()) {
            return Err(ProxyError::SsrfBlocked(format!(
                "Host '{host}' resolved to restricted IP '{}'",
                addr.ip()
            )));
        }
    }

    if !resolved_any {
        return Err(ProxyError::RemoteServer(format!(
            "Host '{host}' did not resolve to any IP address"
        )));
    }

    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_restricted_ipv4() {
        let restricted = [
            "127.0.0.1",
            "127.255.255.255",
            "10.0.0.1",
            "10.255.255.255",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.0.1",
            "192.168.255.255",
            "169.254.169.254",
            "100.64.0.1",
            "100.127.255.255",
            "0.0.0.0",
            "192.0.2.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
        ];

        for ip_str in restricted {
            let ip = Ipv4Addr::from_str(ip_str).unwrap();
            assert!(
                is_private_or_restricted_ipv4(&ip),
                "Expected {} to be restricted",
                ip_str
            );
        }

        let allowed = ["1.1.1.1", "8.8.8.8", "93.184.216.34", "151.101.1.69"];
        for ip_str in allowed {
            let ip = Ipv4Addr::from_str(ip_str).unwrap();
            assert!(
                !is_private_or_restricted_ipv4(&ip),
                "Expected {} to be allowed",
                ip_str
            );
        }
    }

    #[test]
    fn test_restricted_ipv6() {
        let restricted = [
            "::1",
            "::",
            "fe80::1",
            "fe80::dead:beef",
            "fc00::1",
            "fd12:3456:789a::1",
            "ff02::1",
            "2001:db8::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "::ffff:192.168.1.1",
        ];

        for ip_str in restricted {
            let ip = Ipv6Addr::from_str(ip_str).unwrap();
            assert!(
                is_private_or_restricted_ipv6(&ip),
                "Expected {} to be restricted",
                ip_str
            );
        }

        let allowed = [
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
            "::ffff:1.1.1.1",
        ];
        for ip_str in allowed {
            let ip = Ipv6Addr::from_str(ip_str).unwrap();
            assert!(
                !is_private_or_restricted_ipv6(&ip),
                "Expected {} to be allowed",
                ip_str
            );
        }
    }

    #[tokio::test]
    async fn test_validate_url_protocols() {
        assert!(matches!(
            validate_url("ftp://example.com/file").await,
            Err(ProxyError::UnsupportedProtocol)
        ));
        assert!(matches!(
            validate_url("file:///etc/passwd").await,
            Err(ProxyError::UnsupportedProtocol)
        ));
        assert!(matches!(
            validate_url("gopher://127.0.0.1").await,
            Err(ProxyError::UnsupportedProtocol)
        ));
    }

    #[tokio::test]
    async fn test_validate_url_ssrf_direct_ip() {
        assert!(matches!(
            validate_url("http://127.0.0.1:8080/test").await,
            Err(ProxyError::SsrfBlocked(_))
        ));
        assert!(matches!(
            validate_url("http://169.254.169.254/latest/meta-data").await,
            Err(ProxyError::SsrfBlocked(_))
        ));
        assert!(matches!(
            validate_url("http://192.168.1.50/admin").await,
            Err(ProxyError::SsrfBlocked(_))
        ));
    }
}
