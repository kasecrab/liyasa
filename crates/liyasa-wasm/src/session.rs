//! One editor session over one draft.
//!
//! Everything expensive is built once: the component registry, the site
//! metadata, the session nonce, and ED-07's seeded file system. A keystroke
//! pays for scanning, expansion, parsing and rendering the page in front of
//! the author, and for nothing else — which is what NFR-05's
//! keystroke-to-preview target is measured against.

use std::sync::Arc;

use liyasa_components::registry::Registry;
use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};
use liyasa_core::document::{Segment, SourceDocument, TemplateKind};
use liyasa_core::ids::{Locale, Version};
use liyasa_core::markdown::{Audience, ParseOptions, SiteMeta as CoreSiteMeta, TemplateContext};
use liyasa_core::net::Url;
use liyasa_core::source_map::SourceMap;
use liyasa_core::vfs::{Bytes, Vfs, VfsPath};
use liyasa_markdown::source::expand::{self, Budget, ExpandOptions, Undefined};

use crate::api::{
    OpenRequest, Options, ParseRequest, ParseResponse, PreviewRequest, PreviewResponse,
    SerializeRequest, SerializeResponse, SessionStatus, SiteMeta, ValidateMode, ValidateRequest,
    ValidateResponse,
};
use crate::blocks::Blocks;
use crate::vfs::{EditorVfs, Fetch, PRELOAD_LIMIT, Sealed};

/// How many files one draft may pull in through `{% include %}` before the
/// session stops following the chain. Expansion has its own depth cap; this one
/// bounds the fetching that happens before expansion can start.
const INCLUDE_LIMIT: usize = 64;

pub struct Session {
    registry: Registry,
    site: CoreSiteMeta,
    nonce: [u8; 16],
    vfs: EditorVfs,
}

