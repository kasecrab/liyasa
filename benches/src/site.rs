//! The site the figures are measured on.
//!
//! Generated rather than checked in, for the reason the conformance corpus is:
//! ten thousand pages is a quarter of a gigabyte of fixture nobody reads. It is
//! deterministic, so two runs measure the same work, and it is shaped like
//! documentation rather than like lorem ipsum — front matter, headings, a code
//! block, a table, and links to its neighbours — because a page with no links
//! skips the resolver and a page with no code block skips the highlighter, and
//! a benchmark that skips half the engine is not measuring the build.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

/// A generated project that deletes itself.
pub struct Site {
    root: PathBuf,
    pages: usize,
}

/// Pages per navigation group. Ten groups of a hundred at 1,000 pages is the
/// shape of a real manual; one group of ten thousand is not.
const GROUP: usize = 100;

impl Site {
    /// Writes `pages` pages under `root`, which must not exist.
    pub fn generate(root: PathBuf, pages: usize) -> Result<Self, String> {
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
        let site = Self { root, pages };
        site.write("liyasa.json", &site.config(NAVIGATION_ORDER_A))?;
        site.write(
            "index.md",
            "---\ntitle: Home\ndescription: The generated benchmark site.\n---\n\n# Home\n\nEvery page below is generated.\n",
        )?;
        for page in 0..pages {
            site.write(&page_path(page), &body(page, pages))?;
        }
        Ok(site)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn pages(&self) -> usize {
        self.pages
    }

    /// Rewrites one page's body, the way an author's keystroke does.
    ///
    /// `nth` picks the page so a loop of samples does not measure the same
    /// cache row every time; the text changes on every call so the edit is
    /// never a no-op the fingerprint short-circuits.
    pub fn edit(&self, nth: usize) -> Result<(), String> {
        let page = nth % self.pages.max(1);
        let mut text = body(page, self.pages);
        let _ = write!(text, "\nEdited {nth}.\n");
        self.write(&page_path(page), &text)
    }

    /// Reorders the navigation groups, which is the "config change affecting
    /// navigation" row of §6.6: every page's sidebar, breadcrumbs and
    /// previous/next change, and no page's own source does.
    pub fn reorder_navigation(&self) -> Result<(), String> {
        self.write("liyasa.json", &self.config(NAVIGATION_ORDER_B))
    }

    /// The navigation tree, with every page named.
    ///
    /// Named rather than globbed. `schemas/liyasa.schema.json` says a group's
    /// `pages` accepts "a glob such as `getting-started/*`", and
    /// `crates/liyasa-build/src/nav.rs` has no glob branch at all: `item_of`
    /// matches an exact route or an exact source path and raises `E0104`
    /// otherwise, so a globbed benchmark site does not build. Reported; not
    /// this package's to fix. Naming every page is what a real manual's
    /// navigation file does anyway, and it gives the resolver the work the
    /// §6.6 scenario is about.
    fn config(&self, order: Order) -> String {
        let groups = self.pages.div_ceil(GROUP).max(1);
        let mut indices: Vec<usize> = (0..groups).collect();
        // The label changes as well as the order. A site small enough to have
        // one group would otherwise reorder into itself, and the scenario
        // would measure a rebuild of an unchanged config.
        if order == NAVIGATION_ORDER_B {
            indices.reverse();
        }
        let label = if order == NAVIGATION_ORDER_B {
            "Section"
        } else {
            "Part"
        };

        let mut nodes = String::from("\"index.md\"");
        for group in indices {
            let first = group * GROUP;
            let last = ((group + 1) * GROUP).min(self.pages);
            let mut entries = String::new();
            for page in first..last {
                if !entries.is_empty() {
                    entries.push(',');
                }
                let _ = write!(entries, "\"{}\"", page_path(page));
            }
            let _ = write!(
                nodes,
                ",{{\"group\":\"{label} {group}\",\"pages\":[{entries}]}}"
            );
        }
        format!(
            "{{\"name\":\"Benchmark docs\",\
               \"seo\":{{\"canonicalOrigin\":\"https://bench.example\"}},\
               \"navigation\":{{\"pages\":[{nodes}]}}}}"
        )
    }

    fn write(&self, path: &str, text: &str) -> Result<(), String> {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&full, text).map_err(|e| format!("{}: {e}", full.display()))
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Which way round the navigation groups are. Two orders is all the scenario
/// needs, and naming them keeps the call sites readable.
type Order = u8;
const NAVIGATION_ORDER_A: Order = 0;
const NAVIGATION_ORDER_B: Order = 1;

fn page_path(page: usize) -> String {
    format!("guides/part-{:02}/page-{page:05}.md", page / GROUP)
}

/// Roughly 1.4 KB of documentation-shaped Markdown.
fn body(page: usize, total: usize) -> String {
    let previous = (page + total - 1) % total.max(1);
    let next = (page + 1) % total.max(1);
    format!(
        "---\n\
         title: Page {page}\n\
         description: The {page}th generated page of the benchmark site.\n\
         ---\n\
         \n\
         # Page {page}\n\
         \n\
         This page exists to be built. It links to [the one before]({}) and \
         [the one after]({}), so the link resolver has work to do.\n\
         \n\
         ## Configuration\n\
         \n\
         ```json\n\
         {{\"page\": {page}, \"group\": {}, \"of\": {total}}}\n\
         ```\n\
         \n\
         ## Fields\n\
         \n\
         | Field | Type | Default |\n\
         |---|---|---|\n\
         | `id` | integer | `{page}` |\n\
         | `name` | string | `page-{page:05}` |\n\
         | `enabled` | boolean | `true` |\n\
         \n\
         ## Notes\n\
         \n\
         The body is long enough that rendering it is real work and short \
         enough that ten thousand of them fit on a disk. Headings give the \
         table of contents something to build, the table exercises the GFM \
         extension, and the fenced block reaches the highlighter.\n",
        relative(page, previous),
        relative(page, next),
        page / GROUP,
    )
}

/// A link from one generated page to another, as an author would write it.
fn relative(from: usize, to: usize) -> String {
    if from / GROUP == to / GROUP {
        format!("./page-{to:05}.md")
    } else {
        format!("../part-{:02}/page-{to:05}.md", to / GROUP)
    }
}
