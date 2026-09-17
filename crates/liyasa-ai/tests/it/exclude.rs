//! The assistant's exclusion rule against the build's, so the two cannot
//! drift.

use liyasa_ai::config::AiConfig;
use liyasa_ai::exclude::{Environment, PageFacts, exclusion};
use liyasa_build::tree::Indexing;
use liyasa_core::frontmatter::{AiSetting, FrontmatterFields};

/// Every combination of the three front-matter fields the build weighs.
fn fronts() -> Vec<FrontmatterFields> {
    let mut out = Vec::new();
    for hidden in [None, Some(false), Some(true)] {
        for ai in [
            None,
            Some(AiSetting::Enabled(true)),
            Some(AiSetting::Enabled(false)),
            Some(AiSetting::Options {
                instructions: Some("answer briefly".to_owned()),
            }),
        ] {
            out.push(FrontmatterFields {
                hidden,
                ai: ai.clone(),
                ..Default::default()
            });
        }
    }
    out
}

#[test]
fn the_page_level_rule_is_the_builds_rule() {
    let mut config = AiConfig::default();
    // The one place the two deliberately differ: the build applies `noindex`
    // to the sitemap, AST-02 applies it to the assistant. Turn it off so the
    // comparison is of the shared half.
    config.respect_noindex.0 = false;

    for front in fronts() {
        for ai_ignored in [false, true] {
            let page = PageFacts {
                front: &front,
                draft: false,
                ignored: false,
                ai_ignored,
            };
            let mine = exclusion(&page, Environment::Production, &config).is_none();
            let theirs = Indexing::of(&front, ai_ignored).ai;
            assert_eq!(
                mine, theirs,
                "disagreement on hidden={:?} ai={:?} ai_ignored={ai_ignored}",
                front.hidden, front.ai
            );
        }
    }
}

#[test]
fn noindex_is_where_the_two_part_company() {
    let front = FrontmatterFields {
        noindex: Some(true),
        ..Default::default()
    };
    let page = PageFacts {
        front: &front,
        draft: false,
        ignored: false,
        ai_ignored: false,
    };
    // The build still indexes it for search; AST-02 keeps it out of the
    // assistant while `ai.respectNoindex` is on.
    assert!(Indexing::of(&front, false).ai);
    assert!(exclusion(&page, Environment::Production, &AiConfig::default()).is_some());
}
