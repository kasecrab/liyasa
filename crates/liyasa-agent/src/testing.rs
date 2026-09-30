//! A scripted model and an in-memory page tree, for tests (feature `testing`).
//!
//! Sixteen of this package's seventeen requirements are deterministic, and this is
//! what they are tested against. A provider key buys the seventeenth — AGT-02's
//! golden replay — and nothing else, so the rest must not wait on one.
//!
//! [`ScriptedModel`] keeps every request it was sent. That matters more than the
//! replies: the security half of this package is about what reaches a model and
//! where, so a test asserting that a stranger's text arrived as a data block and
//! not in the system prompt needs to see the request, not the answer.

use std::sync::Mutex;

use liyasa_core::ai::{AiError, ChatEvent, ChatModel, ChatRequest};
use liyasa_core::ids::Route;
use liyasa_core::net::{BoxFut, BoxStream};

/// A model that answers from a script and remembers what it was asked.
pub struct ScriptedModel {
    id: String,
    script: Mutex<Vec<Vec<ChatEvent>>>,
    seen: Mutex<Vec<ChatRequest>>,
}

impl ScriptedModel {
    /// One entry per turn. A turn past the end of the script answers `Done`.
    pub fn new(script: impl IntoIterator<Item = Vec<ChatEvent>>) -> Self {
        Self {
            id: "scripted".to_owned(),
            script: Mutex::new(script.into_iter().collect()),
            seen: Mutex::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn named(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// Every request the model was sent, in order.
    pub fn seen(&self) -> Vec<ChatRequest> {
        self.seen.lock().expect("the scripted model's lock").clone()
    }

    /// How many turns are left unused. A test that scripted three turns and used
    /// one has probably not tested what it meant to.
    pub fn unused(&self) -> usize {
        self.script.lock().expect("lock").len()
    }
}

impl ChatModel for ScriptedModel {
    fn id(&self) -> &str {
        &self.id
    }

    fn complete<'a>(
        &'a self,
        req: ChatRequest,
    ) -> BoxFut<'a, Result<BoxStream<'a, ChatEvent>, AiError>> {
        self.seen.lock().expect("lock").push(req);
        let mut script = self.script.lock().expect("lock");
        let events = if script.is_empty() {
            vec![ChatEvent::Done]
        } else {
            script.remove(0)
        };
        Box::pin(std::future::ready(Ok(
            Box::pin(iter(events)) as BoxStream<'a, ChatEvent>
        )))
    }
}

fn iter<T: Send + Unpin + 'static>(items: Vec<T>) -> impl futures_core::Stream<Item = T> + Send {
    struct Iter<T>(std::vec::IntoIter<T>);
    impl<T: Unpin> futures_core::Stream for Iter<T> {
        type Item = T;
        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<T>> {
            std::task::Poll::Ready(self.0.next())
        }
    }
    Iter(items.into_iter())
}

/// An in-memory content tree, keyed by route.
#[derive(Debug, Clone, Default)]
pub struct MemoryPages {
    layout: crate::scope::Layout,
    pages: std::collections::BTreeMap<Route, (String, String)>,
}

impl MemoryPages {
    pub fn new(layout: crate::scope::Layout) -> Self {
        Self {
            layout,
            pages: std::collections::BTreeMap::new(),
        }
    }

    /// Adds a page at `path`, whose route the layout decides.
    #[must_use]
    pub fn with(mut self, path: &str, markdown: impl Into<String>) -> Self {
        let route = self
            .layout
            .route_of(path)
            .unwrap_or_else(|| panic!("`{path}` is not a content page under this layout"));
        self.pages.insert(route, (path.to_owned(), markdown.into()));
        self
    }

    /// The path and current text of a page, or `None`.
    pub fn read(&self, route: &Route) -> Option<(String, String)> {
        let route = crate::scope::normalise_route(route.as_str())?;
        self.pages.get(&route).cloned()
    }

    /// Where a page at this route lives, or would.
    pub fn path_for(&self, route: &Route) -> String {
        if let Some((path, _)) = self.read(route) {
            return path;
        }
        let route =
            crate::scope::normalise_route(route.as_str()).unwrap_or_else(|| Route::new("/"));
        let stem = route.as_str().trim_start_matches('/');
        let leaf = if stem.is_empty() {
            "index.md".to_owned()
        } else {
            format!("{stem}.md")
        };
        if self.layout.content_root.is_empty() {
            leaf
        } else {
            format!("{}/{leaf}", self.layout.content_root)
        }
    }

    pub fn routes(&self) -> impl Iterator<Item = &Route> {
        self.pages.keys()
    }
}

impl crate::run::Pages for MemoryPages {
    fn read(&self, route: &Route) -> Option<(String, String)> {
        MemoryPages::read(self, route)
    }

    fn path_for(&self, route: &Route) -> String {
        MemoryPages::path_for(self, route)
    }
}

/// One `write_page` call, as a scripted model would emit it.
pub fn write_page(id: &str, route: &str, markdown: &str) -> ChatEvent {
    ChatEvent::ToolCall {
        id: id.to_owned(),
        name: crate::tools::WRITE_PAGE.to_owned(),
        input: serde_json::json!({ "route": route, "markdown": markdown }),
    }
}

/// One call to any tool.
pub fn call(id: &str, name: &str, input: serde_json::Value) -> ChatEvent {
    ChatEvent::ToolCall {
        id: id.to_owned(),
        name: name.to_owned(),
        input,
    }
}
