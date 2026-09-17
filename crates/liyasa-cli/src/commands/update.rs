//! CLI-26: `liyasa update`.
//!
//! The verification is [`crate::update`]'s; what is here is reading the flags,
//! finding the index — a directory, a `file://` URL or an `https://` one — and
//! never replacing the binary before both checks have passed.
//!
//! Where the bytes came from changes nothing about what is done to them: the
//! digest of what was actually read, then the signature over that digest, then
//! the replacement.

use liyasa_core::diagnostics::{Diagnostic, code};

use crate::Exit;
use crate::cli::{Global, Update};
use crate::ctx;
use crate::update::{self, Failure};

pub fn run(global: &Global, args: &Update) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);

    let Some(source) = args.index.clone() else {
        // TODO(rfc-0911): a published index gets a default here.
        ctx::report(
            global,
            format,
            Diagnostic::new(code::E0009, "no release index to read".to_owned()).help(
                "Pass `--index <url|directory>`, or set `LIYASA_UPDATE_INDEX`. No release index is published yet.",
            ),
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

    let located = match locate(&source) {
        Ok(located) => located,
        Err(failure) => return fail(global, format, &failure),
    };

    let index = match located.index() {
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

    let bytes = match located.artifact(&artifact.file) {
        Ok(bytes) => bytes,
        Err(failure) => return fail(global, format, &failure),
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
        println!("  artifact   {}", located.describe(&artifact.file));
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

/// Where the index and its artifacts are read from.
enum Source {
    Local(std::path::PathBuf),
    /// Boxed: a runtime and a client are two orders of magnitude larger than a
    /// path, and every local update would carry the difference.
    Remote(Box<Remote>),
}

/// The index URL, and the client that fetched it.
struct Remote {
    index: liyasa_core::net::Url,
    network: crate::net::Network,
}

/// A self-update borrows `SpecRef`: it is the purpose that requires TLS and
/// means "a document this installation was told to fetch". RFC 0911.
const PURPOSE: liyasa_core::net::Purpose = liyasa_core::net::Purpose::SpecRef;

fn locate(source: &str) -> Result<Source, Failure> {
    if let Some(path) = update::source_path(source) {
        return Ok(Source::Local(path));
    }

    let url = liyasa_core::net::Url::parse(source)
        .map_err(|error| Failure::Index(format!("`{source}` is not a URL: {error}")))?;
    // A URL that names a file is the index; anything else is the directory
    // holding one, which is the same rule a local source follows.
    let url = if url.path().ends_with(".json") {
        url
    } else {
        url.join(&format!(
            "{}/{}",
            url.path().trim_end_matches('/'),
            update::INDEX_FILE
        ))
        .map_err(|error| Failure::Index(format!("`{source}`: {error}")))?
    };
    // TODO(rfc-0911): outside a project there is no `network.*` to read, and
    // `liyasa update` usually runs outside one.
    let network = crate::net::Network::for_project(&serde_json::Value::Null)
        .map_err(|diagnostic| Failure::Unreachable(diagnostic.message.clone()))?;
    Ok(Source::Remote(Box::new(Remote {
        index: url,
        network,
    })))
}

impl Source {
    fn index(&self) -> Result<update::Index, Failure> {
        match self {
            Self::Local(path) => update::read_index(path),
            Self::Remote(remote) => {
                let bytes = fetch(&remote.network, &remote.index)?;
                update::parse_index(remote.index.as_str(), &bytes)
            }
        }
    }

    fn artifact(&self, file: &str) -> Result<Vec<u8>, Failure> {
        match self {
            Self::Local(path) => {
                let at = path.parent().filter(|_| path.is_file()).map_or_else(
                    || path.join(file),
                    |root| root.join(file),
                );
                std::fs::read(&at)
                    .map_err(|error| Failure::Index(format!("{}: {error}", at.display())))
            }
            Self::Remote(remote) => {
                let at = remote
                    .index
                    .join(file)
                    .map_err(|error| Failure::Index(format!("`{file}`: {error}")))?;
                fetch(&remote.network, &at)
            }
        }
    }

    fn describe(&self, file: &str) -> String {
        match self {
            Self::Local(path) => path
                .parent()
                .filter(|_| path.is_file())
                .map_or_else(|| path.join(file), |root| root.join(file))
                .display()
                .to_string(),
            Self::Remote(remote) => remote
                .index
                .join(file)
                .map_or_else(|_| file.to_owned(), |url| url.to_string()),
        }
    }
}

fn fetch(network: &crate::net::Network, url: &liyasa_core::net::Url) -> Result<Vec<u8>, Failure> {
    match network.get(url, PURPOSE) {
        Err(error) => Err(Failure::Unreachable(format!(
            "`{url}` could not be reached: {error}"
        ))),
        Ok(response) if response.status >= 400 => Err(Failure::Unreachable(format!(
            "`{url}` answered {}",
            response.status
        ))),
        Ok(response) => Ok(response.body.to_vec()),
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
        Failure::Index(_) | Failure::NoArtifact { .. } | Failure::Unreachable(_) => Exit::Network,
        Failure::Digest { .. } | Failure::Signature(_) => Exit::Errors,
    }
}
