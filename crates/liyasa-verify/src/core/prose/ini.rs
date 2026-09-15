//! `.vale.ini` (VER-61).
//!
//! A small INI: global keys, then one section per file glob. Only the keys
//! that change what runs are read; the rest are kept so a round trip does not
//! lose them.

use std::collections::BTreeMap;

use super::rule::Level;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValeIni {
    /// Where rule packages live, relative to the config file.
    pub styles_path: String,
    /// Findings below this level are not reported.
    pub min_alert_level: Level,
    /// Project vocabularies, which become accept and reject lists.
    pub vocab: Vec<String>,
    pub sections: Vec<Section>,
    /// Keys read but not acted on, kept so nothing is silently dropped.
    pub other: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Section {
    /// The glob the section header carried, such as `*.md`.
    pub glob: String,
    pub based_on_styles: Vec<String>,
    /// `Style.Rule = NO` turns one rule off; `= error` changes its level.
    pub overrides: BTreeMap<String, Override>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Override {
    Off,
    Level(Level),
}

/// Vale's own defaults: rules live under `styles/`, and nothing is filtered
/// out — `MinAlertLevel` starts at `suggestion`, not at the level a rule that
/// names none gets.
impl Default for ValeIni {
    fn default() -> Self {
        Self {
            styles_path: "styles".to_owned(),
            min_alert_level: Level::Suggestion,
            vocab: Vec::new(),
            sections: Vec::new(),
            other: BTreeMap::new(),
        }
    }
}

impl ValeIni {
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        let mut section: Option<Section> = None;

        for line in text.lines() {
            let line = strip_comment(line).trim();
            if line.is_empty() {
                continue;
            }
            if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                if let Some(done) = section.take() {
                    out.sections.push(done);
                }
                section = Some(Section {
                    glob: header.trim().to_owned(),
                    ..Section::default()
                });
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            match &mut section {
                Some(section) => read_section_key(section, key, value),
                None => read_global_key(&mut out, key, value),
            }
        }
        if let Some(done) = section {
            out.sections.push(done);
        }
        out
    }

    /// The styles a file's sections turn on, in declaration order and
    /// deduplicated.
    pub fn styles_for(&self, path: &str) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for section in self.sections.iter().filter(|s| glob_matches(&s.glob, path)) {
            for style in &section.based_on_styles {
                if !out.contains(&style.as_str()) {
                    out.push(style);
                }
            }
        }
        out
    }

    /// What this file's sections say about one rule. A later section wins,
    /// which is Vale's own precedence.
    pub fn override_for(&self, path: &str, rule: &str) -> Option<Override> {
        self.sections
            .iter()
            .filter(|s| glob_matches(&s.glob, path))
            .filter_map(|s| s.overrides.get(rule).copied())
            .next_back()
    }

    pub fn reports(&self, level: Level) -> bool {
        level >= self.min_alert_level
    }
}

fn read_global_key(ini: &mut ValeIni, key: &str, value: &str) {
    match key.to_ascii_lowercase().as_str() {
        "stylespath" => ini.styles_path = value.to_owned(),
        "minalertlevel" => {
            if let Some(level) = level_of(value) {
                ini.min_alert_level = level;
            }
        }
        "vocab" => {
            ini.vocab = value
                .split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .collect();
        }
        _ => {
            ini.other.insert(key.to_owned(), value.to_owned());
        }
    }
}

fn read_section_key(section: &mut Section, key: &str, value: &str) {
    if key.eq_ignore_ascii_case("basedonstyles") {
        section.based_on_styles = value
            .split(',')
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .collect();
        return;
    }
    // `Style.Rule = NO` or `Style.Rule = error`.
    if !key.contains('.') {
        return;
    }
    let setting = match value.to_ascii_uppercase().as_str() {
        "NO" | "FALSE" | "OFF" | "0" => Override::Off,
        _ => match level_of(value) {
            Some(level) => Override::Level(level),
            None => return,
        },
    };
    section.overrides.insert(key.to_owned(), setting);
}

fn level_of(value: &str) -> Option<Level> {
    match value.trim().to_ascii_lowercase().as_str() {
        "suggestion" => Some(Level::Suggestion),
        "warning" => Some(Level::Warning),
        "error" => Some(Level::Error),
        _ => None,
    }
}

/// Levels compare so `MinAlertLevel` can filter.
impl PartialOrd for Level {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Level {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(level: Level) -> u8 {
            match level {
                Level::Suggestion => 0,
                Level::Warning => 1,
                Level::Error => 2,
            }
        }
        rank(*self).cmp(&rank(*other))
    }
}

fn strip_comment(line: &str) -> &str {
    let at = line.find([';', '#']).unwrap_or(line.len());
    &line[..at]
}

/// The glob subset `.vale.ini` headers use: `*`, `*.md`, `docs/*.md`, and the
/// `[formats]`-style literal. `*` inside a segment matches anything but `/`.
fn glob_matches(glob: &str, path: &str) -> bool {
    let glob = glob.trim();
    if glob == "*" || glob.is_empty() {
        return true;
    }
    // A header may hold several comma-separated globs.
    if glob.contains(',') {
        return glob.split(',').any(|one| glob_matches(one, path));
    }
    let path = path.trim_start_matches("./");
    match glob.split_once('*') {
        None => glob == path,
        Some((head, tail)) => {
            let bare = path.rsplit('/').next().unwrap_or(path);
            let candidate = if head.contains('/') { path } else { bare };
            candidate.len() >= head.len() + tail.len()
                && candidate.starts_with(head)
                && candidate.ends_with(tail)
                && !candidate[head.len()..candidate.len() - tail.len()].contains('/')
        }
    }
}
