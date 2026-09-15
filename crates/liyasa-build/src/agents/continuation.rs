//! Continuation headers for the resources that have to be split (RX-64, spec
//! check `single-fetch-completeness`).
//!
//! A Markdown route is never paginated. The generated indexes that do continue
//! say so in their opening lines rather than at the end, because the end is the
//! first thing a truncating pipeline drops — a note there is a note nobody
//! reads.

use std::fmt::Write as _;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};

/// How many lines from the end of a body count as "trailing" for the purposes
/// of `E0407`. Generous: a note four lines from the end is still a note an
/// agent never sees.
const TRAILING_LINES: usize = 12;

/// Where one part sits in a split resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Continuation {
    /// 1-based.
    pub part: usize,
    pub total: usize,
    /// The absolute URL of the resource this part belongs to.
    pub of: String,
    pub previous: Option<String>,
    pub next: Option<String>,
}

impl Continuation {
    /// The opening lines of a part: part number, total, and absolute
    /// neighbours, as a blockquote so the part is still valid Markdown.
    pub fn header(&self) -> String {
        let mut out = format!("> Part {} of {} of {}\n", self.part, self.total, self.of);
        match &self.previous {
            Some(url) => {
                let _ = writeln!(out, "> Previous: {url}");
            }
            None => out.push_str("> Previous: none; this is the first part\n"),
        }
        match &self.next {
            Some(url) => {
                let _ = writeln!(out, "> Next: {url}");
            }
            None => out.push_str("> Next: none; this is the last part\n"),
        }
        out
    }

    /// The part with its header in front of it.
    pub fn open(&self, body: &str) -> String {
        format!("{}\n{}", self.header(), body.trim_start_matches('\n'))
    }
}

/// Whether a line declares a continuation.
fn declares_continuation(line: &str) -> bool {
    let line = line.trim_start_matches(['>', ' ', '-', '*']).trim();
    let lower = line.to_ascii_lowercase();
    lower.starts_with("part ") && lower.contains(" of ")
        || lower.starts_with("previous:")
        || lower.starts_with("next:")
        || lower.starts_with("continued")
        || lower.starts_with("continues in")
        || lower.contains("continued in")
        || lower.contains("continued at")
}

/// Rejects a continuation declared anywhere but the opening lines (`E0407`).
///
/// `name` is the resource's path, so the diagnostic names the file rather than
/// the line it was found on: these resources are generated, and a line number
/// in a generated file points at nothing an author can edit.
pub fn check(name: &str, body: &str, out: &mut Diagnostics) {
    let lines: Vec<&str> = body.lines().collect();
    let opening = opening_len(&lines);
    let trailing_start = lines.len().saturating_sub(TRAILING_LINES).max(opening);
    for (at, line) in lines.iter().enumerate().skip(trailing_start) {
        if declares_continuation(line) {
            out.push(
                Diagnostic::new(
                    code::E0407,
                    format!(
                        "`{name}` declares its continuation on line {} instead of in its opening lines",
                        at + 1
                    ),
                )
                .help(
                    "move the part number, the total, and the absolute previous and next URLs to \
                     the top of the response: a truncating pipeline drops the end first",
                ),
            );
            return;
        }
    }
}

/// How many lines the opening blockquote occupies, which is where a
/// continuation belongs.
fn opening_len(lines: &[&str]) -> usize {
    if !lines.first().is_some_and(|line| line.starts_with('>')) {
        return 0;
    }
    lines
        .iter()
        .position(|line| !line.starts_with('>'))
        .unwrap_or(lines.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts() -> Vec<Continuation> {
        let of = "https://example.com/llms-full.txt".to_owned();
        let url = |n: usize| format!("https://example.com/_llms/full/part-{n}.txt");
        (1..=3)
            .map(|part| Continuation {
                part,
                total: 3,
                of: of.clone(),
                previous: (part > 1).then(|| url(part - 1)),
                next: (part < 3).then(|| url(part + 1)),
            })
            .collect()
    }

    #[test]
    fn rx_64_a_part_declares_its_place_in_its_opening_lines() {
        let body = parts()[1].open("# Guide\n\nContent.\n");
        let opening: Vec<&str> = body.lines().take(3).collect();
        assert_eq!(
            opening,
            [
                "> Part 2 of 3 of https://example.com/llms-full.txt",
                "> Previous: https://example.com/_llms/full/part-1.txt",
                "> Next: https://example.com/_llms/full/part-3.txt",
            ]
        );
    }

    #[test]
    fn rx_64_the_first_and_last_parts_say_so() {
        let parts = parts();
        assert!(parts[0].header().contains("Previous: none"));
        assert!(parts[0].header().contains("Next: https://"));
        assert!(parts[2].header().contains("Next: none"));
    }

    #[test]
    fn rx_64_every_neighbour_url_is_absolute() {
        for part in parts() {
            for line in part.header().lines().skip(1) {
                let url = line
                    .split_once(": ")
                    .map(|(_, url)| url)
                    .unwrap_or_default();
                assert!(
                    url.starts_with("https://") || url.starts_with("none"),
                    "{line}"
                );
            }
        }
    }

    #[test]
    fn rx_64_an_opening_header_is_accepted() {
        let mut diagnostics = Diagnostics::new();
        check(
            "/_llms/full/part-2.txt",
            &parts()[1].open("# Guide\n\nContent.\n"),
            &mut diagnostics,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn rx_64_a_trailing_continuation_note_fails_the_build() {
        let mut diagnostics = Diagnostics::new();
        check(
            "/_llms/full/part-2.txt",
            "# Guide\n\nContent.\n\nContinued in https://example.com/_llms/full/part-3.txt\n",
            &mut diagnostics,
        );
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(codes, ["E0407"]);
        assert!(diagnostics.has_errors());
    }

    #[test]
    fn rx_64_a_trailing_next_link_fails_the_build_too() {
        let mut diagnostics = Diagnostics::new();
        check(
            "/_llms/full/part-2.txt",
            &format!(
                "{}\n# Guide\n\nContent.\n\n> Next: https://example.com/_llms/full/part-3.txt\n",
                parts()[1].header()
            ),
            &mut diagnostics,
        );
        let codes: Vec<&str> = diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(codes, ["E0407"]);
    }

    #[test]
    fn rx_64_ordinary_prose_is_not_a_continuation() {
        let mut diagnostics = Diagnostics::new();
        check(
            "/guide.md",
            "# Guide\n\nThe installer continued past the prompt because `--yes` was set.\n",
            &mut diagnostics,
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }
}
