//! The product skill and the agent card (RX-73).
//!
//! An agent that has never seen this site needs three things in order: what the
//! product is, which pages answer which question, and the richer interface to
//! use once it has a question. That is the whole of `skill.md`
//! (`plan/rfcs/1003-agent-card-and-skill-shape.md`).

use std::fmt::Write as _;

use serde_json::json;

use crate::agents::llms::{MCP_PATH, ROOT_PATH};
use crate::agents::resource::{self, Resource, Surfaces};
use crate::agents::site::{PageRecord, SiteInput};

pub const SKILL_PATH: &str = "/skill.md";
pub const SKILLS_DIR: &str = "/.well-known/skills";
pub const AGENT_CARD_PATH: &str = "/.well-known/agent-card.json";

/// How many pages the generated skill lists before it stops being a skill and
/// starts being a second index.
const KEY_PAGES: usize = 12;

/// Generates the product skill, the custom skills, and the agent card.
pub fn generate(site: &SiteInput) -> Surfaces {
    let mut out = Surfaces::default();
    if !site.agents.skill.enabled {
        return out;
    }

    let name = skill_name(site);
    let description = description(site);
    let body = product_skill(site, &name, &description);
    out.resources
        .push(Resource::new(SKILL_PATH, resource::MARKDOWN, body.clone()));
    out.resources.push(Resource::new(
        format!("{SKILLS_DIR}/{name}.md"),
        resource::MARKDOWN,
        body,
    ));

    for custom in &site.agents.skill.files {
        let slug = slug(&custom.name);
        out.resources.push(
            Resource::new(
                format!("{SKILLS_DIR}/{slug}.md"),
                resource::MARKDOWN,
                custom.body.clone(),
            )
            .restricted_to(custom.groups.clone()),
        );
    }

    out.resources.push(Resource::new(
        AGENT_CARD_PATH,
        resource::JSON,
        agent_card(site, &name, &description),
    ));
    out
}

fn skill_name(site: &SiteInput) -> String {
    let base = site
        .agents
        .mcp
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&site.name);
    slug(base)
}

fn description(site: &SiteInput) -> String {
    site.agents
        .mcp
        .description
        .as_deref()
        .or(site.summary.as_deref())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map_or_else(
            || format!("The documentation for {}.", site.name),
            str::to_owned,
        )
}

fn product_skill(site: &SiteInput, name: &str, description: &str) -> String {
    let mut out = format!("---\nname: {name}\ndescription: {description}\n---\n\n");
    let _ = writeln!(out, "# {} documentation\n", site.name);

    out.push_str("## When to use this skill\n\n");
    let _ = writeln!(
        out,
        "Use it for any question about {}: how to install it, how to configure \
         it, what an option does, or what an endpoint returns. Read the page \
         rather than recalling it — every page is served as Markdown at its own \
         URL with `.md` appended.\n",
        site.name
    );

    out.push_str("## Key pages\n\n");
    for page in key_pages(site) {
        out.push_str(&page_entry(site, page));
    }
    let _ = writeln!(
        out,
        "\nThe full index is at {}.",
        site.origin.resource_url(ROOT_PATH)
    );

    if site.agents.mcp.enabled {
        let _ = write!(
            out,
            "\n## MCP server\n\n{}\n\nIt serves `search`, `fetch`, `list_pages`, \
             and `ask` over Streamable HTTP.\n",
            site.origin.resource_url(MCP_PATH)
        );
    }

    out.push_str("\n## Example prompts\n\n");
    for prompt in example_prompts(site) {
        let _ = writeln!(out, "- {prompt}");
    }
    out
}

/// The pages a skill points at: navigation order, which puts the pages an
/// author thought were important first.
fn key_pages(site: &SiteInput) -> Vec<&PageRecord> {
    let mut out = Vec::new();
    for section in &site.nav {
        for route in &section.routes {
            if let Some(page) = site.page(route).filter(|page| page.is_published()) {
                out.push(page);
            }
            if out.len() == KEY_PAGES {
                return out;
            }
        }
    }
    for page in site.published() {
        if out.len() == KEY_PAGES {
            break;
        }
        if !out.iter().any(|listed| listed.route == page.route) {
            out.push(page);
        }
    }
    out
}

