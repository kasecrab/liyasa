//! What a locale serves for a page it has no translation of (CM-102).
//!
//! `localization.fallback` picks between two answers and there is no third: the
//! default locale's text under a notice, or no route at all.

use std::collections::{BTreeMap, BTreeSet};

use liyasa_core::ids::{Locale, Route};

use super::config::{Fallback, Localization};
use super::locales::Locales;
use super::strings;

/// What one locale does with one base route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Serve {
    /// The locale has its own translation of the page.
    Translated,
    /// The default locale's text, under [`Notice`].
    Untranslated(Notice),
    /// No route at all in this locale.
    Hidden,
}

impl Serve {
    pub fn is_served(&self) -> bool {
        !matches!(self, Serve::Hidden)
    }

    /// Whether the locale's copy of this page belongs in a sitemap.
    ///
    /// A fallback page is the default locale's words at a `/<code>/` route: to
    /// a search engine that is the same document twice, once under a language
    /// it is not written in. It is served because a reader who followed a link
    /// should get the page rather than a 404, and it is not indexed for the
    /// same reason it carries no `hreflang` alternate — the translation does
    /// not exist.
    pub fn is_indexable(&self) -> bool {
        matches!(self, Serve::Translated)
    }
}

/// The "This page is not yet translated" banner (CM-102).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// In the reader's own locale, from the shipped catalogue (CM-105).
    pub message: String,
    /// Where the text being shown actually comes from: the default locale's
    /// route for the same page.
    pub source_href: String,
    /// The default locale's code, for the `lang` attribute on the fallback
    /// body. The words are in that language, whatever the route says.
    pub source_locale: Locale,
}

/// The translation state of a whole site, built once per build.
#[derive(Debug, Clone)]
pub struct Translations {
    locales: Locales,
    fallback: Fallback,
    routes: BTreeMap<Locale, BTreeSet<Route>>,
}

impl Translations {
    pub fn new(
        locales: &Locales,
        localization: &Localization,
        routes: BTreeMap<Locale, BTreeSet<Route>>,
    ) -> Self {
        Self {
            locales: locales.clone(),
            fallback: localization.fallback,
            routes,
        }
    }

    pub fn fallback(&self) -> Fallback {
        self.fallback
    }

    /// Every base route the default locale has, which is the set every other
    /// locale is measured against.
    pub fn source_routes(&self) -> BTreeSet<Route> {
        self.locales
            .default_locale()
            .and_then(|default| self.routes.get(&default).cloned())
            .unwrap_or_default()
    }

    pub fn has(&self, locale: &Locale, base: &Route) -> bool {
        self.routes
            .get(locale)
            .is_some_and(|known| known.contains(base))
    }

    /// What `locale` serves at `base`.
    ///
    /// A route the default locale does not have either is `Hidden` whatever the
    /// config says: there is nothing to fall back to.
    pub fn serve(&self, locale: &Locale, base: &Route) -> Serve {
        if self.has(locale, base) {
            return Serve::Translated;
        }
        let Some(default) = self.locales.default_locale() else {
            return Serve::Hidden;
        };
        if &default == locale || !self.has(&default, base) {
            return Serve::Hidden;
        }
        match self.fallback {
            Fallback::Hide => Serve::Hidden,
            Fallback::Notice => Serve::Untranslated(Notice {
                message: strings::not_translated(locale.as_str()).to_owned(),
                source_href: self
                    .locales
                    .route_of(base, Some(&default))
                    .as_str()
                    .to_owned(),
                source_locale: default,
            }),
        }
    }

    /// Every route one locale serves, translated and fallen back alike, in
    /// route order. This is what that locale's sitemap, `llms.txt` and
    /// navigation are built from.
    pub fn served_by(&self, locale: &Locale) -> Vec<Route> {
        let mut out: BTreeSet<Route> = self.routes.get(locale).cloned().unwrap_or_default();
        if self.fallback == Fallback::Notice {
            for base in self.source_routes() {
                if self.serve(locale, &base).is_served() {
                    out.insert(base);
                }
            }
        }
        out.into_iter().collect()
    }

    /// The routes one locale's sitemap may list: the translated ones only
    /// ([`Serve::is_indexable`]).
    pub fn indexable_by(&self, locale: &Locale) -> Vec<Route> {
        self.served_by(locale)
            .into_iter()
            .filter(|base| self.serve(locale, base).is_indexable())
            .collect()
    }

