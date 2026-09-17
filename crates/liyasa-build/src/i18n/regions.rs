//! Regions (PRD §19.6, AUTH-50..AUTH-55).
//!
//! Off unless `regions.enabled`. Liyasa performs no IP geolocation of its own
//! and stores no address: a region arrives from the reader's identity, from a
//! header the operator's edge set, or from a switcher the reader used, and from
//! nowhere else.
//!
//! The header is the one an attacker can write, so it is honoured only when the
//! request reached us from a peer in `server.trustedProxies`. That check is the
//! server's — this module is handed the answer as
//! [`Request::trusted_peer`](Request) and refuses to read a header without it,
//! and [`trust`] reports at build time when the configuration makes the answer
//! permanently `false`.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_components::gate;
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::frontmatter::RegionGate;
use serde_json::Value;

use super::config::{Detection, Regions};
use crate::variants::Caps;

/// Where a reader's chosen region is kept, for `detection: choice`.
pub const COOKIE: &str = "liyasa_region";

/// The query parameter an agent names a region in (AUTH-53).
pub const QUERY: &str = "region";

/// What the server knows about one request.
#[derive(Debug, Clone, Copy, Default)]
pub struct Request<'a> {
    /// The request headers, in the order they arrived.
    pub headers: &'a [(String, String)],
    /// Whether the peer is in `server.trustedProxies`.
    ///
    /// **Every header read is gated on this.** A reader talking to the origin
    /// directly can set `CF-IPCountry` as easily as a CDN can, and a region is
    /// what decides which content exists for them.
    pub trusted_peer: bool,
    /// The region on the reader's payload, for `detection: auth`.
    pub reader: Option<&'a str>,
    /// The [`COOKIE`] value, for `detection: choice`.
    pub chosen: Option<&'a str>,
}

/// How a page is served across regions (AUTH-52).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rendering {
    /// Every variant is in the static export and the switcher picks between
    /// them in the browser. No server.
    Prerendered,
    /// The server picks one variant per request, because only the server knows
    /// the reader's identity or sees the edge's header.
    ServerSelected,
}

#[derive(Debug, Clone)]
pub struct Detector {
    regions: Regions,
}

impl Detector {
    pub fn new(regions: &Regions) -> Self {
        Self {
            regions: regions.clone(),
        }
    }

    pub fn settings(&self) -> &Regions {
        &self.regions
    }

