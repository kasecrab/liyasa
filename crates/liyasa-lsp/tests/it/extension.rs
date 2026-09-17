//! The extension manifest against the server.
//!
//! Help text and manifests that name something the code does not provide have
//! reached users four times on this project. The extension is JavaScript and
//! the server is Rust, so nothing but a test holds the two together: every
//! command the manifest contributes must be a command the extension registers,
//! and every request the extension sends must be one the server answers.

use std::path::{Path, PathBuf};

fn editor_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("editors/vscode")
}

fn manifest() -> serde_json::Value {
    let text = std::fs::read_to_string(editor_dir().join("package.json"))
        .expect("the extension manifest is in the crate");
    serde_json::from_str(&text).expect("the manifest is valid JSON")
}

fn extension_source() -> String {
    std::fs::read_to_string(editor_dir().join("extension.js"))
        .expect("the extension source is in the crate")
}

fn strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn every_contributed_command_is_registered_in_the_extension() {
    let manifest = manifest();
    let source = extension_source();
    let commands = manifest["contributes"]["commands"]
        .as_array()
        .expect("the manifest contributes commands");
    assert!(!commands.is_empty());
    for command in commands {
        let id = command["command"].as_str().expect("a command has an id");
        assert!(
            source.contains(&format!("registerCommand(\"{id}\"")),
            "`{id}` is contributed but never registered"
        );
    }
}

#[test]
fn every_menu_entry_names_a_contributed_command() {
    let manifest = manifest();
    let contributed: Vec<String> = manifest["contributes"]["commands"]
        .as_array()
        .expect("the manifest contributes commands")
        .iter()
        .filter_map(|c| c["command"].as_str().map(str::to_owned))
        .collect();
    for (menu, entries) in manifest["contributes"]["menus"]
        .as_object()
        .expect("the manifest contributes menus")
    {
        for entry in entries.as_array().expect("a menu is a list") {
            let id = entry["command"]
                .as_str()
                .expect("a menu entry names a command");
            assert!(
                contributed.iter().any(|c| c == id),
                "`{menu}` names `{id}`, which is not contributed"
            );
        }
    }
}

#[test]
fn the_extension_sends_only_requests_the_server_answers() {
    let source = extension_source();
    // The one method outside the standard. Anything else the extension invented
    // would be answered with METHOD_NOT_FOUND at runtime and nowhere else.
    for at in source.match_indices("sendRequest(\"") {
        let rest = &source[at.0 + "sendRequest(\"".len()..];
        let method = rest.split('"').next().unwrap_or_default();
        assert_eq!(
            method, "liyasa/preview",
            "the server answers no other custom request"
        );
    }
    assert!(
        source.contains("sendRequest(\"liyasa/preview\""),
        "the preview is what the custom request is for"
    );
}

#[test]
fn the_default_server_command_is_the_one_the_packet_specifies() {
    // CLI-25 is `liyasa lsp`. It does not exist yet — RFC 3001 — and the
    // README says so; the default here is what it will be, and the setting is
    // how an author points at something else meanwhile.
    let manifest = manifest();
    let properties = &manifest["contributes"]["configuration"]["properties"];
    assert_eq!(properties["liyasa.server.path"]["default"], "liyasa");
    assert_eq!(
        strings(&properties["liyasa.server.args"]["default"]),
        vec!["lsp".to_owned()]
    );
}

#[test]
fn the_readme_does_not_claim_the_command_exists() {
    let readme = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md"))
        .expect("the crate has a README");
    assert!(
        readme.contains("`liyasa lsp` is not wired up yet"),
        "the README says plainly that the command is not built"
    );
}

#[test]
fn the_contributed_language_does_not_fight_vs_code_for_dot_md() {
    // VS Code's built-in Markdown extension owns `.md` and `.markdown`, and an
    // extension may not share a file extension with it: contributing them here
    // either loses — leaving every ordinary page unserved — or wins and takes
    // Markdown's own features away. `.mdx` is unclaimed and is ours.
    let manifest = manifest();
    let language = manifest["contributes"]["languages"]
        .as_array()
        .and_then(|languages| languages.first().cloned())
        .expect("the extension contributes a language");
    assert_eq!(language["id"], "liyasa-markdown");
    assert_eq!(strings(&language["extensions"]), vec![".mdx".to_owned()]);
}

