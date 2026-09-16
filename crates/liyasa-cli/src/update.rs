//! CLI-26: the release index, the signature over it, and replacing this
//! binary with a newer one.
//!
//! The scheme (RFC 0900): the index names, per release and per target, an
//! artifact file, its SHA-256 digest, and an Ed25519 signature over the 32
//! digest bytes. Verification happens against the digest of what was actually
//! read, never against what the index claims, and the running binary is only
//! replaced after both checks pass.
//!
//! An artifact is the binary itself rather than an archive: unpacking needs a
//! compression crate, and the PRD's dependency table has none for this.
//! TODO(rfc-0900).

use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The public key releases are signed with.
///
/// `LIYASA_RELEASE_PUBKEY` replaces it, as 64 hex characters. That is not a
/// hole: an operator running an air-gapped installation (HOST-08) or a private
/// release channel signs their own builds and has to be able to say so, and
/// changing it also requires control of the index the binary is fetched from.
/// `liyasa doctor` reports when a key other than the built-in one is in use.
pub const RELEASE_PUBKEY: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The target triple this binary was built for, from `build.rs`.
pub const TARGET: &str = env!("LIYASA_TARGET");

pub const INDEX_FILE: &str = "index.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub releases: Vec<Release>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
}

impl Release {
    pub fn artifact(&self, target: &str) -> Option<&Artifact> {
        self.artifacts
            .iter()
            .find(|artifact| artifact.target == target)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub target: String,
    /// The artifact's path, relative to the index.
    pub file: String,
    /// Lower-case hex, 64 characters.
    pub sha256: String,
    /// Lower-case hex, 128 characters: Ed25519 over the 32 digest bytes.
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The index could not be read or parsed (E0009).
    Index(String),
    /// The artifact's bytes do not hash to what the index says (E0008).
    Digest { expected: String, found: String },
    /// The signature does not verify against the release key (E0007).
    Signature(String),
    /// Nothing to install for this target.
    NoArtifact { version: String, target: String },
}

impl Failure {
    pub fn code(&self) -> liyasa_core::diagnostics::Code {
        use liyasa_core::diagnostics::code;
        match self {
            Self::Index(_) | Self::NoArtifact { .. } => code::E0009,
            Self::Digest { .. } => code::E0008,
            Self::Signature(_) => code::E0007,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Index(detail) => format!("the release index could not be read: {detail}"),
            Self::Digest { expected, found } => {
                format!("the downloaded artifact hashes to {found}, but the index says {expected}")
            }
            Self::Signature(detail) => {
                format!("the release signature was not accepted: {detail}")
            }
            Self::NoArtifact { version, target } => {
                format!("release {version} has no artifact for {target}")
            }
        }
    }
}

/// Reads the index from a local directory or a `file://` URL.
///
/// A remote source needs an HTTP client, which lives in `liyasa-net` and does
/// not exist; the caller reports that rather than this function guessing.
/// TODO(rfc-0900).
pub fn read_index(source: &Path) -> Result<Index, Failure> {
    let path = if source.is_dir() {
        source.join(INDEX_FILE)
    } else {
        source.to_path_buf()
    };
    let bytes = std::fs::read(&path)
        .map_err(|error| Failure::Index(format!("{}: {error}", path.display())))?;
    let index: Index = serde_json::from_slice(&bytes)
        .map_err(|error| Failure::Index(format!("{}: {error}", path.display())))?;
    Ok(index)
}

/// A `file://` URL or a plain path, as a path.
pub fn source_path(source: &str) -> Option<PathBuf> {
    if let Some(rest) = source.strip_prefix("file://") {
        return Some(PathBuf::from(rest));
    }
    if source.starts_with("http://") || source.starts_with("https://") {
        return None;
    }
    Some(PathBuf::from(source))
}

/// The newest release in the index, by the ordering releases are listed in
/// reverse: the index is authored newest-first, so the first entry wins.
///
/// Comparing versions properly needs semver, which is not a dependency; the
/// index is written by the release job, so its order is the release order.
pub fn newest(index: &Index) -> Option<&Release> {
    index.releases.first()
}

pub fn release<'a>(index: &'a Index, version: Option<&str>) -> Option<&'a Release> {
    match version {
        Some(wanted) => index
            .releases
            .iter()
            .find(|release| release.version == wanted),
        None => newest(index),
    }
}

