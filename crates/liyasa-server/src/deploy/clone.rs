//! The `Cloner` of GIT-11, run by the build worker.
//!
//! `liyasa_git::clone` decides *what* to fetch — depth, blob filter, sparse
//! paths, the byte cap, the refresh interval — and refuses what GIT-11 says to
//! refuse. Nothing executed it: the trait had no implementation anywhere, so
//! `contextRepos` was a config key (CFG-99) whose plan was never carried out.
//!
//! It lives here rather than in `liyasa-git` because the trait's own doc says
//! so: "Clones live on the build-worker volume and never in the serving
//! process (GIT-11), which is why this is a trait the worker holds rather than
//! something the server can call."
//!
//! It runs the system `git` through `spawn_blocking` rather than linking `gix`
//! (RFC 1601, RFC 1611) — and through `spawn_blocking` rather than
//! `tokio::process` because that would need the `process` feature appended to
//! an existing `tokio` row in a shared manifest. A clone is a long blocking
//! operation, so the blocking pool is where it belongs regardless.

use std::path::Path;
use std::process::Command;

use liyasa_core::net::BoxFut;
use liyasa_git::clone::{CloneError, CloneRefusal, CloneSpec, Cloner};

/// Clones and refreshes context repositories with the system `git`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemCloner;

impl Cloner for SystemCloner {
    fn fetch<'a>(
        &'a self,
        spec: &'a CloneSpec,
        into: &'a Path,
    ) -> BoxFut<'a, Result<(), CloneError>> {
        let spec = spec.clone();
        let into = into.to_path_buf();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || fetch_blocking(&spec, &into))
                .await
                .map_err(|error| {
                    CloneError::Failed(format!("the clone task did not finish: {error}"))
                })?
        })
    }
}

/// `contextRepos[].repo` is "a clone URL or `owner/name`" (CFG-99).
///
/// A bare `owner/name` names no host, and there is no configuration that says
/// which one to assume — RFC 1609 records that the git config block does not
/// exist, so there is no `github.host` to read. GitHub is the default because
/// GIT-01 makes it the only provider with an App, and an operator on another
/// host can write the full URL today.
// TODO(rfc-1609): read the host from configuration once the git block exists.
fn url_of(repo: &str) -> Result<String, CloneError> {
    if repo.contains("://") || repo.starts_with("git@") || Path::new(repo).is_absolute() {
        return Ok(repo.to_owned());
    }
    let mut parts = repo.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(name), None) if !owner.is_empty() && !name.is_empty() => {
            Ok(format!("https://github.com/{owner}/{name}.git"))
        }
        _ => Err(CloneError::Failed(format!(
            "`{repo}` is neither a clone URL nor `owner/name`"
        ))),
    }
}

fn fetch_blocking(spec: &CloneSpec, into: &Path) -> Result<(), CloneError> {
    let url = url_of(&spec.repo)?;
    if into.join(".git").is_dir() {
        refresh(spec, into)?;
    } else {
        initial(spec, into, &url)?;
    }
    // "The cap is enforced again while fetching", which `CloneSpec::plan` says
    // in as many words because a host that reports no size cannot be refused
    // in advance. Checked after rather than during: git writes the pack itself
    // and there is no byte stream here to stop. An over-cap clone is removed,
    // not left on the volume as a half-answer.
    let size = disk_bytes(into);
    if size > spec.max_bytes {
        let _ = std::fs::remove_dir_all(into);
        return Err(CloneError::Refused(CloneRefusal::TooLarge {
            repo: spec.repo.clone(),
            size,
            cap: spec.max_bytes,
        }));
    }
    Ok(())
}