/// The nonce is deliberately not shown: it is the secret that makes a directive
/// marker unforgeable, and a panic message is a place secrets end up.
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("site", &self.site.name)
            .field("components", &self.registry.len())
            .field("preload_bytes", &self.vfs.preload_bytes())
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Opens a session, or says everything about the request that was not
    /// usable — not just the first thing, because an editor that has to fix
    /// one field per round trip is an editor nobody uses.
    pub fn open(request: &OpenRequest, fetch: Box<dyn Fetch>) -> Result<Self, Diagnostics> {
        let mut refused = Diagnostics::new();
        let nonce = nonce(&request.nonce, &mut refused);
        let site = site(&request.site, &mut refused);
        let (Some(nonce), Some(site)) = (nonce, site) else {
            return Err(refused);
        };
        let seed = request.seed.iter().map(|entry| {
            (
                VfsPath::new(&entry.path),
                Bytes::from(entry.text.clone().into_bytes()),
            )
        });
        Ok(Self {
            registry: Registry::builtins(),
            site,
            nonce,
            vfs: EditorVfs::new(seed, fetch),
        })
    }

    /// A session for a draft that is one file and has no server behind it.
    pub fn sealed(request: &OpenRequest) -> Result<Self, Diagnostics> {
        Self::open(request, Box::new(Sealed))
    }

    pub fn status(&self) -> SessionStatus {
        SessionStatus {
            preload_bytes: self.vfs.preload_bytes(),
            preload_limit: PRELOAD_LIMIT,
            fetched: self.vfs.fetched(),
            server_render: self.vfs.over_budget(),
        }
    }

    pub fn vfs(&self) -> &EditorVfs {
        &self.vfs
    }

    /// Hands the session a path it said was missing (ED-07). The host fetched
    /// it through `/_liyasa/editor/fs/<path>`; nothing here did any I/O.
    pub fn seed(&self, path: &str, text: &str) {
        self.vfs
            .insert(VfsPath::new(path), Bytes::from(text.as_bytes().to_vec()));
    }

    /// Both representations of §7.16.
    pub fn parse(&self, request: &ParseRequest) -> ParseResponse {
        let prepared = self.prepare(&request.path, &request.source);
        let mut diagnostics = prepared.diagnostics;
        let expanded = match self.expand(&prepared.map, &prepared.document, request.context.clone())
        {
            Ok(expanded) => expanded,
            Err(failed) => {
                diagnostics.extend(failed);
                return ParseResponse {
                    source: prepared.document,
                    document: None,
                    record: Default::default(),
                    missing: prepared.missing,
                    diagnostics: diagnostics.into_vec(),
                };
            }
        };
        let record = expanded.record.clone();
        let document = liyasa_markdown::parse(&expanded, &self.registry, &self.parse_options(&request.options));
        diagnostics.extend(document.diagnostics.as_slice().to_vec());
        ParseResponse {
            source: prepared.document,
            document: Some(document),
            record,
            missing: prepared.missing,
            diagnostics: diagnostics.into_vec(),
        }
    }

    /// The keystroke path: HTML, plus the Markdown and plain text serialized
    /// from the same parse, so the three can never disagree.
    pub fn preview(&self, request: &PreviewRequest) -> PreviewResponse {
        if self.vfs.over_budget() {
            return PreviewResponse {
                server_render: true,
                diagnostics: vec![large_page(self.vfs.preload_bytes())],
                ..PreviewResponse::default()
            };
        }
        let prepared = self.prepare(&request.path, &request.source);
        let mut diagnostics = prepared.diagnostics;
        let expanded = match self.expand(&prepared.map, &prepared.document, request.context.clone())
        {
            Ok(expanded) => expanded,
            Err(failed) => {
                diagnostics.extend(failed);
                return PreviewResponse {
                    missing: prepared.missing,
                    diagnostics: diagnostics.into_vec(),
                    ..PreviewResponse::default()
                };
            }
        };
        let record = expanded.record.clone();
        let document = liyasa_markdown::parse(&expanded, &self.registry, &self.parse_options(&request.options));
        let mut blocks = Blocks::new(&self.registry, &self.site);
        let html = liyasa_markdown::render::html::render(&document.root, &mut blocks);
        diagnostics.extend(document.diagnostics.as_slice().to_vec());
        diagnostics.extend(blocks.take_diagnostics().into_vec());
        PreviewResponse {
            html,
            markdown: liyasa_markdown::render_markdown(&document, Audience::Human, &self.site),
            text: liyasa_markdown::render::render_text(&document),
            record,
            missing: prepared.missing,
            server_render: false,
            diagnostics: diagnostics.into_vec(),
        }
    }

    /// Writes segment edits back, byte-preserving everywhere else.
    pub fn serialize(&self, request: &SerializeRequest) -> SerializeResponse {
        let mut map = SourceMap::new();
        let id = map.intern(VfsPath::new(&request.path), Arc::from(request.source.as_str()));
        let (document, mut diagnostics) = liyasa_markdown::scan(&request.source, id);
        let text =
            liyasa_markdown::serialize_source(&request.source, &document, &request.edits);
        if !request.format {
            return SerializeResponse {
                text,
                diagnostics: diagnostics.into_vec(),
            };
        }
        match liyasa_markdown::format(&text) {
            Ok(formatted) => SerializeResponse {
                text: formatted,
                diagnostics: diagnostics.into_vec(),
            },
            Err(failed) => {
                diagnostics.extend(failed.into_vec());
                SerializeResponse {
                    text,
                    diagnostics: diagnostics.into_vec(),
                }
            }
        }
    }

    /// `liyasa.json` and a page's front matter, each through the loader the
    /// build uses rather than a second implementation of the same rules.
    pub fn validate(&self, request: &ValidateRequest) -> ValidateResponse {
        let mut diagnostics = Diagnostics::new();
        let mut parsed = None;

        if let Some(text) = &request.config {
            let vfs = EditorVfs::sealed([(
                VfsPath::new(liyasa_config::load::CONFIG_FILE),
                Bytes::from(text.clone().into_bytes()),
            )]);
            let mut sources = SourceMap::new();
            let load = liyasa_config::load(&vfs, &mut sources, &liyasa_config::load::Options::default());
            diagnostics.extend(load.diagnostics.as_slice().to_vec());
            let pages: liyasa_config::Pages = request.routes.iter().collect();
            let context = liyasa_config::validate::Context {
                pages: &pages,
                mode: match request.mode {
                    ValidateMode::Dev => liyasa_config::validate::Mode::Dev,
                    ValidateMode::Build => liyasa_config::validate::Mode::Build,
                },
            };
            let checked = liyasa_config::validate_load(&load, &context);
            // With no route set the editor cannot tell a missing page from a
            // page it simply has not listed, so the navigation findings are
            // dropped rather than reported against nothing.
            let navigation_known = !request.routes.is_empty();
            diagnostics.extend(checked.into_vec().into_iter().filter(|diagnostic| {
                navigation_known || !matches!(diagnostic.code.as_str(), "E0104" | "W0130")
            }));
            if !matches!(load.value, serde_json::Value::Null) {
                parsed = Some(load.value);
            }
        }

        if let Some(front) = &request.frontmatter {
            let page = format!("---\n{}\n---\n", front.trim_end_matches('\n'));
            let mut map = SourceMap::new();
            let id = map.intern(VfsPath::new("frontmatter.md"), Arc::from(page.as_str()));
            let (_, found) = liyasa_markdown::scan(&page, id);
            diagnostics.extend(found.into_vec());
        }

        ValidateResponse {
            config: parsed,
            diagnostics: diagnostics.into_vec(),
        }
    }

    fn parse_options(&self, options: &Options) -> ParseOptions {
        ParseOptions {
            html: options.html,
            math: options.math,
            wikilinks: options.wikilinks,
            build_nonce: self.nonce,
        }
    }

    fn expand(
        &self,
        map: &SourceMap,
        document: &SourceDocument,
        context: serde_json::Value,
    ) -> Result<liyasa_core::markdown::Expanded, Vec<Diagnostic>> {
        let options = ExpandOptions {
            budget: Budget::REQUEST,
            undefined: Undefined::Strict,
        };
        let environment = expand::environment(&options);
        let context = TemplateContext {
            values: minijinja::Value::from_serialize(&context),
            tracking: true,
        };
        expand::expand_with(map, document, &context, &environment, &options)
            .map_err(Diagnostics::into_vec)
    }

    /// Interns the draft and every file it includes, fetching through ED-07's
    /// resolver whatever the seed did not carry.
    fn prepare(&self, path: &str, source: &str) -> Prepared {
        let mut map = SourceMap::new();
        let id = map.intern(VfsPath::new(path), Arc::from(source));
        let (document, mut diagnostics) = liyasa_markdown::scan(source, id);
        let mut queue = include_names(&document, source);
        let mut missing = Vec::new();
        let mut seen = 0usize;
        while let Some(name) = queue.pop() {
            if seen >= INCLUDE_LIMIT {
                break;
            }
            let path = VfsPath::new(&name);
            if map.find(&path).is_some() {
                continue;
            }
            // A path the session does not hold is named in `missing` so the
            // host can fetch it and call again; the diagnostic for the include
            // that named it is expansion's, with the span.
            let Ok(bytes) = self.vfs.read(&path) else {
                missing.push(path.as_str().to_owned());
                continue;
            };
            let Ok(text) = std::str::from_utf8(&bytes) else {
                continue;
            };
            let text: Arc<str> = Arc::from(text);
            seen += 1;
            let id = map.intern(path, Arc::clone(&text));
            let (included, found) = liyasa_markdown::scan(&text, id);
            diagnostics.extend(found.into_vec());
            queue.extend(include_names(&included, &text));
        }
        Prepared {
            map,
            document,
            missing,
            diagnostics,
        }
    }
}