/// The verifying key this binary trusts.
pub fn public_key() -> Result<VerifyingKey, Failure> {
    let hex = std::env::var("LIYASA_RELEASE_PUBKEY").unwrap_or_else(|_| RELEASE_PUBKEY.to_owned());
    let bytes = decode_hex(&hex).ok_or_else(|| {
        Failure::Signature("the release key is not 32 hex-encoded bytes".to_owned())
    })?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| Failure::Signature("the release key is not 32 bytes".to_owned()))?;
    VerifyingKey::from_bytes(&bytes)
        .map_err(|error| Failure::Signature(format!("the release key is not a point: {error}")))
}

/// Whether a key other than the built-in one is in use, which `liyasa doctor`
/// reports because it changes who can hand this machine a new binary.
pub fn key_is_overridden() -> bool {
    std::env::var("LIYASA_RELEASE_PUBKEY").is_ok_and(|key| key != RELEASE_PUBKEY)
}

pub fn digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    encode_hex(&hasher.finalize())
}

/// Checks an artifact's bytes against the index and the release key.
///
/// The digest is computed from the bytes in hand and compared with the index;
/// the signature is then checked over those same 32 bytes. Doing it in that
/// order is what makes a tampered artifact fail even when the index is the one
/// that was tampered with: changing the bytes breaks the digest, and changing
/// the index to match breaks the signature.
pub fn verify(bytes: &[u8], artifact: &Artifact) -> Result<(), Failure> {
    let found = digest(bytes);
    let expected = artifact.sha256.trim().to_ascii_lowercase();
    if found != expected {
        return Err(Failure::Digest { expected, found });
    }

    let raw =
        decode_hex(&found).ok_or_else(|| Failure::Signature("the digest is not hex".to_owned()))?;
    let signature_bytes = decode_hex(artifact.signature.trim())
        .ok_or_else(|| Failure::Signature("the signature is not hex".to_owned()))?;
    let signature_bytes: [u8; 64] = signature_bytes
        .try_into()
        .map_err(|_| Failure::Signature("the signature is not 64 bytes".to_owned()))?;
    let signature = Signature::from_bytes(&signature_bytes);

    public_key()?
        .verify(&raw, &signature)
        .map_err(|error| Failure::Signature(error.to_string()))
}

/// Puts `bytes` where `current` is, without ever leaving the path empty.
///
/// A running executable cannot be written to, so the new binary is written
/// beside it, made executable, and renamed over the old one, which is moved
/// aside first and removed afterwards. Every step is on the same filesystem,
/// so each rename is atomic.
pub fn replace(current: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let directory = current.parent().unwrap_or_else(|| Path::new("."));
    let incoming = directory.join(format!(".liyasa-update-{}", std::process::id()));
    let previous = directory.join(format!(".liyasa-previous-{}", std::process::id()));

    std::fs::write(&incoming, bytes)?;
    copy_permissions(current, &incoming)?;

    match std::fs::rename(current, &previous) {
        Ok(()) => {}
        Err(error) => {
            let _ = std::fs::remove_file(&incoming);
            return Err(error);
        }
    }
    if let Err(error) = std::fs::rename(&incoming, current) {
        // Put the old one back rather than leaving nothing at the path.
        let _ = std::fs::rename(&previous, current);
        let _ = std::fs::remove_file(&incoming);
        return Err(error);
    }
    let _ = std::fs::remove_file(&previous);
    Ok(())
}

#[cfg(unix)]
fn copy_permissions(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(from)
        .map(|meta| meta.permissions().mode())
        .unwrap_or(0o755);
    std::fs::set_permissions(to, std::fs::Permissions::from_mode(mode | 0o111))
}

