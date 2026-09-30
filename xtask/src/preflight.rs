//! Everything CI runs that `bin/gate` does not (PRD §30.9).
//!
//! `bin/gate` is a subset of CI, and the difference is invisible: a session
//! reads its own green and pushes. CI was red on `main` for three days in
//! September 2026 with eleven sessions running, and none of them noticed,
//! because none of them was wrong about their own gate. This command is the
//! rest of it, runnable before a push.
//!
//! Four things are outside the local gate:
//!
//! 1. The **upstream corpus**. `bin/gate` runs `conformance` over
//!    `spec/markdown`, which is 49 hand-written cases. CI imports the
//!    CommonMark and GFM suites first and runs over those — an order of
//!    magnitude more cases, with expected output from the reference
//!    implementations rather than from us.
//! 2. **Parity** under wasmtime, over that same imported corpus.
//! 3. The cross-compiles: `liyasa-core` for both wasm targets, and `xtask` for
//!    WASI, which is the binary `parity` runs the corpus through.
//! 4. `--all-features`, which lints and builds the optional code.
//!
//! The suites are fetched with `curl`, the way `ci.yml` fetches them, rather
//! than from inside this process: `liyasa-net` is the only crate in the
//! workspace that opens a socket, and `corpus_import`'s header says why a
//! build tool must not. Shelling out keeps that true. Downloads are cached and
//! `--refresh` re-fetches.
//!
//! **Paths here are absolute, deliberately.** Defect 147 was a fix verified
//! with an absolute corpus path and shipped with a relative one: cargo test
//! binaries run with the CRATE root as the working directory, so `corpus`
//! resolved to nothing and the guard passed by finding no corpus. Every path
//! this module hands to a child is joined onto the repository root.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The upstream versions CI pins. They live in `.github/workflows/ci.yml` as
/// `env:` and are read from it rather than copied, so the two cannot drift —
/// a preflight that checked a different CommonMark release than CI would be
/// worse than no preflight, because it would report a green CI does not have.
pub fn versions(root: &Path) -> Result<(String, String), String> {
    let path = root.join(".github/workflows/ci.yml");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    versions_in(&text).ok_or_else(|| {
        format!(
            "{} declares no COMMONMARK_VERSION or GFM_SPEC_REF; preflight reads the \
             versions from CI rather than keeping its own copy, so it cannot run \
             without them",
            path.display()
        )
    })
}

/// The two pins, read out of a workflow's `env:` block.
pub fn versions_in(text: &str) -> Option<(String, String)> {
    let value = |key: &str| -> Option<String> {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(key)?.strip_prefix(':'))
            .map(|v| v.trim().trim_matches(['"', '\'']).to_owned())
            .filter(|v| !v.is_empty())
    };
    Some((value("COMMONMARK_VERSION")?, value("GFM_SPEC_REF")?))
}

/// The cross-compiles CI does and `bin/gate` does not.
///
/// `bin/gate` builds for the host and nothing else, so a `cfg` mistake is
/// invisible to it — including to the chain gate `bin/integrate` runs, which is
/// the verdict that decides every merge. On 2026-09-30 `main` went red on
/// `wasm32-wasip1` after a full green chain gate: a match arm added between two
/// others inherited nothing from the `#[cfg(not(target_family = "wasm"))]`
/// above it, because an attribute covers one item.
///
/// **These three need no network and no corpus**, which is what separates them
/// from the rest of this command. The upstream suites need a fetch and
/// `--all-features` is a second feature resolution, so `preflight` as a whole
/// belongs off the default gate path — but this part does not, and it is the
/// part that would have caught the red above.
///
/// `xtask` for WASI is the one that matters most and the one CI does not build
/// directly: it is built by `parity`, which needs a corpus, so on a machine
/// with no network nothing compiles it at all.
pub const CROSS: &[(&str, &str, &str)] = &[
    ("wasm-browser", "liyasa-core", "wasm32-unknown-unknown"),
    ("wasm-wasi", "liyasa-core", "wasm32-wasip1"),
    ("wasm-xtask", "xtask", "wasm32-wasip1"),
];

/// One check, and whether it is the reason to stop.
struct Step {
    name: &'static str,
    ok: bool,
    detail: String,
}

