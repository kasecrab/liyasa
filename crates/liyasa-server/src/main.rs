//! `liyasa-server`: the serving half of `liyasa serve` (CLI-11, HOST-02).
//!
//! The `liyasa` binary calls into this crate; this entry point exists so the
//! server can be run, packaged, and tested on its own, which is what the
//! systemd unit and the container image do.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use liyasa_core::ids::JobId;
use liyasa_core::store::{JobQuery, Page};
use liyasa_server::routes::{self, AppState, ServerConfig};
use liyasa_store::{IngestQueue, MasterKey, SqliteStore};

const USAGE: &str = "\
liyasa-server — serve a built documentation site

USAGE:
    liyasa-server [serve] [OPTIONS]
    liyasa-server jobs list [--name <name>] [--state <state>]
    liyasa-server jobs retry <id>
    liyasa-server jobs cancel <id>

OPTIONS:
    --config <dir>        project directory holding liyasa.json (default: .)
    --dist <dir>          the built bundle to serve (default: <config>/dist)
    --db <file>           SQLite application database (default: <config>/liyasa.db)
    --storage <dir>       where analytics.db and segment files live (default: beside --db)
    --listen <addr>       address to bind (default: 127.0.0.1:8080)
    --env <name>          config overlay and the analytics env (default: production)
    --tls-cert <file>     PEM certificate chain; --tls-key must be given too
    --tls-key <file>      PEM private key
    --collector-only      accept analytics events and serve no site (ANA-09)
    --origin <origin>     an origin the collector accepts events for; repeatable
    --offline             make no outbound request of any kind (HOST-08)
    --json-logs           write logs as JSON lines (the default under systemd)
    -h, --help            print this
";

#[derive(Debug, Default)]
struct Options {
    config: Option<PathBuf>,
    dist: Option<PathBuf>,
    db: Option<PathBuf>,
    storage: Option<PathBuf>,
    listen: Option<String>,
    env: Option<String>,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
    collector_only: bool,
    origins: Vec<String>,
    offline: bool,
    json_logs: bool,
}

enum Command {
    Serve(Options),
    JobsList {
        options: Options,
        name: Option<String>,
        state: Option<String>,
    },
    JobsRetry(Options, String),
    JobsCancel(Options, String),
    Help,
}

fn parse(args: &[String]) -> Result<Command, String> {
    let mut options = Options::default();
    let mut positional: Vec<String> = Vec::new();
    let mut name = None;
    let mut state = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        let value = |options_index: &mut usize| -> Result<String, String> {
            *options_index += 1;
            args.get(*options_index)
                .cloned()
                .ok_or_else(|| format!("`{arg}` needs a value"))
        };
        match arg {
            "-h" | "--help" => return Ok(Command::Help),
            "--config" => options.config = Some(value(&mut index)?.into()),
            "--dist" => options.dist = Some(value(&mut index)?.into()),
            "--db" => options.db = Some(value(&mut index)?.into()),
            "--storage" => options.storage = Some(value(&mut index)?.into()),
            "--listen" => options.listen = Some(value(&mut index)?),
            "--env" => options.env = Some(value(&mut index)?),
            "--tls-cert" => options.tls_cert = Some(value(&mut index)?.into()),
            "--tls-key" => options.tls_key = Some(value(&mut index)?.into()),
            "--origin" => options.origins.push(value(&mut index)?),
            "--name" => name = Some(value(&mut index)?),
            "--state" => state = Some(value(&mut index)?),
            "--collector-only" => options.collector_only = true,
            "--offline" => options.offline = true,
            "--json-logs" => options.json_logs = true,
            other if other.starts_with('-') => return Err(format!("unknown option `{other}`")),
            other => positional.push(other.to_owned()),
        }
        index += 1;
    }

    match positional
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] | ["serve"] => Ok(Command::Serve(options)),
        ["jobs", "list"] => Ok(Command::JobsList {
            options,
            name,
            state,
        }),
        ["jobs", "retry", id] => Ok(Command::JobsRetry(options, (*id).to_owned())),
        ["jobs", "cancel", id] => Ok(Command::JobsCancel(options, (*id).to_owned())),
        other => Err(format!("unknown command `{}`", other.join(" "))),
    }
}

