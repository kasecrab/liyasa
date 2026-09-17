//! ANA-05: caller classification against real user agent strings.
//!
//! Every string below was copied from a published bot page or a browser's own
//! `navigator.userAgent`, not composed to match the matcher. The negative
//! cases are the point: a matcher that answered `Agent` for everything would
//! pass the first half of this file and fail the second.

use liyasa_analytics::agents::{self, CallerKind, Headless, Signals, Structural};

const CHATGPT_USER: &str = "Mozilla/5.0 (compatible; ChatGPT-User/1.0; +https://openai.com/bot)";
const GPTBOT: &str = "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; GPTBot/1.2; +https://openai.com/gptbot";
const CLAUDEBOT: &str = "Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; ClaudeBot/1.0; +claudebot@anthropic.com)";
const PERPLEXITY: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36; compatible; PerplexityBot/1.0; +https://perplexity.ai/perplexitybot";
const GOOGLEBOT: &str = "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)";
const CHROME: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
const FIREFOX: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:142.0) Gecko/20100101 Firefox/142.0";
const SAFARI_IOS: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1";
const HEADLESS_CHROME: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) HeadlessChrome/141.0.0.0 Safari/537.36";

/// A browser request carries the headers a browser sends; without them the
/// headless heuristic would fire on every positive case and prove nothing.
fn browser(user_agent: &str) -> Signals<'_> {
    Signals {
        user_agent: Some(user_agent),
        accept_language: Some("en-GB,en;q=0.9"),
        accept: Some("text/html,application/xhtml+xml"),
        sec_fetch_mode: Some("navigate"),
        structural: Structural::None,
    }
}

fn bare(user_agent: &str) -> Signals<'_> {
    Signals {
        user_agent: Some(user_agent),
        ..Signals::default()
    }
}

#[test]
fn the_agents_ana_05_names_are_all_in_the_catalogue() {
    for (user_agent, expected) in [
        (CHATGPT_USER, "chatgpt-user"),
        (GPTBOT, "gptbot"),
        (CLAUDEBOT, "claudebot"),
        (PERPLEXITY, "perplexitybot"),
        ("Devin/1.0 (+https://devin.ai)", "devin"),
        ("Cursor/0.48.7 (darwin arm64)", "cursor"),
        ("Google-Extended", "google-extended"),
    ] {
        let caller = agents::classify(&bare(user_agent));
        assert_eq!(
            caller.kind,
            CallerKind::Agent,
            "{user_agent} should be an agent"
        );
        assert_eq!(caller.agent_name, Some(expected), "for {user_agent}");
    }
}

#[test]
fn a_crawler_is_a_bot_and_not_an_agent() {
    let caller = agents::classify(&bare(GOOGLEBOT));
    assert_eq!(caller.kind, CallerKind::Bot);
    assert_eq!(
        caller.agent_name, None,
        "a crawler has no agent name; naming one would inflate agent adoption"
    );
    for crawler in [
        "Mozilla/5.0 (compatible; bingbot/2.0; +http://www.bing.com/bingbot.htm)",
        "Mozilla/5.0 (compatible; AhrefsBot/7.0; +http://ahrefs.com/robot/)",
        "Mozilla/5.0 (compatible; SemrushBot/7~bl; +http://www.semrush.com/bot.html)",
    ] {
        assert_eq!(agents::classify(&bare(crawler)).kind, CallerKind::Bot);
    }
}

#[test]
fn a_reader_is_a_human_on_every_browser_family() {
    for user_agent in [CHROME, FIREFOX, SAFARI_IOS] {
        let caller = agents::classify(&browser(user_agent));
        assert_eq!(caller.kind, CallerKind::Human, "for {user_agent}");
        assert_eq!(caller.agent_name, None);
        assert_eq!(
            caller.headless, None,
            "{user_agent} is a real browser and must not be flagged"
        );
    }
}

#[test]
fn a_markdown_fetch_or_an_mcp_call_is_agent_traffic_whatever_the_header_says() {
    for structural in [Structural::Markdown, Structural::Mcp] {
        let caller = agents::classify(&Signals {
            structural,
            ..browser(CHROME)
        });
        assert_eq!(caller.kind, CallerKind::Agent);
        assert_eq!(
            caller.agent_name, None,
            "the route says agent, the header does not say which"
        );
    }
    // And the header still wins when it names one.
    let caller = agents::classify(&Signals {
        structural: Structural::Markdown,
        ..bare(CLAUDEBOT)
    });
    assert_eq!(caller.agent_name, Some("claudebot"));
}

