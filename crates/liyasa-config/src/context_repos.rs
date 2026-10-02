//! `contextRepos[]`, in the shape its consumer needs (CFG-99, GIT-11).
//!
//! The schema writes two of the fields as strings a human types — `maxBytes` is
//! `512MB`, `refresh` is `180d` — and `liyasa_git::ContextRepo` wants a byte
//! count and a number of seconds. Something has to convert, and the conversion
//! belongs to whoever owns the spelling, which is the schema's package: the
//! pattern that admits `1.5 GB` and the parser that reads it are then one
//! change apart rather than two crates apart.
//!
//! That is the opposite call from `review.rs`, which deliberately leaves its
//! durations as strings, and the difference is the consumer: VER-77 has a
//! `DurationSetting` that parses the syntax itself, and GIT-11's `ContextRepo`
//! has `Option<u64>`. `tests/config/cfg_99_context_repos.rs` pins both
//! conversions against the parsers already in the workspace, so a second
//! implementation of a syntax cannot drift from the first quietly.

use serde_json::Value;

// TODO(rfc-0112): `ai.agent.contextRepos` is a second key of this name with a
// different shape and a different cap — an access policy (AGT-12) rather than a
// clone policy (GIT-11). Nothing relates an entry of one to an entry of the
// other, and this reader deliberately does not: inventing the relationship in a
// diagnostic would teach an operator a rule the code does not have.

/// GIT-11's limit, ten per project. The number lives in
/// `liyasa_git::clone::MAX_CONTEXT_REPOS` as well, because this crate does not
/// depend on that one; `tests/config/cfg_99_context_repos.rs` asserts the two
/// are equal, so the duplicate cannot drift in silence.
pub const MAX_CONTEXT_REPOS: usize = 10;

/// One entry of `contextRepos[]`, with the two human-written sizes resolved.
///
/// Field for field what `liyasa_git::ContextRepo` holds, so a consumer maps it
/// without deciding anything: `ContextRepo::new(entry.repo)` and the four
/// `with_*` builders. The type is not `ContextRepo` itself because this crate
/// does not depend on `liyasa-git`, and a config reader is a poor reason to
/// make every reader of `liyasa.json` depend on a clone policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContextRepoConfig {
    /// A clone URL, or `owner/name`.
    pub repo: String,
    /// Branch, tag, or commit. `None` leaves it to the remote's HEAD.
    pub r#ref: Option<String>,
    /// The only paths fetched. **Empty is not "everything"** — GIT-11 refuses a
    /// clone with no paths, and `validate` says so as `W0142` before a deploy
    /// finds out.
    pub paths: Vec<String>,
    /// `None` leaves `liyasa_git::DEFAULT_DEPTH`.
    pub depth: Option<u32>,
    /// `maxBytes`, resolved. `None` is either absent or unparseable; the schema
    /// refuses the unparseable case, so a consumer sees its own default.
    pub max_bytes: Option<u64>,
    /// `refresh`, in whole seconds. A sub-second refresh rounds to zero rather
    /// than to one: `0` is "every time you ask", which is what `500ms` means to
    /// something that re-fetches a git repository.
    pub refresh_seconds: Option<u64>,
}

/// Reads `contextRepos[]` out of a whole `liyasa.json` value. A shape the
/// schema would reject reads as absent rather than panicking, and an entry with
/// no `repo` is dropped: there is nothing for a consumer to do with it, and
/// validation has already reported it.
pub fn context_repos(config: &Value) -> Vec<ContextRepoConfig> {
    let Some(entries) = config.pointer("/contextRepos").and_then(Value::as_array) else {
        return Vec::new();
    };
    entries.iter().filter_map(entry).collect()
}

fn entry(node: &Value) -> Option<ContextRepoConfig> {
    let repo = node.get("repo").and_then(Value::as_str)?;
    if repo.is_empty() {
        return None;
    }
    Some(ContextRepoConfig {
        repo: repo.to_owned(),
        r#ref: node.get("ref").and_then(Value::as_str).map(str::to_owned),
        paths: node
            .get("paths")
            .and_then(Value::as_array)
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|path| path.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        depth: node
            .get("depth")
            .and_then(Value::as_u64)
            .map(|depth| u32::try_from(depth).unwrap_or(u32::MAX)),
        max_bytes: node
            .get("maxBytes")
            .and_then(Value::as_str)
            .and_then(parse_bytes),
        refresh_seconds: node
            .get("refresh")
            .and_then(Value::as_str)
            .and_then(parse_refresh_seconds),
    })
}

/// A byte size as the schema's pattern spells it: `512MB`, `1.5 GB`. `KB` is
/// 1024 bytes, which is what every other size in this project means by it.
pub fn parse_bytes(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let number: f64 = number.parse().ok()?;
    let scale: u64 = match unit.trim().to_ascii_uppercase().as_str() {
        "" | "B" => 1,
        "KB" => 1024,
        "MB" => 1024 * 1024,
        "GB" => 1024 * 1024 * 1024,
        _ => return None,
    };
    Some((number * scale as f64) as u64)
}

/// `refresh`, as the schema's pattern spells it: `500ms`, `30s`, `180d`.
pub fn parse_refresh_seconds(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text.find(|c: char| !c.is_ascii_digit())?;
    let (count, unit) = text.split_at(split);
    let count: u64 = count.parse().ok()?;
    let millis: u64 = match unit.trim() {
        "ms" => 1,
        "s" => 1_000,
        "m" => 60 * 1_000,
        "h" => 60 * 60 * 1_000,
        "d" => 24 * 60 * 60 * 1_000,
        _ => return None,
    };
    Some(count.saturating_mul(millis) / 1_000)
}

/// A path that climbs out of the repository it names.
///
/// A leading slash is **not** one: git's own sparse-checkout syntax uses it to
/// anchor at the repository root, so `/etc/passwd` means that repository's own
/// `etc/passwd` and reaches nothing of the host. Only `..` leaves, which is
/// also what `liyasa_git`'s own check says — pinned against it in
/// `tests/config/cfg_99_context_repos.rs`.
pub fn escapes(path: &str) -> bool {
    path.split('/').any(|segment| segment == "..")
}
