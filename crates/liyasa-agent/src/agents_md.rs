//! `AGENTS.md`, the project's instructions to the writing agent (AGT-30).
//!
//! AGT-30 says the file "holds instructions (tone, structure rules, forbidden
//! phrases, product naming)" and does not say in what shape. Two of those four are
//! machine-checkable and two are not, so this module reads the two and passes the
//! rest through:
//!
//! - **Forbidden phrases** and **product naming** are extracted, under the
//!   conventions RFC 2501 records, and checked against what the agent wrote.
//! - **Tone** and **structure rules** are prose, and prose goes into the run's
//!   system prompt as operator text. `AGENTS.md` is written by the operator, so
//!   unlike everything else a run reads, it may be an instruction (§30.2.2).
//!
//! TODO(rfc-2501): the conventions below are this crate's choice, not the PRD's.
//!
//! The conventions are two headings and a list under each. A file with neither is
//! still valid and still reaches the prompt; it simply has nothing to check. That
//! is the point of reading the file loosely: a project whose `AGENTS.md` is
//! paragraphs of tone advice must not have it rejected for lacking a section.
//!
//! The check is on the **written text**, in the validate phase, not on the prompt.
//! A model told not to say "simply" and saying it anyway is the case this exists
//! for, and AGT-30's acceptance test replays exactly that with a mock model.

use crate::injection::{Phrase, fold};

/// The headings whose list items are forbidden phrases, lowercased.
pub const FORBIDDEN_HEADINGS: &[&str] = &[
    "forbidden phrases",
    "forbidden words",
    "never write",
    "do not write",
    "never use",
    "do not use",
];

/// The headings whose list items are naming corrections.
pub const NAMING_HEADINGS: &[&str] = &["product naming", "product names", "naming"];

/// One thing the project does not want written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The text as `AGENTS.md` wrote it, for the message.
    pub text: String,
    /// What to write instead, where the rule says.
    pub instead: Option<String>,
    phrase: Phrase,
}

impl Rule {
    pub fn new(text: impl Into<String>, instead: Option<String>) -> Self {
        let text = text.into();
        Self {
            phrase: Phrase::new(&text),
            text,
            instead,
        }
    }

    /// How a reviewer is told about it.
    pub fn message(&self) -> String {
        match &self.instead {
            Some(instead) => format!(
                "`AGENTS.md` asks for `{}` rather than `{}`",
                instead, self.text
            ),
            None => format!("`AGENTS.md` forbids `{}`", self.text),
        }
    }
}

/// `AGENTS.md`, read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentsMd {
    /// The whole file. Operator text, so it may reach the system prompt.
    pub instructions: String,
    pub rules: Vec<Rule>,
}

/// Where a rule was broken.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: Rule,
    /// 1-based.
    pub line: u32,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.rule.message())
    }
}

/// Reads the file.
pub fn parse(text: &str) -> AgentsMd {
    let mut rules = Vec::new();
    let mut section = Section::Other;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix('#') {
            let name = heading.trim_start_matches('#').trim().to_ascii_lowercase();
            section = if FORBIDDEN_HEADINGS.contains(&name.as_str()) {
                Section::Forbidden
            } else if NAMING_HEADINGS.contains(&name.as_str()) {
                Section::Naming
            } else {
                Section::Other
            };
            continue;
        }
        let Some(item) = list_item(trimmed) else {
            continue;
        };
        match section {
            Section::Forbidden => {
                let (text, instead) = split_correction(item);
                rules.push(Rule::new(text, instead));
            }
            Section::Naming => {
                // A naming row without a correction says nothing checkable: the
                // rule is "write X", and there is no X to look for.
                let (text, instead) = split_correction(item);
                if instead.is_some() {
                    rules.push(Rule::new(text, instead));
                }
            }
            Section::Other => {}
        }
    }
    AgentsMd {
        instructions: text.to_owned(),
        rules,
    }
}

enum Section {
    Forbidden,
    Naming,
    Other,
}

/// The content of a `-`, `*` or `+` list item.
fn list_item(line: &str) -> Option<&str> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some(rest.trim());
        }
    }
    None
}

