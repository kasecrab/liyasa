//! Where the CLI keeps state that belongs to the person rather than to a
//! project: the telemetry choice (CLI-26) and the companion runtime (§6.12).
//!
//! `LIYASA_HOME` overrides everything, which is what a test and an air-gapped
//! installation both need. Otherwise the XDG variables, then the conventional
//! directories under `$HOME`.
//!
//! TODO(rfc-0900): `directories` would give the platform-correct path on macOS
//! and Windows. It is not in the PRD's table and one more dependency for two
//! paths is not worth a row, so the XDG layout is used everywhere; on Windows
//! that means `%USERPROFILE%\.config\liyasa`, which is wrong but writable.

use std::path::PathBuf;

pub const TELEMETRY_FILE: &str = "telemetry";
pub const COMPANION_DIR: &str = "companion";

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn under(explicit: &str, xdg: &str, fallback: &str) -> PathBuf {
    if let Some(root) = std::env::var_os(explicit) {
        return PathBuf::from(root);
    }
    if let Some(root) = std::env::var_os("LIYASA_HOME") {
        return PathBuf::from(root).join(fallback.rsplit('/').next().unwrap_or(fallback));
    }
    if let Some(root) = std::env::var_os(xdg) {
        return PathBuf::from(root).join("liyasa");
    }
    home().map_or_else(|| PathBuf::from(fallback), |home| home.join(fallback))
}

/// Where the telemetry choice and any future preference live.
pub fn config_dir() -> PathBuf {
    under("LIYASA_CONFIG_HOME", "XDG_CONFIG_HOME", ".config/liyasa")
}

/// Where downloaded artifacts live: the companion runtime, update downloads.
pub fn cache_dir() -> PathBuf {
    under("LIYASA_CACHE_HOME", "XDG_CACHE_HOME", ".cache/liyasa")
}

/// Whether anonymous usage reporting is on. CLI-26: off unless turned on, and
/// the environment can force it either way for a CI run.
pub fn telemetry_enabled() -> bool {
    if let Some(value) = std::env::var_os("LIYASA_TELEMETRY") {
        return matches!(
            value.to_string_lossy().trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "on" | "yes"
        );
    }
    std::fs::read_to_string(config_dir().join(TELEMETRY_FILE)).is_ok_and(|text| text.trim() == "on")
}

/// Records the choice. Returns the path written so the command can name it.
pub fn set_telemetry(on: bool) -> std::io::Result<PathBuf> {
    let directory = config_dir();
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(TELEMETRY_FILE);
    std::fs::write(&path, if on { "on\n" } else { "off\n" })?;
    Ok(path)
}

/// Where `liyasa companion install` puts the browser runtime.
pub fn companion_dir() -> PathBuf {
    cache_dir().join(COMPANION_DIR)
}

/// The installed companion runtime's version, if one is installed.
pub fn companion_version() -> Option<String> {
    std::fs::read_to_string(companion_dir().join("version"))
        .ok()
        .map(|text| text.trim().to_owned())
}
