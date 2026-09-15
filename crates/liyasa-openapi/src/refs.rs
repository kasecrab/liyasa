//! Reference resolution across documents (API-02).
//!
//! A `$ref` is `<document>#<json pointer>`. The document part is resolved
//! against the document the reference was written in, so a spec may be split
//! across a directory of files or point at a URL, and the pointer is then read
//! by [`crate::read::Reader`].

/// Resolves a `$ref`'s document part against the document holding it.
///
/// `base` is the key the containing document is filed under: the empty string
/// for the configured spec, an absolute URL, or a project-relative path.
pub fn join(base: &str, document: &str) -> String {
    if is_absolute(document) {
        return document.to_owned();
    }
    if is_absolute(base) {
        return match url::Url::parse(base).and_then(|base| base.join(document)) {
            Ok(joined) => joined.to_string(),
            Err(_) => document.to_owned(),
        };
    }
    let parent = base.rsplit_once('/').map_or("", |(head, _)| head);
    liyasa_core::VfsPath::new(format!("{parent}/{document}")).to_string()
}

fn is_absolute(text: &str) -> bool {
    text.starts_with("http://") || text.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sibling_file_resolves_against_the_documents_directory() {
        assert_eq!(
            join("openapi/api.yaml", "common.yaml"),
            "openapi/common.yaml"
        );
        assert_eq!(
            join("openapi/api.yaml", "./schemas/user.yaml"),
            "openapi/schemas/user.yaml"
        );
        assert_eq!(
            join("openapi/parts/a.yaml", "../common.yaml"),
            "openapi/common.yaml"
        );
    }

    #[test]
    fn a_reference_from_the_configured_document_is_project_relative() {
        assert_eq!(join("", "openapi/common.yaml"), "openapi/common.yaml");
    }

    #[test]
    fn an_absolute_url_wins_over_the_base() {
        assert_eq!(
            join("openapi/api.yaml", "https://example.com/spec.yaml"),
            "https://example.com/spec.yaml"
        );
    }

    #[test]
    fn a_relative_reference_from_a_url_stays_on_that_host() {
        assert_eq!(
            join("https://example.com/specs/api.yaml", "shared/user.yaml"),
            "https://example.com/specs/shared/user.yaml"
        );
    }
}
