//! Reading a JavaScript config file (MIG-02).
//!
//! `docusaurus.config.js` and `sidebars.js` are modules, not data. Most of them
//! export a literal object, and those are evaluated in-process with `boa`. The
//! rest — a TypeScript config, one that reaches into `require`, one that reads
//! the environment — are not something an importer should pretend to
//! understand, so the importer writes the one-line Node script that turns the
//! module into JSON and asks for it to be run.
//!
//! A module evaluated here is the operator's own file, not a remote one, but it
//! still runs under `boa`'s loop and recursion limits: a config with a runaway
//! loop should fail the import, not hang it.

use serde_json::Value;

/// The name of the script the importer writes when it cannot evaluate a module.
pub const EXPORT_SCRIPT: &str = "liyasa-export-config.mjs";

/// The script itself. One line, as MIG-02 asks: it imports the module it is
/// given and writes the JSON beside it, for CommonJS and ESM alike.
pub const EXPORT_SCRIPT_BODY: &str = concat!(
    "import('node:fs').then(fs => import(process.argv[2])",
    ".then(m => fs.writeFileSync(process.argv[2].replace(/\\.[cm]?[jt]s$/, '.json'), ",
    "JSON.stringify(m.default ?? m, null, 2))))\n",
);

/// How to run it, for the diagnostic's help line.
pub fn how_to_run(module: &str) -> String {
    format!("run `node {EXPORT_SCRIPT} ./{module}` and import again")
}

/// Everything the engine needs before a CommonJS module will run: a `module`
/// to assign to, and the globals a config file expects to exist. `require`
/// answers with an empty object rather than throwing, so a module that only
/// destructures an unused import still evaluates.
const PRELUDE: &str = r#"
var module = { exports: {} };
var exports = module.exports;
var process = { env: {}, argv: [], platform: "linux", cwd: function () { return "."; } };
var __dirname = ".";
var __filename = "";
function require() { return {}; }
"#;

/// Rewrites the module syntax `boa` does not run into the script syntax it
/// does. Only the two forms a config file uses are handled; anything else fails
/// evaluation and reaches the Node script.
fn as_script(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + PRELUDE.len());
    out.push_str(PRELUDE);
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        // An `import` at the top of a module has no script equivalent and is
        // never what the config's value depends on.
        if trimmed.starts_with("import ") && trimmed.contains(" from ") {
            continue;
        }
        if trimmed.starts_with("import type ") {
            continue;
        }
        match trimmed.strip_prefix("export default ") {
            Some(rest) => {
                out.push_str("module.exports = ");
                out.push_str(rest);
            }
            None => out.push_str(line),
        }
    }
    out
}

/// Evaluates a module and returns what it exported.
///
/// The value comes back through `JSON.stringify` inside the engine, so a
/// function or an `undefined` in the config drops out exactly as it would for
/// the Node script, and the two paths agree on what the config says.
#[cfg(feature = "import-js")]
pub fn evaluate(source: &str) -> Result<Value, String> {
    use boa_engine::{Context, Source};

    let mut context = Context::default();
    context
        .runtime_limits_mut()
        .set_loop_iteration_limit(1_000_000);
    context.runtime_limits_mut().set_recursion_limit(256);

    let script = as_script(source);
    context
        .eval(Source::from_bytes(script.as_bytes()))
        .map_err(|error| error.to_string())?;
    let exported = context
        .eval(Source::from_bytes(b"JSON.stringify(module.exports)"))
        .map_err(|error| error.to_string())?;
    let text = exported
        .to_string(&mut context)
        .map_err(|error| error.to_string())?
        .to_std_string_escaped();
    // The prelude seeds `module.exports` with an empty object, so an empty one
    // coming back means the module assigned nothing — a config that imported
    // cleanly and said nothing is worse than one that asks for the Node script.
    if text == "undefined" || text == "{}" {
        return Err("the module exported nothing".to_owned());
    }
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

#[cfg(not(feature = "import-js"))]
pub fn evaluate(_source: &str) -> Result<Value, String> {
    Err("this build has no JavaScript engine (feature `import-js`)".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "import-js")]
    #[test]
    fn a_common_js_literal_evaluates() {
        let value = evaluate("module.exports = { title: 'Acme', tagline: 'Docs' };\n")
            .expect("a literal object evaluates");
        assert_eq!(value["title"], "Acme");
        assert_eq!(value["tagline"], "Docs");
    }

    #[cfg(feature = "import-js")]
    #[test]
    fn an_es_module_default_export_evaluates() {
        let value = evaluate("export default { title: 'Acme', url: 'https://acme.example' };\n")
            .expect("a default export evaluates");
        assert_eq!(value["url"], "https://acme.example");
    }

    #[cfg(feature = "import-js")]
    #[test]
    fn a_named_config_assigned_to_the_default_export_evaluates() {
        let value = evaluate(
            "import type { Config } from '@docusaurus/types'\n\
             const config = { title: 'Acme', i18n: { defaultLocale: 'en', locales: ['en', 'de'] } };\n\
             export default config;\n",
        )
        .expect("a named config evaluates");
        assert_eq!(value["i18n"]["locales"][1], "de");
    }

    #[cfg(feature = "import-js")]
    #[test]
    fn nesting_arrays_and_numbers_survive_the_round_trip() {
        let value = evaluate(
            "module.exports = { presets: [['classic', { docs: { routeBasePath: '/' } }]], n: 3, ok: true };",
        )
        .expect("a nested literal evaluates");
        assert_eq!(value["presets"][0][0], "classic");
        assert_eq!(value["presets"][0][1]["docs"]["routeBasePath"], "/");
        assert_eq!(value["n"], 3);
        assert_eq!(value["ok"], true);
    }

    #[cfg(feature = "import-js")]
    #[test]
    fn an_unused_require_does_not_stop_a_literal_config() {
        let value =
            evaluate("const path = require('path');\nmodule.exports = { title: 'Acme' };\n")
                .expect("an unused require evaluates");
        assert_eq!(value["title"], "Acme");
    }

    #[cfg(feature = "import-js")]
    #[test]
    fn a_config_that_reaches_into_a_module_fails_rather_than_guessing() {
        let error = evaluate(
            "const { themes } = require('prism-react-renderer');\n\
             module.exports = { theme: themes.github };\n",
        )
        .expect_err("a config that dereferences a required module cannot be evaluated");
        assert!(!error.is_empty());
    }

    #[cfg(feature = "import-js")]
    #[test]
    fn a_module_that_exports_nothing_is_an_error_not_an_empty_config() {
        assert!(evaluate("const x = 1;\n").is_err());
    }

    #[test]
    fn the_export_script_names_the_module_it_is_given() {
        assert!(how_to_run("docusaurus.config.js").contains("docusaurus.config.js"));
        assert!(EXPORT_SCRIPT_BODY.lines().count() == 1);
    }

    #[test]
    fn module_syntax_becomes_script_syntax() {
        let script = as_script("import x from 'y'\nexport default { a: 1 }\n");
        assert!(!script.contains("import x"));
        assert!(script.contains("module.exports = { a: 1 }"));
    }
}
