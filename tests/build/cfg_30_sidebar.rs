//! CFG-30 in a build: which of §8.4's sixteen node forms reach the sidebar.
//!
//! The config layer accepts every form and checks it (`tests/config/
//! cfg_30_navigation.rs`). The build's resolver handles four — a page string, a
//! group, a directory, and a tab — and silently drops the rest, so a menu, a
//! dropdown, an anchor, a link, and a divider are accepted, validated, and then
//! rendered nowhere. This file pins that, so the day the resolver grows the
//! others the absent half fails and says so.

use std::fs;
use std::path::{Path, PathBuf};

use liyasa_build::engine::{self, Options};
use liyasa_build::git::NoGit;
use liyasa_config::vfs::OsVfs;

/// One of every node form §8.4 lists that needs no second spec document.
const CONFIG: &str = r##"{
  "name": "Acme docs",
  "seo": { "canonicalOrigin": "https://docs.acme.com" },
  "navigation": {
    "pages": [
      "index",
      { "divider": "Product" },
      { "tab": "Guides", "pages": [
        { "group": "Get started", "expanded": true, "pages": ["guides/install"] },
        { "directory": "reference" }
      ]},
      { "menu": "More", "items": ["menu/overview"] },
      { "dropdown": "Products", "items": ["products/cloud"] },
      { "anchor": "Community", "href": "https://acme.dev/community" },
      { "link": "Status", "href": "https://status.acme.dev" }
    ]
  }
}"##;

const PAGES: &[(&str, &str)] = &[
    ("index.md", "---\ntitle: Home\n---\n# Home\n"),
    ("guides/install.md", "---\ntitle: Install\n---\n# Install\n"),
    ("reference/cli.md", "---\ntitle: CLI\n---\n# CLI\n"),
    (
        "menu/overview.md",
        "---\ntitle: Overview\n---\n# Overview\n",
    ),
    ("products/cloud.md", "---\ntitle: Cloud\n---\n# Cloud\n"),
];

struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("liyasa-cfg-30-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("a project directory");
        let project = Self(root);
        project.write("liyasa.json", CONFIG);
        for (path, text) in PAGES {
            project.write(path, text);
        }
        project
    }

    fn write(&self, path: &str, text: &str) -> &Self {
        let full = self.0.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("a directory");
        }
        fs::write(full, text).expect("a file");
        self
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The sidebar of one built page. Each tab carries its own sidebar, so which
/// page you ask decides which subtree you get.
fn sidebar_of(project: &Project, page: &str) -> String {
    let vfs = OsVfs::new(project.path());
    let report = engine::build(
        &vfs,
        &NoGit,
        project.path(),
        &Options {
            build_time: Some(1_789_473_600),
            ..Options::default()
        },
    );
    assert!(!report.failed(false), "{:?}", report.diagnostics);
    let html = fs::read_to_string(project.path().join("dist").join(page))
        .unwrap_or_else(|error| panic!("dist/{page}: {error}"));
    let start = html.find("<aside class=\"ly-sidebar\"").expect("a sidebar");
    let end = html[start..].find("</aside>").expect("the sidebar ends") + start;
    html[start..end].to_owned()
}

fn sidebar(project: &Project) -> String {
    sidebar_of(project, "index.html")
}

#[test]
fn a_page_and_its_group_render() {
    let project = Project::new("rendered");
    let home = sidebar(&project);
    assert!(
        home.contains("ly-sidebar-group"),
        "a group is rendered: {home}"
    );
    assert!(
        home.contains("href=\"/\""),
        "the page string is a link: {home}"
    );
    assert!(
        home.contains("<span>Home</span>"),
        "with the page's title: {home}"
    );
}

/// A tab is listed and its subtree is not rendered, on any page — including the
/// pages inside it. `nav::resolve` builds the tab and its groups; what reaches
/// the sidebar is the first tab's groups on every page, and each tab's link is
/// `href="#"`. So a tabbed navigation renders no route to `guides/install` at
/// all, though the page is built and routable. `crates/liyasa-build/` and
/// `crates/liyasa-theme/` own the two halves of this.
#[test]
fn a_tabs_subtree_reaches_no_sidebar_yet() {
    let project = Project::new("tabs");
    for page in ["index.html", "guides/install/index.html"] {
        let sidebar = sidebar_of(&project, page);
        assert!(sidebar.contains("Guides"), "the tab is listed on {page}");
        for absent in ["Get started", "/guides/install", "/reference/cli"] {
            assert!(
                !sidebar.contains(absent),
                "a tab's subtree now renders on {page}: assert that it does rather than \
                 that it does not, and move CFG-30 on. Missing assertion for `{absent}`"
            );
        }
    }
}

/// The other five. Each is accepted by the schema, checked by `liyasa validate`,
/// and dropped by `nav::resolve`, whose `match` has arms for a string, a group,
/// a directory, and a tab and no others.
#[test]
fn the_forms_the_resolver_drops_are_still_dropped() {
    let project = Project::new("dropped");
    let sidebar = sidebar(&project);
    let dropped = [
        ("divider", "Product"),
        ("menu", "More"),
        ("dropdown", "Products"),
        ("anchor", "Community"),
        ("link", "Status"),
    ];
    let rendered: Vec<&str> = dropped
        .into_iter()
        .filter(|(_, label)| sidebar.contains(label))
        .map(|(form, _)| form)
        .collect();
    assert_eq!(
        rendered,
        Vec::<&str>::new(),
        "`nav::resolve` grew a node form: assert it renders rather than that it does not, \
         and move CFG-30 on"
    );
    // And the pages only those forms name are in the build, just unreachable
    // from the sidebar.
    assert!(
        project
            .path()
            .join("dist/menu/overview/index.html")
            .exists(),
        "the page a menu names is still routable"
    );
}
