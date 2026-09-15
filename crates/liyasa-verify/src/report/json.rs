//! The report as JSON (VER-70), which is the model itself.

use crate::core::scrub::Scrubber;

use super::Report;

pub fn render(report: &Report, scrubber: &Scrubber) -> String {
    // Serialize, then scrub: a check's excerpt is already scrubbed, and this
    // catches anything a caller assembled by hand.
    let json = serde_json::to_string_pretty(report)
        .unwrap_or_else(|error| format!("{{\"error\":\"{error}\"}}"));
    scrubber.scrub(&json)
}
