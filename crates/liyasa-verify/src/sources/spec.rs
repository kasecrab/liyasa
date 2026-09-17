//! What a `verify.sources.<id>` declaration says (RFC 2030).
//!
//! `schemas/liyasa.schema.json` types every entry under `verify.sources` as an
//! open object, so this module is where one gets its meaning: which fields name
//! the document, how the document's contents become facts, and how a fact gets
//! the type JSON cannot carry. Reading is lenient per source, the way
//! [`VerifyConfig`](crate::core::config::VerifyConfig) is lenient per key: a
//! declaration Liyasa cannot read is `E0635` on that source and the rest of the
//! set still applies.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::{Diagnostic, code};
use liyasa_core::ids::FactId;
use liyasa_core::verify::{FactValue, SourceKind};
use serde_json::Value;

/// The type a project says a fact has. JSON carries five of the nine VER-20
/// types on its own; the other four exist only because someone wrote them down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactType {
    Str,
    Num,
    /// `FactValue::Currency` holds minor units, so a JSON number has to be
    /// scaled to them — and whether it already is depends on the document.
    /// `minor_units` is that answer: `false` reads `20` as `$20.00`, `true`
    /// reads `2000` as `$20.00`. An API field called `price_cents` is the
    /// second, and getting it wrong is a hundredfold error that renders.
    Currency {
        code: String,
        minor: u8,
        minor_units: bool,
    },
    Percent,
    Date,
    Bool,
    Enum(Vec<String>),
    List,
    Object,
}

impl FactType {
    fn named(name: &str) -> Option<Self> {
        Some(match name {
            "string" => Self::Str,
            "number" => Self::Num,
            "currency" => Self::Currency {
                code: String::new(),
                minor: 2,
                minor_units: false,
            },
            "percentage" | "percent" => Self::Percent,
            "date" => Self::Date,
            "boolean" => Self::Bool,
            "enum" => Self::Enum(Vec::new()),
            "list" => Self::List,
            "object" => Self::Object,
            _ => return None,
        })
    }

    /// The type a value has when nobody declared one. The four types JSON
    /// cannot express are never inferred.
    pub fn inferred(value: &Value) -> Option<Self> {
        Some(match value {
            Value::String(_) => Self::Str,
            Value::Number(_) => Self::Num,
            Value::Bool(_) => Self::Bool,
            Value::Array(_) => Self::List,
            Value::Object(_) => Self::Object,
            Value::Null => return None,
        })
    }

