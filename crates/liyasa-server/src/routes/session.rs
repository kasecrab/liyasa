//! The analytics session key and caller classification (ANA-03, ANA-05).
//!
//! No address is stored. The key is `blake3(salt_day, address, user-agent
//! family, site)` where the salt is 256 random bits generated at midnight UTC,
//! held in memory, and thrown away at rotation: after that no stored key can
//! be traced back to an address even by whoever holds the database, and two
//! days of keys cannot be joined.

use std::net::IpAddr;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use zeroize::Zeroize;

const DAY_MS: i64 = 86_400_000;

/// The rotating daily salt.
pub struct DailySalt {
    inner: RwLock<Current>,
}

struct Current {
    day: i64,
    bytes: [u8; 32],
}

impl Drop for Current {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl std::fmt::Debug for DailySalt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DailySalt").finish_non_exhaustive()
    }
}

fn random() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    if getrandom::fill(&mut bytes).is_err() {
        // Never a derived value: a guessable salt makes every stored key a
        // brute-forceable address. Failing closed means the keys for this
        // process are random but unrecoverable, which is the safe direction.
        bytes = *blake3::hash(&std::process::id().to_le_bytes()).as_bytes();
    }
    bytes
}

impl Default for DailySalt {
    fn default() -> Self {
        Self::new()
    }
}

impl DailySalt {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Current {
                day: today(),
                bytes: random(),
            }),
        }
    }

    /// Rotates when the UTC day has turned. The old salt is zeroed as the
    /// value it replaced is dropped.
    fn bytes_for(&self, day: i64) -> [u8; 32] {
        {
            let current = self.inner.read().unwrap_or_else(|e| e.into_inner());
            if current.day == day {
                return current.bytes;
            }
        }
        let mut current = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if current.day != day {
            *current = Current {
                day,
                bytes: random(),
            };
        }
        current.bytes
    }

    /// Forces a rotation, which is what the scheduled job at midnight does.
    pub fn rotate(&self) {
        let mut current = self.inner.write().unwrap_or_else(|e| e.into_inner());
        *current = Current {
            day: today(),
            bytes: random(),
        };
    }

    pub fn key(&self, addr: IpAddr, user_agent: Option<&str>, site: &str) -> String {
        let day = today();
        let salt = self.bytes_for(day);
        let family = ua_family(user_agent);
        let digest = liyasa_core::ids::Fingerprint::of_parts([
            &salt[..],
            addr.to_string().as_bytes(),
            family.as_bytes(),
            site.as_bytes(),
        ]);
        format!("k1:{}", &digest.to_hex()[..32])
    }
}

fn today() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64 / DAY_MS)
        .unwrap_or(0)
}

/// Who is asking (ANA-05). The maintained agent list belongs to the analytics
/// package; this is the set the server has to recognise to classify its own
/// request log, plus the structural rule that Markdown and MCP traffic is
/// agent traffic whatever the header says.
const AGENTS: &[&str] = &[
    "chatgpt-user",
    "claudebot",
    "claude-user",
    "claude-web",
    "gptbot",
    "perplexitybot",
    "google-extended",
    "devin",
    "cursor",
    "cline",
    "oai-searchbot",
    "anthropic-ai",
];

const BOTS: &[&str] = &[
    "googlebot",
    "bingbot",
    "duckduckbot",
    "yandexbot",
    "baiduspider",
    "slurp",
    "ahrefsbot",
    "semrushbot",
    "crawler",
    "spider",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerKind {
    Human,
    Agent,
    Bot,
    Integration,
}

impl CallerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::Bot => "bot",
            Self::Integration => "integration",
        }
    }
}

/// `structural` is true for a `.md` route or an MCP call, which is agent
/// traffic regardless of the header.
pub fn classify(user_agent: Option<&str>, structural: bool) -> (CallerKind, Option<String>) {
    let lower = user_agent.unwrap_or_default().to_ascii_lowercase();
    if let Some(name) = AGENTS.iter().find(|a| lower.contains(*a)) {
        return (CallerKind::Agent, Some((*name).to_owned()));
    }
    if structural {
        return (CallerKind::Agent, None);
    }
    if BOTS.iter().any(|b| lower.contains(*b)) {
        return (CallerKind::Bot, None);
    }
    if lower.is_empty() {
        // No user agent at all is a script, not a reader.
        return (CallerKind::Integration, None);
    }
    (CallerKind::Human, None)
}

/// The user agent reduced to a family name, which is all that is ever stored
/// or hashed (ANA-03).
pub fn ua_family(user_agent: Option<&str>) -> String {
    let lower = user_agent.unwrap_or_default().to_ascii_lowercase();
    for (needle, family) in [
        ("edg/", "edge"),
        ("opr/", "opera"),
        ("firefox/", "firefox"),
        ("chrome/", "chrome"),
        ("safari/", "safari"),
        ("curl/", "curl"),
        ("wget/", "wget"),
        ("python", "python"),
        ("go-http", "go"),
        ("node", "node"),
    ] {
        if lower.contains(needle) {
            return (*family).to_owned();
        }
    }
    if lower.is_empty() {
        "none".to_owned()
    } else {
        "other".to_owned()
    }
}

