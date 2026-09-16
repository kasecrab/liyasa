//! TLS termination (HOST-02).
//!
//! A certificate and key from disk, or one obtained from an ACME directory.
//! The provider is `ring`, installed once by `liyasa-net`, so the client and
//! the server share one (RFC 1401).

use std::io::BufRead;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpListener;
use tokio::sync::watch;

use super::AppState;

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("reading {0}: {1}")]
    Io(String, String),
    #[error("{0} holds no {1}")]
    Empty(String, &'static str),
    #[error("tls: {0}")]
    Rustls(String),
}

/// Minimal PEM reader: enough to load a certificate chain and a key without a
/// parsing crate, and strict about what it accepts.
fn pem_blocks(bytes: &[u8], label: &str) -> Vec<Vec<u8>> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in bytes.lines().map_while(Result::ok) {
        let line = line.trim().to_owned();
        if line == begin {
            current = Some(String::new());
        } else if line == end {
            if let Some(body) = current.take()
                && let Some(decoded) = base64_decode(&body)
            {
                out.push(decoded);
            }
        } else if let Some(body) = current.as_mut() {
            body.push_str(&line);
        }
    }
    out
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        if byte == b'=' || byte.is_ascii_whitespace() {
            continue;
        }
        let value = ALPHABET.iter().position(|c| *c == byte)? as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

/// Loads a certificate chain and its private key from PEM files.
pub fn load_config(cert: &Path, key: &Path) -> Result<rustls::ServerConfig, TlsError> {
    liyasa_net::client::install_crypto_provider();
    let cert_bytes =
        std::fs::read(cert).map_err(|e| TlsError::Io(cert.display().to_string(), e.to_string()))?;
    let key_bytes =
        std::fs::read(key).map_err(|e| TlsError::Io(key.display().to_string(), e.to_string()))?;

    let chain: Vec<CertificateDer<'static>> = pem_blocks(&cert_bytes, "CERTIFICATE")
        .into_iter()
        .map(CertificateDer::from)
        .collect();
    if chain.is_empty() {
        return Err(TlsError::Empty(cert.display().to_string(), "certificate"));
    }

    let key = ["PRIVATE KEY", "RSA PRIVATE KEY", "EC PRIVATE KEY"]
        .iter()
        .find_map(|label| {
            let block = pem_blocks(&key_bytes, label).into_iter().next()?;
            Some(match *label {
                "RSA PRIVATE KEY" => PrivateKeyDer::Pkcs1(block.into()),
                "EC PRIVATE KEY" => PrivateKeyDer::Sec1(block.into()),
                _ => PrivateKeyDer::Pkcs8(block.into()),
            })
        })
        .ok_or_else(|| TlsError::Empty(key.display().to_string(), "private key"))?;

    rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .map_err(|e| TlsError::Rustls(e.to_string()))
}

/// Accepts TLS connections until `shutdown` fires. axum's own `serve` takes a
/// plain listener, so the accept loop is here; the peer address is put into
/// the request the same way `into_make_service_with_connect_info` would.
pub async fn serve(
    listener: TcpListener,
    router: Router,
    config: Arc<rustls::ServerConfig>,
    state: Arc<AppState>,
    mut shutdown: watch::Receiver<bool>,
) -> std::io::Result<()> {
    use axum::extract::ConnectInfo;
    use tower::ServiceExt as _;

    let acceptor = tokio_rustls::TlsAcceptor::from(config);
    let mut in_flight = tokio::task::JoinSet::new();
    loop {
        let (stream, peer) = tokio::select! {
            accepted = listener.accept() => accepted?,
            _ = shutdown.changed() => break,
        };
        let acceptor = acceptor.clone();
        let router = router.clone();
        in_flight.spawn(async move {
            let Ok(stream) = acceptor.accept(stream).await else {
                // A failed handshake is a client problem, not a server one.
                return;
            };
            let service = hyper::service::service_fn(
                move |mut request: http::Request<hyper::body::Incoming>| {
                    request.extensions_mut().insert(ConnectInfo(peer));
                    router.clone().oneshot(request)
                },
            );
            let io = hyper_util::rt::TokioIo::new(stream);
            let _ =
                hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
                    .serve_connection_with_upgrades(io, service)
                    .await;
        });
    }
    state.begin_drain();
    // In-flight connections finish inside the drain window; anything still
    // running when it closes is dropped, which is what NFR-31 specifies.
    let _ = tokio::time::timeout(state.config.drain_timeout, async {
        while in_flight.join_next().await.is_some() {}
    })
    .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pem_block_round_trips_through_the_reader() {
        // "hello" in base64 is aGVsbG8=.
        let pem = b"-----BEGIN CERTIFICATE-----\naGVsbG8=\n-----END CERTIFICATE-----\n";
        assert_eq!(pem_blocks(pem, "CERTIFICATE"), vec![b"hello".to_vec()]);
        assert!(pem_blocks(pem, "PRIVATE KEY").is_empty());
    }

    #[test]
    fn several_certificates_make_a_chain_in_order() {
        let pem = b"-----BEGIN CERTIFICATE-----\nYQ==\n-----END CERTIFICATE-----\n\
                    -----BEGIN CERTIFICATE-----\nYg==\n-----END CERTIFICATE-----\n";
        assert_eq!(
            pem_blocks(pem, "CERTIFICATE"),
            vec![b"a".to_vec(), b"b".to_vec()]
        );
    }

    #[test]
    fn a_missing_file_names_itself_rather_than_panicking() {
        let error = load_config(
            Path::new("/nonexistent/cert.pem"),
            Path::new("/nonexistent/key.pem"),
        )
        .expect_err("no such file");
        assert!(error.to_string().contains("cert.pem"), "{error}");
    }

    #[test]
    fn base64_decodes_with_and_without_padding() {
        assert_eq!(base64_decode("aGVsbG8="), Some(b"hello".to_vec()));
        assert_eq!(base64_decode("aGVsbG8"), Some(b"hello".to_vec()));
        assert_eq!(base64_decode("YQ=="), Some(b"a".to_vec()));
        assert_eq!(base64_decode("!!!"), None);
    }
}