pub fn run(root: &Path, refresh: bool, offline: bool) -> Result<(), String> {
    let (commonmark, gfm) = versions(root)?;
    let cache = cache_dir(root)?;
    std::fs::create_dir_all(&cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    println!(
        "preflight: CommonMark {commonmark}, GFM {gfm}, cache {}",
        cache.display()
    );

    let mut steps = Vec::new();

    let corpus = cache.join("corpus");
    match fetch_and_import(root, &cache, &corpus, &commonmark, &gfm, refresh, offline) {
        Ok(cases) => {
            steps.push(Step {
                name: "corpus",
                ok: true,
                detail: format!("{cases} cases imported"),
            });
            steps.push(step(
                "conformance",
                run_cargo(
                    root,
                    &[
                        "run",
                        "-q",
                        "-p",
                        "xtask",
                        "--",
                        "conformance",
                        &corpus.display().to_string(),
                    ],
                ),
            ));
            steps.push(step(
                "parity",
                run_cargo(
                    root,
                    &[
                        "run",
                        "-q",
                        "-p",
                        "xtask",
                        "--",
                        "parity",
                        &corpus.display().to_string(),
                        "--strict",
                    ],
                ),
            ));
        }
        Err(why) => {
            // A corpus that did not arrive is not a pass. The whole point of
            // defect 147 is that a suite over zero cases cannot fail, so the
            // two steps it feeds are reported as unrun rather than skipped.
            steps.push(Step {
                name: "corpus",
                ok: false,
                detail: why,
            });
            steps.push(Step {
                name: "conformance",
                ok: false,
                detail: "not run: no corpus".to_owned(),
            });
            steps.push(Step {
                name: "parity",
                ok: false,
                detail: "not run: no corpus".to_owned(),
            });
        }
    }

    for (name, package, target) in CROSS {
        steps.push(step(
            name,
            run_cargo(root, &["build", "-q", "-p", package, "--target", target]),
        ));
    }
    steps.push(step(
        "all-features",
        run_cargo(
            root,
            &[
                "clippy",
                "-q",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
        ),
    ));

    println!();
    for s in &steps {
        println!(
            "{:<13} {}  {}",
            s.name,
            if s.ok { "ok  " } else { "FAIL" },
            s.detail
        );
    }
    let failed = steps.iter().filter(|s| !s.ok).count();
    match failed {
        0 => {
            println!("\npreflight: green — bin/gate plus this is what CI runs");
            Ok(())
        }
        n => Err(format!(
            "{n} of {} preflight checks failed; CI will be red on this tree",
            steps.len()
        )),
    }
}

fn step(name: &'static str, result: Result<(), String>) -> Step {
    match result {
        Ok(()) => Step {
            name,
            ok: true,
            detail: String::new(),
        },
        Err(detail) => Step {
            name,
            ok: false,
            detail,
        },
    }
}

/// Where the downloads and the imported corpus live.
///
/// Under `target/`, so it is gitignored and `bin/integrate` sweeps it with the
/// rest of the build output. Not under `spec/markdown`: that directory is
/// shared mutable state every worktree reads, and dropping 1,100 upstream
/// cases into it would turn every other package's `corpus` gate step into a
/// different run without a commit anywhere to say so.
fn cache_dir(root: &Path) -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("LIYASA_PREFLIGHT_DIR") {
        return Ok(PathBuf::from(dir));
    }
    Ok(root.join("target").join("preflight"))
}

fn fetch_and_import(
    root: &Path,
    cache: &Path,
    corpus: &Path,
    commonmark: &str,
    gfm: &str,
    refresh: bool,
    offline: bool,
) -> Result<usize, String> {
    let spec = cache.join(format!("commonmark-{commonmark}.json"));
    let gfm_spec = cache.join(format!("gfm-{gfm}.txt"));

    for (path, url) in [
        (
            &spec,
            format!("https://spec.commonmark.org/{commonmark}/spec.json"),
        ),
        (
            &gfm_spec,
            format!("https://raw.githubusercontent.com/github/cmark-gfm/{gfm}/test/spec.txt"),
        ),
    ] {
        let have = path.metadata().map(|m| m.len() > 0).unwrap_or(false);
        if have && !refresh {
            continue;
        }
        if offline {
            return Err(format!(
                "{} is not cached and --offline was given",
                path.display()
            ));
        }
        curl(&url, path)?;
    }

    // Import into a fresh directory. An import over a previous one would leave
    // cases from a version that is no longer pinned, and the count below would
    // then be a claim about two releases at once.
    if corpus.exists() {
        std::fs::remove_dir_all(corpus).map_err(|e| format!("{}: {e}", corpus.display()))?;
    }
    std::fs::create_dir_all(corpus).map_err(|e| format!("{}: {e}", corpus.display()))?;
    run_cargo(
        root,
        &[
            "run",
            "-q",
            "-p",
            "xtask",
            "--",
            "corpus",
            "import",
            "--commonmark",
            &spec.display().to_string(),
            "--commonmark-version",
            commonmark,
            "--gfm",
            &gfm_spec.display().to_string(),
            "--out",
            &corpus.display().to_string(),
        ],
    )?;

    // Count the cases, not the directories above them. The first version of
    // this counted top-level entries and reported "2 cases imported" for a run
    // that had just imported 674, because the importer groups them as
    // `<suite>/<section>/<case>.md`. The number is the empty-corpus guard, so a
    // number that is wrong in the safe direction is still a guard that cannot
    // fire: two suite directories holding nothing would have passed it.
    let cases = md_files(corpus)?;
    // The check defect 147 skipped. An empty corpus makes every suite that
    // reads it green, so it is a failure here rather than a quiet pass.
    if cases == 0 {
        return Err(format!("{} holds no cases after import", corpus.display()));
    }
    Ok(cases)
}

/// Every `.md` file under `dir`, at any depth: one per corpus case.
pub fn md_files(dir: &Path) -> Result<usize, String> {
    let mut found = 0;
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            found += md_files(&path)?;
        } else if path.extension().is_some_and(|e| e == "md") {
            found += 1;
        }
    }
    Ok(found)
}

fn curl(url: &str, into: &Path) -> Result<(), String> {
    println!("preflight: fetching {url}");
    let status = Command::new("curl")
        .args(["-sSfL", "--retry", "2", "-o"])
        .arg(into)
        .arg(url)
        .status()
        .map_err(|e| format!("curl: {e} (is it on PATH?)"))?;
    match status.success() {
        true => Ok(()),
        false => Err(format!("curl {url} exited {status}")),
    }
}

fn run_cargo(root: &Path, args: &[&str]) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut command = Command::new(&cargo);
    command.args(args).current_dir(root);
    // Any step that asks for a wasm target links with `rust-lld`, which rejects
    // the host's mold flag. See `parity::scrub_host_link_flag`.
    if args.iter().any(|a| a.starts_with("wasm32-")) {
        crate::parity::scrub_host_link_flag(&mut command);
    }
    let status = command.status().map_err(|e| format!("{cargo}: {e}"))?;
    match status.success() {
        true => Ok(()),
        false => Err(format!("cargo {} exited {status}", args.join(" "))),
    }
}
