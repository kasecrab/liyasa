//! The command tree of §16.1, and the global flags of CLI-34.
//!
//! Every flag that can sensibly come from the environment carries its
//! `LIYASA_*` name here rather than being read by hand in the command, so
//! CLI-34 holds for a flag the moment it is declared and `--help` documents the
//! variable without a second list to keep in step.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// The `liyasa` binary.
#[derive(Debug, Parser)]
#[command(
    name = "liyasa",
    version,
    about = "Documentation that proves itself.",
    disable_help_subcommand = true,
    max_term_width = 100
)]
pub struct Cli {
    #[command(flatten)]
    pub global: Global,
    #[command(subcommand)]
    pub command: Command,
}

/// Flags every command accepts (CLI-34).
#[derive(Debug, Clone, Args)]
pub struct Global {
    /// Read this `liyasa.json` instead of searching upward from the working
    /// directory.
    #[arg(long, global = true, value_name = "PATH", env = "LIYASA_CONFIG")]
    pub config: Option<PathBuf>,

    /// When to colour output.
    #[arg(
        long,
        global = true,
        value_name = "WHEN",
        value_enum,
        default_value_t = Color::Auto,
        env = "LIYASA_COLOR"
    )]
    pub color: Color,

    /// Print what would happen and change nothing.
    #[arg(long, global = true, env = "LIYASA_DRY_RUN")]
    pub dry_run: bool,

    /// Print errors and nothing else.
    #[arg(long, short, global = true, env = "LIYASA_QUIET")]
    pub quiet: bool,

    /// Make no outbound request; fail rather than reach the network (HOST-08).
    #[arg(long, global = true, env = "LIYASA_OFFLINE")]
    pub offline: bool,

    /// Print output as JSON (CLI-30). Where a command has `--format`, this is
    /// the same as `--format json` and `--format` wins if both are given.
    #[arg(long, global = true, env = "LIYASA_JSON")]
    pub json: bool,
}

