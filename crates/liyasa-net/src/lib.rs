//! The outbound HTTP client (PRD §30.2.3).
//!
//! One client, used by every crate that talks to the network. Policy is
//! enforced at connect time: the host is resolved once, every returned address
//! is checked against the denied classes, and the connection is pinned to a
//! validated address so no second lookup can swap it. Each redirect hop is
//! resolved, validated and pinned again.

pub mod client;
pub mod policy;

pub use client::{Client, ClientOptions};
pub use liyasa_core::net::{
    DenyReason, HostPattern, HostSet, HttpClient, HttpPolicy, HttpRequest, HttpResponse, NetError,
    Purpose,
};
