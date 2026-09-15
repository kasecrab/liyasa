//! What every `Vfs` must do (PRD §34.9).

use super::require;
use crate::ids::Fingerprint;
use crate::vfs::{Vfs, VfsError, VfsKind, VfsPath};

/// What the implementation under test is expected to contain.
pub struct Fixture {
    /// A readable file and its exact bytes.
    pub file: (VfsPath, Vec<u8>),
    /// A directory that contains `file`.
    pub dir: VfsPath,
    /// A path that does not exist.
    pub missing: VfsPath,
    /// A path the implementation must refuse, such as one outside the root.
    /// `None` when the implementation has no notion of a denied path.
    pub denied: Option<VfsPath>,
}

pub fn check(vfs: &dyn Vfs, fixture: &Fixture) {
    let (path, bytes) = &fixture.file;

    let read = vfs.read(path).expect("the fixture file reads");
    require!(
        read.as_ref() == bytes.as_slice(),
        "read returns the file's bytes unchanged"
    );

    let meta = vfs.metadata(path).expect("the fixture file has metadata");
    require!(
        meta.kind == VfsKind::File,
        "a file's metadata says File, not {:?}",
        meta.kind
    );
    require!(
        meta.size == bytes.len() as u64,
        "metadata size is {} but the file is {} bytes",
        meta.size,
        bytes.len()
    );

    let fingerprint = vfs
        .fingerprint(path)
        .expect("the fixture file fingerprints");
    require!(
        fingerprint == Fingerprint::of(bytes),
        "fingerprint is not blake3 of the file's bytes"
    );
    require!(
        vfs.fingerprint(path).ok() == Some(fingerprint),
        "fingerprint is not stable across calls"
    );

    let listed = vfs.list(&fixture.dir).expect("the fixture directory lists");
    require!(
        listed.contains(path),
        "list omits a file that reads; list returned {listed:?}"
    );
    for entry in &listed {
        require!(
            vfs.metadata(entry).is_ok(),
            "list returned `{entry}`, which has no metadata"
        );
    }

    let dir_meta = vfs
        .metadata(&fixture.dir)
        .expect("the fixture directory has metadata");
    require!(
        dir_meta.kind == VfsKind::Dir,
        "a directory's metadata says Dir"
    );

    match vfs.read(&fixture.missing) {
        Err(VfsError::NotFound(at)) => {
            require!(
                at == fixture.missing,
                "NotFound names `{at}`, not the path asked for"
            );
        }
        other => panic!("contract violated: reading a missing path gave {other:?}, not NotFound"),
    }
    require!(
        matches!(vfs.metadata(&fixture.missing), Err(VfsError::NotFound(_))),
        "metadata on a missing path must be NotFound"
    );
    require!(
        matches!(
            vfs.fingerprint(&fixture.missing),
            Err(VfsError::NotFound(_))
        ),
        "fingerprint on a missing path must be NotFound"
    );

    if let Some(denied) = &fixture.denied {
        require!(
            matches!(
                vfs.read(denied),
                Err(VfsError::Denied(_) | VfsError::NotFound(_))
            ),
            "`{denied}` must be refused, not read"
        );
    }
}
