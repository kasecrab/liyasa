//! The CSP sources each third-party integration needs (ANA-60, ANA-61,
//! ANA-62), so enabling one under `integrations` updates the policy by exactly
//! its declared sources and nothing else.
//!
//! The hosts are the ones each vendor documents for its loader as of
//! September 2026. An operator whose vendor moved a host extends the policy in
//! `security.csp` until the row here is corrected.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Integration {
    /// The key under `integrations`.
    pub key: &'static str,
    pub script_src: &'static [&'static str],
    pub connect_src: &'static [&'static str],
    pub img_src: &'static [&'static str],
    pub media_src: &'static [&'static str],
    pub frame_src: &'static [&'static str],
    pub font_src: &'static [&'static str],
}

const fn integration(key: &'static str) -> Integration {
    Integration {
        key,
        script_src: &[],
        connect_src: &[],
        img_src: &[],
        media_src: &[],
        frame_src: &[],
        font_src: &[],
    }
}

pub const INTEGRATIONS: &[Integration] = &[
    // ---- Analytics (ANA-60) ----
    Integration {
        script_src: &["assets.adobedtm.com"],
        connect_src: &["*.omtrdc.net", "*.demdex.net"],
        img_src: &["*.omtrdc.net", "*.demdex.net"],
        ..integration("adobeAnalytics")
    },
    Integration {
        script_src: &["cdn.amplitude.com"],
        connect_src: &["api2.amplitude.com", "api.eu.amplitude.com"],
        ..integration("amplitude")
    },
    Integration {
        script_src: &["www.clarity.ms", "*.clarity.ms"],
        connect_src: &["*.clarity.ms"],
        img_src: &["*.clarity.ms"],
        ..integration("clarity")
    },
    Integration {
        script_src: &["tag.clearbitscripts.com"],
        connect_src: &["reveal.clearbit.com"],
        ..integration("clearbit")
    },
    Integration {
        script_src: &["cdn.usefathom.com"],
        connect_src: &["cdn.usefathom.com"],
        img_src: &["cdn.usefathom.com"],
        ..integration("fathom")
    },
    Integration {
        script_src: &["www.googletagmanager.com"],
        connect_src: &[
            "www.googletagmanager.com",
            "*.google-analytics.com",
            "*.analytics.google.com",
        ],
        img_src: &["www.googletagmanager.com", "*.google-analytics.com"],
        ..integration("ga4")
    },
    Integration {
        script_src: &["www.googletagmanager.com"],
        connect_src: &["www.googletagmanager.com"],
        img_src: &["www.googletagmanager.com"],
        ..integration("gtm")
    },
    Integration {
        script_src: &["cdn.heapanalytics.com", "heapanalytics.com"],
        connect_src: &["heapanalytics.com"],
        img_src: &["heapanalytics.com"],
        ..integration("heap")
    },
    Integration {
        script_src: &["cdn.hightouch-events.com"],
        connect_src: &["*.hightouch-events.com"],
        ..integration("hightouch")
    },
    Integration {
        script_src: &["static.hotjar.com", "script.hotjar.com"],
        connect_src: &["*.hotjar.com", "wss://*.hotjar.com"],
        img_src: &["*.hotjar.com"],
        frame_src: &["vars.hotjar.com"],
        font_src: &["script.hotjar.com"],
        ..integration("hotjar")
    },
    Integration {
        script_src: &["cdn.getkoala.com"],
        connect_src: &["api2.getkoala.com"],
        ..integration("koala")
    },
    Integration {
        script_src: &["cdn.logrocket.io", "cdn.lr-ingest.io", "cdn.lr-in.com"],
        connect_src: &["*.logrocket.io", "*.lr-ingest.io", "*.lr-in.com"],
        ..integration("logrocket")
    },
    Integration {
        script_src: &["cdn.mxpnl.com"],
        connect_src: &["api-js.mixpanel.com", "api-eu.mixpanel.com"],
        ..integration("mixpanel")
    },
    Integration {
        script_src: &["api.pirsch.io"],
        connect_src: &["api.pirsch.io"],
        ..integration("pirsch")
    },
    Integration {
        script_src: &["plausible.io"],
        connect_src: &["plausible.io"],
        ..integration("plausible")
    },
    Integration {
        script_src: &["*.i.posthog.com"],
        connect_src: &["*.i.posthog.com"],
        ..integration("posthog")
    },
    Integration {
        script_src: &["cdn.segment.com"],
        connect_src: &["api.segment.io", "cdn.segment.com"],
        ..integration("segment")
    },
    // ---- Support widgets (ANA-61) ----
    Integration {
        script_src: &["widget.intercom.io", "js.intercomcdn.com"],
        connect_src: &["*.intercom.io", "wss://*.intercom.io", "*.intercomcdn.com"],
        img_src: &["*.intercomcdn.com", "*.intercom.io"],
        media_src: &["*.intercomcdn.com"],
        frame_src: &["*.intercom.io"],
        font_src: &["js.intercomcdn.com"],
        ..integration("intercom")
    },
    Integration {
        script_src: &["chat-assets.frontapp.com"],
        connect_src: &["*.frontapp.com", "wss://*.frontapp.com"],
        frame_src: &["*.frontapp.com"],
        ..integration("front")
    },
    Integration {
        script_src: &["client.crisp.chat"],
        connect_src: &[
            "client.crisp.chat",
            "wss://client.relay.crisp.chat",
            "storage.crisp.chat",
        ],
        img_src: &["*.crisp.chat"],
        media_src: &["client.crisp.chat"],
        frame_src: &["game.crisp.chat"],
        font_src: &["client.crisp.chat"],
        ..integration("crisp")
    },
    Integration {
        script_src: &["static.zdassets.com", "ekr.zdassets.com"],
        connect_src: &[
            "*.zdassets.com",
            "*.zendesk.com",
            "wss://*.zendesk.com",
            "*.zopim.com",
            "wss://*.zopim.com",
        ],
        img_src: &["*.zdassets.com", "*.zendesk.com"],
        media_src: &["static.zdassets.com"],
        frame_src: &["*.zendesk.com"],
        font_src: &["static.zdassets.com"],
        ..integration("zendesk")
    },
    Integration {
        script_src: &["chat.cdn-plain.com"],
        connect_src: &["*.plain.com", "wss://*.plain.com", "*.cdn-plain.com"],
        frame_src: &["chat.cdn-plain.com"],
        ..integration("plain")
    },
    // ---- Consent providers (ANA-61) ----
    Integration {
        script_src: &["cmp.osano.com"],
        connect_src: &["*.osano.com"],
        img_src: &["*.osano.com"],
        ..integration("osano")
    },
    Integration {
        script_src: &["cdn.transcend.io", "*.transcend-cdn.com"],
        connect_src: &["*.transcend.io", "*.transcend-cdn.com"],
        ..integration("transcend")
    },
    Integration {
        script_src: &["cdn.cookielaw.org", "*.onetrust.com"],
        connect_src: &["cdn.cookielaw.org", "*.onetrust.com"],
        img_src: &["cdn.cookielaw.org"],
        ..integration("onetrust")
    },
    Integration {
        script_src: &["consent.cookiebot.com", "consentcdn.cookiebot.com"],
        connect_src: &["consent.cookiebot.com", "consentcdn.cookiebot.com"],
        img_src: &["imgsct.cookiebot.com"],
        frame_src: &["consentcdn.cookiebot.com"],
        ..integration("cookiebot")
    },
    // The built-in banner is first-party and declares nothing.
    integration("builtin"),
];

