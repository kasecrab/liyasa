//! The engine half of `liyasa dev` (PRD §6.6, CLI-02).
//!
//! The server and the CLI are other packages' work; what belongs here is the
//! loop between them: a first render off the persisted cache, then one rebuild
//! transaction per debounced batch, with the mock reader context the dev flags
//! set (`--groups`, `--region`, `--locale`, `--version`).
//!
//! A rebuild is the ordinary build: every page whose inputs are unchanged is a
//! cache hit, so the cost of an edit is the edited page.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use liyasa_core::build::Variant;
use liyasa_core::ids::{Locale, Route, Version};
use liyasa_core::vfs::VfsPath;

use crate::engine::{self, Options, Report};
use crate::git::{GitMeta, NoGit};
use crate::watch::{Batch, Kind};

/// The dev-only flags that change what is built rather than how it is served.
#[derive(Debug, Clone, Default)]
pub struct Flags {
    /// `--groups a,b`: the reader groups to mock.
    pub groups: Vec<String>,
    /// `--region`.
    pub region: Option<String>,
    /// `--locale`.
    pub locale: Option<String>,
    /// `--version`.
    pub version: Option<String>,
    /// `--drafts`.
    pub drafts: bool,
    /// `--base-path`.
    pub base_path: Option<String>,
}

impl Flags {
    /// The variant the preview shows: what a reader in those groups, that
    /// region, that locale, and that version would be served.
    pub fn variant(&self) -> Variant {
        Variant {
            version: self.version.clone().map(Version::new),
            locale: self.locale.clone().map(Locale::new),
            groups: self.groups.iter().cloned().collect(),
            region: self.region.clone(),
            ..Variant::default()
        }
    }

    fn options(&self) -> Options {
        Options {
            drafts: self.drafts,
            base_path: self.base_path.clone(),
            // A dev build is dated from the repository or the epoch, never the
            // wall clock: an edit must not change every page's timestamp.
            build_time: Some(0),
            ..Options::default()
        }
    }
}

/// What one rebuild transaction did.
#[derive(Debug)]
pub struct Rebuild {
    pub report: Report,
    pub elapsed: Duration,
    /// Routes whose HTML changed, which is what the socket tells the client.
    pub changed: Vec<Route>,
    /// Whether the client reloads rather than patching the body.
    pub reload: bool,
}

/// One dev session over one project.
pub struct Session {
    root: PathBuf,
    flags: Flags,
    /// The last build's HTML per route, so a rebuild can say what changed
    /// rather than reloading everything.
    previous: std::collections::BTreeMap<Route, liyasa_core::ids::Fingerprint>,
}

impl Session {
    pub fn new(root: &Path, flags: Flags) -> Self {
        Self {
            root: root.to_path_buf(),
            flags,
            previous: std::collections::BTreeMap::new(),
        }
    }

    pub fn flags(&self) -> &Flags {
        &self.flags
    }

    /// The first render: warm from `.liyasa/cache` when the project was built
    /// before, which is what makes the 1 s figure of CLI-02 reachable.
    pub fn first_render(&mut self, vfs: &dyn liyasa_core::vfs::Vfs) -> Rebuild {
        self.run(vfs, &NoGit, None)
    }

    /// One debounced batch (§6.6): a storm re-fingerprints the tree, which is
    /// what the ordinary build does anyway, so the difference is only whether
    /// the client patches or reloads.
    pub fn rebuild(&mut self, vfs: &dyn liyasa_core::vfs::Vfs, batch: &Batch) -> Rebuild {
        self.run(vfs, &NoGit, Some(batch))
    }

    pub fn rebuild_with_git(
        &mut self,
        vfs: &dyn liyasa_core::vfs::Vfs,
        git: &dyn GitMeta,
        batch: &Batch,
    ) -> Rebuild {
        self.run(vfs, git, Some(batch))
    }

    fn run(
        &mut self,
        vfs: &dyn liyasa_core::vfs::Vfs,
        git: &dyn GitMeta,
        batch: Option<&Batch>,
    ) -> Rebuild {
        let started = Instant::now();
        let report = engine::build(vfs, git, &self.root, &self.flags.options());
        let elapsed = started.elapsed();

        let mut changed = Vec::new();
        let mut current = std::collections::BTreeMap::new();
        if let Some(manifest) = &report.manifest {
            for route in &manifest.routes {
                let hash = route
                    .variants
                    .first()
                    .map(|variant| variant.hash)
                    .unwrap_or_else(|| liyasa_core::ids::Fingerprint::of(route.route.as_str()));
                if self.previous.get(&route.route) != Some(&hash) && !self.previous.is_empty() {
                    changed.push(route.route.clone());
                }
                current.insert(route.route.clone(), hash);
            }
        }
        let removed: Vec<Route> = self
            .previous
            .keys()
            .filter(|route| !current.contains_key(*route))
            .cloned()
            .collect();
        changed.extend(removed);
        changed.sort();
        changed.dedup();
        self.previous = current;

        Rebuild {
            reload: batch.is_none_or(Batch::needs_reload),
            report,
            elapsed,
            changed,
        }
    }
}

/// The routes a batch could have touched, for a server that wants to narrow a
/// socket message before the build finishes.
pub fn touched(batch: &Batch) -> BTreeSet<VfsPath> {
    match batch.kind {
        Kind::Storm => BTreeSet::new(),
        _ => batch.paths.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flags_become_the_previewed_variant() {
        let flags = Flags {
            groups: vec!["admin".to_owned()],
            region: Some("eu".to_owned()),
            locale: Some("de".to_owned()),
            version: Some("v2".to_owned()),
            ..Flags::default()
        };
        let variant = flags.variant();
        assert_eq!(crate::variants::key(&variant), "v=v2,l=de,g=admin,r=eu");
    }

    #[test]
    fn no_flags_is_the_default_variant() {
        assert_eq!(Flags::default().variant(), Variant::default());
    }

    #[test]
    fn a_storm_narrows_to_nothing_rather_than_to_everything() {
        let batch = Batch {
            paths: BTreeSet::new(),
            kind: Kind::Storm,
        };
        assert!(touched(&batch).is_empty());
        assert!(batch.needs_reload());
    }

    #[test]
    fn a_dev_build_is_dated_from_the_epoch_rather_than_the_wall_clock() {
        assert_eq!(Flags::default().options().build_time, Some(0));
    }
}