/// `wrong -> right` in any of its spellings, or just `wrong`.
///
/// Inline code and emphasis are stripped, so `` `simply` `` and `**simply**` are
/// the same rule: an operator writing a style guide in Markdown will mark up the
/// word being discussed, and a rule that only matched the bare form would silently
/// do nothing.
fn split_correction(item: &str) -> (String, Option<String>) {
    for separator in ["→", "->", "=>", " not ", " rather than ", " instead of "] {
        if let Some((left, right)) = item.split_once(separator) {
            let (wrong, right_text) = match separator {
                // `X not Y` and `X rather than Y` and `X instead of Y` put the
                // WANTED word first; the arrows put it second.
                " not " | " rather than " | " instead of " => (right, left),
                _ => (left, right),
            };
            return (
                unmark(wrong),
                Some(unmark(right_text)).filter(|s| !s.is_empty()),
            );
        }
    }
    (unmark(item), None)
}

/// Drops the Markdown a style guide marks its examples with, and the words around
/// it that are instruction rather than example.
///
/// `- Write **Acme Cloud**, not Acmecloud` means the wanted text is `Acme Cloud`,
/// not `Write **Acme Cloud**,`. The leading verb and the punctuation are the
/// operator writing a sentence, and a rule that kept them would tell a reviewer to
/// write "Write **Acme Cloud**,".
fn unmark(text: &str) -> String {
    let mut text = text.trim();
    for verb in [
        "Write ", "write ", "Use ", "use ", "Prefer ", "prefer ", "Say ", "say ",
    ] {
        if let Some(rest) = text.strip_prefix(verb) {
            text = rest.trim();
            break;
        }
    }
    text.trim_matches(|c: char| {
        c.is_whitespace() || matches!(c, '.' | ',' | ';' | ':' | '`' | '*' | '_' | '"' | '\'')
    })
    .to_owned()
}

