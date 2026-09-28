//! Reading the Markdown twin: its title, its sections, and a snippet around a
//! match.
//!
//! The twin is what `<route>.md` serves — the document `agents/markdown`
//! renders for `Audience::Agent`, so it is ordinary Markdown with ATX
//! headings and no front matter. Everything here is a function of that text,
//! which is why it is a module of its own with its own tests rather than
//! private helpers on the reader: the reader needs a bundle and these need a
//! string.

use liyasa_markdown::ast::anchors::slugify;

/// A page's title: its first level-one heading.
///
/// Falls back to the last route segment in title case, because a page with no
/// `# ` heading still has to be listable — an agent choosing between
/// `/guides/install` and a blank is not choosing.
pub fn title_of(markdown: &str, route: &str) -> String {
    for line in markdown.lines() {
        if let Some(text) = line.strip_prefix("# ") {
            let text = text.trim();
            if !text.is_empty() {
                return text.to_owned();
            }
        }
    }
    title_from_route(route)
}

fn title_from_route(route: &str) -> String {
    let segment = route.rsplit('/').find(|part| !part.is_empty());
    let Some(segment) = segment else {
        return "Home".to_owned();
    };
    let mut out = String::with_capacity(segment.len());
    for (index, word) in segment.split(['-', '_']).enumerate() {
        if word.is_empty() {
            continue;
        }
        if index > 0 {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    match out.is_empty() {
        true => segment.to_owned(),
        false => out,
    }
}

/// One heading and the text under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub level: u8,
    pub heading: String,
    pub anchor: String,
    /// The heading line and everything below it, up to the next heading of
    /// the same level or shallower.
    pub body: String,
}

/// Every section of a document, in order.
///
/// A fenced code block can contain a line that looks like a heading, and a
/// section boundary taken inside one would cut a code sample in half and hand
/// the agent something that does not compile. So fences are tracked.
pub fn sections(markdown: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut fence: Option<String> = None;
    let mut starts: Vec<(usize, u8)> = Vec::new();
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0usize;
    for line in markdown.split_inclusive('\n') {
        lines.push((offset, line));
        offset += line.len();
    }

    for (index, (offset, line)) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if let Some(open) = &fence {
            if trimmed.starts_with(open.as_str()) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            let marker: String = trimmed.chars().take(3).collect();
            fence = Some(marker);
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if hashes == 0 || hashes > 6 {
            continue;
        }
        let rest = &trimmed[hashes..];
        if !rest.starts_with(' ') {
            continue;
        }
        let heading = rest.trim().trim_end_matches('#').trim().to_owned();
        if heading.is_empty() {
            continue;
        }
        starts.push((index, hashes as u8));
        out.push(Section {
            level: hashes as u8,
            anchor: slugify(&heading),
            heading,
            body: String::new(),
        });
        let _ = offset;
    }

    for (position, (line_index, level)) in starts.iter().enumerate() {
        let end = starts
            .iter()
            .skip(position + 1)
            .find(|(_, next)| next <= level)
            .map(|(index, _)| *index)
            .unwrap_or(lines.len());
        let start_byte = lines[*line_index].0;
        let end_byte = match end < lines.len() {
            true => lines[end].0,
            false => markdown.len(),
        };
        out[position].body = markdown[start_byte..end_byte].trim_end().to_owned();
    }
    out
}

/// The section a caller named, by anchor or by heading text.
///
/// An agent that read a search result has the anchor; an agent that read the
/// page has the heading. Refusing the second spelling would make the tool
/// work only for callers who had already used the other tool.
pub fn section_named<'a>(sections: &'a [Section], name: &str) -> Option<&'a Section> {
    let wanted = name.trim().trim_start_matches('#');
    sections
        .iter()
        .find(|section| section.anchor == wanted)
        .or_else(|| {
            sections
                .iter()
                .find(|section| section.heading.eq_ignore_ascii_case(wanted))
        })
        .or_else(|| {
            let slug = slugify(wanted);
            sections.iter().find(|section| section.anchor == slug)
        })
}

