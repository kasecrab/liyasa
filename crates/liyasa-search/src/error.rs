//! Search failures, each a registered code (`crates/liyasa-core/src/diagnostics/codes.toml`).

use liyasa_core::diagnostics::{Diagnostic, code};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SearchError {
    #[error("this build reads search index format v{supported}; the index is v{found}")]
    FormatVersion { found: u32, supported: u32 },
    #[error("`{file}` is truncated or corrupt")]
    Corrupt { file: String },
    #[error("{0}")]
    Query(String),
    #[error("no CJK dictionary is installed for `{0}`")]
    MissingDictionary(String),
}

impl SearchError {
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Self::FormatVersion { .. } => Diagnostic::new(code::E1002, self.to_string()).help(
                "rebuild the site with the version of Liyasa that serves it, or upgrade the reader",
            ),
            Self::Corrupt { .. } => Diagnostic::new(code::E1003, self.to_string())
                .help("delete `search-index/` and rebuild"),
            Self::Query(_) => Diagnostic::new(code::E1004, self.to_string()),
            Self::MissingDictionary(language) => Diagnostic::new(code::E1006, self.to_string())
                .help(format!(
                    "run `liyasa add dictionary {language}`, or drop the `cjk-dict` feature to \
                     index with bigrams"
                )),
        }
    }

    pub fn corrupt(file: impl Into<String>) -> Self {
        Self::Corrupt { file: file.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_carries_its_code() {
        let cases = [
            (
                SearchError::FormatVersion {
                    found: 2,
                    supported: 1,
                },
                "E1002",
            ),
            (SearchError::corrupt("postings-0.bin"), "E1003"),
            (SearchError::Query("unbalanced quote".to_owned()), "E1004"),
            (SearchError::MissingDictionary("ja".to_owned()), "E1006"),
        ];
        for (error, expected) in cases {
            let diagnostic = error.diagnostic();
            assert_eq!(diagnostic.code.as_str(), expected);
            assert!(!diagnostic.message.is_empty());
            assert!(diagnostic.url.ends_with(expected));
        }
    }

    #[test]
    fn the_message_names_the_file_that_failed() {
        assert!(
            SearchError::corrupt("docs-3.bin")
                .to_string()
                .contains("docs-3.bin")
        );
    }
}
