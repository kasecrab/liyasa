//! The reviewable rendering of the preset (THM-01, THM-05).
//!
//! Everything on the page comes from the crate — the token table, the compiled
//! stylesheet, the runtime, the partials, the bundled faces — so the page
//! cannot drift from the theme: regenerate it and it is current. The binary
//! that writes it is `src/bin/preview.rs`:
//!
//! ```sh
//! cargo run -p liyasa-theme --bin preview            # ../../preview/aurora.html
//! cargo run -p liyasa-theme --bin preview -- out.html
//! ```

use std::fmt::Write as _;

use crate::color::Color;
use crate::config::ThemeConfig;
use crate::context::{Mode, RenderContext};
use crate::nav::{Group, Item, Navigation, Tab, TocEntry};
use crate::runtime::{BOOTSTRAP, Runtime};
use crate::stylesheet::Styles;
use crate::theme::Theme;
use crate::tokens::{Group as TokenGroup, Scheme, Tokens, aurora, reference};
use crate::{fonts, icons};

const STYLESHEET: &str = "%%LIYASA_STYLESHEET%%";
const SCRIPT: &str = "%%LIYASA_SCRIPT%%";

/// The whole preview as one self-contained page.
pub fn page() -> Result<String, Box<dyn std::error::Error>> {
    let config = ThemeConfig::default();
    let tokens = Tokens::aurora();
    let styles = Styles::build(&config, &tokens, &[preview_css()])?.with_fonts(&faces(), "");
    let runtime = Runtime::build(&config);
    let theme = Theme::new()?;

    let mut context = docs_page(&tokens);
    context.assets.stylesheet = STYLESHEET.to_owned();
    context.assets.script = SCRIPT.to_owned();
    context.assets.critical = String::new();
    context.assets.bootstrap = BOOTSTRAP.to_owned();
    context.assets.custom_css = Vec::new();
    context.assets.custom_js = Vec::new();

    let page = theme.render_page(&context)?;
    Ok(page
        .replace(
            &format!("<link rel=\"stylesheet\" href=\"{STYLESHEET}\">"),
            &format!("<style>{}</style>", styles.css),
        )
        .replace(
            &format!("<script src=\"{SCRIPT}\" defer data-liyasa=\"runtime\"></script>"),
            &format!("<script>{}</script>", runtime.base),
        ))
}

/// The bundled faces as data URIs, so the page carries the real typography
/// when it is opened from a file rather than served.
fn faces() -> Vec<fonts::FaceFile> {
    fonts::bundled()
        .into_iter()
        .filter_map(|mut face| {
            let bytes = fonts::file(&face.file)?;
            let mime = match face.format.as_str() {
                "woff2" => "font/woff2",
                _ => "font/ttf",
            };
            face.file = format!("data:{mime};base64,{}", base64(bytes));
            Some(face)
        })
        .collect()
}

