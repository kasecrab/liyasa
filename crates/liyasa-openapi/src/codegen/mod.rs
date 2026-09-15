//! Request sample generation (API-30, API-31).
//!
//! One generator per language, each a minijinja template over the same
//! [`Request`](crate::sample::Request), so a new language is a template and a
//! registry row rather than code. An operator overrides any of them by name;
//! `x-codeSamples` on an operation turns generation off for it and shows what
//! the spec wrote instead (API-31).

pub mod corpus;

use std::collections::BTreeMap;

use minijinja::{Environment, Error, ErrorKind, Value as JinjaValue};
use serde::Serialize;

use crate::model::{CodeSample, OperationRef, Spec};
use crate::sample::{Options, Request};

/// One language's generator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Generator {
    /// How config and `x-codeSamples` name this language.
    pub id: &'static str,
    /// What the selector shows.
    pub label: &'static str,
    /// The fence language the sample is highlighted as.
    pub highlight: &'static str,
    /// Where the template lives, relative to the crate root.
    pub template_path: &'static str,
    /// The corpus cases this generator is asserted against (API-30).
    pub corpus: &'static [&'static str],
}

/// Every case in [`corpus`], which every generator is run over.
const FULL_CORPUS: &[&str] = corpus::CASES;

/// The registry. The order here is the order the selector shows when a site
/// does not configure one.
pub const GENERATORS: &[Generator] = &[
    Generator {
        id: "curl",
        label: "cURL",
        highlight: "bash",
        template_path: "src/codegen/templates/curl.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "javascript",
        label: "JavaScript",
        highlight: "javascript",
        template_path: "src/codegen/templates/javascript.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "python",
        label: "Python",
        highlight: "python",
        template_path: "src/codegen/templates/python-requests.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "go",
        label: "Go",
        highlight: "go",
        template_path: "src/codegen/templates/go.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "node",
        label: "Node (axios)",
        highlight: "javascript",
        template_path: "src/codegen/templates/node-axios.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "python-httpx",
        label: "Python (httpx)",
        highlight: "python",
        template_path: "src/codegen/templates/python-httpx.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "ruby",
        label: "Ruby",
        highlight: "ruby",
        template_path: "src/codegen/templates/ruby.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "php",
        label: "PHP",
        highlight: "php",
        template_path: "src/codegen/templates/php.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "java",
        label: "Java",
        highlight: "java",
        template_path: "src/codegen/templates/java.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "csharp",
        label: "C#",
        highlight: "csharp",
        template_path: "src/codegen/templates/csharp.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "rust",
        label: "Rust",
        highlight: "rust",
        template_path: "src/codegen/templates/rust.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "kotlin",
        label: "Kotlin",
        highlight: "kotlin",
        template_path: "src/codegen/templates/kotlin.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "swift",
        label: "Swift",
        highlight: "swift",
        template_path: "src/codegen/templates/swift.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "elixir",
        label: "Elixir",
        highlight: "elixir",
        template_path: "src/codegen/templates/elixir.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "httpie",
        label: "HTTPie",
        highlight: "bash",
        template_path: "src/codegen/templates/httpie.jinja",
        corpus: FULL_CORPUS,
    },
    Generator {
        id: "powershell",
        label: "PowerShell",
        highlight: "powershell",
        template_path: "src/codegen/templates/powershell.jinja",
        corpus: FULL_CORPUS,
    },
];

/// The languages a site offers when it configures none: the four that every
/// API's readers reach for first.
pub const DEFAULT_LANGUAGES: &[&str] = &["curl", "javascript", "python", "go"];

const SOURCES: &[(&str, &str)] = &[
    ("curl", include_str!("templates/curl.jinja")),
    ("javascript", include_str!("templates/javascript.jinja")),
    ("python", include_str!("templates/python-requests.jinja")),
    ("go", include_str!("templates/go.jinja")),
    ("node", include_str!("templates/node-axios.jinja")),
    ("python-httpx", include_str!("templates/python-httpx.jinja")),
    ("ruby", include_str!("templates/ruby.jinja")),
    ("php", include_str!("templates/php.jinja")),
    ("java", include_str!("templates/java.jinja")),
    ("csharp", include_str!("templates/csharp.jinja")),
    ("rust", include_str!("templates/rust.jinja")),
    ("kotlin", include_str!("templates/kotlin.jinja")),
    ("swift", include_str!("templates/swift.jinja")),
    ("elixir", include_str!("templates/elixir.jinja")),
    ("httpie", include_str!("templates/httpie.jinja")),
    ("powershell", include_str!("templates/powershell.jinja")),
];

