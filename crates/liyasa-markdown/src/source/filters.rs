//! Liyasa's template filters and functions (CM-14, CM-15).
//!
//! Only the ones that are a pure function of their arguments and the template
//! context live here. `markdown`, `link`, `asset`, `page`, `pages`, `openapi`,
//! `region_available`, `now`, and `snippet` need the content tree, the asset
//! manifest, or the build clock, none of which this crate may reach: it does no
//! I/O so that it builds for WebAssembly (§6.2). The build installs those on
//! the same environment before calling [`expand`](super::expand).
//!
//! `fact` and `env` are here because they are lookups into the template context
//! itself (CM-12), which is also what lets them raise `E0209` and `E0211` with
//! the name that was missing.

use liyasa_core::diagnostics::code;
use minijinja::value::{Kwargs, Value};
use minijinja::{Environment, Error, ErrorKind};

/// Installs every filter and function this crate owns.
pub fn install(env: &mut Environment<'_>) {
    env.add_filter("slugify", slugify_filter);
    env.add_filter("anchor", slugify_filter);
    env.add_filter("truncate_chars", truncate_chars);
    env.add_filter("plural", plural);
    env.add_filter("number", number);
    env.add_filter("currency", currency);
    env.add_filter("date", date);
    env.add_filter("json", to_json);
    env.add_filter("yaml", to_yaml);
    env.add_filter("toml", to_toml);
    env.add_function("fact", fact);
}

/// The diagnostic code a template error carries out of this module.
///
/// minijinja errors have no code of their own, so the code travels in the
/// message where [`describe`](super::expand) can read it back.
fn tagged(code: liyasa_core::Code, message: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::InvalidOperation, format!("{code}: {message}"))
}

// ---- text ----

fn slugify_filter(value: &str) -> String {
    slugify(value)
}

/// The anchor form: lowercase, runs of anything that is not a letter or digit
/// become one hyphen, and the ends are trimmed.
pub fn slugify(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut pending = false;
    for ch in value.chars() {
        if ch.is_alphanumeric() {
            if pending && !out.is_empty() {
                out.push('-');
            }
            pending = false;
            out.extend(ch.to_lowercase());
        } else {
            pending = true;
        }
    }
    out
}

/// Counts characters, not bytes, so a truncation never splits one.
fn truncate_chars(value: &str, length: usize, kwargs: Kwargs) -> Result<String, Error> {
    let ellipsis: String = kwargs
        .get::<Option<String>>("end")?
        .unwrap_or_else(|| "…".to_owned());
    kwargs.assert_all_used()?;
    if value.chars().count() <= length {
        return Ok(value.to_owned());
    }
    let kept = length.saturating_sub(ellipsis.chars().count());
    Ok(value.chars().take(kept).collect::<String>() + &ellipsis)
}

/// `{{ n | plural("file", "files") }}`; the plural defaults to the singular
/// plus `s`.
fn plural(count: i64, one: String, many: Option<String>) -> String {
    if count == 1 {
        one
    } else {
        many.unwrap_or_else(|| format!("{one}s"))
    }
}

// ---- numbers ----

struct Locale {
    group: &'static str,
    decimal: &'static str,
    /// Whether a currency symbol follows the amount.
    suffix: bool,
}

fn locale_of(tag: Option<&str>) -> Locale {
    match tag.and_then(|tag| tag.split(['-', '_']).next()) {
        Some("de" | "es" | "it" | "nl" | "pt" | "id" | "tr") => Locale {
            group: ".",
            decimal: ",",
            suffix: true,
        },
        Some("fr" | "ru" | "pl" | "cs" | "sv" | "nb" | "fi" | "uk") => Locale {
            group: "\u{202f}",
            decimal: ",",
            suffix: true,
        },
        _ => Locale {
            group: ",",
            decimal: ".",
            suffix: false,
        },
    }
}

fn symbol_of(currency: &str) -> String {
    match currency {
        "USD" => "$".to_owned(),
        "EUR" => "€".to_owned(),
        "GBP" => "£".to_owned(),
        "JPY" => "¥".to_owned(),
        "INR" => "₹".to_owned(),
        other => other.to_owned(),
    }
}

