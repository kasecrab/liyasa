//! A mock server derived from a spec (API-32).
//!
//! A generated sample that compiles proves nothing: it has to send what the
//! operation says it sends. So the samples are executed against a server the
//! spec itself describes, and the request that arrives is compared with the
//! one the generator was given. A template that forgets a header, encodes a
//! query wrongly, or sends a form as JSON fails here rather than in a reader's
//! terminal.
//!
//! The server speaks enough HTTP/1.1 to answer a sample and no more. It is not
//! a fixture for anything a browser touches — the playground's e2e tests run
//! against the real reader runtime — and it never leaves loopback.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::example::{self, Fill, Side};
use crate::model::{Method, Spec};

/// One request the mock server answered.
#[derive(Debug, Clone, Default)]
pub struct Received {
    pub method: String,
    /// The target as it arrived, query string and all.
    pub target: String,
    /// The path with the query removed.
    pub path: String,
    /// Query parameters in arrival order, still percent-encoded.
    pub query: Vec<(String, String)>,
    /// Header names lowercased, because a client may send any case.
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    /// No route in the spec matched: the sample asked for something the
    /// operation does not describe.
    pub matched: bool,
}

impl Received {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_lowercase()).map(String::as_str)
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn query_value(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Debug, Clone)]
struct Route {
    method: Method,
    /// The path split on `/`, with `{name}` segments marked.
    segments: Vec<Segment>,
    status: u16,
    media_type: String,
    body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Literal(String),
    Parameter,
}

/// The routes one spec describes.
pub struct Mock {
    routes: Vec<Route>,
}

impl Mock {
    pub fn new(spec: &Spec) -> Self {
        let mut routes = Vec::new();
        for operation in spec.operations() {
            if operation.webhook {
                continue;
            }
            let (status, media_type, body) = response(&operation);
            routes.push(Route {
                method: operation.method,
                segments: split(operation.path),
                status,
                media_type,
                body,
            });
        }
        Self { routes }
    }

    /// Binds loopback on a port the operating system picks and answers until
    /// the returned handle is dropped.
    pub fn serve(self) -> std::io::Result<Server> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let received: Arc<Mutex<Vec<Received>>> = Arc::default();
        let stopping = Arc::new(AtomicBool::new(false));

        let log = Arc::clone(&received);
        let flag = Arc::clone(&stopping);
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                // One connection at a time: a sample is one request, and a
                // test that hangs is easier to read than one that races.
                if let Some(request) = answer(&self.routes, stream)
                    && let Ok(mut log) = log.lock()
                {
                    log.push(request);
                }
            }
        });

        Ok(Server {
            address,
            received,
            stopping,
            thread: Some(thread),
        })
    }
}

pub struct Server {
    address: SocketAddr,
    received: Arc<Mutex<Vec<Received>>>,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn base_url(&self) -> String {
        format!("http://{}", self.address)
    }

    pub fn received(&self) -> Vec<Received> {
        self.received
            .lock()
            .map(|log| log.clone())
            .unwrap_or_default()
    }

    /// The last request, which is what a test that sent one wants.
    pub fn last(&self) -> Option<Received> {
        self.received().pop()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        // The accept loop is blocked in `incoming`; one connection wakes it so
        // it can see the flag and leave.
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn split(path: &str) -> Vec<Segment> {
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            if segment.starts_with('{') && segment.ends_with('}') {
                Segment::Parameter
            } else {
                Segment::Literal(segment.to_owned())
            }
        })
        .collect()
}