struct Prepared {
    map: SourceMap,
    document: SourceDocument,
    /// ED-07: what the draft named and the session does not hold.
    missing: Vec<String>,
    diagnostics: Diagnostics,
}

/// `{% include %}`, `{% import %}`, `{% from %}` and `{% snippet %}` names, in
/// the forms `liyasa_markdown::source::expand` resolves. It is not reachable
/// from outside that crate, so this is a deliberate second reading of the same
/// statements: `tests/it/session.rs` proves the two agree by fetching a file
/// only this scanner can have asked for and finding its text in the preview.
fn include_names(document: &SourceDocument, text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for segment in &document.segments {
        let Segment::Template { span, kind } = segment else {
            continue;
        };
        if matches!(kind, TemplateKind::Comment) {
            continue;
        }
        let tag = &text[span.start as usize..span.end as usize];
        names.extend(includes_in(tag));
    }
    names
}

fn includes_in(tag: &str) -> Option<String> {
    let inner = tag
        .trim_start_matches("{%")
        .trim_end_matches("%}")
        .trim_matches('-')
        .trim();
    let (statement, rest) = inner.split_once(char::is_whitespace)?;
    let name = rest.split_whitespace().next()?.trim_matches(['"', '\'']);
    if name.is_empty() {
        return None;
    }
    match statement {
        "include" | "import" | "from" => Some(name.to_owned()),
        "snippet" => Some(format!("snippets/{name}.md")),
        _ => None,
    }
}

