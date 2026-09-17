//! ED-07's browser file system: seeded once, server-backed after that.
//!
//! The whole content tree is never shipped to the browser. The session opens
//! with the page and everything the page's `ExpansionRecord` named, and any
//! other path is fetched on demand through `/_liyasa/editor/fs/<path>` —
//! authenticated, read-only, scoped to the draft — and cached by fingerprint
//! for the rest of the session.
//!
//! The fetch itself is the host's: this crate does no I/O. [`Fetch`] is the
//! seam, and `bindings` supplies the one that calls back into JavaScript.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use liyasa_core::ids::Fingerprint;
use liyasa_core::vfs::{Bytes, Vfs, VfsError, VfsKind, VfsMeta, VfsPath};

/// ED-07's cap on the preload set. Past it the editor previews on the server.
pub const PRELOAD_LIMIT: u32 = 2 * 1024 * 1024;

/// Resolves a path the seed did not carry.
///
/// `None` is "the draft has no such file", which becomes [`VfsError::NotFound`];
/// there is no way to report a transport failure differently, because the
/// browser's answer to a failed fetch and to a missing file is the same 404.
pub trait Fetch: Send + Sync {
    fn fetch(&self, path: &VfsPath) -> Option<Bytes>;
}

/// A resolver that has nothing behind it: every path must be in the seed.
pub struct Sealed;

impl Fetch for Sealed {
    fn fetch(&self, _path: &VfsPath) -> Option<Bytes> {
        None
    }
}

struct Entry {
    bytes: Bytes,
    fingerprint: Fingerprint,
}

impl Entry {
    fn new(bytes: Bytes) -> Self {
        Self {
            fingerprint: Fingerprint::of(&bytes),
            bytes,
        }
    }
}

pub struct EditorVfs {
    /// The seed and everything fetched since, one map: a caller cannot tell
    /// them apart and must not depend on the difference.
    files: Mutex<BTreeMap<VfsPath, Entry>>,
    preload_bytes: u32,
    fetched: Mutex<u32>,
    fetch: Box<dyn Fetch>,
}

impl EditorVfs {
    /// Seeds the session. `preload_bytes` counts what the seed carried, which
    /// is what ED-07's cap is about; what the session fetches later does not
    /// add to it.
    pub fn new(seed: impl IntoIterator<Item = (VfsPath, Bytes)>, fetch: Box<dyn Fetch>) -> Self {
        let mut files = BTreeMap::new();
        let mut preload_bytes: u32 = 0;
        for (path, bytes) in seed {
            preload_bytes = preload_bytes.saturating_add(bytes.len() as u32);
            files.insert(path, Entry::new(bytes));
        }
        Self {
            files: Mutex::new(files),
            preload_bytes,
            fetched: Mutex::new(0),
            fetch,
        }
    }

    /// A session with nothing behind it, for a draft that is one file.
    pub fn sealed(seed: impl IntoIterator<Item = (VfsPath, Bytes)>) -> Self {
        Self::new(seed, Box::new(Sealed))
    }

    pub fn preload_bytes(&self) -> u32 {
        self.preload_bytes
    }

    /// ED-07: over the cap the editor shows the large-page notice and the
    /// preview endpoint renders instead.
    pub fn over_budget(&self) -> bool {
        self.preload_bytes > PRELOAD_LIMIT
    }

    pub fn fetched(&self) -> u32 {
        *lock(&self.fetched)
    }

    /// Adds a file the host resolved after the session opened. It does not
    /// count towards the preload set: ED-07's cap is about what the page ships
    /// with, not about what it asks for afterwards.
    pub fn insert(&self, path: VfsPath, bytes: Bytes) {
        lock(&self.files).insert(path, Entry::new(bytes));
        *lock(&self.fetched) += 1;
    }

    pub fn holds(&self, path: &VfsPath) -> bool {
        lock(&self.files).contains_key(path)
    }

    /// The fingerprint a path is cached under, without reading it back.
    pub fn cached_fingerprint(&self, path: &VfsPath) -> Option<Fingerprint> {
        lock(&self.files).get(path).map(|entry| entry.fingerprint)
    }

    /// Reads through the cache, fetching once and only once per path.
    fn entry(&self, path: &VfsPath) -> Result<(Bytes, Fingerprint), VfsError> {
        if let Some(entry) = lock(&self.files).get(path) {
            return Ok((entry.bytes.clone(), entry.fingerprint));
        }
        let Some(bytes) = self.fetch.fetch(path) else {
            return Err(VfsError::NotFound(path.clone()));
        };
        let entry = Entry::new(bytes);
        let found = (entry.bytes.clone(), entry.fingerprint);
        lock(&self.files).insert(path.clone(), entry);
        *lock(&self.fetched) += 1;
        Ok(found)
    }

    /// The root is always a directory; anything else is one when a seeded or
    /// fetched path sits directly under it.
    fn is_dir(&self, dir: &VfsPath) -> bool {
        dir.as_str().is_empty() || !self.children(dir).is_empty()
    }

    /// `VfsPath::parent` is `None` for a top-level file, which is the root
    /// rather than no parent at all.
    fn children(&self, dir: &VfsPath) -> Vec<VfsPath> {
        lock(&self.files)
            .keys()
            .filter(|path| &path.parent().unwrap_or_else(|| VfsPath::new("")) == dir)
            .cloned()
            .collect()
    }
}

/// A poisoned lock still holds the map, and dropping the session over it would
/// lose the draft. There is one thread in a Web Worker anyway.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Vfs for EditorVfs {
    fn read(&self, path: &VfsPath) -> Result<Bytes, VfsError> {
        self.entry(path).map(|(bytes, _)| bytes)
    }

    fn metadata(&self, path: &VfsPath) -> Result<VfsMeta, VfsError> {
        if self.is_dir(path) {
            return Ok(VfsMeta {
                size: 0,
                mtime: None,
                kind: VfsKind::Dir,
            });
        }
        let (bytes, _) = self.entry(path)?;
        Ok(VfsMeta {
            size: bytes.len() as u64,
            mtime: None,
            kind: VfsKind::File,
        })
    }

    fn list(&self, dir: &VfsPath) -> Result<Vec<VfsPath>, VfsError> {
        if !self.is_dir(dir) {
            return Err(VfsError::NotFound(dir.clone()));
        }
        Ok(self.children(dir))
    }

    fn fingerprint(&self, path: &VfsPath) -> Result<Fingerprint, VfsError> {
        self.entry(path).map(|(_, fingerprint)| fingerprint)
    }
}