    /// How far each locale has got, for the build report and for the translate
    /// automation (CM-106): translated routes over the default locale's.
    pub fn coverage(&self) -> BTreeMap<Locale, (usize, usize)> {
        let total = self.source_routes().len();
        self.locales
            .codes()
            .into_iter()
            .map(|locale| {
                let held = self
                    .routes
                    .get(&locale)
                    .map(|known| {
                        known
                            .iter()
                            .filter(|route| self.source_routes().contains(*route))
                            .count()
                    })
                    .unwrap_or(0);
                (locale, (held, total))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::locales::tests_support::{locales, routes};
    use super::*;

    fn translations(fallback: Fallback) -> Translations {
        Translations::new(
            &locales(),
            &Localization {
                fallback,
                route_visitors: false,
            },
            routes(&[
                ("en", &["/", "/guides/install", "/reference"]),
                ("de", &["/", "/guides/install"]),
                ("pt-BR", &["/"]),
            ]),
        )
    }

    #[test]
    fn a_translated_page_is_served_as_itself() {
        let state = translations(Fallback::Notice);
        assert_eq!(
            state.serve(&Locale::new("de"), &Route::new("/guides/install")),
            Serve::Translated
        );
    }

    #[test]
    fn an_untranslated_page_carries_a_notice_in_the_readers_language() {
        let state = translations(Fallback::Notice);
        let Serve::Untranslated(notice) =
            state.serve(&Locale::new("de"), &Route::new("/reference"))
        else {
            panic!("the notice fallback serves the page");
        };
        assert_eq!(notice.source_href, "/reference");
        assert_eq!(notice.source_locale, Locale::new("en"));
        assert!(
            notice.message.contains("übersetzt"),
            "the notice is in the reader's locale: {}",
            notice.message
        );
    }

    #[test]
    fn hiding_removes_the_route_rather_than_translating_it() {
        let state = translations(Fallback::Hide);
        assert_eq!(
            state.serve(&Locale::new("de"), &Route::new("/reference")),
            Serve::Hidden
        );
        assert_eq!(
            state.served_by(&Locale::new("de")),
            vec![Route::new("/"), Route::new("/guides/install")]
        );
    }

    #[test]
    fn a_route_the_default_locale_lacks_is_hidden_whatever_the_config_says() {
        let state = translations(Fallback::Notice);
        assert_eq!(
            state.serve(&Locale::new("de"), &Route::new("/nowhere")),
            Serve::Hidden,
            "there is nothing to fall back to"
        );
    }

    #[test]
    fn the_default_locale_never_falls_back_to_itself() {
        let state = translations(Fallback::Notice);
        assert_eq!(
            state.serve(&Locale::new("en"), &Route::new("/nowhere")),
            Serve::Hidden
        );
    }

    #[test]
    fn a_notice_site_serves_every_route_in_every_locale() {
        let state = translations(Fallback::Notice);
        assert_eq!(
            state.served_by(&Locale::new("pt-BR")),
            vec![
                Route::new("/"),
                Route::new("/guides/install"),
                Route::new("/reference")
            ]
        );
    }

    #[test]
    fn only_a_real_translation_reaches_a_sitemap() {
        let state = translations(Fallback::Notice);
        assert_eq!(
            state.indexable_by(&Locale::new("pt-BR")),
            vec![Route::new("/")],
            "a fallback page is the default locale's words at a translated route"
        );
        assert_eq!(state.indexable_by(&Locale::new("de")).len(), 2);
    }

    #[test]
    fn coverage_counts_translations_against_the_default_locale() {
        let state = translations(Fallback::Notice);
        let coverage = state.coverage();
        assert_eq!(coverage[&Locale::new("en")], (3, 3));
        assert_eq!(coverage[&Locale::new("de")], (2, 3));
        assert_eq!(coverage[&Locale::new("pt-BR")], (1, 3));
    }

    #[test]
    fn a_translation_of_a_route_the_source_dropped_does_not_count_as_coverage() {
        let state = Translations::new(
            &locales(),
            &Localization::default(),
            routes(&[("en", &["/"]), ("de", &["/", "/removed"])]),
        );
        assert_eq!(
            state.coverage()[&Locale::new("de")],
            (1, 1),
            "a page the source no longer has is not progress against the source"
        );
    }
}