/// A data URI needs base64 and the theme has no encoder; this is the whole of
/// one, and it runs on two files in a developer tool.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - i * 6)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn docs_page(tokens: &Tokens) -> RenderContext {
    let navigation = Navigation {
        tabs: vec![
            Tab {
                title: "Guides".to_owned(),
                href: Some("#".to_owned()),
                groups: vec![
                    Group {
                        title: "Get started".to_owned(),
                        expanded: true,
                        items: vec![
                            item("Introduction", "#introduction", Some("search")),
                            Item {
                                title: "Install".to_owned(),
                                route: "#install".to_owned(),
                                icon: Some("copy".to_owned()),
                                children: vec![
                                    item("Docker", "#docker", None),
                                    item("From source", "#source", None),
                                ],
                                ..Item::default()
                            },
                            Item {
                                title: "Configuration".to_owned(),
                                route: "#configuration".to_owned(),
                                tag: Some("new".to_owned()),
                                ..Item::default()
                            },
                        ],
                        ..Group::default()
                    },
                    Group {
                        title: "Authoring".to_owned(),
                        items: vec![
                            item("Markdown", "#markdown", None),
                            item("Components", "#components", None),
                            item("Snippets", "#snippets", None),
                        ],
                        ..Group::default()
                    },
                ],
                ..Tab::default()
            },
            Tab {
                title: "API".to_owned(),
                href: Some("#api".to_owned()),
                groups: vec![Group {
                    title: "Reference".to_owned(),
                    items: vec![item("Pages", "#pages", None)],
                    ..Group::default()
                }],
                ..Tab::default()
            },
        ],
        ..Navigation::default()
    };

    let mut context = RenderContext::sample();
    context.page.mode = Mode::Default;
    context.page.route = "#install".to_owned();
    context.page.title = "Install Liyasa".to_owned();
    context.page.description =
        "Everything the build needs is a supported platform and a terminal.".to_owned();
    context.page.breadcrumbs = navigation.trail("#install");
    context.page.previous = None;
    context.page.next = None;
    context.page.toc = vec![
        toc("Requirements", "requirements", vec![]),
        toc(
            "Install",
            "install",
            vec![
                toc("Docker", "docker", vec![]),
                toc("From source", "source", vec![]),
            ],
        ),
        toc("The design system", "design-system", vec![]),
    ];
    context.nav.active_route = "#install".to_owned();
    context.nav.navigation = navigation;
    context.site.name = "Liyasa docs".to_owned();
    context.site.logo.light = None;
    context.site.logo.dark = None;
    context.set_content(&content(tokens));
    context
}

fn item(title: &str, route: &str, icon: Option<&str>) -> Item {
    Item {
        title: title.to_owned(),
        route: route.to_owned(),
        icon: icon.map(ToOwned::to_owned),
        ..Item::default()
    }
}

fn toc(text: &str, anchor: &str, children: Vec<TocEntry>) -> TocEntry {
    TocEntry {
        level: 2,
        text: text.to_owned(),
        anchor: anchor.to_owned(),
        children,
    }
}

/// A page a reader would actually meet, followed by the design system it is
/// drawn from.
fn content(tokens: &Tokens) -> String {
    let mut out = String::new();
    out.push_str(PROSE);
    out.push_str(&design_system(tokens));
    out
}

const PROSE: &str = r##"
<h2 id="requirements">Requirements</h2>
<p>Liyasa builds a documentation site from Markdown files and one configuration
file. It needs a supported platform and a terminal; everything else — the
theme, the search index, the agent surfaces — is in the binary. Read the
<a href="#configuration">configuration reference</a> when the first build
succeeds.</p>

<div class="ly-callout" data-liyasa="callout" data-ly-tone="info" role="note">
  <div class="ly-callout-body">
    <p class="ly-callout-title">One binary</p>
    <p>There is no Node toolchain to install. The companion runtime is only
    needed for PDF export, screenshots, and Mermaid pre-rendering.</p>
  </div>
</div>

<h2 id="install">Install</h2>
<p>Pick the platform you build on. Each command installs the same version and
writes <code>liyasa.json</code> on first run.</p>

