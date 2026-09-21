//! Every public function on the two composition types that nothing in the
//! product calls (defects 65, 86, 91, and HOST-05's tracing).
//!
//! Seven times in two days this project has shipped a correct, unit-tested
//! mechanism that nothing invokes: the assistant's tools with no endpoint to
//! construct them, a role source with nothing reading it, a JWT verifier no
//! handler calls, an operator key with no schema, and a trace exporter that
//! had never executed. Every one was found by a person happening to reach for
//! it. This is the mechanical version, scoped to the two types where the
//! product is composed.
//!
//! **It counts callers in production code only.** A test calling something is
//! not the product calling it — that is precisely how HOST-05's tracing came
//! to be marked done, by an acceptance test that built its own state and
//! called the exporter by hand. A version of this ratchet that accepted a test
//! as a caller would have passed on the same day.
//!
//! What an entry leaving `UNCALLED_TODAY` proves: some production code now
//! names that function. Not that the RIGHT code does, and not that the call is
//! correct. "Is it wired" becomes mechanical here; "is it wired correctly"
//! stays a reading job.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Every public function of `AppState` or `Runtime` with no caller outside
/// tests, each with the reason it is acceptable — or the defect it is.
///
/// A bare list of names becomes the thing nobody reads. Adding a row here is
/// a claim that somebody looked, so the claim is written down.
const UNCALLED_TODAY: &[(&str, &str)] = &[
    (
        "export_once",
        "deliberate: the `once` variant exists so a test need not wait on the \
         five-second export timer. The loop is what production runs.",
    ),
    (
        "with_limiter",
        "deliberate: a test-only builder. `main` configures pools through \
         `state.limiter.configure` instead, so there is nothing to replace.",
    ),
];

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Source with comments and `#[cfg(test)]` modules removed, so a name that
/// appears only in a doc comment or only in a unit test does not read as a
/// caller. Both mistakes have already been made measuring this.
fn production_source() -> String {
    let mut out = String::new();
    let mut files = Vec::new();
    collect(&repo().join("crates"), &mut files);
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        out.push_str(&strip_test_modules(&strip_comments(&text)));
        out.push('\n');
    }
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_block = 0usize;
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_block > 0 {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block -= 1;
            } else if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_block += 1;
            }
            continue;
        }
        if in_string {
            if c == '\\' {
                chars.next();
            } else if c == '"' {
                in_string = false;
            }
            out.push(c);
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
                out.push('\n');
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                in_block = 1;
            }
            _ => out.push(c),
        }
    }
    out
}

