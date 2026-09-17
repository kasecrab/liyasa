//! Previews: their hosts, what they may not do, and when they go away
//! (GIT-30, GIT-32, GIT-33).
//!
//! "Never indexable" is four separate things — a response header, a
//! `robots.txt`, the absence of a sitemap, and a line in `llms.txt` — because
//! four different clients look in four different places, and a preview that
//! leaks into search results outranks the page it was previewing.

use liyasa_core::ids::ProjectId;
use serde::{Deserialize, Serialize};

use super::environment::Protection;
use super::untrusted::{TRUSTED_LABEL, UNTRUSTED_LABEL};

/// GIT-30's default: a preview is deleted fourteen days after its pull request
/// is merged or closed.
pub const LIFETIME_DAYS: i64 = 14;
pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// The header every preview response carries.
pub const ROBOTS_TAG: &str = "noindex, nofollow";
pub const ROBOTS_TAG_HEADER: &str = "x-robots-tag";

/// The `robots.txt` a preview serves, whatever the site's own says.
pub const ROBOTS_TXT: &str = "User-agent: *\nDisallow: /\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub project: ProjectId,
    pub host: String,
    pub branch: String,
    pub commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<u64>,
    #[serde(default)]
    pub untrusted: bool,
    pub protection: Protection,
}

impl Preview {
    pub fn new(
        project: ProjectId,
        host: impl Into<String>,
        branch: impl Into<String>,
        commit: impl Into<String>,
    ) -> Self {
        Self {
            project,
            host: host.into(),
            branch: branch.into(),
            commit: commit.into(),
            pull_request: None,
            untrusted: false,
            protection: Protection::default(),
        }
    }

    pub fn for_pull_request(mut self, number: u64) -> Self {
        self.pull_request = Some(number);
        self
    }

    pub fn untrusted(mut self) -> Self {
        self.untrusted = true;
        self
    }

    pub fn with_protection(mut self, protection: Protection) -> Self {
        self.protection = protection;
        self
    }

    pub fn url(&self) -> String {
        format!("https://{}/", self.host)
    }

    /// What the widget and the pull-request comment call this build.
    pub fn label(&self) -> &'static str {
        match self.untrusted {
            true => UNTRUSTED_LABEL,
            false => TRUSTED_LABEL,
        }
    }

    /// The commit, shortened the way every git interface shortens it.
    pub fn short_commit(&self) -> &str {
        let cut = self.commit.len().min(7);
        self.commit.get(..cut).unwrap_or(&self.commit)
    }

    /// Headers every response from a preview carries (GIT-30).
    pub fn headers(&self) -> Vec<(&'static str, String)> {
        vec![(ROBOTS_TAG_HEADER, ROBOTS_TAG.to_owned())]
    }

    /// A preview emits no sitemap: a sitemap is an invitation to crawl.
    pub fn emits_sitemap(&self) -> bool {
        false
    }

    /// The line `llms.txt` carries so an agent that fetched the preview knows
    /// it is not the canonical text (GIT-30).
    pub fn llms_txt_note(&self, production_url: &str) -> String {
        format!(
            "> This is a preview, not canonical. The published documentation is at {}.",
            production_url.trim_end_matches('/')
        )
    }

    /// When this preview is deleted, given when its pull request closed.
    pub fn retire_at(&self, closed_at_ms: i64) -> i64 {
        closed_at_ms.saturating_add(LIFETIME_DAYS * DAY_MS)
    }

    /// What the preview widget shows (GIT-32).
    pub fn widget(&self) -> Widget {
        Widget {
            label: self.label().to_owned(),
            branch: self.branch.clone(),
            commit: self.short_commit().to_owned(),
            feedback_url: format!("{}_liyasa/feedback", self.url()),
        }
    }

    /// The comment posted on the pull request (GIT-30). The changed pages are
    /// listed because "your preview is ready" without them makes a reviewer
    /// open the whole site to find out what moved.
    pub fn comment(&self, changed_pages: &[String]) -> String {
        let mut body = format!(
            "**Liyasa {}** for `{}` at `{}`\n\n{}\n",
            self.label(),
            self.branch,
            self.short_commit(),
            self.url()
        );
        if self.untrusted {
            body.push_str(
                "\nBuilt without secrets and without live truth sources, because the head is \
                 not trusted. Facts come from the last production snapshot.\n",
            );
        }
        match changed_pages.is_empty() {
            true => body.push_str("\nNo pages changed.\n"),
            false => {
                body.push_str("\nPages changed:\n\n");
                for page in changed_pages {
                    body.push_str(&format!(
                        "- [{page}]({}{})\n",
                        self.url(),
                        page.trim_start_matches('/')
                    ));
                }
            }
        }
        body
    }
}

/// The preview widget's contents (GIT-32).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Widget {
    pub label: String,
    pub branch: String,
    pub commit: String,
    pub feedback_url: String,
}

/// Whether a viewer may see a protected preview (GIT-32).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewer {
    Anonymous,
    /// Signed in to the organization that owns the project.
    Member,
    /// Holds the preview's shared password.
    PasswordHolder,
}