    /// The value as this type, or what is wrong with it. The caller turns the
    /// message into `E0605`: a value that does not fit its declared type is the
    /// same class of failure as one that does not fit its schema.
    pub fn coerce(&self, value: &Value) -> Result<FactValue, String> {
        match self {
            Self::Str => match value {
                Value::String(text) => Ok(FactValue::Str(text.clone())),
                other => Err(mismatch("a string", other)),
            },
            Self::Num => number(value).map(FactValue::Num),
            Self::Currency {
                code,
                minor,
                minor_units,
            } => {
                let scale = if *minor_units {
                    1.0
                } else {
                    10f64.powi(i32::from(*minor))
                };
                let amount = (number(value)? * scale).round();
                if !amount.is_finite() || amount.abs() > i64::MAX as f64 {
                    return Err(format!("{value} is too large to be a currency amount"));
                }
                Ok(FactValue::Currency {
                    amount: amount as i64,
                    minor: *minor,
                    code: code.clone(),
                })
            }
            Self::Percent => number(value).map(FactValue::Percent),
            Self::Date => match value {
                Value::String(text) if is_iso_8601(text) => Ok(FactValue::Date(text.clone())),
                Value::String(text) => Err(format!("`{text}` is not an ISO 8601 date")),
                other => Err(mismatch("an ISO 8601 date", other)),
            },
            Self::Bool => match value {
                Value::Bool(flag) => Ok(FactValue::Bool(*flag)),
                other => Err(mismatch("a boolean", other)),
            },
            Self::Enum(allowed) => match value {
                Value::String(text) if allowed.is_empty() || allowed.contains(text) => {
                    Ok(FactValue::Enum(text.clone()))
                }
                Value::String(text) => Err(format!(
                    "`{text}` is not one of {}",
                    allowed
                        .iter()
                        .map(|v| format!("`{v}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
                other => Err(mismatch("one of the declared values", other)),
            },
            Self::List => match value {
                Value::Array(items) => Ok(FactValue::List(
                    items
                        .iter()
                        .map(|item| {
                            FactType::inferred(item)
                                .ok_or_else(|| mismatch("a value", item))
                                .and_then(|kind| kind.coerce(item))
                        })
                        .collect::<Result<_, _>>()?,
                )),
                other => Err(mismatch("a list", other)),
            },
            Self::Object => match value {
                Value::Object(fields) => Ok(FactValue::Object(
                    fields
                        .iter()
                        .map(|(key, field)| {
                            FactType::inferred(field)
                                .ok_or_else(|| mismatch("a value", field))
                                .and_then(|kind| kind.coerce(field))
                                .map(|coerced| (key.clone(), coerced))
                        })
                        .collect::<Result<_, _>>()?,
                )),
                other => Err(mismatch("an object", other)),
            },
        }
    }
}

fn number(value: &Value) -> Result<f64, String> {
    value.as_f64().ok_or_else(|| mismatch("a number", value))
}

fn mismatch(wanted: &str, got: &Value) -> String {
    format!("expected {wanted}, got {got}")
}

/// Enough of RFC 3339 to tell a date from a string that is not one. The `date`
/// filter parses the rest; this only decides whether the value may carry the
/// type.
fn is_iso_8601(text: &str) -> bool {
    let date = text.split(['T', ' ']).next().unwrap_or(text);
    let mut parts = date.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let digits =
        |part: &str, width: usize| part.len() == width && part.bytes().all(|b| b.is_ascii_digit());
    digits(year, 4) && digits(month, 2) && digits(day, 2)
}

/// Where a source's credential comes from and how it is sent (VER-25).
///
/// The value itself never appears in a declaration: `secret` is the *name* the
/// secret store or the environment knows it by, resolved through
/// [`SecretSource`](liyasa_core::verify::SecretSource) at refresh time so a
/// repository never carries one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceAuth {
    pub secret: String,
    /// The header a `url` or `openapi` source sends it in. A `command` source
    /// has no headers and passes the value as an environment variable named
    /// after the secret.
    pub header: String,
    /// `{}` is replaced by the secret's value.
    pub format: String,
}

impl Default for SourceAuth {
    fn default() -> Self {
        Self {
            secret: String::new(),
            header: "Authorization".to_owned(),
            format: "Bearer {}".to_owned(),
        }
    }
}

/// One declared truth source.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpec {
    pub id: String,
    pub kind: SourceKind,
    /// The document, or the script a `command` source runs.
    pub path: Option<String>,
    pub url: Option<String>,
    /// `command` only: argv. `path` is what the allow list is checked against.
    pub command: Vec<String>,
    /// `manual` only: the attested values, as written.
    pub values: BTreeMap<FactId, Value>,
    pub owner: Option<String>,
    pub expires: Option<String>,
    /// Fact ID → RFC 6901 pointer into the document. Empty means "flatten".
    pub facts: BTreeMap<FactId, String>,
    pub types: BTreeMap<FactId, FactType>,
    /// A JSON Schema the document must satisfy before any fact is read.
    pub schema: Option<Value>,
    /// Hex SHA-256 of the expected certificate or public key (VER-26).
    pub pin: Option<String>,
    pub auth: Option<SourceAuth>,
}

/// A declaration that failed to say its `kind` still parses, so the rest of its
/// fields can be reported too; `file` is the kind it stands as until then, and
/// the missing `kind` is already `E0635`.
impl Default for SourceSpec {
    fn default() -> Self {
        Self {
            id: String::new(),
            kind: SourceKind::File,
            path: None,
            url: None,
            command: Vec::new(),
            values: BTreeMap::new(),
            owner: None,
            expires: None,
            facts: BTreeMap::new(),
            types: BTreeMap::new(),
            schema: None,
            pin: None,
            auth: None,
        }
    }
}

impl SourceSpec {
    /// Reads one declaration. Errors are about this source alone.
    pub fn parse(id: &str, value: &Value) -> (Self, Vec<Diagnostic>) {
        let mut problems = Vec::new();
        let mut out = Self {
            id: id.to_owned(),
            ..Self::default()
        };
        let Some(object) = value.as_object() else {
            problems.push(bad(id, "kind", "an object", value));
            return (out, problems);
        };

        match object.get("kind") {
            Some(Value::String(name)) => match kind_of(name) {
                Some(kind) => out.kind = kind,
                None => problems.push(bad(
                    id,
                    "kind",
                    "a source kind",
                    &Value::String(name.clone()),
                )),
            },
            Some(other) => problems.push(bad(id, "kind", "a source kind", other)),
            None => problems.push(
                Diagnostic::new(
                    code::E0635,
                    format!("`verify.sources.{id}` does not say what `kind` of source it is"),
                )
                .help("one of `file`, `repo`, `url`, `openapi`, `command`, `manual`, `screenshot`"),
            ),
        }

        for (key, target) in [
            ("path", &mut out.path),
            ("url", &mut out.url),
            ("owner", &mut out.owner),
            ("expires", &mut out.expires),
            ("pin", &mut out.pin),
        ] {
            match object.get(key) {
                Some(Value::String(text)) => *target = Some(text.clone()),
                Some(other) => problems.push(bad(id, key, "a string", other)),
                None => {}
            }
        }

        match object.get("command") {
            Some(Value::Array(argv)) => {
                for item in argv {
                    match item {
                        Value::String(text) => out.command.push(text.clone()),
                        other => problems.push(bad(id, "command", "a list of strings", other)),
                    }
                }
            }
            Some(other) => problems.push(bad(id, "command", "a list of strings", other)),
            None => {}
        }

        match object.get("facts") {
            Some(Value::Object(map)) => {
                for (fact, pointer) in map {
                    match pointer {
                        Value::String(text) => {
                            out.facts.insert(FactId::new(fact.clone()), text.clone());
                        }
                        other => problems.push(bad(id, "facts", "a JSON pointer", other)),
                    }
                }
            }
            Some(other) => problems.push(bad(id, "facts", "an object", other)),
            None => {}
        }

        match object.get("values") {
            Some(Value::Object(map)) => {
                for (fact, value) in map {
                    out.values.insert(FactId::new(fact.clone()), value.clone());
                }
            }
            Some(other) => problems.push(bad(id, "values", "an object", other)),
            None => {}
        }

        match object.get("types") {
            Some(Value::Object(map)) => {
                for (fact, declared) in map {
                    match fact_type(declared) {
                        Ok(kind) => {
                            out.types.insert(FactId::new(fact.clone()), kind);
                        }
                        Err(why) => problems.push(Diagnostic::new(
                            code::E0635,
                            format!("`verify.sources.{id}.types.{fact}`: {why}"),
                        )),
                    }
                }
            }
            Some(other) => problems.push(bad(id, "types", "an object", other)),
            None => {}
        }

        if let Some(declared) = object.get("auth") {
            match source_auth(declared) {
                Ok(auth) => out.auth = Some(auth),
                Err(why) => problems.push(Diagnostic::new(
                    code::E0635,
                    format!("`verify.sources.{id}.auth`: {why}"),
                )),
            }
        }

        if let Some(schema) = object.get("schema") {
            out.schema = Some(schema.clone());
        }

        problems.extend(out.lint());
        (out, problems)
    }

    /// The facts this source is declared to produce. A source that flattens its
    /// document produces whatever is in it, which is not knowable until it is
    /// fetched, so this is empty for one of those.
    pub fn declared_facts(&self) -> Vec<FactId> {
        if self.kind == SourceKind::Manual {
            return self.values.keys().cloned().collect();
        }
        self.facts.keys().cloned().collect()
    }

    /// Declarations that are well-formed JSON and still unusable.
    fn lint(&self) -> Vec<Diagnostic> {
        let mut problems = Vec::new();
        let id = &self.id;
        let missing = |field: &str| {
            Diagnostic::new(
                code::E0635,
                format!(
                    "`verify.sources.{id}` is a `{}` source and has no `{field}`",
                    kind_name(self.kind)
                ),
            )
        };
        match self.kind {
            SourceKind::File | SourceKind::Repo => {
                if self.path.is_none() {
                    problems.push(missing("path"));
                }
            }
            SourceKind::Url | SourceKind::Screenshot => {
                if self.url.is_none() {
                    problems.push(missing("url"));
                }
            }
            SourceKind::OpenApi => {
                if self.url.is_none() && self.path.is_none() {
                    problems.push(missing("url` or `path"));
                }
            }
            SourceKind::Command => {
                if self.command.is_empty() {
                    problems.push(missing("command"));
                }
                if self.path.is_none() {
                    problems.push(
                        missing("path")
                            .help("`path` is the script the server's allow list names and hashes"),
                    );
                }
            }
            SourceKind::Manual => {
                if self.owner.is_none() {
                    problems.push(missing("owner"));
                }
                if self.expires.is_none() {
                    problems.push(missing("expires"));
                }
                if self.values.is_empty() {
                    problems.push(missing("values"));
                }
            }
            // `SourceKind` is `#[non_exhaustive]`: a kind added to the contract
            // has no required fields here until this match names it.
            _ => {}
        }
        if let Some(expires) = &self.expires
            && !is_iso_8601(expires)
        {
            problems.push(Diagnostic::new(
                code::E0635,
                format!("`verify.sources.{id}.expires` is `{expires}`, not an RFC 3339 date"),
            ));
        }
        if let Some(pin) = &self.pin
            && !(pin.len() == 64 && pin.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            problems.push(Diagnostic::new(
                code::E0635,
                format!("`verify.sources.{id}.pin` is not a hex SHA-256 digest"),
            ));
        }
        problems
    }
}

fn fact_type(value: &Value) -> Result<FactType, String> {
    match value {
        Value::String(name) => {
            FactType::named(name).ok_or_else(|| format!("`{name}` is not a fact type"))
        }
        Value::Object(fields) => {
            let Some(Value::String(name)) = fields.get("type") else {
                return Err("no `type`".to_owned());
            };
            let base =
                FactType::named(name).ok_or_else(|| format!("`{name}` is not a fact type"))?;
            Ok(match base {
                FactType::Currency { .. } => {
                    let Some(Value::String(code)) = fields.get("code") else {
                        return Err("a currency needs an ISO 4217 `code`".to_owned());
                    };
                    let minor = match fields.get("minor") {
                        None => 2,
                        Some(Value::Number(n)) => n
                            .as_u64()
                            .filter(|digits| *digits <= 4)
                            .ok_or_else(|| format!("`minor` is {n}, not 0 to 4"))?
                            as u8,
                        Some(other) => return Err(format!("`minor` is {other}, not a number")),
                    };
                    FactType::Currency {
                        code: code.clone(),
                        minor,
                        minor_units: matches!(fields.get("minorUnits"), Some(Value::Bool(true))),
                    }
                }
                FactType::Enum(_) => {
                    let Some(Value::Array(values)) = fields.get("values") else {
                        return Err("an enum needs its `values`".to_owned());
                    };
                    FactType::Enum(
                        values
                            .iter()
                            .map(|v| match v {
                                Value::String(text) => Ok(text.clone()),
                                other => Err(format!("`values` holds {other}, not a string")),
                            })
                            .collect::<Result<_, _>>()?,
                    )
                }
                other => other,
            })
        }
        other => Err(format!("{other} is not a fact type")),
    }
}

/// `"secret:NAME"`, `"env:NAME"`, or the object form with a header and a
/// format. Both prefixes resolve through the same `SecretSource`: which of the
/// two a name lives in is the caller's to know, not a declaration's.
fn source_auth(value: &Value) -> Result<SourceAuth, String> {
    match value {
        Value::String(text) => {
            let name = text
                .strip_prefix("secret:")
                .or_else(|| text.strip_prefix("env:"))
                .ok_or_else(|| format!("`{text}` is not `secret:<name>` or `env:<name>`"))?;
            if name.is_empty() {
                return Err("names no secret".to_owned());
            }
            Ok(SourceAuth {
                secret: name.to_owned(),
                ..SourceAuth::default()
            })
        }
        Value::Object(fields) => {
            let secret = match (fields.get("secret"), fields.get("env")) {
                (Some(Value::String(name)), _) | (None, Some(Value::String(name))) => name.clone(),
                _ => return Err("has no `secret` or `env`".to_owned()),
            };
            let text = |key: &str, fallback: &str| match fields.get(key) {
                Some(Value::String(value)) => Ok(value.clone()),
                Some(other) => Err(format!("`{key}` is {other}, not a string")),
                None => Ok(fallback.to_owned()),
            };
            let default = SourceAuth::default();
            let format = text("format", &default.format)?;
            if !format.contains("{}") {
                return Err(format!(
                    "`format` is `{format}`, which has no `{{}}` to put the value in"
                ));
            }
            Ok(SourceAuth {
                secret,
                header: text("header", &default.header)?,
                format,
            })
        }
        other => Err(format!("{other} is not a credential")),
    }
}

fn kind_of(name: &str) -> Option<SourceKind> {
    Some(match name {
        "file" => SourceKind::File,
        "repo" => SourceKind::Repo,
        "url" => SourceKind::Url,
        "openapi" => SourceKind::OpenApi,
        "command" => SourceKind::Command,
        "screenshot" => SourceKind::Screenshot,
        "manual" => SourceKind::Manual,
        _ => return None,
    })
}

pub fn kind_name(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::File => "file",
        SourceKind::Repo => "repo",
        SourceKind::Url => "url",
        SourceKind::OpenApi => "openapi",
        SourceKind::Command => "command",
        SourceKind::Screenshot => "screenshot",
        SourceKind::Manual => "manual",
        _ => "source",
    }
}

fn bad(id: &str, field: &str, wanted: &str, got: &Value) -> Diagnostic {
    Diagnostic::new(
        code::E0635,
        format!("`verify.sources.{id}.{field}` is {got}, not {wanted}"),
    )
}

/// Every declared source, and the fact → source index VER-21 needs (RFC 2031).
#[derive(Debug, Clone, Default)]
pub struct SourceSet {
    specs: BTreeMap<String, SourceSpec>,
    owners: BTreeMap<FactId, String>,
}

impl SourceSet {
    /// Reads the whole `verify.sources` object.
    ///
    /// `commands` is VER-25's allow list rather than a source, and it lives in
    /// this object, so it is the one key that never names one.
    pub fn parse(value: &Value) -> (Self, Vec<Diagnostic>) {
        let mut out = Self::default();
        let mut problems = Vec::new();
        let Some(object) = value.as_object() else {
            if !value.is_null() {
                problems.push(Diagnostic::new(
                    code::E0635,
                    format!("`verify.sources` is {value}, not an object"),
                ));
            }
            return (out, problems);
        };
        for (id, declaration) in object {
            if id == "commands" {
                continue;
            }
            let (spec, mut found) = SourceSpec::parse(id, declaration);
            problems.append(&mut found);
            problems.extend(out.insert(spec));
        }
        (out, problems)
    }

