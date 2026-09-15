//! `liyasa build`: the driver that turns a project into `dist/` (PRD §6.6,
//! CLI-03).
//!
//! The order is fixed because the queries depend on each other: the git
//! snapshot and the clock first, because everything dated reads them; then the
//! config, the content tree, the theme's assets, every page, the files a page
//! links to, and finally the manifest that names all of it.
//!
//! Reads go through the `Vfs`; writes go straight to the file system, because
//! `dist/` is the engine's own output and no other crate reads it through the
//! seam.

pub mod settings;
pub mod theme;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use liyasa_components::registry::Registry;
use liyasa_core::build::{ArtifactCache, Variant};
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::ids::{BuildId, Fingerprint, Locale, Route};
use liyasa_core::markdown::{SiteMeta, TemplateContext};
use liyasa_core::net::Url;
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::{Vfs, VfsPath};
use rayon::prelude::*;

use crate::cache::{DiskCache, Index, InputStamp, Row};
use crate::git::{GitMeta, GitSnapshot};
use crate::images::codec::ImageCodec;
use crate::manifest::{
    AssetEntry, ImageEntry, ImageVariantEntry, Manifest, RouteEntry, VariantEntry,
};
use crate::redirects::Table;
use crate::variants::{self, Coordinates, Mode, Reads};
use crate::{assets, clock, images, manifest, redirects, render, tree};

pub use settings::Settings;

/// `.liyasa`, the project's own cache directory.
pub const CACHE_DIR: &str = ".liyasa";
pub const GIT_META: &str = "git-meta.json";

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Overrides `build.output`.
    pub output: Option<PathBuf>,
    /// `--clean`: start from an empty `dist/` and an empty cache.
    pub clean: bool,
    /// `--drafts`.
    pub drafts: bool,
    /// `--strict`: warnings are errors.
    pub strict: bool,
    /// `--base-path`.
    pub base_path: Option<String>,
    /// `--env`: the config overlay to merge.
    pub env: Option<String>,
    /// `--build-time`, in seconds since the Unix epoch.
    pub build_time: Option<i64>,
    /// `--profile`: keep a timing per phase.
    pub profile: bool,
    /// `--images`: run the image pre-pass in this build.
    pub eager_images: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub build_id: Option<BuildId>,
    pub clock_unix: i64,
    pub pages: usize,
    pub variants: usize,
    pub dynamic: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub images_generated: u64,
    /// Every file this build's output consists of.
    pub written: Vec<String>,
    /// How many of them actually reached the disk; the rest were already there.
    pub rewritten: usize,
    pub timings: Vec<(&'static str, Duration)>,
    pub manifest: Option<Manifest>,
    pub diagnostics: Diagnostics,
}

impl Report {
    /// Whether this build should exit non-zero. `--strict` promotes warnings,
    /// which is the only thing the flag does (CLI-03).
    pub fn failed(&self, strict: bool) -> bool {
        self.diagnostics.has_errors() || (strict && !self.diagnostics.is_empty())
    }

    pub fn wrote(&self, path: &str) -> bool {
        self.written.iter().any(|written| written == path)
    }
}

struct Phase {
    started: Instant,
    timings: Vec<(&'static str, Duration)>,
}

impl Phase {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            timings: Vec::new(),
        }
    }

    fn mark(&mut self, name: &'static str) {
        self.timings.push((name, self.started.elapsed()));
        self.started = Instant::now();
    }
}