fn nonce(text: &str, refused: &mut Diagnostics) -> Option<[u8; 16]> {
    if text.len() != 32 {
        refused.push(
            Diagnostic::new(
                code::E1200,
                format!("session nonce is {} characters, not 32", text.len()),
            )
            .help("generate 16 random bytes per session and send them as lower-case hexadecimal"),
        );
        return None;
    }
    let mut out = [0u8; 16];
    let (pairs, _) = text.as_bytes().as_chunks::<2>();
    for (byte, pair) in out.iter_mut().zip(pairs) {
        let Ok(pair) = std::str::from_utf8(pair) else {
            refused.push(Diagnostic::new(
                code::E1200,
                "session nonce is not text".to_owned(),
            ));
            return None;
        };
        let Ok(parsed) = u8::from_str_radix(pair, 16) else {
            refused.push(Diagnostic::new(
                code::E1200,
                format!("session nonce contains `{pair}`, which is not hexadecimal"),
            ));
            return None;
        };
        *byte = parsed;
    }
    Some(out)
}

fn site(meta: &SiteMeta, refused: &mut Diagnostics) -> Option<CoreSiteMeta> {
    let mut url = |field: &str, value: &str| match Url::parse(value) {
        Ok(url) => Some(url),
        Err(error) => {
            refused.push(
                Diagnostic::new(
                    code::E1200,
                    format!("site metadata `{field}` is not an absolute URL: {error}"),
                )
                .help("send the site's own origin, the one `liyasa.json` declares"),
            );
            None
        }
    };
    let canonical_origin = url("canonical_origin", &meta.canonical_origin);
    let llms_txt = url("llms_txt", &meta.llms_txt);
    Some(CoreSiteMeta {
        name: meta.name.clone(),
        canonical_origin: canonical_origin?,
        llms_txt: llms_txt?,
        version: meta.version.as_deref().map(Version::new),
        locale: Locale::new(&meta.locale),
    })
}

fn large_page(bytes: u32) -> Diagnostic {
    Diagnostic::new(
        code::W1201,
        format!(
            "the files this page needs come to {bytes} bytes, over the {PRELOAD_LIMIT}-byte \
             browser budget"
        ),
    )
    .help("the preview endpoint renders this page instead; nothing is lost but the local render")
}