fn page_entry(site: &SiteInput, page: &PageRecord) -> String {
    let url = site.origin.markdown_url(&page.route);
    match page
        .description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty())
    {
        Some(description) => format!("- [{}]({url}): {description}\n", page.title),
        None => format!("- [{}]({url})\n", page.title),
    }
}

fn example_prompts(site: &SiteInput) -> Vec<String> {
    let name = &site.name;
    let mut out = vec![
        format!("\"How do I get started with {name}?\""),
        format!("\"What does this {name} error mean?\""),
    ];
    if let Some(page) = key_pages(site).first() {
        out.push(format!(
            "\"Summarize {} from the {name} docs.\"",
            page.title
        ));
    }
    out.push(format!(
        "\"Which {name} page documents the configuration keys?\""
    ));
    out
}

/// The agent card. Field names follow the A2A card; the document claims no
/// version of it, because nothing in this repository tracks one
/// (`plan/rfcs/1003-agent-card-and-skill-shape.md`).
fn agent_card(site: &SiteInput, name: &str, description: &str) -> String {
    let mut skills = vec![json!({
        "id": name,
        "name": format!("{} documentation", site.name),
        "description": description,
        "tags": ["documentation", "reference"],
        "examples": example_prompts(site),
    })];
    for custom in &site.agents.skill.files {
        skills.push(json!({
            "id": slug(&custom.name),
            "name": custom.name,
            "description": custom.description,
            "tags": ["documentation"],
            "examples": [],
        }));
    }

    let card = json!({
        "name": site.name,
        "description": description,
        "url": site.origin.base(),
        "documentationUrl": site.origin.resource_url(ROOT_PATH),
        "version": site.version.as_ref().map_or("1.0.0", |v| v.as_str()),
        "defaultInputModes": ["text/plain"],
        "defaultOutputModes": ["text/plain", "text/markdown"],
        "capabilities": {
            "streaming": site.agents.mcp.enabled,
            "pushNotifications": false,
        },
        "skills": skills,
    });
    let mut body = serde_json::to_string_pretty(&card).unwrap_or_else(|_| "{}".to_owned());
    body.push('\n');
    body
}

fn slug(text: &str) -> String {
    let slug = liyasa_components::anchor::slug(text);
    if slug.is_empty() {
        "docs".to_owned()
    } else {
        slug
    }
}

/// Every skill an index should mention, for the `llms.txt` optional section.
pub fn skill_urls(site: &SiteInput) -> Vec<String> {
    generate(site)
        .resources
        .iter()
        .filter(|r| r.is_public() && r.path.ends_with(".md"))
        .map(|r| site.origin.resource_url(&r.path))
        .collect()
}

#[cfg(test)]
mod tests {
    use liyasa_core::ids::{Locale, Route};

    use super::*;
    use crate::agents::site::{
        AgentsSettings, CanonicalOrigin, CustomSkill, FeedsSettings, NavSection,
    };

    fn page(route: &str, title: &str) -> PageRecord {
        PageRecord {
            id: None,
            route: Route::new(route),
            title: title.to_owned(),
            description: Some(format!("What {title} is for.")),
            locale: Locale::new("en"),
            version: None,
            tab: None,
            group: None,
            indexable: true,
            personalized: false,
            markdown: format!("# {title}\n"),
            updated: None,
            changelog: false,
        }
    }

    fn site() -> SiteInput {
        SiteInput {
            name: "Liyasa".to_owned(),
            summary: Some("Documentation that agents and people can both read.".to_owned()),
            origin: CanonicalOrigin::parse("https://example.com").expect("a valid origin"),
            locale: Locale::new("en"),
            version: None,
            pages: vec![page("/guide/install", "Install"), page("/api/pets", "Pets")],
            nav: vec![NavSection {
                title: "Getting started".to_owned(),
                tab: None,
                routes: vec![Route::new("/guide/install"), Route::new("/api/pets")],
            }],
            agents: AgentsSettings::default(),
            feeds: FeedsSettings::default(),
        }
    }

    fn body_of(surfaces: &Surfaces, path: &str) -> String {
        surfaces
            .get(path)
            .unwrap_or_else(|| panic!("{path}"))
            .body
            .clone()
    }