pub fn by_key(key: &str) -> Option<&'static Integration> {
    INTEGRATIONS.iter().find(|entry| entry.key == key)
}

/// The integrations `config.integrations` enables, in registry order.
///
/// An analytics or support key is enabled when present and not `false`;
/// `cookieConsent` names a provider, as a string or as `{ "provider": … }`.
/// `telemetry` is Liyasa's own CLI telemetry and loads nothing in a page.
pub fn enabled(config: &Value) -> Vec<&'static Integration> {
    let Some(block) = config.get("integrations").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut keys: Vec<String> = Vec::new();
    for (key, value) in block {
        match key.as_str() {
            "telemetry" => {}
            "cookieConsent" => {
                let provider = match value {
                    Value::String(name) => Some(name.clone()),
                    Value::Object(object) => object
                        .get("provider")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    Value::Bool(true) => Some("builtin".to_owned()),
                    _ => None,
                };
                if let Some(provider) = provider {
                    keys.push(provider);
                }
            }
            other => {
                if !matches!(value, Value::Bool(false) | Value::Null) {
                    keys.push(other.to_owned());
                }
            }
        }
    }
    INTEGRATIONS
        .iter()
        .filter(|entry| keys.iter().any(|key| key == entry.key))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn every_key_is_unique_and_every_host_is_a_bare_source() {
        for (i, entry) in INTEGRATIONS.iter().enumerate() {
            assert!(
                INTEGRATIONS[..i].iter().all(|other| other.key != entry.key),
                "{} is declared twice",
                entry.key
            );
            for host in entry
                .script_src
                .iter()
                .chain(entry.connect_src)
                .chain(entry.img_src)
                .chain(entry.media_src)
                .chain(entry.frame_src)
                .chain(entry.font_src)
            {
                assert!(
                    !host.contains(' ') && !host.contains('\'') && !host.ends_with('/'),
                    "{host} is not a CSP source expression"
                );
                assert!(
                    !host.starts_with("https://"),
                    "{host}: a scheme-less host matches https and nothing else under `default-src 'self'`"
                );
            }
        }
    }

    #[test]
    fn the_prd_list_is_covered() {
        for key in [
            "adobeAnalytics",
            "amplitude",
            "clarity",
            "clearbit",
            "fathom",
            "ga4",
            "gtm",
            "heap",
            "hightouch",
            "hotjar",
            "koala",
            "logrocket",
            "mixpanel",
            "pirsch",
            "plausible",
            "posthog",
            "segment",
            "intercom",
            "front",
            "crisp",
            "zendesk",
            "plain",
            "osano",
            "transcend",
            "onetrust",
            "cookiebot",
            "builtin",
        ] {
            assert!(by_key(key).is_some(), "{key} is not in the registry");
        }
    }

    #[test]
    fn enabling_an_integration_selects_exactly_it() {
        let config = json!({
            "integrations": {
                "plausible": { "id": "docs.acme.com", "consent": "none" },
                "hotjar": false,
                "telemetry": true,
                "cookieConsent": "cookiebot",
                "unknownVendor": { "id": "x" }
            }
        });
        let keys: Vec<&str> = enabled(&config).iter().map(|entry| entry.key).collect();
        assert_eq!(keys, ["plausible", "cookiebot"]);
    }

    #[test]
    fn the_built_in_banner_and_telemetry_declare_nothing() {
        let config = json!({ "integrations": { "cookieConsent": true, "telemetry": true } });
        let entries = enabled(&config);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "builtin");
        assert!(entries[0].script_src.is_empty());
        assert!(enabled(&json!({})).is_empty());
    }
}
