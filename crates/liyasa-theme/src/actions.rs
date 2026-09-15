//! Contextual page actions (RX-100, CFG-72).
//!
//! Every action is a link or a copy; none of them is a request the page makes
//! on its own, so THM-32 holds: an assistant is opened only when a reader
//! clicks the item that says so.

use serde::{Deserialize, Serialize};

use crate::strings::Strings;

/// Where the menu appears (CFG-72).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Placement {
    #[default]
    Header,
    Sidebar,
    Both,
}

impl Placement {
    pub fn in_header(self) -> bool {
        matches!(self, Self::Header | Self::Both)
    }

    pub fn in_sidebar(self) -> bool {
        matches!(self, Self::Sidebar | Self::Both)
    }
}

/// One configured entry: a documented name, or an operator's own link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Item {
    Named(String),
    Custom {
        label: String,
        href: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub placement: Placement,
    pub items: Vec<Item>,
    pub exclude: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            placement: Placement::Header,
            items: DEFAULT_ITEMS
                .iter()
                .map(|id| Item::Named((*id).to_owned()))
                .collect(),
            exclude: Vec::new(),
        }
    }
}

/// The menu a site gets without configuring one.
pub const DEFAULT_ITEMS: &[&str] = &[
    "copy-markdown",
    "view-markdown",
    "open-in-chatgpt",
    "open-in-claude",
    "edit-on-github",
];

/// Every name CFG-72 and RX-100 document.
pub const KNOWN_ITEMS: &[&str] = &[
    "copy-markdown",
    "view-markdown",
    "open-in-chatgpt",
    "open-in-claude",
    "open-in-perplexity",
    "open-in-grok",
    "open-in-google-ai",
    "open-in-devin",
    "copy-mcp-url",
    "download-pdf",
    "edit-on-github",
    "suggest-edit",
];

/// The provider endpoints the "open in" items use.
///
/// RX-100 names the providers and says each "opens the provider with a prompt
/// containing the page's Markdown URL"; it does not give the endpoints, and an
/// endpoint is a third party's to change. They are listed here as one table so
/// a change is one line, and an operator who disagrees overrides the entry with
/// a custom item.
// TODO(rfc-0502): confirm each endpoint against the provider's own
// documentation before 1.0.
pub const PROVIDERS: &[(&str, &str, &str)] = &[
    ("open-in-chatgpt", "ChatGPT", "https://chatgpt.com/?q="),
    ("open-in-claude", "Claude", "https://claude.ai/new?q="),
    (
        "open-in-perplexity",
        "Perplexity",
        "https://www.perplexity.ai/search?q=",
    ),
    ("open-in-grok", "Grok", "https://grok.com/?q="),
    (
        "open-in-google-ai",
        "Google AI Studio",
        "https://aistudio.google.com/prompts/new_chat?prompt=",
    ),
    ("open-in-devin", "Devin", "https://app.devin.ai/?prompt="),
];

/// What a page can hand an action.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Targets {
    /// Absolute URL of the page's Markdown (RX-100, W0406).
    pub markdown_url: String,
    /// The Markdown itself, when the build inlined it for the copy action.
    pub markdown: Option<String>,
    pub mcp_url: Option<String>,
    pub pdf_url: Option<String>,
    pub edit_url: Option<String>,
    pub suggest_url: Option<String>,
}

/// A resolved menu entry, ready for the template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    pub label: String,
    pub icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
    /// The id of the element whose text the action copies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copy_target: Option<String>,
    /// A same-origin path the action fetches and copies (RFC 0505). RX-14
    /// forbids inlining a page's Markdown into its HTML, so copying it means
    /// fetching it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copy_url: Option<String>,
    pub external: bool,
}

/// Resolves the configured menu for one page.
///
/// An item whose target the page does not have is dropped rather than rendered
/// dead: a site without a PDF pipeline should not show "Download as PDF".
pub fn resolve(config: &Config, targets: &Targets, strings: &Strings) -> Vec<Action> {
    let mut out = Vec::new();
    for item in &config.items {
        let action = match item {
            Item::Custom { label, href, icon } => Some(Action {
                id: "custom".to_owned(),
                label: label.clone(),
                icon: icon.clone().unwrap_or_else(|| "link".to_owned()),
                href: Some(href.clone()),
                copy_target: None,
                copy_url: None,
                external: is_external(href),
            }),
            Item::Named(name) => named(name, targets, strings),
        };
        let Some(action) = action else { continue };
        if config.exclude.contains(&action.id) {
            continue;
        }
        if out
            .iter()
            .any(|existing: &Action| existing.id == action.id && existing.id != "custom")
        {
            continue;
        }
        out.push(action);
    }
    out
}

