//! The forms a link and an image reference can take (CM-35, CM-36).
//!
//! Resolving one needs the route table and the file system, and this crate has
//! neither. What it can do is say which of CM-36's forms an `href` is written
//! in, so the build reads the grammar off one list instead of rediscovering it,
//! and so `E0401` can say *which* kind of target went missing.

/// The forms CM-36 lists, plus the two every document has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// `[text](./other.md)` — relative to the page.
    Relative,
    /// `[text](/route)` — site-absolute.
    Route,
    /// `[text](page:id)` — by page identity, so it survives a rename.
    PageId,
    /// `[text](#anchor)` — a heading on this page.
    Anchor,
    /// `https:`, `mailto:`, and anything else with a scheme.
    External,
}

/// Which form an `href` is written in.
pub fn form_of(href: &str) -> Form {
    let href = href.trim();
    if href.starts_with('#') {
        return Form::Anchor;
    }
    // Protocol-relative is somebody else's host.
    if href.starts_with("//") {
        return Form::External;
    }
    if href.starts_with('/') {
        return Form::Route;
    }
    match scheme_of(href) {
        Some(scheme) if scheme == "page" => Form::PageId,
        Some(_) => Form::External,
        None => Form::Relative,
    }
}

/// The part of `page:id#anchor` or `./other.md#anchor` after the `#`.
pub fn anchor_of(href: &str) -> Option<&str> {
    let (_, anchor) = href.split_once('#')?;
    (!anchor.is_empty()).then_some(anchor)
}

/// The target of `page:id`, without its scheme or anchor.
pub fn page_id_of(href: &str) -> Option<&str> {
    let rest = href.trim().strip_prefix("page:")?;
    let id = rest.split('#').next().unwrap_or(rest);
    (!id.is_empty()).then_some(id)
}

/// The scheme of an absolute reference, lowercased.
///
/// A `:` that follows anything but scheme characters is part of a path, so
/// `./a:b.md` is relative and `page:install` is not.
fn scheme_of(href: &str) -> Option<String> {
    let mut scheme = String::new();
    for ch in href.chars() {
        match ch {
            ':' => return (!scheme.is_empty()).then_some(scheme),
            _ if ch.is_ascii_alphanumeric() || ch == '+' || ch == '-' || ch == '.' => {
                scheme.extend(ch.to_lowercase());
            }
            _ => return None,
        }
    }
    None
}

// ---- light and dark pairs (CM-35) ----

/// Extensions a dark variant is looked for beside.
pub const IMAGE_EXTENSIONS: &[&str] = &["avif", "gif", "jpeg", "jpg", "png", "svg", "webp"];

/// Infix that marks the dark half of a pair.
pub const DARK_INFIX: &str = ".dark";

/// The path `image.png`'s dark twin would have, or `None` when the reference is
/// not a local image or is already the dark half.
///
/// Whether the file is there is the build's question; this is only what to look
/// for, so that the convention lives in one place rather than in whichever pass
/// happens to need it.
pub fn dark_variant(src: &str) -> Option<String> {
    if form_of(src) == Form::External {
        return None;
    }
    let (stem, extension) = src.rsplit_once('.')?;
    let known = IMAGE_EXTENSIONS
        .iter()
        .any(|known| extension.eq_ignore_ascii_case(known));
    if !known || stem.is_empty() || is_dark(src) {
        return None;
    }
    Some(format!("{stem}{DARK_INFIX}.{extension}"))
}

/// Whether a reference is already the dark half of a pair.
pub fn is_dark(src: &str) -> bool {
    let Some((stem, _)) = src.rsplit_once('.') else {
        return false;
    };
    stem.to_ascii_lowercase().ends_with(DARK_INFIX)
}

#[cfg(test)]
mod tests;