/// Builds the project rooted at `root`, reading through `vfs`.
pub fn build(vfs: &dyn Vfs, git: &dyn GitMeta, root: &Path, options: &Options) -> Report {
    let mut report = Report::default();
    let mut phase = Phase::new();
    let mut sources = SourceMap::new();

    // 1. Config.
    let load = liyasa_config::load(
        vfs,
        &mut sources,
        &liyasa_config::Options {
            root: VfsPath::new(""),
            env: options.env.clone(),
        },
    );
    report.diagnostics.extend(load.diagnostics.into_vec());
    let mut settings = Settings::from_value(&load.value);
    if let Some(base_path) = &options.base_path {
        settings.base_path = base_path.clone();
        settings.images.base_path = base_path.clone();
    }
    let config_fingerprint = Fingerprint::of(load.value.to_string());
    phase.mark("config");

    // 2. Content tree.
    let tree = tree::discover(
        vfs,
        &mut sources,
        &tree::Options {
            output: settings.output.clone(),
            drafts: options.drafts || settings.drafts,
        },
    );
    report
        .diagnostics
        .extend(tree.diagnostics.as_slice().to_vec());

    // CM-90: the pages each declared version serves, from its own tree or from
    // the shared one.
    let declared = crate::versions::Versions::new(&settings.versions);
    let mut tree = tree;
    tree.pages = crate::versions::expand(tree.pages, &declared);
    phase.mark("content_tree");

    // 3. Git-derived data, frozen once and fingerprinted (§6.6.2 rule 2).
    let snapshot = GitSnapshot::take(git, tree.pages.iter().map(|page| page.path.clone()));
    let cache_root = root.join(CACHE_DIR);
    if options.clean {
        let _ = std::fs::remove_dir_all(&cache_root);
    }
    let git_meta = serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_owned());
    write_meta(&cache_root, GIT_META, git_meta.as_bytes(), &mut report);

    // 4. Build clock.
    let resolved = clock::resolve(&clock::Inputs::of_build(options.build_time, &snapshot));
    report.diagnostics.extend(resolved.diagnostics.into_vec());
    report.clock_unix = clock::unix_seconds(resolved.clock);
    phase.mark("clock");

    let mut outputs = if options.clean {
        Outputs::default()
    } else {
        Outputs::load(&cache_root)
    };

    // 5. Output directory.
    let output = options
        .output
        .clone()
        .unwrap_or_else(|| root.join(&settings.output));
    if options.clean {
        let _ = std::fs::remove_dir_all(&output);
    }

    // 6. Theme assets.
    let assets_built = theme::build(&load.value, &settings, &mut report);
    for file in &assets_built.files {
        write_file(
            &output,
            &file.path,
            file.bytes.as_bytes(),
            &mut report,
            &mut outputs,
        );
    }
    phase.mark("theme");

    // 7. Pages.
    let cache = DiskCache::new(cache_root.join("cache"));
    let (mut index, index_diagnostics) = Index::load(&cache_root.join("cache").join(Index::FILE));
    report.diagnostics.extend(index_diagnostics.into_vec());

    let registry = Registry::builtins();
    let site = site_meta(&settings);

    // §6.6 query 7: one navigation per version, resolved once and shared by
    // every page that version serves.
    let mut navigations: BTreeMap<
        Option<liyasa_core::ids::Version>,
        liyasa_theme::nav::Navigation,
    > = BTreeMap::new();
    let mut version_keys: Vec<Option<liyasa_core::ids::Version>> =
        tree.pages.iter().map(|page| page.version.clone()).collect();
    version_keys.sort();
    version_keys.dedup();
    for version in version_keys {
        let resolved = crate::nav::resolve(&load.value, &tree, &declared, version.as_ref());
        report.diagnostics.extend(resolved.diagnostics.into_vec());
        navigations.insert(version, resolved.navigation);
    }
    let all_routes: BTreeSet<Route> = tree.pages.iter().map(|page| page.route.clone()).collect();
    let pages = render_pages(
        &tree,
        &sources,
        &all_routes,
        &settings,
        &registry,
        &site,
        &cache,
        &assets_built,
        &navigations,
        config_fingerprint,
    );
    phase.mark("pages");

    // 8. What the pages produced.
    let mut routes = Vec::new();
    let mut referenced: BTreeSet<VfsPath> = BTreeSet::new();
    let mut entries: Vec<crate::changelog::Entry> = Vec::new();
    for outcome in &pages {
        report
            .diagnostics
            .extend(outcome.diagnostics.as_slice().to_vec());
        report.cache_hits += outcome.cache_hits;
        report.cache_misses += outcome.cache_misses;
        report.variants += outcome.variants.len();
        if outcome.dynamic {
            report.dynamic += 1;
        }
        referenced.extend(outcome.referenced.iter().cloned());
        entries.extend(outcome.changelog.iter().cloned());

        let mut entries = Vec::new();
        for (key, path, html) in &outcome.variants {
            write_file(&output, path, html.as_bytes(), &mut report, &mut outputs);
            entries.push(VariantEntry {
                key: key.clone(),
                path: path.clone(),
                hash: Fingerprint::of(html),
            });
        }
        index.record(
            "page_html",
            outcome.route.as_str(),
            Row {
                inputs: InputStamp::of(vfs, &outcome.source)
                    .map(|stamp| vec![stamp])
                    .unwrap_or_default(),
                derived: vec![config_fingerprint, assets_built.fingerprint],
                output: Fingerprint::of(&outcome.markdown),
            },
        );
        routes.push(RouteEntry {
            route: outcome.route.clone(),
            source: outcome.source.as_str().to_owned(),
            markdown: markdown_url(&settings.base_path, &outcome.route),
            hidden: outcome.hidden,
            dynamic: outcome.dynamic,
            variants: entries,
        });
    }
    report.pages = pages.len();
    if let Some(over) = variants::check_site_cap(report.variants, &settings.caps) {
        report.diagnostics.push(over);
    }
    phase.mark("write_pages");

    // 8b. The changelog stream and its feeds (CM-120, CM-121).
    //
    // A directory of dated files is a stream too: each file is its own page and
    // an entry in the index the build writes at `/changelog`.
    for page in &tree.pages {
        if !crate::changelog::is_entry_file(page.path.as_str()) {
            continue;
        }
        if let Some(entry) = crate::changelog::from_file(
            page.path.as_str(),
            &page.route,
            page.front.title.as_deref(),
            &page.front.keywords,
            page.front.tag.as_deref(),
            page.front.description.as_deref().unwrap_or_default(),
        ) {
            entries.push(entry);
        }
    }
    let entries = crate::changelog::stream(entries);
    let stream_route = Route::new(format!("/{}", crate::changelog::DIRECTORY));
    let has_stream_page = tree.pages.iter().any(|page| page.route == stream_route);
    if !entries.is_empty() && !has_stream_page {
        let stream_page = tree::Page {
            path: VfsPath::new(format!("{}/index.md", crate::changelog::DIRECTORY)),
            route: stream_route.clone(),
            base_route: stream_route.clone(),
            version: None,
            fingerprint: Fingerprint::of("generated changelog stream"),
            front: liyasa_core::frontmatter::FrontmatterFields {
                title: Some("Changelog".to_owned()),
                ..liyasa_core::frontmatter::FrontmatterFields::default()
            },
            indexing: tree::Indexing {
                navigation: true,
                sitemap: true,
                search: true,
                ai: true,
            },
            hidden: false,
            draft: false,
        };
        let navigation = navigations.get(&None).cloned().unwrap_or_default();
        let mut stream_diagnostics = Diagnostics::new();
        let html = theme::page_html(
            &settings,
            &assets_built,
            &stream_page,
            &Variant::default(),
            &crate::changelog::stream_html(&entries),
            &navigation,
            &mut stream_diagnostics,
        );
        report.diagnostics.extend(stream_diagnostics.into_vec());
        write_file(
            &output,
            "changelog/index.html",
            html.as_bytes(),
            &mut report,
            &mut outputs,
        );
    }
    if !entries.is_empty() {
        let origin = settings.canonical_origin.trim_end_matches('/');
        for (path, body) in [
            (
                crate::changelog::RSS_PATH,
                crate::changelog::rss(&entries, &settings.name, origin),
            ),
            (
                crate::changelog::ATOM_PATH,
                crate::changelog::atom(&entries, &settings.name, origin),
            ),
            (
                crate::changelog::JSON_PATH,
                crate::changelog::json_feed(&entries, &settings.name, origin),
            ),
        ] {
            write_file(&output, path, body.as_bytes(), &mut report, &mut outputs);
        }
    }

    // 8c. The agent surfaces (§11.7, §11.8, RX-03): Markdown routes,
    // `llms.txt`, the skill, the sitemap, robots, and the feeds. WP-10 owns
    // what they say; the engine owns which pages reach them (CM-80).
    let surfaces_written = write_surfaces(
        &output,
        &tree,
        &pages,
        &settings,
        &navigations,
        report.clock_unix,
        &mut report,
        &mut outputs,
    );
    phase.mark("agent_surfaces");
    let _ = surfaces_written;

    // 8d. The sitemap: every route CM-80 lets into it.
    if !settings.canonical_origin.is_empty() {
        let entries: Vec<crate::sitemap::Entry> = tree
            .pages
            .iter()
            .filter(|page| page.indexing.sitemap && !page.draft)
            .map(|page| crate::sitemap::Entry {
                route: page.route.clone(),
                updated: page.front.updated.clone(),
            })
            .collect();
        let xml = crate::sitemap::render(&entries, &settings.canonical_origin, report.clock_unix);
        write_file(
            &output,
            crate::sitemap::FILE,
            xml.as_bytes(),
            &mut report,
            &mut outputs,
        );
    }

    // 9. Assets and the image tier.
    let (asset_entries, image_entries, generated, headers) = copy_assets(
        vfs,
        root,
        &output,
        &tree,
        &referenced,
        &settings,
        options,
        &cache,
        &mut report,
        &mut outputs,
    );
    report.images_generated = generated;
    phase.mark("assets");

    // 10. Redirects and headers.
    let (table, redirect_diagnostics) =
        Table::compile(&settings.redirects, &settings.external_allow);
    report.diagnostics.extend(redirect_diagnostics.into_vec());
    if !table.is_empty() {
        write_file(
            &output,
            "_redirects",
            redirects::netlify(&table).as_bytes(),
            &mut report,
            &mut outputs,
        );
        write_file(
            &output,
            "vercel.json",
            redirects::vercel(&table).as_bytes(),
            &mut report,
            &mut outputs,
        );
    }

    // 11. The manifest.
    let mut inputs: BTreeMap<String, Fingerprint> = tree
        .pages
        .iter()
        .map(|page| (page.path.as_str().to_owned(), page.fingerprint))
        .collect();
    inputs.insert("liyasa.json".to_owned(), config_fingerprint);
    inputs.insert(
        format!("{CACHE_DIR}/{GIT_META}"),
        Fingerprint::of(&git_meta),
    );
    for name in &settings.env {
        if let Ok(value) = std::env::var(name) {
            inputs.insert(format!("env:{name}"), Fingerprint::of(value));
        }
    }
    let lockfile = vfs
        .fingerprint(&VfsPath::new("liyasa.lock"))
        .ok()
        .or_else(|| vfs.fingerprint(&VfsPath::new("Cargo.lock")).ok());

    let built = Manifest {
        build_id: manifest::build_id(&inputs, report.clock_unix, lockfile),
        liyasa_version: crate::cache::VERSION.to_owned(),
        built_at: report.clock_unix,
        base_path: settings.base_path.clone(),
        routes,
        assets: asset_entries,
        images: image_entries,
        redirects: table.manifest_entries(),
        inputs,
    }
    .sorted();
    write_file(
        &output,
        manifest::FILE,
        built.to_json().as_bytes(),
        &mut report,
        &mut outputs,
    );
    if !headers.is_empty() {
        write_file(
            &output,
            "_headers",
            headers.as_bytes(),
            &mut report,
            &mut outputs,
        );
    }

    report.build_id = Some(built.build_id);
    report.manifest = Some(built);

    let _ = index.save(&cache_root.join("cache").join(Index::FILE));
    outputs.save(&cache_root);
    phase.mark("manifest");

    if options.profile {
        report.timings = phase.timings;
    }
    report.written.sort();
    report
}

