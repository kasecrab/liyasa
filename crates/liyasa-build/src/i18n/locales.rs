//! Locales: where a translation lives and where it is served (CM-100, CM-101).
//!
//! Two content models, which may be mixed, exactly as versions have two: a full
//! tree under `locales/<code>/` mirroring the default tree, and a translation
//! beside its source as `page.<code>.md`. Either way the default locale is
//! served at the un-prefixed route and every other locale under `/<code>/`.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::ids::{Locale, Route};
use liyasa_core::vfs::VfsPath;
use serde_json::Value;

use super::config::LocaleDecl;

/// The directory a full locale tree lives in.
pub const LOCALES_DIR: &str = "locales";

/// One entry of the navbar's language switcher (CM-101).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitcherEntry {
    pub locale: Locale,
    pub label: String,
    pub href: String,
    pub current: bool,
    /// The page has no translation in that locale, so the entry points at what
    /// the reader would actually be served.
    pub untranslated: bool,
}

/// One `<link rel="alternate" hreflang>` of a page (CM-101).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alternate {
    /// The `hreflang` value: a locale code, or `x-default`.
    pub hreflang: String,
    pub href: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Locales {
    decls: Vec<LocaleDecl>,
}

impl Locales {
    pub fn new(decls: &[LocaleDecl]) -> Self {
        Self {
            decls: decls.to_vec(),
        }
    }

    /// `locales` as the schema spells it: bare codes, in which case the first is
    /// the default, or objects, in which case one carries `default: true`.
    pub fn from_value(value: &Value) -> Self {
        let Some(array) = value.get("locales").and_then(Value::as_array) else {
            return Self::default();
        };
        let mut decls: Vec<LocaleDecl> = array
            .iter()
            .filter_map(|item| match item {
                Value::String(code) => Some(LocaleDecl {
                    label: code.clone(),
                    code: code.clone(),
                    default: false,
                }),
                Value::Object(_) => {
                    let code = item.get("code")?.as_str()?.to_owned();
                    Some(LocaleDecl {
                        label: item
                            .get("label")
                            .and_then(Value::as_str)
                            .unwrap_or(&code)
                            .to_owned(),
                        default: item
                            .get("default")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        code,
                    })
                }
                _ => None,
            })
            .collect();
        // CM-100: a bare list has no way to mark a default, so the first wins.
        // E0108 is `liyasa-config`'s to raise when an object list marks none.
        if !decls.iter().any(|decl| decl.default)
            && let Some(first) = decls.first_mut()
        {
            first.default = true;
        }
        Self { decls }
    }

    pub fn is_empty(&self) -> bool {
        self.decls.is_empty()
    }

    pub fn codes(&self) -> Vec<Locale> {
        self.decls
            .iter()
            .map(|decl| Locale::new(decl.code.clone()))
            .collect()
    }

    pub fn declarations(&self) -> &[LocaleDecl] {
        &self.decls
    }

    pub fn default_locale(&self) -> Option<Locale> {
        self.decls
            .iter()
            .find(|decl| decl.default)
            .or_else(|| self.decls.first())
            .map(|decl| Locale::new(decl.code.clone()))
    }

    pub fn declaration(&self, locale: &Locale) -> Option<&LocaleDecl> {
        self.decls.iter().find(|decl| decl.code == locale.as_str())
    }

    pub fn is_declared(&self, code: &str) -> bool {
        self.decls.iter().any(|decl| decl.code == code)
    }

    /// The locale a file belongs to and the path within the default tree
    /// (CM-100).
    ///
    /// `locales/de/guides/install.md` and `guides/install.de.md` both answer
    /// `(de, guides/install.md)`, so the two content models produce the same
    /// base route and one page can be translated either way.
    ///
    /// A suffix is a locale only when the site declares it: `api.v2.md` is a
    /// page called `api.v2`, not a `v2` translation of `api`.
    pub fn of_path(&self, path: &VfsPath) -> Option<(Locale, VfsPath)> {
        if let Some(found) = self.of_tree(path) {
            return Some(found);
        }
        self.of_suffix(path)
    }

