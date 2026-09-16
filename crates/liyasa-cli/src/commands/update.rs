//! CLI-26: `liyasa update`.
//!
//! The verification is [`crate::update`]'s; what is here is reading the flags,
//! refusing a source this build cannot reach, and never replacing the binary
//! before both checks have passed.

use liyasa_core::diagnostics::{Diagnostic, code};

use crate::Exit;
use crate::cli::{Global, Update};
use crate::ctx;
use crate::update::{self, Failure};

pub fn run(global: &Global, args: &Update) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);

    let Some(source) = args.index.clone() else {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0009,
                "no release index to read".to_owned(),
            )
            .help("Pass `--index <directory>`, or set `LIYASA_UPDATE_INDEX`. This build has no network client, so the published index cannot be reached yet."),
        );
        return Exit::Network;
    };

    if global.offline && !source.starts_with("file://") && source.contains("://") {
        ctx::report(
            global,
            format,
            Diagnostic::new(code::E0009, "`--offline` refuses a remote release index"),
        );
        return Exit::Network;
    }

    let Some(path) = update::source_path(&source) else {
        // TODO(rfc-0900): a remote index needs `liyasa-net`.
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0006,
                format!("`{source}` is a remote index and this build has no HTTP client"),
            )
            .help("Point `--index` at a directory or a `file://` URL."),
        );
        return Exit::Network;
    };

    let index = match update::read_index(&path) {
        Ok(index) => index,
        Err(failure) => return fail(global, format, &failure),
    };

    let Some(release) = update::release(&index, args.version_name.as_deref()) else {
        ctx::report(
            global,
            format,
            Diagnostic::new(
                code::E0009,
                match &args.version_name {
                    Some(version) => format!("the index has no release {version}"),
                    None => "the index lists no releases".to_owned(),
                },
            ),
        );
        return Exit::Errors;
    };

    let current = crate::commands::version::VERSION;
    if release.version == current {
        if !global.quiet {
            println!("liyasa {current} is the newest release in this index");
        }
        return Exit::Success;
    }

    if args.check {
        if global.json {
            println!(
                "{}",
                serde_json::json!({
                    "current": current,
                    "available": release.version,
                    "notes": release.notes,
                })
            );
        } else {
            println!(
                "liyasa {current} is installed; {} is available",
                release.version
            );
            if let Some(notes) = &release.notes {
                println!("{notes}");
            }
        }
        return Exit::Success;
    }

    let Some(artifact) = release.artifact(update::TARGET) else {
        return fail(
            global,
            format,
            &Failure::NoArtifact {
                version: release.version.clone(),
                target: update::TARGET.to_owned(),
            },
        );
    };

    let artifact_path = path.parent().filter(|_| path.is_file()).map_or_else(
        || path.join(&artifact.file),
        |root| root.join(&artifact.file),
    );

    let bytes = match std::fs::read(&artifact_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return fail(
                global,
                format,
                &Failure::Index(format!("{}: {error}", artifact_path.display())),
            );
        }
    };

    // Before anything is written: the digest of what was read, then the
    // signature over that digest.
    if let Err(failure) = update::verify(&bytes, artifact) {
        return fail(global, format, &failure);
    }

    let Ok(executable) = std::env::current_exe() else {
        ctx::report(
            global,
            format,
            Diagnostic::new(code::E0002, "this process cannot name its own binary"),
        );
        return Exit::Errors;
    };

    if global.dry_run {
        println!("update plan (--dry-run; nothing was replaced)");
        println!("  current    {current}");
        println!("  available  {}", release.version);
        println!("  target     {}", update::TARGET);
        println!("  artifact   {}", artifact_path.display());
        println!("  digest     {} (verified)", artifact.sha256);
        println!("  signature  verified");
        println!("  would replace {}", executable.display());
        return Exit::Success;
    }

    match update::replace(&executable, &bytes) {
        Ok(()) => {
            if !global.quiet {
                println!(
                    "updated liyasa {current} to {} ({})",
                    release.version,
                    executable.display()
                );
            }
            Exit::Success
        }
        Err(error) => {
            ctx::report(
                global,
                format,
                Diagnostic::new(
                    code::E0002,
                    format!("could not replace `{}`: {error}", executable.display()),
                )
                .help("Run it again with permission to write the directory the binary is in."),
            );
            Exit::Errors
        }
    }
}

fn fail(global: &Global, format: crate::cli::Format, failure: &Failure) -> Exit {
    let mut diagnostic = Diagnostic::new(failure.code(), failure.message());
    if matches!(failure, Failure::Digest { .. } | Failure::Signature(_)) {
        diagnostic = diagnostic
            .help("The binary was not replaced. Fetch the release again, and report it if it fails a second time.");
    }
    ctx::report(global, format, diagnostic);
    match failure {
        Failure::Index(_) | Failure::NoArtifact { .. } => Exit::Network,
        Failure::Digest { .. } | Failure::Signature(_) => Exit::Errors,
    }
}
