//! VER-20: facts of each type, formatted with the typed filters in two
//! locales, against golden output.
//!
//! The facts are not built by hand. They come out of a declared `file` source,
//! through the refresher, into the `facts.*` layer of a real template context,
//! and are expanded by the expander the build uses — because the thing under
//! test is whether a *typed* fact survives that journey in a form the filters
//! can read, and a hand-built `FactValue` would never have made the journey.

use std::sync::Arc;

use liyasa_core::conformance::block_on;
use liyasa_core::conformance::fixtures::MemoryVfs;
use liyasa_core::ids::FactId;
use liyasa_core::net::{BoxFut, HttpClient, HttpPolicy, HttpRequest, HttpResponse, NetError};
use liyasa_core::source_map::SourceMap;
use liyasa_core::verify::FactValue;
use liyasa_core::vfs::VfsPath;
use liyasa_markdown::source::expand::ExpandOptions;
use liyasa_markdown::source::{Layers, environment, expand, scan};
use liyasa_verify::sources::kinds::{BuildTrust, DeclaredSource};
use liyasa_verify::sources::refresh::{Facts, Refresher};
use liyasa_verify::sources::snapshot::SnapshotLog;
use liyasa_verify::sources::spec::SourceSpec;

const DOCUMENT: &str = r#"{
  "name": "Pro",
  "seats": 1234.5,
  "price": 1234.5,
  "uptime": 99.95,
  "since": "2026-01-31",
  "trial": true,
  "stage": "beta",
  "regions": ["eu", "us"],
  "limits": { "seats": 5 }
}"#;

const DECLARATION: &str = r#"{
  "kind": "file",
  "path": "facts/plan.json",
  "facts": {
    "plan.name": "/name",
    "plan.seats": "/seats",
    "plan.price": "/price",
    "plan.uptime": "/uptime",
    "plan.since": "/since",
    "plan.trial": "/trial",
    "plan.stage": "/stage",
    "plan.regions": "/regions",
    "plan.limits": "/limits"
  },
  "types": {
    "plan.name": "string",
    "plan.seats": "number",
    "plan.price": { "type": "currency", "code": "USD", "minor": 2 },
    "plan.uptime": "percentage",
    "plan.since": "date",
    "plan.trial": "boolean",
    "plan.stage": { "type": "enum", "values": ["ga", "beta"] },
    "plan.regions": "list",
    "plan.limits": "object"
  }
}"#;

const PAGE: &str = "\
name {{ fact(\"plan.name\") }}
number {{ fact(\"plan.seats\") | number(LOCALE) }}
currency {{ fact(\"plan.price\") | currency(\"USD\", LOCALE) }}
percentage {{ fact(\"plan.uptime\") | number(LOCALE) }}%
date {{ fact(\"plan.since\") | date(\"%Y-%m-%d\") }}
boolean {{ fact(\"plan.trial\") }}
enum {{ fact(\"plan.stage\") }}
list {{ fact(\"plan.regions\") | join(\", \") }}
object {{ fact(\"plan.limits\").seats }}
";

/// Every type, in the locale whose separators are `,` and `.`.
///
/// `True` rather than `true` is minijinja rendering a boolean the way Jinja2
/// does; it is the template engine's spelling, not this crate's.
const GOLDEN_EN: &str = "\
name Pro
number 1,234.50
currency $1,234.50
percentage 99.95%
date 2026-01-31
boolean True
enum beta
list eu, us
object 5
";

/// The same facts in a locale that groups with `.`, separates with `,`, and
/// puts the currency symbol after the amount.
const GOLDEN_DE: &str = "\
name Pro
number 1.234,50
currency 1.234,50\u{a0}$
percentage 99,95%
date 2026-01-31
boolean True
enum beta
list eu, us
object 5
";

struct Offline;

impl HttpClient for Offline {
    fn fetch<'a>(
        &'a self,
        _req: HttpRequest,
        _policy: &'a HttpPolicy,
    ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
        Box::pin(std::future::ready(Err(NetError::Io(
            "a file source needs no network".to_owned(),
        ))))
    }
}

fn facts() -> Facts {
    let (spec, problems) = SourceSpec::parse(
        "plan",
        &serde_json::from_str(DECLARATION).expect("a declaration"),
    );
    assert!(problems.is_empty(), "{problems:#?}");
    let vfs = Arc::new(MemoryVfs::new().with("facts/plan.json", DOCUMENT));
    let sources = [DeclaredSource::new(spec).with_vfs(vfs)];
    let log = SnapshotLog::new();
    let report = block_on(Refresher::new(&log, BuildTrust::Trusted, "main").refresh(
        &sources,
        &Offline,
        None,
        std::time::SystemTime::UNIX_EPOCH,
    ));
    assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    report.facts
}

fn rendered(facts: &Facts, locale: &str) -> String {
    let text = PAGE.replace("LOCALE", &format!("\"{locale}\""));
    let mut map = SourceMap::new();
    let id = map.intern(VfsPath::new("page.md"), Arc::from(text.as_str()));
    let (document, diagnostics) = scan(&text, id);
    assert!(!diagnostics.has_errors(), "{diagnostics:#?}");
    let context = Layers {
        facts: facts.as_context(),
        ..Layers::default()
    }
    .build();
    let environment = environment(&ExpandOptions::default());
    expand(&map, &document, &context, &environment)
        .unwrap_or_else(|problems| panic!("{problems:#?}"))
        .text
}

#[test]
fn every_type_survives_the_source_with_the_type_it_was_declared() {
    let facts = facts();
    let value = |id: &str| facts.get(&FactId::new(id)).map(|fact| fact.value.clone());

    assert_eq!(value("plan.name"), Some(FactValue::Str("Pro".to_owned())));
    assert_eq!(value("plan.seats"), Some(FactValue::Num(1234.5)));
    assert_eq!(
        value("plan.price"),
        Some(FactValue::Currency {
            amount: 123_450,
            minor: 2,
            code: "USD".to_owned()
        })
    );
    assert_eq!(value("plan.uptime"), Some(FactValue::Percent(99.95)));
    assert_eq!(
        value("plan.since"),
        Some(FactValue::Date("2026-01-31".to_owned()))
    );
    assert_eq!(value("plan.trial"), Some(FactValue::Bool(true)));
    assert_eq!(
        value("plan.stage"),
        Some(FactValue::Enum("beta".to_owned()))
    );
    assert!(matches!(value("plan.regions"), Some(FactValue::List(_))));
    assert!(matches!(value("plan.limits"), Some(FactValue::Object(_))));
}

#[test]
fn the_typed_filters_render_every_type_in_two_locales() {
    let facts = facts();
    assert_eq!(rendered(&facts, "en-GB"), GOLDEN_EN);
    assert_eq!(rendered(&facts, "de-DE"), GOLDEN_DE);
}