    fn insert(&mut self, spec: SourceSpec) -> Vec<Diagnostic> {
        let mut problems = Vec::new();
        for fact in spec.declared_facts() {
            if let Some(first) = self.owners.get(&fact) {
                problems.push(Diagnostic::new(
                    code::E0635,
                    format!(
                        "fact `{fact}` is declared by both `{first}` and `{}`, so it has no one source",
                        spec.id
                    ),
                ));
                continue;
            }
            self.owners.insert(fact, spec.id.clone());
        }
        self.specs.insert(spec.id.clone(), spec);
        problems
    }

    pub fn get(&self, id: &str) -> Option<&SourceSpec> {
        self.specs.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &SourceSpec> {
        self.specs.values()
    }

    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// VER-21's second half: the source a fact came from. `None` for a fact no
    /// declaration produces — a fact table on disk with no `verify.sources`
    /// entry has no source, and inventing one would be a lie (RFC 2031).
    pub fn source_of(&self, fact: &FactId) -> Option<&str> {
        self.owners.get(fact).map(String::as_str)
    }

    /// The facts a source produces, for the reverse direction: a changed
    /// source reaches the blocks that read its facts.
    pub fn facts_of(&self, source: &str) -> Vec<FactId> {
        self.specs
            .get(source)
            .map(SourceSpec::declared_facts)
            .unwrap_or_default()
    }
}

/// The document's scalars as facts, keyed by dotted path (RFC 2030).
///
/// Used by a source with no `facts` map. An array index is a path segment, so
/// `{"a": [1]}` is `a.0`.
pub fn flatten(document: &Value) -> BTreeMap<FactId, Value> {
    let mut out = BTreeMap::new();
    walk(document, &mut String::new(), &mut out);
    out
}

fn walk(value: &Value, path: &mut String, out: &mut BTreeMap<FactId, Value>) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields {
                let at = path.len();
                if at != 0 {
                    path.push('.');
                }
                path.push_str(key);
                walk(field, path, out);
                path.truncate(at);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                let at = path.len();
                if at != 0 {
                    path.push('.');
                }
                path.push_str(&index.to_string());
                walk(item, path, out);
                path.truncate(at);
            }
        }
        scalar => {
            if !path.is_empty() {
                out.insert(FactId::new(path.clone()), scalar.clone());
            }
        }
    }
}