fn named(name: &str, targets: &Targets, strings: &Strings) -> Option<Action> {
    let action = |label: &str, icon: &str, href: Option<String>, copy: Option<String>| Action {
        id: name.to_owned(),
        label: label.to_owned(),
        icon: icon.to_owned(),
        external: href.as_deref().is_some_and(is_external),
        href,
        copy_target: copy,
        copy_url: None,
    };
    match name {
        "copy-markdown" => (!targets.markdown_url.is_empty()).then(|| Action {
            copy_url: Some(same_origin(&targets.markdown_url)),
            ..action(&strings.copy_page, "clipboard", None, None)
        }),
        "view-markdown" => Some(action(
            &strings.view_markdown,
            "file-text",
            Some(same_origin(&targets.markdown_url)),
            None,
        )),
        "copy-mcp-url" => targets.mcp_url.as_ref().map(|_| {
            action(
                &strings.copy_mcp_url,
                "server",
                None,
                Some("ly-mcp-url".to_owned()),
            )
        }),
        "download-pdf" => targets
            .pdf_url
            .clone()
            .map(|href| action(&strings.download_pdf, "download", Some(href), None)),
        "edit-on-github" => targets
            .edit_url
            .clone()
            .map(|href| action(&strings.edit_page, "pencil", Some(href), None)),
        "suggest-edit" => targets
            .suggest_url
            .clone()
            .map(|href| action(&strings.suggest_edit, "message-square", Some(href), None)),
        _ => {
            let (_, provider, endpoint) = PROVIDERS.iter().find(|(id, _, _)| *id == name)?;
            if targets.markdown_url.is_empty() {
                return None;
            }
            Some(action(
                &format!("{} {provider}", strings.open_in),
                "sparkles",
                Some(provider_url(endpoint, &targets.markdown_url)),
                None,
            ))
        }
    }
}

