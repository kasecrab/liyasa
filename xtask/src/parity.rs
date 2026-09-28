//! Native and WebAssembly parity for the corpus (PRD §30.9).
//!
//! The same corpus runs against the WebAssembly build of the same code and the
//! two must agree case for case: the editor renders its preview from a
//! `wasm32` build of the parser, so a divergence is a preview that lies.
//!
//! The comparison is a digest of every case's outcome rather than a rerun of
//! the diff, so a mismatch is one line and the detail is one `--verbose` away.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The WASI target: it gives the runner a file system, so it runs the real
/// corpus rather than an embedded copy. `wasm32-unknown-unknown` is covered
/// separately by `cargo build -p liyasa-core --target wasm32-unknown-unknown`.
const TARGET: &str = "wasm32-wasip1";

pub fn run(dir: &Path, engine: &str, strict: bool) -> Result<(), String> {
    let native = crate::conformance::report(dir, engine, None)?;
    println!("native  {}", native.digest());

    let wasm = match build(strict)? {
        Some(path) => path,
        None => {
            println!("wasm    unavailable; parity not checked");
            return Ok(());
        }
    };
    let Some(runtime) = runtime(strict)? else {
        println!("wasm    no runtime; parity not checked");
        return Ok(());
    };

    let corpus = dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    let output = Command::new(&runtime)
        .arg("run")
        .arg("--dir")
        .arg(format!("{}::/corpus", corpus.display()))
        .arg(&wasm)
        .args(["conformance", "/corpus", "--engine", engine])
        .output()
        .map_err(|e| format!("{runtime}: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let digest = stdout
        .lines()
        .find_map(|line| line.strip_prefix("digest "))
        .ok_or_else(|| {
            format!(
                "the WebAssembly run printed no digest\n{}{stdout}",
                String::from_utf8_lossy(&output.stderr)
            )
        })?;
    println!("wasm    {digest}");

    if digest == native.digest().to_string() {
        println!("parity  identical");
        return Ok(());
    }
    Err("native and WebAssembly runs disagree; rerun each with --verbose".to_owned())
}

fn build(strict: bool) -> Result<Option<PathBuf>, String> {
    let mut command = Command::new(cargo());
    command.args(["build", "-p", "xtask", "--target", TARGET]);
    scrub_host_link_flag(&mut command);
    let status = command.status().map_err(|e| format!("cargo: {e}"))?;
    if !status.success() {
        return if strict {
            Err(format!(
                "cargo build --target {TARGET} failed; is the target installed?"
            ))
        } else {
            Ok(None)
        };
    }
    let path = target_dir()?.join(TARGET).join("debug").join("xtask.wasm");
    if path.exists() {
        return Ok(Some(path));
    }
    Err(format!("{} was not produced", path.display()))
}

fn runtime(strict: bool) -> Result<Option<String>, String> {
    let name = std::env::var("LIYASA_WASM_RUNTIME").unwrap_or_else(|_| "wasmtime".to_owned());
    match Command::new(&name).arg("--version").output() {
        Ok(_) => Ok(Some(name)),
        Err(_) if strict => Err(format!(
            "`{name}` is not on PATH; install it or set LIYASA_WASM_RUNTIME"
        )),
        Err(_) => Ok(None),
    }
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}

fn target_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let output = Command::new(cargo())
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    value["target_directory"]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| "cargo metadata has no target_directory".to_owned())
}

/// Take the host's linker flag out of `RUSTFLAGS` for a wasm child.
///
/// `bin/buildenv` exports `-C link-arg=-fuse-ld=mold`, and `rust-lld` — which
/// is what links a wasm target — answers
///
/// ```text
/// rust-lld: error: unknown argument: -fuse-ld=mold
/// ```
///
/// so every local `parity` run died at the link step before comparing anything.
/// `buildenv` documents the defect and carries a partial fix that only covers
/// `CARGO_BUILD_TARGET`, not a `--target` on the command line, which is how
/// this module asks for wasm. Its complete fix — dropping the global
/// `RUSTFLAGS` and scoping mold to the host triple — changes every rustc
/// command line in the workspace and so invalidates sccache for every unit; it
/// is deliberately deferred to a pause.
///
/// This is the narrow version of it: one child process, one flag removed, host
/// builds untouched and their cache entries with them. Anything else in
/// `RUSTFLAGS` is preserved rather than cleared, because `RUSTFLAGS=` would
/// silently drop a flag somebody set on purpose.
pub fn scrub_host_link_flag(command: &mut Command) {
    if let Ok(flags) = std::env::var("RUSTFLAGS")
        && let Some(rest) = without_mold(&flags)
    {
        command.env("RUSTFLAGS", rest);
    }
}

/// `RUSTFLAGS` with mold's linker flag removed, or `None` if it was not there.
///
/// Both spellings cargo accepts are handled: `-C link-arg=...` as two tokens,
/// which is what `buildenv` writes, and `-Clink-arg=...` as one. Removing half
/// of the two-token form would leave a bare `-C` and rustc would reject the
/// command line, which is a worse failure than the one being fixed.
pub fn without_mold(flags: &str) -> Option<String> {
    const ARG: &str = "link-arg=-fuse-ld=mold";
    if !flags.contains("-fuse-ld=mold") {
        return None;
    }
    let mut out: Vec<&str> = Vec::new();
    let mut tokens = flags.split_whitespace().peekable();
    while let Some(token) = tokens.next() {
        if token == "-C" && tokens.peek() == Some(&ARG) {
            tokens.next();
            continue;
        }
        if token.strip_prefix("-C") == Some(ARG) {
            continue;
        }
        out.push(token);
    }
    Some(out.join(" "))
}
