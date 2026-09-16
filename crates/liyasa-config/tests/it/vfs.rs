//! Both `Vfs` implementations against the kit in `liyasa_core::conformance`.

use liyasa_config::vfs::{MemVfs, OsVfs};
use liyasa_core::conformance::vfs::{Fixture, check};
use liyasa_core::vfs::{Vfs, VfsError, VfsPath};

const BYTES: &[u8] = b"{ \"name\": \"Acme Docs\" }\n";

fn fixture() -> Fixture {
    Fixture {
        file: (VfsPath::new("docs/liyasa.json"), BYTES.to_vec()),
        dir: VfsPath::new("docs"),
        missing: VfsPath::new("docs/absent.json"),
        denied: Some(VfsPath::new("docs/escape")),
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("liyasa-config-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("docs")).expect("scratch root is creatable");
    std::fs::write(root.join("docs/liyasa.json"), BYTES).expect("fixture file is writable");
    root
}

#[test]
fn memory_is_a_vfs() {
    let mut vfs = MemVfs::new();
    vfs.insert(VfsPath::new("docs/liyasa.json"), BYTES);
    check(
        &vfs,
        &Fixture {
            denied: None,
            ..fixture()
        },
    );
}

#[test]
fn the_file_system_is_a_vfs() {
    let root = scratch("conformance");
    check(
        &OsVfs::new(&root),
        &Fixture {
            denied: None,
            ..fixture()
        },
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_symlink_out_of_the_root_is_denied() {
    let root = scratch("escape");
    let outside = std::env::temp_dir().join(format!("liyasa-outside-{}", std::process::id()));
    std::fs::write(&outside, b"outside").expect("the outside file is writable");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, root.join("docs/escape")).expect("symlink is creatable");
    #[cfg(not(unix))]
    return;

    let vfs = OsVfs::new(&root);
    let escape = VfsPath::new("docs/escape");
    assert!(matches!(vfs.read(&escape), Err(VfsError::Denied(_))));
    assert!(matches!(vfs.metadata(&escape), Err(VfsError::Denied(_))));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&outside);
}

#[test]
fn a_path_that_climbs_past_the_root_stays_inside_it() {
    let root = scratch("climb");
    let vfs = OsVfs::new(&root);
    assert_eq!(
        vfs.read(&VfsPath::new("../../etc/passwd"))
            .expect_err("a path above the root does not exist"),
        VfsError::NotFound(VfsPath::new("etc/passwd")),
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn listing_is_sorted_and_relative_to_the_root() {
    let root = scratch("listing");
    std::fs::write(root.join("docs/a.md"), b"a").expect("writable");
    std::fs::write(root.join("docs/z.md"), b"z").expect("writable");
    let listed = OsVfs::new(&root)
        .list(&VfsPath::new("docs"))
        .expect("lists");
    assert_eq!(
        listed,
        vec![
            VfsPath::new("docs/a.md"),
            VfsPath::new("docs/liyasa.json"),
            VfsPath::new("docs/z.md"),
        ]
    );
    let _ = std::fs::remove_dir_all(&root);
}
