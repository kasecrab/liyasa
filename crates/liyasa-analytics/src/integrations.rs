//! Third-party integrations and the consent gate (ANA-60, ANA-07, ANA-62).
//!
//! An integration is enabled by a key in `integrations` that is present and
//! not `false` or `null` (CFG-81). Every one of them is gated: a row that does
//! not say `consent: "none"` does not load before the consent provider reports
//! a grant. RFC 1703 records why absent means `required` rather than `none`.
//!
//! Liyasa maintains each vendor's script and connection origins (ANA-62), so
//! enabling one never leaves an operator to work out why the content security
//! policy blocked it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VendorKind {
    /// ANA-60's list.
    Analytics,
    /// ANA-61's support widgets.
    Support,
    /// ANA-61's consent providers. These load before consent by definition —
    /// they are what asks for it.
    Consent,
    /// Liyasa's own, which is not a third party and is not gated.
    FirstParty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vendor {
    /// The `integrations` key in `schemas/liyasa.schema.json`.
    pub key: &'static str,
    pub name: &'static str,
    pub kind: VendorKind,
    /// `script-src` origins (ANA-62).
    pub script_src: &'static [&'static str],
    /// `connect-src` origins.
    pub connect_src: &'static [&'static str],
    /// Whether the vendor is known to write cookies. Not the consent rule —
    /// every third party is gated — but an operator choosing between two
    /// vendors wants to know.
    pub sets_cookies: bool,
}

