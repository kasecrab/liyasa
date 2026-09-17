//! The runners that execute user code, and the sandboxes they execute it in
//! (VER-01, VER-02.1, VER-02.6 to VER-02.9, VER-02.17, VER-03 to VER-06).
//!
//! `core::runners` holds the half of §14 that only reads code. This half runs
//! it, and VER-03 is the reason the two are separate modules: nothing here
//! executes anything outside a `Sandbox`, so a machine with no container
//! runtime still gets every in-process check.

pub mod hidden;

pub use hidden::{DEFAULT_PREFIX, Split};
