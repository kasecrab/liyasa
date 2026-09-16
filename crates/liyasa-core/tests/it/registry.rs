//! The rules `codes.toml` is held to (PRD §34.5).

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::diagnostics::{self, Code, Severity};

#[test]
fn registry_is_not_empty() {
    assert!(
        diagnostics::registry().len() >= 115,
        "every code named in PRD §34.5 must have a row"
    );
}

#[test]
fn codes_are_unique() {
    let mut seen = BTreeMap::new();
    for info in diagnostics::registry() {
        assert!(
            seen.insert(info.code.as_str(), info).is_none(),
            "duplicate code {}",
            info.code
        );
    }
}

#[test]
fn registry_is_sorted() {
    // `Code::new` binary-searches it.
    let mut previous = "";
    for info in diagnostics::registry() {
        assert!(
            previous < info.code.as_str(),
            "{} is out of order",
            info.code
        );
        previous = info.code.as_str();
    }
}

#[test]
fn numbers_are_never_reused_across_prefixes() {
    let mut seen: BTreeMap<u16, Code> = BTreeMap::new();
    for info in diagnostics::registry() {
        if let Some(other) = seen.insert(info.code.number(), info.code) {
            panic!("{} reuses the number of {other}", info.code);
        }
    }
}

#[test]
fn prefix_agrees_with_default_severity() {
    for info in diagnostics::registry() {
        let expected = match info.code.as_str().as_bytes()[0] {
            b'E' => Severity::Error,
            b'W' => Severity::Warning,
            other => panic!("{} has prefix `{}`, not E or W", info.code, other as char),
        };
        assert_eq!(
            info.severity, expected,
            "{} disagrees with its prefix",
            info.code
        );
    }
}

#[test]
fn every_code_falls_in_a_range_its_crate_owns() {
    for info in diagnostics::registry() {
        let number = info.code.number();
        let range = diagnostics::ranges()
            .iter()
            .find(|r| (r.first..=r.last).contains(&number))
            .unwrap_or_else(|| panic!("{} is outside every declared range", info.code));
        assert_eq!(
            info.krate, range.krate,
            "{} is claimed by {} but {}-{} belongs to {}",
            info.code, info.krate, range.first, range.last, range.krate
        );
    }
}

#[test]
fn ranges_are_ordered_and_do_not_overlap() {
    let mut end = 0u16;
    for range in diagnostics::ranges() {
        assert!(
            range.first <= range.last,
            "{}-{} is inverted",
            range.first,
            range.last
        );
        assert!(
            range.first > end,
            "{}-{} overlaps the range ending at {end}",
            range.first,
            range.last
        );
        end = range.last;
    }
}

#[test]
fn lookup_round_trips_and_rejects_strangers() {
    for info in diagnostics::registry() {
        let looked_up = Code::new(info.code.as_str()).expect("registered code resolves");
        assert_eq!(looked_up, info.code);
        assert_eq!(looked_up.title(), info.title);
    }
    assert!(Code::new("E9999").is_none());
    assert!(Code::new("").is_none());
    assert!(Code::new("nonsense").is_none());
}

#[test]
fn help_urls_are_generated_from_the_code() {
    let code = diagnostics::code::E0210;
    assert_eq!(
        code.url(),
        format!("{}E0210", liyasa_core::site::HELP_URL_BASE)
    );
}

#[test]
fn deserializing_an_unregistered_code_is_an_error() {
    let err = serde_json::from_str::<Code>("\"E9999\"").expect_err("must reject");
    assert!(err.to_string().contains("unknown code"), "{err}");
}

/// Every code the workspace raises, as a scan of `crates/*/src/`.
///
/// Emission is literal on this tree: `code::NNNN` generated from the registry
/// (418 sites) or `Code::new("NNNN")` (2). The three `Code::new(<variable>)`
/// sites are lookups, not emissions — a user's `--only` filter, a code parsed
/// back out of a minijinja message, a SARIF rule id — so none of them raises a
/// diagnostic that is not already raised somewhere literal. No `macro_rules!`
/// emits a coded diagnostic and no code is built by formatting a number; both
/// were checked when this was written.
///
/// `tests/` and `xtask/` are deliberately **not** scanned. A code a test names
/// is not a code the product raises: `E0721` and `W0720` appear only in
/// `tests/src/budget.rs` under `TODO(rfc-1101)`, waiting for `band` to move
/// into `liyasa-build`, and counting them as raised would hide exactly the
/// state this is here to track.
fn raised_codes() -> BTreeSet<String> {
    fn walk(dir: &std::path::Path, out: &mut BTreeSet<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let body = std::fs::read_to_string(&path).unwrap_or_default();
                out.extend(codes_in(&body));
            }
        }
    }

    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../");
    let mut out = BTreeSet::new();
    for entry in std::fs::read_dir(&crates)
        .expect("crates/ is readable")
        .flatten()
    {
        let src = entry.path().join("src");
        if src.is_dir() {
            walk(&src, &mut out);
        }
    }
    out
}