/// The twin as the reader's own browser should resolve it (RFC 0505).
///
/// `Targets::markdown_url` is absolute so a provider can fetch it, but the two
/// actions the reader dereferences must land on the host that served the page:
/// a preview, a staging host or a mirror serves its own Markdown, and fetching
/// the published origin from one of them copies a stranger's page. A URL that
/// is already a path is left as it is.
pub fn same_origin(markdown_url: &str) -> String {
    let Ok(parsed) = url::Url::parse(markdown_url) else {
        return markdown_url.to_owned();
    };
    let mut out = parsed.path().to_owned();
    if let Some(query) = parsed.query() {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = parsed.fragment() {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

/// The provider link: the endpoint plus a prompt carrying the Markdown URL.
pub fn provider_url(endpoint: &str, markdown_url: &str) -> String {
    let prompt =
        format!("Read {markdown_url} so I can ask questions about this documentation page.");
    let encoded: String = url::form_urlencoded::byte_serialize(prompt.as_bytes()).collect();
    format!("{endpoint}{encoded}")
}

fn is_external(href: &str) -> bool {
    href.starts_with("http://") || href.starts_with("https://") || href.starts_with("//")
}

/// Names in `items` or `exclude` that no action answers to (CFG-72).
pub fn unknown_items(config: &Config) -> Vec<String> {
    let known = |name: &String| KNOWN_ITEMS.contains(&name.as_str());
    config
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Named(name) if !known(name) => Some(name.clone()),
            _ => None,
        })
        .chain(config.exclude.iter().filter(|name| !known(name)).cloned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets() -> Targets {
        Targets {
            markdown_url: "https://docs.example/guide/install.md".to_owned(),
            markdown: Some("# Install".to_owned()),
            mcp_url: Some("https://docs.example/mcp".to_owned()),
            pdf_url: None,
            edit_url: Some("https://github.com/acme/docs/edit/main/guide/install.md".to_owned()),
            suggest_url: None,
        }
    }

    #[test]
    fn the_default_menu_is_the_documented_one() {
        let actions = resolve(&Config::default(), &targets(), &Strings::default());
        let ids: Vec<&str> = actions.iter().map(|action| action.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "copy-markdown",
                "view-markdown",
                "open-in-chatgpt",
                "open-in-claude",
                "edit-on-github"
            ]
        );
    }

    #[test]
    fn copying_the_page_fetches_it_rather_than_inlining_it() {
        let actions = resolve(&Config::default(), &targets(), &Strings::default());
        let copy = actions
            .iter()
            .find(|action| action.id == "copy-markdown")
            .expect("copy is in the default menu");
        assert_eq!(copy.copy_url.as_deref(), Some("/guide/install.md"));
        assert!(
            copy.copy_target.is_none(),
            "RX-14 forbids inlining the source"
        );
        assert!(copy.href.is_none(), "copying is not navigation");
    }

    #[test]
    fn the_reader_reaches_the_twin_on_the_host_that_served_the_page() {
        // RFC 0505: a preview, a staging host and a mirror all serve their own
        // Markdown; fetching the published origin from one of them copies a
        // stranger's page into the reader's clipboard.
        let actions = resolve(&Config::default(), &targets(), &Strings::default());
        for id in ["copy-markdown", "view-markdown"] {
            let action = actions
                .iter()
                .find(|action| action.id == id)
                .unwrap_or_else(|| panic!("`{id}` is in the default menu"));
            let url = action
                .copy_url
                .as_deref()
                .or(action.href.as_deref())
                .unwrap_or_else(|| panic!("`{id}` names the twin"));
            assert_eq!(url, "/guide/install.md");
            assert!(!action.external, "`{id}` stays on this site");
        }
    }

    #[test]
    fn a_provider_is_handed_a_url_it_can_reach() {
        // The opposite of the rule above: the party dereferencing the URL is
        // not the reader's browser, so a path would be unreachable.
        let actions = resolve(&Config::default(), &targets(), &Strings::default());
        for (id, _, _) in PROVIDERS {
            let Some(action) = actions.iter().find(|action| &action.id == id) else {
                continue;
            };
            let href = action.href.as_deref().unwrap_or_default();
            assert!(
                href.contains("https%3A%2F%2Fdocs.example%2Fguide%2Finstall.md"),
                "`{id}` hands over `{href}`"
            );
        }
    }

    #[test]
    fn a_twin_that_is_already_a_path_is_left_alone() {
        let targets = Targets {
            markdown_url: "/guide/install.md".to_owned(),
            ..targets()
        };
        let actions = resolve(&Config::default(), &targets, &Strings::default());
        let copy = actions
            .iter()
            .find(|action| action.id == "copy-markdown")
            .expect("copy is in the default menu");
        assert_eq!(copy.copy_url.as_deref(), Some("/guide/install.md"));
    }

    #[test]
    fn a_query_and_a_fragment_survive_the_trip_to_a_path() {
        assert_eq!(
            same_origin("https://docs.example/a/b.md?v=2#top"),
            "/a/b.md?v=2#top"
        );
        assert_eq!(same_origin("not a url"), "not a url");
    }

    #[test]
    fn a_provider_link_carries_the_markdown_url_in_its_prompt() {
        let actions = resolve(&Config::default(), &targets(), &Strings::default());
        let claude = actions
            .iter()
            .find(|action| action.id == "open-in-claude")
            .expect("claude is in the default menu");
        let href = claude.href.as_deref().unwrap_or_default();
        assert!(href.starts_with("https://claude.ai/new?q="));
        assert!(href.contains("guide%2Finstall.md"), "{href}");
        assert!(claude.external);
        assert_eq!(claude.label, "Open in Claude");
    }

    #[test]
    fn an_action_without_a_target_is_dropped_not_rendered_dead() {
        let config = Config {
            items: KNOWN_ITEMS
                .iter()
                .map(|id| Item::Named((*id).to_owned()))
                .collect(),
            ..Config::default()
        };
        let actions = resolve(&config, &targets(), &Strings::default());
        let ids: Vec<&str> = actions.iter().map(|action| action.id.as_str()).collect();
        assert!(!ids.contains(&"download-pdf"), "no pdf url was given");
        assert!(!ids.contains(&"suggest-edit"));
        assert!(ids.contains(&"copy-mcp-url"));
    }

    #[test]
    fn exclude_removes_an_item_from_the_menu() {
        let config = Config {
            exclude: vec!["open-in-chatgpt".to_owned()],
            ..Config::default()
        };
        let actions = resolve(&config, &targets(), &Strings::default());
        assert!(actions.iter().all(|action| action.id != "open-in-chatgpt"));
        assert_eq!(actions.len(), DEFAULT_ITEMS.len() - 1);
    }

    #[test]
    fn a_custom_entry_keeps_its_label_and_is_not_deduplicated() {
        let config = Config {
            items: vec![
                Item::Custom {
                    label: "Status".to_owned(),
                    href: "https://status.example".to_owned(),
                    icon: Some("activity".to_owned()),
                },
                Item::Custom {
                    label: "Support".to_owned(),
                    href: "/support".to_owned(),
                    icon: None,
                },
            ],
            ..Config::default()
        };
        let actions = resolve(&config, &targets(), &Strings::default());
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].label, "Status");
        assert!(actions[0].external);
        assert!(!actions[1].external, "a site-relative link is not external");
        assert_eq!(actions[1].icon, "link");
    }

    #[test]
    fn a_name_no_action_answers_to_is_reported() {
        let config = Config {
            items: vec![Item::Named("open-in-nothing".to_owned())],
            exclude: vec!["copy-markdown".to_owned(), "nope".to_owned()],
            ..Config::default()
        };
        assert_eq!(
            unknown_items(&config),
            vec!["open-in-nothing".to_owned(), "nope".to_owned()]
        );
        assert!(resolve(&config, &targets(), &Strings::default()).is_empty());
    }

    #[test]
    fn strings_drive_every_label() {
        let strings = Strings {
            copy_page: "Kopieren".to_owned(),
            open_in: "Öffnen in".to_owned(),
            ..Strings::default()
        };
        let actions = resolve(&Config::default(), &targets(), &strings);
        assert_eq!(actions[0].label, "Kopieren");
        assert_eq!(actions[2].label, "Öffnen in ChatGPT");
    }

    #[test]
    fn placement_answers_both_questions() {
        assert!(Placement::Both.in_header() && Placement::Both.in_sidebar());
        assert!(Placement::Header.in_header() && !Placement::Header.in_sidebar());
        assert!(Placement::Sidebar.in_sidebar() && !Placement::Sidebar.in_header());
    }
}
