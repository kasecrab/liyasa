//! The address-class and URL rules, as pure functions (PRD §30.2.3).

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use liyasa_core::net::{DenyReason, HttpPolicy, Purpose, Url};

/// Names that always resolve to instance metadata regardless of DNS.
const METADATA_HOSTS: &[&str] = &["metadata.google.internal", "metadata", "instance-data"];

/// Purposes that never leave TLS (VER-26): fact values, specification
/// references and agent fetches.
pub fn requires_https(purpose: Purpose) -> bool {
    matches!(
        purpose,
        Purpose::FactSource | Purpose::SpecRef | Purpose::AgentFetch
    )
}

/// Whether `addr` is in a class the policy denies: loopback, link-local,
/// private, unique-local, multicast, unspecified, the cloud metadata
/// addresses, and IPv4-mapped forms of the same.
pub fn is_denied_address(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => is_denied_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_denied_v4(mapped);
            }
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6 == Ipv6Addr::new(0xfd00, 0xec2, 0, 0, 0, 0, 0, 0x254)
                // Documentation and benchmarking ranges are not routable either.
                || (v6.segments()[0] == 0x2001 && v6.segments()[1] == 0x0db8)
                // 6to4 and Teredo wrap an IPv4 address; unwrap and re-check.
                || (v6.segments()[0] == 0x2002
                    && is_denied_v4(Ipv4Addr::from(
                        ((v6.segments()[1] as u32) << 16) | v6.segments()[2] as u32,
                    )))
                || (v6.segments()[0] == 0x2001
                    && v6.segments()[1] == 0
                    && is_denied_v4(Ipv4Addr::from(
                        !(((v6.segments()[6] as u32) << 16) | v6.segments()[7] as u32),
                    )))
        }
    }
}

fn is_denied_v4(v4: Ipv4Addr) -> bool {
    let [a, b, _, _] = v4.octets();
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_multicast()
        || v4.is_unspecified()
        || v4.is_documentation()
        // 100.64.0.0/10, carrier-grade NAT: inside the operator's network.
        || (a == 100 && (64..=127).contains(&b))
        // 192.0.0.0/24, IETF protocol assignments.
        || (a == 192 && b == 0 && v4.octets()[2] == 0)
        // 198.18.0.0/15, benchmarking.
        || (a == 198 && (18..=19).contains(&b))
        // 240.0.0.0/4, reserved.
        || a >= 240
}

/// The checks that need only the URL: scheme, credentials, the allow and deny
/// lists, and the metadata names. Address classes are checked at connect time
/// by the client, after resolution.
pub fn check_url(url: &Url, policy: &HttpPolicy, hop: u8) -> Result<(), DenyReason> {
    match url.scheme() {
        "https" => {}
        "http" if !requires_https(policy.purpose) => {}
        _ => return Err(DenyReason::Scheme),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(DenyReason::Credentials);
    }
    let host = url.host_str().ok_or(DenyReason::Scheme)?;
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if METADATA_HOSTS.contains(&host.as_str()) {
        return Err(DenyReason::HostNotAllowed(host));
    }
    if policy.deny_hosts.matches(&host) {
        return Err(DenyReason::HostNotAllowed(host));
    }
    if !policy.allow_hosts.is_empty() && !policy.allow_hosts.matches(&host) {
        return Err(if hop == 0 {
            DenyReason::HostNotAllowed(host)
        } else {
            DenyReason::RedirectHop(hop)
        });
    }
    Ok(())
}

/// The address the policy lets the client connect to, or the reason it
/// cannot. A single denied address in the answer rejects the whole request:
/// an attacker cannot mix a public and a private address.
pub fn check_addresses(addrs: &[IpAddr], policy: &HttpPolicy, hop: u8) -> Result<(), DenyReason> {
    for addr in addrs {
        if is_denied_address(*addr) && !policy.allow_private {
            return Err(if hop == 0 {
                DenyReason::AddressClass(*addr)
            } else {
                DenyReason::RedirectHop(hop)
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().expect("an address")
    }

    #[test]
    fn every_documented_class_is_denied() {
        for text in [
            "127.0.0.1",
            "127.255.255.254",
            "10.0.0.1",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "169.254.0.1",
            "100.64.0.1",
            "100.127.255.255",
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "198.18.0.1",
            "240.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "fd00::1",
            "fc00::1",
            "ff02::1",
            "fd00:ec2::254",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "2002:0a00:0001::1",
        ] {
            assert!(is_denied_address(ip(text)), "{text} must be denied");
        }
    }

    #[test]
    fn public_addresses_are_allowed() {
        for text in [
            "8.8.8.8",
            "1.1.1.1",
            "93.184.216.34",
            "2606:4700::1111",
            "172.32.0.1",
            "100.128.0.1",
        ] {
            assert!(!is_denied_address(ip(text)), "{text} must be allowed");
        }
    }
}