/// A readable line or two around the first match, with the terms intact.
///
/// Returns `None` when nothing matched, so a caller cannot report a hit it
/// cannot show.
pub fn snippet_for(markdown: &str, terms: &[String], width: usize) -> Option<String> {
    let lower = markdown.to_lowercase();
    let at = terms
        .iter()
        .filter_map(|term| lower.find(term.as_str()))
        .min()?;
    // Back up to a character boundary and forward to one, because a byte
    // index into a multi-byte character panics on slicing.
    let start = floor_boundary(markdown, at.saturating_sub(width / 2));
    let end = ceil_boundary(markdown, (at + width / 2).min(markdown.len()));
    let mut text = markdown[start..end].replace(['\n', '\r'], " ");
    while text.contains("  ") {
        text = text.replace("  ", " ");
    }
    let text = text.trim().to_owned();
    match text.is_empty() {
        true => None,
        false => Some(text),
    }
}

fn floor_boundary(text: &str, mut index: usize) -> usize {
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn ceil_boundary(text: &str, mut index: usize) -> usize {
    while index < text.len() && !text.is_char_boundary(index) {
        index += 1;
    }
    index
}

/// The words a query is matched on: lowercase, punctuation dropped, one
/// character discarded.
pub fn terms(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|word| word.chars().count() > 1)
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "> For AI agents: an index is at /llms.txt\n\
                        \n\
                        # Install\n\
                        \n\
                        Run it.\n\
                        \n\
                        ## From source\n\
                        \n\
                        Clone and build.\n\
                        \n\
                        ### Windows\n\
                        \n\
                        Use the installer.\n\
                        \n\
                        ## From a package\n\
                        \n\
                        Use your package manager.\n";

    #[test]
    fn the_title_is_the_first_level_one_heading() {
        assert_eq!(title_of(PAGE, "/guides/install"), "Install");
    }

    #[test]
    fn a_page_with_no_heading_is_named_from_its_route() {
        assert_eq!(
            title_of("just prose\n", "/guides/getting-started"),
            "Getting Started"
        );
        assert_eq!(title_of("", "/"), "Home");
    }

    #[test]
    fn a_section_runs_to_the_next_heading_of_its_level_or_shallower() {
        let sections = sections(PAGE);
        let from_source = section_named(&sections, "from-source").expect("the section");
        assert!(from_source.body.contains("Clone and build."));
        // Its deeper subsection belongs to it...
        assert!(from_source.body.contains("Use the installer."));
        // ...and its sibling does not.
        assert!(
            !from_source.body.contains("package manager"),
            "{}",
            from_source.body
        );
    }

    #[test]
    fn a_section_is_found_by_anchor_or_by_heading() {
        let sections = sections(PAGE);
        for name in ["from-source", "From source", "#from-source", "FROM SOURCE"] {
            assert_eq!(
                section_named(&sections, name).map(|s| s.heading.as_str()),
                Some("From source"),
                "{name}"
            );
        }
        assert!(section_named(&sections, "no-such-section").is_none());
    }

    #[test]
    fn a_heading_inside_a_fence_is_not_a_section() {
        // Cutting here would hand an agent half a code sample, which is worse
        // than handing it the whole page.
        let page = "# Config\n\ntext\n\n```sh\n# not a heading\nliyasa build\n```\n\nmore\n";
        let found = sections(page);
        let headings: Vec<&str> = found.iter().map(|s| s.heading.as_str()).collect();
        assert_eq!(headings, ["Config"]);
        assert!(found[0].body.contains("liyasa build"));
    }

    #[test]
    fn a_snippet_carries_the_match_and_survives_multibyte_text() {
        let terms = terms("build");
        let snippet = snippet_for(PAGE, &terms, 40).expect("a snippet");
        assert!(snippet.to_lowercase().contains("build"), "{snippet}");

        // A byte index landing inside a character used to panic here.
        let unicode = format!("{}build{}", "é".repeat(40), "→".repeat(40));
        let snippet = snippet_for(&unicode, &terms, 21).expect("a snippet");
        assert!(snippet.contains("build"), "{snippet}");
    }

    #[test]
    fn nothing_matching_is_no_snippet_rather_than_an_empty_one() {
        assert_eq!(snippet_for(PAGE, &terms("kubernetes"), 40), None);
    }

    #[test]
    fn a_one_character_word_is_not_a_term() {
        // Otherwise `a` matches every page and the ranking is noise.
        assert_eq!(terms("How do I build?"), ["how", "do", "build"]);
    }
}