    /// The region this request is in, or `None` when nothing said.
    ///
    /// `regions.detection` is tried in the order it was written, and the first
    /// source that names a declared region wins. A source that names one the
    /// site does not declare is treated as having said nothing rather than as
    /// having said something wrong.
    pub fn detect(&self, request: &Request<'_>) -> Option<String> {
        if !self.regions.enabled {
            return None;
        }
        for how in &self.regions.detection {
            let found = match how {
                Detection::Auth => request.reader.and_then(|code| self.declared(code)),
                Detection::Choice => request.chosen.and_then(|code| self.declared(code)),
                Detection::Header => self.from_header(request),
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }

    /// The region to serve: what was detected, or `regions.default`.
    pub fn resolve(&self, request: &Request<'_>) -> Option<String> {
        if !self.regions.enabled {
            return None;
        }
        self.detect(request).or_else(|| {
            self.regions
                .default
                .as_deref()
                .and_then(|code| self.declared(code))
        })
    }

    /// A region header, and only from a peer we were told to believe.
    fn from_header(&self, request: &Request<'_>) -> Option<String> {
        if !request.trusted_peer {
            return None;
        }
        for name in self.regions.headers() {
            let found = request
                .headers
                .iter()
                .find(|(header, _)| header.eq_ignore_ascii_case(name))
                .and_then(|(_, value)| self.declared(value.trim()));
            if found.is_some() {
                return found;
            }
        }
        None
    }

    /// The declared spelling of a code, whatever case it arrived in. `CF-IPCountry`
    /// sends `US` and a config normally declares `us`.
    fn declared(&self, code: &str) -> Option<String> {
        self.regions
            .list
            .iter()
            .find(|one| one.eq_ignore_ascii_case(code))
            .cloned()
    }

    /// How this site's variants are served (AUTH-52).
    ///
    /// Pre-rendering needs two things: nothing to detect that only a server can
    /// see, and few enough regions that shipping them all is cheaper than a
    /// server. "Small" is `build.maxVariantsPerPage`, which is already the
    /// number of variants this build is willing to write for one page.
    pub fn rendering(&self, caps: &Caps) -> Rendering {
        let server_only = self
            .regions
            .detection
            .iter()
            .any(|how| matches!(how, Detection::Auth | Detection::Header));
        match !server_only && self.regions.list.len() <= caps.per_page {
            true => Rendering::Prerendered,
            false => Rendering::ServerSelected,
        }
    }

    /// Every region the static export carries, plus the default variant
    /// (AUTH-52). The `None` is the page as a reader with no region sees it.
    pub fn export_variants(&self) -> Vec<Option<String>> {
        if !self.regions.enabled {
            return vec![None];
        }
        let mut out = vec![None];
        out.extend(self.regions.list.iter().cloned().map(Some));
        out
    }

    /// Whether a variant's HTML is the one a crawler should index (AUTH-54).
    ///
    /// One variant per page is indexable and it is the default one, so a
    /// region-gated page is one document to a search engine rather than one per
    /// region saying different things at the same URL.
    pub fn is_indexable(&self, region: Option<&str>) -> bool {
        match region {
            None => true,
            Some(region) => self
                .regions
                .default
                .as_deref()
                .is_some_and(|default| default.eq_ignore_ascii_case(region)),
        }
    }
}

/// Whether a page's `regions` front matter admits a variant (AUTH-51).
///
/// The semantics are RFC 0401's and the predicates are
/// `liyasa_components::gate`'s, so a page gate and the `:::region` block gate
/// inside it cannot drift: a gate the build cannot check is a gate that did not
/// hold.
pub fn admits_page(front: Option<&RegionGate>, region: Option<&str>) -> bool {
    let Some(front) = front else {
        return true;
    };
    let only = front.only.clone().unwrap_or_default();
    let except = front.except.clone().unwrap_or_default();
    gate::is_one_of(&only, region)
        && (except.is_empty() || region.is_some_and(|code| !except.iter().any(|one| one == code)))
}

/// The `regions` key of a navigation node, in either spelling the schema's
/// untyped declaration allows: a bare list of codes, or `{ only, except }`.
pub fn node_gate(node: &Value) -> Option<RegionGate> {
    match node.get("regions")? {
        Value::Array(codes) => Some(RegionGate {
            only: Some(
                codes
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
            ),
            except: None,
        }),
        Value::String(code) => Some(RegionGate {
            only: Some(vec![code.clone()]),
            except: None,
        }),
        Value::Object(_) => Some(RegionGate {
            only: list(node.get("regions")?, "only"),
            except: list(node.get("regions")?, "except"),
        }),
        _ => None,
    }
}

fn list(value: &Value, key: &str) -> Option<Vec<String>> {
    Some(
        value
            .get(key)?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
    )
}

/// What an agent asked for: one region, or the union of all of them (AUTH-53).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// No `?region=`: every region's content, each block labelled with the
    /// regions it is in, so an agent is never handed a silently reduced page.
    Union,
    One(String),
}

/// `?region=us` on a Markdown route (AUTH-53).
///
/// An undeclared region is [`Scope::Union`] rather than an empty page: an agent
/// that guesses a code should see everything, not nothing.
pub fn scope_of_query(query: &str, regions: &Regions) -> Scope {
    for pair in query.trim_start_matches('?').split('&') {
        let Some((name, value)) = pair.split_once('=') else {
            continue;
        };
        if name != QUERY {
            continue;
        }
        if let Some(found) = regions
            .list
            .iter()
            .find(|one| one.eq_ignore_ascii_case(value.trim()))
        {
            return Scope::One(found.clone());
        }
    }
    Scope::Union
}

/// The "Available in: US, CA" line a gated block carries in the union render
/// (AUTH-53).
///
/// `None` for a block that is in every region, which needs no label. Codes are
/// upper-cased because AUTH-50's are ISO 3166-1 alpha-2 and an aggregate such
/// as `eu` reads as `EU` beside them.
pub fn label(gate: Option<&RegionGate>, regions: &Regions) -> Option<String> {
    let gate = gate?;
    let only = gate.only.clone().unwrap_or_default();
    let except = gate.except.clone().unwrap_or_default();
    if only.is_empty() && except.is_empty() {
        return None;
    }
    let shown: Vec<String> = match only.is_empty() {
        false => only
            .iter()
            .filter(|code| !except.iter().any(|one| one == *code))
            .cloned()
            .collect(),
        true => regions
            .list
            .iter()
            .filter(|code| !except.iter().any(|one| one == *code))
            .cloned()
            .collect(),
    };
    if shown.is_empty() {
        return Some("Available in: no region".to_owned());
    }
    let names: Vec<String> = shown.iter().map(|code| code.to_uppercase()).collect();
    Some(format!("Available in: {}", names.join(", ")))
}

/// `W0724`: `detection` names `header` and no peer is trusted, so no header is
/// ever read and that detection step does nothing.
pub fn trust(regions: &Regions, trusted_proxies: &[String]) -> Option<Diagnostic> {
    (regions.enabled && regions.detects(Detection::Header) && trusted_proxies.is_empty()).then(
        || {
            Diagnostic::new(
                code::W0724,
                "`regions.detection` reads a region from a request header, and \
                 `server.trustedProxies` lists no peer, so every region header is ignored"
                    .to_owned(),
            )
            .help(
                "add the CIDR of the edge that sets the header to `server.trustedProxies`, or \
                 drop `header` from `regions.detection`",
            )
        },
    )
}

/// `W0725`: something named a region the site does not declare.
///
/// `at` is what named it — a route, a navigation label, a facts key — so the
/// warning points at the file to change rather than at the config.
pub fn undeclared(regions: &Regions, named: &BTreeSet<String>, at: &str) -> Vec<Diagnostic> {
    if !regions.enabled {
        return Vec::new();
    }
    named
        .iter()
        .filter(|code| !regions.is_declared(code))
        .map(|code| {
            let diagnostic = Diagnostic::new(
                code::W0725,
                format!("`{at}` names region `{code}`, which `regions.list` does not declare"),
            );
            match nearest(code, &regions.list) {
                Some(near) => diagnostic.help(format!("did you mean `{near}`?")),
                None => diagnostic.help(format!(
                    "add `{code}` to `regions.list`, or correct the name; the gate admits nobody \
                     as it stands"
                )),
            }
        })
        .collect()
}

/// The declared region a misspelling is one edit away from.
fn nearest<'a>(code: &str, declared: &'a [String]) -> Option<&'a str> {
    declared.iter().map(String::as_str).find(|one| {
        one.eq_ignore_ascii_case(code)
            || (one.len().abs_diff(code.len()) <= 1 && shares_prefix(one, code))
    })
}

