//! Every dialect into the 3.1 model (API-01).

pub mod v20;
pub mod v30;

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};

use crate::tree::Value;
use crate::version::SpecVersion;

/// Rewrites a parsed document into the 3.1 shape, whatever it arrived as.
/// Returns what the conversion has to say for itself.
pub fn to_3_1(root: &mut Value, version: &SpecVersion) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    match version {
        SpecVersion::V3_1(_) => {}
        SpecVersion::V3_0(_) => v30::normalize(root),
        SpecVersion::V2(text) => {
            diagnostics.push(
                Diagnostic::new(
                    code::W0509,
                    format!("Swagger {text} was converted to OpenAPI 3.1"),
                )
                .help(
                    "conversion is mechanical and covers what a reference page needs; \
                     publish a 3.1 document to keep the conversion out of the loop",
                ),
            );
            v20::convert(root);
            v30::normalize(root);
        }
    }
    diagnostics
}
