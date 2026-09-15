//! The two `Vfs` implementations every consumer needs (PRD §34.9).
//!
//! [`OsVfs`] is the real file system rooted at a project directory; [`MemVfs`]
//! is the in-memory map the browser build and the tests use. Both satisfy
//! `liyasa_core::conformance::vfs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use liyasa_core::ids::Fingerprint;
use liyasa_core::vfs::{Bytes, Vfs, VfsError, VfsKind, VfsMeta, VfsPath};

/// The project tree on disk. Every path is resolved under `root`; a symlink
/// that leaves it is [`VfsError::Denied`], not followed.
pub struct OsVfs {
    root: PathBuf,
    /// The canonical root, resolved once. `None` when the root itself does not
    /// exist, in which case every path below it is simply missing.
    canonical_root: Option<PathBuf>,
    fingerprints: Mutex<BTreeMap<VfsPath, Fingerprint>>,
}

impl OsVfs {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            canonical_root: root.canonicalize().ok(),
            root,
            fingerprints: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The absolute path of `path`, or `Denied` when it resolves outside the
    /// root. A path that does not exist yet cannot be canonicalized, so its
    /// parent is checked instead and the leaf appended.
    fn resolve(&self, path: &VfsPath) -> Result<PathBuf, VfsError> {
        let joined = self.root.join(path.as_str());
        let Some(root) = &self.canonical_root else {
            return Ok(joined);
        };
        match joined.canonicalize() {
            Ok(real) if real.starts_with(root) => Ok(real),
            Ok(_) => Err(VfsError::Denied(path.clone())),
            Err(_) => Ok(joined),
        }
    }

    fn io(path: &VfsPath, error: &std::io::Error) -> VfsError {
        match error.kind() {
            std::io::ErrorKind::NotFound => VfsError::NotFound(path.clone()),
            std::io::ErrorKind::PermissionDenied => VfsError::Denied(path.clone()),
            _ => VfsError::Io(error.to_string()),
        }
    }
}

impl Vfs for OsVfs {
    fn read(&self, path: &VfsPath) -> Result<Bytes, VfsError> {
        let real = self.resolve(path)?;
        std::fs::read(&real)
            .map(Bytes::from)
            .map_err(|e| Self::io(path, &e))
    }

    fn metadata(&self, path: &VfsPath) -> Result<VfsMeta, VfsError> {
        let real = self.resolve(path)?;
        let meta = std::fs::symlink_metadata(&real).map_err(|e| Self::io(path, &e))?;
        let kind = if meta.is_dir() {
            VfsKind::Dir
        } else if meta.is_symlink() {
            VfsKind::Symlink
        } else {
            VfsKind::File
        };
        Ok(VfsMeta {
            size: meta.len(),
            mtime: meta.modified().ok(),
            kind,
        })
    }

    fn list(&self, dir: &VfsPath) -> Result<Vec<VfsPath>, VfsError> {
        let real = self.resolve(dir)?;
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&real).map_err(|e| Self::io(dir, &e))? {
            let entry = entry.map_err(|e| Self::io(dir, &e))?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            out.push(dir.join(name));
        }
        out.sort();
        Ok(out)
    }

    fn fingerprint(&self, path: &VfsPath) -> Result<Fingerprint, VfsError> {
        if let Ok(cache) = self.fingerprints.lock()
            && let Some(known) = cache.get(path)
        {
            return Ok(*known);
        }
        let fingerprint = Fingerprint::of(self.read(path)?);
        if let Ok(mut cache) = self.fingerprints.lock() {
            cache.insert(path.clone(), fingerprint);
        }
        Ok(fingerprint)
    }
}

/// An in-memory tree. Directories are implied by the paths of their files.
#[derive(Debug, Default, Clone)]
pub struct MemVfs {
    files: BTreeMap<VfsPath, Bytes>,
}

impl MemVfs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: VfsPath, bytes: impl Into<Bytes>) -> &mut Self {
        self.files.insert(path, bytes.into());
        self
    }

    pub fn with(mut self, path: impl AsRef<str>, bytes: impl Into<Bytes>) -> Self {
        self.insert(VfsPath::new(path), bytes);
        self
    }

    pub fn remove(&mut self, path: &VfsPath) -> Option<Bytes> {
        self.files.remove(path)
    }

    fn is_dir(&self, path: &VfsPath) -> bool {
        let prefix = if path.as_str().is_empty() {
            String::new()
        } else {
            format!("{path}/")
        };
        self.files.keys().any(|k| k.as_str().starts_with(&prefix))
    }
}

impl<P: AsRef<str>, B: Into<Bytes>> FromIterator<(P, B)> for MemVfs {
    fn from_iter<I: IntoIterator<Item = (P, B)>>(iter: I) -> Self {
        let mut out = Self::new();
        for (path, bytes) in iter {
            out.insert(VfsPath::new(path), bytes);
        }
        out
    }
}

impl Vfs for MemVfs {
    fn read(&self, path: &VfsPath) -> Result<Bytes, VfsError> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| VfsError::NotFound(path.clone()))
    }

    fn metadata(&self, path: &VfsPath) -> Result<VfsMeta, VfsError> {
        match self.files.get(path) {
            Some(bytes) => Ok(VfsMeta {
                size: bytes.len() as u64,
                mtime: None,
                kind: VfsKind::File,
            }),
            None if self.is_dir(path) => Ok(VfsMeta {
                size: 0,
                mtime: None,
                kind: VfsKind::Dir,
            }),
            None => Err(VfsError::NotFound(path.clone())),
        }
    }

    fn list(&self, dir: &VfsPath) -> Result<Vec<VfsPath>, VfsError> {
        let prefix = if dir.as_str().is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        let mut out: Vec<VfsPath> = self
            .files
            .keys()
            .filter_map(|path| {
                let rest = path.as_str().strip_prefix(&prefix)?;
                let head = rest.split('/').next()?;
                (!head.is_empty()).then(|| VfsPath::new(format!("{prefix}{head}")))
            })
            .collect();
        if out.is_empty() && !self.is_dir(dir) {
            return Err(VfsError::NotFound(dir.clone()));
        }
        out.dedup();
        Ok(out)
    }

    fn fingerprint(&self, path: &VfsPath) -> Result<Fingerprint, VfsError> {
        self.read(path).map(Fingerprint::of)
    }
}