fn shares_prefix(a: &str, b: &str) -> bool {
    let shared = a
        .chars()
        .zip(b.chars())
        .take_while(|(x, y)| x.eq_ignore_ascii_case(y))
        .count();
    shared > 0 && shared + 1 >= a.len().min(b.len())
}

/// The availability matrix of AUTH-51, as `Host.features` wants it: feature
/// name to the regions it exists in.
///
/// Two spellings are read, because a facts file is written by hand: a list of
/// region codes, and a table of code to boolean. A feature declared under a
/// namespace keeps its dotted path, so `facts/availability.json`'s
/// `{"billing": {"sso": ["us"]}}` is the feature `billing.sso`.
pub fn features(facts: &Value) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    collect(facts, "", &mut out);
    out
}

fn collect(value: &Value, prefix: &str, out: &mut BTreeMap<String, Vec<String>>) {
    let Some(map) = value.as_object() else {
        return;
    };
    for (key, entry) in map {
        let name = match prefix.is_empty() {
            true => key.clone(),
            false => format!("{prefix}.{key}"),
        };
        match entry {
            Value::Array(codes) => {
                out.insert(
                    name,
                    codes
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect(),
                );
            }
            Value::Object(table) if table.values().all(Value::is_boolean) => {
                out.insert(
                    name,
                    table
                        .iter()
                        .filter(|(_, on)| on.as_bool() == Some(true))
                        .map(|(code, _)| code.clone())
                        .collect(),
                );
            }
            Value::Object(_) => collect(entry, &name, out),
            _ => {}
        }
    }
}

