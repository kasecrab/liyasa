//! Directives: the info string, leaf and inline forms, the tag form, and the
//! checks over the parsed tree (PRD §7.5.1, `plan/rfcs/0003-parser-spike.md`).
//!
//! comrak 0.55 owns container segmentation, so this module parses what comrak
//! hands over and scans the two forms comrak does not know about.

pub mod info;
pub mod inline;
pub mod leaf;
pub mod mask;
pub mod props;
pub mod tag;