fn number(value: Value, locale: Option<String>) -> Result<String, Error> {
    let amount = as_number(&value)
        .ok_or_else(|| tagged(code::E0203, format!("`number` needs a number, got {value}")))?;
    Ok(group(
        amount,
        &locale_of(locale.as_deref()),
        decimals(amount),
    ))
}

fn currency(value: Value, currency: String, locale: Option<String>) -> Result<String, Error> {
    let amount = as_number(&value).ok_or_else(|| {
        tagged(
            code::E0203,
            format!("`currency` needs a number, got {value}"),
        )
    })?;
    let locale = locale_of(locale.as_deref());
    let symbol = symbol_of(&currency);
    let digits = if currency == "JPY" { 0 } else { 2 };
    let text = group(amount, &locale, digits);
    Ok(if locale.suffix {
        format!("{text}\u{a0}{symbol}")
    } else {
        format!("{symbol}{text}")
    })
}

/// minijinja keeps integers and floats apart; both are numbers here.
fn as_number(value: &Value) -> Option<f64> {
    f64::try_from(value.clone()).ok()
}

fn decimals(amount: f64) -> usize {
    if amount.fract() == 0.0 { 0 } else { 2 }
}

fn group(amount: f64, locale: &Locale, digits: usize) -> String {
    let text = format!("{:.*}", digits, amount.abs());
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text.as_str(), None),
    };
    let mut out = String::with_capacity(text.len() + whole.len() / 3 + 2);
    if amount.is_sign_negative() && amount != 0.0 {
        out.push('-');
    }
    for (at, ch) in whole.chars().enumerate() {
        if at > 0 && (whole.len() - at) % 3 == 0 {
            out.push_str(locale.group);
        }
        out.push(ch);
    }
    if let Some(fraction) = fraction {
        out.push_str(locale.decimal);
        out.push_str(fraction);
    }
    out
}

// ---- dates ----

/// Formats an ISO 8601 date or date-time with a strftime subset.
///
/// No calendar crate is in the dependency table (§6.2.1) and the build clock is
/// the only time source that reaches output (§6.6.2), so this reads the fields
/// out of the string rather than converting to a calendar type.
fn date(value: &str, format: Option<String>) -> Result<String, Error> {
    let parts = Date::parse(value)
        .ok_or_else(|| tagged(code::E0203, format!("`date` needs ISO 8601, got `{value}`")))?;
    let format = format.unwrap_or_else(|| "%Y-%m-%d".to_owned());
    let mut out = String::with_capacity(format.len() + 8);
    let mut rest = format.as_str();
    while let Some(at) = rest.find('%') {
        out.push_str(&rest[..at]);
        let Some(directive) = rest[at + 1..].chars().next() else {
            out.push('%');
            rest = "";
            break;
        };
        out.push_str(&parts.render(directive));
        rest = &rest[at + 1 + directive.len_utf8()..];
    }
    out.push_str(rest);
    Ok(out)
}

struct Date {
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

impl Date {
    fn parse(text: &str) -> Option<Self> {
        let (date, time) = match text.split_once(['T', ' ']) {
            Some((date, time)) => (date, Some(time)),
            None => (text, None),
        };
        let mut fields = date.split('-');
        let year = fields.next()?.parse().ok()?;
        let month = fields
            .next()?
            .parse()
            .ok()
            .filter(|m| (1..=12).contains(m))?;
        let day = fields
            .next()?
            .parse()
            .ok()
            .filter(|d| (1..=31).contains(d))?;
        let mut clock = time
            .unwrap_or_default()
            .trim_end_matches('Z')
            .split(':')
            .map(|field| field.split('.').next().unwrap_or(field).parse().ok());
        Some(Self {
            year,
            month,
            day,
            hour: clock.next().flatten().unwrap_or_default(),
            minute: clock.next().flatten().unwrap_or_default(),
            second: clock.next().flatten().unwrap_or_default(),
        })
    }

    /// Zeller's congruence: 0 is Sunday.
    fn weekday(&self) -> usize {
        let (mut month, mut year) = (self.month as i64, self.year);
        if month < 3 {
            month += 12;
            year -= 1;
        }
        let (century, decade) = (year.div_euclid(100), year.rem_euclid(100));
        let day = (self.day as i64
            + (13 * (month + 1)) / 5
            + decade
            + decade / 4
            + century / 4
            + 5 * century)
            .rem_euclid(7);
        // Zeller counts Saturday as 0.
        ((day + 6) % 7) as usize
    }

