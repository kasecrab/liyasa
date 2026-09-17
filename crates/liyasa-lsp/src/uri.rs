//! `file:` URIs in, filesystem paths out, and back.
//!
//! The workspace is addressed by `VfsPath` everywhere inside Liyasa and by URI
//! everywhere on the wire, and the two disagree about percent-encoding, about
//! Windows drive letters, and about what an absolute path looks like. They
//! disagree in exactly one place: here.

use std::path::{Path, PathBuf};

/// The filesystem path a `file:` URI names, or `None` for any other scheme —
/// an editor can hold an `untitled:` buffer open, and that buffer has no path.
pub fn to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // `file:///a/b` has an empty authority; `file://host/a/b` names a host we
    // cannot read, so only the empty and `localhost` authorities are ours.
    let path = match rest.strip_prefix("localhost/") {
        Some(path) => format!("/{path}"),
        None if rest.starts_with('/') => rest.to_owned(),
        None => return None,
    };
    let decoded = percent_decode(&path);
    // `file:///C:/x` — the drive letter makes the leading slash spurious.
    let decoded = match decoded.as_bytes() {
        [b'/', drive, b':', ..] if drive.is_ascii_alphabetic() => decoded[1..].to_owned(),
        _ => decoded,
    };
    Some(PathBuf::from(decoded))
}

/// The `file:` URI for a path. Only the characters that would change how a URI
/// parses are encoded; a documentation tree full of ordinary names comes back
/// unchanged.
pub fn from_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => out.push(byte as char),
            b'/' | b'-' | b'_' | b'.' | b'~' | b':' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Decodes `%XX`. A `%` that is not followed by two hex digits is left as it
/// stands: it is a literal per cent in a file name, not a broken escape.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && let (Some(hi), Some(lo)) = (
                bytes.get(at + 1).and_then(|b| (*b as char).to_digit(16)),
                bytes.get(at + 2).and_then(|b| (*b as char).to_digit(16)),
            )
        {
            out.push(u8::try_from(hi * 16 + lo).unwrap_or(b'?'));
            at += 3;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