impl Options {
    fn root(&self) -> PathBuf {
        self.config.clone().unwrap_or_else(|| PathBuf::from("."))
    }

    fn dist(&self) -> PathBuf {
        self.dist
            .clone()
            .unwrap_or_else(|| self.root().join("dist"))
    }

    fn db(&self) -> PathBuf {
        self.db
            .clone()
            .unwrap_or_else(|| self.root().join("liyasa.db"))
    }

    fn storage(&self) -> PathBuf {
        self.storage.clone().unwrap_or_else(|| {
            self.db()
                .parent()
                .map(Path::to_owned)
                .unwrap_or_else(|| PathBuf::from("."))
        })
    }
}

use std::path::Path;

/// The master key for the secret store. An instance without one generates a
/// key and says so: the secrets it writes are then readable only by this
/// instance, which is right for a first run and wrong for a restart, so the
/// message names the variable to set.
fn master_key() -> Result<MasterKey, String> {
    match std::env::var("LIYASA_MASTER_KEY") {
        Ok(hex) => MasterKey::from_hex(&hex).map_err(|e| e.to_string()),
        Err(_) => {
            let key = MasterKey::generate().map_err(|e| e.to_string())?;
            tracing::warn!(
                target: "liyasa_server",
                "no LIYASA_MASTER_KEY: generated an ephemeral key, so stored secrets will not \
                 survive a restart"
            );
            Ok(key)
        }
    }
}