impl AgentsMd {
    /// Every rule broken by `written`, with the line it was broken on.
    ///
    /// The word-boundary fold only: a one-word rule must not fire inside a longer
    /// word, or `simply` forbids `simplify`.
    pub fn violations(&self, written: &str) -> Vec<Violation> {
        let mut out = Vec::new();
        for (index, line) in written.lines().enumerate() {
            let folded = fold(line);
            for rule in &self.rules {
                if rule.phrase.found_spaced(&folded) {
                    out.push(Violation {
                        rule: rule.clone(),
                        line: index as u32 + 1,
                    });
                }
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.instructions.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
# Writing for Acme

Write in the present tense and address the reader as `you`.

## Forbidden phrases

- simply
- `just` works
- click here

## Product naming

- Write **Acme Cloud**, not Acmecloud
- acme-cli → `acme`

## Structure

Every page opens with a sentence that says what it is for.
";

    #[test]
    fn the_whole_file_is_kept_as_operator_instructions() {
        let parsed = parse(FILE);
        assert!(parsed.instructions.contains("present tense"));
        assert!(parsed.instructions.contains("opens with a sentence"));
        assert!(!parsed.is_empty());
    }

    #[test]
    fn the_forbidden_phrases_are_read() {
        let parsed = parse(FILE);
        let texts: Vec<&str> = parsed.rules.iter().map(|r| r.text.as_str()).collect();
        assert!(texts.contains(&"simply"), "{texts:?}");
        assert!(texts.contains(&"click here"), "{texts:?}");
    }

    #[test]
    fn markup_around_a_phrase_is_stripped() {
        // An operator writing a style guide marks up the word being discussed. A
        // rule that only matched the bare form would quietly do nothing.
        let parsed = parse(
            "## Forbidden phrases\n\n- `simply`\n- **easy**\n- \"obviously\"\n- _clearly_.\n",
        );
        let texts: Vec<&str> = parsed.rules.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(texts, ["simply", "easy", "obviously", "clearly"]);
    }

    #[test]
    fn a_naming_rule_reads_the_wrong_form_whichever_way_it_is_written() {
        let parsed = parse(FILE);
        let acmecloud = parsed
            .rules
            .iter()
            .find(|r| r.text == "Acmecloud")
            .expect("`X, not Y` puts the wrong form second");
        assert_eq!(acmecloud.instead.as_deref(), Some("Acme Cloud"));

        let cli = parsed
            .rules
            .iter()
            .find(|r| r.text == "acme-cli")
            .expect("`Y → X` puts the wrong form first");
        assert_eq!(cli.instead.as_deref(), Some("acme"));
    }

    #[test]
    fn a_naming_row_with_no_correction_is_not_a_rule() {
        // "Write Acme Cloud" says nothing to look for.
        let parsed = parse("## Product naming\n\n- Acme Cloud\n");
        assert!(parsed.rules.is_empty(), "{:?}", parsed.rules);
    }

    #[test]
    fn a_file_with_no_sections_is_still_valid_and_has_nothing_to_check() {
        let parsed = parse("Write clearly. Keep paragraphs short.\n");
        assert!(parsed.rules.is_empty());
        assert!(!parsed.is_empty());
        assert!(parsed.violations("Simply click here.").is_empty());
    }

    #[test]
    fn a_forbidden_phrase_in_written_output_is_a_violation_with_its_line() {
        // AGT-30's acceptance shape: the model wrote the phrase and the validate
        // phase has to see it.
        let parsed = parse(FILE);
        let written = "# Install\n\nSimply download the file.\n\nThen run it.\n";
        let violations = parsed.violations(written);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].line, 3);
        assert_eq!(violations[0].rule.text, "simply");
        assert!(
            violations[0].to_string().contains("forbids"),
            "{}",
            violations[0]
        );
    }

    #[test]
    fn a_naming_violation_says_what_to_write_instead() {
        let parsed = parse(FILE);
        let violations = parsed.violations("Acmecloud is where it runs.\n");
        assert_eq!(violations.len(), 1, "{violations:?}");
        let message = violations[0].rule.message();
        assert!(message.contains("Acme Cloud"), "{message}");
        assert!(message.contains("Acmecloud"), "{message}");
    }

    #[test]
    fn case_and_markup_in_the_written_text_do_not_hide_a_violation() {
        let parsed = parse(FILE);
        for written in [
            "SIMPLY download it.",
            "**Simply** download it.",
            "Simply   download it.",
        ] {
            assert_eq!(
                parsed.violations(written).len(),
                1,
                "`{written}` passed the style rule"
            );
        }
    }

    #[test]
    fn a_one_word_rule_does_not_fire_inside_a_longer_word() {
        // `simply` must not forbid `simplify`. This is why the check uses the
        // word-boundary fold and not the one the injection corpus uses.
        let parsed = parse("## Forbidden phrases\n\n- simply\n");
        assert!(parsed.violations("This simplifies the setup.").is_empty());
        assert!(parsed.violations("It is a simple setup.").is_empty());
    }

    #[test]
    fn a_multi_word_rule_needs_the_words_together() {
        let parsed = parse("## Forbidden phrases\n\n- click here\n");
        assert_eq!(parsed.violations("Click here to start.").len(), 1);
        assert!(
            parsed
                .violations("Click the button, then look here.")
                .is_empty()
        );
    }

    #[test]
    fn every_forbidden_heading_spelling_is_recognised() {
        for heading in FORBIDDEN_HEADINGS {
            let parsed = parse(&format!("## {heading}\n\n- simply\n"));
            assert_eq!(parsed.rules.len(), 1, "`{heading}` was not recognised");
        }
    }

    #[test]
    fn a_heading_at_any_depth_opens_a_section() {
        for hashes in ["#", "##", "###", "####"] {
            let parsed = parse(&format!("{hashes} Forbidden phrases\n\n- simply\n"));
            assert_eq!(parsed.rules.len(), 1, "`{hashes}` was not a heading");
        }
    }

    #[test]
    fn a_list_outside_a_recognised_section_is_not_a_rule() {
        let parsed = parse("## Structure\n\n- open with a summary\n- keep it short\n");
        assert!(parsed.rules.is_empty(), "{:?}", parsed.rules);
    }

    #[test]
    fn a_section_ends_at_the_next_heading() {
        let parsed = parse("## Forbidden phrases\n\n- simply\n\n## Structure\n\n- be brief\n");
        assert_eq!(parsed.rules.len(), 1);
        assert_eq!(parsed.rules[0].text, "simply");
    }

    #[test]
    fn every_list_marker_is_read() {
        let parsed = parse("## Forbidden phrases\n\n- a phrase\n* b phrase\n+ c phrase\n");
        assert_eq!(parsed.rules.len(), 3);
    }

    #[test]
    fn an_empty_file_has_no_rules_and_no_instructions() {
        let parsed = parse("");
        assert!(parsed.is_empty());
        assert!(parsed.rules.is_empty());
    }
}
