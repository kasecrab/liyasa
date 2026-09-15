//! URL schemes in sanitized HTML (CM-32).
//!
//! `javascript:` executes and `data:` can carry a document, so both are
//! rejected — except a `data:image/*` URL, which is a picture and nothing else.

/// Schemes that may appear in a sanitized attribute.
pub const SCHEMES: &[&str] = &[
    "http", "https", "mailto", "tel", "ftp", "ftps", "sms", "irc", "ircs", "magnet", "news", "xmpp",
];

/// Schemes the build resolves away before any HTML is written (CM-36).
///
/// `page:` is the link form that survives a rename. No browser knows it, so it
/// is not in [`SCHEMES`]: it is allowed only by [`link_allowed`], on a Markdown
/// link, which is the one destination `liyasa_build::links` rewrites to a
/// route. An unresolved one is `E0401`, an error, so it does not reach a
/// reader in a build that succeeds.
pub const INTERNAL_SCHEMES: &[&str] = &["page"];

/// Image types a `data:` URL may declare.
pub const DATA_IMAGE_TYPES: &[&str] = &[
    "image/apng",
    "image/avif",
    "image/gif",
    "image/jpeg",
    "image/png",
    "image/svg+xml",
    "image/webp",
];

/// Whether a URL may be kept. `image` relaxes only the `data:` rule.
///
/// The check runs over the URL a browser would see, with character references
/// resolved, because `java&#9;script:x` navigates exactly like `javascript:x`.
pub fn allowed(url: &str, image: bool) -> bool {
    let decoded = super::html::decode_refs(url);
    let url = decoded.as_str();
    let Some(scheme) = scheme_of(url) else {
        // Relative, root-relative, protocol-relative, or a bare fragment.
        return true;
    };
    if scheme == "data" {
        return image && is_image_data(url);
    }
    SCHEMES.contains(&scheme.as_str())
}

/// Whether a Markdown link's destination may be kept.
///
/// Wider than [`allowed`] by exactly [`INTERNAL_SCHEMES`].
pub fn link_allowed(url: &str) -> bool {
    if allowed(url, false) {
        return true;
    }
    let decoded = super::html::decode_refs(url);
    scheme_of(&decoded).is_some_and(|scheme| INTERNAL_SCHEMES.contains(&scheme.as_str()))
}

/// The scheme of an absolute URL, lowercased, ignoring the whitespace and
/// control characters a browser strips before parsing.
fn scheme_of(url: &str) -> Option<String> {
    let mut scheme = String::new();
    for ch in url.chars() {
        match ch {
            ':' => {
                return (!scheme.is_empty()).then_some(scheme);
            }
            // A browser drops these before it looks at the scheme, so
            // `java\tscript:` is `javascript:` and must be treated as one.
            _ if ch.is_whitespace() || ch.is_control() => {}
            _ if ch.is_ascii_alphanumeric() || ch == '+' || ch == '-' || ch == '.' => {
                scheme.extend(ch.to_lowercase());
            }
            // Anything else means there was no scheme: `/a:b`, `?x=:`, `#a:b`.
            _ => return None,
        }
    }
    None
}

fn is_image_data(url: &str) -> bool {
    let Some(rest) = url.split_once(':').map(|(_, rest)| rest) else {
        return false;
    };
    let media = rest.split([';', ',']).next().unwrap_or_default();
    let media = media.trim().to_ascii_lowercase();
    DATA_IMAGE_TYPES.contains(&media.as_str())
}

#[cfg(test)]
mod tests;
