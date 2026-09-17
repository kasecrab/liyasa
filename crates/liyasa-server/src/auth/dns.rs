//! The DNS seam the domain flow verifies against (HOST-20).
//!
//! A [`Resolver`] rather than a direct `hickory-resolver` call, for two
//! reasons. HOST-20's acceptance test is "given a custom domain with a test
//! DNS server", and a trait the test supplies is that server at the only point
//! the flow can tell the difference; and HOST-08 forbids an offline instance
//! any outbound request, which is one implementation of this trait rather than
//! a condition threaded through the flow (RFC 1502).

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::RwLock;

use liyasa_core::net::BoxFut;

/// The records the domain flow reads. Nothing here writes DNS.
pub trait Resolver: std::fmt::Debug + Send + Sync {
    fn txt<'a>(&'a self, name: &'a str) -> BoxFut<'a, Vec<String>>;
    fn cname<'a>(&'a self, name: &'a str) -> BoxFut<'a, Option<String>>;
    /// `A` and `AAAA` together: an apex uses an ALIAS or an address record
    /// because CNAME at the apex is not allowed.
    fn addresses<'a>(&'a self, name: &'a str) -> BoxFut<'a, Vec<IpAddr>>;
}

/// A zone held in memory: the test DNS server of HOST-20's acceptance test,
/// and the only resolver an offline instance has.
#[derive(Debug, Default)]
pub struct Zone {
    records: RwLock<Records>,
}

#[derive(Debug, Default)]
struct Records {
    txt: BTreeMap<String, Vec<String>>,
    cname: BTreeMap<String, String>,
    addresses: BTreeMap<String, Vec<IpAddr>>,
}

impl Zone {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_txt(&self, name: &str, values: impl IntoIterator<Item = impl Into<String>>) {
        self.write().txt.insert(
            normalize(name),
            values.into_iter().map(Into::into).collect(),
        );
    }

    pub fn set_cname(&self, name: &str, target: &str) {
        self.write()
            .cname
            .insert(normalize(name), normalize(target));
    }

    pub fn set_addresses(&self, name: &str, addresses: impl IntoIterator<Item = IpAddr>) {
        self.write()
            .addresses
            .insert(normalize(name), addresses.into_iter().collect());
    }

    pub fn clear(&self, name: &str) {
        let name = normalize(name);
        let mut records = self.write();
        records.txt.remove(&name);
        records.cname.remove(&name);
        records.addresses.remove(&name);
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Records> {
        self.records.write().unwrap_or_else(|e| e.into_inner())
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Records> {
        self.records.read().unwrap_or_else(|e| e.into_inner())
    }
}

impl Resolver for Zone {
    fn txt<'a>(&'a self, name: &'a str) -> BoxFut<'a, Vec<String>> {
        let values = self
            .read()
            .txt
            .get(&normalize(name))
            .cloned()
            .unwrap_or_default();
        Box::pin(async move { values })
    }

    fn cname<'a>(&'a self, name: &'a str) -> BoxFut<'a, Option<String>> {
        let target = self.read().cname.get(&normalize(name)).cloned();
        Box::pin(async move { target })
    }

    fn addresses<'a>(&'a self, name: &'a str) -> BoxFut<'a, Vec<IpAddr>> {
        let addresses = self
            .read()
            .addresses
            .get(&normalize(name))
            .cloned()
            .unwrap_or_default();
        Box::pin(async move { addresses })
    }
}

/// A resolver that answers nothing, which is what an offline instance has
/// (HOST-08). Every verification against it fails as unverified rather than
/// making a request.
#[derive(Debug, Default, Clone, Copy)]
pub struct Offline;

impl Resolver for Offline {
    fn txt<'a>(&'a self, _name: &'a str) -> BoxFut<'a, Vec<String>> {
        Box::pin(async { Vec::new() })
    }

    fn cname<'a>(&'a self, _name: &'a str) -> BoxFut<'a, Option<String>> {
        Box::pin(async { None })
    }

    fn addresses<'a>(&'a self, _name: &'a str) -> BoxFut<'a, Vec<IpAddr>> {
        Box::pin(async { Vec::new() })
    }
}

/// A host as DNS compares it: lower case, no trailing dot.
pub fn normalize(name: &str) -> String {
    name.trim().trim_end_matches('.').to_ascii_lowercase()
}

/// Whether a host is a zone apex, where CNAME is not allowed and an ALIAS or
/// an address record stands in. Two labels, or three where the second-to-last
/// is a known public suffix of the `co.uk` shape.
pub fn is_apex(host: &str) -> bool {
    let name = normalize(host);
    let labels: Vec<&str> = name.split('.').filter(|l| !l.is_empty()).collect();
    match labels.len() {
        0 | 1 => true,
        2 => true,
        3 => matches!(
            labels[1],
            "co" | "com" | "net" | "org" | "gov" | "edu" | "ac"
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[tokio::test]
    async fn a_zone_answers_what_it_was_given_and_nothing_else() {
        let zone = Zone::new();
        zone.set_txt("_liyasa-challenge.docs.example.com", ["token-a"]);
        zone.set_cname("docs.example.com", "sites.liyasa.dev");
        zone.set_addresses("example.com", [IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5))]);

        assert_eq!(
            zone.txt("_liyasa-challenge.docs.example.com").await,
            ["token-a"]
        );
        assert_eq!(
            zone.cname("docs.example.com").await.as_deref(),
            Some("sites.liyasa.dev")
        );
        assert_eq!(zone.addresses("example.com").await.len(), 1);
        assert!(zone.txt("absent.example.com").await.is_empty());
        assert!(zone.cname("absent.example.com").await.is_none());
    }

    #[tokio::test]
    async fn a_lookup_is_case_insensitive_and_ignores_a_trailing_dot() {
        let zone = Zone::new();
        zone.set_cname("Docs.Example.COM.", "sites.liyasa.dev");
        assert_eq!(
            zone.cname("docs.example.com").await.as_deref(),
            Some("sites.liyasa.dev")
        );
        assert_eq!(
            zone.cname("DOCS.EXAMPLE.COM.").await.as_deref(),
            Some("sites.liyasa.dev")
        );
    }

    #[tokio::test]
    async fn clearing_a_name_removes_every_record_type() {
        let zone = Zone::new();
        zone.set_txt("x.example.com", ["a"]);
        zone.set_cname("x.example.com", "y");
        zone.clear("x.example.com");
        assert!(zone.txt("x.example.com").await.is_empty());
        assert!(zone.cname("x.example.com").await.is_none());
    }

    #[tokio::test]
    async fn an_offline_resolver_answers_nothing_at_all() {
        assert!(Offline.txt("anything").await.is_empty());
        assert!(Offline.cname("anything").await.is_none());
        assert!(Offline.addresses("anything").await.is_empty());
    }

    #[test]
    fn an_apex_is_where_a_cname_is_not_allowed() {
        assert!(is_apex("example.com"));
        assert!(is_apex("example.co.uk"));
        assert!(!is_apex("docs.example.com"));
        assert!(!is_apex("docs.example.co.uk"));
        assert!(!is_apex("a.b.c.example.com"));
    }
}
