//! MIG-10: the starter `liyasa new` writes.
//!
//! The files are embedded in the binary (CLI-32: one static binary, no
//! template downloaded at first run). Three placeholders are substituted; the
//! rest is written as it is, so what a reader sees in `templates/starter/` is
//! what lands on their disk.

use serde_json::Value;

/// A placeholder rather than `{{ name }}`, because the pages are Liyasa
/// Markdown and `{{ … }}` is expanded at build time. A template syntax inside
/// a template would need escaping in every file that used it.
const NAME: &str = "__LIYASA_NAME__";
const PRESET: &str = "__LIYASA_PRESET__";
const SCHEMA: &str = "__LIYASA_SCHEMA__";

/// One file of the starter, at the path it gets in the new project.
pub struct Template {
    pub path: &'static str,
    pub body: &'static str,
}

/// The starter, minus the files whose presence is a choice.
const BASE: &[Template] = &[
    Template {
        path: "index.md",
        body: include_str!("../templates/starter/index.md"),
    },
    Template {
        path: "guides/quickstart.md",
        body: include_str!("../templates/starter/guides/quickstart.md"),
    },
    Template {
        path: "guides/configuration.md",
        body: include_str!("../templates/starter/guides/configuration.md"),
    },
    Template {
        path: "guides/verification.md",
        body: include_str!("../templates/starter/guides/verification.md"),
    },
    Template {
        path: "checklist.md",
        body: include_str!("../templates/starter/checklist.md"),
    },
    Template {
        path: "facts/pricing.json",
        body: include_str!("../templates/starter/facts/pricing.json"),
    },
    Template {
        path: "facts/sources.toml",
        body: include_str!("../templates/starter/facts/sources.toml"),
    },
    Template {
        path: "README.md",
        body: include_str!("../templates/starter/README.md"),
    },
    Template {
        path: ".liyasaignore",
        body: include_str!("../templates/starter/.liyasaignore"),
    },
    // Named `gitignore` in the repository: a `.gitignore` inside the crate
    // would apply to the crate rather than travel with it.
    Template {
        path: ".gitignore",
        body: include_str!("../templates/starter/gitignore"),
    },
];

const CONFIG: &str = include_str!("../templates/starter/liyasa.json");
const SPEC: &str = include_str!("../templates/starter/openapi/api.yaml");
const WORKFLOW: &str = include_str!("../templates/starter/ci/github.yml");

/// The theme presets `liyasa.json` accepts, which is what `--preset` is
/// checked against before anything is written.
pub const PRESETS: &[&str] = &[
    "aurora", "atlas", "meadow", "slate", "ember", "harbor", "quill", "signal", "lumen",
];

pub const DEFAULT_PRESET: &str = "aurora";

#[derive(Debug, Clone)]
pub struct Options {
    pub name: String,
    pub preset: String,
    pub openapi: bool,
    pub ci: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            name: "Documentation".to_owned(),
            preset: DEFAULT_PRESET.to_owned(),
            openapi: true,
            ci: true,
        }
    }
}

/// Every file the scaffold writes, in the order it writes them.
pub fn files(options: &Options) -> Vec<(String, String)> {
    let mut out = vec![("liyasa.json".to_owned(), config(options))];
    for template in BASE {
        out.push((template.path.to_owned(), fill(template.body, options)));
    }
    if options.openapi {
        out.push(("openapi/api.yaml".to_owned(), SPEC.to_owned()));
    }
    if options.ci {
        out.push((".github/workflows/docs.yml".to_owned(), WORKFLOW.to_owned()));
    }
    out
}

/// `liyasa.json` is built through `serde_json` rather than by substitution, so
/// the `openapi` key can be left out entirely and so the result is already in
/// the canonical form `liyasa format` would write.
fn config(options: &Options) -> String {
    let filled = fill(CONFIG, options);
    let Ok(mut value) = serde_json::from_str::<Value>(&filled) else {
        // The template is checked by a test; this arm exists so a corrupted
        // build writes something rather than panicking on the user.
        return filled;
    };
    if options.openapi
        && let Some(object) = value.as_object_mut()
    {
        object.insert(
            "openapi".to_owned(),
            serde_json::json!([{ "id": "api", "source": "openapi/api.yaml" }]),
        );
    }
    let mut text = serde_json::to_string_pretty(&value).unwrap_or(filled);
    text.push('\n');
    text
}

fn fill(body: &str, options: &Options) -> String {
    body.replace(NAME, &options.name)
        .replace(PRESET, &options.preset)
        .replace(SCHEMA, &schema_url())
}

fn schema_url() -> String {
    liyasa_config::schema::named("config")
        .map_or_else(String::new, liyasa_config::schema::schema_url)
}

/// A site name from a directory name: `acme-docs` becomes `Acme docs`.
pub fn name_from_directory(directory: &std::path::Path) -> String {
    let stem = directory
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty() && name != ".")
        .unwrap_or_else(|| "Documentation".to_owned());
    let words = stem.replace(['-', '_'], " ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => words,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_template_is_valid_json_once_filled() {
        let text = config(&Options::default());
        let value: Value = serde_json::from_str(&text).expect("the scaffolded config parses");
        assert_eq!(value["name"], "Documentation");
        assert_eq!(value["theme"]["preset"], DEFAULT_PRESET);
        assert!(value["openapi"].is_array());
    }

    #[test]
    fn leaving_the_spec_out_leaves_the_key_out() {
        let options = Options {
            openapi: false,
            ..Options::default()
        };
        let value: Value = serde_json::from_str(&config(&options)).expect("valid JSON");
        assert!(value.get("openapi").is_none());
        assert!(
            !files(&options)
                .iter()
                .any(|(path, _)| path.contains("openapi"))
        );
    }

    #[test]
    fn the_config_is_already_canonically_formatted() {
        // `liyasa format --check` must pass on a fresh scaffold, and the
        // canonical form of a config is pretty-printed JSON with a newline.
        let text = config(&Options::default());
        let value: Value = serde_json::from_str(&text).expect("valid JSON");
        let mut canonical = serde_json::to_string_pretty(&value).expect("re-serializes");
        canonical.push('\n');
        assert_eq!(text, canonical);
    }

    #[test]
    fn no_placeholder_survives() {
        for (path, body) in files(&Options::default()) {
            for placeholder in [NAME, PRESET, SCHEMA] {
                assert!(
                    !body.contains(placeholder),
                    "{path} still contains {placeholder}"
                );
            }
        }
    }

    #[test]
    fn the_default_preset_is_one_the_schema_allows() {
        assert!(PRESETS.contains(&DEFAULT_PRESET));
    }

    #[test]
    fn a_directory_name_becomes_a_title() {
        assert_eq!(
            name_from_directory(std::path::Path::new("/tmp/acme-docs")),
            "Acme docs"
        );
        assert_eq!(
            name_from_directory(std::path::Path::new("/tmp/my_handbook")),
            "My handbook"
        );
    }

    #[test]
    fn every_page_the_navigation_names_is_written() {
        let written: Vec<String> = files(&Options::default())
            .into_iter()
            .map(|(path, _)| path)
            .collect();
        let config: Value = serde_json::from_str(&config(&Options::default())).expect("valid JSON");
        for group in config["navigation"].as_array().expect("navigation") {
            for page in group["pages"].as_array().expect("pages") {
                let route = page.as_str().expect("a page path");
                assert!(
                    written.contains(&format!("{route}.md")),
                    "navigation names `{route}`, which the scaffold does not write"
                );
            }
        }
    }
}
