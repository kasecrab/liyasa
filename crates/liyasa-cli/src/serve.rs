//! A static file server for `liyasa dev`.
//!
//! HTTP/1.1 over `std::net::TcpListener`, by hand. The alternative was axum,
//! which §6.2 puts in `liyasa-server` and which does not exist; a dev server
//! that serves a directory and answers one polling endpoint is a few hundred
//! lines, and adding a web framework to the CLI for it would be the unilateral
//! dependency move §31.6 forbids. `liyasa serve` (CLI-11) is a different
//! program with authentication, TLS, and a database, and is WP-14's.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Polled by the injected script; a change means the build moved on.
pub const BUILD_ENDPOINT: &str = "/_liyasa/dev-build";

/// How often the page asks. Short enough to feel immediate, long enough that
/// an idle tab is not a busy loop.
const POLL_MS: u32 = 500;

/// Injected before `</body>` of every HTML response.
fn reload_script(build: u64) -> String {
    format!(
        "<script>(function(){{var at={build};setInterval(function(){{\
fetch('{BUILD_ENDPOINT}',{{cache:'no-store'}}).then(function(r){{return r.text()}})\
.then(function(t){{if(+t!==at){{location.reload()}}}}).catch(function(){{}})}},{POLL_MS})}})();</script>"
    )
}

pub struct Server {
    listener: TcpListener,
    root: PathBuf,
    build: Arc<AtomicU64>,
}

impl Server {
    /// Binds, or gives back the error so the caller can report it as a
    /// diagnostic rather than panicking on a port already in use.
    pub fn bind(host: &str, port: u16, root: PathBuf) -> std::io::Result<Self> {
        Ok(Self {
            listener: TcpListener::bind((host, port))?,
            root,
            build: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.listener.local_addr()
    }

    /// The counter every open page is polling. Bump it after a rebuild.
    pub fn build_counter(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.build)
    }

    /// Serves until the process ends. One thread per connection: a dev server
    /// has one reader and a handful of asset requests per page.
    pub fn serve_forever(self) {
        for stream in self.listener.incoming() {
            let Ok(stream) = stream else { continue };
            let root = self.root.clone();
            let build = Arc::clone(&self.build);
            std::thread::spawn(move || {
                let _ = handle(stream, &root, &build);
            });
        }
    }
}

fn handle(mut stream: TcpStream, root: &Path, build: &AtomicU64) -> std::io::Result<()> {
    let target = match request_target(&mut stream)? {
        Some(target) => target,
        None => return Ok(()),
    };

    if target == BUILD_ENDPOINT {
        let body = build.load(Ordering::Relaxed).to_string();
        return respond(
            &mut stream,
            200,
            "text/plain; charset=utf-8",
            body.as_bytes(),
            true,
        );
    }

    match resolve(root, &target) {
        Some((path, media)) => {
            let bytes = std::fs::read(&path)?;
            if media.starts_with("text/html") {
                let html = inject(
                    &String::from_utf8_lossy(&bytes),
                    build.load(Ordering::Relaxed),
                );
                respond(&mut stream, 200, media, html.as_bytes(), true)
            } else {
                respond(&mut stream, 200, media, &bytes, true)
            }
        }
        None => {
            let custom = root.join("404.html");
            let (body, media) = match std::fs::read(&custom) {
                Ok(bytes) => (
                    inject(
                        &String::from_utf8_lossy(&bytes),
                        build.load(Ordering::Relaxed),
                    )
                    .into_bytes(),
                    "text/html; charset=utf-8",
                ),
                Err(_) => (
                    format!("404: no route for {target}\n").into_bytes(),
                    "text/plain; charset=utf-8",
                ),
            };
            respond(&mut stream, 404, media, &body, true)
        }
    }
}

/// The path from the request line, with the query string dropped and percent
/// escapes decoded. `None` for a request this server will not answer.
fn request_target(stream: &mut TcpStream) -> std::io::Result<Option<String>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }

    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    if !matches!(method, "GET" | "HEAD") {
        return Ok(None);
    }

    // Drain the headers so the client does not see a reset while writing.
    let mut header = String::new();
    while reader.read_line(&mut header)? > 2 {
        header.clear();
    }

    let path = target.split(['?', '#']).next().unwrap_or("/");
    Ok(Some(percent_decode(path)))
}

