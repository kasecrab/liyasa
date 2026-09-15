//! One conformance kit per frozen trait (PRD §31.7 gate item 5).
//!
//! A trait is a contract only if every implementation behaves the same way.
//! Each kit here is a function an implementer calls from their own test suite:
//!
//! ```ignore
//! #[test]
//! fn real_file_system_is_a_vfs() {
//!     liyasa_core::conformance::vfs::check(&RealVfs::new(root), &fixture());
//! }
//! ```
//!
//! The kits panic with a message naming the rule that broke, because they run
//! inside the implementer's `#[test]` and a panic is what a test reports.
//!
//! Enabled by the `conformance` feature, which downstream crates turn on in
//! their dev-dependencies so the kits never reach a production build.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

pub mod artifact_cache;
pub mod component_registry;
pub mod fixtures;
pub mod http_client;
pub mod rate_limiter;
pub mod runner;
pub mod secret_source;
pub mod vfs;

/// Drives one future to completion on the calling thread.
///
/// The kits for the async seams need an executor and `liyasa-core` has no
/// runtime dependency, so this is the smallest one that works: the traits'
/// futures are driven by the caller, never by a reactor, and a kit awaits
/// exactly one at a time.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = Box::pin(future);
    loop {
        if let Poll::Ready(value) = Pin::new(&mut future).poll(&mut context) {
            return value;
        }
        std::hint::spin_loop();
    }
}

/// Keeps a kit's assertions readable: the message always names the rule.
macro_rules! require {
    ($condition:expr, $($message:tt)*) => {
        assert!($condition, "contract violated: {}", format_args!($($message)*))
    };
}

pub(crate) use require;
