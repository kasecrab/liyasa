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
