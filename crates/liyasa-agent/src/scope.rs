//! What a path in the project IS, so a run can be told what it may write.
//!
//! AGT-04 words its prohibitions in terms of things — navigation, config,
//! redirects, facts, `AGENTS.md` — and a run writes files. This module is the
//! join: it classifies a repository-relative path into a [`Target`], and
//! [`Restrictions`](crate::trust::Restrictions) decides each target.
//!
//! Three properties matter more than the classification being complete.
//!
//! It is **fail-closed**. A path the classifier does not recognise is
//! [`Target::Unknown`], which a restricted run may not write. The alternative —
//! unrecognised means ordinary content — makes every future layout change a
//! silent widening of an untrusted run's reach.
//!
//! It **normalises before it classifies**, and refuses rather than normalising
//! what it cannot. `./liyasa.json`, `content/../liyasa.json` and
//! `content\nav.json` all have to land where `liyasa.json` and `content/nav.json`
//! land, or the classification is a string comparison a caller can walk around.
//!
//! It **carries the layout rather than assuming one**. The default is the
//! conventional project — `liyasa.json` at the root, facts under `facts/`,
//! content under the root — but a site that puts its content in `docs/` must not
//! thereby get an unclassified tree.

use std::collections::BTreeSet;

use liyasa_core::ids::Route;

/// The conventional project layout (PRD §33).
pub const DEFAULT_CONFIG_FILE: &str = "liyasa.json";
pub const DEFAULT_AGENTS_FILE: &str = "AGENTS.md";
pub const DEFAULT_FACT_DIRS: &[&str] = &["facts"];
/// Extensions a content page is written in.
pub const PAGE_EXTENSIONS: &[&str] = &["md", "mdx"];

/// Where the things AGT-04 names live in one project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The config file, which carries `navigation`, `redirects` and
    /// `automations` as keys rather than as files of their own.
    pub config_file: String,
    /// The agent's own instructions (AGT-30).
    pub agents_file: String,
    /// Directories whose contents are fact sources.
    pub fact_dirs: BTreeSet<String>,
    /// The directory content pages live under, `""` for the project root.
    pub content_root: String,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            config_file: DEFAULT_CONFIG_FILE.to_owned(),
            agents_file: DEFAULT_AGENTS_FILE.to_owned(),
            fact_dirs: DEFAULT_FACT_DIRS.iter().map(|d| (*d).to_owned()).collect(),
            content_root: String::new(),
        }
    }
}

/// Which of AGT-04's prohibitions a config edit reaches.
///
/// `navigation`, `redirects` and `automations` are keys in one file, so a path
/// alone cannot tell them apart; the caller that has the diff names the keys and
/// gets the specific reason, and a caller that has only the path still gets
/// [`ConfigArea::Unspecified`], which is refused exactly as the others are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConfigArea {
    Navigation,
    Redirects,
    Automations,
    /// Some other key, or the file as a whole.
    Unspecified,
}

impl ConfigArea {
    /// The config keys AGT-04 names, in the order it names them.
    pub const NAMED: [ConfigArea; 3] = [
        ConfigArea::Navigation,
        ConfigArea::Redirects,
        ConfigArea::Automations,
    ];

    /// The top-level config key, for a diff that knows which it touched.
    pub fn from_key(key: &str) -> Self {
        match key {
            "navigation" | "navbar" | "footer" => ConfigArea::Navigation,
            "redirects" => ConfigArea::Redirects,
            "automations" => ConfigArea::Automations,
            _ => ConfigArea::Unspecified,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            ConfigArea::Navigation => "navigation",
            ConfigArea::Redirects => "redirects",
            ConfigArea::Automations => "automations",
            ConfigArea::Unspecified => "config",
        }
    }

