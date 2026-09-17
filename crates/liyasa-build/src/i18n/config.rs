//! What `liyasa.json` says about locales, regions, and variations (PRD §7.11,
//! §7.12, §19.6).
//!
//! Read from the loaded JSON value for the reason `engine::settings` gives:
//! `liyasa-config` has already validated the document against the schema by the
//! time this runs (CFG-94), and the build wants a default for every key.

use serde_json::Value;

/// One entry of `locales` (CM-100).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleDecl {
    pub code: String,
    /// What the switcher shows, in that locale's own language.
    pub label: String,
    pub default: bool,
}

/// What a reader sees when a page has no translation in their locale (CM-102).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fallback {
    /// The default locale's text under "This page is not yet translated".
    #[default]
    Notice,
    /// Nothing: the page is not served in that locale at all.
    Hide,
}

impl Fallback {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "notice" => Some(Fallback::Notice),
            "hide" => Some(Fallback::Hide),
            _ => None,
        }
    }
}

/// `localization` (CM-102, CM-104).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Localization {
    pub fallback: Fallback,
    /// `localization.routeVisitors`: send a reader to the locale their browser
    /// asks for. Off by default.
    pub route_visitors: bool,
}

impl Localization {
    pub fn from_value(value: &Value) -> Self {
        let node = value.get("localization");
        Self {
            // TODO(rfc-2700): the schema declares no default for
            // `localization.fallback`; CM-102 names the notice first and it is
            // the reading that still serves the page, so it is the default here.
            fallback: node
                .and_then(|node| node.get("fallback"))
                .and_then(Value::as_str)
                .and_then(Fallback::parse)
                .unwrap_or_default(),
            route_visitors: node
                .and_then(|node| node.get("routeVisitors"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }
}

/// How a reader's region is decided (AUTH-50).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detection {
    /// From the reader payload.
    Auth,
    /// From a request header set by the operator's edge.
    Header,
    /// From a switcher in the navbar.
    Choice,
}

impl Detection {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "auth" => Some(Detection::Auth),
            "header" => Some(Detection::Header),
            "choice" => Some(Detection::Choice),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Detection::Auth => "auth",
            Detection::Header => "header",
            Detection::Choice => "choice",
        }
    }
}

/// The headers an edge sets a region in, when `regions.header` names none.
///
/// AUTH-50 lists all three by name and lets the operator configure a fourth.
/// The order is the order they are tried in.
pub const DEFAULT_REGION_HEADERS: &[&str] =
    &["CF-IPCountry", "X-Vercel-IP-Country", "X-Liyasa-Region"];

/// `regions` (AUTH-50).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Regions {
    pub enabled: bool,
    /// ISO 3166-1 alpha-2 codes, or custom aggregates such as `eu`.
    pub list: Vec<String>,
    /// The region a reader is served when none can be determined.
    pub default: Option<String>,
    /// Tried in order; an empty list detects nothing.
    pub detection: Vec<Detection>,
    /// `regions.header`, when the operator's edge sets one of its own.
    pub header: Option<String>,
    /// `regions.availability`: the facts file saying which features exist in
    /// which region (AUTH-51).
    pub availability: Option<String>,
}

impl Regions {
    pub fn from_value(value: &Value) -> Self {
        let Some(node) = value.get("regions") else {
            return Self::default();
        };
        Self {
            enabled: node
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            list: strings(node, "list"),
            default: node
                .get("default")
                .and_then(Value::as_str)
                .map(str::to_owned),
            detection: node
                .get("detection")
                .and_then(Value::as_array)
                .map(|array| {
                    array
                        .iter()
                        .filter_map(Value::as_str)
                        .filter_map(Detection::parse)
                        .collect()
                })
                .unwrap_or_default(),
            header: node
                .get("header")
                .and_then(Value::as_str)
                .map(str::to_owned),
            availability: node
                .get("availability")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }
    }

    /// Whether `code` is one this site has content for.
    pub fn is_declared(&self, code: &str) -> bool {
        self.list.iter().any(|one| one == code)
    }

    /// The headers a region may arrive in, most specific first.
    ///
    /// The operator's own header is tried before the three AUTH-50 names, so an
    /// edge that sets both wins over whatever the CDN left behind.
    pub fn headers(&self) -> Vec<&str> {
        let mut out = Vec::new();
        if let Some(header) = &self.header {
            out.push(header.as_str());
        }
        // TODO(rfc-2700): the schema has one `regions.header`, and AUTH-50
        // names three. An unset key means all three rather than none.
        for name in DEFAULT_REGION_HEADERS {
            if !out.iter().any(|held| held.eq_ignore_ascii_case(name)) {
                out.push(name);
            }
        }
        out
    }

