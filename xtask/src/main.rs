//! Repository tasks. Run with `cargo run -p xtask -- <command>`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use xtask::spike::engines;
use xtask::{conformance, corpus, corpus_import, corpus_seed, parity, schemas};

const USAGE: &str = "\
usage: cargo run -p xtask -- <command>

  schemas [--check] [--dir DIR]
      Regenerate schemas/ from the frozen Rust types.

  conformance DIR [--engine NAME] [--filter TEXT] [-v]
      Run the conformance corpus in DIR through one engine.

  parity DIR [--engine NAME] [--strict]
      Run the corpus natively and under WebAssembly and compare (§30.9).

  spike DIR [--filter TEXT]
      Run the corpus through every parser candidate and compare (§7.5.2).

  corpus import --commonmark FILE --gfm FILE --out DIR
      Import the upstream CommonMark and GFM suites.

  corpus seed --out DIR [--engine NAME] [--overwrite]
      Generate the Liyasa half of the corpus.

  corpus review-sample DIR [--percent N]
      Print the deterministic sample the fixture reviewer checks by hand.

engines: comrak-directive (default), comrak-markers, markdown-rs, pulldown-cmark
";

const DEFAULT_ENGINE: &str = "comrak-directive";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = dispatch(&args);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(args: &[String]) -> Result<(), String> {
    let verbose = args.iter().any(|a| a == "-v" || a == "--verbose");
    match args.first().map(String::as_str) {
        Some("schemas") => {
            let dir =
                flag(args, "--dir").map_or_else(|| repo_root().join("schemas"), PathBuf::from);
            schemas::run(&dir, args.iter().any(|a| a == "--check"))
        }
        Some("conformance") => {
            let dir = positional(args).ok_or("conformance needs a directory")?;
            let engine = flag(args, "--engine").unwrap_or(DEFAULT_ENGINE);
            conformance::main(Path::new(dir), engine, flag(args, "--filter"), verbose)
        }
        Some("parity") => {
            let dir = positional(args).ok_or("parity needs a directory")?;
            let engine = flag(args, "--engine").unwrap_or(DEFAULT_ENGINE);
            parity::run(Path::new(dir), engine, args.iter().any(|a| a == "--strict"))
        }
        Some("spike") => {
            let dir = positional(args).ok_or("spike needs a directory")?;
            spike(Path::new(dir), flag(args, "--filter"))
        }
        Some("corpus") => match args.get(1).map(String::as_str) {
            Some("import") => {
                let out = PathBuf::from(flag(args, "--out").ok_or("--out is required")?);
                let mut total = 0;
                if let Some(spec) = flag(args, "--commonmark") {
                    let version = flag(args, "--commonmark-version").unwrap_or("0.31.2");
                    let n = corpus_import::commonmark(Path::new(spec), version, &out)?;
                    println!("imported {n} CommonMark cases");
                    total += n;
                }
                if let Some(spec) = flag(args, "--gfm") {
                    let n = corpus_import::gfm(Path::new(spec), &out)?;
                    println!("imported {n} GFM cases");
                    total += n;
                }
                if total == 0 {
                    return Err("nothing imported; pass --commonmark and/or --gfm".to_owned());
                }
                Ok(())
            }
            Some("seed") => {
                let out = PathBuf::from(flag(args, "--out").ok_or("--out is required")?);
                let name = flag(args, "--engine").unwrap_or(DEFAULT_ENGINE);
                let engine = engines::by_name(name).ok_or(format!("unknown engine `{name}`"))?;
                let overwrite = args.iter().any(|a| a == "--overwrite");
                let (written, kept) = corpus_seed::run(&out, engine.as_ref(), overwrite)?;
                println!("seeded {written} cases, kept {kept} existing");
                Ok(())
            }
            Some("review-sample") => {
                let dir = args.get(2).ok_or("review-sample needs a directory")?;
                let percent = flag(args, "--percent")
                    .map_or(Ok(20), str::parse)
                    .map_err(|e| format!("--percent: {e}"))?;
                let cases = corpus::load(Path::new(dir))?;
                let sample = corpus_seed::review_sample(&cases, percent);
                for case in &sample {
                    println!("{}", case.header.id);
                }
                eprintln!("{} of {} cases ({percent}%)", sample.len(), cases.len());
                Ok(())
            }
            _ => Err(format!("unknown corpus subcommand\n\n{USAGE}")),
        },
        Some("help" | "--help" | "-h") | None => {
            print!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
}

/// Runs every candidate over the same corpus and prints the comparison the
/// parser decision rests on (§7.5.2).
fn spike(dir: &Path, filter: Option<&str>) -> Result<(), String> {
    let cases = corpus::load(dir)?;
    println!("{} cases from {}\n", cases.len(), dir.display());
    println!(
        "{:<18} {:>7} {:>7} {:>7} {:>8}",
        "engine", "pass", "fail", "skip", "pending"
    );
    let mut rows = Vec::new();
    for engine in engines::all() {
        let report = conformance::run(
            &cases,
            &conformance::Options {
                engine: engine.as_ref(),
                filter,
                verbose: false,
            },
        );
        let (pass, fail, skip, pending) = report.counts();
        println!(
            "{:<18} {pass:>7} {fail:>7} {skip:>7} {pending:>8}",
            engine.name()
        );
        rows.push((engine.name(), report));
    }
    println!();
    for (name, report) in &rows {
        let failures: Vec<_> = report
            .outcomes
            .iter()
            .filter(|o| o.verdict == conformance::Verdict::Fail)
            .collect();
        if failures.is_empty() {
            continue;
        }
        println!("{name}: {} failing", failures.len());
        for outcome in failures.iter().take(10) {
            println!("  {}", outcome.id);
        }
        if failures.len() > 10 {
            println!("  … and {} more", failures.len() - 10);
        }
    }
    Ok(())
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).map(String::as_str)
}

/// The first argument after the command that is not a flag or a flag's value.
fn positional(args: &[String]) -> Option<&str> {
    let mut skip_next = false;
    for arg in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg.starts_with('-') {
            skip_next = !matches!(
                arg.as_str(),
                "-v" | "--verbose" | "--check" | "--overwrite" | "--strict"
            );
            continue;
        }
        return Some(arg);
    }
    None
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}