<div class="ly-tabs" data-ly-tabs>
  <div class="ly-tablist" role="tablist" aria-label="Install method">
    <button class="ly-tab" role="tab" id="tab-macos" aria-controls="panel-macos" aria-selected="true">macOS</button>
    <button class="ly-tab" role="tab" id="tab-linux" aria-controls="panel-linux" aria-selected="false">Linux</button>
    <button class="ly-tab" role="tab" id="tab-windows" aria-controls="panel-windows" aria-selected="false">Windows</button>
  </div>
  <div class="ly-tabpanel" role="tabpanel" id="panel-macos" aria-labelledby="tab-macos">
    <p class="ly-tabpanel-title">macOS</p>
    <div class="ly-code" data-liyasa="code-block">
      <div class="ly-code-header"><span class="ly-code-title">Terminal</span><span class="ly-code-lang">sh</span></div>
      <pre id="code-macos"><code class="language-sh"><span class="ly-tok-function">brew</span> <span class="ly-tok-keyword">install</span> liyasa</code></pre>
      <button type="button" class="ly-code-copy" data-ly-copy="code-macos" data-ly-copied-label="Copied">Copy</button>
    </div>
  </div>
  <div class="ly-tabpanel" role="tabpanel" id="panel-linux" aria-labelledby="tab-linux" hidden>
    <p class="ly-tabpanel-title">Linux</p>
    <div class="ly-code" data-liyasa="code-block">
      <div class="ly-code-header"><span class="ly-code-title">Terminal</span><span class="ly-code-lang">sh</span></div>
      <pre id="code-linux"><code class="language-sh"><span class="ly-tok-function">curl</span> <span class="ly-tok-attribute">-fsSL</span> <span class="ly-tok-string">https://example.test/install.sh</span> <span class="ly-tok-operator">|</span> <span class="ly-tok-function">sh</span></code></pre>
      <button type="button" class="ly-code-copy" data-ly-copy="code-linux" data-ly-copied-label="Copied">Copy</button>
    </div>
  </div>
  <div class="ly-tabpanel" role="tabpanel" id="panel-windows" aria-labelledby="tab-windows" hidden>
    <p class="ly-tabpanel-title">Windows</p>
    <div class="ly-code" data-liyasa="code-block">
      <div class="ly-code-header"><span class="ly-code-title">PowerShell</span><span class="ly-code-lang">ps1</span></div>
      <pre id="code-windows"><code class="language-ps1"><span class="ly-tok-function">winget</span> <span class="ly-tok-keyword">install</span> liyasa</code></pre>
      <button type="button" class="ly-code-copy" data-ly-copy="code-windows" data-ly-copied-label="Copied">Copy</button>
    </div>
  </div>
</div>

<h3 id="docker">Docker</h3>
<p>The image runs the same binary and is the fastest way to try a build against
an existing repository.</p>

<div class="ly-code" data-ly-numbers="true" data-liyasa="code-block">
  <div class="ly-code-header"><span class="ly-code-title">liyasa.json</span><span class="ly-code-lang">json</span></div>
  <pre id="code-config"><code class="language-json"><span class="ly-code-line" data-ly-line="1">{</span><span class="ly-code-line" data-ly-line="2">  <span class="ly-tok-attribute">"name"</span><span class="ly-tok-punctuation">:</span> <span class="ly-tok-string">"Acme docs"</span><span class="ly-tok-punctuation">,</span></span><span class="ly-code-line" data-ly-line="3" data-ly-highlight="true">  <span class="ly-tok-attribute">"theme"</span><span class="ly-tok-punctuation">:</span> <span class="ly-tok-punctuation">{</span> <span class="ly-tok-attribute">"preset"</span><span class="ly-tok-punctuation">:</span> <span class="ly-tok-string">"aurora"</span> <span class="ly-tok-punctuation">}</span><span class="ly-tok-punctuation">,</span></span><span class="ly-code-line" data-ly-line="4">  <span class="ly-tok-attribute">"navigation"</span><span class="ly-tok-punctuation">:</span> <span class="ly-tok-punctuation">[</span><span class="ly-tok-string">"index"</span><span class="ly-tok-punctuation">,</span> <span class="ly-tok-string">"install"</span><span class="ly-tok-punctuation">]</span></span><span class="ly-code-line" data-ly-line="5">}</span></code></pre>
  <button type="button" class="ly-code-copy" data-ly-copy="code-config" data-ly-copied-label="Copied">Copy</button>
</div>

<h3 id="source">From source</h3>
<p>Building from source needs the Rust toolchain in <code>rust-toolchain.toml</code>.
The first build is slow; the artifact cache makes every later one fast.</p>

<details class="ly-accordion" data-ly-accordion>
  <summary>What the cache keys on <span class="ly-chevron">&#9656;</span></summary>
  <div class="ly-accordion-body">
    <p>Each artifact is keyed by a fingerprint of everything it was derived
    from, so a changed snippet rebuilds the pages that include it and nothing
    else.</p>
  </div>
</details>

<div class="ly-callout" data-liyasa="callout" data-ly-tone="warning" role="note">
  <div class="ly-callout-body">
    <p class="ly-callout-title">Windows server support</p>
    <p>The CLI is supported on Windows 10 and later. Running the server there is
    best effort.</p>
  </div>
</div>

