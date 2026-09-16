//! CLI-33: `liyasa.lock`, in the shape §34.12 fixes.
//!
//! Every entry is content-addressed, because the point of the file is that two
//! machines given the same lock produce the same site. What the lock records
//! today is what this release can address: the Liyasa version, the theme
//! preset, the bundled font files by digest, and the companion runtime. Runner
//! images and component packs get their tables when the crates that install
//! them exist, and their absence is a missing row rather than a wrong one.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The lock format version. The CLI refuses a file whose major is newer than
/// this, because a key it does not understand may be the one that matters.
pub const FORMAT_VERSION: u32 = 1;

pub const LOCK_FILE: &str = "liyasa.lock";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    pub version: u32,
    pub liyasa: Tool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<Component>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runners: Vec<Runner>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fonts: BTreeMap<String, Font>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub companion: Option<Companion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tool {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub preset: String,
    /// `builtin`, or a git URL for a community preset.
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    pub source: String,
    pub rev: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Runner {
    pub id: String,
    pub image: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Font {
    pub source: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Companion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chromium: Option<Pinned>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pinned {
    pub version: String,
    #[serde(default)]
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Not TOML, or not a lock (E0005).
    Unreadable(String),
    /// Written by a newer Liyasa than this one (E0005).
    TooNew { found: u32, understood: u32 },
}

impl Failure {
    pub fn code(&self) -> liyasa_core::diagnostics::Code {
        liyasa_core::diagnostics::code::E0005
    }

    pub fn message(&self) -> String {
        match self {
            Self::Unreadable(detail) => format!("`{LOCK_FILE}` could not be read: {detail}"),
            Self::TooNew { found, understood } => format!(
                "`{LOCK_FILE}` is format version {found}; this Liyasa understands {understood}"
            ),
        }
    }
}

/// What the project as it stands now would lock to.
pub fn compute(config: &serde_json::Value) -> Lock {
    let preset = config
        .pointer("/theme/preset")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(crate::scaffold::DEFAULT_PRESET)
        .to_owned();

    let mut fonts = BTreeMap::new();
    for face in liyasa_theme::fonts::bundled() {
        if let Some(bytes) = liyasa_theme::fonts::file(&face.file) {
            fonts.insert(
                face.family.clone(),
                Font {
                    source: "bundled".to_owned(),
                    sha256: sha256(bytes),
                },
            );
        }
    }

    Lock {
        version: FORMAT_VERSION,
        liyasa: Tool {
            version: crate::commands::version::VERSION.to_owned(),
        },
        theme: Some(Theme {
            preset,
            source: "builtin".to_owned(),
        }),
        components: Vec::new(),
        runners: Vec::new(),
        fonts,
        companion: crate::home::companion_version().map(|version| Companion {
            chromium: Some(Pinned {
                version,
                sha256: String::new(),
            }),
        }),
    }
}

pub fn read(path: &Path) -> Result<Option<Lock>, Failure> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Failure::Unreadable(error.to_string())),
    };

    // The version is read before the rest, so a newer file is reported as too
    // new rather than as a pile of unknown keys.
    let value: toml::Value =
        toml::from_str(&text).map_err(|error| Failure::Unreadable(error.to_string()))?;
    let found = value
        .get("version")
        .and_then(toml::Value::as_integer)
        .and_then(|found| u32::try_from(found).ok())
        .unwrap_or(FORMAT_VERSION);
    if found > FORMAT_VERSION {
        return Err(Failure::TooNew {
            found,
            understood: FORMAT_VERSION,
        });
    }

    toml::from_str(&text)
        .map(Some)
        .map_err(|error| Failure::Unreadable(error.to_string()))
}

pub fn write(path: &Path, lock: &Lock) -> std::io::Result<()> {
    std::fs::write(path, render(lock))
}

pub fn render(lock: &Lock) -> String {
    let mut text = toml::to_string_pretty(lock).unwrap_or_default();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    format!("# Written by Liyasa. Do not edit; run `liyasa lock update`.\n{text}")
}