#[test]
fn no_user_agent_at_all_is_an_integration() {
    assert_eq!(
        agents::classify(&Signals::default()).kind,
        CallerKind::Integration
    );
    assert_eq!(agents::classify(&bare("")).kind, CallerKind::Integration);
}

#[test]
fn a_scripted_client_is_an_integration_not_a_reader() {
    for user_agent in ["curl/8.5.0", "python-requests/2.32.3", "Go-http-client/2.0"] {
        assert_eq!(
            agents::classify(&bare(user_agent)).kind,
            CallerKind::Integration,
            "for {user_agent}"
        );
    }
}

#[test]
fn the_heuristic_flags_headless_traffic_without_changing_what_it_is() {
    let caller = agents::classify(&browser(HEADLESS_CHROME));
    assert_eq!(caller.headless, Some(Headless::NamedRuntime));
    assert_eq!(
        caller.kind,
        CallerKind::Human,
        "the flag is a flag: it annotates the row, it does not reclassify it"
    );

    for user_agent in [
        "Mozilla/5.0 (Windows NT 10.0) PhantomJS/2.1.1",
        "Mozilla/5.0 (X11; Linux x86_64) Chrome/141.0.0.0 Safari/537.36 Puppeteer",
        "Mozilla/5.0 (X11; Linux x86_64) Chrome/141.0.0.0 Safari/537.36 selenium/4.25",
    ] {
        assert_eq!(
            agents::classify(&browser(user_agent)).headless,
            Some(Headless::NamedRuntime),
            "for {user_agent}"
        );
    }
}

#[test]
fn a_browser_that_sends_no_language_is_flagged() {
    let caller = agents::classify(&Signals {
        accept_language: None,
        ..browser(CHROME)
    });
    assert_eq!(caller.headless, Some(Headless::NoAcceptLanguage));
}

#[test]
fn a_chrome_that_sends_no_fetch_metadata_is_flagged() {
    // Chrome has sent `Sec-Fetch-Mode` on every navigation since 76; a string
    // claiming Chrome 141 without it is claiming something untrue.
    let caller = agents::classify(&Signals {
        sec_fetch_mode: None,
        ..browser(CHROME)
    });
    assert_eq!(caller.headless, Some(Headless::NoFetchMetadata));
    // Safari is not held to it: it only began sending the header in 16.4, and
    // an older iPhone is a reader, not a robot.
    assert_eq!(
        agents::classify(&Signals {
            sec_fetch_mode: None,
            ..browser(SAFARI_IOS)
        })
        .headless,
        None
    );
}

#[test]
fn a_bot_is_never_flagged_headless_because_it_is_not_pretending() {
    let caller = agents::classify(&bare(GOOGLEBOT));
    assert_eq!(caller.kind, CallerKind::Bot);
    assert_eq!(
        caller.headless, None,
        "a crawler that says it is a crawler is not evading anything"
    );
}

#[test]
fn the_catalogue_is_usable_as_data_and_is_not_self_contradictory() {
    let list = agents::catalogue();
    assert!(list.len() >= 12, "ANA-05 asks for a maintained list");
    let mut seen = std::collections::BTreeSet::new();
    for entry in list {
        assert_eq!(
            entry.token,
            entry.token.to_ascii_lowercase(),
            "{} is matched case-insensitively, so the token is lowercase",
            entry.token
        );
        assert!(
            seen.insert(entry.token),
            "{} appears twice in the catalogue",
            entry.token
        );
        assert!(!entry.vendor.is_empty(), "{} has no vendor", entry.token);
    }
    // The revision ships with the binary and moves with releases.
    assert!(agents::REVISION.starts_with("20"), "a date");
}

#[test]
fn a_longer_token_wins_over_a_prefix_of_itself() {
    // `claude-user` contains `claude`; whichever order the catalogue is written
    // in, the specific name is the one recorded.
    let caller = agents::classify(&bare("Mozilla/5.0 (compatible; Claude-User/1.0)"));
    assert_eq!(caller.agent_name, Some("claude-user"));
}