    fn of_tree(&self, path: &VfsPath) -> Option<(Locale, VfsPath)> {
        let rest = path.as_str().strip_prefix(LOCALES_DIR)?.strip_prefix('/')?;
        let (code, within) = rest.split_once('/')?;
        self.is_declared(code)
            .then(|| (Locale::new(code.to_owned()), VfsPath::new(within)))
    }

    fn of_suffix(&self, path: &VfsPath) -> Option<(Locale, VfsPath)> {
        let text = path.as_str();
        let (stem, extension) = text.rsplit_once('.')?;
        let (base, code) = stem.rsplit_once('.')?;
        if !self.is_declared(code) {
            return None;
        }
        Some((
            Locale::new(code.to_owned()),
            VfsPath::new(format!("{base}.{extension}")),
        ))
    }

    /// Where a page is served: the default locale keeps the bare route, every
    /// other locale is prefixed (CM-101).
    pub fn route_of(&self, base: &Route, locale: Option<&Locale>) -> Route {
        let Some(locale) = locale else {
            return base.clone();
        };
        if self.default_locale().as_ref() == Some(locale) {
            return base.clone();
        }
        let trimmed = base.as_str().trim_matches('/');
        match trimmed.is_empty() {
            true => Route::new(format!("/{locale}")),
            false => Route::new(format!("/{locale}/{trimmed}")),
        }
    }

    /// `/de/llms.txt`, `/de/sitemap.xml`, and the rest of the per-locale
    /// surfaces (CM-101).
    pub fn surface(&self, locale: Option<&Locale>, name: &str) -> String {
        self.route_of(&Route::new(format!("/{name}")), locale)
            .as_str()
            .to_owned()
    }

    /// The language switcher for one page (CM-101).
    ///
    /// A locale that does not have the page still gets an entry, because
    /// `localization.fallback` decides what the reader lands on and the
    /// switcher's job is to offer the language, not to hide it. `untranslated`
    /// says which entries are in that state so the theme can mark them.
    pub fn switcher(
        &self,
        base: &Route,
        current: Option<&Locale>,
        routes: &BTreeMap<Locale, BTreeSet<Route>>,
    ) -> Vec<SwitcherEntry> {
        self.decls
            .iter()
            .map(|decl| {
                let locale = Locale::new(decl.code.clone());
                let translated = routes
                    .get(&locale)
                    .is_some_and(|known| known.contains(base));
                SwitcherEntry {
                    href: self.route_of(base, Some(&locale)).as_str().to_owned(),
                    current: current == Some(&locale),
                    label: decl.label.clone(),
                    untranslated: !translated,
                    locale,
                }
            })
            .collect()
    }

    /// The `hreflang` alternates of one page (CM-101).
    ///
    /// Only a locale that actually serves the page is listed: an alternate
    /// pointing at a route that 404s, or at the default locale's text under a
    /// "not yet translated" notice, tells a search engine the page exists in a
    /// language it does not. `x-default` names the default locale.
    ///
    /// `origin` is the canonical origin with no trailing slash. An empty origin
    /// yields site-relative hrefs, which is what a build with no
    /// `seo.canonicalOrigin` has to do.
    pub fn alternates(
        &self,
        base: &Route,
        routes: &BTreeMap<Locale, BTreeSet<Route>>,
        origin: &str,
    ) -> Vec<Alternate> {
        let origin = origin.trim_end_matches('/');
        let default = self.default_locale();
        let mut out = Vec::new();
        let mut listed = 0usize;
        for decl in &self.decls {
            let locale = Locale::new(decl.code.clone());
            if !routes
                .get(&locale)
                .is_some_and(|known| known.contains(base))
            {
                continue;
            }
            let href = format!("{origin}{}", self.route_of(base, Some(&locale)));
            if default.as_ref() == Some(&locale) {
                out.push(Alternate {
                    hreflang: "x-default".to_owned(),
                    href: href.clone(),
                });
            }
            out.push(Alternate {
                hreflang: decl.code.clone(),
                href,
            });
            listed += 1;
        }
        // One locale is the page pointing at itself and says nothing, however
        // many entries that one locale contributed.
        match listed > 1 {
            true => out,
            false => Vec::new(),
        }
    }
}