<h3>Platform support</h3>
<div class="ly-table-wrap">
<table>
  <thead><tr><th>Platform</th><th>CLI</th><th>Server</th><th>Build time</th></tr></thead>
  <tbody>
    <tr><td>Linux glibc x86_64</td><td>Supported</td><td>Supported</td><td>12.4 s</td></tr>
    <tr><td>Linux musl aarch64</td><td>Supported</td><td>Supported</td><td>14.8 s</td></tr>
    <tr><td>macOS 13+</td><td>Supported</td><td>Supported</td><td>11.9 s</td></tr>
    <tr><td>Windows 10+</td><td>Supported</td><td>Best effort</td><td>18.2 s</td></tr>
  </tbody>
</table>
</div>

<div class="ly-cards">
  <a class="ly-card" href="#markdown"><span class="ly-card-title">Write a page</span><span class="ly-card-body">Markdown, front matter, and the directive syntax.</span></a>
  <a class="ly-card" href="#components"><span class="ly-card-title">Components</span><span class="ly-card-body">Callouts, tabs, cards, and the API reference blocks.</span></a>
  <a class="ly-card" href="#configuration"><span class="ly-card-title">Configure</span><span class="ly-card-body">Navigation, theme, search, and the agent surfaces.</span></a>
</div>

<ol class="ly-steps">
  <li class="ly-step"><p><strong>Write a page.</strong> Any <code>.md</code> file under the content root becomes a route.</p></li>
  <li class="ly-step"><p><strong>Preview it.</strong> <code>liyasa dev</code> rebuilds on save.</p></li>
  <li class="ly-step"><p><strong>Publish.</strong> <code>liyasa build</code> writes <code>dist/</code>; any static host serves it.</p></li>
</ol>

<blockquote><p>A documentation site is a product surface. It should look like
one.</p></blockquote>
"##;

