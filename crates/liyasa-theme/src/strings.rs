//! Every word the theme puts on screen (THM-40).
//!
//! No string is written into a template. An operator replaces any of them in
//! `theme/strings.json`, or per locale in `theme/strings.<locale>.json`, so
//! re-branding never means ejecting a partial and a locale never means a fork.
//!
//! A file rather than a config key: `schemas/liyasa.schema.json` has no
//! `theme.strings`, the schema is the single source of truth for config keys
//! (CFG-94), and the theme directory already holds `tokens.css`, `partials/`,
//! and `layouts/`.
// TODO(rfc-0503): revisit if the schema gains a key for interface strings.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

macro_rules! strings {
    ($($field:ident $key:literal = $default:literal;)*) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(default, rename_all = "camelCase")]
        pub struct Strings {
            $(pub $field: String,)*
        }

        impl Default for Strings {
            fn default() -> Self {
                Self { $($field: $default.to_owned(),)* }
            }
        }

        impl Strings {
            /// The documented key of every string, for `theme.strings` and the
            /// localization files.
            pub const KEYS: &'static [&'static str] = &[$($key,)*];

            pub fn get(&self, key: &str) -> Option<&str> {
                match key {
                    $($key => Some(self.$field.as_str()),)*
                    _ => None,
                }
            }

            pub fn set(&mut self, key: &str, value: impl Into<String>) -> bool {
                match key {
                    $($key => { self.$field = value.into(); true })*
                    _ => false,
                }
            }
        }
    };
}

strings! {
    skip_to_content "skipToContent" = "Skip to content";
    search "search" = "Search";
    search_placeholder "searchPlaceholder" = "Search the documentation";
    search_empty "searchEmpty" = "No results";
    search_hint "searchHint" = "Press Enter to open, Escape to close";
    ask_ai "askAi" = "Ask AI";
    assistant "assistant" = "Assistant";
    menu "menu" = "Menu";
    close "close" = "Close";
    on_this_page "onThisPage" = "On this page";
    back_to_top "backToTop" = "Back to top";
    previous "previous" = "Previous";
    next "next" = "Next";
    last_updated "lastUpdated" = "Last updated";
    edit_page "editPage" = "Edit this page";
    suggest_edit "suggestEdit" = "Suggest an edit";
    feedback_question "feedbackQuestion" = "Was this page helpful?";
    feedback_yes "feedbackYes" = "Yes";
    feedback_no "feedbackNo" = "No";
    feedback_thanks "feedbackThanks" = "Thank you for the feedback";
    copy "copy" = "Copy";
    copied "copied" = "Copied";
    copy_page "copyPage" = "Copy page as Markdown";
    view_markdown "viewMarkdown" = "View as Markdown";
    copy_mcp_url "copyMcpUrl" = "Copy MCP server URL";
    download_pdf "downloadPdf" = "Download as PDF";
    page_actions "pageActions" = "Page actions";
    open_in "openIn" = "Open in";
    toggle_theme "toggleTheme" = "Toggle dark mode";
    dismiss_banner "dismissBanner" = "Dismiss";
    version "version" = "Version";
    language "language" = "Language";
    not_found_title "notFoundTitle" = "Page not found";
    not_found_description "notFoundDescription" = "The page you are looking for does not exist.";
    not_found_home "notFoundHome" = "Back to the documentation";
    built_with "builtWith" = "Built with Liyasa";
}

impl Strings {
    /// Where an operator's replacements live, relative to the project root.
    pub const FILE: &'static str = "theme/strings.json";

    /// The per-locale file for a locale, which wins over [`Strings::FILE`].
    pub fn file_for(locale: &str) -> String {
        format!("theme/strings.{locale}.json")
    }

    /// Applies one of those files. An unknown key is returned to the caller
    /// rather than ignored, so a typo in a brand override is reportable.
    pub fn with_overrides(mut self, overrides: &BTreeMap<String, String>) -> (Self, Vec<String>) {
        let mut unknown = Vec::new();
        for (key, value) in overrides {
            if !self.set(key, value.clone()) {
                unknown.push(key.clone());
            }
        }
        (self, unknown)
    }
}

/// Why a strings file was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StringsError {
    #[error("`{file}` is not valid JSON: {message}")]
    Json { file: String, message: String },
}

impl StringsError {
    pub fn diagnostic(&self) -> liyasa_core::Diagnostic {
        liyasa_core::Diagnostic::new(liyasa_core::diagnostics::code::E0101, self.to_string())
    }
}

impl Strings {
    /// Parses a strings file over these defaults, returning the keys it did not
    /// recognize alongside the result.
    pub fn parse(file: &str, json: &str) -> Result<(Self, Vec<String>), StringsError> {
        let overrides: BTreeMap<String, String> =
            serde_json::from_str(json).map_err(|error| StringsError::Json {
                file: file.to_owned(),
                message: error.to_string(),
            })?;
        Ok(Self::default().with_overrides(&overrides))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_resolves_and_round_trips() {
        let strings = Strings::default();
        for key in Strings::KEYS {
            let value = strings
                .get(key)
                .unwrap_or_else(|| panic!("`{key}` has no default"));
            assert!(!value.is_empty(), "`{key}` is empty");
        }
        assert_eq!(Strings::KEYS.len(), 36);
    }

    #[test]
    fn an_operator_replaces_any_word_including_the_built_with_line() {
        let overrides = BTreeMap::from([
            ("builtWith".to_owned(), "Docs by Acme".to_owned()),
            ("askAi".to_owned(), "Ask Acme".to_owned()),
            ("notAKey".to_owned(), "x".to_owned()),
        ]);
        let (strings, unknown) = Strings::default().with_overrides(&overrides);
        assert_eq!(strings.built_with, "Docs by Acme");
        assert_eq!(strings.ask_ai, "Ask Acme");
        assert_eq!(unknown, vec!["notAKey".to_owned()]);
    }

    #[test]
    fn a_strings_file_is_parsed_and_its_typos_reported() {
        let (strings, unknown) = Strings::parse(
            Strings::FILE,
            r#"{"builtWith": "Docs by Acme", "bultWith": "typo"}"#,
        )
        .expect("the file parses");
        assert_eq!(strings.built_with, "Docs by Acme");
        assert_eq!(unknown, vec!["bultWith".to_owned()]);
        assert_eq!(Strings::file_for("de"), "theme/strings.de.json");

        let error = Strings::parse(Strings::FILE, "{").expect_err("invalid JSON is reported");
        assert_eq!(error.diagnostic().code.as_str(), "E0101");
    }

    #[test]
    fn the_defaults_serialize_under_their_documented_keys() {
        let json = serde_json::to_value(Strings::default()).expect("strings serialize");
        let object = json.as_object().expect("an object");
        assert_eq!(object.len(), Strings::KEYS.len());
        for key in Strings::KEYS {
            assert!(
                object.contains_key(*key),
                "`{key}` is not a serialized field"
            );
        }
    }
}
