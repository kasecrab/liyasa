//! Which address a request is charged to (AUTH-50, §30.2.5).
//!
//! The socket peer, unless the peer is a configured trusted proxy, in which
//! case the last untrusted hop in `X-Forwarded-For` (or `CF-Connecting-IP`
//! when Cloudflare is the listed proxy). With no trusted proxies configured,
//! forwarded headers are ignored entirely, so a client cannot spoof its
//! address to evade the limiter, poison the session key, or forge a region.

use std::net::IpAddr;

use http::HeaderMap;

/// An address range in CIDR form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    addr: IpAddr,
    bits: u8,
}

impl Cidr {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (addr, bits) = match text.split_once('/') {
            Some((addr, bits)) => (addr, bits.parse().ok()?),
            None => {
                let addr: IpAddr = text.parse().ok()?;
                (text, if addr.is_ipv4() { 32u8 } else { 128 })
            }
        };
        let addr: IpAddr = addr.parse().ok()?;
        let width = if addr.is_ipv4() { 32 } else { 128 };
        (bits <= width).then_some(Self { addr, bits })
    }

    pub fn contains(&self, other: IpAddr) -> bool {
        fn masked(bytes: &[u8], bits: u8) -> Vec<u8> {
            let mut out = bytes.to_vec();
            for (i, byte) in out.iter_mut().enumerate() {
                let taken = (bits as usize).saturating_sub(i * 8).min(8);
                *byte &= if taken == 8 {
                    0xff
                } else {
                    !(0xffu16 >> taken) as u8
                };
            }
            out
        }
        match (self.addr, other) {
            (IpAddr::V4(a), IpAddr::V4(b)) => {
                masked(&a.octets(), self.bits) == masked(&b.octets(), self.bits)
            }
            (IpAddr::V6(a), IpAddr::V6(b)) => {
                masked(&a.octets(), self.bits) == masked(&b.octets(), self.bits)
            }
            // A v4-mapped forwarded address against a v4 range, and the
            // reverse: compare the addresses that are really the same family.
            (IpAddr::V4(_), IpAddr::V6(b)) => match b.to_ipv4_mapped() {
                Some(b) => self.contains(IpAddr::V4(b)),
                None => false,
            },
            (IpAddr::V6(a), IpAddr::V4(_)) => match a.to_ipv4_mapped() {
                Some(a) => Self {
                    addr: IpAddr::V4(a),
                    bits: self.bits.saturating_sub(96),
                }
                .contains(other),
                None => false,
            },
        }
    }
}

/// The trusted-proxy configuration. Empty by default, which is the safe
/// default and the one the docs tell self-hosters to keep unless a proxy
/// really is in front.
#[derive(Debug, Clone, Default)]
pub struct TrustedProxies {
    ranges: Vec<Cidr>,
    /// Cloudflare is named rather than inferred: only then is
    /// `CF-Connecting-IP` read.
    cloudflare: bool,
}

impl TrustedProxies {
    /// Builds from `server.trustedProxies`. An entry that is not a CIDR is
    /// skipped rather than trusted.
    pub fn new(entries: &[String]) -> Self {
        let ranges: Vec<Cidr> = entries.iter().filter_map(|e| Cidr::parse(e)).collect();
        // Cloudflare's ranges are what an operator lists to put Cloudflare in
        // front; the header is read only when one of them is listed.
        let cloudflare = entries.iter().any(|e| {
            let e = e.trim();
            e.eq_ignore_ascii_case("cloudflare") || CLOUDFLARE_RANGES.contains(&e)
        });
        Self { ranges, cloudflare }
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn trusts(&self, addr: IpAddr) -> bool {
        self.ranges.iter().any(|r| r.contains(addr))
    }

    /// The address the limiter and the region header are charged to.
    pub fn client_ip(&self, peer: IpAddr, headers: &HeaderMap) -> IpAddr {
        if self.ranges.is_empty() || !self.trusts(peer) {
            return peer;
        }
        if self.cloudflare
            && let Some(addr) = headers
                .get("cf-connecting-ip")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<IpAddr>().ok())
        {
            return addr;
        }
        // Walk the chain from the right, skipping hops that are themselves
        // trusted proxies; the first untrusted one is the client.
        let forwarded: Vec<IpAddr> = headers
            .get_all(http::header::FORWARDED)
            .iter()
            .chain(headers.get_all("x-forwarded-for").iter())
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(','))
            .filter_map(parse_hop)
            .collect();
        for addr in forwarded.iter().rev() {
            if !self.trusts(*addr) {
                return *addr;
            }
        }
        forwarded.first().copied().unwrap_or(peer)
    }
}

