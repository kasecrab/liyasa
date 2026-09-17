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
fn the_language_the_extension_serves_covers_the_page_extensions() {
    let manifest = manifest();
    let language = manifest["contributes"]["languages"]
        .as_array()
        .and_then(|languages| languages.first().cloned())
        .expect("the extension contributes a language");
    assert_eq!(language["id"], "liyasa-markdown");
    let extensions = strings(&language["extensions"]);
    for expected in [".md", ".mdx", ".markdown"] {
        assert!(
            extensions.iter().any(|e| e == expected),
            "`{expected}` is a page extension: {extensions:?}"
        );
    }
}

#[test]
fn the_document_selector_and_the_contributed_language_are_the_same_name() {
    let manifest = manifest();
    let id = manifest["contributes"]["languages"][0]["id"]
        .as_str()
        .expect("the language has an id");
    let source = extension_source();
    assert!(
        source.contains(&format!("language: \"{id}\"")),
        "the client selects the language the manifest contributes"
    );
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