pub fn generator(id: &str) -> Option<&'static Generator> {
    GENERATORS.iter().find(|g| g.id == id)
}

/// One rendered sample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sample {
    pub language: String,
    pub label: String,
    pub highlight: String,
    pub source: String,
    /// True when the spec supplied it through `x-codeSamples` rather than
    /// Liyasa generating it (API-31).
    pub from_spec: bool,
}

/// The templates, compiled once per site.
pub struct Registry {
    environment: Environment<'static>,
    /// An operator's replacement for a generator's template, by language id.
    overrides: BTreeMap<String, String>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl Registry {
    pub fn new() -> Self {
        let mut environment = Environment::new();
        environment.add_filter("shell", shell);
        environment.add_filter("dquote", dquote);
        environment.add_filter("php_string", php_string);
        environment.add_filter("python_value", python_value);
        for (id, source) in SOURCES {
            // The sources are compiled into the binary, so a failure here is a
            // bug in this crate rather than in anyone's input.
            if let Err(error) = environment.add_template(id, source) {
                unreachable!("built-in template `{id}` does not compile: {error}");
            }
        }
        Self {
            environment,
            overrides: BTreeMap::new(),
        }
    }

    /// Replaces one language's template with an operator's (API-30).
    pub fn override_template(&mut self, id: &str, source: String) -> Result<(), String> {
        self.overrides.insert(id.to_owned(), source);
        let source = self
            .overrides
            .get(id)
            .map(String::as_str)
            .unwrap_or_default();
        // `add_template_owned` keeps the environment's lifetime out of the
        // caller's hands; the map above owns the text.
        self.environment
            .add_template_owned(id.to_owned(), source.to_owned())
            .map_err(|error| error.to_string())
    }

    /// Renders one language's sample for a request.
    pub fn render(&self, language: &str, request: &Request) -> Result<Sample, String> {
        let generator =
            generator(language).ok_or_else(|| format!("no generator for `{language}`"))?;
        let template = self
            .environment
            .get_template(language)
            .map_err(|error| error.to_string())?;
        let source = template
            .render(minijinja::context! { req => JinjaValue::from_serialize(request) })
            .map_err(|error| error.to_string())?;
        Ok(Sample {
            language: generator.id.to_owned(),
            label: generator.label.to_owned(),
            highlight: generator.highlight.to_owned(),
            source: source.trim().to_owned(),
            from_spec: false,
        })
    }

    /// Every sample for one operation, in the site's configured order.
    ///
    /// `x-codeSamples` replaces the generated set entirely rather than adding
    /// to it: a spec that ships its own samples has decided what a reader
    /// should copy (API-31).
    pub fn samples(
        &self,
        spec: &Spec,
        operation: &OperationRef<'_>,
        languages: &[String],
        options: &Options,
    ) -> Vec<Sample> {
        if !operation.operation.code_samples.is_empty() {
            return operation
                .operation
                .code_samples
                .iter()
                .map(from_spec)
                .collect();
        }
        let wanted = if operation.operation.liyasa.code_samples.is_empty() {
            languages.to_vec()
        } else {
            operation.operation.liyasa.code_samples.clone()
        };
        let request = Request::build(spec, operation, options);
        wanted
            .iter()
            .filter_map(|language| self.render(language, &request).ok())
            .collect()
    }
}

fn from_spec(sample: &CodeSample) -> Sample {
    let known = generator(&sample.lang);
    Sample {
        language: sample.lang.clone(),
        label: sample
            .label
            .clone()
            .or_else(|| known.map(|g| g.label.to_owned()))
            .unwrap_or_else(|| sample.lang.clone()),
        highlight: known.map_or(sample.lang.clone(), |g| g.highlight.to_owned()),
        source: sample.source.clone(),
        from_spec: true,
    }
}

// ---- filters ----

/// A POSIX shell single-quoted word, which is the only quoting that needs no
/// knowledge of what is inside it.
fn shell(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// A double-quoted string for the C-family languages, with the escapes they
/// all agree on.
fn dquote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// PHP's double-quoted string, where `$` starts an interpolation and so is
/// escaped: the placeholders are `$ACCESS_TOKEN` and friends.
fn php_string(text: &str) -> String {
    dquote(text).replace('$', "\\$")
}

/// JSON text as a Python literal, so a sample is `True` rather than `true`.
///
/// The text is read back through the document tree rather than through
/// `serde_json`, whose map is sorted: a body's keys are the author's order and
/// stay that way all the way into the sample.
fn python_value(text: &str) -> Result<String, Error> {
    let value = crate::tree::parse(text.as_bytes(), "sample")
        .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.message.clone()))?;
    Ok(python_literal(&value, 0))
}

