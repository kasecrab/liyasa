//! Translated OpenAPI specifications (CM-103).
//!
//! A locale either has a spec of its own or it has the default one with its
//! descriptions translated. Which of the two is decided per locale per spec, so
//! a site can hand-write the German spec and let the automation do the rest.
//!
//! The per-locale file is found by the two conventions CM-100 already gives
//! content — `openapi.de.yaml` beside the source, or the whole path under
//! `locales/de/` — so a translated spec is filed where a translated page is and
//! no config key is needed to point at it.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::ids::{Fingerprint, Locale};
use liyasa_core::vfs::VfsPath;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::locales::LOCALES_DIR;

/// Where the automation caches the descriptions it has translated.
pub const CACHE: &str = "locales/.liyasa-openapi.json";

/// Where one locale's copy of a spec comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The default locale, or a locale that has a spec of its own.
    File(String),
    /// No spec of its own: the default document with its description strings
    /// translated and cached.
    Translated { from: String },
}

impl Source {
    pub fn path(&self) -> &str {
        match self {
            Source::File(path) | Source::Translated { from: path } => path,
        }
    }
}

/// The `source` of one `openapi` entry, in either spelling the schema allows.
pub fn source_of(entry: &Value) -> Option<String> {
    match entry {
        Value::String(path) => Some(path.clone()),
        Value::Object(_) => entry.get("source")?.as_str().map(str::to_owned),
        _ => None,
    }
}

/// A spec the entry names for this locale outright.
// TODO(rfc-2700): CM-103 says "`openapi` config accepts per-locale spec files"
// and the schema's `openapi` entry has no key for one, so this reads a
// `locales` map that a config cannot yet spell. The conventions below are what
// a site actually uses today.
pub fn declared(entry: &Value, locale: &Locale) -> Option<String> {
    entry
        .get("locales")?
        .get(locale.as_str())?
        .as_str()
        .map(str::to_owned)
}

/// Where a locale's own spec would live, most specific first.
///
/// Empty for a remote source: `https://api.acme.com/openapi.yaml` is somebody
/// else's URL and guessing a translated one off it would fetch a document the
/// operator never published. A remote spec is translated, or named outright.
pub fn candidates(source: &str, locale: &Locale) -> Vec<VfsPath> {
    if source.contains("://") {
        return Vec::new();
    }
    let path = VfsPath::new(source);
    let text = path.as_str();
    let mut out = Vec::new();
    if let Some((stem, extension)) = text.rsplit_once('.') {
        out.push(VfsPath::new(format!("{stem}.{locale}.{extension}")));
    }
    out.push(VfsPath::new(format!("{LOCALES_DIR}/{locale}/{text}")));
    out
}

/// Which document this locale's API pages are built from.
///
/// `exists` answers whether a path is in the project; the default locale is
/// always the source itself.
pub fn resolve(
    entry: &Value,
    locale: &Locale,
    default: Option<&Locale>,
    exists: impl Fn(&VfsPath) -> bool,
) -> Option<Source> {
    let source = source_of(entry)?;
    if default == Some(locale) {
        return Some(Source::File(source));
    }
    if let Some(named) = declared(entry, locale) {
        return Some(Source::File(named));
    }
    for candidate in candidates(&source, locale) {
        if exists(&candidate) {
            return Some(Source::File(candidate.as_str().to_owned()));
        }
    }
    Some(Source::Translated { from: source })
}

/// The description strings the automation has translated, keyed by the digest
/// of the English they were translated from.
///
/// Keyed by digest rather than by JSON pointer, for two reasons: the same
/// sentence appears under many operations and is translated once, and an edit
/// to the English invalidates its translation without anything having to notice
/// — the digest simply stops matching.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Descriptions {
    pub entries: BTreeMap<String, BTreeMap<String, String>>,
}