impl Global {
    /// The format a command should use: its own `--format` when that was set
    /// to anything but the default, otherwise `--json`, otherwise text.
    pub fn resolve(&self, explicit: Format) -> Format {
        if explicit == Format::Text && self.json {
            Format::Json
        } else {
            explicit
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Color {
    /// Colour when stdout is a terminal.
    #[default]
    Auto,
    Always,
    Never,
}

/// How a command that reports diagnostics should print them (CLI-30).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum Format {
    /// Code frames for a human.
    #[default]
    Text,
    /// One JSON document, stable across releases.
    Json,
    /// SARIF 2.1.0, for GitHub code scanning.
    Sarif,
    /// JUnit XML, for a CI test report.
    Junit,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a new documentation project (CLI-01).
    New(New),
    /// Serve the project with live reload (CLI-02).
    Dev(Dev),
    /// Build the site into the output directory (CLI-03).
    Build(Build),
    /// Check configuration, content, links, and specs (CLI-04).
    Validate(Validate),
    /// Rewrite Markdown, front matter, and config into canonical form (CLI-05).
    Format(Format_),
    /// Run the verification checks (CLI-06).
    Verify(Verify),
    /// Check internal and external links (CLI-07).
    BrokenLinks(BrokenLinks),
    /// Run accessibility, performance, and agent-readiness tests (CLI-08).
    Test(Test),
    /// Print the documentation quality score (CLI-09).
    Score(Score),
    /// Export the built site in another shape (CLI-10).
    Export(Export),
    /// Run the Liyasa server (CLI-11).
    Serve(Serve),
    /// Query the local search index (CLI-12).
    Search(Search),
    /// Print a JSON Schema (CLI-13).
    Schema(Schema),
    /// Upgrade `liyasa.json` between schema versions (CLI-14).
    MigrateConfig(MigrateConfig),
    /// Theme override helpers (CLI-17).
    #[command(subcommand)]
    Theme(Theme),
    /// Replace this binary with a newer signed release (CLI-26).
    Update(Update),
    /// Print the version (CLI-26).
    Version(Version),
    /// Turn anonymous usage reporting on or off (CLI-26).
    #[command(subcommand)]
    Telemetry(Telemetry),
    /// Print a shell completion script (CLI-26).
    Completions(Completions),
    /// Report what this machine can and cannot do (CLI-27).
    Doctor(Doctor),
    /// Manage the optional browser runtime (§6.12).
    #[command(subcommand)]
    Companion(Companion),
    /// Inspect and refresh `liyasa.lock` (CLI-33).
    #[command(subcommand)]
    Lock(Lock),
    /// Run the language server an editor talks to over stdin and stdout
    /// (CLI-25).
    Lsp,
    /// Print the artifact size budgets (CLI-35).
    ///
    /// Hidden: it exists so the release job reads the table from one place
    /// rather than carrying its own copy of every number.
    #[command(hide = true)]
    Budgets,
}

#[derive(Debug, Args)]
pub struct New {
    /// Where to put the project. Defaults to the working directory.
    pub directory: Option<PathBuf>,
    /// The site name. Prompted for when absent unless `--yes`.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    /// A built-in starter name or a git URL.
    #[arg(long, value_name = "NAME|URL")]
    pub template: Option<String>,
    /// The theme preset to start from.
    #[arg(long, value_name = "PRESET")]
    pub preset: Option<String>,
    /// Include the sample OpenAPI specification.
    #[arg(long, overrides_with = "no_openapi")]
    pub openapi: bool,
    /// Leave the sample OpenAPI specification out.
    #[arg(long)]
    pub no_openapi: bool,
    /// Run `git init` in the new project.
    #[arg(long, overrides_with = "no_git")]
    pub git: bool,
    /// Do not run `git init`.
    #[arg(long)]
    pub no_git: bool,
    /// Write a continuous-integration workflow.
    #[arg(long, overrides_with = "no_ci")]
    pub ci: bool,
    /// Do not write a continuous-integration workflow.
    #[arg(long)]
    pub no_ci: bool,
    /// Take the default for every question instead of asking.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct Dev {
    #[arg(
        long,
        short,
        value_name = "PORT",
        default_value_t = 3000,
        env = "LIYASA_PORT"
    )]
    pub port: u16,
    #[arg(
        long,
        value_name = "HOST",
        default_value = "127.0.0.1",
        env = "LIYASA_HOST"
    )]
    pub host: String,
    /// Open a browser once the first render is ready.
    #[arg(long, overrides_with = "no_open", env = "LIYASA_OPEN")]
    pub open: bool,
    #[arg(long)]
    pub no_open: bool,
    /// Mock reader groups, comma separated.
    #[arg(long, value_name = "A,B", value_delimiter = ',')]
    pub groups: Vec<String>,
    /// Mock the reader's region.
    #[arg(long, value_name = "REGION")]
    pub region: Option<String>,
    /// Render this locale.
    #[arg(long, value_name = "LOCALE")]
    pub locale: Option<String>,
    /// Render this content version.
    #[arg(long = "version", value_name = "VERSION")]
    pub version_name: Option<String>,
    /// Skip the OpenAPI pages, which are the slowest part of a cold start.
    #[arg(long)]
    pub disable_openapi: bool,
    /// Do not prefetch linked pages in the reader.
    #[arg(long)]
    pub disable_prefetch: bool,
    /// Validate against the schema in this working copy rather than the
    /// published one.
    #[arg(long)]
    pub local_schema: bool,
    /// Include pages marked `draft: true`.
    #[arg(long, env = "LIYASA_DRAFTS")]
    pub drafts: bool,
    /// Re-run verification on every rebuild.
    #[arg(long)]
    pub verify: bool,
}

#[derive(Debug, Args)]
pub struct Build {
    /// Where the built site goes.
    #[arg(long, short, value_name = "DIR", env = "LIYASA_OUTPUT")]
    pub output: Option<PathBuf>,
    /// Empty the output directory and the cache first.
    #[arg(long)]
    pub clean: bool,
    /// Merge `liyasa.<env>.json` over the configuration.
    #[arg(long, value_name = "ENV", env = "LIYASA_ENV")]
    pub env: Option<String>,
    /// Serve the site from this path prefix.
    #[arg(long, value_name = "PATH", env = "LIYASA_BASE_PATH")]
    pub base_path: Option<String>,
    /// Include pages marked `draft: true`.
    #[arg(long, env = "LIYASA_DRAFTS")]
    pub drafts: bool,
    /// Treat warnings as errors.
    #[arg(long, env = "LIYASA_STRICT")]
    pub strict: bool,
    /// Print how long each phase took.
    #[arg(long)]
    pub profile: bool,
    /// Date this build from a fixed instant, so two builds of the same inputs
    /// agree (§6.6.2). A Unix timestamp or an RFC 3339 date.
    ///
    /// `SOURCE_DATE_EPOCH` wins over it, and a git commit is used when neither
    /// is given.
    #[arg(
        long,
        value_name = "WHEN",
        value_parser = crate::clock::parse,
        env = "LIYASA_BUILD_TIME"
    )]
    pub build_time: Option<i64>,
    /// Build twice and report any file that differed (E0706).
    #[arg(long)]
    pub check_determinism: bool,
    /// Fail rather than change `liyasa.lock` (CLI-33).
    #[arg(long)]
    pub locked: bool,
}