/// Reads `server`, `analytics`, and `network` out of `liyasa.json`.
fn server_config(
    options: &Options,
) -> (
    ServerConfig,
    routes::client_ip::TrustedProxies,
    Vec<(
        liyasa_core::server::RateLimitPool,
        liyasa_core::server::RateLimit,
    )>,
) {
    use liyasa_core::server::{RateLimit, RateLimitPool};

    let vfs = liyasa_config::vfs::OsVfs::new(options.root());
    let mut sources = liyasa_core::source_map::SourceMap::new();
    let load = liyasa_config::load(
        &vfs,
        &mut sources,
        &liyasa_config::Options {
            root: liyasa_core::vfs::VfsPath::new(""),
            env: options.env.clone(),
        },
    );
    let value = load.value;
    let server = value.get("server");
    let analytics = value.get("analytics");

    let string = |parent: Option<&serde_json::Value>, key: &str| -> Option<String> {
        parent?.get(key)?.as_str().map(str::to_owned)
    };
    let integer = |parent: Option<&serde_json::Value>, key: &str| -> Option<u32> {
        parent?.get(key)?.as_u64().map(|n| n as u32)
    };

    let trusted: Vec<String> = server
        .and_then(|s| s.get("trustedProxies"))
        .and_then(|v| v.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| e.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();

    let mut limits = Vec::new();
    if let Some(rate_limits) = server.and_then(|s| s.get("rateLimits")) {
        for (key, pool) in [
            ("pages", RateLimitPool::Pages),
            ("agentPages", RateLimitPool::AgentPages),
            ("search", RateLimitPool::Search),
            ("assistant", RateLimitPool::Assistant),
            ("feedback", RateLimitPool::Feedback),
            ("proxy", RateLimitPool::PlaygroundProxy),
            ("rest", RateLimitPool::Rest),
            ("mcp", RateLimitPool::Mcp),
            ("auth", RateLimitPool::Auth),
        ] {
            if let Some(per_minute) = integer(Some(rate_limits), key) {
                let default = routes::limiter::default_limit(pool);
                limits.push((
                    pool,
                    RateLimit {
                        per_minute,
                        // The configured value is the minute budget; the burst
                        // keeps its documented ratio to it.
                        burst: (per_minute / 5).max(1),
                        daily: default.daily,
                    },
                ));
            }
        }
    }

    let config = ServerConfig {
        site: string(Some(&value), "name")
            .or_else(|| {
                options
                    .root()
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "liyasa".to_owned()),
        env: options
            .env
            .clone()
            .unwrap_or_else(|| "production".to_owned()),
        offline: options.offline
            || server
                .and_then(|s| s.get("offline"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        drain_timeout: string(server, "drainTimeout")
            .and_then(|text| parse_duration(&text))
            .unwrap_or(Duration::from_secs(30)),
        collector_only: options.collector_only,
        collector_origins: options.origins.clone(),
        analytics_enabled: analytics
            .and_then(|a| a.get("enabled"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        region_header: None,
        jobs_lease: Duration::from_secs(
            server
                .and_then(|s| s.get("jobs"))
                .and_then(|j| j.get("leaseSeconds"))
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(60),
        ),
    };
    (
        config,
        routes::client_ip::TrustedProxies::new(&trusted),
        limits,
    )
}

/// `30s`, `500ms`, `2h` — the duration spelling the schema uses.
fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim();
    let split = text.find(|c: char| c.is_ascii_alphabetic())?;
    let (number, unit) = text.split_at(split);
    let number: u64 = number.parse().ok()?;
    Some(match unit {
        "ms" => Duration::from_millis(number),
        "s" => Duration::from_secs(number),
        "m" => Duration::from_secs(number * 60),
        "h" => Duration::from_secs(number * 3600),
        "d" => Duration::from_secs(number * 86_400),
        _ => return None,
    })
}

async fn open_store(options: &Options, ingest: IngestQueue) -> Result<Arc<SqliteStore>, String> {
    let key = master_key()?;
    SqliteStore::open(&options.db(), key, ingest)
        .await
        .map(Arc::new)
        .map_err(|e| format!("opening {}: {e}", options.db().display()))
}

async fn run_serve(options: Options) -> Result<(), String> {
    routes::telemetry::init_logging(options.json_logs);
    let (config, proxies, limits) = server_config(&options);
    let offline = config.offline;
    let collector_only = config.collector_only;

    let ingest = IngestQueue::new(100_000, 5_000);
    let store = open_store(&options, ingest.clone()).await?;

    let mut state = AppState::new(config)
        .with_proxies(proxies)
        .with_ingest(ingest.clone());
    for (pool, limit) in limits {
        use liyasa_core::server::RateLimiter as _;
        state.limiter.configure(pool, limit);
    }
    // Every secret the store knows is redacted wherever it appears (§30.2.4).
    let scrubber = liyasa_verify::core::scrub::Scrubber::with_secrets(
        store.secrets_typed().names().iter().filter_map(|name| {
            use liyasa_core::verify::SecretSource as _;
            store
                .secrets_typed()
                .get(name)
                .map(|v| v.as_str().to_owned())
        }),
    );
    state = state.with_scrubber(scrubber);

    if !collector_only {
        match routes::bundle::Bundle::open(&options.dist()) {
            Ok(bundle) => state = state.with_bundle(Arc::new(bundle)),
            Err(error) => {
                return Err(format!(
                    "reading the bundle at {}: {error}. Run `liyasa build` first, or pass --dist.",
                    options.dist().display()
                ));
            }
        }
    }
    let state = Arc::new(state.with_store(store));

    let mut runtime = routes::serve::Runtime::new(state.clone());
    runtime
        .spawn_ingest_at(
            &options.storage().join("analytics.db"),
            liyasa_store::IngestOptions {
                raw_sink: liyasa_store::RawSink::Auto(options.storage().join("segments")),
                ..liyasa_store::IngestOptions::default()
            },
        )
        .await
        .map_err(|e| format!("opening the analytics database: {e}"))?;
    runtime.spawn_salt_rotation();
    if !offline {
        match liyasa_net::Client::new(liyasa_net::ClientOptions::default()) {
            Ok(client) => runtime.spawn_webhooks(Arc::new(client)),
            Err(error) => tracing::warn!(
                target: "liyasa_server",
                %error,
                "no outbound client: webhooks will not be delivered"
            ),
        }
    }

    let listen = options
        .listen
        .clone()
        .unwrap_or_else(|| "127.0.0.1:8080".to_owned());
    let listener = tokio::net::TcpListener::bind(&listen)
        .await
        .map_err(|e| format!("binding {listen}: {e}"))?;
    let bound = listener
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or(listen);
    let router = routes::router(state.clone());

    let stop = runtime.shutdown_signal();

    match (&options.tls_cert, &options.tls_key) {
        (Some(cert), Some(key)) => {
            let tls = routes::tls::load_config(cert, key).map_err(|e| e.to_string())?;
            tracing::info!(target: "liyasa_server", address = %bound, tls = true, "listening");
            spawn_signal(&runtime);
            routes::tls::serve(listener, router, Arc::new(tls), state.clone(), stop)
                .await
                .map_err(|e| e.to_string())?;
            runtime.drain().await;
            Ok(())
        }
        (None, None) => {
            tracing::info!(target: "liyasa_server", address = %bound, tls = false, "listening");
            spawn_signal(&runtime);
            runtime
                .serve(listener, router)
                .await
                .map_err(|e| e.to_string())
        }
        _ => Err("--tls-cert and --tls-key are given together".to_owned()),
    }
}

/// Turns the first `SIGTERM` or `SIGINT` into a drain.
fn spawn_signal(runtime: &routes::serve::Runtime) {
    let tx = runtime.stopper();
    tokio::spawn(async move {
        routes::serve::shutdown_signal().await;
        tracing::info!(target: "liyasa_server", "draining");
        tx.stop();
    });
}

/// The `--state` filter, as the job table spells it.
fn parse_state(text: &str) -> Result<liyasa_core::store::JobState, String> {
    use liyasa_core::store::JobState;
    Ok(match text {
        "queued" => JobState::Queued,
        "leased" => JobState::Leased,
        "done" => JobState::Done,
        "failed" => JobState::Failed,
        "dead" => JobState::Dead,
        other => {
            return Err(format!(
                "`{other}` is not a job state; one of queued, leased, done, failed, dead"
            ));
        }
    })
}

async fn run_jobs(
    options: Options,
    action: &str,
    argument: Option<String>,
    state_filter: Option<String>,
) -> Result<(), String> {
    let store = open_store(&options, IngestQueue::new(1024, 256)).await?;
    let jobs = store.jobs_typed();
    match action {
        "list" => {
            let query = JobQuery {
                name: argument,
                state: state_filter.as_deref().map(parse_state).transpose()?,
                ..JobQuery::default()
            };
            let rows = jobs
                .list(
                    &query,
                    Page {
                        cursor: None,
                        limit: 100,
                    },
                )
                .await
                .map_err(|e| e.to_string())?;
            if rows.is_empty() {
                println!("no jobs");
            }
            for job in rows {
                println!(
                    "{}  {:<24} {:<8} attempt {}/{}  {}",
                    job.id,
                    job.name,
                    routes::jobs::state_text(job.state),
                    job.attempts,
                    job.max_attempts,
                    job.error.as_deref().unwrap_or("")
                );
            }
            Ok(())
        }
        "retry" | "cancel" => {
            let id = argument.ok_or("an id is required")?;
            let job_id = JobId::parse(&id).ok_or_else(|| format!("`{id}` is not a ULID"))?;
            let result = if action == "retry" {
                jobs.retry(&job_id).await
            } else {
                jobs.cancel(&job_id).await
            };
            match result {
                Ok(()) => {
                    println!("{id} {action}");
                    Ok(())
                }
                Err(liyasa_core::store::StoreError::NotFound) => {
                    Err(format!("no job `{id}` is in a state `{action}` applies to"))
                }
                Err(error) => Err(error.to_string()),
            }
        }
        other => Err(format!("unknown jobs action `{other}`")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match parse(&args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("liyasa-server: {error}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if matches!(command, Command::Help) {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("liyasa-server: starting the runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let result = runtime.block_on(async move {
        match command {
            Command::Serve(options) => run_serve(options).await,
            Command::JobsList {
                options,
                name,
                state,
            } => run_jobs(options, "list", name, state).await,
            Command::JobsRetry(options, id) => run_jobs(options, "retry", Some(id), None).await,
            Command::JobsCancel(options, id) => run_jobs(options, "cancel", Some(id), None).await,
            Command::Help => Ok(()),
        }
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("liyasa-server: {error}");
            ExitCode::FAILURE
        }
    }
}
