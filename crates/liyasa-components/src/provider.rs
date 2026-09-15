//! The embed allow list (CMP-51, CMP-53).
//!
//! An embed is a plain `<iframe>`, so every provider needs a `frame-src` entry
//! in the page's CSP and nothing else: Liyasa never asks a host for
//! cross-origin isolation headers, which is what keeps embeds working on every
//! host.

/// One allow-listed provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Provider {
    pub name: &'static str,
    /// The origin the CSP must allow in `frame-src`.
    pub frame_src: &'static str,
    /// Whether the embed keeps the page's aspect ratio like a video.
    pub video: bool,
}

const YOUTUBE: Provider = Provider {
    name: "youtube",
    frame_src: "https://www.youtube-nocookie.com",
    video: true,
};
const VIMEO: Provider = Provider {
    name: "vimeo",
    frame_src: "https://player.vimeo.com",
    video: true,
};
const LOOM: Provider = Provider {
    name: "loom",
    frame_src: "https://www.loom.com",
    video: true,
};
const FIGMA: Provider = Provider {
    name: "figma",
    frame_src: "https://www.figma.com",
    video: false,
};
const CODESANDBOX: Provider = Provider {
    name: "codesandbox",
    frame_src: "https://codesandbox.io",
    video: false,
};
const STACKBLITZ: Provider = Provider {
    name: "stackblitz",
    frame_src: "https://stackblitz.com",
    video: false,
};
const EXCALIDRAW: Provider = Provider {
    name: "excalidraw",
    frame_src: "https://excalidraw.com",
    video: false,
};
const GIST: Provider = Provider {
    name: "gist",
    frame_src: "https://gist.github.com",
    video: false,
};

/// Every provider an `embed` may name.
pub const ALLOWED: &[Provider] = &[
    YOUTUBE,
    VIMEO,
    LOOM,
    FIGMA,
    CODESANDBOX,
    STACKBLITZ,
    EXCALIDRAW,
    GIST,
];

/// A provider and the URL to put in the `src`, or `None` when the URL names no
/// allow-listed provider.
pub fn resolve(url: &str) -> Option<(Provider, String)> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    match host.as_str() {
        "youtube.com" | "m.youtube.com" => {
            let id = query_value(path, "v")?;
            Some((YOUTUBE, format!("{}/embed/{id}", YOUTUBE.frame_src)))
        }
        "youtu.be" => {
            let id = first_segment(path)?;
            Some((YOUTUBE, format!("{}/embed/{id}", YOUTUBE.frame_src)))
        }
        "youtube-nocookie.com" => Some((YOUTUBE, url.to_owned())),
        "vimeo.com" => {
            let id = first_segment(path)?;
            Some((VIMEO, format!("{}/video/{id}", VIMEO.frame_src)))
        }
        "player.vimeo.com" => Some((VIMEO, url.to_owned())),
        "loom.com" => {
            let id = path.strip_prefix("share/").and_then(first_segment)?;
            Some((LOOM, format!("{}/embed/{id}", LOOM.frame_src)))
        }
        "figma.com" => Some((
            FIGMA,
            format!(
                "{}/embed?embed_host=liyasa&url={}",
                FIGMA.frame_src,
                encode(url)
            ),
        )),
        "codesandbox.io" => {
            let id = path.strip_prefix("s/").and_then(first_segment)?;
            Some((CODESANDBOX, format!("{}/embed/{id}", CODESANDBOX.frame_src)))
        }
        "stackblitz.com" => {
            let id = path.strip_prefix("edit/").and_then(first_segment)?;
            Some((
                STACKBLITZ,
                format!("{}/edit/{id}?embed=1", STACKBLITZ.frame_src),
            ))
        }
        "excalidraw.com" => Some((EXCALIDRAW, url.to_owned())),
        "gist.github.com" => {
            let id = path.split('?').next().unwrap_or(path).trim_end_matches('/');
            (!id.is_empty()).then(|| (GIST, format!("{}/{id}.pibb", GIST.frame_src)))
        }
        _ => None,
    }
}

/// Whether a URL is a video an `image`-like component should frame rather than
/// download.
pub fn is_video_url(url: &str) -> bool {
    resolve(url).is_some_and(|(provider, _)| provider.video)
}

fn first_segment(path: &str) -> Option<&str> {
    let segment = path.split(['/', '?', '#']).find(|part| !part.is_empty())?;
    (!segment.is_empty()).then_some(segment)
}

fn query_value<'a>(path: &'a str, key: &str) -> Option<&'a str> {
    let query = path.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then_some(value)
    })
}

/// Percent-encodes what a query parameter cannot carry literally.
fn encode(url: &str) -> String {
    let mut out = String::with_capacity(url.len() + 8);
    for byte in url.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_youtube_watch_url_becomes_a_nocookie_embed() {
        let (provider, src) =
            resolve("https://www.youtube.com/watch?v=dQw4w9WgXcQ").expect("known");
        assert_eq!(provider.name, "youtube");
        assert_eq!(src, "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ");
    }

    #[test]
    fn a_short_youtube_url_resolves_too() {
        let (_, src) = resolve("https://youtu.be/dQw4w9WgXcQ?t=30").expect("known");
        assert_eq!(src, "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ");
    }

    #[test]
    fn loom_and_vimeo_resolve() {
        assert_eq!(
            resolve("https://www.loom.com/share/abc123").map(|(_, src)| src),
            Some("https://www.loom.com/embed/abc123".to_owned())
        );
        assert_eq!(
            resolve("https://vimeo.com/76979871").map(|(_, src)| src),
            Some("https://player.vimeo.com/video/76979871".to_owned())
        );
    }

    #[test]
    fn a_figma_url_is_carried_as_a_parameter() {
        let (_, src) = resolve("https://www.figma.com/design/abc/Title").expect("known");
        assert!(src.starts_with("https://www.figma.com/embed?embed_host=liyasa&url="));
        assert!(src.contains("%3A%2F%2F"), "{src}");
    }

    #[test]
    fn a_gist_becomes_its_embeddable_form() {
        assert_eq!(
            resolve("https://gist.github.com/kasecrab/abc123").map(|(_, src)| src),
            Some("https://gist.github.com/kasecrab/abc123.pibb".to_owned())
        );
    }

    #[test]
    fn an_unlisted_host_resolves_to_nothing() {
        assert!(resolve("https://evil.example.com/embed/1").is_none());
        assert!(resolve("/local/video.mp4").is_none());
        assert!(resolve("javascript:alert(1)").is_none());
    }

    #[test]
    fn a_provider_url_with_no_id_is_rejected() {
        assert!(resolve("https://www.youtube.com/").is_none());
        assert!(resolve("https://www.loom.com/share/").is_none());
    }

    #[test]
    fn only_the_video_providers_keep_an_aspect_ratio() {
        assert!(is_video_url("https://vimeo.com/1"));
        assert!(!is_video_url("https://excalidraw.com/#json=1,2"));
    }
}
