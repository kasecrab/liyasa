//! What the assistant may index (AST-02).
//!
//! The page-level half of the rule is already decided by the build, in
//! [`liyasa_build::tree::Indexing`], and is called rather than restated: a
//! second copy of "hidden unless `ai: true`" would drift from the first and
//! nothing would notice. What this module adds is the part the build does not
//! know — the deployment environment, and `ai.respectNoindex`, which the build
//! applies to the sitemap and not to the assistant.

use liyasa_build::tree::Indexing;
use liyasa_core::frontmatter::FrontmatterFields;

use crate::config::AiConfig;

/// Where the deployment being indexed is serving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Production,
    /// A preview or branch deployment. Not indexed unless the operator turned
    /// it on for THAT preview.
    Preview {
        assistant: bool,
    },
}

/// What the walk found about one page, beyond its front matter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageFacts<'a> {
    pub front: &'a FrontmatterFields,
    /// The build left it out (no `--drafts`).
    pub draft: bool,
    /// `.liyasaignore` matched: out of the build, and so out of everything.
    pub ignored: bool,
    /// `.liyasa-aiignore` matched: out of AI indexing alone.
    pub ai_ignored: bool,
}

/// Why a page is not in the assistant's index. Carried rather than collapsed to
/// a boolean so the dashboard can answer "why is this page not answerable".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Excluded {
    Draft,
    Preview,
    Ignored,
    AiIgnored,
    /// `hidden: true` and no `ai: true` to bring it back.
    Hidden,
    /// `ai: false`.
    OptedOut,
    /// `noindex: true` with `ai.respectNoindex` on.
    Noindex,
}

impl Excluded {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Preview => "preview",
            Self::Ignored => "liyasaignore",
            Self::AiIgnored => "liyasa-aiignore",
            Self::Hidden => "hidden",
            Self::OptedOut => "ai-false",
            Self::Noindex => "noindex",
        }
    }
}

/// `None` when the page is indexable.
pub fn exclusion(page: &PageFacts<'_>, env: Environment, config: &AiConfig) -> Option<Excluded> {
    if page.ignored {
        return Some(Excluded::Ignored);
    }
    if page.draft {
        return Some(Excluded::Draft);
    }
    if let Environment::Preview { assistant: false } = env {
        return Some(Excluded::Preview);
    }
    if page.ai_ignored {
        return Some(Excluded::AiIgnored);
    }
    if !Indexing::of(page.front, false).ai {
        // The build has already weighed `hidden` against `ai`; only the reason
        // is recovered here.
        return Some(match page.front.ai {
            Some(_) => Excluded::OptedOut,
            None => Excluded::Hidden,
        });
    }
    if config.respect_noindex.0 && page.front.noindex.unwrap_or(false) {
        return Some(Excluded::Noindex);
    }
    None
}

#[cfg(test)]
mod tests {
    use liyasa_core::frontmatter::AiSetting;

    use super::*;

    fn facts(front: &FrontmatterFields) -> PageFacts<'_> {
        PageFacts {
            front,
            draft: false,
            ignored: false,
            ai_ignored: false,
        }
    }

    #[test]
    fn an_ordinary_page_is_indexed() {
        let front = FrontmatterFields::default();
        assert_eq!(
            exclusion(
                &facts(&front),
                Environment::Production,
                &AiConfig::default()
            ),
            None
        );
    }

    #[test]
    fn a_hidden_page_needs_ai_true() {
        let mut front = FrontmatterFields {
            hidden: Some(true),
            ..Default::default()
        };
        assert_eq!(
            exclusion(
                &facts(&front),
                Environment::Production,
                &AiConfig::default()
            ),
            Some(Excluded::Hidden)
        );
        front.ai = Some(AiSetting::Enabled(true));
        assert_eq!(
            exclusion(
                &facts(&front),
                Environment::Production,
                &AiConfig::default()
            ),
            None
        );
    }

    #[test]
    fn ai_false_opts_a_visible_page_out() {
        let front = FrontmatterFields {
            ai: Some(AiSetting::Enabled(false)),
            ..Default::default()
        };
        assert_eq!(
            exclusion(
                &facts(&front),
                Environment::Production,
                &AiConfig::default()
            ),
            Some(Excluded::OptedOut)
        );
    }

    #[test]
    fn a_preview_is_out_unless_that_preview_turned_it_on() {
        let front = FrontmatterFields::default();
        assert_eq!(
            exclusion(
                &facts(&front),
                Environment::Preview { assistant: false },
                &AiConfig::default()
            ),
            Some(Excluded::Preview)
        );
        assert_eq!(
            exclusion(
                &facts(&front),
                Environment::Preview { assistant: true },
                &AiConfig::default()
            ),
            None
        );
    }

    #[test]
    fn noindex_is_respected_only_when_the_operator_asked() {
        let front = FrontmatterFields {
            noindex: Some(true),
            ..Default::default()
        };
        let mut config = AiConfig::default();
        assert!(config.respect_noindex.0, "the PRD default is on");
        assert_eq!(
            exclusion(&facts(&front), Environment::Production, &config),
            Some(Excluded::Noindex)
        );
        config.respect_noindex.0 = false;
        assert_eq!(
            exclusion(&facts(&front), Environment::Production, &config),
            None
        );
    }

    #[test]
    fn the_two_ignore_files_are_distinguished() {
        let front = FrontmatterFields::default();
        let mut page = facts(&front);
        page.ai_ignored = true;
        assert_eq!(
            exclusion(&page, Environment::Production, &AiConfig::default()),
            Some(Excluded::AiIgnored)
        );
        page.ignored = true;
        assert_eq!(
            exclusion(&page, Environment::Production, &AiConfig::default()),
            Some(Excluded::Ignored)
        );
    }

    #[test]
    fn a_draft_is_out_of_a_production_index() {
        let front = FrontmatterFields::default();
        let mut page = facts(&front);
        page.draft = true;
        assert_eq!(
            exclusion(&page, Environment::Production, &AiConfig::default()),
            Some(Excluded::Draft)
        );
    }
}
