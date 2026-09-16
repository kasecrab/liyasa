//! A loopback HTTP/1.1 server with the responses the client tests need.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub struct Server {
    pub port: u16,
    pub connections: Arc<AtomicUsize>,
}

impl Server {
    pub fn url(&self, path: &str) -> url::Url {
        format!("http://127.0.0.1:{}{}", self.port, path)
            .parse()
            .expect("a url")
    }
}

pub async fn start() -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let port = listener.local_addr().expect("an address").port();
    let connections = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&connections);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            counter.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let mut read = 0;
                loop {
                    let n = match stream.read(&mut buf[read..]).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    read += n;
                    if buf[..read].windows(4).any(|w| w == b"\r\n\r\n") || read == buf.len() {
                        break;
                    }
                }
                let head = String::from_utf8_lossy(&buf[..read]).into_owned();
                let mut lines = head.lines();
                let request = lines.next().unwrap_or_default().to_owned();
                let mut parts = request.split(' ');
                let method = parts.next().unwrap_or_default().to_owned();
                let path = parts.next().unwrap_or_default().to_owned();
                let (status, headers, body): (u16, Vec<(String, String)>, Vec<u8>) =
                    match path.as_str() {
                        "/ok" => (200, vec![], b"hello".to_vec()),
                        "/echo-method" => (200, vec![], method.into_bytes()),
                        "/big" => (200, vec![], vec![b'x'; 4096]),
                        "/big-no-length" => (200, vec![], vec![b'y'; 4096]),
                        "/slow" => {
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                            (200, vec![], b"late".to_vec())
                        }
                        "/away" => (
                            302,
                            vec![("Location".to_owned(), "http://example.invalid/".to_owned())],
                            vec![],
                        ),
                        "/private" => (
                            302,
                            vec![("Location".to_owned(), "http://10.0.0.1/".to_owned())],
                            vec![],
                        ),
                        "/see-other" => (
                            303,
                            vec![("Location".to_owned(), "/echo-method".to_owned())],
                            vec![],
                        ),
                        "/keep-method" => (
                            307,
                            vec![("Location".to_owned(), "/echo-method".to_owned())],
                            vec![],
                        ),
                        p if p.starts_with("/redirect/") => {
                            let n: u32 = p["/redirect/".len()..].parse().unwrap_or(0);
                            let target = if n <= 1 {
                                "/ok".to_owned()
                            } else {
                                format!("/redirect/{}", n - 1)
                            };
                            (302, vec![("Location".to_owned(), target)], vec![])
                        }
                        _ => (404, vec![], b"nope".to_vec()),
                    };
                let mut out = format!("HTTP/1.1 {status} X\r\n");
                if path != "/big-no-length" {
                    out.push_str(&format!("Content-Length: {}\r\n", body.len()));
                }
                for (k, v) in headers {
                    out.push_str(&format!("{k}: {v}\r\n"));
                }
                out.push_str("Connection: close\r\n\r\n");
                let _ = stream.write_all(out.as_bytes()).await;
                let _ = stream.write_all(&body).await;
                let _ = stream.shutdown().await;
            });
        }
    });
    Server { port, connections }
}
