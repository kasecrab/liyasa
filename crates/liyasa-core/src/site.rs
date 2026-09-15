//! Where the built site lives.
//!
//! Every absolute URL Liyasa emits is derived from [`SITE_URL`], so moving the
//! site is one edit here rather than a search across the workspace.

macro_rules! site_url {
    () => {
        "https://kasecrab.github.io/liyasa"
    };
}

/// Root of the published site, with no trailing slash.
pub const SITE_URL: &str = site_url!();

/// Root of the documentation, with no trailing slash.
pub const DOCS_URL: &str = concat!(site_url!(), "/docs");

/// Base for the `$id` of every published JSON Schema.
pub const SCHEMA_URL_BASE: &str = concat!(site_url!(), "/schema/v1/");

/// Base for the generated help URL of a code with no explicit `url` row.
pub const HELP_URL_BASE: &str = concat!(site_url!(), "/docs/errors/");