    /// The areas a change to the config file reached.
    ///
    /// AGT-04 and AGT-06 name `navigation`, `redirects` and `automations`
    /// separately, and all three are keys in one file, so the only way to say
    /// which of them a diff touched is to read the keys. Without this,
    /// [`Self::from_key`] is a branch nothing can reach and every config change
    /// reports as the generic `config` — which is refused identically but tells a
    /// reviewer less than the requirement's own wording does.
    ///
    /// A side that does not parse is [`Err`]. It is NOT "no keys changed": a diff
    /// that breaks the JSON would otherwise report nothing, and the gate would
    /// pass a file it could not read. Same rule as the front matter.
    ///
    /// The result is sorted and never empty for a real change: a change the key
    /// scan cannot attribute comes back as [`ConfigArea::Unspecified`], so a
    /// caller never has to decide what an empty list means.
    pub fn changed(
        before: Option<&str>,
        after: Option<&str>,
    ) -> Result<Vec<ConfigArea>, Unparseable> {
        let parse = |text: Option<&str>| -> Result<serde_json::Value, Unparseable> {
            match text {
                None => Ok(serde_json::Value::Null),
                Some(text) if text.trim().is_empty() => Ok(serde_json::Value::Null),
                Some(text) => serde_json::from_str(text).map_err(|_| Unparseable),
            }
        };
        let (before, after) = (parse(before)?, parse(after)?);
        if before == after {
            return Ok(Vec::new());
        }
        let keys: BTreeSet<&str> = [&before, &after]
            .into_iter()
            .filter_map(serde_json::Value::as_object)
            .flat_map(|object| object.keys().map(String::as_str))
            .collect();
        let mut areas: BTreeSet<ConfigArea> = keys
            .into_iter()
            .filter(|key| before.get(key) != after.get(key))
            .map(ConfigArea::from_key)
            .collect();
        if areas.is_empty() {
            // The file changed and no top-level key did — whitespace, key order,
            // or a side that is not an object at all. Still a config change.
            areas.insert(ConfigArea::Unspecified);
        }
        Ok(areas.into_iter().collect())
    }
}

/// A config or front matter side that could not be parsed, so the comparison could
/// not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the config file no longer parses, so the change cannot be attributed")]
pub struct Unparseable;

/// What one path in the project is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A content page, at the route it will be served under.
    Page(Route),
    /// The config file. `navigation`, `redirects` and `automations` live here.
    Config(ConfigArea),
    /// A fact source the truth engine reads.
    Facts,
    /// `AGENTS.md`.
    AgentsMd,
    /// Something inside the project that is none of the above — an asset, a
    /// snippet, a theme file. Refused to a restricted run, like everything the
    /// trigger did not name.
    Other,
    /// A path or route that could not be normalised, or that leaves the project.
    Unknown { path: String },
}

/// A site route, normalised, or `None` if it is not one.
///
/// A route is not a path — it has a leading slash and is what a reader sees — but
/// it needs the same treatment: `/guides/../secret` must not compare equal to
/// anything, and a write scope holding `/guides/install` must match a call that
/// writes `/guides/./install`. Normalising both sides is what makes the
/// comparison a comparison rather than a string match a caller can dodge.
///
/// The root is `/`. Everything else has no trailing slash, which is the form
/// [`liyasa_core::ids::Route`] documents.
pub fn normalise_route(route: &str) -> Option<Route> {
    if !route.starts_with('/') || route.contains('\0') || route.contains('\\') {
        return None;
    }
    let mut parts = Vec::new();
    for part in route.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            other => parts.push(other),
        }
    }
    Some(Route::new(format!("/{}", parts.join("/"))))
}