#[test]
fn the_client_selects_vs_codes_markdown_as_well_as_its_own_language() {
    // Without this an ordinary `.md` page — which is nearly every page — gets
    // no diagnostics, no completion and no hover, and nothing says why.
    let source = extension_source();
    for id in ["markdown", "liyasa-markdown"] {
        assert!(
            source.contains(&format!("language: \"{id}\"")),
            "`{id}` is a language this extension must serve"
        );
    }
}

#[test]
fn the_preview_menu_is_offered_on_every_language_the_client_selects() {
    let manifest = manifest();
    for entry in manifest["contributes"]["menus"]["editor/title"]
        .as_array()
        .expect("a menu is a list")
    {
        let when = entry["when"].as_str().expect("a menu entry is conditional");
        for id in ["markdown", "liyasa-markdown"] {
            assert!(
                when.contains(id),
                "`{id}` is served, so the command belongs on its title bar: {when}"
            );
        }
    }
}

#[test]
fn the_language_configuration_the_manifest_names_is_there_and_parses() {
    let manifest = manifest();
    let path = manifest["contributes"]["languages"][0]["configuration"]
        .as_str()
        .expect("the language names a configuration file");
    let file = editor_dir().join(path.trim_start_matches("./"));
    let text = std::fs::read_to_string(&file)
        .unwrap_or_else(|_| panic!("{} is in the crate", file.display()));
    serde_json::from_str::<serde_json::Value>(&text).expect("it is valid JSON");
}

#[test]
fn the_entry_point_the_manifest_names_is_there() {
    let manifest = manifest();
    let main = manifest["main"]
        .as_str()
        .expect("the manifest names an entry point");
    assert!(editor_dir().join(main.trim_start_matches("./")).exists());
}

#[test]
fn the_preview_webview_runs_no_script_and_fetches_nothing() {
    // The preview shows what the build would serve; it does not run it, and a
    // page under review is not a page to be trusted.
    let source = extension_source();
    assert!(
        source.contains("enableScripts: false"),
        "no scripts in the webview"
    );
    assert!(
        source.contains("default-src 'none'"),
        "the webview's content security policy denies by default"
    );
}

#[test]
fn every_notification_the_extension_subscribes_to_is_one_the_server_acts_on() {
    // The client watches liyasa.json, facts/ and snippets/ and sends
    // `workspace/didChangeWatchedFiles` when one changes. A server that ignored
    // it would keep answering from an index the project has moved past, and
    // nothing would say so.
    let source = extension_source();
    assert!(
        source.contains("createFileSystemWatcher"),
        "the client watches the files the index is built from"
    );
    for path in ["liyasa.json", "facts/**", "snippets/**"] {
        assert!(
            source.contains(path),
            "`{path}` feeds the index and is watched"
        );
    }
}

#[test]
fn a_server_that_will_not_start_is_reported_and_the_commands_still_work() {
    // `liyasa lsp` does not exist yet (RFC 3001), so this is the path every
    // user takes today. An exception out of `activate` would register no
    // commands and print nothing an author can act on.
    let source = extension_source();
    let activate = source
        .split_once("async function activate(")
        .map(|(_, rest)| rest)
        .expect("the extension has an activate function");
    let commands_at = activate
        .find("registerCommand")
        .expect("activate registers the commands");
    let start_at = activate
        .find("await start(")
        .expect("activate starts the server");
    assert!(
        commands_at < start_at,
        "the commands are registered before the server is started"
    );
    assert!(
        source.contains("showErrorMessage"),
        "the failure is reported"
    );
    assert!(
        source.contains("liyasa.server.path"),
        "and it names the setting that fixes it"
    );
}

#[test]
fn the_handover_in_the_readme_names_types_the_cli_actually_has() {
    // The three lines in the README are what WP-09 will paste. Prose that will
    // not compile is the same defect as prose naming a flag nobody built.
    let readme = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("README.md"))
        .expect("the crate has a README");
    assert!(
        readme.contains("Exit::Success") && readme.contains("Exit::Errors"),
        "`commands::dispatch` returns `Exit`, so the arm produces one"
    );
    assert!(
        !readme.contains("ExitCode::Success"),
        "there is no such variant; `std::process::ExitCode` spells it SUCCESS \
         and `dispatch` does not return one at all"
    );
    assert!(
        readme.contains("liyasa_lsp::serve_stdio()"),
        "the arm calls the entry point this crate exposes"
    );
}