fn initial(spec: &CloneSpec, into: &Path, url: &str) -> Result<(), CloneError> {
    if let Some(parent) = into.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            CloneError::Failed(format!("preparing {}: {error}", parent.display()))
        })?;
    }
    // `--no-checkout` so nothing is materialised before the sparse list is
    // set. Cloning with `--sparse` would check out the root files first, which
    // on a repository whose allow list is `src/handlers` means fetching blobs
    // the allow list excludes.
    let mut args: Vec<String> = vec!["clone".to_owned(), "--no-checkout".to_owned()];
    args.extend(spec.arguments());
    args.push(url.to_owned());
    args.push(into.display().to_string());
    run(Path::new("."), &args)?;
    sparse(spec, into)?;
    run(into, &["checkout".to_owned()])
}

fn refresh(spec: &CloneSpec, into: &Path) -> Result<(), CloneError> {
    let mut args: Vec<String> = vec![
        "fetch".to_owned(),
        format!("--depth={}", spec.depth),
        "--no-tags".to_owned(),
        "origin".to_owned(),
    ];
    if let Some(reference) = &spec.r#ref {
        args.push(reference.clone());
    }
    run(into, &args)?;
    // The sparse list may have changed since the clone was taken, and a
    // refresh that kept the old one would serve paths the config no longer
    // allows.
    sparse(spec, into)?;
    // A context repository is a read-only cache, so the local state is not
    // something anyone has edits in: taking the fetched commit wholesale is
    // right, and a merge would be the wrong shape.
    run(
        into,
        &[
            "reset".to_owned(),
            "--hard".to_owned(),
            "FETCH_HEAD".to_owned(),
        ],
    )
}

/// `--no-cone` so `contextRepos[].paths` are taken as written.
///
/// Cone mode only understands directory prefixes, and the schema's example
/// allow list includes `openapi.yaml` — a file. In cone mode that entry would
/// be silently widened to everything or dropped.
fn sparse(spec: &CloneSpec, into: &Path) -> Result<(), CloneError> {
    let mut args: Vec<String> = vec![
        "sparse-checkout".to_owned(),
        "set".to_owned(),
        "--no-cone".to_owned(),
        "--".to_owned(),
    ];
    args.extend(spec.sparse_paths.iter().map(|path| format!("/{path}")));
    run(into, &args)
}

/// Every byte under `root`, including `.git`.
///
/// The cap is about what the clone costs the build-worker volume, and the pack
/// is most of that, so a walk that skipped `.git` would measure the wrong
/// thing.
fn disk_bytes(root: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            match entry.file_type() {
                // Not followed: a symlink's target may be outside the clone,
                // and counting it would charge this repository for bytes it
                // does not own.
                Ok(kind) if kind.is_symlink() => {}
                Ok(kind) if kind.is_dir() => stack.push(entry.path()),
                Ok(_) => total += entry.metadata().map(|meta| meta.len()).unwrap_or(0),
                Err(_) => {}
            }
        }
    }
    total
}

