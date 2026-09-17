//! Who is asking (ANA-05).
//!
//! The catalogue ships with the binary and moves with releases: an operator
//! who wants agent adoption counted does not want to maintain a regex list,
//! and a list fetched at runtime would be a network dependency on the request
//! path. [`REVISION`] is what a release note cites.
//!
//! Classification is deliberately conservative. A string is an agent only when
//! it names one, a bot only when it names one, and an integration only when it
//! names a client library or says nothing at all; everything else is a reader.
//! The headless heuristic is separate and never changes the kind — it annotates
//! a row so an operator can ask "and how much of that was a browser nobody was
//! looking at", which is a different question from who sent it.

/// The day the catalogue last changed. Cited in release notes so an operator
/// can tell whether an upgrade would move their agent numbers.
pub const REVISION: &str = "2026-09-17";

/// What an agent is for. Agent traffic is not one thing: a coding agent
/// fetching a page mid-task and a crawler collecting a training corpus both
/// count as `agent`, and an operator reading adoption wants them apart.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Purpose {
    /// Fetching this page because someone asked a question about it now.
    Assistant,
    /// A coding agent working in a repository.
    Coder,
    /// Crawling to build a corpus or an index for later.
    Crawl,
}

/// One row of the maintained list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Agent {
    /// Matched case-insensitively as a substring of the user agent.
    pub token: &'static str,
    pub vendor: &'static str,
    pub purpose: Purpose,
}

/// The maintained list ANA-05 names, plus the ones that have appeared since.
///
/// `google-extended` is a robots.txt token rather than a user agent, and is
/// here because ANA-05 names it: a request that does send it is unambiguous,
/// and a row that never matches costs one substring test.
const AGENTS: &[Agent] = &[
    Agent {
        token: "anthropic-ai",
        vendor: "Anthropic",
        purpose: Purpose::Crawl,
    },
    Agent {
        token: "applebot-extended",
        vendor: "Apple",
        purpose: Purpose::Crawl,
    },
    Agent {
        token: "bytespider",
        vendor: "ByteDance",
        purpose: Purpose::Crawl,
    },
    Agent {
        token: "ccbot",
        vendor: "Common Crawl",
        purpose: Purpose::Crawl,
    },
    Agent {
        token: "chatgpt-user",
        vendor: "OpenAI",
        purpose: Purpose::Assistant,
    },
    Agent {
        token: "claude-user",
        vendor: "Anthropic",
        purpose: Purpose::Assistant,
    },
    Agent {
        token: "claude-web",
        vendor: "Anthropic",
        purpose: Purpose::Assistant,
    },
    Agent {
        token: "claudebot",
        vendor: "Anthropic",
        purpose: Purpose::Crawl,
    },
    Agent {
        token: "cline",
        vendor: "Cline",
        purpose: Purpose::Coder,
    },
    Agent {
        token: "cohere-ai",
        vendor: "Cohere",
        purpose: Purpose::Crawl,
    },
    Agent {
        token: "cursor",
        vendor: "Anysphere",
        purpose: Purpose::Coder,
    },
    Agent {
        token: "devin",
        vendor: "Cognition",
        purpose: Purpose::Coder,
    },
    Agent {
        token: "diffbot",
        vendor: "Diffbot",
        purpose: Purpose::Crawl,
    },
    Agent {
        token: "duckassistbot",
        vendor: "DuckDuckGo",
        purpose: Purpose::Assistant,
    },
    Agent {
        token: "google-extended",
        vendor: "Google",
        purpose: Purpose::Crawl,
    },
    Agent {
        token: "gptbot",
        vendor: "OpenAI",
        purpose: Purpose::Crawl,
    },
    Agent {
        token: "meta-externalagent",
        vendor: "Meta",
        purpose: Purpose::Crawl,
    },
    Agent {
        token: "oai-searchbot",
        vendor: "OpenAI",
        purpose: Purpose::Assistant,
    },
    Agent {
        token: "perplexity-user",
        vendor: "Perplexity",
        purpose: Purpose::Assistant,
    },
    Agent {
        token: "perplexitybot",
        vendor: "Perplexity",
        purpose: Purpose::Crawl,
    },
    Agent {
        token: "windsurf",
        vendor: "Codeium",
        purpose: Purpose::Coder,
    },
    Agent {
        token: "youbot",
        vendor: "You.com",
        purpose: Purpose::Assistant,
    },
];

/// Search and SEO crawlers: traffic that is neither a reader nor an agent
/// acting for one.
const BOTS: &[&str] = &[
    "ahrefsbot",
    "baiduspider",
    "bingbot",
    "crawler",
    "duckduckbot",
    "googlebot",
    "mj12bot",
    "petalbot",
    "semrushbot",
    "slurp",
    "spider",
    "yandexbot",
];

