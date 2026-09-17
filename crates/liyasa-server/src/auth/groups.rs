//! Who may see what (AUTH-07, AUTH-10, AUTH-11).
//!
//! One decision function, called by every surface. AUTH-10 says search, the
//! assistant, `llms.txt` and the Markdown routes filter identically, and the
//! way to make that true rather than aspirational is for there to be one
//! [`decide`] and no second opinion anywhere.
//!
//! A page's effective groups are its own plus every navigation ancestor's, and
//! a reader must satisfy each level that declares any: within a level, any one
//! group is enough; across levels, all of them must be satisfied. The spec
//! names both shapes and does not say how they compose (RFC 1503).

use std::collections::BTreeSet;

use crate::auth::session::Principal;

/// What a page or navigation node declared.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Declared {
    /// `groups: [...]` on the page or the navigation node. Empty means the
    /// level places no restriction of its own.
    pub groups: BTreeSet<String>,
    /// `access: public` (§7.6): visible on a private-by-default site without
    /// a session.
    pub public: bool,
}

impl Declared {
    pub fn groups<I, S>(groups: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            groups: groups.into_iter().map(Into::into).collect(),
            public: false,
        }
    }

    pub fn public() -> Self {
        Self {
            groups: BTreeSet::new(),
            public: true,
        }
    }

    pub fn is_open(&self) -> bool {
        self.groups.is_empty() && !self.public
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// No session, and one would settle it: the reader is sent to the login
    /// flow rather than told the page does not exist.
    SignIn,
    /// A session that does not qualify. AUTH-10's "a reader sees only what
    /// their groups allow" is about what is *listed*, so the caller renders
    /// this as a 404 on a route and as an omission in a listing.
    Deny,
}

impl Decision {
    pub fn is_allowed(self) -> bool {
        self == Decision::Allow
    }
}

/// `public: true` (AUTH-01) or `public: false` (§7.6) for the site as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteDefault {
    Public,
    Private,
}

/// The one access decision.
///
/// `chain` is the navigation ancestors from the root down, then the page
/// itself. A caller with no navigation passes just the page.
pub fn decide(site: SiteDefault, chain: &[Declared], reader: Option<&Principal>) -> Decision {
    // `access: public` anywhere on the chain opens that level, but a
    // restricted ancestor still applies: a public page under a partner-only
    // section is reachable only by a partner, which is what §7.6 means by
    // "private by default with `access: public` pages" — the exception is to
    // the *default*, not to an explicit restriction.
    let page_is_public = chain.last().is_some_and(|d| d.public);

    let mut needs_session = site == SiteDefault::Private && !page_is_public;
    for level in chain {
        if level.groups.is_empty() {
            continue;
        }
        needs_session = true;
        let Some(reader) = reader else {
            return Decision::SignIn;
        };
        if !level
            .groups
            .iter()
            .any(|group| reader.groups.contains(group))
        {
            return Decision::Deny;
        }
    }
    match needs_session && reader.is_none() {
        true => Decision::SignIn,
        false => Decision::Allow,
    }
}

/// Filters a listing — navigation, search results, `llms.txt`, a sitemap —
/// keeping what the reader may see. A denied entry is omitted rather than
/// shown as locked: AUTH-10 says the reader sees only what their groups allow.
pub fn filter<'a, T, F>(
    site: SiteDefault,
    reader: Option<&Principal>,
    items: impl IntoIterator<Item = &'a T>,
    chain_of: F,
) -> Vec<&'a T>
where
    T: 'a,
    F: Fn(&T) -> Vec<Declared>,
{
    items
        .into_iter()
        .filter(|item| decide(site, &chain_of(item), reader).is_allowed())
        .collect()
}