fn run(dir: &Path, args: &[String]) -> Result<(), CloneError> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|error| CloneError::Failed(format!("git could not be run: {error}")))?;
    if output.status.success() {
        return Ok(());
    }
    // The first line of stderr, because git's own message is the useful part
    // and a wall of advice is not. An empty stderr still names the command.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let first = stderr.lines().find(|line| !line.trim().is_empty());
    Err(CloneError::Failed(match first {
        Some(line) => line.trim().to_owned(),
        None => format!("git {} failed with no message", args.join(" ")),
    }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use liyasa_git::clone::ContextRepo;

    use super::*;

    static NTH: AtomicU64 = AtomicU64::new(0);

    /// A scratch directory, removed when it goes out of scope. The counter is
    /// beside the pid because nextest gives each test a process and CI's
    /// `cargo test` gives them all one.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(what: &str) -> Self {
            let nth = NTH.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("liyasa-clone-{what}-{}-{nth}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn git_available() -> bool {
        Command::new("git").arg("--version").output().is_ok()
    }

    /// A source repository with two directories and a root file.
    fn source(at: &Path) {
        let git = |args: &[&str]| {
            let owned: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
            run(at, &owned).expect("the fixture's git succeeds");
        };
        git(&["init", "--quiet"]);
        git(&["config", "user.name", "Fixture"]);
        git(&["config", "user.email", "fixture@example.com"]);
        // Partial clone over file:// needs the source to allow it; without
        // this git refuses the filter rather than ignoring it.
        git(&["config", "uploadpack.allowFilter", "true"]);
        git(&["config", "uploadpack.allowAnySHA1InWant", "true"]);
        for (dir, name) in [("wanted", "kept.md"), ("unwanted", "dropped.md")] {
            std::fs::create_dir_all(at.join(dir)).expect("a directory");
            std::fs::write(at.join(dir).join(name), "body").expect("a write");
        }
        std::fs::write(at.join("openapi.yaml"), "openapi: 3.1.0").expect("a write");
        git(&["add", "."]);
        git(&["commit", "--quiet", "-m", "fixture"]);
    }

    #[tokio::test]
    async fn only_the_allowed_paths_are_materialised() {
        if !git_available() {
            eprintln!("SKIPPED: `git` is not on PATH, so there is nothing to clone with.");
            return;
        }
        let from = Scratch::new("source");
        source(from.path());
        let to = Scratch::new("into");
        let into = to.path().join("repo");

        let repo = ContextRepo::new(format!("file://{}", from.path().display()))
            .with_paths(&["wanted", "openapi.yaml"]);
        let spec = CloneSpec::plan(&repo, None).expect("a plan");
        SystemCloner
            .fetch(&spec, &into)
            .await
            .expect("the clone succeeds");

        assert!(
            into.join("wanted/kept.md").is_file(),
            "an allowed directory is checked out"
        );
        assert!(
            into.join("openapi.yaml").is_file(),
            "an allowed FILE is checked out, which cone mode could not express"
        );
        assert!(
            !into.join("unwanted/dropped.md").exists(),
            "a path outside the allow list is never materialised"
        );
    }

    #[tokio::test]
    async fn a_clone_over_its_cap_is_refused_and_removed() {
        if !git_available() {
            eprintln!("SKIPPED: `git` is not on PATH, so there is nothing to clone with.");
            return;
        }
        let from = Scratch::new("source-cap");
        source(from.path());
        let to = Scratch::new("into-cap");
        let into = to.path().join("repo");

        // One byte: every real clone exceeds it, which is the point — the cap
        // is enforced after fetching because the host reported no size.
        let repo = ContextRepo::new(format!("file://{}", from.path().display()))
            .with_paths(&["wanted"])
            .with_max_bytes(1);
        let spec = CloneSpec::plan(&repo, None).expect("a plan with no known size");
        let error = SystemCloner
            .fetch(&spec, &into)
            .await
            .expect_err("over the cap");

        match error {
            CloneError::Refused(CloneRefusal::TooLarge { cap, size, .. }) => {
                assert_eq!(cap, 1);
                assert!(size > 1, "the measured size is the clone's own bytes");
            }
            other => panic!("the cap must refuse rather than fail: {other:?}"),
        }
        assert!(
            !into.exists(),
            "an over-cap clone is removed rather than left on the volume"
        );
    }

    #[test]
    fn a_bare_owner_name_resolves_and_anything_else_is_named() {
        assert_eq!(
            url_of("kasecrab/api").as_deref(),
            Ok("https://github.com/kasecrab/api.git")
        );
        assert_eq!(
            url_of("https://gitlab.example/g/p.git").as_deref(),
            Ok("https://gitlab.example/g/p.git"),
            "a full URL is passed through to whatever host it names"
        );
        assert_eq!(
            url_of("git@github.com:kasecrab/api.git").as_deref(),
            Ok("git@github.com:kasecrab/api.git")
        );
        let error = url_of("not/a/repo/at/all").expect_err("three slashes is neither form");
        assert!(
            format!("{error}").contains("neither a clone URL nor"),
            "{error}"
        );
    }
}