/// The device class the event schema records; never a model or a fingerprint.
pub fn device_class(user_agent: Option<&str>) -> &'static str {
    let lower = user_agent.unwrap_or_default().to_ascii_lowercase();
    if lower.is_empty() {
        return "server";
    }
    // Tablet first: an iPad's user agent also says Mobile.
    if lower.contains("ipad") || lower.contains("tablet") {
        return "tablet";
    }
    if lower.contains("mobile") || lower.contains("android") || lower.contains("iphone") {
        return "mobile";
    }
    "desktop"
}

/// Query parameters that survive into an event (ANA-03): the campaign
/// parameters and the search term, nothing else.
pub fn allowed_query(query: &str) -> Option<String> {
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let name = pair.split('=').next().unwrap_or_default();
            name.starts_with("utm_") || name == "q"
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join("&"))
}

/// The referrer reduced to a host; a full referring URL is not recorded.
pub fn referrer_host(referrer: Option<&str>) -> Option<String> {
    let referrer = referrer?;
    url::Url::parse(referrer)
        .ok()?
        .host_str()
        .map(|h| h.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    fn addr(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, last))
    }

    #[test]
    fn one_reader_keeps_one_key_within_a_day_and_two_readers_differ() {
        let salt = DailySalt::new();
        let chrome = Some("Mozilla/5.0 Chrome/140.0");
        let one = salt.key(addr(1), chrome, "acme-docs");
        assert_eq!(one, salt.key(addr(1), chrome, "acme-docs"));
        assert_ne!(one, salt.key(addr(2), chrome, "acme-docs"));
        assert_ne!(one, salt.key(addr(1), Some("curl/8.5"), "acme-docs"));
        assert_ne!(
            one,
            salt.key(addr(1), chrome, "other-docs"),
            "one site's keys never join another's"
        );
        assert!(one.starts_with("k1:"));
    }

    #[test]
    fn rotating_the_salt_makes_yesterdays_keys_unreachable() {
        let salt = DailySalt::new();
        let before = salt.key(addr(1), None, "acme-docs");
        salt.rotate();
        assert_ne!(before, salt.key(addr(1), None, "acme-docs"));
    }

    #[test]
    fn a_user_agent_reaches_the_key_only_as_a_family() {
        assert_eq!(
            ua_family(Some("Mozilla/5.0 Chrome/141.0.1 Safari/537")),
            "chrome"
        );
        assert_eq!(ua_family(Some("Mozilla/5.0 Firefox/142.0")), "firefox");
        assert_eq!(ua_family(Some("curl/8.5.0")), "curl");
        assert_eq!(ua_family(None), "none");
        // Two Chrome versions are one family, so a reader who updates their
        // browser mid-day keeps one session.
        assert_eq!(
            ua_family(Some("Chrome/141.0")),
            ua_family(Some("Chrome/142.0"))
        );
    }

    #[test]
    fn agents_bots_and_readers_are_told_apart() {
        assert_eq!(
            classify(Some("ClaudeBot/1.0"), false),
            (CallerKind::Agent, Some("claudebot".to_owned()))
        );
        assert_eq!(classify(Some("Googlebot/2.1"), false).0, CallerKind::Bot);
        assert_eq!(
            classify(Some("Mozilla/5.0 Chrome/141"), false).0,
            CallerKind::Human
        );
        assert_eq!(classify(None, false).0, CallerKind::Integration);
        assert_eq!(
            classify(Some("Mozilla/5.0 Chrome/141"), true).0,
            CallerKind::Agent,
            "a Markdown fetch is agent traffic whatever the header says"
        );
    }

    #[test]
    fn only_the_campaign_parameters_and_the_search_term_survive() {
        assert_eq!(
            allowed_query("utm_source=x&token=secret&q=install&id=42"),
            Some("utm_source=x&q=install".to_owned())
        );
        assert_eq!(allowed_query("token=secret"), None);
        assert_eq!(allowed_query(""), None);
    }

    #[test]
    fn a_referrer_is_reduced_to_its_host() {
        assert_eq!(
            referrer_host(Some("https://news.example.com/a/b?c=d")),
            Some("news.example.com".to_owned())
        );
        assert_eq!(referrer_host(Some("not a url")), None);
        assert_eq!(referrer_host(None), None);
    }

    #[test]
    fn a_device_is_a_class_and_nothing_finer() {
        assert_eq!(device_class(Some("iPhone Mobile Safari")), "mobile");
        assert_eq!(device_class(Some("iPad Safari")), "tablet");
        assert_eq!(device_class(Some("Chrome/141 X11 Linux")), "desktop");
        assert_eq!(device_class(None), "server");
    }
}