/// The facts one fetched document yields under one declaration.
///
/// With a `facts` map, exactly those facts: a pointer that resolves to nothing
/// is an error naming the pointer, because a mapping that silently produces no
/// fact is how a page ends up referencing a fact that does not exist. Without
/// one, every scalar leaf.
pub fn read_facts(
    spec: &SourceSpec,
    document: &Value,
) -> Result<BTreeMap<FactId, FactValue>, String> {
    let raw: BTreeMap<FactId, Value> = if spec.facts.is_empty() {
        flatten(document)
    } else {
        let mut out = BTreeMap::new();
        for (fact, pointer) in &spec.facts {
            let found = document
                .pointer(pointer)
                .ok_or_else(|| format!("`{pointer}` (fact `{fact}`) is not in the document"))?;
            out.insert(fact.clone(), found.clone());
        }
        out
    };
    let mut values = BTreeMap::new();
    for (fact, value) in raw {
        let kind = match spec.types.get(&fact) {
            Some(declared) => declared.clone(),
            None => match FactType::inferred(&value) {
                Some(inferred) => inferred,
                // A JSON null is the absence of a value, not a fact.
                None => continue,
            },
        };
        let coerced = kind
            .coerce(&value)
            .map_err(|why| format!("fact `{fact}`: {why}"))?;
        values.insert(fact, coerced);
    }
    Ok(values)
}

#[cfg(test)]
mod tests;