    fn render(&self, directive: char) -> String {
        match directive {
            'Y' => self.year.to_string(),
            'y' => format!("{:02}", self.year.rem_euclid(100)),
            'm' => format!("{:02}", self.month),
            'd' => format!("{:02}", self.day),
            'e' => self.day.to_string(),
            'H' => format!("{:02}", self.hour),
            'M' => format!("{:02}", self.minute),
            'S' => format!("{:02}", self.second),
            'B' => MONTHS[(self.month - 1) as usize].to_owned(),
            'b' => MONTHS[(self.month - 1) as usize][..3].to_owned(),
            'A' => DAYS[self.weekday()].to_owned(),
            'a' => DAYS[self.weekday()][..3].to_owned(),
            '%' => "%".to_owned(),
            other => format!("%{other}"),
        }
    }
}

// ---- serialization ----

fn to_json(value: Value) -> Result<String, Error> {
    serde_json::to_string_pretty(&value)
        .map_err(|e| tagged(code::E0203, format!("`json` could not serialize: {e}")))
}

fn to_yaml(value: Value) -> Result<String, Error> {
    liyasa_core::yaml::to_string(&value).map_err(|e| {
        tagged(
            code::E0203,
            format!("`yaml` could not serialize: {}", e.message),
        )
    })
}

/// A TOML writer for the shapes documentation actually puts in a fence: a table
/// of scalars, arrays, and nested tables. No TOML crate is in the dependency
/// table (§6.2.1), and adding one for an output filter is not worth a row.
fn to_toml(value: Value) -> Result<String, Error> {
    let json: serde_json::Value = serde_json::to_value(&value)
        .map_err(|e| tagged(code::E0203, format!("`toml` could not serialize: {e}")))?;
    let serde_json::Value::Object(table) = &json else {
        return Err(tagged(code::E0203, "`toml` needs a table at the top level"));
    };
    let mut out = String::new();
    write_toml(table, "", &mut out);
    Ok(out)
}

fn write_toml(table: &serde_json::Map<String, serde_json::Value>, path: &str, out: &mut String) {
    for (key, value) in table {
        if !matches!(value, serde_json::Value::Object(_)) {
            out.push_str(&format!("{key} = {}\n", toml_scalar(value)));
        }
    }
    for (key, value) in table {
        if let serde_json::Value::Object(nested) = value {
            let name = if path.is_empty() {
                key.clone()
            } else {
                format!("{path}.{key}")
            };
            out.push_str(&format!("\n[{name}]\n"));
            write_toml(nested, &name, out);
        }
    }
}

fn toml_scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "\"\"".to_owned(),
        serde_json::Value::Bool(flag) => flag.to_string(),
        serde_json::Value::Number(number) => number.to_string(),
        serde_json::Value::String(text) => format!("{:?}", text),
        serde_json::Value::Array(items) => format!(
            "[{}]",
            items.iter().map(toml_scalar).collect::<Vec<_>>().join(", ")
        ),
        serde_json::Value::Object(_) => "{}".to_owned(),
    }
}

// ---- context lookups ----

/// `fact("plan.pro.price")` reads `facts.*` and records the dependency; the
/// dependency itself is recorded by expansion's syntactic pass (CM-19).
fn fact(state: &minijinja::State<'_, '_>, id: String) -> Result<Value, Error> {
    let mut found = state.lookup("facts").unwrap_or_default();
    for part in id.split('.') {
        found = found.get_attr(part)?;
        if found.is_undefined() {
            return Err(tagged(code::E0209, format!("no fact `{id}`")));
        }
    }
    Ok(found)
}

/// The allow-listed environment variables, reachable both ways.
///
/// CM-12 spells the accessor `env.CI` and CM-15 spells the call `env("CI")`,
/// and minijinja resolves both against the same name: a plain map in the
/// context would shadow a global function and make the call form unreachable.
/// One object answers to both, and a name that is not allow-listed is `E0211`
/// either way.
#[derive(Debug)]
pub struct EnvAccessor(pub Value);