    #[test]
    fn rx_73_the_product_skill_carries_everything_the_requirement_names() {
        let surfaces = generate(&site());
        let skill = body_of(&surfaces, SKILL_PATH);
        assert!(skill.starts_with("---\nname: liyasa\n"), "{skill}");
        assert!(
            skill.contains("description: Documentation that agents"),
            "{skill}"
        );
        assert!(skill.contains("## When to use this skill"), "{skill}");
        assert!(skill.contains("## Key pages"), "{skill}");
        assert!(
            skill.contains("- [Install](https://example.com/guide/install.md)"),
            "{skill}"
        );
        assert!(skill.contains("## MCP server"), "{skill}");
        assert!(skill.contains("https://example.com/mcp"), "{skill}");
        assert!(skill.contains("## Example prompts"), "{skill}");
    }

    #[test]
    fn rx_73_the_skill_is_served_from_both_paths_with_the_same_bytes() {
        let surfaces = generate(&site());
        assert_eq!(
            body_of(&surfaces, SKILL_PATH),
            body_of(&surfaces, "/.well-known/skills/liyasa.md")
        );
    }

    #[test]
    fn rx_73_custom_skills_are_published_under_well_known() {
        let mut site = site();
        site.agents.skill.files = vec![CustomSkill {
            name: "Migrating from v1".to_owned(),
            description: "How to move a v1 project to v2.".to_owned(),
            body: "---\nname: migrating\n---\n\n# Migrating\n".to_owned(),
            groups: Vec::new(),
        }];
        let surfaces = generate(&site);
        let custom = surfaces
            .get("/.well-known/skills/migrating-from-v1.md")
            .expect("the custom skill");
        assert!(custom.body.contains("# Migrating"));
        assert!(custom.is_public());
    }

    #[test]
    fn rx_73_a_group_restricted_skill_carries_its_groups() {
        let mut site = site();
        site.agents.skill.files = vec![CustomSkill {
            name: "Internal runbook".to_owned(),
            description: "On-call steps.".to_owned(),
            body: "# Runbook\n".to_owned(),
            groups: vec!["staff".to_owned()],
        }];
        let surfaces = generate(&site);
        let custom = surfaces
            .get("/.well-known/skills/internal-runbook.md")
            .expect("the custom skill");
        assert_eq!(custom.groups, ["staff"]);
        assert!(!custom.is_public());
    }

    #[test]
    fn rx_73_the_agent_card_is_json_with_one_entry_per_skill() {
        let mut site = site();
        site.agents.skill.files = vec![CustomSkill {
            name: "Migrating".to_owned(),
            description: "How to move to v2.".to_owned(),
            body: "# Migrating\n".to_owned(),
            groups: Vec::new(),
        }];
        let surfaces = generate(&site);
        let card: serde_json::Value =
            serde_json::from_str(&body_of(&surfaces, AGENT_CARD_PATH)).expect("valid JSON");
        assert_eq!(card["name"], "Liyasa");
        assert_eq!(card["url"], "https://example.com");
        assert_eq!(card["documentationUrl"], "https://example.com/llms.txt");
        let skills = card["skills"].as_array().expect("skills");
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0]["id"], "liyasa");
        assert_eq!(skills[1]["id"], "migrating");
        assert!(
            !skills[0]["examples"]
                .as_array()
                .expect("examples")
                .is_empty()
        );
    }

    #[test]
    fn rx_73_a_personalized_page_is_never_a_key_page() {
        let mut site = site();
        let mut private = page("/dashboard", "Dashboard");
        private.personalized = true;
        site.pages.push(private);
        site.nav[0].routes.push(Route::new("/dashboard"));
        assert!(!body_of(&generate(&site), SKILL_PATH).contains("Dashboard"));
    }

    #[test]
    fn rx_73_skills_can_be_turned_off_entirely() {
        let mut site = site();
        site.agents.skill.enabled = false;
        assert!(generate(&site).resources.is_empty());
    }

    #[test]
    fn rx_73_every_key_page_link_is_absolute_and_markdown() {
        let skill = body_of(&generate(&site()), SKILL_PATH);
        for target in crate::agents::llms::link_targets(&skill) {
            assert!(target.starts_with("https://example.com/"), "{target}");
            assert!(target.ends_with(".md"), "{target}");
        }
    }
}