/// Every base route each locale serves, which is what the switcher and the
/// alternates are built from.
pub fn routes_by_locale<'a>(
    pages: impl IntoIterator<Item = (&'a Locale, &'a Route)>,
) -> BTreeMap<Locale, BTreeSet<Route>> {
    let mut out: BTreeMap<Locale, BTreeSet<Route>> = BTreeMap::new();
    for (locale, route) in pages {
        out.entry(locale.clone()).or_default().insert(route.clone());
    }
    out
}

#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub fn decls() -> Vec<LocaleDecl> {
        vec![
            LocaleDecl {
                code: "en".to_owned(),
                label: "English".to_owned(),
                default: true,
            },
            LocaleDecl {
                code: "de".to_owned(),
                label: "Deutsch".to_owned(),
                default: false,
            },
            LocaleDecl {
                code: "pt-BR".to_owned(),
                label: "Português do Brasil".to_owned(),
                default: false,
            },
        ]
    }

    pub fn locales() -> Locales {
        Locales::new(&decls())
    }

    pub fn routes(pairs: &[(&str, &[&str])]) -> BTreeMap<Locale, BTreeSet<Route>> {
        pairs
            .iter()
            .map(|(locale, routes)| {
                (
                    Locale::new((*locale).to_owned()),
                    routes.iter().map(|route| Route::new(*route)).collect(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::*;
    use super::*;

    fn value(json: &str) -> Value {
        serde_json::from_str(json).expect("the fixture is JSON")
    }

    #[test]
    fn a_bare_list_makes_the_first_locale_the_default() {
        let locales = Locales::from_value(&value(r#"{"locales":["en","de"]}"#));
        assert_eq!(locales.default_locale(), Some(Locale::new("en")));
        assert_eq!(
            locales.codes(),
            vec![Locale::new("en"), Locale::new("de")],
            "the declaration order is kept"
        );
    }

    #[test]
    fn an_explicit_default_wins_wherever_it_sits() {
        let locales = Locales::from_value(&value(
            r#"{"locales":["de",{"code":"en","default":true,"label":"English"}]}"#,
        ));
        assert_eq!(locales.default_locale(), Some(Locale::new("en")));
        assert_eq!(
            locales
                .declaration(&Locale::new("en"))
                .map(|decl| decl.label.as_str()),
            Some("English")
        );
        assert_eq!(
            locales
                .declaration(&Locale::new("de"))
                .map(|decl| decl.label.as_str()),
            Some("de"),
            "a bare code labels itself"
        );
    }

    #[test]
    fn a_site_with_no_locales_is_not_a_localized_site() {
        let locales = Locales::from_value(&value("{}"));
        assert!(locales.is_empty());
        assert_eq!(locales.default_locale(), None);
        let base = Route::new("/guides/install");
        assert_eq!(locales.route_of(&base, None), base);
    }

    #[test]
    fn a_locale_tree_and_a_suffix_reach_the_same_page() {
        let locales = locales();
        assert_eq!(
            locales.of_path(&VfsPath::new("locales/de/guides/install.md")),
            Some((Locale::new("de"), VfsPath::new("guides/install.md")))
        );
        assert_eq!(
            locales.of_path(&VfsPath::new("guides/install.de.md")),
            Some((Locale::new("de"), VfsPath::new("guides/install.md")))
        );
    }

    #[test]
    fn a_hyphenated_locale_works_in_both_models() {
        let locales = locales();
        assert_eq!(
            locales.of_path(&VfsPath::new("locales/pt-BR/index.md")),
            Some((Locale::new("pt-BR"), VfsPath::new("index.md")))
        );
        assert_eq!(
            locales.of_path(&VfsPath::new("index.pt-BR.md")),
            Some((Locale::new("pt-BR"), VfsPath::new("index.md")))
        );
    }

    #[test]
    fn a_dot_in_a_file_name_is_not_a_locale_unless_the_site_declares_it() {
        let locales = locales();
        assert_eq!(locales.of_path(&VfsPath::new("api.v2.md")), None);
        assert_eq!(locales.of_path(&VfsPath::new("guides/install.md")), None);
        assert_eq!(
            locales.of_path(&VfsPath::new("locales/fr/index.md")),
            None,
            "an undeclared locale directory is an ordinary directory"
        );
    }

    #[test]
    fn the_default_locale_keeps_the_bare_route_and_the_rest_are_prefixed() {
        let locales = locales();
        let base = Route::new("/guides/install");
        assert_eq!(locales.route_of(&base, Some(&Locale::new("en"))), base);
        assert_eq!(
            locales.route_of(&base, Some(&Locale::new("de"))),
            Route::new("/de/guides/install")
        );
        assert_eq!(
            locales.route_of(&Route::new("/"), Some(&Locale::new("de"))),
            Route::new("/de")
        );
    }

    #[test]
    fn every_locale_has_its_own_agent_surfaces() {
        let locales = locales();
        assert_eq!(
            locales.surface(Some(&Locale::new("en")), "llms.txt"),
            "/llms.txt"
        );
        assert_eq!(
            locales.surface(Some(&Locale::new("de")), "llms.txt"),
            "/de/llms.txt"
        );
        assert_eq!(
            locales.surface(Some(&Locale::new("de")), "sitemap.xml"),
            "/de/sitemap.xml"
        );
    }

    #[test]
    fn the_switcher_offers_every_language_and_marks_the_untranslated_ones() {
        let locales = locales();
        let known = routes(&[
            ("en", &["/guides/install"]),
            ("de", &["/guides/install"]),
            ("pt-BR", &["/"]),
        ]);
        let entries = locales.switcher(
            &Route::new("/guides/install"),
            Some(&Locale::new("en")),
            &known,
        );
        assert_eq!(entries.len(), 3);
        assert!(entries[0].current);
        assert!(!entries[0].untranslated);
        assert_eq!(entries[1].href, "/de/guides/install");
        assert!(!entries[1].untranslated);
        assert!(entries[2].untranslated, "pt-BR has no such page");
        assert_eq!(
            entries[2].href, "/pt-BR/guides/install",
            "the offer still points at where the locale serves the route"
        );
    }

    #[test]
    fn alternates_name_only_the_locales_that_have_the_page() {
        let locales = locales();
        let known = routes(&[("en", &["/guides/install"]), ("de", &["/guides/install"])]);
        let alternates = locales.alternates(
            &Route::new("/guides/install"),
            &known,
            "https://docs.acme.com/",
        );
        assert_eq!(
            alternates,
            vec![
                Alternate {
                    hreflang: "x-default".to_owned(),
                    href: "https://docs.acme.com/guides/install".to_owned(),
                },
                Alternate {
                    hreflang: "en".to_owned(),
                    href: "https://docs.acme.com/guides/install".to_owned(),
                },
                Alternate {
                    hreflang: "de".to_owned(),
                    href: "https://docs.acme.com/de/guides/install".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn a_page_in_one_locale_has_no_alternates_at_all() {
        let locales = locales();
        let known = routes(&[("en", &["/guides/install"])]);
        assert!(
            locales
                .alternates(&Route::new("/guides/install"), &known, "https://d.acme.com")
                .is_empty(),
            "a lone self-referential alternate says nothing"
        );
    }

    #[test]
    fn an_absent_origin_leaves_the_hrefs_site_relative() {
        let locales = locales();
        let known = routes(&[("en", &["/"]), ("de", &["/"])]);
        let alternates = locales.alternates(&Route::new("/"), &known, "");
        assert_eq!(alternates[0].href, "/");
        assert_eq!(alternates[2].href, "/de");
    }

    #[test]
    fn the_route_table_is_built_from_the_pages() {
        let pairs = [
            (Locale::new("en"), Route::new("/")),
            (Locale::new("de"), Route::new("/")),
            (Locale::new("en"), Route::new("/guides")),
        ];
        let table = routes_by_locale(pairs.iter().map(|(l, r)| (l, r)));
        assert_eq!(table[&Locale::new("en")].len(), 2);
        assert_eq!(table[&Locale::new("de")].len(), 1);
    }
}