/// The groups a page's `:::visibility` blocks name, which is what its variant
/// set is computed from (AUTH-11). Sorted and deduplicated, so a page that
/// names the same group twice has one variant dimension and not two.
pub fn referenced_groups<'a>(blocks: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    blocks
        .into_iter()
        .flat_map(|list| list.split(','))
        .map(|group| group.trim())
        .filter(|group| !group.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The group subset of a reader that a page's variants are keyed on: only the
/// groups the page actually references (§6.6.3). A reader in forty groups on a
/// page that mentions one has two possible variants, not 2^40.
pub fn variant_groups(
    referenced: &BTreeSet<String>,
    reader: Option<&Principal>,
) -> BTreeSet<String> {
    let Some(reader) = reader else {
        return BTreeSet::new();
    };
    referenced
        .iter()
        .filter(|group| reader.groups.contains(*group))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader(groups: &[&str]) -> Principal {
        Principal::new("reader").with_groups(groups.iter().copied())
    }

    #[test]
    fn a_public_site_serves_an_undeclared_page_to_anyone() {
        assert_eq!(
            decide(SiteDefault::Public, &[Declared::default()], None),
            Decision::Allow
        );
    }

    #[test]
    fn a_private_site_asks_an_anonymous_reader_to_sign_in() {
        assert_eq!(
            decide(SiteDefault::Private, &[Declared::default()], None),
            Decision::SignIn
        );
        assert_eq!(
            decide(
                SiteDefault::Private,
                &[Declared::default()],
                Some(&reader(&[]))
            ),
            Decision::Allow
        );
    }

    #[test]
    fn a_private_site_serves_an_access_public_page_without_a_session() {
        assert_eq!(
            decide(SiteDefault::Private, &[Declared::public()], None),
            Decision::Allow
        );
    }

    #[test]
    fn a_page_with_groups_on_a_public_site_is_a_private_section() {
        let chain = [Declared::groups(["partner"])];
        assert_eq!(decide(SiteDefault::Public, &chain, None), Decision::SignIn);
        assert_eq!(
            decide(SiteDefault::Public, &chain, Some(&reader(&["partner"]))),
            Decision::Allow
        );
        assert_eq!(
            decide(SiteDefault::Public, &chain, Some(&reader(&["staff"]))),
            Decision::Deny
        );
    }

    #[test]
    fn any_one_of_a_levels_groups_is_enough() {
        let chain = [Declared::groups(["partner", "staff"])];
        assert_eq!(
            decide(SiteDefault::Public, &chain, Some(&reader(&["staff"]))),
            Decision::Allow
        );
    }

    #[test]
    fn every_level_that_declares_groups_must_be_satisfied() {
        // A partner-only section containing an admin-only page.
        let chain = [Declared::groups(["partner"]), Declared::groups(["admin"])];
        assert_eq!(
            decide(
                SiteDefault::Public,
                &chain,
                Some(&reader(&["partner", "admin"]))
            ),
            Decision::Allow
        );
        assert_eq!(
            decide(SiteDefault::Public, &chain, Some(&reader(&["admin"]))),
            Decision::Deny,
            "the section is still partner-only"
        );
        assert_eq!(
            decide(SiteDefault::Public, &chain, Some(&reader(&["partner"]))),
            Decision::Deny
        );
    }

    #[test]
    fn access_public_does_not_open_a_restricted_ancestor() {
        let chain = [Declared::groups(["partner"]), Declared::public()];
        assert_eq!(decide(SiteDefault::Private, &chain, None), Decision::SignIn);
        assert_eq!(
            decide(SiteDefault::Private, &chain, Some(&reader(&["staff"]))),
            Decision::Deny
        );
        assert_eq!(
            decide(SiteDefault::Private, &chain, Some(&reader(&["partner"]))),
            Decision::Allow
        );
    }

    #[test]
    fn a_listing_omits_what_the_reader_may_not_see() {
        struct Page {
            route: &'static str,
            groups: &'static [&'static str],
        }
        let pages = [
            Page {
                route: "/public",
                groups: &[],
            },
            Page {
                route: "/partner",
                groups: &["partner"],
            },
            Page {
                route: "/admin",
                groups: &["admin"],
            },
        ];
        let visible = filter(
            SiteDefault::Public,
            Some(&reader(&["partner"])),
            pages.iter(),
            |page| vec![Declared::groups(page.groups.iter().copied())],
        );
        assert_eq!(
            visible.iter().map(|p| p.route).collect::<Vec<_>>(),
            ["/public", "/partner"]
        );
    }

    #[test]
    fn an_anonymous_listing_shows_only_what_needs_no_session() {
        struct Page {
            route: &'static str,
            groups: &'static [&'static str],
        }
        let pages = [
            Page {
                route: "/public",
                groups: &[],
            },
            Page {
                route: "/partner",
                groups: &["partner"],
            },
        ];
        let visible = filter(SiteDefault::Public, None, pages.iter(), |page| {
            vec![Declared::groups(page.groups.iter().copied())]
        });
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].route, "/public");
    }

    #[test]
    fn the_groups_a_page_references_come_from_its_visibility_blocks() {
        let groups = referenced_groups(["admin,partner", " admin ", "", "staff"]);
        assert_eq!(
            groups.iter().map(String::as_str).collect::<Vec<_>>(),
            ["admin", "partner", "staff"]
        );
    }

    #[test]
    fn a_pages_variant_keys_on_the_groups_it_mentions_and_no_others() {
        let referenced = referenced_groups(["admin"]);
        let many = reader(&["admin", "partner", "staff", "beta", "internal"]);
        assert_eq!(
            variant_groups(&referenced, Some(&many)),
            ["admin".to_owned()].into()
        );
        assert!(variant_groups(&referenced, Some(&reader(&["partner"]))).is_empty());
        assert!(variant_groups(&referenced, None).is_empty());
    }
}