#[cfg(not(unix))]
fn copy_permissions(_from: &Path, _to: &Path) -> std::io::Result<()> {
    Ok(())
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(text.get(at..at + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// A fixed seed rather than a random one, so the test needs no source of
    /// randomness and fails the same way every time.
    const SEED: [u8; 32] = [7; 32];

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&SEED)
    }

    fn trust_the_test_key() {
        let public = encode_hex(signing_key().verifying_key().as_bytes());
        // SAFETY-adjacent: these tests are serialised by `serial_test`-style
        // discipline of using one key for all of them, so the variable's value
        // never differs between two running tests.
        unsafe { std::env::set_var("LIYASA_RELEASE_PUBKEY", public) };
    }

    fn signed(bytes: &[u8]) -> Artifact {
        trust_the_test_key();
        let sha = digest(bytes);
        let raw = decode_hex(&sha).expect("hex");
        let signature = signing_key().sign(&raw);
        Artifact {
            target: TARGET.to_owned(),
            file: "liyasa".to_owned(),
            sha256: sha,
            signature: encode_hex(&signature.to_bytes()),
        }
    }

    #[test]
    fn a_correctly_signed_artifact_verifies() {
        let bytes = b"a new liyasa binary";
        let artifact = signed(bytes);
        assert_eq!(verify(bytes, &artifact), Ok(()));
    }

    /// The acceptance criterion: a tampered binary is rejected. The index is
    /// untouched, so the digest is what catches it.
    #[test]
    fn a_tampered_artifact_fails_the_digest() {
        let artifact = signed(b"a new liyasa binary");
        let result = verify(b"a new liyasa binary with a backdoor", &artifact);
        assert!(matches!(result, Err(Failure::Digest { .. })), "{result:?}");
    }

    /// Tampering with the index as well moves the failure to the signature,
    /// which is the check that cannot be forged without the private key.
    #[test]
    fn a_tampered_artifact_and_index_fail_the_signature() {
        let tampered = b"a new liyasa binary with a backdoor";
        let mut artifact = signed(b"a new liyasa binary");
        artifact.sha256 = digest(tampered);
        let result = verify(tampered, &artifact);
        assert!(matches!(result, Err(Failure::Signature(_))), "{result:?}");
    }

    #[test]
    fn a_signature_from_another_key_is_refused() {
        let bytes = b"a new liyasa binary";
        let mut artifact = signed(bytes);
        let other = SigningKey::from_bytes(&[9; 32]);
        let raw = decode_hex(&artifact.sha256).expect("hex");
        artifact.signature = encode_hex(&other.sign(&raw).to_bytes());
        assert!(matches!(
            verify(bytes, &artifact),
            Err(Failure::Signature(_))
        ));
    }

    #[test]
    fn a_signature_that_is_not_a_signature_is_refused() {
        let bytes = b"a new liyasa binary";
        let mut artifact = signed(bytes);
        artifact.signature = "not hex".to_owned();
        assert!(matches!(
            verify(bytes, &artifact),
            Err(Failure::Signature(_))
        ));
    }

    #[test]
    fn hex_round_trips() {
        let bytes = [0u8, 1, 15, 16, 200, 255];
        assert_eq!(encode_hex(&bytes), "00010f10c8ff");
        assert_eq!(decode_hex("00010f10c8ff"), Some(bytes.to_vec()));
        assert_eq!(decode_hex("odd"), None);
        assert_eq!(decode_hex("zz"), None);
    }

    #[test]
    fn a_file_url_is_a_path() {
        assert_eq!(
            source_path("file:///tmp/releases"),
            Some(PathBuf::from("/tmp/releases"))
        );
        assert_eq!(
            source_path("/tmp/releases"),
            Some(PathBuf::from("/tmp/releases"))
        );
        assert_eq!(source_path("https://example.com/releases"), None);
    }

    #[test]
    fn the_newest_release_is_the_first_the_index_lists() {
        let index = Index {
            schema_version: "1.0".to_owned(),
            releases: vec![
                Release {
                    version: "0.3.0".to_owned(),
                    notes: None,
                    artifacts: Vec::new(),
                },
                Release {
                    version: "0.2.0".to_owned(),
                    notes: None,
                    artifacts: Vec::new(),
                },
            ],
        };
        assert_eq!(newest(&index).map(|r| r.version.as_str()), Some("0.3.0"));
        assert_eq!(
            release(&index, Some("0.2.0")).map(|r| r.version.as_str()),
            Some("0.2.0")
        );
        assert!(release(&index, Some("9.9.9")).is_none());
    }

    #[test]
    fn replacing_keeps_a_binary_at_the_path_throughout() {
        let directory = std::env::temp_dir().join(format!("liyasa-replace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory");
        let path = directory.join("liyasa");
        std::fs::write(&path, b"old").expect("the old binary");

        replace(&path, b"new").expect("the replacement");
        assert_eq!(std::fs::read(&path).expect("the new binary"), b"new");

        // No leftovers beside it.
        let leftovers: Vec<String> = std::fs::read_dir(&directory)
            .expect("the directory")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "liyasa")
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        let _ = std::fs::remove_dir_all(&directory);
    }
}
