//! The release benchmark (NFR-01). `cargo run --release -p liyasa-benches --bin bench`.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use liyasa_benches::budget::SIZES;
use liyasa_benches::measure::{self, Measurement};
use liyasa_benches::report;

const USAGE: &str = "\
usage: cargo run --release -p liyasa-benches --bin bench -- <command>

  run [--sizes N,N,N] [--out FILE] [--dir DIR] [--strict]
      Measure each size in its own process, print the changelog table and the
      §6.6 budgets, and write the record to FILE. Default sizes 100,1000;
      NFR-01's full set is 100,1000,10000 and takes minutes.
      --strict exits non-zero when a budget is missed.

  measure --pages N [--dir DIR]
      Measure one size in this process and print its JSON record. `run` calls
      this; peak resident set is a per-process high-water mark, so one size per
      process is the only way to report it per size.

  report --in FILE
      Re-print the tables from a record written earlier.

Run it in release. A debug build measures rustc's inlining decisions, not the
engine.
";

/// `run` without `--sizes` leaves out 10,000: it costs minutes and a quarter of
/// a gigabyte of disk, which is the wrong default for somebody checking a
/// change. The release pipeline passes the full set.
const DEFAULT_SIZES: &[usize] = &[100, 1_000];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match dispatch(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("bench: {message}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(args: &[String]) -> Result<ExitCode, String> {
    match args.first().map(String::as_str) {
        Some("run") => run(args),
        Some("measure") => {
            let pages = flag(args, "--pages")
                .ok_or("measure needs --pages")?
                .parse()
                .map_err(|e| format!("--pages: {e}"))?;
            let measurement = measure::measure(&dir(args), pages)?;
            println!(
                "{}",
                serde_json::to_string(&measurement).map_err(|e| e.to_string())?
            );
            Ok(ExitCode::SUCCESS)
        }
        Some("report") => {
            let path = flag(args, "--in").ok_or("report needs --in")?;
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let runs: Vec<Measurement> =
                serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
            print(&runs);
            Ok(ExitCode::SUCCESS)
        }
        Some("help" | "--help" | "-h") | None => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        Some(other) => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let sizes = match flag(args, "--sizes") {
        Some(list) => list
            .split(',')
            .map(|s| {
                s.trim()
                    .parse::<usize>()
                    .map_err(|e| format!("--sizes: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => DEFAULT_SIZES.to_vec(),
    };
    for size in &sizes {
        if !SIZES.contains(size) {
            eprintln!("bench: {size} is not one of the sizes NFR-01 names ({SIZES:?})");
        }
    }

    let directory = dir(args);
    let mut runs = Vec::new();
    for size in sizes {
        eprintln!("bench: measuring {size} pages");
        runs.push(measure_in_a_child(&directory, size)?);
    }

    print(&runs);

    if let Some(path) = flag(args, "--out") {
        let json = serde_json::to_string_pretty(&runs).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| format!("{path}: {e}"))?;
        eprintln!("bench: wrote {path}");
    }

    let missed = report::missed(&runs);
    if missed.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    for budget in &missed {
        eprintln!(
            "bench: MISSED {} ({})",
            budget.scenario,
            report::metric_label(budget.metric)
        );
    }
    if args.iter().any(|a| a == "--strict") {
        Ok(ExitCode::FAILURE)
    } else {
        Ok(ExitCode::SUCCESS)
    }
}

/// Runs `measure` in a child of this binary and reads back its record.
fn measure_in_a_child(directory: &Path, pages: usize) -> Result<Measurement, String> {
    let exe = std::env::current_exe().map_err(|e| format!("this binary's path: {e}"))?;
    let output = std::process::Command::new(&exe)
        .arg("measure")
        .arg("--pages")
        .arg(pages.to_string())
        .arg("--dir")
        .arg(directory)
        .output()
        .map_err(|e| format!("{}: {e}", exe.display()))?;
    if !output.status.success() {
        return Err(format!(
            "measuring {pages} pages failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("the child's record did not parse: {e}"))
}

fn print(runs: &[Measurement]) {
    println!("{}", report::table(runs));
    println!("{}", report::budgets(runs));
    println!("{}", report::provenance(runs));
}

fn dir(args: &[String]) -> PathBuf {
    flag(args, "--dir").map_or_else(std::env::temp_dir, PathBuf::from)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).map(String::as_str)
}