impl EnvAccessor {
    fn read(&self, name: &str) -> Result<Value, Error> {
        let found = self.0.get_attr(name).unwrap_or_default();
        if found.is_undefined() {
            return Err(tagged(
                code::E0211,
                format!("`{name}` is not allow-listed in `build.env`"),
            ));
        }
        Ok(found)
    }
}

impl minijinja::value::Object for EnvAccessor {
    fn get_value(self: &std::sync::Arc<Self>, key: &Value) -> Option<Value> {
        key.as_str().and_then(|name| self.read(name).ok())
    }

    fn enumerate(self: &std::sync::Arc<Self>) -> minijinja::value::Enumerator {
        match self.0.try_iter() {
            Ok(keys) => minijinja::value::Enumerator::Values(keys.collect()),
            Err(_) => minijinja::value::Enumerator::NonEnumerable,
        }
    }

    fn call(
        self: &std::sync::Arc<Self>,
        _state: &minijinja::State<'_, '_>,
        args: &[Value],
    ) -> Result<Value, Error> {
        let [name] = args else {
            return Err(tagged(code::E0211, "`env` takes one variable name"));
        };
        let Some(name) = name.as_str() else {
            return Err(tagged(code::E0211, "`env` takes one variable name"));
        };
        self.read(name)
    }
}

#[cfg(test)]
mod tests {
    use minijinja::context;

    use super::*;

    fn render(template: &str, values: Value) -> String {
        let mut env = Environment::new();
        install(&mut env);
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        env.template_from_str(template)
            .expect("a valid template")
            .render(values)
            .expect("a successful render")
    }

    fn fails(template: &str, values: Value) -> String {
        let mut env = Environment::new();
        install(&mut env);
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        env.template_from_str(template)
            .expect("a valid template")
            .render(values)
            .expect_err("a failure")
            .to_string()
    }

    #[test]
    fn slugify_makes_an_anchor() {
        assert_eq!(slugify("Getting Started!"), "getting-started");
        assert_eq!(slugify("  A — B  "), "a-b");
        assert_eq!(slugify("v2.0 limits"), "v2-0-limits");
        assert_eq!(slugify("café Ünicode"), "café-ünicode");
        assert_eq!(slugify("---"), "");
    }

    #[test]
    fn anchor_is_the_same_rule() {
        assert_eq!(
            render("{{ t | anchor }}", context! { t => "Rate Limits" }),
            "rate-limits"
        );
    }

    #[test]
    fn truncate_chars_counts_characters() {
        assert_eq!(
            render("{{ t | truncate_chars(5) }}", context! { t => "abcdefgh" }),
            "abcd…"
        );
        assert_eq!(
            render("{{ t | truncate_chars(8) }}", context! { t => "abcdefgh" }),
            "abcdefgh"
        );
        assert_eq!(
            render(
                "{{ t | truncate_chars(4, end=\"...\") }}",
                context! { t => "abcdefgh" }
            ),
            "a..."
        );
        assert_eq!(
            render(
                "{{ t | truncate_chars(3) }}",
                context! { t => "日本語です" }
            ),
            "日本…"
        );
    }

    #[test]
    fn plural_picks_a_form() {
        assert_eq!(
            render("{{ n | plural(\"file\") }}", context! { n => 1 }),
            "file"
        );
        assert_eq!(
            render("{{ n | plural(\"file\") }}", context! { n => 2 }),
            "files"
        );
        assert_eq!(
            render(
                "{{ n | plural(\"entry\", \"entries\") }}",
                context! { n => 0 }
            ),
            "entries"
        );
    }

    #[test]
    fn number_groups_by_locale() {
        assert_eq!(
            render("{{ n | number }}", context! { n => 1234567 }),
            "1,234,567"
        );
        assert_eq!(
            render("{{ n | number(\"de\") }}", context! { n => 1234567 }),
            "1.234.567"
        );
        assert_eq!(
            render("{{ n | number }}", context! { n => 1234.5 }),
            "1,234.50"
        );
        assert_eq!(
            render("{{ n | number(\"de-DE\") }}", context! { n => 1234.5 }),
            "1.234,50"
        );
        assert_eq!(
            render("{{ n | number }}", context! { n => -1234 }),
            "-1,234"
        );
    }