/// The path as a `/`-separated project-relative path, or `None` if it leaves the
/// project or cannot be read as one.
///
/// `..` is refused rather than resolved. Resolving it would be correct for a
/// path that stays inside, and there is no reason for the agent to produce one,
/// so the cheaper rule is also the safer one.
pub fn normalise(path: &str) -> Option<String> {
    if path.is_empty() || path.starts_with('/') || path.contains('\0') {
        return None;
    }
    // A Windows drive letter or a UNC path is absolute and not project-relative.
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

impl Layout {
    /// Classifies one repository-relative path.
    pub fn classify(&self, path: &str) -> Target {
        let Some(normalised) = normalise(path) else {
            return Target::Unknown {
                path: path.to_owned(),
            };
        };
        let path = normalised;
        if path == self.config_file {
            return Target::Config(ConfigArea::Unspecified);
        }
        if path == self.agents_file || path.ends_with(&format!("/{}", self.agents_file)) {
            return Target::AgentsMd;
        }
        if self
            .fact_dirs
            .iter()
            .any(|dir| path == *dir || path.starts_with(&format!("{dir}/")))
        {
            return Target::Facts;
        }
        match self.route_of(&path) {
            Some(route) => Target::Page(route),
            None => Target::Other,
        }
    }

    /// The route a content path is served under, or `None` when the path is not
    /// a content page.
    pub fn route_of(&self, path: &str) -> Option<Route> {
        let rest = if self.content_root.is_empty() {
            path
        } else {
            path.strip_prefix(&format!("{}/", self.content_root))?
        };
        let (stem, extension) = rest.rsplit_once('.')?;
        if !PAGE_EXTENSIONS.contains(&extension) {
            return None;
        }
        // `index.md` is its directory, not a page called `index`.
        let stem = stem.strip_suffix("index").map_or(stem, |s| {
            s.strip_suffix('/')
                .unwrap_or(if s.is_empty() { "" } else { s })
        });
        Some(Route::new(format!("/{stem}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_conventional_layout_places_each_thing_agt_04_names() {
        let layout = Layout::default();
        assert_eq!(
            layout.classify("liyasa.json"),
            Target::Config(ConfigArea::Unspecified)
        );
        assert_eq!(layout.classify("AGENTS.md"), Target::AgentsMd);
        assert_eq!(layout.classify("facts/limits.json"), Target::Facts);
        assert_eq!(
            layout.classify("guides/install.md"),
            Target::Page(Route::new("/guides/install"))
        );
        assert_eq!(layout.classify("assets/logo.svg"), Target::Other);
    }

    #[test]
    fn a_traversal_is_refused_rather_than_resolved() {
        let layout = Layout::default();
        for hostile in [
            "../liyasa.json",
            "guides/../../liyasa.json",
            "/etc/passwd",
            "C:\\liyasa.json",
            "",
        ] {
            assert_eq!(
                layout.classify(hostile),
                Target::Unknown {
                    path: hostile.to_owned()
                },
                "`{hostile}` classified as something a restricted run could write"
            );
        }
    }

    #[test]
    fn a_dodged_separator_lands_where_the_plain_path_lands() {
        // The classification is a comparison against a normalised path, so
        // `./liyasa.json` and `guides\install.md` cannot walk around it.
        let layout = Layout::default();
        assert_eq!(
            layout.classify("./liyasa.json"),
            Target::Config(ConfigArea::Unspecified)
        );
        assert_eq!(
            layout.classify("guides\\install.md"),
            Target::Page(Route::new("/guides/install"))
        );
        assert_eq!(layout.classify("./AGENTS.md"), Target::AgentsMd);
        assert_eq!(layout.classify("facts//limits.json"), Target::Facts);
    }

    #[test]
    fn an_index_page_is_its_directory() {
        let layout = Layout::default();
        assert_eq!(
            layout.classify("guides/index.md"),
            Target::Page(Route::new("/guides"))
        );
        assert_eq!(layout.classify("index.md"), Target::Page(Route::new("/")));
    }

    #[test]
    fn a_content_root_does_not_leave_the_tree_unclassified() {
        let layout = Layout {
            content_root: "docs".to_owned(),
            ..Layout::default()
        };
        assert_eq!(
            layout.classify("docs/guides/install.md"),
            Target::Page(Route::new("/guides/install"))
        );
        // Outside the content root it is not a page — and `Other`, not `Page`,
        // is what a restricted run is refused on.
        assert_eq!(layout.classify("elsewhere/install.md"), Target::Other);
    }

    #[test]
    fn a_nested_agents_file_is_still_the_agents_file() {
        // AGT-30 puts it in the docs root, which is not always the repository
        // root; a run that may not touch it may not touch the nested one.
        assert_eq!(
            Layout::default().classify("docs/AGENTS.md"),
            Target::AgentsMd
        );
    }

    #[test]
    fn a_config_change_names_the_area_agt_04_words() {
        for (key, area) in [
            ("navigation", ConfigArea::Navigation),
            ("navbar", ConfigArea::Navigation),
            ("footer", ConfigArea::Navigation),
            ("redirects", ConfigArea::Redirects),
            ("automations", ConfigArea::Automations),
            ("theme", ConfigArea::Unspecified),
        ] {
            let before = r#"{"name":"Acme"}"#;
            let after = format!("{{\"name\":\"Acme\",\"{key}\":[]}}");
            assert_eq!(
                ConfigArea::changed(Some(before), Some(&after)),
                Ok(vec![area]),
                "a change to `{key}` was not attributed"
            );
        }
    }

    #[test]
    fn a_change_to_two_areas_names_both() {
        let before = r#"{"name":"Acme"}"#;
        let after = r#"{"name":"Acme","navigation":[],"redirects":[]}"#;
        assert_eq!(
            ConfigArea::changed(Some(before), Some(after)),
            Ok(vec![ConfigArea::Navigation, ConfigArea::Redirects])
        );
    }

    #[test]
    fn an_unchanged_config_names_nothing() {
        let same = r#"{"name":"Acme","redirects":[]}"#;
        assert_eq!(ConfigArea::changed(Some(same), Some(same)), Ok(Vec::new()));
    }

    #[test]
    fn a_config_that_no_longer_parses_is_not_an_absence_of_change() {
        // The same hole the front matter has: a diff that breaks the JSON would
        // otherwise report no key as changed, and the gate would pass a file it
        // could not read.
        assert_eq!(
            ConfigArea::changed(Some(r#"{"name":"Acme"}"#), Some(r#"{"name":"#)),
            Err(Unparseable)
        );
    }

    #[test]
    fn a_change_no_key_scan_can_attribute_is_still_a_config_change() {
        // Reordered keys, reformatted whitespace, or a side that is not an object.
        // An empty list would make a caller decide what it meant.
        assert_eq!(
            ConfigArea::changed(Some(r#"{"a":1,"b":2}"#), Some(r#"[1,2]"#)),
            Ok(vec![ConfigArea::Unspecified])
        );
    }

    #[test]
    fn adding_or_deleting_the_config_file_is_a_change() {
        assert_eq!(
            ConfigArea::changed(None, Some(r#"{"redirects":[]}"#)),
            Ok(vec![ConfigArea::Redirects])
        );
        assert_eq!(
            ConfigArea::changed(Some(r#"{"navigation":[]}"#), None),
            Ok(vec![ConfigArea::Navigation])
        );
    }

    #[test]
    fn a_route_normalises_and_refuses_a_traversal() {
        assert_eq!(
            normalise_route("/guides/install"),
            Some(Route::new("/guides/install"))
        );
        assert_eq!(
            normalise_route("/guides/./install"),
            Some(Route::new("/guides/install"))
        );
        assert_eq!(
            normalise_route("/guides//install"),
            Some(Route::new("/guides/install"))
        );
        assert_eq!(
            normalise_route("/guides/install/"),
            Some(Route::new("/guides/install"))
        );
        assert_eq!(normalise_route("/"), Some(Route::new("/")));
        for hostile in [
            "/guides/../secret",
            "/..",
            "guides/install",
            "/a\\b",
            "/a\0b",
        ] {
            assert_eq!(normalise_route(hostile), None, "`{hostile}` normalised");
        }
    }

    #[test]
    fn the_config_areas_agt_04_names_map_from_their_keys() {
        assert_eq!(ConfigArea::from_key("navigation"), ConfigArea::Navigation);
        assert_eq!(ConfigArea::from_key("redirects"), ConfigArea::Redirects);
        assert_eq!(ConfigArea::from_key("automations"), ConfigArea::Automations);
        assert_eq!(ConfigArea::from_key("theme"), ConfigArea::Unspecified);
    }
}