/// What would change, in the terms a person can act on. Empty means the lock
/// on disk already describes this project, which is what `--locked` requires.
pub fn changes(current: Option<&Lock>, fresh: &Lock) -> Vec<String> {
    let Some(current) = current else {
        return vec![format!("`{LOCK_FILE}` does not exist yet")];
    };
    let mut out = Vec::new();

    if current.liyasa.version != fresh.liyasa.version {
        out.push(format!(
            "liyasa {} -> {}",
            current.liyasa.version, fresh.liyasa.version
        ));
    }
    match (&current.theme, &fresh.theme) {
        (Some(was), Some(now)) if was.preset != now.preset => {
            out.push(format!("theme preset {} -> {}", was.preset, now.preset));
        }
        (None, Some(now)) => out.push(format!("theme preset {} added", now.preset)),
        (Some(was), None) => out.push(format!("theme preset {} removed", was.preset)),
        _ => {}
    }
    for (family, font) in &fresh.fonts {
        match current.fonts.get(family) {
            None => out.push(format!("font {family} added")),
            Some(was) if was.sha256 != font.sha256 => {
                out.push(format!("font {family} changed"));
            }
            Some(_) => {}
        }
    }
    for family in current.fonts.keys() {
        if !fresh.fonts.contains_key(family) {
            out.push(format!("font {family} removed"));
        }
    }
    let was = current
        .companion
        .as_ref()
        .and_then(|c| c.chromium.as_ref())
        .map(|pin| pin.version.clone());
    let now = fresh
        .companion
        .as_ref()
        .and_then(|c| c.chromium.as_ref())
        .map(|pin| pin.version.clone());
    if was != now {
        out.push(format!(
            "companion runtime {} -> {}",
            was.unwrap_or_else(|| "none".to_owned()),
            now.unwrap_or_else(|| "none".to_owned())
        ));
    }
    out
}

fn sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> serde_json::Value {
        serde_json::json!({ "name": "Acme", "theme": { "preset": "ember" } })
    }

    #[test]
    fn a_computed_lock_round_trips_through_toml() {
        let lock = compute(&config());
        let text = render(&lock);
        let parsed: Lock = toml::from_str(&text).expect("the rendered lock parses");
        assert_eq!(parsed, lock);
    }

    #[test]
    fn the_lock_records_the_preset_and_the_bundled_fonts() {
        let lock = compute(&config());
        assert_eq!(
            lock.theme.as_ref().map(|t| t.preset.as_str()),
            Some("ember")
        );
        assert_eq!(lock.version, FORMAT_VERSION);
        assert!(!lock.fonts.is_empty(), "no fonts were recorded");
        for font in lock.fonts.values() {
            assert_eq!(font.sha256.len(), 64, "not a sha256: {}", font.sha256);
        }
    }

    #[test]
    fn a_lock_that_matches_has_no_changes() {
        let lock = compute(&config());
        assert!(changes(Some(&lock), &lock).is_empty());
    }

    #[test]
    fn a_changed_preset_is_a_change() {
        let before = compute(&config());
        let after = compute(&serde_json::json!({ "theme": { "preset": "slate" } }));
        let found = changes(Some(&before), &after);
        assert_eq!(found, vec!["theme preset ember -> slate".to_owned()]);
    }

    #[test]
    fn no_lock_at_all_is_a_change() {
        let fresh = compute(&config());
        assert_eq!(changes(None, &fresh).len(), 1);
    }

    #[test]
    fn a_newer_format_is_refused_rather_than_misread() {
        let directory = std::env::temp_dir().join(format!("liyasa-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory");
        let path = directory.join(LOCK_FILE);
        std::fs::write(&path, "version = 99\n[liyasa]\nversion = \"9.9.9\"\n").expect("the lock");

        assert_eq!(
            read(&path),
            Err(Failure::TooNew {
                found: 99,
                understood: FORMAT_VERSION
            })
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_missing_lock_is_not_an_error() {
        assert_eq!(read(Path::new("/nonexistent/liyasa.lock")), Ok(None));
    }

    #[test]
    fn a_lock_that_is_not_toml_is_reported() {
        let directory =
            std::env::temp_dir().join(format!("liyasa-lock-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory");
        let path = directory.join(LOCK_FILE);
        std::fs::write(&path, "this is not toml {{{").expect("the lock");

        assert!(matches!(read(&path), Err(Failure::Unreadable(_))));
        let _ = std::fs::remove_dir_all(&directory);
    }
}
