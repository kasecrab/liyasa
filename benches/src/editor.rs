//! Keystroke to preview, through the renderer the editor actually uses.
//!
//! NFR-05 budgets "keystroke to preview under 50 ms for pages under 5,000 words
//! using the WebAssembly renderer". That figure is about a browser, and this
//! measures a NATIVE build of the same crate — so it cannot confirm the budget
//! and does not claim to.
//!
//! **It can falsify it.** `liyasa-wasm` compiled for the host runs the same
//! parse, expand and render that `wasm32-unknown-unknown` runs, on a machine
//! with no browser event loop, no module instantiation and no JavaScript
//! boundary to cross. Every one of those makes the browser slower, never
//! faster. So a native render over 50 ms is proof the browser misses the
//! budget, while a native render under it proves only that the budget is not
//! already lost before the browser is involved.
//!
//! That asymmetry is the whole value and the reason the report prints it as a
//! lower bound rather than as the row's figure. A number that cannot confirm
//! the thing it is placed beside has to say so, or it gets read as the thing.
//!
//! The browser half needs Playwright driving the real editor, which lives in
//! `web/` and is not this package's path.

use std::time::{Duration, Instant};

use liyasa_wasm::api::{OpenRequest, PreviewRequest, SeedEntry, SiteMeta};
use liyasa_wasm::session::Session;

/// NFR-05's precondition. The budget is for pages under this length, so a
/// measurement of a longer one would not be about the row.
pub const WORD_LIMIT: usize = 5_000;

/// A page just under the limit, which is the slowest page the budget covers.
///
/// Prose with headings and inline marks rather than one repeated word: the
/// parser's work is in block and inline structure, and a page of `word word
/// word` would measure the fast path and report it as the budget's case.
pub fn page(words: usize) -> String {
    let mut out = String::from("---\ntitle: Preview budget\n---\n\n");
    // Counted the way the budget counts, by whitespace-separated token, and
    // counted as the page is built rather than afterwards. The first version
    // emitted a fixed number of chunks and produced 5,389 words for a target of
    // 4,900, because `[a link](/elsewhere)` is two tokens and `**bold**` is one
    // — so it measured a page OUTSIDE the range the row covers and reported it
    // as the row's case.
    let mut written = out.split_whitespace().count();
    let mut section = 0;
    let push = |out: &mut String, chunk: &str, written: &mut usize| {
        *written += chunk.split_whitespace().count();
        out.push_str(chunk);
    };
    while written < words {
        section += 1;
        push(&mut out, &format!("## Section {section}\n\n"), &mut written);
        for paragraph in 0..4 {
            if written >= words {
                break;
            }
            for word in 0..40 {
                if written >= words {
                    break;
                }
                // Inline marks on a predictable cadence, so the inline parser
                // has real work and the figure does not drift with the text.
                let chunk = match (paragraph + word) % 11 {
                    0 => "`code` ",
                    3 => "**bold** ",
                    7 => "[a link](/elsewhere) ",
                    _ => "word ",
                };
                push(&mut out, chunk, &mut written);
            }
            out.push_str("\n\n");
        }
    }
    out
}

/// What one preview of `source` costs, and what it produced.
///
/// The output length is returned with the duration on purpose: an empty render
/// is fast, and a measurement that cannot tell "rendered nothing quickly" from
/// "rendered the page quickly" is the shape of every vacuous check in this
/// repository.
pub struct Preview {
    pub elapsed: Duration,
    pub html_bytes: usize,
    pub words: usize,
}

/// Render the page once per sample and keep the median and the slowest.
///
/// Median rather than mean: one scheduler stall should not move the figure a
/// release note quotes. The maximum is kept beside it because the budget is
/// about a keystroke feeling instant, and a p50 that hides a 300 ms outlier
/// describes an editor nobody would call responsive.
pub fn measure(samples: usize) -> Result<(Preview, Duration), String> {
    let source = page(WORD_LIMIT - 100);
    let words = source.split_whitespace().count();

    let request = OpenRequest {
        // 32 hexadecimal characters: the nonce makes a directive marker
        // unforgeable, and `Session::sealed` refuses anything else.
        nonce: "0123456789abcdef0123456789abcdef".to_owned(),
        site: SiteMeta {
            name: "Benchmark docs".to_owned(),
            canonical_origin: "https://bench.example".to_owned(),
            llms_txt: "https://bench.example/llms.txt".to_owned(),
            version: None,
            locale: "en".to_owned(),
        },
        seed: Vec::<SeedEntry>::new(),
    };
    let session =
        Session::sealed(&request).map_err(|e| format!("the session did not open: {e:?}"))?;

    let preview = PreviewRequest {
        path: "guides/budget.md".to_owned(),
        source: source.clone(),
        context: serde_json::Value::Null,
        options: Default::default(),
    };

    // One render before timing. The first call through a fresh session pays for
    // allocator warm-up and lazily built tables that a keystroke in a live
    // editor has already paid for, so timing it would measure session open and
    // report it as a keystroke.
    let first = session.preview(&preview);
    if first.html.is_empty() {
        return Err(format!(
            "the preview produced no HTML, so there is nothing to time: {:?}",
            first.diagnostics
        ));
    }

    let mut timings = Vec::with_capacity(samples);
    let mut html_bytes = 0;
    for _ in 0..samples.max(1) {
        let at = Instant::now();
        let response = session.preview(&preview);
        timings.push(at.elapsed());
        html_bytes = response.html.len();
    }
    timings.sort_unstable();
    let median = timings[timings.len() / 2];
    let slowest = *timings.last().expect("at least one sample");

    Ok((
        Preview {
            elapsed: median,
            html_bytes,
            words,
        },
        slowest,
    ))
}