#[derive(Debug, Args)]
pub struct Validate {
    /// Run only these checks. Repeat or comma-separate.
    ///
    /// TODO(rfc-0901): CLI-04 spells the third subset `--config`, which is
    /// CLI-34's global flag for the configuration path.
    #[arg(long, value_name = "SUBSET", value_enum, value_delimiter = ',')]
    pub only: Vec<Subset>,
    /// Shorthand for `--only openapi`.
    #[arg(long)]
    pub openapi: bool,
    /// Shorthand for `--only links`.
    #[arg(long)]
    pub links: bool,
    #[arg(long, value_name = "FORMAT", value_enum, default_value_t = Format::Text)]
    pub format: Format,
    /// Treat warnings as errors.
    #[arg(long, env = "LIYASA_STRICT")]
    pub strict: bool,
    /// Also list the pages that are rendered on demand rather than written as
    /// files (§6.6.4), so they can be kept few.
    #[arg(long)]
    pub personalization: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Subset {
    Config,
    Frontmatter,
    Content,
    Components,
    Links,
    Openapi,
    Navigation,
}

/// `liyasa format`. Named with a trailing underscore because [`Format`] is the
/// output-format enum; the command is still spelled `format`.
#[derive(Debug, Args)]
#[command(name = "format")]
pub struct Format_ {
    /// Only these files. Defaults to the whole project.
    pub paths: Vec<PathBuf>,
    /// Report what is unformatted and change nothing.
    #[arg(long)]
    pub check: bool,
    /// Rewrite tag-form components into directive form.
    #[arg(long)]
    pub directives: bool,
}

#[derive(Debug, Args)]
pub struct Verify {
    /// Run only these check classes.
    #[arg(long, value_name = "CLASS", value_enum, value_delimiter = ',')]
    pub only: Vec<CheckClass>,
    /// Re-read every truth source before checking.
    #[arg(long)]
    pub refresh: bool,
    /// Ignore cached check results.
    #[arg(long)]
    pub no_cache: bool,
    /// Only pages that changed since this git reference.
    #[arg(long, value_name = "REF")]
    pub changed: Option<String>,
    #[arg(long, value_name = "FORMAT", value_enum, default_value_t = Format::Text)]
    pub format: Format,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CheckClass {
    Code,
    Facts,
    Links,
    Screenshots,
    Prose,
}

#[derive(Debug, Args)]
pub struct BrokenLinks {
    /// How many requests to have in flight at once.
    #[arg(long, value_name = "N", default_value_t = 8)]
    pub concurrency: usize,
    /// How long to wait for one response, in seconds.
    #[arg(long, value_name = "SECONDS", default_value_t = 10)]
    pub timeout: u64,
    /// A URL or host to accept without checking. Repeatable.
    #[arg(long, value_name = "URL|HOST")]
    pub allow: Vec<String>,
    /// Check only links inside the site.
    #[arg(long)]
    pub internal_only: bool,
    #[arg(long, value_name = "FORMAT", value_enum, default_value_t = Format::Text)]
    pub format: Format,
}

#[derive(Debug, Args)]
pub struct Test {
    /// Accessibility checks (RX-91).
    #[arg(long)]
    pub a11y: bool,
    /// Lighthouse budgets. Needs the companion runtime.
    #[arg(long)]
    pub perf: bool,
    /// The §25 agent-readiness checks against the built output.
    #[arg(long)]
    pub agents: bool,
    /// The search assertions in `tests/search.toml`.
    #[arg(long)]
    pub search: bool,
    /// The built site to test. Defaults to the configured output directory.
    #[arg(long, value_name = "DIR", env = "LIYASA_OUTPUT")]
    pub output: Option<PathBuf>,
    /// Score exactly these pages instead of sampling the site (§25). A route
    /// (`/guide/install`) or an absolute URL on this site's origin. Repeat the
    /// flag or separate with commas.
    ///
    /// Explicitly selected pages are scored as given regardless of how few
    /// there are, where a sample of under five is not.
    #[arg(long, value_name = "URL", value_delimiter = ',')]
    pub urls: Vec<String>,
    #[arg(long, value_name = "FORMAT", value_enum, default_value_t = Format::Text)]
    pub format: Format,
}

#[derive(Debug, Args)]
pub struct Score {
    /// The built site to score. Defaults to the configured output directory.
    #[arg(long, value_name = "DIR", env = "LIYASA_OUTPUT")]
    pub output: Option<PathBuf>,
    #[arg(long, value_name = "FORMAT", value_enum, default_value_t = Format::Text)]
    pub format: Format,
}

#[derive(Debug, Args)]
pub struct Export {
    /// Where the export goes.
    #[arg(long, short, value_name = "DIR", env = "LIYASA_OUTPUT")]
    pub output: Option<PathBuf>,
    /// The static site. The default.
    #[arg(long)]
    pub r#static: bool,
    /// Rewrite every asset reference so the export works from a file:// URL.
    #[arg(long)]
    pub offline: bool,
    /// One PDF of the whole site (RX-80). Needs the companion runtime.
    #[arg(long)]
    pub pdf: bool,
    /// Only the Markdown twins and `llms.txt`.
    #[arg(long)]
    pub markdown: bool,
    /// Wrap the export in a zip archive.
    #[arg(long)]
    pub zip: bool,
}

#[derive(Debug, Args)]
pub struct Serve {
    #[arg(
        long,
        value_name = "ADDR",
        default_value = "0.0.0.0:8080",
        env = "LIYASA_LISTEN"
    )]
    pub listen: String,
    #[arg(long, value_name = "URL", env = "LIYASA_DB")]
    pub db: Option<String>,
    #[arg(long, value_name = "URL", env = "LIYASA_STORAGE")]
    pub storage: Option<String>,
    /// Terminate TLS for these domains with automatic certificates.
    #[arg(long, value_name = "DOMAIN")]
    pub tls: Vec<String>,
    /// Run first-time setup and print the one-time admin token.
    #[arg(long)]
    pub init: bool,
    /// Serve only the analytics ingest endpoint (ANA-09).
    #[arg(long)]
    pub collector_only: bool,
}

