//! Cached results (VER-06).
//!
//! "Results are cached by the hash of the block, its setup, the runner image
//! digest, and the referenced fixtures." Every one of those is in the key, and
//! nothing else is: not the clock, not the check's own id, not the outcome of
//! the last run. A key that moved between two identical runs would make the
//! cache a slower way of never hitting.
//!
//! `liyasa verify --no-cache` builds this `disabled`, which stores nothing and
//! answers nothing — a miss every time rather than a cache that has to be
//! emptied.

use std::collections::BTreeMap;

use liyasa_core::ids::Fingerprint;
use liyasa_core::verify::{CheckResult, CheckSpec};

use super::code::Binding;
use super::image::ImagePin;

/// What VER-06 names, and only that.
pub fn key(spec: &CheckSpec, binding: &Binding, pin: &ImagePin) -> Fingerprint {
    let input = serde_json::to_vec(&spec.input).unwrap_or_default();
    let expect = serde_json::to_vec(&spec.expect).unwrap_or_default();
    let mut parts: Vec<Vec<u8>> = vec![
        input,
        expect,
        binding.setup.clone().unwrap_or_default().into_bytes(),
        pin.digest.clone().into_bytes(),
        pin.image.clone().into_bytes(),
    ];
    // The environment and the fence's other attributes change what runs as
    // surely as the source does.
    for (name, value) in &binding.env {
        parts.push(name.clone().into_bytes());
        parts.push(value.clone().into_bytes());
    }
    for (name, value) in &binding.attrs {
        parts.push(name.clone().into_bytes());
        parts.push(value.clone().into_bytes());
    }
    // Fixtures by content: a fixture whose bytes changed is a different check
    // even though the block did not move.
    for (path, bytes) in &binding.fixtures {
        parts.push(path.as_str().as_bytes().to_vec());
        parts.push(bytes.as_ref().to_vec());
    }
    for (path, bytes) in &binding.expected {
        parts.push(path.as_str().as_bytes().to_vec());
        parts.push(bytes.as_ref().to_vec());
    }
    Fingerprint::of_parts(parts.iter().map(Vec::as_slice))
}

#[derive(Debug, Clone)]
pub struct ResultCache {
    entries: BTreeMap<Fingerprint, CheckResult>,
    enabled: bool,
}

impl Default for ResultCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ResultCache {
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            enabled: true,
        }
    }

    /// `liyasa verify --no-cache`.
    pub fn disabled() -> Self {
        Self {
            entries: BTreeMap::new(),
            enabled: false,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn get(&self, key: &Fingerprint) -> Option<&CheckResult> {
        self.enabled.then(|| self.entries.get(key)).flatten()
    }

    pub fn put(&mut self, key: Fingerprint, result: CheckResult) {
        if self.enabled {
            self.entries.insert(key, result);
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests;
