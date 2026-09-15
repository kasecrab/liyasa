//! The theme's side of a build: the stylesheet, the runtime, and the page
//! shell every route is rendered into (PRD §10, RX-02).
//!
//! CSS and JS are hashed into their file names, because they are immutable
//! once built and a reader should never receive a stale one.

use liyasa_core::build::Variant;
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::ids::Fingerprint;
use liyasa_theme::config::ThemeConfig;
use liyasa_theme::context::{self, Mode, RenderContext};
use liyasa_theme::runtime::Runtime;
use liyasa_theme::stylesheet::Styles;
use liyasa_theme::theme::Theme;
use liyasa_theme::tokens::Tokens;
use serde_json::Value;

use super::settings::Settings;
use crate::tree;

/// One built file and where it goes under `dist/`.
#[derive(Debug, Clone)]
pub struct File {
    pub path: String,
    pub bytes: String,
}

/// What the theme contributes to a build.
pub struct Assets {
    pub files: Vec<File>,
    pub stylesheet_url: String,
    pub script_url: String,
    pub critical: String,
    pub bootstrap: String,
    /// Covers the theme's config and its output, so a theme change invalidates
    /// every cached page.
    pub fingerprint: Fingerprint,
    theme: Option<Theme>,
}

/// Compiles the theme's assets. A theme that will not compile is reported and
/// the build continues with no stylesheet, because unstyled pages beat none.
pub fn build(config: &Value, settings: &Settings, report: &mut super::Report) -> Assets {
    let theme_config: ThemeConfig = config
        .get("theme")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .ok()
        .flatten()
        .unwrap_or_default();

    let (tokens, token_diagnostics) = Tokens::from_config(&theme_config);
    report.diagnostics.extend(token_diagnostics.into_vec());

    let (css, critical) = match Styles::build(&theme_config, &tokens, &[]) {
        Ok(styles) => (styles.css, styles.critical),
        Err(error) => {
            report.diagnostics.push(
                Diagnostic::new(
                    code::E0703,
                    format!("the stylesheet did not compile: {error}"),
                )
                .help("the build continued without it"),
            );
            (String::new(), String::new())
        }
    };
    let runtime = Runtime::build(&theme_config);

    let base = settings.base_path.trim_end_matches('/').to_owned();
    let css_name = hashed("_liyasa/theme", &css, "css");
    let js_name = hashed("_liyasa/theme", &runtime.base, "js");
    let fingerprint = Fingerprint::of_parts([
        css.as_bytes(),
        runtime.base.as_bytes(),
        serde_json::to_string(&theme_config)
            .unwrap_or_default()
            .as_bytes(),
    ]);

    let theme = match Theme::new() {
        Ok(theme) => Some(theme),
        Err(error) => {
            report.diagnostics.push(Diagnostic::new(
                code::E0202,
                format!("the theme's templates did not load: {error}"),
            ));
            None
        }
    };

    Assets {
        stylesheet_url: format!("{base}/{css_name}"),
        script_url: format!("{base}/{js_name}"),
        files: vec![
            File {
                path: css_name,
                bytes: css,
            },
            File {
                path: js_name,
                bytes: runtime.base.clone(),
            },
        ],
        critical,
        bootstrap: runtime.bootstrap.to_owned(),
        fingerprint,
        theme,
    }
}

/// Wraps one page's body in the theme's shell.
pub fn page_html(
    settings: &Settings,
    assets: &Assets,
    page: &tree::Page,
    variant: &Variant,
    body: &str,
    diagnostics: &mut Diagnostics,
) -> String {
    let context = render_context(settings, assets, page, variant, body);
    let Some(theme) = &assets.theme else {
        return body.to_owned();
    };
    match theme.render_page(&context) {
        Ok(html) => html,
        Err(error) => {
            diagnostics.push(
                Diagnostic::new(
                    code::E0202,
                    format!("`{}` could not be laid out: {error}", page.route),
                )
                .help("the page's own HTML was written without the theme's shell"),
            );
            body.to_owned()
        }
    }
}

fn render_context(
    settings: &Settings,
    assets: &Assets,
    page: &tree::Page,
    variant: &Variant,
    body: &str,
) -> RenderContext {
    let title = page
        .front
        .title
        .clone()
        .unwrap_or_else(|| page.route.as_str().to_owned());
    let mut context = RenderContext {
        page: context::Page {
            route: page.route.as_str().to_owned(),
            title: title.clone(),
            description: page.front.description.clone().unwrap_or_default(),
            mode: mode_of(page),
            content: body.to_owned(),
            markdown_url: super::markdown_url(&settings.base_path, &page.route),
            og: context::Og {
                title,
                description: page.front.description.clone().unwrap_or_default(),
                image: None,
            },
            ..context::Page::default()
        },
        site: context::Site {
            name: settings.name.clone(),
            description: settings.description.clone(),
            origin: settings.canonical_origin.clone(),
            base_path: settings.base_path.clone(),
            locale: variant
                .locale
                .as_ref()
                .map(|locale| locale.as_str().to_owned())
                .unwrap_or_else(|| settings.locale.clone()),
            version: variant
                .version
                .as_ref()
                .map(|version| version.as_str().to_owned()),
            direction: "ltr".to_owned(),
            built_with: true,
            built_with_url: liyasa_core::site::SITE_URL.to_owned(),
            ..context::Site::default()
        },
        assets: context::Assets {
            stylesheet: assets.stylesheet_url.clone(),
            script: assets.script_url.clone(),
            critical: assets.critical.clone(),
            bootstrap: assets.bootstrap.clone(),
            ..context::Assets::default()
        },
        ..RenderContext::default()
    };
    context.assets.page_data = context::page_data(&context);
    context
}

fn mode_of(page: &tree::Page) -> Mode {
    match page.front.mode {
        Some(liyasa_core::frontmatter::PageMode::Wide) => Mode::Wide,
        Some(liyasa_core::frontmatter::PageMode::Custom) => Mode::Custom,
        Some(liyasa_core::frontmatter::PageMode::Frame) => Mode::Frame,
        Some(liyasa_core::frontmatter::PageMode::Center) => Mode::Center,
        Some(liyasa_core::frontmatter::PageMode::Assistant) => Mode::Assistant,
        _ => Mode::Default,
    }
}

/// `<prefix>.<digest>.<extension>`: the file name carries the hash, so the URL
/// changes when the bytes do (RX-02).
fn hashed(prefix: &str, bytes: &str, extension: &str) -> String {
    let digest = Fingerprint::of(bytes).to_hex();
    format!("{prefix}.{}.{extension}", &digest[..16])
}
