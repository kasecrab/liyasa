//! Loading one spec from bytes (API-01).
//!
//! Parse, decide the dialect, rewrite it into 3.1, read the model. Fetching
//! the bytes — a file through [`Vfs`](liyasa_core::Vfs), a URL through
//! [`HttpClient`](liyasa_core::HttpClient) — is [`crate::source`]'s job, so
//! this function is the whole pipeline for a document already in hand.

use liyasa_core::diagnostics::Diagnostics;

use crate::model::Spec;
use crate::read::Documents;
use crate::source::{Fetcher, Location};
use crate::{SpecError, normalize, read, tree, version};

#[derive(Debug)]
pub struct Loaded {
    pub spec: Spec,
    pub diagnostics: Diagnostics,
}

/// Loads a document that needs no reference outside itself.
pub fn from_bytes(id: &str, origin: &str, bytes: &[u8]) -> Result<Loaded, SpecError> {
    from_documents(id, origin, bytes, Documents::default)
}

/// Loads a document whose `$ref`s reach other documents, which `documents`
/// has already fetched. The root is inserted over whatever it holds.
pub fn from_documents(
    id: &str,
    origin: &str,
    bytes: &[u8],
    documents: impl FnOnce() -> Documents,
) -> Result<Loaded, SpecError> {
    let mut root = tree::parse(bytes, origin)?;
    let dialect = version::detect(&root)?;
    let mut diagnostics = normalize::to_3_1(&mut root, &dialect);

    let mut docs = documents();
    docs.insert(docs.root_key().to_owned(), root);
    let mut reader = read::Reader::new(&docs);
    let spec = reader.spec(id, dialect);
    diagnostics.extend(reader.into_diagnostics());
    Ok(Loaded { spec, diagnostics })
}

/// Loads a spec from wherever it is written, following every `$ref` that
/// leaves the document (API-01, API-02).
pub async fn from_source(
    id: &str,
    at: &Location,
    fetcher: &Fetcher<'_>,
) -> Result<Loaded, SpecError> {
    let bytes = fetcher.bytes(at).await?;
    let mut root = tree::parse(&bytes, &at.key())?;
    let dialect = version::detect(&root)?;
    let mut diagnostics = normalize::to_3_1(&mut root, &dialect);

    let (docs, fetched) = fetcher.documents(at, root).await;
    diagnostics.extend(fetched);
    let mut reader = read::Reader::new(&docs);
    let spec = reader.spec(id, dialect);
    diagnostics.extend(reader.into_diagnostics());
    Ok(Loaded { spec, diagnostics })
}