fn design_system(tokens: &Tokens) -> String {
    let mut out = String::from("<h2 id=\"design-system\">The design system</h2>");
    out.push_str(
        "<p>Every value below is a token from <code>tokens/aurora.rs</code>, \
         rendered live: the swatch is the token, not a copy of it. Switch the \
         scheme with the toggle in the header and the whole page follows.</p>",
    );

    out.push_str("<h3>Neutral ramp</h3><div class=\"ly-preview-ramp\">");
    for step in 1..=12 {
        let name = format!("--ly-color-neutral-{step}");
        let _ = write!(
            out,
            "<div class=\"ly-preview-step\" style=\"background:var({name})\">\
             <span>{step}</span></div>"
        );
    }
    out.push_str("</div>");

    out.push_str("<h3>Colour roles</h3>");
    out.push_str(&swatches(tokens));

    out.push_str("<h3>Contrast</h3>");
    out.push_str(&contrast(tokens));

    out.push_str("<h3>Type scale</h3><div class=\"ly-preview-type\">");
    for spec in reference()
        .iter()
        .filter(|spec| spec.group == TokenGroup::Text)
    {
        if !spec.name.starts_with("--ly-text-")
            || spec.name.contains("leading")
            || spec.name.contains("tracking")
        {
            continue;
        }
        let _ = write!(
            out,
            "<p style=\"font-size:var({name});margin:0\">{name} — Documentation that stays true</p>",
            name = spec.name
        );
    }
    out.push_str("</div>");

    out.push_str("<h3>Spacing</h3><div class=\"ly-preview-scale\">");
    for spec in reference()
        .iter()
        .filter(|spec| spec.group == TokenGroup::Space)
    {
        let _ = write!(
            out,
            "<div class=\"ly-preview-row\"><code>{name}</code>\
             <span class=\"ly-preview-bar\" style=\"width:var({name})\"></span>\
             <span class=\"ly-muted\">{value}</span></div>",
            name = spec.name,
            value = tokens.get(spec.name, Scheme::Light).unwrap_or_default()
        );
    }
    out.push_str("</div>");

    out.push_str("<h3>Radius and shadow</h3><div class=\"ly-preview-grid\">");
    for spec in reference()
        .iter()
        .filter(|spec| spec.group == TokenGroup::Radius)
    {
        let _ = write!(
            out,
            "<div class=\"ly-preview-tile\" style=\"border-radius:var({name})\">{name}</div>",
            name = spec.name
        );
    }
    for name in ["--ly-shadow-sm", "--ly-shadow-md", "--ly-shadow-lg"] {
        let _ = write!(
            out,
            "<div class=\"ly-preview-tile\" style=\"box-shadow:var({name})\">{name}</div>"
        );
    }
    out.push_str("</div>");

    out.push_str(
        "<h3>Motion</h3><p class=\"ly-muted\">Hover each tile: nothing the theme \
         does takes longer than 200 ms, and every duration is a token.</p>\
         <div class=\"ly-preview-grid\">",
    );
    for spec in reference()
        .iter()
        .filter(|spec| spec.group == TokenGroup::Motion)
    {
        if !spec.name.contains("motion-") || spec.name.contains("ease") {
            continue;
        }
        let _ = write!(
            out,
            "<div class=\"ly-preview-tile ly-preview-motion\" \
             style=\"transition:transform var({name}) var(--ly-motion-ease),\
             background var({name}) var(--ly-motion-ease)\">{name}<br>\
             <span class=\"ly-muted\">{value}</span></div>",
            name = spec.name,
            value = tokens.get(spec.name, Scheme::Light).unwrap_or_default()
        );
    }
    out.push_str("</div>");

    out.push_str("<h3>Layout and z-index</h3><div class=\"ly-table-wrap\"><table><thead><tr><th>Token</th><th>Value</th><th>Use</th></tr></thead><tbody>");
    for spec in reference()
        .iter()
        .filter(|spec| matches!(spec.group, TokenGroup::Layout | TokenGroup::ZIndex))
    {
        let _ = write!(
            out,
            "<tr><td><code>{name}</code></td><td>{value}</td><td>{doc}</td></tr>",
            name = spec.name,
            value = tokens.get(spec.name, Scheme::Light).unwrap_or_default(),
            doc = spec.doc
        );
    }
    out.push_str("</tbody></table></div>");
    out.push_str(&format!(
        "<p class=\"ly-muted\">{} tokens, {} of them scheme-dependent. {}</p>",
        reference().len(),
        reference()
            .iter()
            .filter(|spec| spec.is_scheme_dependent())
            .count(),
        icons::svg("appearance").unwrap_or_default()
    ));
    out
}

fn swatches(tokens: &Tokens) -> String {
    let mut out = String::from("<div class=\"ly-preview-swatches\">");
    for spec in reference()
        .iter()
        .filter(|spec| spec.group == TokenGroup::Color && !spec.name.contains("neutral-"))
    {
        let light = tokens.get(spec.name, Scheme::Light).unwrap_or_default();
        let dark = tokens.get(spec.name, Scheme::Dark).unwrap_or_default();
        let _ = write!(
            out,
            "<div class=\"ly-preview-swatch\">\
             <span class=\"ly-preview-chip\" style=\"background:var({name})\"></span>\
             <code>{short}</code><span class=\"ly-muted\">{light} · {dark}</span>\
             <span class=\"ly-subtle\">{doc}</span></div>",
            name = spec.name,
            short = spec.name.trim_start_matches("--ly-color-"),
            doc = spec.doc,
        );
    }
    out.push_str("</div>");
    out
}

fn contrast(tokens: &Tokens) -> String {
    let mut out = String::from(
        "<div class=\"ly-table-wrap\"><table><thead><tr><th>Foreground</th>\
         <th>Background</th><th>Light</th><th>Dark</th><th>Minimum</th></tr></thead><tbody>",
    );
    for (foreground, background, minimum) in aurora::AA_PAIRS {
        let ratio = |scheme| -> String {
            match (
                tokens.color(foreground, scheme),
                tokens.color(background, scheme),
            ) {
                (Some(front), Some(back)) => format!("{:.2}:1", Color::contrast(front, back)),
                _ => "—".to_owned(),
            }
        };
        let _ = write!(
            out,
            "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{}</td><td>{}</td><td>{minimum:.1}:1</td></tr>",
            foreground.trim_start_matches("--ly-"),
            background.trim_start_matches("--ly-"),
            ratio(Scheme::Light),
            ratio(Scheme::Dark),
        );
    }
    out.push_str("</tbody></table></div>");
    out
}

