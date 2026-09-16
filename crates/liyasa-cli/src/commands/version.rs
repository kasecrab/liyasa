//! CLI-26: `liyasa version`.

use crate::Exit;
use crate::cli::{Global, Version};

/// What this binary is, for a human and for a machine.
pub const NAME: &str = "liyasa";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The feature rows CLI-32 names, in the order they are documented.
pub fn features() -> Vec<&'static str> {
    let mut out = Vec::new();
    if cfg!(feature = "build") {
        out.push("build");
    }
    if cfg!(feature = "verify") {
        out.push("verify");
    }
    if cfg!(feature = "server") {
        out.push("server");
    }
    if cfg!(feature = "ai") {
        out.push("ai");
    }
    out
}

/// The interface versions a caller may need to match against: the config schema
/// this binary reads and the agent-readiness check set it grades with.
pub fn interfaces() -> [(&'static str, String); 2] {
    [
        (
            "configSchema",
            liyasa_config::schema::CONFIG_SCHEMA_VERSION.to_string(),
        ),
        (
            "agentSpec",
            liyasa_build::agents::spec::SPEC_VERSION.to_owned(),
        ),
    ]
}

pub fn run(global: &Global, _args: &Version) -> Exit {
    if global.json {
        let interfaces: serde_json::Map<String, serde_json::Value> = interfaces()
            .into_iter()
            .map(|(key, value)| (key.to_owned(), serde_json::Value::String(value)))
            .collect();
        let document = serde_json::json!({
            "name": NAME,
            "version": VERSION,
            "features": features(),
            "interfaces": interfaces,
        });
        println!("{document:#}");
    } else {
        println!("{NAME} {VERSION}");
        println!("features: {}", features().join(", "));
        for (name, value) in interfaces() {
            println!("{name}: {value}");
        }
    }
    Exit::Success
}