fn response(operation: &crate::model::OperationRef<'_>) -> (u16, String, String) {
    let chosen = operation
        .operation
        .responses
        .iter()
        .find(|(status, _)| status.starts_with('2'))
        .or_else(|| operation.operation.responses.iter().next());
    let Some((status, response)) = chosen else {
        return (204, String::new(), String::new());
    };
    let code = status.parse().unwrap_or(200);
    let Some((media_type, media)) = response.preferred() else {
        return (code, String::new(), String::new());
    };
    let value = media
        .example
        .clone()
        .or_else(|| media.examples.values().find_map(|e| e.value.clone()))
        .or_else(|| {
            media
                .schema
                .as_ref()
                .map(|schema| example::of(schema, Side::Response, Fill::All))
        });
    let body = match value {
        Some(value) if media_type.contains("json") => {
            serde_json::to_string(&value).unwrap_or_default()
        }
        Some(value) => example::as_text(&value),
        None => String::new(),
    };
    (code, media_type.to_owned(), body)
}

fn matches(route: &Route, method: &str, path: &str) -> bool {
    if !route.method.as_str().eq_ignore_ascii_case(method) {
        return false;
    }
    let got = split(path);
    if got.len() != route.segments.len() {
        return false;
    }
    route
        .segments
        .iter()
        .zip(&got)
        .all(|(want, got)| match (want, got) {
            (Segment::Parameter, _) => true,
            (Segment::Literal(want), Segment::Literal(got)) => want == got,
            (Segment::Literal(want), Segment::Parameter) => want == "{}",
        })
}

fn answer(routes: &[Route], stream: TcpStream) -> Option<Received> {
    let peer = stream.try_clone().ok()?;
    let mut reader = BufReader::new(stream);

    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let target = parts.next()?.to_owned();

    let mut headers = BTreeMap::new();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.insert(name.trim().to_lowercase(), value.trim().to_owned());
        }
    }

    let mut writer = peer;
    if headers
        .get("expect")
        .is_some_and(|value| value.eq_ignore_ascii_case("100-continue"))
    {
        let _ = writer.write_all(b"HTTP/1.1 100 Continue\r\n\r\n");
        let _ = writer.flush();
    }

    let mut body = Vec::new();
    if let Some(length) = headers.get("content-length").and_then(|v| v.parse().ok()) {
        body.resize(length, 0);
        reader.read_exact(&mut body).ok()?;
    } else if headers
        .get("transfer-encoding")
        .is_some_and(|value| value.contains("chunked"))
    {
        read_chunked(&mut reader, &mut body)?;
    }

    let (path, query_string) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let query = query_string
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(name), decode(value))
        })
        .collect();

    let route = routes.iter().find(|route| matches(route, &method, path));
    let (status, media_type, payload) = match route {
        Some(route) => (
            route.status,
            route.media_type.clone(),
            route.body.clone().into_bytes(),
        ),
        None => (
            404,
            "application/json".to_owned(),
            format!(r#"{{"error":"no operation in the spec matches {method} {path}"}}"#)
                .into_bytes(),
        ),
    };

    let mut head = format!("HTTP/1.1 {status} {}\r\n", reason(status));
    if !media_type.is_empty() {
        head.push_str(&format!("Content-Type: {media_type}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n", payload.len()));
    head.push_str("Connection: close\r\n\r\n");
    let _ = writer.write_all(head.as_bytes());
    let _ = writer.write_all(&payload);
    let _ = writer.flush();
    let _ = writer.shutdown(Shutdown::Write);

    Some(Received {
        method,
        target: target.clone(),
        path: path.to_owned(),
        query,
        headers,
        body,
        matched: route.is_some(),
    })
}

fn read_chunked(reader: &mut BufReader<TcpStream>, body: &mut Vec<u8>) -> Option<()> {
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return Some(());
        }
        let size = usize::from_str_radix(line.trim().split(';').next()?.trim(), 16).ok()?;
        if size == 0 {
            let mut trailer = String::new();
            let _ = reader.read_line(&mut trailer);
            return Some(());
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..]).ok()?;
        let mut crlf = [0_u8; 2];
        reader.read_exact(&mut crlf).ok()?;
    }
}

fn decode(text: &str) -> String {
    let bytes = text.replace('+', " ").into_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Unknown",
    }
}