    #[test]
    fn currency_places_the_symbol_by_locale() {
        assert_eq!(
            render("{{ n | currency(\"USD\") }}", context! { n => 1234.5 }),
            "$1,234.50"
        );
        assert_eq!(
            render(
                "{{ n | currency(\"EUR\", \"de\") }}",
                context! { n => 1234.5 }
            ),
            "1.234,50\u{a0}€"
        );
        assert_eq!(
            render("{{ n | currency(\"JPY\") }}", context! { n => 1234 }),
            "¥1,234"
        );
        assert_eq!(
            render("{{ n | currency(\"CHF\") }}", context! { n => 10 }),
            "CHF10.00"
        );
    }

    #[test]
    fn a_number_filter_on_text_is_reported() {
        assert!(fails("{{ t | number }}", context! { t => "x" }).contains("E0203"));
    }

    #[test]
    fn date_formats_iso_input() {
        let values = context! { d => "2026-09-15" };
        assert_eq!(render("{{ d | date }}", values.clone()), "2026-09-15");
        assert_eq!(
            render("{{ d | date(\"%e %B %Y\") }}", values.clone()),
            "15 September 2026"
        );
        assert_eq!(
            render("{{ d | date(\"%b %y\") }}", values.clone()),
            "Sep 26"
        );
        assert_eq!(render("{{ d | date(\"%A\") }}", values), "Tuesday");
    }

    #[test]
    fn date_reads_the_clock_of_a_date_time() {
        let values = context! { d => "2026-09-15T08:04:02Z" };
        assert_eq!(render("{{ d | date(\"%H:%M:%S\") }}", values), "08:04:02");
    }

    #[test]
    fn a_percent_sign_escapes_itself() {
        assert_eq!(
            render("{{ d | date(\"%Y%%\") }}", context! { d => "2026-01-02" }),
            "2026%"
        );
    }

    #[test]
    fn a_date_that_is_not_iso_is_reported() {
        assert!(fails("{{ d | date }}", context! { d => "yesterday" }).contains("E0203"));
    }

    #[test]
    fn json_yaml_and_toml_serialize() {
        let values = context! { v => context! { name => "A", count => 2 } };
        assert!(render("{{ v | json }}", values.clone()).contains("\"name\": \"A\""));
        assert!(render("{{ v | yaml }}", values.clone()).contains("name: A"));
        let toml = render("{{ v | toml }}", values);
        assert!(toml.contains("name = \"A\""), "{toml}");
        assert!(toml.contains("count = 2"), "{toml}");
    }

    #[test]
    fn toml_writes_nested_tables_after_the_scalars() {
        let values = context! { v => context! {
            title => "Docs",
            build => context! { command => "liyasa build" },
        } };
        let toml = render("{{ v | toml }}", values);
        assert!(
            toml.find("title = ")
                .is_some_and(|at| at < toml.find("[build]").unwrap_or(usize::MAX)),
            "{toml}"
        );
        assert!(toml.contains("command = \"liyasa build\""), "{toml}");
    }

    #[test]
    fn fact_reads_a_dotted_path() {
        let values = context! { facts => context! {
            plan => context! { pro => context! { price => 99 } },
        } };
        assert_eq!(render("{{ fact(\"plan.pro.price\") }}", values), "99");
    }

    #[test]
    fn a_missing_fact_is_reported() {
        let values = context! { facts => context! { plan => context! { pro => 1 } } };
        assert!(fails("{{ fact(\"plan.free.price\") }}", values).contains("E0209"));
    }

    fn with_env(pairs: Value) -> Value {
        context! { env => Value::from_object(EnvAccessor(pairs)) }
    }

    #[test]
    fn env_reads_the_allow_list_both_ways() {
        let values = with_env(context! { CI => "true" });
        assert_eq!(render("{{ env(\"CI\") }}", values.clone()), "true");
        assert_eq!(render("{{ env.CI }}", values), "true");
    }

    #[test]
    fn a_variable_outside_the_allow_list_is_reported() {
        let values = with_env(context! { CI => "true" });
        assert!(fails("{{ env(\"SECRET\") }}", values).contains("E0211"));
    }
}
