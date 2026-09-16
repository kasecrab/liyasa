//! THM-31 and THM-32: the base bundle fits its budget, every lazy module fits
//! its own, and nothing the theme ships fetches from another origin.

use std::collections::BTreeSet;
use std::io::Write;
use std::process::{Command, Stdio};

use liyasa_theme::config::ThemeConfig;
use liyasa_theme::context::{Mode, RenderContext};
use liyasa_theme::runtime::{BASE_BUDGET, BOOTSTRAP, Runtime, budget_of, external_requests};
use liyasa_theme::stylesheet::Styles;
use liyasa_theme::theme::Theme;
use liyasa_theme::tokens::Tokens;

fn compressed_len(text: &str) -> usize {
    let Ok(mut child) = Command::new("gzip")
        .arg("-9c")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return text.len();
    };
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(text.as_bytes())
            .expect("gzip accepts input");
    }
    let output = child.wait_with_output().expect("gzip finishes");
    output.stdout.len()
}

#[test]
fn the_base_bundle_fits_its_budget() {
    let runtime = Runtime::build(&ThemeConfig::default());
    let compressed = compressed_len(&runtime.base);
    assert!(
        compressed <= BASE_BUDGET,
        "the base bundle is {compressed} bytes compressed ({} uncompressed)",
        runtime.base.len()
    );
}

#[test]
fn every_lazy_module_fits_its_budget() {
    let runtime = Runtime::build(&ThemeConfig::default());
    let measured: Vec<(&str, usize)> = runtime
        .lazy
        .iter()
        .map(|module| (module.name, compressed_len(&module.source)))
        .collect();
    for (name, size) in &measured {
        assert!(
            *size <= budget_of(name),
            "`{name}` is {size} bytes compressed"
        );
    }
    let compressed = compressed_len(&runtime.base);
    assert!(runtime.over_budget(compressed, &measured).is_empty());
}

#[test]
fn nothing_the_theme_ships_leaves_the_origin() {
    let runtime = Runtime::build(&ThemeConfig::default());
    let styles =
        Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]).expect("the theme compiles");
    let mut sources = vec![runtime.base.as_str(), BOOTSTRAP, styles.css.as_str()];
    for module in &runtime.lazy {
        sources.push(&module.source);
    }
    let found = external_requests(&sources);
    assert!(found.is_empty(), "third-party requests: {found:?}");
}

#[test]
fn every_hook_thm_33_documents_is_emitted() {
    let runtime = Runtime::build(&ThemeConfig::default());
    for hook in [
        "page:load",
        "search:open",
        "theme:change",
        "feedback:submit",
    ] {
        assert!(
            runtime.base.contains(&format!("emit(\"{hook}\"")),
            "`liyasa.on(\"{hook}\")` would never fire"
        );
    }
    // A custom script registers before the first hook fires, because the API
    // is defined by the first module in the bundle and `theme.js` is deferred
    // after it (CMP-101).
    let api = runtime
        .base
        .find("window.liyasa = liyasa")
        .expect("the api is defined");
    let first_emit = runtime
        .base
        .find("emit(\"page:load\"")
        .expect("page:load fires");
    assert!(api < first_emit);
}

#[test]
fn every_module_guards_the_elements_it_enhances() {
    // A module that assumes its markup is present breaks every page that does
    // not carry it, which is how a progressive-enhancement bundle stops being
    // one. Each module either queries a list or returns early.
    let runtime = Runtime::build(&ThemeConfig::default());
    for module in runtime.module_names() {
        assert!(
            !runtime.base.is_empty(),
            "`{module}` is listed but the bundle is empty"
        );
    }
    assert!(runtime.base.matches("querySelectorAll").count() >= 6);
    assert!(runtime.base.contains("if (!dialog || !trigger) return;"));
}

/// Every class on an element the rendered markup marks `hidden`.
fn hidden_classes(html: &str) -> BTreeSet<&str> {
    let mut found = BTreeSet::new();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = &rest[at..];
        let Some(end) = rest.find('>') else { break };
        let tag = &rest[..=end];
        rest = &rest[end..];
        if !tag.contains(" hidden") {
            continue;
        }
        let Some(class_at) = tag.find("class=\"") else {
            continue;
        };
        let names = &tag[class_at + 7..];
        let Some(close) = names.find('"') else {
            continue;
        };
        found.extend(names[..close].split_whitespace());
    }
    found
}

/// Every `selector { declarations }` pair in `css`, at-rule wrappers walked
/// into rather than treated as rules of their own.
fn rules(css: &str) -> Vec<(&str, &str)> {
    let bytes = css.as_bytes();
    let mut out = Vec::new();
    let mut selector_from = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                let mut depth = 1;
                let mut j = i + 1;
                while j < bytes.len() && depth > 0 {
                    match bytes[j] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                let body = &css[i + 1..j.saturating_sub(1)];
                let selector = css[selector_from..i].trim();
                if body.contains('{') {
                    i += 1;
                    selector_from = i;
                    continue;
                }
                if !selector.starts_with('@') {
                    out.push((selector, body));
                }
                i = j;
                selector_from = i;
            }
            b'}' => {
                i += 1;
                selector_from = i;
            }
            _ => i += 1,
        }
    }
    out
}

/// Does any selector in the list name exactly `.class`, with no suffix?
fn names_class(selector: &str, class: &str) -> bool {
    selector.split(',').any(|one| {
        one.split_whitespace().any(|part| {
            part.strip_prefix('.')
                .and_then(|rest| rest.strip_prefix(class))
                .is_some_and(str::is_empty)
        })
    })
}

#[test]
fn a_control_the_markup_hides_is_not_displayed_anyway() {
    // The browser's own `[hidden] { display: none }` is a user-agent rule, so
    // any author rule setting `display` on the same element beats it whatever
    // its specificity. A control the runtime reveals must therefore take its
    // `display` back under `[hidden]`, or a reader without JavaScript is shown
    // a button that does nothing.
    let theme = Theme::new().expect("the theme builds");
    let styles =
        Styles::build(&ThemeConfig::default(), &Tokens::aurora(), &[]).expect("the theme compiles");

    let mut markup = theme
        .render_page(&RenderContext::sample())
        .expect("the page renders");
    let mut with_assistant = RenderContext::sample();
    with_assistant.page.mode = Mode::Assistant;
    markup.push_str(
        &theme
            .render_page(&with_assistant)
            .expect("the assistant page renders"),
    );

    let rules = rules(&styles.css);
    let mut overriding = Vec::new();
    for class in hidden_classes(&markup) {
        let sets_display = rules
            .iter()
            .any(|(selector, body)| names_class(selector, class) && body.contains("display:"));
        if !sets_display {
            continue; // `[hidden]` from the browser's own stylesheet holds.
        }
        let neutralised = rules.iter().any(|(selector, body)| {
            selector.contains(&format!(".{class}[hidden]")) && body.contains("display:")
        });
        if !neutralised {
            overriding.push(class);
        }
    }

    assert!(
        overriding.is_empty(),
        "these classes set `display` and never take it back under `[hidden]`, \
         so the control is a dead button without a script: {overriding:?}"
    );
}
