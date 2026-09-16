//! CLI-02 (process half) and MIG-10: `liyasa dev`.
//!
//! The rebuild loop is `liyasa-build`'s; the socket, the file watch, and the
//! live reload are here. The server is [`crate::serve`], written by hand
//! because §6.2 puts a web framework in `liyasa-server`.

use std::sync::atomic::Ordering;
use std::time::Duration;

use liyasa_build::dev::{Flags, Session};
use liyasa_build::watch::Watch;
use liyasa_config::vfs::OsVfs;
use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::source_map::SourceMap;
use liyasa_markdown::source::route::Ignore;

use crate::Exit;
use crate::cli::{Dev, Global};
use crate::{ctx, serve};

/// How long to block on the watcher before looping, so a Ctrl-C is noticed
/// promptly on platforms where it is delivered to the main thread.
const TICK: Duration = Duration::from_millis(250);

pub fn run(global: &Global, args: &Dev) -> Exit {
    let format = global.resolve(crate::cli::Format::Text);
    let cwd = ctx::cwd();
    let project = match ctx::locate(global, &cwd) {
        Ok(project) => project,
        Err(diagnostic) => {
            ctx::report(global, format, diagnostic);
            return Exit::Errors;
        }
    };

    let output = crate::commands::output_dir(&project);
    let flags = Flags {
        groups: args.groups.clone(),
        region: args.region.clone(),
        locale: args.locale.clone(),
        version: args.version_name.clone(),
        drafts: args.drafts,
        base_path: None,
    };

    if global.dry_run {
        println!("dev plan (--dry-run; nothing was served)");
        println!("  project  {}", ctx::display_relative(&project.root, &cwd));
        println!("  output   {}", ctx::display_relative(&output, &cwd));
        println!("  listen   http://{}:{}", args.host, args.port);
        println!(
            "  drafts   {}",
            if args.drafts { "included" } else { "excluded" }
        );
        return Exit::Success;
    }

    let printer = crate::diag::Printer::new(format, ctx::use_color(global));
    let vfs = OsVfs::new(&project.root);
    let mut session = Session::new(&project.root, flags);

    let first = session.first_render(&vfs);
    printer.emit(&first.report.diagnostics, &SourceMap::new());

    let server = match serve::Server::bind(&args.host, args.port, output.clone()) {
        Ok(server) => server,
        Err(error) => {
            ctx::report(
                global,
                format,
                Diagnostic::new(
                    code::E0002,
                    format!("could not listen on {}:{}: {error}", args.host, args.port),
                )
                .help("Pass `--port` to pick another, or stop whatever is already listening."),
            );
            return Exit::Errors;
        }
    };

    let address = server.local_addr().map_or_else(
        |_| format!("{}:{}", args.host, args.port),
        |at| at.to_string(),
    );
    let url = format!("http://{address}");
    let counter = server.build_counter();
    std::thread::spawn(move || server.serve_forever());

    if !global.quiet {
        println!(
            "{} pages in {:.2}s",
            first.report.pages,
            first.elapsed.as_secs_f64()
        );
        println!("serving {url}");
        println!(
            "watching {} for changes",
            ctx::display_relative(&project.root, &cwd)
        );
    }

    if args.open && !args.no_open {
        serve::open_browser(&url);
    }

    let ignore = std::fs::read_to_string(project.root.join(liyasa_build::tree::IGNORE_FILE))
        .map_or_else(|_| Ignore::parse(""), |text| Ignore::parse(&text));
    let output_name = output.file_name().map_or_else(
        || "dist".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );

    let watch = match Watch::new(&project.root, ignore, &output_name) {
        Ok(watch) => watch,
        Err(error) => {
            ctx::report(
                global,
                format,
                Diagnostic::new(
                    code::E0002,
                    format!("could not watch `{}`: {error}", project.root.display()),
                )
                .help("The site is being served; edits will not rebuild it."),
            );
            // Serving without watching is still useful, so this blocks rather
            // than exiting.
            loop {
                std::thread::park();
            }
        }
    };

    loop {
        let Some(batch) = watch.next_batch(TICK) else {
            continue;
        };
        if batch.is_empty() {
            continue;
        }
        let rebuilt = session.rebuild(&vfs, &batch);
        printer.emit(&rebuilt.report.diagnostics, &SourceMap::new());
        counter.fetch_add(1, Ordering::Relaxed);
        if !global.quiet {
            let changed = rebuilt.changed.len();
            println!(
                "rebuilt in {:.0} ms ({changed} page{} changed)",
                rebuilt.elapsed.as_secs_f64() * 1000.0,
                if changed == 1 { "" } else { "s" }
            );
        }
    }
}