/// The seventeen ANA-60 names, the five support widgets and four consent
/// providers of ANA-61, and Liyasa's own telemetry key.
///
/// Keys match `schemas/liyasa.schema.json`; `tests/it/integrations.rs` holds
/// the two to each other, because a key that exists here and not in the schema
/// is an integration an operator cannot turn on.
pub const VENDORS: &[Vendor] = &[
    Vendor {
        key: "adobeAnalytics",
        name: "Adobe Analytics",
        kind: VendorKind::Analytics,
        script_src: &["https://assets.adobedtm.com"],
        connect_src: &["https://*.demdex.net", "https://*.omtrdc.net"],
        sets_cookies: true,
    },
    Vendor {
        key: "amplitude",
        name: "Amplitude",
        kind: VendorKind::Analytics,
        script_src: &["https://cdn.amplitude.com"],
        connect_src: &["https://api2.amplitude.com", "https://api.eu.amplitude.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "clarity",
        name: "Microsoft Clarity",
        kind: VendorKind::Analytics,
        script_src: &["https://www.clarity.ms"],
        connect_src: &["https://*.clarity.ms"],
        sets_cookies: true,
    },
    Vendor {
        key: "clearbit",
        name: "Clearbit",
        kind: VendorKind::Analytics,
        script_src: &["https://tag.clearbitscripts.com"],
        connect_src: &["https://x.clearbitjs.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "fathom",
        name: "Fathom",
        kind: VendorKind::Analytics,
        script_src: &["https://cdn.usefathom.com"],
        connect_src: &["https://cdn.usefathom.com"],
        sets_cookies: false,
    },
    Vendor {
        key: "ga4",
        name: "Google Analytics 4",
        kind: VendorKind::Analytics,
        script_src: &["https://www.googletagmanager.com"],
        connect_src: &[
            "https://www.google-analytics.com",
            "https://*.analytics.google.com",
        ],
        sets_cookies: true,
    },
    Vendor {
        key: "gtm",
        name: "Google Tag Manager",
        kind: VendorKind::Analytics,
        script_src: &["https://www.googletagmanager.com"],
        connect_src: &["https://www.googletagmanager.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "heap",
        name: "Heap",
        kind: VendorKind::Analytics,
        script_src: &["https://cdn.heapanalytics.com"],
        connect_src: &["https://heapanalytics.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "hightouch",
        name: "Hightouch",
        kind: VendorKind::Analytics,
        script_src: &["https://cdn.hightouch-events.com"],
        connect_src: &["https://us-east-1.hightouch-events.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "hotjar",
        name: "Hotjar",
        kind: VendorKind::Analytics,
        script_src: &["https://static.hotjar.com", "https://script.hotjar.com"],
        connect_src: &["https://*.hotjar.com", "wss://*.hotjar.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "koala",
        name: "Koala",
        kind: VendorKind::Analytics,
        script_src: &["https://cdn.getkoala.com"],
        connect_src: &["https://api2.getkoala.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "logrocket",
        name: "LogRocket",
        kind: VendorKind::Analytics,
        script_src: &["https://cdn.logrocket.io", "https://cdn.lr-ingest.io"],
        connect_src: &["https://*.logrocket.io", "https://*.lr-ingest.io"],
        sets_cookies: true,
    },
    Vendor {
        key: "mixpanel",
        name: "Mixpanel",
        kind: VendorKind::Analytics,
        script_src: &["https://cdn.mxpnl.com"],
        connect_src: &["https://api-js.mixpanel.com", "https://api-eu.mixpanel.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "pirsch",
        name: "Pirsch",
        kind: VendorKind::Analytics,
        script_src: &["https://api.pirsch.io"],
        connect_src: &["https://api.pirsch.io"],
        sets_cookies: false,
    },
    Vendor {
        key: "plausible",
        name: "Plausible",
        kind: VendorKind::Analytics,
        script_src: &["https://plausible.io"],
        connect_src: &["https://plausible.io"],
        sets_cookies: false,
    },
    Vendor {
        key: "posthog",
        name: "PostHog",
        kind: VendorKind::Analytics,
        script_src: &["https://*.posthog.com"],
        connect_src: &["https://*.posthog.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "segment",
        name: "Segment",
        kind: VendorKind::Analytics,
        script_src: &["https://cdn.segment.com"],
        connect_src: &["https://api.segment.io"],
        sets_cookies: true,
    },
    Vendor {
        key: "intercom",
        name: "Intercom",
        kind: VendorKind::Support,
        script_src: &["https://widget.intercom.io", "https://js.intercomcdn.com"],
        connect_src: &[
            "https://api-iam.intercom.io",
            "wss://nexus-websocket-a.intercom.io",
        ],
        sets_cookies: true,
    },
    Vendor {
        key: "front",
        name: "Front",
        kind: VendorKind::Support,
        script_src: &["https://chat-assets.frontapp.com"],
        connect_src: &["https://api.frontapp.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "crisp",
        name: "Crisp",
        kind: VendorKind::Support,
        script_src: &["https://client.crisp.chat"],
        connect_src: &["https://client.crisp.chat", "wss://client.relay.crisp.chat"],
        sets_cookies: true,
    },
    Vendor {
        key: "zendesk",
        name: "Zendesk",
        kind: VendorKind::Support,
        script_src: &["https://static.zdassets.com"],
        connect_src: &["https://*.zendesk.com", "wss://*.zendesk.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "plain",
        name: "Plain",
        kind: VendorKind::Support,
        script_src: &["https://chat.cdn-plain.com"],
        connect_src: &["https://chat.uk.plain.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "osano",
        name: "Osano",
        kind: VendorKind::Consent,
        script_src: &["https://cmp.osano.com"],
        connect_src: &["https://cmp.osano.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "transcend",
        name: "Transcend",
        kind: VendorKind::Consent,
        script_src: &["https://transcend-cdn.com"],
        connect_src: &["https://consent.transcend.io"],
        sets_cookies: true,
    },
    Vendor {
        key: "onetrust",
        name: "OneTrust",
        kind: VendorKind::Consent,
        script_src: &["https://cdn.cookielaw.org"],
        connect_src: &["https://geolocation.onetrust.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "cookiebot",
        name: "Cookiebot",
        kind: VendorKind::Consent,
        script_src: &["https://consent.cookiebot.com"],
        connect_src: &["https://consentcdn.cookiebot.com"],
        sets_cookies: true,
    },
    Vendor {
        key: "builtin",
        name: "Liyasa's minimal banner",
        kind: VendorKind::Consent,
        script_src: &[],
        connect_src: &[],
        sets_cookies: false,
    },
    Vendor {
        key: "telemetry",
        name: "Liyasa CLI telemetry",
        kind: VendorKind::FirstParty,
        script_src: &[],
        connect_src: &[],
        sets_cookies: false,
    },
];

pub fn vendor(key: &str) -> Option<&'static Vendor> {
    VENDORS.iter().find(|v| v.key == key)
}

/// `$defs/integration`'s `consent`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Consent {
    /// The default when the row does not say (RFC 1703).
    #[default]
    Required,
    None,
}

/// One enabled integration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Configured {
    pub key: String,
    pub name: &'static str,
    pub kind: VendorKind,
    /// The vendor's own site or container id, when the row carries one.
    pub id: Option<String>,
    pub consent: Consent,
}

impl Configured {
    /// Whether the script may be in the page before consent is given.
    ///
    /// A consent provider may: it is what asks. Liyasa's own may: it is not a
    /// third party. Everything else may not.
    pub fn loads_before_consent(&self) -> bool {
        match self.kind {
            VendorKind::Consent | VendorKind::FirstParty => true,
            VendorKind::Analytics | VendorKind::Support => self.consent == Consent::None,
        }
    }
}

/// A key the `integrations` block names that Liyasa does not know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unknown(pub String);

/// Reads the `integrations` block.
///
/// Returns what is enabled and the keys that are not vendors. `additionalProperties`
/// is true on that object in the schema, so an unknown key is not a validation
/// error and would otherwise be silently ignored — which is how an operator
/// ends up certain they enabled something they did not.
pub fn configure(block: &Value) -> (Vec<Configured>, Vec<Unknown>) {
    let mut enabled = Vec::new();
    let mut unknown = Vec::new();
    let Some(object) = block.as_object() else {
        return (enabled, unknown);
    };
    for (key, value) in object {
        // `false` and `null` mean off (CFG-81); so does an absent key.
        if value.is_null() || value == &Value::Bool(false) {
            continue;
        }
        // `cookieConsent` is the one key whose VALUE names the vendor:
        // `"cookieConsent": "onetrust"`, or `true` for Liyasa's own banner.
        // Every other key is the vendor.
        let (lookup, id, consent) = if key == CONSENT_KEY {
            let provider = consent_key(value);
            (provider.clone(), Some(provider), Consent::None)
        } else {
            let (id, consent) = read_row(value);
            (key.clone(), id, consent)
        };
        let Some(found) = vendor(&lookup) else {
            unknown.push(Unknown(if key == CONSENT_KEY {
                format!("{key}: {lookup}")
            } else {
                key.clone()
            }));
            continue;
        };
        enabled.push(Configured {
            key: lookup,
            name: found.name,
            kind: found.kind,
            id,
            consent,
        });
    }
    enabled.sort_by(|a, b| a.key.cmp(&b.key));
    (enabled, unknown)
}

/// The key whose value names a vendor rather than being one.
pub const CONSENT_KEY: &str = "cookieConsent";

/// Which provider `cookieConsent` names. `true` is Liyasa's built-in banner
/// (CFG-81).
fn consent_key(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Object(map) => map
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or("builtin")
            .to_owned(),
        _ => "builtin".to_owned(),
    }
}

/// A row is `true`, a bare id string, or an object with `id` and `consent`.
fn read_row(value: &Value) -> (Option<String>, Consent) {
    match value {
        Value::String(text) => (Some(text.clone()), Consent::Required),
        Value::Object(map) => {
            let id = map
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| {
                    map.get("provider")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                });
            let consent = match map.get("consent").and_then(Value::as_str) {
                Some("none") => Consent::None,
                // Anything else, including a value the schema would reject,
                // falls to the safe side.
                _ => Consent::Required,
            };
            (id, consent)
        }
        _ => (None, Consent::Required),
    }
}

/// The content security policy sources an enabled set needs (ANA-62).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Csp {
    pub script_src: Vec<String>,
    pub connect_src: Vec<String>,
}

/// Collects the sources, de-duplicated and ordered so the header is stable
/// between builds.
pub fn csp(configured: &[Configured]) -> Csp {
    let mut script: Vec<String> = Vec::new();
    let mut connect: Vec<String> = Vec::new();
    for item in configured {
        let Some(found) = vendor(&item.key) else {
            continue;
        };
        for source in found.script_src {
            if !script.iter().any(|s| s == source) {
                script.push((*source).to_owned());
            }
        }
        for source in found.connect_src {
            if !connect.iter().any(|s| s == source) {
                connect.push((*source).to_owned());
            }
        }
    }
    script.sort();
    connect.sort();
    Csp {
        script_src: script,
        connect_src: connect,
    }
}

/// The consent provider an operator configured, if any.
pub fn consent_provider(configured: &[Configured]) -> Option<&Configured> {
    configured.iter().find(|c| c.kind == VendorKind::Consent)
}

/// Integrations that are gated but have no provider to gate them: they would
/// never load. A dashboard Settings page says so rather than leaving an
/// operator to notice their analytics are silent.
pub fn gated_without_a_provider(configured: &[Configured]) -> Vec<&Configured> {
    if consent_provider(configured).is_some() {
        return Vec::new();
    }
    configured
        .iter()
        .filter(|c| !c.loads_before_consent())
        .collect()
}