impl Descriptions {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn render(&self) -> String {
        let mut out = serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_owned());
        out.push('\n');
        out
    }

    fn key(text: &str) -> String {
        Fingerprint::of(text).to_hex()
    }

    pub fn get(&self, locale: &Locale, english: &str) -> Option<&str> {
        self.entries
            .get(locale.as_str())?
            .get(&Self::key(english))
            .map(String::as_str)
    }

    pub fn put(&mut self, locale: &Locale, english: &str, translated: impl Into<String>) {
        self.entries
            .entry(locale.as_str().to_owned())
            .or_default()
            .insert(Self::key(english), translated.into());
    }

    /// The strings the automation still has to translate, in first-seen order
    /// with repeats collapsed.
    pub fn missing<'a>(
        &self,
        locale: &Locale,
        texts: impl IntoIterator<Item = &'a str>,
    ) -> Vec<&'a str> {
        let mut seen = BTreeSet::new();
        texts
            .into_iter()
            .filter(|text| !text.trim().is_empty())
            .filter(|text| self.get(locale, text).is_none())
            .filter(|text| seen.insert(*text))
            .collect()
    }

    /// Drops what the spec no longer says, so an edited API does not leave the
    /// cache growing every translation it ever had.
    pub fn prune<'a>(&mut self, locale: &Locale, live: impl IntoIterator<Item = &'a str>) {
        let keep: BTreeSet<String> = live.into_iter().map(Self::key).collect();
        if let Some(held) = self.entries.get_mut(locale.as_str()) {
            held.retain(|key, _| keep.contains(key));
            if held.is_empty() {
                self.entries.remove(locale.as_str());
            }
        }
    }

    /// The translation of one string, or the English it was not translated
    /// from. A missing translation shows the reader the original rather than a
    /// blank field.
    pub fn text<'a>(&'a self, locale: &Locale, english: &'a str) -> &'a str {
        self.get(locale, english).unwrap_or(english)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(json: &str) -> Value {
        serde_json::from_str(json).expect("the fixture is JSON")
    }

    fn de() -> Locale {
        Locale::new("de")
    }

    fn en() -> Locale {
        Locale::new("en")
    }

    #[test]
    fn a_spec_is_named_in_either_config_spelling() {
        assert_eq!(
            source_of(&value(r#""api/openapi.yaml""#)).as_deref(),
            Some("api/openapi.yaml")
        );
        assert_eq!(
            source_of(&value(r#"{"id":"core","source":"api/openapi.yaml"}"#)).as_deref(),
            Some("api/openapi.yaml")
        );
        assert_eq!(source_of(&value("{}")), None);
    }

    #[test]
    fn a_locale_looks_for_its_spec_where_it_looks_for_its_pages() {
        assert_eq!(
            candidates("api/openapi.yaml", &de()),
            vec![
                VfsPath::new("api/openapi.de.yaml"),
                VfsPath::new("locales/de/api/openapi.yaml"),
            ]
        );
    }

    #[test]
    fn a_remote_spec_is_never_guessed_at() {
        assert!(candidates("https://api.acme.com/openapi.yaml", &de()).is_empty());
    }

    #[test]
    fn the_default_locale_reads_the_source_itself() {
        let entry = value(r#""api/openapi.yaml""#);
        assert_eq!(
            resolve(&entry, &en(), Some(&en()), |_| false),
            Some(Source::File("api/openapi.yaml".to_owned()))
        );
    }

    #[test]
    fn a_locale_with_its_own_file_uses_it() {
        let entry = value(r#""api/openapi.yaml""#);
        assert_eq!(
            resolve(&entry, &de(), Some(&en()), |path| path.as_str()
                == "api/openapi.de.yaml"),
            Some(Source::File("api/openapi.de.yaml".to_owned()))
        );
        assert_eq!(
            resolve(&entry, &de(), Some(&en()), |path| path.as_str()
                == "locales/de/api/openapi.yaml"),
            Some(Source::File("locales/de/api/openapi.yaml".to_owned()))
        );
    }

    #[test]
    fn a_locale_with_no_file_falls_to_the_translated_descriptions() {
        let entry = value(r#""api/openapi.yaml""#);
        assert_eq!(
            resolve(&entry, &de(), Some(&en()), |_| false),
            Some(Source::Translated {
                from: "api/openapi.yaml".to_owned()
            })
        );
    }

    #[test]
    fn a_named_per_locale_spec_wins_over_the_conventions() {
        let entry = value(r#"{"source":"api/openapi.yaml","locales":{"de":"api/de.yaml"}}"#);
        assert_eq!(
            resolve(&entry, &de(), Some(&en()), |_| true),
            Some(Source::File("api/de.yaml".to_owned()))
        );
    }

    #[test]
    fn a_translation_is_cached_against_the_english_it_was_made_from() {
        let mut cache = Descriptions::default();
        cache.put(&de(), "Creates a user.", "Legt einen Benutzer an.");
        assert_eq!(
            cache.get(&de(), "Creates a user."),
            Some("Legt einen Benutzer an.")
        );
        assert_eq!(
            cache.get(&de(), "Creates a user"),
            None,
            "an edit to the English invalidates its translation"
        );
        assert_eq!(cache.get(&Locale::new("fr"), "Creates a user."), None);
    }

    #[test]
    fn the_reader_sees_the_english_rather_than_a_blank_field() {
        let cache = Descriptions::default();
        assert_eq!(cache.text(&de(), "Creates a user."), "Creates a user.");
    }

    #[test]
    fn the_automation_is_told_what_is_left_to_translate() {
        let mut cache = Descriptions::default();
        cache.put(&de(), "Creates a user.", "Legt einen Benutzer an.");
        assert_eq!(
            cache.missing(
                &de(),
                [
                    "Creates a user.",
                    "Deletes a user.",
                    "Deletes a user.",
                    "   ",
                    "Lists users.",
                ]
            ),
            vec!["Deletes a user.", "Lists users."],
            "repeats collapse and blank strings are not work"
        );
    }

    #[test]
    fn a_description_the_spec_dropped_leaves_the_cache() {
        let mut cache = Descriptions::default();
        cache.put(&de(), "Creates a user.", "Legt einen Benutzer an.");
        cache.put(&de(), "Removed operation.", "Entfernt.");
        cache.prune(&de(), ["Creates a user."]);
        assert!(cache.get(&de(), "Creates a user.").is_some());
        assert!(cache.get(&de(), "Removed operation.").is_none());

        cache.prune(&de(), []);
        assert!(
            cache.entries.is_empty(),
            "a locale with nothing left does not keep an empty table"
        );
    }

    #[test]
    fn the_cache_round_trips_through_its_file() {
        let mut cache = Descriptions::default();
        cache.put(&de(), "Creates a user.", "Legt einen Benutzer an.");
        let json = cache.render();
        assert!(json.ends_with('\n'));
        assert_eq!(Descriptions::parse(&json).expect("it parses"), cache);
    }
}