pub fn may_view(protection: Protection, viewer: Viewer) -> bool {
    match protection {
        Protection::Public => true,
        Protection::Organization => viewer == Viewer::Member,
        Protection::Password => {
            matches!(viewer, Viewer::PasswordHolder | Viewer::Member)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectId {
        ProjectId(ulid::Ulid::from_bytes([3; 16]))
    }

    fn preview() -> Preview {
        Preview::new(
            project(),
            "liyasa-pr-42.preview.example.com",
            "patch-1",
            "abc123def456",
        )
        .for_pull_request(42)
    }

    #[test]
    fn every_preview_response_says_noindex_nofollow() {
        let headers = preview().headers();
        assert_eq!(
            headers,
            vec![("x-robots-tag", "noindex, nofollow".to_owned())]
        );
    }

    #[test]
    fn a_preview_disallows_every_crawler_and_emits_no_sitemap() {
        assert_eq!(ROBOTS_TXT, "User-agent: *\nDisallow: /\n");
        assert!(!preview().emits_sitemap());
    }

    #[test]
    fn llms_txt_names_the_production_url_an_agent_should_have_read_instead() {
        let note = preview().llms_txt_note("https://docs.example.com/");
        assert!(note.contains("preview, not canonical"), "{note}");
        assert!(note.contains("https://docs.example.com"), "{note}");
        assert!(!note.contains("com/."), "the trailing slash is trimmed");
    }

    #[test]
    fn a_preview_is_deleted_fourteen_days_after_its_pull_request_closes() {
        let closed = 1_700_000_000_000;
        assert_eq!(
            preview().retire_at(closed),
            closed + 14 * 24 * 60 * 60 * 1000
        );
    }

    #[test]
    fn the_widget_shows_the_branch_the_short_commit_and_a_feedback_action() {
        let widget = preview().widget();
        assert_eq!(widget.branch, "patch-1");
        assert_eq!(widget.commit, "abc123d");
        assert_eq!(widget.label, TRUSTED_LABEL);
        assert!(
            widget.feedback_url.ends_with("/_liyasa/feedback"),
            "{widget:?}"
        );
    }

    #[test]
    fn an_untrusted_preview_is_labelled_as_one_everywhere_it_appears() {
        let preview = preview().untrusted();
        assert_eq!(preview.label(), "untrusted preview");
        assert_eq!(preview.widget().label, "untrusted preview");
        let comment = preview.comment(&[]);
        assert!(comment.contains("untrusted preview"), "{comment}");
        assert!(comment.contains("without secrets"), "{comment}");
    }

    #[test]
    fn the_comment_lists_the_pages_that_changed_with_links_into_the_preview() {
        let comment = preview().comment(&["/guides/install".to_owned(), "/api".to_owned()]);
        assert!(
            comment.contains("liyasa-pr-42.preview.example.com"),
            "{comment}"
        );
        assert!(
            comment.contains(
                "- [/guides/install](https://liyasa-pr-42.preview.example.com/guides/install)"
            ),
            "{comment}"
        );
        assert!(comment.contains("- [/api]"), "{comment}");
    }

    #[test]
    fn a_comment_with_nothing_changed_says_so_rather_than_showing_an_empty_list() {
        let comment = preview().comment(&[]);
        assert!(comment.contains("No pages changed."), "{comment}");
    }

    #[test]
    fn a_short_commit_survives_a_commit_shorter_than_seven_characters() {
        let short = Preview::new(project(), "h", "b", "abc");
        assert_eq!(short.short_commit(), "abc");
        let empty = Preview::new(project(), "h", "b", "");
        assert_eq!(empty.short_commit(), "");
    }

    #[test]
    fn organization_protection_refuses_an_anonymous_viewer() {
        assert!(!may_view(Protection::Organization, Viewer::Anonymous));
        assert!(may_view(Protection::Organization, Viewer::Member));
        assert!(
            !may_view(Protection::Organization, Viewer::PasswordHolder),
            "a password is not a substitute for membership when the policy is organization"
        );
    }

    #[test]
    fn password_protection_admits_the_password_and_the_organization() {
        assert!(may_view(Protection::Password, Viewer::PasswordHolder));
        assert!(may_view(Protection::Password, Viewer::Member));
        assert!(!may_view(Protection::Password, Viewer::Anonymous));
    }

    #[test]
    fn a_public_preview_admits_everyone_and_is_still_not_indexable() {
        assert!(may_view(Protection::Public, Viewer::Anonymous));
        let public = preview().with_protection(Protection::Public);
        assert_eq!(public.headers()[0].1, ROBOTS_TAG);
        assert!(!public.emits_sitemap());
    }

    #[test]
    fn a_preview_round_trips_as_json() {
        let preview = preview().untrusted().with_protection(Protection::Password);
        let text = serde_json::to_string(&preview).expect("a preview serializes");
        let back: Preview = serde_json::from_str(&text).expect("it deserializes");
        assert_eq!(back, preview);
    }
}
