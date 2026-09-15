//! The file-access seam (PRD §34.9).
//!
//! `liyasa-core` and the other wasm-facing crates do no I/O themselves: in the
//! CLI this trait is the real file system, in the browser an in-memory map, in
//! tests a fixture.

use std::fmt;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::ids::Fingerprint;

pub type Bytes = bytes::Bytes;

/// A path normalized to forward slashes and relative to the project root.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct VfsPath(String);

impl VfsPath {
    /// Normalizes separators, strips `.` segments and any leading `/`, and
    /// resolves `..` lexically. A path that escapes the root normalizes to the
    /// root; a `Vfs` implementation is still expected to reject it.
    pub fn new(path: impl AsRef<str>) -> Self {
        let mut out: Vec<&str> = Vec::new();
        for segment in path.as_ref().split(['/', '\\']) {
            match segment {
                "" | "." => {}
                ".." => {
                    out.pop();
                }
                other => out.push(other),
            }
        }
        Self(out.join("/"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn extension(&self) -> Option<&str> {
        self.file_name()?.rsplit_once('.').map(|(_, ext)| ext)
    }

    pub fn file_name(&self) -> Option<&str> {
        self.0.rsplit('/').next().filter(|s| !s.is_empty())
    }

    pub fn parent(&self) -> Option<Self> {
        self.0
            .rsplit_once('/')
            .map(|(head, _)| Self(head.to_owned()))
    }

    pub fn join(&self, other: impl AsRef<str>) -> Self {
        if self.0.is_empty() {
            return Self::new(other);
        }
        Self::new(format!("{}/{}", self.0, other.as_ref()))
    }
}

impl fmt::Display for VfsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VfsKind {
    File,
    Dir,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VfsMeta {
    pub size: u64,
    pub mtime: Option<SystemTime>,
    pub kind: VfsKind,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VfsError {
    #[error("no such file: {0}")]
    NotFound(VfsPath),
    #[error("access denied: {0}")]
    Denied(VfsPath),
    #[error("i/o error: {0}")]
    Io(String),
}

pub trait Vfs: Send + Sync {
    fn read(&self, path: &VfsPath) -> Result<Bytes, VfsError>;
    fn metadata(&self, path: &VfsPath) -> Result<VfsMeta, VfsError>;
    fn list(&self, dir: &VfsPath) -> Result<Vec<VfsPath>, VfsError>;
    /// blake3 of the file's bytes. An implementation may cache this; callers
    /// must not assume it re-reads.
    fn fingerprint(&self, path: &VfsPath) -> Result<Fingerprint, VfsError>;
}