/// `liyasa build --check-determinism`: builds twice and diffs (§6.6.2 item 6).
///
/// `None` is a build that reproduced; the diagnostic names what moved.
pub fn check_determinism(
    vfs: &dyn Vfs,
    git: &dyn GitMeta,
    root: &Path,
    options: &Options,
) -> Option<Diagnostic> {
    let first = build_into(vfs, git, root, options, "determinism-a");
    let second = build_into(vfs, git, root, options, "determinism-b");
    let differences: Vec<String> = first
        .keys()
        .chain(second.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|path| first.get(*path) != second.get(*path))
        .cloned()
        .collect();

    if differences.is_empty() {
        return None;
    }
    Some(
        Diagnostic::new(
            code::E0706,
            format!(
                "two builds of the same inputs differ in {} file(s): {}",
                differences.len(),
                differences
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )
        .help("a build must not read the wall clock, the environment, or iteration order"),
    )
}

fn build_into(
    vfs: &dyn Vfs,
    git: &dyn GitMeta,
    root: &Path,
    options: &Options,
    name: &str,
) -> BTreeMap<String, Fingerprint> {
    let output =
        std::env::temp_dir().join(format!("liyasa-{name}-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&output);
    let report = build(
        vfs,
        git,
        root,
        &Options {
            output: Some(output.clone()),
            ..options.clone()
        },
    );
    let mut out = BTreeMap::new();
    for path in &report.written {
        if let Ok(bytes) = std::fs::read(output.join(path)) {
            out.insert(path.clone(), Fingerprint::of(bytes));
        }
    }
    let _ = std::fs::remove_dir_all(&output);
    out
}

/// One page, rendered at every variant it has.
struct Outcome {
    route: Route,
    source: VfsPath,
    hidden: bool,
    dynamic: bool,
    markdown: String,
    /// `(variant key, output path, html)`.
    variants: Vec<(String, String, String)>,
    referenced: Vec<VfsPath>,
    /// `::update` entries this page holds (CM-120).
    changelog: Vec<crate::changelog::Entry>,
    cache_hits: usize,
    cache_misses: usize,
    diagnostics: Diagnostics,
}

#[allow(clippy::too_many_arguments)]
fn render_pages(
    tree: &tree::Tree,
    sources: &SourceMap,
    all_routes: &BTreeSet<Route>,
    settings: &Settings,
    registry: &Registry,
    site: &SiteMeta,
    cache: &DiskCache,
    assets_built: &theme::Assets,
    navigations: &BTreeMap<Option<liyasa_core::ids::Version>, liyasa_theme::nav::Navigation>,
    config_fingerprint: Fingerprint,
) -> Vec<Outcome> {
    // §6.6: pages render in parallel with rayon.
    tree.pages
        .par_iter()
        .map(|page| {
            let mut diagnostics = Diagnostics::new();
            let Some(id) = sources.find(&page.path) else {
                return Outcome {
                    route: page.route.clone(),
                    source: page.path.clone(),
                    hidden: page.hidden,
                    dynamic: false,
                    markdown: String::new(),
                    variants: Vec::new(),
                    referenced: Vec::new(),
                    changelog: Vec::new(),
                    cache_hits: 0,
                    cache_misses: 0,
                    diagnostics,
                };
            };
            let text = sources.get(id).text.clone();
            let (source, scan) = liyasa_markdown::scan(&text, id);
            diagnostics.extend(scan.into_vec());

            let mut reads = Reads {
                groups: page.front.groups.iter().cloned().collect(),
                regions: page
                    .front
                    .regions
                    .as_ref()
                    .and_then(|gate| gate.only.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .collect(),
                personalized: page.front.personalized.unwrap_or(false),
                ..Reads::default()
            };
            let coordinates = Coordinates {
                versions: page
                    .version
                    .clone()
                    .map(|version| vec![version])
                    .unwrap_or_default(),
                locales: page.front.locales.clone(),
                products: page.front.product.iter().cloned().collect(),
            };

            let options = render::Options::new(registry, site).anonymous();
            let mut outcome = variants::of_page(&page.route, &reads, &coordinates, &settings.caps);

            // The Markdown and the link set are cached beside the HTML: a warm
            // build renders nothing, and `<route>.md` and the files a page
            // links to must still be written.
            let markdown_key =
                crate::cache::key("page_markdown", &[page.fingerprint, config_fingerprint]);
            let deps_key = crate::cache::key("page_deps", &[page.fingerprint, config_fingerprint]);
            let changelog_key =
                crate::cache::key("page_changelog", &[page.fingerprint, config_fingerprint]);
            let mut markdown = cache
                .get(&markdown_key)
                .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
                .unwrap_or_default();
            let mut rendered: Vec<(String, String, String)> = Vec::new();
            let mut referenced: Vec<VfsPath> = cache
                .get(&deps_key)
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            let mut changelog: Vec<crate::changelog::Entry> = cache
                .get(&changelog_key)
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            let mut recorded = false;
            let (mut hits, mut misses) = (0usize, 0usize);
            let mut converged = false;

            for _ in 0..settings.caps.iterations.max(1) {
                rendered.clear();
                let mut grew = false;
                for variant in variant_set(&outcome) {
                    let navigation_fingerprint = navigations
                        .get(&page.version)
                        .and_then(|navigation| serde_json::to_vec(navigation).ok())
                        .map(Fingerprint::of)
                        .unwrap_or_else(|| Fingerprint::of("no navigation"));
                    let key = crate::cache::key(
                        "page_html",
                        &[
                            page.fingerprint,
                            config_fingerprint,
                            assets_built.fingerprint,
                            navigation_fingerprint,
                            Fingerprint::of(variants::key(&variant)),
                        ],
                    );
                    let context = template_context(settings, page, &variant);
                    let html = match cache.get(&key) {
                        Some(cached) => {
                            hits += 1;
                            String::from_utf8(cached.to_vec()).unwrap_or_default()
                        }
                        None => {
                            misses += 1;
                            let page_render = render::page(sources, &source, &context, &options);
                            diagnostics.extend(page_render.diagnostics.as_slice().to_vec());
                            grew |= reads.absorb(&page_render.record);
                            if !recorded {
                                recorded = true;
                                markdown = agent_markdown(
                                    &page_render,
                                    page,
                                    registry,
                                    site,
                                    all_routes,
                                    &mut diagnostics,
                                );
                                referenced = referenced_files(&page_render);
                                changelog = page_render
                                    .document
                                    .as_ref()
                                    .map(|document| {
                                        crate::changelog::from_page(&page.route, document)
                                    })
                                    .unwrap_or_default();
                            }
                            let navigation =
                                navigations.get(&page.version).cloned().unwrap_or_default();
                            let html = theme::page_html(
                                settings,
                                assets_built,
                                page,
                                &variant,
                                &page_render.html,
                                &navigation,
                                &mut diagnostics,
                            );
                            let _ = cache.put(
                                &key,
                                liyasa_core::vfs::Bytes::from(html.clone().into_bytes()),
                                &[page.fingerprint, config_fingerprint],
                            );
                            html
                        }
                    };
                    let paths = variants::output_paths(&page.route, std::slice::from_ref(&variant));
                    let variant_key = variants::key(&variant);
                    if let Some(path) = paths.get(&variant_key) {
                        rendered.push((variant_key, path.clone(), html));
                    }
                }
                if !grew {
                    converged = true;
                    break;
                }
                outcome = variants::of_page(&page.route, &reads, &coordinates, &settings.caps);
            }
            if !converged {
                diagnostics.push(variants::did_not_converge(&page.route, &settings.caps));
            }
            diagnostics.extend(outcome.diagnostics.as_slice().to_vec());

            // A cache that lost the Markdown but kept the HTML still has to
            // produce `<route>.md`, so one render fills what is missing.
            if !recorded && markdown.is_empty() {
                let variant = variant_set(&outcome).first().cloned().unwrap_or_default();
                let context = template_context(settings, page, &variant);
                let page_render = render::page(sources, &source, &context, &options);
                diagnostics.extend(page_render.diagnostics.as_slice().to_vec());
                markdown = agent_markdown(
                    &page_render,
                    page,
                    registry,
                    site,
                    all_routes,
                    &mut diagnostics,
                );
                referenced = referenced_files(&page_render);
                changelog = page_render
                    .document
                    .as_ref()
                    .map(|document| crate::changelog::from_page(&page.route, document))
                    .unwrap_or_default();
                recorded = true;
            }
            if recorded {
                let _ = cache.put(
                    &markdown_key,
                    liyasa_core::vfs::Bytes::from(markdown.clone().into_bytes()),
                    &[page.fingerprint, config_fingerprint],
                );
                if let Ok(encoded) = serde_json::to_vec(&referenced) {
                    let _ = cache.put(
                        &deps_key,
                        liyasa_core::vfs::Bytes::from(encoded),
                        &[page.fingerprint, config_fingerprint],
                    );
                }
                if let Ok(encoded) = serde_json::to_vec(&changelog) {
                    let _ = cache.put(
                        &changelog_key,
                        liyasa_core::vfs::Bytes::from(encoded),
                        &[page.fingerprint, config_fingerprint],
                    );
                }
            }

            Outcome {
                route: page.route.clone(),
                source: page.path.clone(),
                hidden: page.hidden,
                dynamic: outcome.mode == Mode::Dynamic,
                markdown,
                variants: rendered,
                referenced,
                changelog,
                cache_hits: hits,
                cache_misses: misses,
                diagnostics,
            }
        })
        .collect()
}

/// A dynamic page still ships its default variant (§6.6.3 item 4).
fn variant_set(outcome: &variants::Outcome) -> Vec<Variant> {
    match outcome.mode {
        Mode::Static => outcome.variants.clone(),
        Mode::Dynamic => outcome
            .variants
            .first()
            .cloned()
            .map(|variant| vec![variant])
            .unwrap_or_else(|| vec![Variant::default()]),
    }
}

fn template_context(settings: &Settings, page: &tree::Page, variant: &Variant) -> TemplateContext {
    let env: BTreeMap<String, String> = settings
        .env
        .iter()
        .filter_map(|name| std::env::var(name).ok().map(|value| (name.clone(), value)))
        .collect();
    TemplateContext {
        values: minijinja::context! {
            site => minijinja::context! {
                name => settings.name.clone(),
                description => settings.description.clone(),
                url => settings.canonical_origin.clone(),
                basePath => settings.base_path.clone(),
            },
            page => minijinja::context! {
                title => page.front.title.clone(),
                description => page.front.description.clone(),
                route => page.route.as_str(),
            },
            version => variant.version.as_ref().map(|v| v.as_str().to_owned()),
            locale => variant
                .locale
                .clone()
                .unwrap_or_else(|| Locale::new(settings.locale.clone()))
                .as_str()
                .to_owned(),
            product => variant.product.clone(),
            reader => minijinja::context! {
                groups => variant.groups.iter().cloned().collect::<Vec<_>>(),
                region => variant.region.clone(),
            },
            env => env,
        },
        tracking: true,
    }
}

/// Builds and writes every agent surface, one set per version (CM-92).
#[allow(clippy::too_many_arguments)]
fn write_surfaces(
    output: &Path,
    tree: &tree::Tree,
    pages: &[Outcome],
    settings: &Settings,
    navigations: &BTreeMap<Option<liyasa_core::ids::Version>, liyasa_theme::nav::Navigation>,
    clock_unix: i64,
    report: &mut Report,
    outputs: &mut Outputs,
) -> usize {
    let Some(origin) = crate::agents::site::CanonicalOrigin::parse(&settings.canonical_origin)
    else {
        // Without an origin every absolute URL in a surface would be wrong, so
        // the surfaces are skipped rather than written with a placeholder.
        report.diagnostics.push(
            Diagnostic::new(
                code::W0131,
                "`seo.canonicalOrigin` is not set, so the agent surfaces were not written",
            )
            .help("set `seo.canonicalOrigin` to the site's production origin"),
        );
        return 0;
    };

    let default_version = settings
        .default_version()
        .map(|version| version.name.clone());
    let mut written = 0;
    for version in navigations.keys() {
        let records: Vec<crate::agents::site::PageRecord> = tree
            .pages
            .iter()
            .filter(|page| &page.version == version)
            .filter_map(|page| {
                let outcome = pages.iter().find(|outcome| outcome.route == page.route)?;
                Some(crate::agents::site::PageRecord {
                    id: page.front.id,
                    route: page.route.clone(),
                    title: page
                        .front
                        .title
                        .clone()
                        .unwrap_or_else(|| page.route.as_str().to_owned()),
                    description: page.front.description.clone(),
                    locale: Locale::new(settings.locale.clone()),
                    version: version.clone(),
                    tab: None,
                    group: None,
                    indexable: page.indexing.ai && page.indexing.sitemap && !page.draft,
                    personalized: outcome.dynamic,
                    markdown: outcome.markdown.clone(),
                    updated: page.front.updated.clone(),
                    changelog: crate::changelog::is_entry_file(page.path.as_str())
                        || !outcome.changelog.is_empty(),
                })
            })
            .collect();
        if records.is_empty() {
            continue;
        }

        let navigation = navigations.get(version).cloned().unwrap_or_default();
        let site = crate::agents::site::SiteInput {
            name: settings.name.clone(),
            summary: (!settings.description.is_empty()).then(|| settings.description.clone()),
            origin: origin.clone(),
            locale: Locale::new(settings.locale.clone()),
            version: version.clone(),
            nav: sections(&navigation),
            pages: records,
            agents: crate::agents::site::AgentsSettings::default(),
            feeds: crate::agents::site::FeedsSettings::default(),
        };
        let surfaces = crate::agents::surfaces(
            &site,
            &crate::agents::Options {
                clock: liyasa_core::build::BuildClock(
                    std::time::UNIX_EPOCH
                        + std::time::Duration::from_secs(clock_unix.max(0) as u64),
                ),
                not_found: None,
            },
        );
        report
            .diagnostics
            .extend(surfaces.diagnostics.as_slice().to_vec());
        for resource in &surfaces.resources {
            // CM-92: a version's surfaces live under its own prefix. A page's
            // Markdown route already carries it, because the route does.
            let prefixed = match version {
                // The default version is served unprefixed (CM-91), so only
                // the others move.
                Some(version)
                    if Some(version.as_str()) != default_version.as_deref()
                        && !resource.path.starts_with(&format!("/{version}/")) =>
                {
                    format!("/{version}{}", resource.path)
                }
                _ => resource.path.clone(),
            };
            let path = prefixed.trim_start_matches('/');
            if path.is_empty() {
                continue;
            }
            write_file(output, path, resource.body.as_bytes(), report, outputs);
            written += 1;
        }
    }
    written
}

/// The `llms.txt` sections, which mirror the navigation groups (RX-70).
fn sections(navigation: &liyasa_theme::nav::Navigation) -> Vec<crate::agents::site::NavSection> {
    let mut out = Vec::new();
    for tab in &navigation.tabs {
        for group in &tab.groups {
            out.push(crate::agents::site::NavSection {
                title: match group.title.is_empty() {
                    true => tab.title.clone(),
                    false => group.title.clone(),
                },
                tab: (!tab.title.is_empty()).then(|| tab.title.clone()),
                routes: group
                    .items
                    .iter()
                    .map(|item| Route::new(item.route.clone()))
                    .collect(),
            });
        }
    }
    out
}

/// The Markdown an agent fetches (§11.7), serialized from the same render the
/// HTML came from.
fn agent_markdown(
    rendered: &render::Page,
    page: &tree::Page,
    registry: &Registry,
    site: &SiteMeta,
    all_routes: &BTreeSet<Route>,
    diagnostics: &mut Diagnostics,
) -> String {
    let Some(document) = &rendered.document else {
        return rendered.markdown.clone();
    };
    let options = crate::agents::markdown::Options {
        site,
        registry,
        route: &page.route,
        frontmatter: Some(&page.front),
        routes: all_routes,
        site_instructions: None,
        openapi_schema: None,
    };
    let produced = crate::agents::render_page(document, &options);
    diagnostics.extend(produced.diagnostics.as_slice().to_vec());
    produced.markdown
}

/// Files a page links to that are not pages, which the asset pass copies
/// (CM-84).
fn referenced_files(page: &render::Page) -> Vec<VfsPath> {
    let Some(document) = &page.document else {
        return Vec::new();
    };
    let mut out = Vec::new();
    collect_links(&document.root, &mut out);
    out
}

fn collect_links(block: &liyasa_core::document::Block, out: &mut Vec<VfsPath>) {
    use liyasa_core::document::Node;
    for child in &block.children {
        match child {
            Node::Block(block) => collect_links(block, out),
            Node::Inline(inline) => collect_inline(inline, out),
        }
    }
}

fn collect_inline(inline: &liyasa_core::document::Inline, out: &mut Vec<VfsPath>) {
    use liyasa_core::document::Inline;
    match inline {
        Inline::Link { href, children, .. } => {
            if let Some(path) = local_file(href) {
                out.push(path);
            }
            for child in children {
                collect_inline(child, out);
            }
        }
        Inline::Image { src, .. } => {
            if let Some(path) = local_file(src) {
                out.push(path);
            }
        }
        Inline::Emph(children) | Inline::Strong(children) | Inline::Strike(children) => {
            for child in children {
                collect_inline(child, out);
            }
        }
        _ => {}
    }
}

/// A link is to a file this build copies when it is relative and its extension
/// is not a page's.
fn local_file(href: &str) -> Option<VfsPath> {
    if href.contains("://") || href.starts_with("//") || href.starts_with('#') {
        return None;
    }
    let path = VfsPath::new(href.split(['?', '#']).next().unwrap_or(href));
    let extension = path.extension()?;
    (!liyasa_markdown::source::route::PAGE_EXTENSIONS.contains(&extension)).then_some(path)
}

#[allow(clippy::too_many_arguments)]
fn copy_assets(
    vfs: &dyn Vfs,
    root: &Path,
    output: &Path,
    tree: &tree::Tree,
    referenced: &BTreeSet<VfsPath>,
    settings: &Settings,
    options: &Options,
    cache: &DiskCache,
    report: &mut Report,
    outputs: &mut Outputs,
) -> (Vec<AssetEntry>, Vec<ImageEntry>, u64, String) {
    let from_tree: Vec<(VfsPath, Fingerprint)> = tree
        .assets
        .iter()
        .map(|asset| (asset.path.clone(), asset.fingerprint))
        .collect();
    let linked: Vec<(VfsPath, Fingerprint)> = referenced
        .iter()
        .filter(|path| !tree.assets.iter().any(|asset| &&asset.path == path))
        .filter_map(|path| vfs.fingerprint(path).ok().map(|fp| (path.clone(), fp)))
        .collect();

    let plan = assets::plan(&from_tree, &linked, &settings.asset_options());
    let mut entries = Vec::new();
    let mut image_entries = Vec::new();
    let mut generated = 0u64;

    for asset in plan.entries() {
        let Ok(bytes) = vfs.read(&asset.source) else {
            report.diagnostics.push(
                Diagnostic::new(code::E0703, format!("`{}` could not be read", asset.source))
                    .help("the file was listed by the content tree but is gone"),
            );
            continue;
        };
        write_file(output, &asset.output, &bytes, report, outputs);
        entries.push(AssetEntry {
            source: asset.source.as_str().to_owned(),
            path: asset.output.clone(),
            url: asset.url.clone(),
            hash: asset.fingerprint,
            content_type: asset.content_type.to_owned(),
            disposition: asset.disposition,
        });

        let extension = asset.source.extension().unwrap_or_default();
        if !images::Format::is_processable(extension) {
            continue;
        }
        let width = {
            use crate::images::Encoder as _;
            ImageCodec.dimensions(&bytes).map(|(width, _)| width)
        };
        let plan = images::plan(&asset.source, asset.fingerprint, width, &settings.images);
        if plan.is_empty() {
            continue;
        }
        if settings.images.eager || options.eager_images {
            let tier = images::Tier {
                cache,
                encoder: &ImageCodec,
            };
            let pre = tier.pre_pass(&bytes, &plan);
            generated += pre.generated;
            for derived in &plan.derived {
                if let Ok(variant) = tier.variant(&bytes, derived, plan.source_fingerprint) {
                    write_file(
                        output,
                        &images::variant_path(derived.key, &derived.variant),
                        &variant,
                        report,
                        outputs,
                    );
                }
            }
        }
        image_entries.push(ImageEntry {
            source: asset.source.as_str().to_owned(),
            original_url: plan.original_url.clone(),
            variants: plan
                .derived
                .iter()
                .map(|derived| ImageVariantEntry {
                    key: derived.key,
                    url: derived.url.clone(),
                    width: derived.variant.width,
                    format: derived.variant.format,
                })
                .collect(),
        });
    }
    let _ = root;
    (entries, image_entries, generated, assets::headers(&plan))
}

fn site_meta(settings: &Settings) -> SiteMeta {
    let origin = Url::parse(&settings.canonical_origin)
        .or_else(|_| Url::parse("https://example.invalid"))
        .unwrap_or_else(|error| unreachable!("a literal origin parses: {error}"));
    let llms_txt = origin.join("llms.txt").unwrap_or_else(|_| origin.clone());
    SiteMeta {
        name: settings.name.clone(),
        canonical_origin: origin,
        llms_txt,
        version: None,
        locale: Locale::new(settings.locale.clone()),
    }
}

fn markdown_url(base_path: &str, route: &Route) -> String {
    let base = base_path.trim_end_matches('/');
    let trimmed = route.as_str().trim_matches('/');
    match trimmed.is_empty() {
        true => format!("{base}/index.md"),
        false => format!("{base}/{trimmed}.md"),
    }
}

/// What the last build wrote, so a rebuild rewrites only what changed.
///
/// A dev-server rebuild of a 1,000-page site otherwise rewrites three files per
/// page for one edit, which is most of its budget and every one of them a file
/// event for whoever is watching.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Outputs {
    files: BTreeMap<String, Fingerprint>,
}

impl Outputs {
    const FILE: &'static str = "outputs.json";

    fn load(cache_root: &Path) -> Self {
        std::fs::read(cache_root.join(Self::FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn save(&self, cache_root: &Path) {
        if let Ok(bytes) = serde_json::to_vec(self) {
            let _ = std::fs::create_dir_all(cache_root);
            let _ = std::fs::write(cache_root.join(Self::FILE), bytes);
        }
    }

    /// Whether these bytes still have to reach the file system.
    fn changed(&self, relative: &str, fingerprint: Fingerprint, path: &Path) -> bool {
        self.files.get(relative) != Some(&fingerprint) || !path.exists()
    }

    fn record(&mut self, relative: &str, fingerprint: Fingerprint) {
        self.files.insert(relative.to_owned(), fingerprint);
    }
}

/// `.liyasa/` is the engine's own state, not output: it is written but never
/// reported as part of `dist/`.
fn write_meta(root: &Path, relative: &str, bytes: &[u8], report: &mut Report) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(error) = std::fs::write(&path, bytes) {
        report.diagnostics.push(Diagnostic::new(
            code::E0002,
            format!("could not write {}: {error}", path.display()),
        ));
    }
}

/// Writes one file under `root` and records it by its relative path, so a
/// report from two different output directories still compares equal.
///
/// A file whose bytes the last build already wrote is left alone.
fn write_file(
    root: &Path,
    relative: &str,
    bytes: &[u8],
    report: &mut Report,
    outputs: &mut Outputs,
) {
    let path = root.join(relative);
    let fingerprint = Fingerprint::of(bytes);
    report.written.push(relative.to_owned());
    if !outputs.changed(relative, fingerprint, &path) {
        return;
    }
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        report.diagnostics.push(Diagnostic::new(
            code::E0002,
            format!("could not create {}: {error}", parent.display()),
        ));
        return;
    }
    match std::fs::write(&path, bytes) {
        Ok(()) => {
            report.rewritten += 1;
            outputs.record(relative, fingerprint);
        }
        Err(error) => report.diagnostics.push(Diagnostic::new(
            code::E0002,
            format!("could not write {}: {error}", path.display()),
        )),
    }
}
