//! Runner images, pinned by digest (VER-03).
//!
//! VER-03 says the container sandbox runs "a Docker or Podman image per
//! language, pinned by digest in `liyasa.lock`". A built-in runner therefore
//! names an image but never a digest: a digest baked into the binary is stale
//! the day after it ships. The digest comes from the project —
//! `verify.runners.images` or the lock — and a language with no digest is
//! `E0610` rather than a run against whatever `latest` happens to be today.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::{Diagnostic, code};

use crate::core::config::RunnersConfig;

/// An image and the digest it is pinned to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePin {
    pub image: String,
    /// `sha256:…`, as the registry writes it.
    pub digest: String,
}

impl ImagePin {
    /// Splits `name@sha256:…`. A reference with no digest is not a pin.
    pub fn parse(reference: &str) -> Option<Self> {
        let (image, digest) = reference.trim().split_once('@')?;
        (is_digest(digest) && !image.is_empty()).then(|| Self {
            image: image.to_owned(),
            digest: digest.to_owned(),
        })
    }

    pub fn reference(&self) -> String {
        format!("{}@{}", self.image, self.digest)
    }
}

fn is_digest(text: &str) -> bool {
    let Some(hex) = text.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The pins a project has, by language.
#[derive(Debug, Clone, Default)]
pub struct Images {
    registry: Option<String>,
    by_language: BTreeMap<String, String>,
}

impl Images {
    pub fn new(config: &RunnersConfig) -> Self {
        Self {
            registry: config
                .registry
                .as_deref()
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .map(str::to_owned),
            by_language: config
                .images
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
                .collect(),
        }
    }

    /// Adds what `liyasa.lock` recorded. Config wins: an operator who wrote a
    /// digest into `liyasa.json` meant that one.
    pub fn with_lock(mut self, entries: impl IntoIterator<Item = (String, String)>) -> Self {
        for (language, reference) in entries {
            self.by_language
                .entry(language.to_ascii_lowercase())
                .or_insert(reference);
        }
        self
    }

    /// The first of `keys` that is pinned. A runner claims several languages
    /// and an operator pins under whichever name they think of the image by,
    /// so `shell` and `bash` both find the shell image.
    pub fn pin_any(&self, keys: &[&str]) -> Result<ImagePin, Diagnostic> {
        let mut last = None;
        for key in keys {
            match self.pin(key) {
                Ok(pin) => return Ok(pin),
                Err(problem) => last = Some(problem),
            }
        }
        Err(last.unwrap_or_else(|| unpinned("", "no language was named")))
    }

    pub fn pin(&self, language: &str) -> Result<ImagePin, Diagnostic> {
        let language = language.trim().to_ascii_lowercase();
        let reference = self
            .by_language
            .get(&language)
            .ok_or_else(|| unpinned(&language, "nothing pins it"))?;
        let mut pin = ImagePin::parse(reference)
            .ok_or_else(|| unpinned(&language, &format!("`{reference}` names no digest")))?;
        if let Some(registry) = &self.registry {
            pin.image = rehost(&pin.image, registry);
        }
        Ok(pin)
    }
}

/// HOST-08's private registry: the image keeps its repository path and changes
/// host. A reference whose first segment is not a host (no dot, no colon, and
/// not `localhost`) has no host to replace, so the registry is prefixed.
fn rehost(image: &str, registry: &str) -> String {
    let registry = registry.trim_end_matches('/');
    match image.split_once('/') {
        Some((first, rest))
            if first.contains('.') || first.contains(':') || first == "localhost" =>
        {
            format!("{registry}/{rest}")
        }
        _ => format!("{registry}/{image}"),
    }
}

fn unpinned(language: &str, why: &str) -> Diagnostic {
    Diagnostic::new(
        code::E0610,
        format!("the runner image for `{language}` is not pinned to a digest: {why}"),
    )
    .help(
        "run `liyasa lock` to record the digest, or set `verify.runners.images` to `name@sha256:…`",
    )
}

#[cfg(test)]
mod tests;
