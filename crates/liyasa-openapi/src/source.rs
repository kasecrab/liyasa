//! Fetching a spec and everything its references reach (API-01, API-02).
//!
//! A `$ref` may leave the document it is written in, so loading is a traversal
//! rather than one read: every document is fetched, scanned for further
//! references, and the ones it names are fetched in turn, until the set
//! closes. Local documents go through [`Vfs`], remote ones through
//! [`HttpClient`] under the project's policy, and `--local-schema` is the
//! absence of an `HttpClient` rather than a second code path.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::net::{HttpClient, HttpPolicy, HttpRequest, Method, NetError, Url};
use liyasa_core::vfs::{Vfs, VfsError, VfsPath};

use crate::read::Documents;
use crate::tree::{Value, as_str};
use crate::{SpecError, refs, tree};

/// How many documents one spec may pull in. A spec that reaches past this is
/// reported rather than followed: the traversal is over inputs Liyasa does not
/// own and the cost is a build that never finishes.
const MAX_DOCUMENTS: usize = 512;

/// Where a spec or an overlay is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    File(VfsPath),
    Remote(Box<Url>),
}

impl Location {
    /// Reads a configured `source`, which is a URL when it parses as an
    /// absolute `http` or `https` one and a project path otherwise.
    pub fn parse(source: &str) -> Self {
        match Url::parse(source) {
            Ok(url) if matches!(url.scheme(), "http" | "https") => Self::Remote(Box::new(url)),
            _ => Self::File(VfsPath::new(source)),
        }
    }

    /// The key this document is filed under in a [`Documents`] set, which is
    /// also what a relative `$ref` inside it resolves against.
    pub fn key(&self) -> String {
        match self {
            Self::File(path) => path.to_string(),
            Self::Remote(url) => url.to_string(),
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote(_))
    }
}

impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.key())
    }
}

/// Reads spec documents from wherever they live.
pub struct Fetcher<'a> {
    vfs: &'a dyn Vfs,
    /// `None` is `--local-schema`: a remote reference is refused rather than
    /// fetched.
    http: Option<&'a dyn HttpClient>,
    policy: &'a HttpPolicy,
}

impl<'a> Fetcher<'a> {
    pub fn new(vfs: &'a dyn Vfs, http: Option<&'a dyn HttpClient>, policy: &'a HttpPolicy) -> Self {
        Self { vfs, http, policy }
    }

    /// Reads one document's bytes.
    pub async fn bytes(&self, at: &Location) -> Result<Vec<u8>, SpecError> {
        match at {
            Location::File(path) => self.vfs.read(path).map(|b| b.to_vec()).map_err(|e| {
                Box::new(match e {
                    VfsError::NotFound(path) => {
                        Diagnostic::new(code::E0503, format!("no spec at `{path}`"))
                            .help("`openapi[].source` is a path inside the project, or a URL")
                    }
                    VfsError::Denied(path) => Diagnostic::new(
                        code::E0503,
                        format!("`{path}` is outside the project or cannot be read"),
                    ),
                    VfsError::Io(message) => {
                        Diagnostic::new(code::E0503, format!("cannot read the spec: {message}"))
                    }
                })
            }),
            Location::Remote(url) => {
                let Some(http) = self.http else {
                    return Err(Box::new(
                        Diagnostic::new(
                            code::E0806,
                            format!("`{url}` was not fetched because this build is local-only"),
                        )
                        .help("drop `--local-schema`, or vendor the document into the project"),
                    ));
                };
                let request = HttpRequest {
                    method: Method::GET,
                    url: url.as_ref().clone(),
                    headers: Vec::new(),
                    body: None,
                };
                match http.fetch(request, self.policy).await {
                    Ok(response) if (200..300).contains(&response.status) => {
                        Ok(response.body.to_vec())
                    }
                    Ok(response) => Err(Box::new(Diagnostic::new(
                        code::E0503,
                        format!("`{url}` answered {}", response.status),
                    ))),
                    Err(error) => Err(Box::new(describe(url, &error))),
                }
            }
        }
    }

