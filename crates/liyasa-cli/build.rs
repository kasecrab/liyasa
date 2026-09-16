//! The target triple is not available to the compiled code any other way, and
//! `liyasa update` needs it to pick an artifact, `liyasa doctor` to report what
//! this binary is (CLI-32).

fn main() {
    println!(
        "cargo::rustc-env=LIYASA_TARGET={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_owned())
    );
    println!("cargo::rerun-if-changed=build.rs");
}