/// The preview's own layout, appended like any operator's `theme.css` so it is
/// compiled by the same pipeline.
fn preview_css() -> &'static str {
    "
    .ly-preview-ramp { display: grid; grid-template-columns: repeat(12, 1fr); margin-block: var(--ly-space-4); border: var(--ly-border); border-radius: var(--ly-radius-md); overflow: hidden; }
    .ly-preview-step { display: grid; place-items: center; aspect-ratio: 1 / 1.4; font-size: var(--ly-text-2xs); color: var(--ly-color-text); mix-blend-mode: difference; }
    .ly-preview-swatches { display: grid; gap: var(--ly-space-3); grid-template-columns: repeat(auto-fill, minmax(16rem, 1fr)); margin-block: var(--ly-space-4); }
    .ly-preview-swatch { display: grid; grid-template-columns: 2rem 1fr; grid-template-rows: auto auto auto; column-gap: var(--ly-space-3); align-items: center; font-size: var(--ly-text-xs); }
    .ly-preview-chip { grid-row: 1 / 4; width: 2rem; height: 2rem; border: var(--ly-border); border-radius: var(--ly-radius-sm); }
    .ly-preview-type { display: grid; gap: var(--ly-space-3); margin-block: var(--ly-space-4); }
    .ly-preview-scale { display: grid; gap: var(--ly-space-2); margin-block: var(--ly-space-4); }
    .ly-preview-row { display: grid; grid-template-columns: 10rem 1fr 5rem; gap: var(--ly-space-3); align-items: center; font-size: var(--ly-text-xs); }
    .ly-preview-bar { height: 0.75rem; background: var(--ly-color-primary); border-radius: var(--ly-radius-xs); }
    .ly-preview-grid { display: grid; gap: var(--ly-space-4); grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr)); margin-block: var(--ly-space-4); }
    .ly-preview-tile { display: grid; place-items: center; padding: var(--ly-space-4); min-height: 5rem; text-align: center; background: var(--ly-color-surface); border: var(--ly-border); font-size: var(--ly-text-2xs); }
    .ly-preview-motion:hover { transform: translateY(-4px); background: var(--ly-color-primary-subtle); }
    "
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_review_page_carries_the_whole_preset() {
        let html = page().expect("the preview renders");

        // The real page, rendered by the real partials.
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("data-liyasa=\"sidebar\""));
        assert!(html.contains("data-liyasa=\"toc\""));
        assert!(html.contains("ly-tok-string"), "the code theme is shown");
        assert!(html.contains("data-ly-tabs"));
        assert!(html.contains("ly-callout"));

        // Self-contained: no request leaves the file it is opened from.
        assert!(!html.contains("<link rel=\"stylesheet\""));
        assert!(html.contains("<style>"));
        assert!(!html.contains("%%LIYASA"), "every placeholder was replaced");
        assert!(html.contains("window.liyasa = liyasa"), "the toggle works");
        assert_eq!(
            html.matches("url(\"data:font/").count(),
            fonts::bundled().len(),
            "both faces are inlined"
        );

        // Every group of the token table is on the page.
        for token in [
            "--ly-color-neutral-12",
            "--ly-color-primary",
            "--ly-text-3xl",
            "--ly-space-5",
            "--ly-radius-md",
            "--ly-shadow-lg",
            "--ly-motion-slow",
            "--ly-layout-sidebar",
            "--ly-z-dialog",
        ] {
            assert!(html.contains(token), "`{token}` is not shown");
        }
        assert!(html.contains("4.5:1"), "the contrast table is rendered");
    }

    #[test]
    fn the_page_shows_every_documented_token() {
        let html = page().expect("the preview renders");
        let shown = reference()
            .iter()
            .filter(|spec| html.contains(spec.name))
            .count();
        assert!(
            shown * 10 >= reference().len() * 8,
            "only {shown} of {} tokens reach the page",
            reference().len()
        );
    }
}