/// Client libraries and tools. Requests from these are scripted rather than
/// read, and counting them as readers is how a page with a health check on it
/// grows a thousand daily visitors.
const CLIENTS: &[&str] = &[
    "axios",
    "curl",
    "go-http",
    "guzzle",
    "httpie",
    "insomnia",
    "java/",
    "libwww",
    "node-fetch",
    "okhttp",
    "postman",
    "python",
    "ruby",
    "undici",
    "wget",
];

/// Runtimes that announce themselves in the string. A browser under automation
/// usually strips these; the ones that do not are the cheap half of the check.
const AUTOMATION: &[&str] = &[
    "chrome-lighthouse",
    "electron/",
    "headlesschrome",
    "phantomjs",
    "playwright",
    "puppeteer",
    "selenium",
    "slimerjs",
    "webdriver",
];

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
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

/// Why the heuristic thinks nobody was looking at this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Headless {
    /// The string names an automation runtime.
    NamedRuntime,
    /// A browser that sent no `Accept-Language`. Every shipping browser sends
    /// one; a scripted client driving a browser engine usually does not.
    NoAcceptLanguage,
    /// A Chromium string with no `Sec-Fetch-Mode`, which Chrome has sent on
    /// every navigation since 76. Safari and older engines are not held to it.
    NoFetchMetadata,
}

impl Headless {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NamedRuntime => "named_runtime",
            Self::NoAcceptLanguage => "no_accept_language",
            Self::NoFetchMetadata => "no_fetch_metadata",
        }
    }
}

/// A route that settles the question before the header is read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Structural {
    #[default]
    None,
    /// A `.md` fetch: the Markdown surface exists for agents (§10).
    Markdown,
    Mcp,
}

/// What the request carried. Only headers a classification may legitimately
/// read: no address, no cookie, nothing that reaches storage.
#[derive(Debug, Clone, Copy, Default)]
pub struct Signals<'a> {
    pub user_agent: Option<&'a str>,
    pub accept_language: Option<&'a str>,
    pub accept: Option<&'a str>,
    pub sec_fetch_mode: Option<&'a str>,
    pub structural: Structural,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caller {
    pub kind: CallerKind,
    /// The catalogue token, which is the name the dashboard groups by.
    pub agent_name: Option<&'static str>,
    pub purpose: Option<Purpose>,
    pub headless: Option<Headless>,
}

pub fn catalogue() -> &'static [Agent] {
    AGENTS
}

/// Looks a token up in the catalogue.
pub fn agent(token: &str) -> Option<&'static Agent> {
    AGENTS.iter().find(|a| a.token == token)
}

pub fn classify(signals: &Signals<'_>) -> Caller {
    let lower = signals.user_agent.unwrap_or_default().to_ascii_lowercase();

    // Longest match wins, so `claude-user` is not shadowed by a shorter token
    // that happens to be a prefix of it whatever order the list is written in.
    let named = AGENTS
        .iter()
        .filter(|a| lower.contains(a.token))
        .max_by_key(|a| a.token.len());

    if let Some(found) = named {
        return Caller {
            kind: CallerKind::Agent,
            agent_name: Some(found.token),
            purpose: Some(found.purpose),
            headless: None,
        };
    }
    if signals.structural != Structural::None {
        return Caller {
            kind: CallerKind::Agent,
            agent_name: None,
            purpose: Some(Purpose::Assistant),
            headless: None,
        };
    }
    if BOTS.iter().any(|b| lower.contains(b)) {
        return Caller {
            kind: CallerKind::Bot,
            agent_name: None,
            purpose: None,
            headless: None,
        };
    }
    if lower.is_empty() || CLIENTS.iter().any(|c| lower.contains(c)) {
        return Caller {
            kind: CallerKind::Integration,
            agent_name: None,
            purpose: None,
            headless: None,
        };
    }
    Caller {
        kind: CallerKind::Human,
        agent_name: None,
        purpose: None,
        headless: headless(&lower, signals),
    }
}

/// The heuristic layer. Runs only on traffic already classified as a reader:
/// a crawler that says it is a crawler is not evading anything, and flagging
/// it would put the same request in two buckets.
fn headless(lower: &str, signals: &Signals<'_>) -> Option<Headless> {
    if AUTOMATION.iter().any(|a| lower.contains(a)) {
        return Some(Headless::NamedRuntime);
    }
    if signals.accept_language.is_none() {
        return Some(Headless::NoAcceptLanguage);
    }
    let chromium = lower.contains("chrome/") || lower.contains("edg/");
    if chromium && signals.sec_fetch_mode.is_none() {
        return Some(Headless::NoFetchMetadata);
    }
    None
}