    /// Fetches `root` and every document its references reach, filing each
    /// under the key a `$ref` inside it resolves against.
    ///
    /// A document that cannot be read is reported and left out; the reader
    /// then reports the references into it, so one unreachable file does not
    /// cost the rest of the spec.
    pub async fn documents(&self, root: &Location, root_tree: Value) -> (Documents, Diagnostics) {
        let mut diagnostics = Diagnostics::new();
        let mut documents = Documents::new(root.key(), root_tree.clone());

        let mut queue: Vec<(String, Value)> = vec![(root.key(), root_tree)];
        let mut seen: Vec<String> = vec![root.key()];
        while let Some((base, tree)) = queue.pop() {
            for reference in external_refs(&tree) {
                let key = refs::join(&base, &reference);
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key.clone());
                if seen.len() > MAX_DOCUMENTS {
                    diagnostics.push(Diagnostic::new(
                        code::E0502,
                        format!("this spec reaches more than {MAX_DOCUMENTS} documents"),
                    ));
                    return (documents, diagnostics);
                }
                let at = Location::parse(&key);
                match self.bytes(&at).await {
                    Ok(bytes) => match tree::parse(&bytes, &key) {
                        Ok(parsed) => {
                            documents.insert(key, parsed.clone());
                            queue.push((at.key(), parsed));
                        }
                        Err(error) => diagnostics.push(*error),
                    },
                    Err(error) => diagnostics.push(*error),
                }
            }
        }
        (documents, diagnostics)
    }
}

/// Every `$ref` in a document that names a document other than this one, in
/// the order they appear.
pub fn external_refs(tree: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk(tree, &mut out);
    out
}

fn walk(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Mapping(map) => {
            if let Some(reference) = map.get("$ref").and_then(as_str) {
                let document = reference.split('#').next().unwrap_or_default();
                if !document.is_empty() && !out.iter().any(|seen| seen == document) {
                    out.push(document.to_owned());
                }
            }
            for (_, item) in map.iter() {
                walk(item, out);
            }
        }
        Value::Sequence(items) => {
            for item in items {
                walk(item, out);
            }
        }
        _ => {}
    }
}

/// Turns a network failure into the diagnostic the operator can act on: a
/// policy denial names the rule, everything else names the layer that failed.
fn describe(url: &Url, error: &NetError) -> Diagnostic {
    match error {
        NetError::PolicyDenied { reason } => {
            Diagnostic::new(code::E0806, format!("`{url}` was not fetched: {reason}"))
                .help("add the host to the project's allow list, or vendor the document")
        }
        NetError::Timeout => Diagnostic::new(code::E0503, format!("`{url}` timed out")),
        NetError::TooLarge => Diagnostic::new(
            code::E0503,
            format!("`{url}` is larger than this build allows"),
        ),
        NetError::Dns(detail) => {
            Diagnostic::new(code::E0503, format!("`{url}` did not resolve: {detail}"))
        }
        NetError::Tls(detail) => Diagnostic::new(
            code::E0503,
            format!("`{url}` failed its TLS handshake: {detail}"),
        ),
        NetError::Io(detail) => {
            Diagnostic::new(code::E0503, format!("`{url}` could not be read: {detail}"))
        }
        NetError::Status(status) => {
            Diagnostic::new(code::E0503, format!("`{url}` answered {status}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_source_is_remote_and_a_path_is_not() {
        assert!(Location::parse("https://example.com/api.yaml").is_remote());
        assert!(!Location::parse("openapi/api.yaml").is_remote());
        assert_eq!(
            Location::parse("./openapi/api.yaml").key(),
            "openapi/api.yaml",
            "a project path is normalized"
        );
    }

    #[test]
    fn a_file_url_is_a_path_rather_than_something_to_fetch() {
        assert!(!Location::parse("file:///etc/passwd").is_remote());
    }

    #[test]
    fn only_references_that_leave_the_document_are_collected() {
        let tree = tree::parse(
            br##"
paths:
  /a:
    get:
      parameters:
        - $ref: "#/components/parameters/Id"
      responses:
        "200":
          content:
            application/json:
              schema: { $ref: "common.yaml#/User" }
components:
  schemas:
    B: { $ref: "https://example.com/spec.yaml#/B" }
    C: { $ref: "common.yaml#/Other" }
"##,
            "test",
        )
        .expect("the fixture parses");
        assert_eq!(
            external_refs(&tree),
            vec!["common.yaml", "https://example.com/spec.yaml"],
            "a local pointer names no document, and a document is listed once"
        );
    }
}