    pub fn detects(&self, how: Detection) -> bool {
        self.detection.contains(&how)
    }
}

/// One entry of `variations` (CM-111).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariationDecl {
    pub name: String,
    pub label: String,
    /// The options a page may name. Empty when the config declares none, in
    /// which case the build learns them from the pages themselves.
    pub options: Vec<String>,
}

impl VariationDecl {
    pub fn from_value(value: &Value) -> Vec<Self> {
        let Some(array) = value.get("variations").and_then(Value::as_array) else {
            return Vec::new();
        };
        array
            .iter()
            .filter_map(|item| {
                let name = item.get("name")?.as_str()?.to_owned();
                Some(Self {
                    label: item
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or(&name)
                        .to_owned(),
                    // TODO(rfc-2700): `schemas/liyasa.schema.json` has no
                    // `options` key, and CM-111's own example declares one.
                    // Read it when it is there; `variations::Variations` derives
                    // the set from the pages when it is not.
                    options: strings(item, "options"),
                    name,
                })
            })
            .collect()
    }
}

fn strings(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(json: &str) -> Value {
        serde_json::from_str(json).expect("the fixture is JSON")
    }

    #[test]
    fn regions_are_off_until_a_config_turns_them_on() {
        let regions = Regions::from_value(&value("{}"));
        assert!(!regions.enabled);
        assert!(regions.list.is_empty());
        assert!(regions.detection.is_empty());
        assert_eq!(regions.default, None);
    }

    #[test]
    fn the_detection_order_is_the_order_it_was_written_in() {
        let regions = Regions::from_value(&value(
            r#"{"regions":{"enabled":true,"list":["us","eu"],"default":"us",
                 "detection":["auth","header","choice"]}}"#,
        ));
        assert!(regions.enabled);
        assert_eq!(
            regions.detection,
            vec![Detection::Auth, Detection::Header, Detection::Choice]
        );
        assert!(regions.detects(Detection::Choice));
        assert!(regions.is_declared("eu"));
        assert!(!regions.is_declared("ca"));
    }

    #[test]
    fn an_unknown_detection_name_is_dropped_rather_than_guessed_at() {
        let regions = Regions::from_value(&value(r#"{"regions":{"detection":["ip","auth"]}}"#));
        assert_eq!(regions.detection, vec![Detection::Auth]);
    }

    #[test]
    fn the_operators_header_is_tried_before_the_three_the_prd_names() {
        let configured = Regions::from_value(&value(r#"{"regions":{"header":"X-Acme-Country"}}"#));
        assert_eq!(
            configured.headers(),
            vec![
                "X-Acme-Country",
                "CF-IPCountry",
                "X-Vercel-IP-Country",
                "X-Liyasa-Region"
            ]
        );
        assert_eq!(
            Regions::default().headers(),
            DEFAULT_REGION_HEADERS.to_vec()
        );
    }

    #[test]
    fn naming_one_of_the_three_does_not_list_it_twice() {
        let regions = Regions::from_value(&value(r#"{"regions":{"header":"cf-ipcountry"}}"#));
        assert_eq!(
            regions.headers(),
            vec!["cf-ipcountry", "X-Vercel-IP-Country", "X-Liyasa-Region"]
        );
    }

    #[test]
    fn localization_defaults_to_a_notice_and_no_visitor_routing() {
        let localization = Localization::from_value(&value("{}"));
        assert_eq!(localization.fallback, Fallback::Notice);
        assert!(!localization.route_visitors);
    }

    #[test]
    fn hiding_an_untranslated_page_is_opt_in() {
        let localization = Localization::from_value(&value(
            r#"{"localization":{"fallback":"hide","routeVisitors":true}}"#,
        ));
        assert_eq!(localization.fallback, Fallback::Hide);
        assert!(localization.route_visitors);
    }

    #[test]
    fn a_variation_reads_its_options_when_the_config_has_them() {
        let decls = VariationDecl::from_value(&value(
            r#"{"variations":[{"name":"deployment","options":["cloud","self-hosted"]},
                 {"name":"edition"}]}"#,
        ));
        assert_eq!(decls.len(), 2);
        assert_eq!(decls[0].options, vec!["cloud", "self-hosted"]);
        assert_eq!(
            decls[0].label, "deployment",
            "the name is the fallback label"
        );
        assert!(decls[1].options.is_empty());
    }
}