/// Drops `#[cfg(test)] mod ... { .. }` by matching braces from the `mod`.
fn strip_test_modules(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let Some(open) = after.find('{') else {
            break;
        };
        let mut depth = 0usize;
        let mut end = None;
        for (index, c) in after[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + index + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        match end {
            Some(end) => rest = &after[end..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// The public functions declared in one `impl` block.
fn public_functions(source: &str, header: &str) -> Vec<String> {
    let Some(start) = source.find(header) else {
        panic!("`{header}` is not in that file any more; this ratchet is measuring nothing");
    };
    let body = &source[start + header.len()..];
    let mut depth = 1usize;
    let mut end = body.len();
    for (index, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = index;
                    break;
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for line in body[..end].lines() {
        let line = line.trim();
        for prefix in ["pub fn ", "pub async fn "] {
            if let Some(rest) = line.strip_prefix(prefix)
                && let Some(name) = rest.split('(').next()
            {
                out.push(name.trim().to_owned());
            }
        }
    }
    out
}

/// Occurrences of `name(` that are not the definition.
fn has_production_caller(source: &str, name: &str) -> bool {
    let needle = format!("{name}(");
    let mut from = 0;
    while let Some(at) = source[from..].find(&needle) {
        let absolute = from + at;
        let before = source[..absolute].trim_end();
        let is_definition = before.ends_with("fn");
        let preceded_by_ident = source[..absolute]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if !is_definition && !preceded_by_ident {
            return true;
        }
        from = absolute + needle.len();
    }
    false
}

fn uncalled() -> BTreeSet<String> {
    let source = production_source();
    let mod_rs = std::fs::read_to_string(repo().join("crates/liyasa-server/src/routes/mod.rs"))
        .expect("routes/mod.rs");
    let serve_rs = std::fs::read_to_string(repo().join("crates/liyasa-server/src/routes/serve.rs"))
        .expect("routes/serve.rs");

    let mut names = public_functions(&mod_rs, "impl AppState {");
    names.extend(public_functions(&serve_rs, "impl Runtime {"));
    assert!(
        names.len() > 20,
        "only {} public functions found; the parser has stopped matching and \
         an empty result would read as a clean bill of health",
        names.len()
    );

    names
        .into_iter()
        .filter(|name| !has_production_caller(&source, name))
        .collect()
}

#[test]
fn nothing_new_is_built_without_a_caller() {
    let found = uncalled();
    let pinned: BTreeSet<String> = UNCALLED_TODAY
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect();
    assert_eq!(
        found, pinned,
        "\nthe set of public functions with no production caller changed.\n\
         If it GREW, something was built that nothing invokes — wire it, or \
         add a row to UNCALLED_TODAY saying why it is deliberate.\n\
         If it SHRANK, delete the row: its entry leaving this list is the \
         proof that the wiring landed.\n"
    );
}

#[test]
fn every_pinned_entry_carries_a_reason_somebody_wrote() {
    for (name, reason) in UNCALLED_TODAY {
        assert!(
            reason.len() > 40,
            "`{name}` has no real reason, and a bare list is the thing nobody \
             reads"
        );
    }
    let mut names: Vec<&str> = UNCALLED_TODAY.iter().map(|(name, _)| *name).collect();
    let sorted = {
        let mut copy = names.clone();
        copy.sort_unstable();
        copy
    };
    assert_eq!(names, sorted, "UNCALLED_TODAY is not sorted");
    names.dedup();
    assert_eq!(names.len(), UNCALLED_TODAY.len(), "a name is pinned twice");
}

#[test]
fn a_test_only_caller_does_not_count_as_a_caller() {
    // The property the whole ratchet rests on, and the one that would have
    // caught HOST-05: an acceptance test calling something is not the product
    // calling it. `with_limiter` is called by tests/src/server.rs and by
    // nothing else, so if this ever fails the source filter has stopped
    // excluding tests and the ratchet is worthless.
    //
    // It used to name `with_tracer`, which was a defect rather than a
    // deliberate test-only builder. That entry left the list when the binary
    // started enabling the tracer, which is what an entry leaving is supposed
    // to mean.
    let source = production_source();
    assert!(
        !has_production_caller(&source, "with_limiter"),
        "`with_limiter` reads as called from production, so either it gained a \
         real caller — delete its row and pick another test-only function for \
         this assertion — or the filter is broken"
    );
}

// ---- the same shape, one level up: a surface declared and never mounted ----

/// Every `/_liyasa/` prefix `pool_for` claims that no route answers, with the
/// reason it is acceptable — or the defect it is.
///
/// `pool_for` maps a request path to a rate-limit bucket, and it is the
/// closest thing the server has to a list of its own surfaces: complete and
/// current for everything that does exist, and nobody adds a limiter bucket
/// speculatively. That is exactly what makes it dangerous — it is a list of
/// intentions that reads as a list of facts, and four entries sat in it for
/// weeks pointing at nothing (defect 150). Each has its mechanism built and
/// unit-tested; what is missing is the route.
const UNROUTED_TODAY: &[(&str, &str)] = &[
    (
        "/_liyasa/assistant",
        "defect 146: the retrieval filter is written and tested — `ReaderContext.groups` \
         feeds `ChunkQuery.groups` and the store applies it during retrieval (RFC 1807) — \
         but `ReaderContext` is constructed only in tests, because no handler builds one.",
    ),
    (
        "/_liyasa/mcp",
        "defect 149: and this one reaches users. Every generated `llms.txt` carries \
         `MCP server: <origin>/mcp`, so an agent reading the site's own index is told \
         where to connect and the address answers 404.",
    ),
    (
        "/_liyasa/proxy",
        "defect 150: API-41's playground proxy. `ProxySource::of`, `allow_list`, \
         `forwardable` and `ProxyEvent` all exist in `liyasa-openapi`; `git grep \
         playground` over this crate returns nothing.",
    ),
    (
        "/_liyasa/search",
        "defect 146: an index is built into `dist/search-index/` and nothing reads it — \
         no endpoint, and no client-side fetch either. It was briefly served as a static \
         file until that was recognised as shipping every restricted page's text.",
    ),
];

/// The `starts_with("/_liyasa/...")` prefixes `pool_for` claims.
///
/// Scoped to that function by brace matching: `pool_name` below it matches on
/// the same paths for its metric label, and counting those would double every
/// prefix.
fn pool_prefixes(mod_rs: &str) -> BTreeSet<String> {
    const HEADER: &str = "pub fn pool_for(";
    let start = mod_rs
        .find(HEADER)
        .expect("`pool_for` is not in routes/mod.rs any more; this ratchet measures nothing");
    let body = &mod_rs[start..];
    let open = body.find('{').expect("a body");
    let mut depth = 0usize;
    let mut end = body.len();
    for (index, c) in body[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = open + index;
                    break;
                }
            }
            _ => {}
        }
    }
    let mut out = BTreeSet::new();
    let mut rest = &body[open..end];
    while let Some(at) = rest.find("starts_with(\"") {
        let after = &rest[at + "starts_with(\"".len()..];
        if let Some(close) = after.find('"') {
            let path = &after[..close];
            if path.starts_with("/_liyasa/") {
                out.insert(path.to_owned());
            }
            rest = &after[close..];
        } else {
            break;
        }
    }
    out
}

/// Every path handed to `.route(`, across the crate.
///
/// Reads the first string literal after each `.route(` rather than matching
/// `.route("..."` on one line: `/_liyasa/e` is mounted with its path on its
/// own line, and a single-line pattern reports it as unrouted. That mistake
/// was made while writing this.
fn mounted_paths(source: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = source;
    while let Some(at) = rest.find(".route(") {
        let after = &rest[at + ".route(".len()..];
        let Some(open) = after.find('"') else {
            rest = after;
            continue;
        };
        let literal = &after[open + 1..];
        match literal.find('"') {
            Some(close) => {
                out.insert(literal[..close].to_owned());
                rest = &literal[close..];
            }
            None => break,
        }
    }
    out
}

fn server_source() -> String {
    let mut out = String::new();
    let mut files = Vec::new();
    collect(&repo().join("crates/liyasa-server/src"), &mut files);
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        out.push_str(&strip_test_modules(&strip_comments(&text)));
        out.push('\n');
    }
    out
}

#[test]
fn the_route_census_is_not_defeated_by_a_nested_router() {
    // `mounted_paths` reads `.route(` calls. A `nest()` would mount a whole
    // subtree under a prefix that never appears in one, and every assertion
    // below would then be measuring a list it cannot see. There is no nesting
    // in this crate today; if that changes, this ratchet has to learn about it
    // before it can be trusted again.
    let source = server_source();
    for needle in [".nest(", ".nest_service("] {
        assert!(
            !source.contains(needle),
            "`{needle}` appeared in liyasa-server. `mounted_paths` cannot see routes mounted \
             that way, so the unrouted-pool ratchet is now measuring less than it claims. \
             Teach it to resolve the prefix before removing this assertion."
        );
    }
}

#[test]
fn no_pool_claims_a_surface_that_nothing_serves() {
    let mod_rs = std::fs::read_to_string(repo().join("crates/liyasa-server/src/routes/mod.rs"))
        .expect("routes/mod.rs");
    let prefixes = pool_prefixes(&mod_rs);
    assert!(
        prefixes.len() >= 5,
        "only {} pool prefixes parsed out of `pool_for`; the parser has stopped matching \
         and an empty result would read as a clean bill of health",
        prefixes.len()
    );

    let mounted = mounted_paths(&server_source());
    assert!(
        mounted.iter().any(|path| path == "/_liyasa/e"),
        "`/_liyasa/e` is mounted across several lines and is the case that proves \
         `mounted_paths` reads more than one-line `.route(\"...\")` calls; it is missing, \
         so the census is wrong"
    );

    let unrouted: BTreeSet<String> = prefixes
        .into_iter()
        .filter(|prefix| !mounted.iter().any(|path| path.starts_with(prefix)))
        .collect();
    let pinned: BTreeSet<String> = UNROUTED_TODAY
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect();
    assert_eq!(
        unrouted, pinned,
        "\nthe set of rate-limit pools with no route changed.\n\
         If it GREW, a pool was added for a surface nobody mounted — mount it, or add a \
         row to UNROUTED_TODAY saying why the bucket exists without the endpoint.\n\
         If it SHRANK, delete the row: the entry leaving this list is the proof that the \
         endpoint landed.\n"
    );
}

#[test]
fn every_unrouted_pool_carries_a_reason_somebody_wrote() {
    for (path, reason) in UNROUTED_TODAY {
        assert!(
            reason.len() > 40,
            "`{path}` is pinned with a reason too short to have been thought about: {reason}"
        );
    }
}
