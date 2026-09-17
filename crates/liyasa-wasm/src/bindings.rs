//! The two objects JavaScript sees.
//!
//! Thin by design: every one of these is a conversion, a call into
//! [`crate::session`] or [`crate::search`], and a conversion back. The
//! behaviour is in those modules, where the host's tests can reach it — the
//! gate builds this crate for the host, not for `wasm32`, so anything that
//! lives only here is code no test runs.
//!
//! Payloads cross as structured values through `serde-wasm-bindgen`, not as
//! JSON text: a keystroke serializes a whole Rendered AST and a round trip
//! through `JSON.parse` on top of that is a cost the preview budget does not
//! have. `ts/liyasa-wasm.d.ts` is the declaration for both sides.
//!
//! **ED-07 resolves paths by asking, not by calling back.** A response names
//! what the draft needs and the session does not hold; the host fetches it and
//! hands it over with `seed`. The alternative — a synchronous callback into
//! JavaScript — can only be backed by a synchronous `XMLHttpRequest` or by
//! `Atomics.wait` over a `SharedArrayBuffer`, which needs cross-origin
//! isolation. Neither is something the module should require of the page that
//! embeds it.

use std::collections::BTreeMap;

use liyasa_core::diagnostics::Diagnostics;
use wasm_bindgen::prelude::*;

use crate::api::{
    OpenRequest, ParseRequest, PreviewRequest, SearchRequest, SerializeRequest, ValidateRequest,
};
use liyasa_search::idx::writer::MANIFEST;

/// One editor session over one draft.
#[wasm_bindgen(js_name = Session)]
pub struct JsSession {
    inner: crate::session::Session,
}

#[wasm_bindgen(js_class = Session)]
impl JsSession {
    pub fn open(request: JsValue) -> Result<JsSession, JsValue> {
        let request: OpenRequest = from_js(request)?;
        let inner = crate::session::Session::sealed(&request).map_err(refused)?;
        Ok(Self { inner })
    }

    /// Hands over a path a response named in `missing` (ED-07).
    pub fn seed(&self, path: &str, text: &str) {
        self.inner.seed(path, text);
    }

    pub fn parse(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: ParseRequest = from_js(request)?;
        to_js(&self.inner.parse(&request))
    }

    pub fn preview(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: PreviewRequest = from_js(request)?;
        to_js(&self.inner.preview(&request))
    }

    pub fn validate(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: ValidateRequest = from_js(request)?;
        to_js(&self.inner.validate(&request))
    }

    pub fn serialize(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: SerializeRequest = from_js(request)?;
        to_js(&self.inner.serialize(&request))
    }

    pub fn status(&self) -> Result<JsValue, JsValue> {
        to_js(&self.inner.status())
    }
}

/// The browser search worker over `liyasa-idx` shard bytes.
#[wasm_bindgen(js_name = Searcher)]
pub struct JsSearcher {
    inner: crate::search::Searcher,
}

#[wasm_bindgen(js_class = Searcher)]
impl JsSearcher {
    /// `manifest.json` is what the worker fetches first; the shard files follow
    /// through `addFile`.
    pub fn open(manifest: Vec<u8>) -> Result<JsSearcher, JsValue> {
        let files = BTreeMap::from([(MANIFEST.to_owned(), manifest)]);
        let inner = crate::search::Searcher::open(files).map_err(refused)?;
        Ok(Self { inner })
    }

    #[wasm_bindgen(js_name = addFile)]
    pub fn add_file(&mut self, name: String, bytes: Vec<u8>) {
        self.inner.add_file(name, bytes);
    }

    pub fn search(&self, request: JsValue) -> Result<JsValue, JsValue> {
        let request: SearchRequest = from_js(request)?;
        to_js(&self.inner.search(&request))
    }
}

fn from_js<T: serde::de::DeserializeOwned>(value: JsValue) -> Result<T, JsValue> {
    serde_wasm_bindgen::from_value(value).map_err(JsValue::from)
}

fn to_js<T: serde::Serialize>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(JsValue::from)
}

/// A refusal reaches JavaScript as the diagnostics themselves, not as a string,
/// so the editor shows them the way it shows every other one.
fn refused(diagnostics: Diagnostics) -> JsValue {
    let joined = || {
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>()
            .join("; ")
    };
    serde_wasm_bindgen::to_value(diagnostics.as_slice())
        .unwrap_or_else(|_| JsValue::from_str(&joined()))
}