/// One `X-Forwarded-For` element, or the `for=` part of an RFC 7239
/// `Forwarded` element.
fn parse_hop(text: &str) -> Option<IpAddr> {
    let mut hop = text.trim();
    if let Some(rest) = hop.strip_prefix("for=") {
        hop = rest;
    } else if hop.contains('=') {
        return None;
    }
    let hop = hop.trim_matches('"');
    if let Ok(addr) = hop.parse::<IpAddr>() {
        return Some(addr);
    }
    // `[2001:db8::1]:443` and `198.51.100.1:443`.
    if let Some(rest) = hop.strip_prefix('[')
        && let Some((addr, _)) = rest.split_once(']')
    {
        return addr.parse().ok();
    }
    hop.rsplit_once(':')
        .and_then(|(addr, _)| addr.parse().ok())
        .or_else(|| hop.parse().ok())
}

/// The ranges an operator lists to say "Cloudflare is in front"; listing any
/// one of them turns `CF-Connecting-IP` on.
const CLOUDFLARE_RANGES: &[&str] = &[
    "173.245.48.0/20",
    "103.21.244.0/22",
    "103.22.200.0/22",
    "103.31.4.0/22",
    "141.101.64.0/18",
    "108.162.192.0/18",
    "190.93.240.0/20",
    "188.114.96.0/20",
    "197.234.240.0/22",
    "198.41.128.0/17",
    "162.158.0.0/15",
    "104.16.0.0/13",
    "104.24.0.0/14",
    "172.64.0.0/13",
    "131.0.72.0/22",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(
                http::HeaderName::from_bytes(name.as_bytes()).expect("a header name"),
                value.parse().expect("a header value"),
            );
        }
        map
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().expect("an address")
    }

    #[test]
    fn with_no_trusted_proxy_a_forwarded_header_is_ignored() {
        let proxies = TrustedProxies::default();
        let sent = headers(&[("x-forwarded-for", "1.2.3.4")]);
        assert_eq!(
            proxies.client_ip(ip("198.51.100.7"), &sent),
            ip("198.51.100.7")
        );
    }

    #[test]
    fn a_listed_proxy_hands_over_the_last_untrusted_hop() {
        let proxies = TrustedProxies::new(&["10.0.0.0/8".to_owned()]);
        let sent = headers(&[("x-forwarded-for", "203.0.113.9, 10.0.0.5")]);
        assert_eq!(proxies.client_ip(ip("10.0.0.1"), &sent), ip("203.0.113.9"));
    }

    #[test]
    fn a_client_cannot_prepend_a_hop_to_become_someone_else() {
        // The attacker sends `X-Forwarded-For: 9.9.9.9`; the real proxy
        // appends the attacker's own address, which is the one that counts.
        let proxies = TrustedProxies::new(&["10.0.0.0/8".to_owned()]);
        let sent = headers(&[("x-forwarded-for", "9.9.9.9, 203.0.113.9")]);
        assert_eq!(proxies.client_ip(ip("10.0.0.1"), &sent), ip("203.0.113.9"));
    }

    #[test]
    fn an_untrusted_peer_is_charged_even_when_a_proxy_list_exists() {
        let proxies = TrustedProxies::new(&["10.0.0.0/8".to_owned()]);
        let sent = headers(&[("x-forwarded-for", "203.0.113.9")]);
        assert_eq!(
            proxies.client_ip(ip("198.51.100.7"), &sent),
            ip("198.51.100.7")
        );
    }

    #[test]
    fn cloudflares_header_is_read_only_when_cloudflare_is_listed() {
        let sent = headers(&[
            ("cf-connecting-ip", "203.0.113.9"),
            ("x-forwarded-for", "198.51.100.1"),
        ]);
        let plain = TrustedProxies::new(&["10.0.0.0/8".to_owned()]);
        assert_eq!(plain.client_ip(ip("10.0.0.1"), &sent), ip("198.51.100.1"));
        let cloudflare = TrustedProxies::new(&["173.245.48.0/20".to_owned()]);
        assert_eq!(
            cloudflare.client_ip(ip("173.245.48.9"), &sent),
            ip("203.0.113.9")
        );
    }

    #[test]
    fn a_port_and_the_rfc_7239_spelling_both_parse() {
        let proxies = TrustedProxies::new(&["10.0.0.0/8".to_owned()]);
        let sent = headers(&[("x-forwarded-for", "203.0.113.9:51234")]);
        assert_eq!(proxies.client_ip(ip("10.0.0.1"), &sent), ip("203.0.113.9"));
        let sent = headers(&[("forwarded", "for=\"[2001:db8::1]:443\"")]);
        assert_eq!(proxies.client_ip(ip("10.0.0.1"), &sent), ip("2001:db8::1"));
    }

    #[test]
    fn a_cidr_matches_only_inside_its_prefix() {
        let range = Cidr::parse("192.168.1.0/24").expect("a range");
        assert!(range.contains(ip("192.168.1.255")));
        assert!(!range.contains(ip("192.168.2.1")));
        assert!(
            Cidr::parse("2001:db8::/32")
                .expect("a range")
                .contains(ip("2001:db8:1::1"))
        );
        assert!(
            Cidr::parse("10.0.0.1")
                .expect("a bare address")
                .contains(ip("10.0.0.1"))
        );
        assert!(Cidr::parse("not-a-range").is_none());
        assert!(Cidr::parse("10.0.0.0/40").is_none());
    }
}
