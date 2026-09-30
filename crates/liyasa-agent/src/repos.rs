//! Context repositories and their allow lists (AGT-12).
//!
//! "File and directory allow lists per repo" is the requirement, and the whole
//! value of it is in two decisions that a looser reading gets wrong.
//!
//! An **empty allow list allows nothing**. A repository configured with no `allow`
//! is a repository the agent may not read, not one it may read entirely. The
//! agent can write what it read into a public page, so the failure mode of the
//! other reading is a private source tree on the documentation site.
//!
//! A prefix matches at a **path boundary**. `allow: ["src"]` permits `src/lib.rs`
//! and does not permit `srcs/secret.rs`, because a plain `starts_with` would.
//!
//! `deny` wins over `allow`, and a path that does not normalise is refused rather
//! than normalised — the same rule [`crate::scope`] applies to the docs tree.

use crate::config::ContextRepo;

/// Why a repository read was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RepoDenial {
    #[error("`{repo}` is not a configured context repository")]
    Unknown { repo: String },
    #[error("`{path}` is not in `{repo}`'s allow list")]
    NotAllowed { repo: String, path: String },
    #[error("`{path}` is in `{repo}`'s deny list")]
    Denied { repo: String, path: String },
    #[error("`{path}` is not a path inside `{repo}`")]
    NotAPath { repo: String, path: String },
}

/// Whether a prefix covers a path at a path boundary.
fn covers(prefix: &str, path: &str) -> bool {
    let prefix = prefix.trim_matches('/');
    if prefix.is_empty() {
        // An explicit `""` or `"/"` entry means the whole repository. Allowed,
        // because an operator may mean it; it is not what an ABSENT list means.
        return true;
    }
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

/// Decides one read of one context repository.
pub fn permits(repos: &[ContextRepo], repo: &str, path: &str) -> Result<(), RepoDenial> {
    let Some(configured) = repos.iter().find(|r| r.name == repo) else {
        return Err(RepoDenial::Unknown {
            repo: repo.to_owned(),
        });
    };
    let Some(path) = crate::scope::normalise(path) else {
        return Err(RepoDenial::NotAPath {
            repo: repo.to_owned(),
            path: path.to_owned(),
        });
    };
    if configured.deny.iter().any(|prefix| covers(prefix, &path)) {
        return Err(RepoDenial::Denied {
            repo: repo.to_owned(),
            path,
        });
    }
    if configured.allow.iter().any(|prefix| covers(prefix, &path)) {
        return Ok(());
    }
    Err(RepoDenial::NotAllowed {
        repo: repo.to_owned(),
        path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repos() -> Vec<ContextRepo> {
        vec![
            ContextRepo {
                name: "acme/api".to_owned(),
                allow: vec!["src".to_owned(), "openapi.yaml".to_owned()],
                deny: vec!["src/secrets".to_owned()],
            },
            ContextRepo {
                name: "acme/closed".to_owned(),
                allow: Vec::new(),
                deny: Vec::new(),
            },
        ]
    }

    #[test]
    fn an_allowed_directory_and_an_allowed_file_are_read() {
        assert_eq!(permits(&repos(), "acme/api", "src/lib.rs"), Ok(()));
        assert_eq!(permits(&repos(), "acme/api", "openapi.yaml"), Ok(()));
        assert_eq!(permits(&repos(), "acme/api", "src"), Ok(()));
    }

    #[test]
    fn a_path_outside_the_allow_list_is_refused() {
        assert_eq!(
            permits(&repos(), "acme/api", "Makefile"),
            Err(RepoDenial::NotAllowed {
                repo: "acme/api".to_owned(),
                path: "Makefile".to_owned()
            })
        );
    }

    #[test]
    fn a_prefix_matches_at_a_path_boundary_and_not_as_a_string() {
        // `src` must not admit `srcs/`.
        assert!(permits(&repos(), "acme/api", "srcs/secret.rs").is_err());
        assert!(permits(&repos(), "acme/api", "src-private/x.rs").is_err());
    }

    #[test]
    fn the_deny_list_wins_over_the_allow_list() {
        assert_eq!(
            permits(&repos(), "acme/api", "src/secrets/keys.rs"),
            Err(RepoDenial::Denied {
                repo: "acme/api".to_owned(),
                path: "src/secrets/keys.rs".to_owned()
            })
        );
    }

    #[test]
    fn a_repo_with_no_allow_list_allows_nothing() {
        // The failure mode of the other reading is a private source tree on a
        // public documentation site.
        for path in ["README.md", "src/lib.rs", "."] {
            assert!(
                permits(&repos(), "acme/closed", path).is_err(),
                "`{path}` was readable from a repo with no allow list"
            );
        }
    }

    #[test]
    fn an_unconfigured_repo_is_refused() {
        assert_eq!(
            permits(&repos(), "someone/else", "src/lib.rs"),
            Err(RepoDenial::Unknown {
                repo: "someone/else".to_owned()
            })
        );
    }

    #[test]
    fn a_traversal_out_of_the_repo_is_refused() {
        for path in ["../../etc/passwd", "src/../../secrets", "/etc/passwd"] {
            assert!(
                matches!(
                    permits(&repos(), "acme/api", path),
                    Err(RepoDenial::NotAPath { .. })
                ),
                "`{path}` was not refused as a path"
            );
        }
    }

    #[test]
    fn a_normalised_path_still_matches_its_prefix() {
        assert_eq!(permits(&repos(), "acme/api", "./src/lib.rs"), Ok(()));
        assert_eq!(permits(&repos(), "acme/api", "src//lib.rs"), Ok(()));
    }

    #[test]
    fn an_explicit_whole_repo_entry_is_honoured() {
        // Different from an absent list: an operator who writes `"allow": ["/"]`
        // has said so.
        let repos = vec![ContextRepo {
            name: "acme/open".to_owned(),
            allow: vec!["/".to_owned()],
            deny: vec!["vendor".to_owned()],
        }];
        assert_eq!(permits(&repos, "acme/open", "anything/here.rs"), Ok(()));
        assert!(permits(&repos, "acme/open", "vendor/x.rs").is_err());
    }
}