#[derive(Debug, Args)]
pub struct Search {
    /// What to search for.
    pub query: String,
    /// The search index directory. Defaults to the built site's.
    #[arg(long, value_name = "DIR")]
    pub index: Option<PathBuf>,
    #[arg(long, value_name = "N", default_value_t = 10)]
    pub limit: usize,
    #[arg(long, value_name = "LOCALE")]
    pub locale: Option<String>,
    #[arg(long = "version", value_name = "VERSION")]
    pub version_name: Option<String>,
    #[arg(long, value_name = "TAB")]
    pub tab: Option<String>,
}

#[derive(Debug, Args)]
pub struct Schema {
    /// Which schema to print. Prints the list when absent.
    pub which: Option<String>,
}

#[derive(Debug, Args)]
pub struct MigrateConfig {
    /// Write the upgraded configuration back. Without it the result is printed.
    #[arg(long)]
    pub write: bool,
}

#[derive(Debug, Subcommand)]
pub enum Theme {
    /// Copy a default partial into `theme/partials/` for editing (THM-23).
    Eject(ThemeEject),
    /// Show what an override changed relative to the current default.
    Diff(ThemeDiff),
    /// Print the resolved design tokens.
    Tokens(ThemeTokens),
}

#[derive(Debug, Args)]
pub struct ThemeEject {
    /// The partial to copy. Prints the list when absent.
    pub partial: Option<String>,
}

#[derive(Debug, Args)]
pub struct ThemeDiff {
    /// Only this partial.
    pub partial: Option<String>,
}

#[derive(Debug, Args)]
pub struct ThemeTokens {}

#[derive(Debug, Args)]
pub struct Update {
    /// The release index to read. A directory or a `file://` URL today.
    #[arg(long, value_name = "SOURCE", env = "LIYASA_UPDATE_INDEX")]
    pub index: Option<String>,
    /// Report what is available and replace nothing.
    #[arg(long)]
    pub check: bool,
    /// Install this version rather than the newest.
    #[arg(long = "version", value_name = "VERSION")]
    pub version_name: Option<String>,
}

#[derive(Debug, Args)]
pub struct Version {}

#[derive(Debug, Subcommand)]
pub enum Telemetry {
    /// Start reporting anonymous usage.
    On,
    /// Stop reporting anonymous usage.
    Off,
    /// Say whether reporting is on. The default is off.
    Status,
}

#[derive(Debug, Args)]
pub struct Completions {
    /// The shell to generate for.
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}

#[derive(Debug, Args)]
pub struct Doctor {}

#[derive(Debug, Subcommand)]
pub enum Companion {
    /// Download and verify the pinned browser runtime.
    Install(CompanionInstall),
    /// Say whether the runtime is installed and which version.
    Status,
    /// Delete the installed runtime.
    Remove,
}

#[derive(Debug, Args)]
pub struct CompanionInstall {
    /// The archive to install. A directory or a `file://` URL today.
    #[arg(long, value_name = "SOURCE", env = "LIYASA_COMPANION_SOURCE")]
    pub source: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum Lock {
    /// Refresh `liyasa.lock` from the project as it is now.
    Update,
    /// Report what would change without writing (the `--locked` predicate).
    Check,
}