fn python_literal(value: &crate::tree::Value, depth: usize) -> String {
    use crate::tree::Value;
    let pad = "    ".repeat(depth + 1);
    let close = "    ".repeat(depth);
    match value {
        Value::Null => "None".to_owned(),
        Value::Bool(true) => "True".to_owned(),
        Value::Bool(false) => "False".to_owned(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => dquote(text),
        Value::Sequence(items) if items.is_empty() => "[]".to_owned(),
        Value::Sequence(items) => {
            let inner: Vec<String> = items
                .iter()
                .map(|item| format!("{pad}{}", python_literal(item, depth + 1)))
                .collect();
            format!("[\n{}\n{close}]", inner.join(",\n"))
        }
        Value::Mapping(map) if map.is_empty() => "{}".to_owned(),
        Value::Mapping(map) => {
            let inner: Vec<String> = map
                .iter()
                .filter_map(|(key, item)| {
                    Some(format!(
                        "{pad}{}: {}",
                        dquote(crate::tree::as_str(key)?),
                        python_literal(item, depth + 1)
                    ))
                })
                .collect();
            format!("{{\n{}\n{close}}}", inner.join(",\n"))
        }
        Value::Tagged(tagged) => python_literal(&tagged.value, depth),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_generator_has_a_template_that_compiles() {
        let registry = Registry::new();
        for generator in GENERATORS {
            assert!(
                registry.environment.get_template(generator.id).is_ok(),
                "`{}` has no compiled template",
                generator.id
            );
        }
        assert_eq!(
            GENERATORS.len(),
            SOURCES.len(),
            "every registry row has a source and the other way round"
        );
    }

    #[test]
    fn the_default_languages_are_all_registered() {
        for language in DEFAULT_LANGUAGES {
            assert!(
                generator(language).is_some(),
                "`{language}` is not registered"
            );
        }
    }

    #[test]
    fn a_single_quote_cannot_escape_a_shell_word() {
        assert_eq!(shell("it's"), r"'it'\''s'");
        assert_eq!(shell("a b"), "'a b'");
    }

    #[test]
    fn a_double_quoted_string_escapes_what_would_end_it() {
        assert_eq!(dquote(r#"say "hi"\"#), r#""say \"hi\"\\""#);
        assert_eq!(dquote("two\nlines"), r#""two\nlines""#);
    }

    #[test]
    fn a_php_string_escapes_the_dollar_that_would_interpolate() {
        assert_eq!(
            php_string("Bearer $ACCESS_TOKEN"),
            r#""Bearer \$ACCESS_TOKEN""#
        );
    }

    #[test]
    fn json_becomes_a_python_literal_with_pythons_own_spellings() {
        let rendered = python_value(r#"{"a": true, "b": null, "c": [1, 2]}"#).expect("it converts");
        assert!(rendered.contains("\"a\": True"), "{rendered}");
        assert!(rendered.contains("\"b\": None"), "{rendered}");
        assert!(rendered.contains("[\n"), "a list is laid out: {rendered}");
    }
}