/// A file for this path, and what to call it.
///
/// `/guides/install` is `guides/install/index.html` the way a static host
/// serves it; `/guides/install.md` is the Markdown twin; anything else is
/// looked up literally.
pub fn resolve(root: &Path, target: &str) -> Option<(PathBuf, &'static str)> {
    // No `..` ever leaves the output directory.
    if target.split('/').any(|segment| segment == "..") {
        return None;
    }
    let trimmed = target.trim_start_matches('/');

    let candidates = if trimmed.is_empty() {
        vec![root.join("index.html")]
    } else if Path::new(trimmed).extension().is_some() {
        vec![root.join(trimmed)]
    } else {
        vec![
            root.join(trimmed).join("index.html"),
            root.join(format!("{trimmed}.html")),
            root.join(trimmed),
        ]
    };

    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(|path| {
            let media = media_type(&path);
            (path, media)
        })
}

pub fn media_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

/// Puts the reload script just before `</body>`, or at the end when the page
/// has no body close tag.
pub fn inject(html: &str, build: u64) -> String {
    let script = reload_script(build);
    match html.rfind("</body>") {
        Some(at) => format!("{}{script}{}", &html[..at], &html[at..]),
        None => format!("{html}{script}"),
    }
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    media: &str,
    body: &[u8],
    no_store: bool,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Error",
    };
    let cache = if no_store {
        "Cache-Control: no-store\r\n"
    } else {
        ""
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {media}\r\nContent-Length: {}\r\n{cache}Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Opens a browser, for `liyasa dev --open`. Best effort: a machine with no
/// browser is not an error.
pub fn open_browser(url: &str) {
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("open", &[])]
    } else if cfg!(target_os = "windows") {
        &[("cmd", &["/C", "start", ""])]
    } else {
        &[("xdg-open", &[]), ("gio", &["open"])]
    };
    for (program, prefix) in candidates {
        let ok = std::process::Command::new(program)
            .args(*prefix)
            .arg(url)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .is_ok();
        if ok {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("liyasa-serve-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a directory");
        root
    }

    fn write(root: &Path, relative: &str, body: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("a directory");
        }
        std::fs::write(path, body).expect("a file");
    }

    #[test]
    fn a_directory_route_serves_its_index() {
        let root = scratch("index");
        write(&root, "index.html", "<html></html>");
        write(&root, "guides/install/index.html", "<html>install</html>");

        assert_eq!(
            resolve(&root, "/").map(|(path, _)| path),
            Some(root.join("index.html"))
        );
        assert_eq!(
            resolve(&root, "/guides/install").map(|(path, _)| path),
            Some(root.join("guides/install/index.html"))
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_markdown_twin_is_served_as_markdown() {
        let root = scratch("markdown");
        write(&root, "guides/install.md", "# Install\n");
        let (_, media) = resolve(&root, "/guides/install.md").expect("the twin");
        assert_eq!(media, "text/markdown; charset=utf-8");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A dev server that serves the whole filesystem would be a hole even on
    /// localhost.
    #[test]
    fn a_traversal_is_refused() {
        let root = scratch("traversal");
        write(&root, "index.html", "<html></html>");
        assert!(resolve(&root, "/../../etc/passwd").is_none());
        assert!(resolve(&root, "/a/../../secret").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_route_resolves_to_nothing() {
        let root = scratch("missing");
        write(&root, "index.html", "<html></html>");
        assert!(resolve(&root, "/nope").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_reload_script_goes_inside_the_body() {
        let injected = inject("<html><body>hi</body></html>", 7);
        assert!(injected.contains("</body></html>"));
        assert!(injected.contains(BUILD_ENDPOINT));
        let at = injected.find("<script>").expect("the script");
        let body = injected.find("</body>").expect("the body close");
        assert!(at < body, "the script is not inside the body");
    }

    #[test]
    fn a_page_without_a_body_still_gets_the_script() {
        assert!(inject("<h1>hi</h1>", 1).contains(BUILD_ENDPOINT));
    }

    #[test]
    fn percent_escapes_are_decoded() {
        assert_eq!(percent_decode("/a%20b"), "/a b");
        assert_eq!(percent_decode("/plain"), "/plain");
        assert_eq!(percent_decode("/100%"), "/100%");
    }

    #[test]
    fn media_types_cover_what_a_site_serves() {
        for (name, expected) in [
            ("a.html", "text/html; charset=utf-8"),
            ("a.css", "text/css; charset=utf-8"),
            ("a.js", "text/javascript; charset=utf-8"),
            ("a.woff2", "font/woff2"),
            ("a.svg", "image/svg+xml"),
            ("a.unknown", "application/octet-stream"),
        ] {
            assert_eq!(media_type(Path::new(name)), expected, "{name}");
        }
    }
}