/// Every region name a matrix mentions, for [`undeclared`].
pub fn named_in(features: &BTreeMap<String, Vec<String>>) -> BTreeSet<String> {
    features.values().flatten().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regions(detection: &[&str]) -> Regions {
        Regions {
            enabled: true,
            list: vec!["us".to_owned(), "ca".to_owned(), "eu".to_owned()],
            default: Some("us".to_owned()),
            detection: detection
                .iter()
                .filter_map(|d| Detection::parse(d))
                .collect(),
            header: None,
            availability: None,
        }
    }

    fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect()
    }

    fn gate(only: &[&str], except: &[&str]) -> RegionGate {
        RegionGate {
            only: (!only.is_empty()).then(|| only.iter().map(|c| (*c).to_owned()).collect()),
            except: (!except.is_empty()).then(|| except.iter().map(|c| (*c).to_owned()).collect()),
        }
    }

    #[test]
    fn regions_off_detects_nothing_whatever_the_request_carries() {
        let detector = Detector::new(&Regions {
            enabled: false,
            ..regions(&["auth", "header", "choice"])
        });
        let carried = headers(&[("CF-IPCountry", "CA")]);
        let request = Request {
            headers: &carried,
            trusted_peer: true,
            reader: Some("eu"),
            chosen: Some("ca"),
        };
        assert_eq!(detector.detect(&request), None);
        assert_eq!(detector.resolve(&request), None);
    }

    /// AUTH-50's forgery rule, which is the whole point of the header source.
    #[test]
    fn a_region_header_from_an_untrusted_peer_is_ignored() {
        let detector = Detector::new(&regions(&["header"]));
        let carried = headers(&[("CF-IPCountry", "CA")]);
        let forged = Request {
            headers: &carried,
            trusted_peer: false,
            ..Request::default()
        };
        assert_eq!(detector.detect(&forged), None);
        assert_eq!(
            detector.resolve(&forged),
            Some("us".to_owned()),
            "the reader gets the default, not the region they claimed"
        );

        let from_edge = Request {
            trusted_peer: true,
            ..forged
        };
        assert_eq!(detector.detect(&from_edge), Some("ca".to_owned()));
    }

    #[test]
    fn a_header_is_matched_however_the_edge_cased_it() {
        let detector = Detector::new(&regions(&["header"]));
        let carried = headers(&[("cf-ipcountry", " CA ")]);
        assert_eq!(
            detector.detect(&Request {
                headers: &carried,
                trusted_peer: true,
                ..Request::default()
            }),
            Some("ca".to_owned()),
            "the declared spelling comes back, not the wire spelling"
        );
    }

    #[test]
    fn the_detection_order_decides_which_source_wins() {
        let carried = headers(&[("X-Liyasa-Region", "eu")]);
        let request = Request {
            headers: &carried,
            trusted_peer: true,
            reader: Some("ca"),
            chosen: Some("us"),
        };
        assert_eq!(
            Detector::new(&regions(&["auth", "header", "choice"])).detect(&request),
            Some("ca".to_owned())
        );
        assert_eq!(
            Detector::new(&regions(&["header", "auth"])).detect(&request),
            Some("eu".to_owned())
        );
        assert_eq!(
            Detector::new(&regions(&["choice"])).detect(&request),
            Some("us".to_owned())
        );
    }

    #[test]
    fn a_source_naming_an_undeclared_region_falls_through_to_the_next() {
        let detector = Detector::new(&regions(&["auth", "choice"]));
        assert_eq!(
            detector.detect(&Request {
                reader: Some("jp"),
                chosen: Some("eu"),
                ..Request::default()
            }),
            Some("eu".to_owned())
        );
    }

    /// `?` on the first source's value would have returned from the whole walk,
    /// so a reader with no identity would never have reached the switcher
    /// behind it.
    #[test]
    fn a_source_with_nothing_to_say_falls_through_to_the_next() {
        assert_eq!(
            Detector::new(&regions(&["auth", "choice"])).detect(&Request {
                reader: None,
                chosen: Some("eu"),
                ..Request::default()
            }),
            Some("eu".to_owned())
        );
        let carried = headers(&[("CF-IPCountry", "CA")]);
        assert_eq!(
            Detector::new(&regions(&["auth", "header"])).detect(&Request {
                headers: &carried,
                trusted_peer: true,
                reader: None,
                chosen: None,
            }),
            Some("ca".to_owned())
        );
    }

    #[test]
    fn an_empty_detection_list_leaves_every_reader_in_the_default() {
        let detector = Detector::new(&regions(&[]));
        let carried = headers(&[("CF-IPCountry", "CA")]);
        assert_eq!(
            detector.resolve(&Request {
                headers: &carried,
                trusted_peer: true,
                reader: Some("eu"),
                chosen: Some("ca"),
            }),
            Some("us".to_owned())
        );
    }

    #[test]
    fn a_switcher_alone_is_pre_rendered_and_needs_no_server() {
        let detector = Detector::new(&regions(&["choice"]));
        assert_eq!(detector.rendering(&Caps::default()), Rendering::Prerendered);
        assert_eq!(
            detector.export_variants(),
            vec![
                None,
                Some("us".to_owned()),
                Some("ca".to_owned()),
                Some("eu".to_owned())
            ],
            "the export carries every variant plus a default"
        );
    }

    #[test]
    fn anything_only_a_server_can_see_makes_the_server_choose() {
        for detection in [
            &["header", "choice"][..],
            &["auth"][..],
            &["auth", "choice"][..],
        ] {
            assert_eq!(
                Detector::new(&regions(detection)).rendering(&Caps::default()),
                Rendering::ServerSelected,
                "{detection:?}"
            );
        }
    }

    #[test]
    fn too_many_regions_to_pre_render_is_a_server_too() {
        let caps = Caps {
            per_page: 2,
            ..Caps::default()
        };
        assert_eq!(
            Detector::new(&regions(&["choice"])).rendering(&caps),
            Rendering::ServerSelected,
            "three regions over a cap of two"
        );
    }

    #[test]
    fn only_the_default_variant_is_indexable() {
        let detector = Detector::new(&regions(&["choice"]));
        assert!(detector.is_indexable(Some("us")));
        assert!(!detector.is_indexable(Some("eu")));
        assert!(
            detector.is_indexable(None),
            "an ungated page is one document"
        );
    }

    #[test]
    fn a_page_gate_admits_only_what_it_can_check() {
        assert!(admits_page(None, None), "no gate is no gate");
        assert!(admits_page(Some(&gate(&["us", "ca"], &[])), Some("ca")));
        assert!(!admits_page(Some(&gate(&["us"], &[])), Some("eu")));
        assert!(
            !admits_page(Some(&gate(&["us"], &[])), None),
            "a build that does not know the region cannot show the gate held"
        );
        assert!(admits_page(Some(&gate(&[], &["eu"])), Some("us")));
        assert!(!admits_page(Some(&gate(&[], &["eu"])), Some("eu")));
        assert!(
            !admits_page(Some(&gate(&[], &["eu"])), None),
            "nor that the reader is outside an excluded set"
        );
    }

    #[test]
    fn a_navigation_node_declares_its_regions_in_either_spelling() {
        let bare: Value = serde_json::from_str(r#"{"group":"Billing","regions":["us","ca"]}"#)
            .expect("the fixture is JSON");
        assert_eq!(
            node_gate(&bare).and_then(|gate| gate.only),
            Some(vec!["us".to_owned(), "ca".to_owned()])
        );
        let object: Value =
            serde_json::from_str(r#"{"group":"Billing","regions":{"except":["eu"]}}"#)
                .expect("the fixture is JSON");
        let parsed = node_gate(&object).expect("a gate");
        assert_eq!(parsed.except, Some(vec!["eu".to_owned()]));
        assert!(!admits_page(Some(&parsed), Some("eu")));

        let ungated: Value = serde_json::from_str(r#"{"group":"Billing"}"#).expect("JSON");
        assert_eq!(node_gate(&ungated), None);
    }

    #[test]
    fn an_agent_with_no_query_is_shown_every_region() {
        let regions = regions(&["choice"]);
        assert_eq!(scope_of_query("", &regions), Scope::Union);
        assert_eq!(scope_of_query("?format=md", &regions), Scope::Union);
        assert_eq!(
            scope_of_query("?region=jp", &regions),
            Scope::Union,
            "a code the site does not have shows everything, not nothing"
        );
        assert_eq!(
            scope_of_query("?region=CA&format=md", &regions),
            Scope::One("ca".to_owned())
        );
    }

    #[test]
    fn a_gated_block_says_which_regions_it_is_in() {
        let regions = regions(&["choice"]);
        assert_eq!(
            label(Some(&gate(&["us", "ca"], &[])), &regions).as_deref(),
            Some("Available in: US, CA")
        );
        assert_eq!(
            label(Some(&gate(&[], &["eu"])), &regions).as_deref(),
            Some("Available in: US, CA"),
            "an exclusion is spelled out as the regions that remain"
        );
        assert_eq!(label(None, &regions), None);
        assert_eq!(
            label(Some(&gate(&[], &[])), &regions),
            None,
            "an empty gate is no gate and needs no label"
        );
        assert_eq!(
            label(Some(&gate(&["us"], &["us"])), &regions).as_deref(),
            Some("Available in: no region"),
            "a gate that contradicts itself says so rather than reading as global"
        );
    }

    #[test]
    fn a_header_source_with_no_trusted_peer_is_reported() {
        let diagnostic = trust(&regions(&["header", "choice"]), &[]).expect("W0724");
        assert_eq!(diagnostic.code.as_str(), "W0724");
        assert!(trust(&regions(&["header"]), &["10.0.0.0/8".to_owned()]).is_none());
        assert!(trust(&regions(&["choice"]), &[]).is_none());
        assert!(
            trust(
                &Regions {
                    enabled: false,
                    ..regions(&["header"])
                },
                &[]
            )
            .is_none()
        );
    }

    #[test]
    fn a_region_nothing_declares_is_reported_where_it_was_named() {
        let named: BTreeSet<String> = ["us".to_owned(), "uk".to_owned()].into_iter().collect();
        let found = undeclared(&regions(&["choice"]), &named, "/pricing");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code.as_str(), "W0725");
        assert!(found[0].message.contains("`uk`"), "{}", found[0].message);
        assert!(found[0].message.contains("/pricing"));
    }

    #[test]
    fn nothing_is_reported_while_regions_are_off() {
        let named: BTreeSet<String> = ["uk".to_owned()].into_iter().collect();
        assert!(
            undeclared(
                &Regions {
                    enabled: false,
                    ..regions(&["choice"])
                },
                &named,
                "/pricing"
            )
            .is_empty()
        );
    }

    #[test]
    fn an_availability_matrix_reads_both_spellings() {
        let facts: Value = serde_json::from_str(
            r#"{"sso":["us","eu"],
                "billing":{"instalments":{"us":true,"eu":false,"ca":true}},
                "note":"not a feature"}"#,
        )
        .expect("the fixture is JSON");
        let features = features(&facts);
        assert_eq!(features["sso"], vec!["us".to_owned(), "eu".to_owned()]);
        assert_eq!(
            features["billing.instalments"],
            vec!["ca".to_owned(), "us".to_owned()],
            "a false cell is an absence, not a region"
        );
        assert!(!features.contains_key("note"));
        assert_eq!(
            named_in(&features),
            ["ca".to_owned(), "eu".to_owned(), "us".to_owned()]
                .into_iter()
                .collect::<BTreeSet<String>>()
        );
    }

    #[test]
    fn a_typo_in_the_matrix_is_reported_against_the_feature_that_holds_it() {
        let facts: Value =
            serde_json::from_str(r#"{"sso":["us","eu"],"billing":["usa"]}"#).expect("JSON");
        let features = features(&facts);
        let found = undeclared(
            &regions(&["choice"]),
            &named_in(&features),
            "facts/availability",
        );
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("`usa`"));
        assert_eq!(
            found[0].help.as_deref(),
            Some("did you mean `us`?"),
            "{:?}",
            found[0].help
        );
    }
}
