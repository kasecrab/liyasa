//! URL schemes in sanitized HTML (CM-32).
//!
//! `javascript:` executes and `data:` can carry a document, so both are
//! rejected — except a `data:image/*` URL, which is a picture and nothing else.

/// Schemes that may appear in a sanitized attribute.
pub const SCHEMES: &[&str] = &[
    "http", "https", "mailto", "tel", "ftp", "ftps", "sms", "irc", "ircs", "magnet", "news", "xmpp",
];

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
pub fn allowed(url: &str, image: bool) -> bool {
    let Some(scheme) = scheme_of(url) else {
        // Relative, root-relative, protocol-relative, or a bare fragment.
        return true;
    };
    if scheme == "data" {
        return image && is_image_data(url);
    }
    SCHEMES.contains(&scheme.as_str())
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