/// Every `[EW]dddd` token in a file's **code**, ignoring comments.
///
/// Comments have to go: `E0110` is named in one doc comment in `liyasa-import`
/// and raised nowhere, and a scan that counted prose would call that raised.
/// Five codes were hiding that way. String literals stay, because
/// `Code::new("E0805")` is a real emission, so this walks the file tracking
/// whether it is inside a string, a line comment or a block comment rather
/// than stripping `//` by regex — a URL in a string would otherwise swallow
/// the rest of its line.
fn codes_in(body: &str) -> BTreeSet<String> {
    #[derive(PartialEq)]
    enum In {
        Code,
        Str,
        RawStr,
        Line,
        Block,
    }

    let bytes = body.as_bytes();
    let mut out = BTreeSet::new();
    let mut state = In::Code;
    let mut at = 0usize;
    while at < bytes.len() {
        let rest = &bytes[at..];
        match state {
            In::Line if rest[0] == b'\n' => state = In::Code,
            In::Block if rest.starts_with(b"*/") => {
                state = In::Code;
                at += 1;
            }
            In::Line | In::Block => {}
            In::Str if rest[0] == b'\\' => at += 1,
            In::Str | In::RawStr if rest[0] == b'"' => state = In::Code,
            In::Code if rest.starts_with(b"//") => state = In::Line,
            In::Code if rest.starts_with(b"/*") => {
                state = In::Block;
                at += 1;
            }
            In::Code if rest.starts_with(b"r\"") => {
                state = In::RawStr;
                at += 1;
            }
            In::Code if rest[0] == b'"' => state = In::Str,
            // Code, and inside a string: `Code::new("E0805")` is an emission.
            In::Code | In::Str | In::RawStr => {
                if let Some(code) = code_at(body, bytes, at) {
                    out.insert(code);
                }
            }
        }
        at += 1;
    }
    out
}

/// A whole `[EW]dddd` word starting at `at`, if there is one.
fn code_at(body: &str, bytes: &[u8], at: usize) -> Option<String> {
    if !matches!(bytes[at], b'E' | b'W') {
        return None;
    }
    let word_before = at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
    let word_after = bytes
        .get(at + 5)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_');
    if word_before || word_after {
        return None;
    }
    let candidate = body.get(at..at + 5)?;
    candidate[1..]
        .bytes()
        .all(|b| b.is_ascii_digit())
        .then(|| candidate.to_owned())
}

/// The codes registered but not yet raised, pinned exactly (RFC 0008).
///
/// Claiming a code before the thing that raises it is the intended workflow —
/// CLAUDE.md says claim codes and never invent them, so a package reserves its
/// range early and fills it in later. `E09xx` is the AI crate, most of `E08xx`
/// the server, several `E06xx` verification runners: all unbuilt, all correctly
/// claimed. So the invariant is not "every code is raised" but "this set does
/// not change without someone saying why".
///
/// The list is pinned rather than the count, for three reasons: the failure
/// message can name what moved without a second lookup, `git blame` on a line
/// gives the commit that claimed it, and thirty concurrent packages appending
/// separate lines conflict far less than they would over one integer.
///
/// Removing a line means the code is now raised — delete it in the same commit.
/// Adding one means a code was claimed ahead of its implementation, which is
/// allowed; the commit that adds it is where the reason belongs.
const UNRAISED: &[&str] = &[
    "E0110", "E0604", "E0605", "E0606", "E0608", "E0620", "E0621", "E0704", "E0721", "E0801",
    "E0802", "E0803", "E0804", "E0808", "E0809", "E0810", "E0901", "E0902", "E0903", "E0904",
    "E0905", "W0015", "W0622", "W0713", "W0720", "W0811", "W0813", "W0906", "W1001", "W1005",
    "W1110", "W1111", "W1112", "W1113", "W1114", "W1115", "W1116",
];

#[test]
fn the_codes_nothing_raises_are_pinned_and_shrinking() {
    let raised = raised_codes();
    assert!(
        raised.len() > 100,
        "the scan found only {} codes, so it is broken rather than the registry",
        raised.len()
    );

    let actual: BTreeSet<&str> = diagnostics::registry()
        .iter()
        .map(|info| info.code.as_str())
        .filter(|code| !raised.contains(*code))
        .collect();
    let pinned: BTreeSet<&str> = UNRAISED.iter().copied().collect();

    let newly_unraised: Vec<&str> = actual.difference(&pinned).copied().collect();
    let now_raised: Vec<&str> = pinned.difference(&actual).copied().collect();

    assert!(
        newly_unraised.is_empty(),
        "registered and raised by nothing in crates/*/src/: {newly_unraised:?}\n\
         Raise it, or add it to UNRAISED in the same commit and say in the message \
         what will raise it (RFC 0008)."
    );
    assert!(
        now_raised.is_empty(),
        "these are raised now and can leave UNRAISED: {now_raised:?}\n\
         Delete them from the list in the commit that made them raise, so the \
         pin keeps meaning something."
    );
}
