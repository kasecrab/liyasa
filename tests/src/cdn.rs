//! A CDN in front of the server, for AUTH-13.
//!
//! AUTH-13 asks for the variant key to be proven complete "against the
//! build-time set, the server LRU, and a CDN-emulating layer (a caching proxy
//! in front of the server that honours `Cache-Control`, `Vary`, and
//! purge-by-tag)". A real CDN is not available to a test, and the properties
//! being tested are not a vendor's: they are that a shared cache keys on what
//! it was told to vary on, never stores a `private` response, and drops a tag
//! when the tag is purged.
//!
//! So this is that cache, written to the rules a shared HTTP cache follows,
//! and deliberately naive: it caches exactly what the headers permit and
//! nothing more. A bug in the key shows up here as one reader being served
//! another reader's body, which is the thing AUTH-13 exists to prevent.

use std::collections::BTreeMap;
use std::sync::Mutex;

/// What the origin answered, as a cache sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub body: String,
    pub cache_control: String,
    /// Request header names whose values select between stored responses.
    pub vary: Vec<String>,
    /// Surrogate keys a purge names.
    pub tags: Vec<String>,
}

impl Response {
    pub fn new(body: impl Into<String>, cache_control: &str) -> Self {
        Self {
            body: body.into(),
            cache_control: cache_control.to_owned(),
            vary: Vec::new(),
            tags: Vec::new(),
        }
    }

    pub fn vary(mut self, names: &[&str]) -> Self {
        self.vary = names.iter().map(|n| (*n).to_owned()).collect();
        self
    }

    pub fn tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_owned());
        self
    }

    /// Whether a *shared* cache may store this at all. `private` and
    /// `no-store` both forbid it, which is the rule AUTH-13's purge clause
    /// depends on.
    pub fn storable_in_shared_cache(&self) -> bool {
        let directives = self.cache_control.to_ascii_lowercase();
        !directives
            .split(',')
            .map(str::trim)
            .any(|d| d == "private" || d == "no-store")
    }
}

/// One request as the cache keys it: the URL plus the headers the stored
/// response said to vary on.
#[derive(Debug, Clone, Default)]
pub struct Request {
    pub url: String,
    pub headers: BTreeMap<String, String>,
}

impl Request {
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_owned(),
            headers: BTreeMap::new(),
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers
            .insert(name.to_ascii_lowercase(), value.to_owned());
        self
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Hit {
    #[default]
    Miss,
    Hit,
    /// The origin answered something a shared cache may not store.
    Uncacheable,
}

#[derive(Debug, Default)]
pub struct Cdn {
    stored: Mutex<BTreeMap<String, Response>>,
    /// Per URL, the `Vary` of whatever is stored there: a cache cannot know
    /// which headers matter until an origin has told it once.
    vary: Mutex<BTreeMap<String, Vec<String>>>,
    last: Mutex<Hit>,
}

impl Cdn {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fetches through the cache. `origin` is called on a miss.
    pub fn fetch<F>(&self, request: &Request, origin: F) -> Response
    where
        F: FnOnce(&Request) -> Response,
    {
        if let Some(stored) = self.lookup(request) {
            *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Hit::Hit;
            return stored;
        }
        let response = origin(request);
        let storable = response.storable_in_shared_cache();
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = match storable {
            true => Hit::Miss,
            false => Hit::Uncacheable,
        };
        if storable {
            self.vary
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(request.url.clone(), response.vary.clone());
            let key = key_for(request, &response.vary);
            self.stored
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(key, response.clone());
        }
        response
    }

    pub fn last(&self) -> Hit {
        *self.last.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A deploy purge: every stored response carrying the tag goes.
    pub fn purge_tag(&self, tag: &str) -> usize {
        let mut stored = self.stored.lock().unwrap_or_else(|e| e.into_inner());
        let before = stored.len();
        stored.retain(|_, response| !response.tags.iter().any(|t| t == tag));
        before - stored.len()
    }

    pub fn len(&self) -> usize {
        self.stored.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lookup(&self, request: &Request) -> Option<Response> {
        let vary = self
            .vary
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&request.url)
            .cloned()?;
        self.stored
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key_for(request, &vary))
            .cloned()
    }
}

/// The URL plus, for each `Vary` name in order, that request header's value.
/// Length-prefixed for the same reason the server's own key is: a header value
/// must not be able to spell the next field.
fn key_for(request: &Request, vary: &[String]) -> String {
    let mut key = format!("{}:{};", request.url.len(), request.url);
    for name in vary {
        let name = name.to_ascii_lowercase();
        let value = request.headers.get(&name).map(String::as_str).unwrap_or("");
        key.push_str(&format!(
            "{}:{}={}:{};",
            name.len(),
            name,
            value.len(),
            value
        ));
    }
    key
}
