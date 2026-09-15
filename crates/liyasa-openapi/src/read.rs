//! Reading the document tree into the typed model (API-01, API-02).
//!
//! The reader is hand-written rather than derived so that every complaint
//! carries the JSON pointer of the node it is about, which is what API-53 and
//! `liyasa validate --openapi` promise, and so that a spec Liyasa did not
//! write can be wrong in one place without losing the rest of the page.
//!
//! `$ref` is resolved here, against documents a [`Documents`] set has already
//! fetched, because resolving on the tree cannot terminate on a recursive
//! schema. A reference that closes a cycle becomes a named stub instead, which
//! is the "expand" control of API-11.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};

use crate::model::{
    AdditionalProperties, Callback, CodeSample, Components, Contact, Discriminator, Encoding,
    Example, Extensions, ExternalDocs, Header, Info, License, Link, MediaType, Method, Number,
    OAuthFlow, OAuthFlows, Operation, OrderedMap, Parameter, ParameterIn, PathItem, RequestBody,
    Response, Schema, SchemaType, SecurityRequirement, SecurityScheme, SecuritySchemeKind, Server,
    ServerVariable, Spec, Style, Tag, XLiyasa, Xml,
};
use crate::tree::{Pointer, Value, as_bool, as_map, as_seq, as_str};
use crate::version::SpecVersion;

/// Every document a spec's references reach: the one that was configured, plus
/// whatever `$ref` pulled in.
///
/// Each is filed under the key a relative `$ref` written inside it resolves
/// against, which is the spec's own path, so the reader and the fetcher agree
/// on what `shared/params.yaml` means.
#[derive(Debug, Clone, Default)]
pub struct Documents {
    documents: OrderedMap<Value>,
    root: String,
}

impl Documents {
    pub fn new(key: impl Into<String>, root: Value) -> Self {
        let key = key.into();
        let mut documents = OrderedMap::new();
        documents.insert(key.clone(), root);
        Self {
            documents,
            root: key,
        }
    }

    pub fn insert(&mut self, key: impl Into<String>, document: Value) {
        self.documents.insert(key, document);
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.documents.get(key)
    }

    pub fn root(&self) -> Option<&Value> {
        self.documents.get(&self.root)
    }

    pub fn root_key(&self) -> &str {
        &self.root
    }

    pub fn contains(&self, key: &str) -> bool {
        self.documents.contains_key(key)
    }
}

/// Where the reader is: which document, and where in it.
#[derive(Debug, Clone)]
pub struct At {
    pub doc: String,
    pub pointer: Pointer,
}

impl At {
    pub fn root_of(doc: impl Into<String>) -> Self {
        Self {
            doc: doc.into(),
            pointer: Pointer::root(),
        }
    }

    #[must_use]
    pub fn push(&self, segment: &str) -> Self {
        Self {
            doc: self.doc.clone(),
            pointer: self.pointer.push(segment),
        }
    }

    #[must_use]
    pub fn index(&self, at: usize) -> Self {
        Self {
            doc: self.doc.clone(),
            pointer: self.pointer.index(at),
        }
    }

    fn key(&self) -> String {
        format!("{}#{}", self.doc, self.pointer.as_str())
    }
}

impl std::fmt::Display for At {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.doc.is_empty() {
            write!(f, "{}", self.pointer)
        } else {
            write!(f, "{}#{}", self.doc, self.pointer.as_str())
        }
    }
}

/// How deep a self-referential schema is expanded before the reader stops and
/// leaves a named stub for the reader interface to expand on demand (API-11).
const DEPTH_LIMIT: usize = 12;

/// How many `$ref` hops a chain may take before the reader calls it a mistake
/// rather than a chain.
const MAX_HOPS: usize = 32;

pub struct Reader<'a> {
    docs: &'a Documents,
    diagnostics: Diagnostics,
    /// `doc#pointer` of every reference currently being followed.
    stack: Vec<String>,
}

impl<'a> Reader<'a> {
    pub fn new(docs: &'a Documents) -> Self {
        Self {
            docs,
            diagnostics: Diagnostics::new(),
            stack: Vec::new(),
        }
    }

    pub fn into_diagnostics(self) -> Diagnostics {
        self.diagnostics
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    fn complain(&mut self, at: &At, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(
            code::E0501,
            format!("{at}: {}", message.into()),
        ));
    }

    /// Reads the whole document. The version is supplied because it is decided
    /// before normalizing, on the source document.
    pub fn spec(&mut self, id: impl Into<String>, version: SpecVersion) -> Spec {
        let at = At::root_of(self.docs.root_key());
        let root = self.docs.root().cloned().unwrap_or(Value::Null);
        if as_map(&root).is_none() {
            self.complain(&at, "the document is not a mapping");
        }
        Spec {
            id: id.into(),
            version,
            info: self.info(&root, &at.push("info")),
            servers: self.servers(&root, &at),
            paths: self.path_items(&root, &at, "paths"),
            webhooks: self.path_items(&root, &at, "webhooks"),
            components: self.components(&root, &at),
            security: self.security(&root, &at),
            tags: self.tags(&root, &at),
            external_docs: self.external_docs(&root, &at),
            extensions: extensions_of(&root),
        }
    }

    // ---- references ----

    /// Follows `$ref` until a node that is not one, or until a cycle. Returns
    /// the node, where it lives, and the component name a stub needs.
    ///
    /// The cycle stack is not touched here: a reference is a cycle only while
    /// the node it names is still being *read*, which outlives following it,
    /// so [`Reader::read_at`] is what pushes and pops.
    fn follow(&mut self, value: &Value, at: &At) -> Followed {
        let mut node = value.clone();
        let mut place = at.clone();
        let mut name = None;
        for _ in 0..MAX_HOPS {
            let Some(reference) = crate::tree::field(&node, "$ref").and_then(as_str) else {
                return Followed::Node(node, place, name);
            };
            let reference = reference.to_owned();
            let Some(target) = self.target(&reference, &place) else {
                return Followed::Missing;
            };
            if let Some(found) = component_name(&target.pointer) {
                name = Some(found);
            }
            if self.stack.contains(&target.key()) || self.stack.len() >= DEPTH_LIMIT {
                return Followed::Cycle(name);
            }
            let Some(document) = self.docs.get(&target.doc) else {
                self.diagnostics.push(
                    Diagnostic::new(
                        code::E0502,
                        format!("{place}: `{reference}` points at a document that was not loaded"),
                    )
                    .help("a remote `$ref` needs its host in the project's allow list"),
                );
                return Followed::Missing;
            };
            let Some(found) = crate::tree::get(document, &target.pointer).cloned() else {
                self.diagnostics.push(
                    Diagnostic::new(
                        code::E0502,
                        format!("{place}: `{reference}` does not resolve"),
                    )
                    .help(format!(
                        "nothing is at `{}` in that document",
                        target.pointer
                    )),
                );
                return Followed::Missing;
            };
            node = found;
            place = target;
        }
        self.diagnostics.push(Diagnostic::new(
            code::E0502,
            format!("{at}: this `$ref` chain is more than {MAX_HOPS} hops long"),
        ));
        Followed::Missing
    }

    /// Reads a node with its own place on the cycle stack, so a reference back
    /// to it while it is still being read is seen as the cycle it is.
    fn read_at<T>(
        &mut self,
        place: &At,
        node: &Value,
        read: impl FnOnce(&mut Self, &Value, &At) -> T,
    ) -> T {
        self.stack.push(place.key());
        let out = read(self, node, place);
        self.stack.pop();
        out
    }

    /// Splits `document#/pointer` and resolves the document part relative to
    /// the document the reference was written in.
    fn target(&mut self, reference: &str, at: &At) -> Option<At> {
        let (document, pointer) = match reference.split_once('#') {
            Some((document, pointer)) => (document, pointer),
            None => (reference, ""),
        };
        let doc = if document.is_empty() {
            at.doc.clone()
        } else {
            crate::refs::join(&at.doc, document)
        };
        if !self.docs.contains(&doc) {
            self.diagnostics.push(
                Diagnostic::new(
                    code::E0502,
                    format!("{at}: `{reference}` points outside the loaded documents"),
                )
                .help("check the path, and that a remote host is in the allow list"),
            );
            return None;
        }
        Some(At {
            doc,
            pointer: Pointer::parse(pointer),
        })
    }

    // ---- leaves ----

    fn string(&mut self, value: &Value, at: &At, key: &str) -> Option<String> {
        let found = crate::tree::field(value, key)?;
        match as_str(found) {
            Some(text) => Some(text.to_owned()),
            None => {
                self.complain(&at.push(key), "expected a string");
                None
            }
        }
    }

    fn required_string(&mut self, value: &Value, at: &At, key: &str) -> String {
        match self.string(value, at, key) {
            Some(text) => text,
            None => {
                if crate::tree::field(value, key).is_none() {
                    self.complain(at, format!("`{key}` is required"));
                }
                String::new()
            }
        }
    }

    fn flag(&mut self, value: &Value, at: &At, key: &str) -> bool {
        let Some(found) = crate::tree::field(value, key) else {
            return false;
        };
        match as_bool(found) {
            Some(flag) => flag,
            None => {
                self.complain(&at.push(key), "expected true or false");
                false
            }
        }
    }

    fn count(&mut self, value: &Value, at: &At, key: &str) -> Option<u64> {
        let found = crate::tree::field(value, key)?;
        match found.as_u64() {
            Some(number) => Some(number),
            None => {
                self.complain(
                    &at.push(key),
                    "expected a whole number that is not negative",
                );
                None
            }
        }
    }

    fn number(&mut self, value: &Value, at: &At, key: &str) -> Option<Number> {
        let found = crate::tree::field(value, key)?;
        match found {
            Value::Number(number) => Some(number.clone()),
            _ => {
                self.complain(&at.push(key), "expected a number");
                None
            }
        }
    }

    fn strings(&mut self, value: &Value, at: &At, key: &str) -> Vec<String> {
        self.list(value, at, key, |reader, item, at| match as_str(item) {
            Some(text) => Some(text.to_owned()),
            None => {
                reader.complain(at, "expected a string");
                None
            }
        })
    }

    /// Reads a sequence, skipping (with a diagnostic) any element the reader
    /// could not make sense of.
    fn list<T>(
        &mut self,
        value: &Value,
        at: &At,
        key: &str,
        mut read: impl FnMut(&mut Self, &Value, &At) -> Option<T>,
    ) -> Vec<T> {
        let Some(found) = crate::tree::field(value, key) else {
            return Vec::new();
        };
        let at = at.push(key);
        let Some(items) = as_seq(found) else {
            self.complain(&at, "expected a list");
            return Vec::new();
        };
        items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| read(self, item, &at.index(index)))
            .collect()
    }

    /// Reads a mapping in document order, skipping entries that do not read.
    fn mapping<T>(
        &mut self,
        value: &Value,
        at: &At,
        key: &str,
        mut read: impl FnMut(&mut Self, &Value, &At) -> Option<T>,
    ) -> OrderedMap<T> {
        let Some(found) = crate::tree::field(value, key) else {
            return OrderedMap::new();
        };
        let at = at.push(key);
        let Some(map) = as_map(found) else {
            self.complain(&at, "expected a mapping");
            return OrderedMap::new();
        };
        let mut out = OrderedMap::new();
        for (name, item) in map.iter() {
            let Some(name) = as_str(name) else {
                self.complain(&at, "a key in this mapping is not a string");
                continue;
            };
            if Extensions::is_extension_key(name) {
                continue;
            }
            if let Some(read) = read(self, item, &at.push(name)) {
                out.insert(name.to_owned(), read);
            }
        }
        out
    }

    /// Reads a node that may be a `$ref`, through `read`.
    fn referenced<T: Default>(
        &mut self,
        value: &Value,
        at: &At,
        read: impl FnOnce(&mut Self, &Value, &At) -> T,
    ) -> Option<T> {
        match self.follow(value, at) {
            Followed::Node(node, place, _) => Some(self.read_at(&place, &node, read)),
            Followed::Cycle(_) => Some(T::default()),
            Followed::Missing => None,
        }
    }

    // ---- the document ----

    fn info(&mut self, root: &Value, at: &At) -> Info {
        let Some(value) = crate::tree::field(root, "info") else {
            self.complain(
                &At::root_of(self.docs.root_key().to_owned()),
                "`info` is required",
            );
            return Info::default();
        };
        Info {
            title: self.required_string(value, at, "title"),
            version: self.required_string(value, at, "version"),
            summary: self.string(value, at, "summary"),
            description: self.string(value, at, "description"),
            terms_of_service: self.string(value, at, "termsOfService"),
            contact: crate::tree::field(value, "contact").map(|contact| {
                let at = at.push("contact");
                Contact {
                    name: self.string(contact, &at, "name"),
                    url: self.string(contact, &at, "url"),
                    email: self.string(contact, &at, "email"),
                }
            }),
            license: crate::tree::field(value, "license").map(|license| {
                let at = at.push("license");
                License {
                    name: self.required_string(license, &at, "name"),
                    identifier: self.string(license, &at, "identifier"),
                    url: self.string(license, &at, "url"),
                }
            }),
            extensions: extensions_of(value),
        }
    }

    fn servers(&mut self, value: &Value, at: &At) -> Vec<Server> {
        self.list(value, at, "servers", |reader, item, at| {
            Some(Server {
                url: reader.required_string(item, at, "url"),
                description: reader.string(item, at, "description"),
                variables: reader.mapping(item, at, "variables", |reader, variable, at| {
                    Some(ServerVariable {
                        default: reader.required_string(variable, at, "default"),
                        enumeration: reader.strings(variable, at, "enum"),
                        description: reader.string(variable, at, "description"),
                    })
                }),
            })
        })
    }

    fn tags(&mut self, value: &Value, at: &At) -> Vec<Tag> {
        self.list(value, at, "tags", |reader, item, at| {
            Some(Tag {
                name: reader.required_string(item, at, "name"),
                description: reader.string(item, at, "description"),
                external_docs: reader.external_docs(item, at),
                extensions: extensions_of(item),
            })
        })
    }

    fn external_docs(&mut self, value: &Value, at: &At) -> Option<ExternalDocs> {
        let docs = crate::tree::field(value, "externalDocs")?;
        let at = at.push("externalDocs");
        Some(ExternalDocs {
            url: self.required_string(docs, &at, "url"),
            description: self.string(docs, &at, "description"),
        })
    }

    fn security(&mut self, value: &Value, at: &At) -> Vec<SecurityRequirement> {
        self.list(value, at, "security", |reader, item, at| {
            let Some(map) = as_map(item) else {
                reader.complain(
                    at,
                    "a security requirement is a mapping of scheme to scopes",
                );
                return None;
            };
            let mut out = OrderedMap::new();
            for (name, scopes) in map.iter() {
                let Some(name) = as_str(name) else { continue };
                let scopes = as_seq(scopes)
                    .map(|items| items.iter().filter_map(as_str).map(str::to_owned).collect())
                    .unwrap_or_default();
                out.insert(name.to_owned(), scopes);
            }
            Some(SecurityRequirement(out))
        })
    }

    fn path_items(&mut self, root: &Value, at: &At, key: &str) -> OrderedMap<PathItem> {
        self.mapping(root, at, key, |reader, item, at| {
            reader.referenced(item, at, Self::path_item)
        })
    }

    fn path_item(&mut self, value: &Value, at: &At) -> PathItem {
        let mut operations = OrderedMap::new();
        for method in Method::ALL {
            let name = method.lowercase();
            if let Some(found) = crate::tree::field(value, name) {
                let at = at.push(name);
                operations.insert(name.to_owned(), self.operation(found, &at));
            }
        }
        PathItem {
            summary: self.string(value, at, "summary"),
            description: self.string(value, at, "description"),
            operations,
            servers: self.servers(value, at),
            parameters: self.parameters(value, at),
            extensions: extensions_of(value),
        }
    }

    fn operation(&mut self, value: &Value, at: &At) -> Operation {
        let extensions = extensions_of(value);
        let liyasa = XLiyasa::read(&extensions);
        Operation {
            operation_id: self.string(value, at, "operationId"),
            summary: self.string(value, at, "summary"),
            description: self.string(value, at, "description"),
            tags: self.strings(value, at, "tags"),
            deprecated: self.flag(value, at, "deprecated"),
            parameters: self.parameters(value, at),
            request_body: crate::tree::field(value, "requestBody").and_then(|body| {
                self.referenced(body, &at.push("requestBody"), Self::request_body)
            }),
            responses: self.mapping(value, at, "responses", |reader, item, at| {
                reader.referenced(item, at, Self::response)
            }),
            callbacks: self.mapping(value, at, "callbacks", |reader, item, at| {
                reader.referenced(item, at, |reader, item, at| {
                    Callback(reader.mapping_of_path_items(item, at))
                })
            }),
            security: crate::tree::field(value, "security").map(|_| self.security(value, at)),
            servers: self.servers(value, at),
            external_docs: self.external_docs(value, at),
            code_samples: self.code_samples(value, at),
            extensions,
            liyasa,
        }
    }

    /// A callback's own mapping: expression to path item, with no `callbacks`
    /// key above it.
    fn mapping_of_path_items(&mut self, value: &Value, at: &At) -> OrderedMap<PathItem> {
        let Some(map) = as_map(value) else {
            self.complain(at, "expected a mapping of expression to path item");
            return OrderedMap::new();
        };
        let mut out = OrderedMap::new();
        for (expression, item) in map.iter() {
            let Some(expression) = as_str(expression) else {
                continue;
            };
            if Extensions::is_extension_key(expression) {
                continue;
            }
            let at = at.push(expression);
            if let Some(item) = self.referenced(item, &at, Self::path_item) {
                out.insert(expression.to_owned(), item);
            }
        }
        out
    }

    /// `x-codeSamples`, and the older `x-code-samples` spelling that Redoc
    /// established and several published specs still use (API-31).
    fn code_samples(&mut self, value: &Value, at: &At) -> Vec<CodeSample> {
        let key = ["x-codeSamples", "x-code-samples"]
            .into_iter()
            .find(|key| crate::tree::field(value, key).is_some());
        let Some(key) = key else {
            return Vec::new();
        };
        self.list(value, at, key, |reader, item, at| {
            let lang = reader.required_string(item, at, "lang");
            if lang.is_empty() {
                return None;
            }
            Some(CodeSample {
                lang,
                label: reader.string(item, at, "label"),
                source: reader.string(item, at, "source").unwrap_or_default(),
            })
        })
    }

    fn parameters(&mut self, value: &Value, at: &At) -> Vec<Parameter> {
        self.list(value, at, "parameters", |reader, item, at| {
            reader.referenced(item, at, Self::parameter)
        })
    }

    fn parameter(&mut self, value: &Value, at: &At) -> Parameter {
        let extensions = extensions_of(value);
        let liyasa = XLiyasa::read(&extensions);
        let location = match self.string(value, at, "in").as_deref() {
            Some(text) => match ParameterIn::parse(text) {
                Some(location) => location,
                None => {
                    self.complain(
                        &at.push("in"),
                        format!("`{text}` is not one of path, query, header, or cookie"),
                    );
                    ParameterIn::default()
                }
            },
            None => {
                self.complain(at, "`in` is required");
                ParameterIn::default()
            }
        };
        let required = self.flag(value, at, "required");
        if location == ParameterIn::Path && !required {
            self.complain(at, "a path parameter must be `required: true`");
        }
        Parameter {
            name: self.required_string(value, at, "name"),
            location,
            description: self.string(value, at, "description"),
            required,
            deprecated: self.flag(value, at, "deprecated"),
            allow_empty_value: self.flag(value, at, "allowEmptyValue"),
            style: self.style(value, at),
            explode: crate::tree::field(value, "explode").and_then(as_bool),
            allow_reserved: self.flag(value, at, "allowReserved"),
            schema: self.maybe_schema(value, at),
            content: self.content(value, at),
            example: crate::tree::field(value, "example").cloned(),
            examples: self.examples(value, at),
            extensions,
            liyasa,
        }
    }

    fn style(&mut self, value: &Value, at: &At) -> Style {
        match self.string(value, at, "style") {
            Some(text) => match Style::parse(&text) {
                Some(style) => style,
                None => {
                    self.complain(&at.push("style"), format!("`{text}` is not a known style"));
                    Style::Unset
                }
            },
            None => Style::Unset,
        }
    }

    fn request_body(&mut self, value: &Value, at: &At) -> RequestBody {
        RequestBody {
            description: self.string(value, at, "description"),
            required: self.flag(value, at, "required"),
            content: self.content(value, at),
            extensions: extensions_of(value),
        }
    }

    fn content(&mut self, value: &Value, at: &At) -> OrderedMap<MediaType> {
        self.mapping(value, at, "content", |reader, item, at| {
            Some(MediaType {
                schema: reader.maybe_schema(item, at),
                example: crate::tree::field(item, "example").cloned(),
                examples: reader.examples(item, at),
                encoding: reader.mapping(item, at, "encoding", |reader, encoding, at| {
                    Some(Encoding {
                        content_type: reader.string(encoding, at, "contentType"),
                        headers: reader.headers(encoding, at),
                        style: reader.style(encoding, at),
                        explode: crate::tree::field(encoding, "explode").and_then(as_bool),
                        allow_reserved: reader.flag(encoding, at, "allowReserved"),
                    })
                }),
                extensions: extensions_of(item),
            })
        })
    }

    fn examples(&mut self, value: &Value, at: &At) -> OrderedMap<Example> {
        self.mapping(value, at, "examples", |reader, item, at| {
            reader.referenced(item, at, Self::example)
        })
    }

    fn example(&mut self, value: &Value, at: &At) -> Example {
        Example {
            summary: self.string(value, at, "summary"),
            description: self.string(value, at, "description"),
            value: crate::tree::field(value, "value").cloned(),
            external_value: self.string(value, at, "externalValue"),
        }
    }

    fn response(&mut self, value: &Value, at: &At) -> Response {
        Response {
            description: self.required_string(value, at, "description"),
            headers: self.headers(value, at),
            content: self.content(value, at),
            links: self.mapping(value, at, "links", |reader, item, at| {
                reader.referenced(item, at, Self::link)
            }),
            extensions: extensions_of(value),
        }
    }

    fn headers(&mut self, value: &Value, at: &At) -> OrderedMap<Header> {
        self.mapping(value, at, "headers", |reader, item, at| {
            reader.referenced(item, at, Self::header)
        })
    }

    fn header(&mut self, value: &Value, at: &At) -> Header {
        Header {
            description: self.string(value, at, "description"),
            required: self.flag(value, at, "required"),
            deprecated: self.flag(value, at, "deprecated"),
            style: self.style(value, at),
            explode: crate::tree::field(value, "explode").and_then(as_bool),
            schema: self.maybe_schema(value, at),
            content: self.content(value, at),
            example: crate::tree::field(value, "example").cloned(),
            examples: self.examples(value, at),
        }
    }

    fn link(&mut self, value: &Value, at: &At) -> Link {
        Link {
            operation_ref: self.string(value, at, "operationRef"),
            operation_id: self.string(value, at, "operationId"),
            description: self.string(value, at, "description"),
            parameters: self.mapping(value, at, "parameters", |_, item, _| Some(item.clone())),
            request_body: crate::tree::field(value, "requestBody").cloned(),
            server: crate::tree::field(value, "server").map(|server| {
                let at = at.push("server");
                Server {
                    url: self.required_string(server, &at, "url"),
                    description: self.string(server, &at, "description"),
                    variables: OrderedMap::new(),
                }
            }),
        }
    }

    fn security_scheme(&mut self, value: &Value, at: &At) -> Option<SecurityScheme> {
        let kind = match self.string(value, at, "type").as_deref() {
            Some("http") => SecuritySchemeKind::Http {
                scheme: self.required_string(value, at, "scheme"),
                bearer_format: self.string(value, at, "bearerFormat"),
            },
            Some("apiKey") => SecuritySchemeKind::ApiKey {
                name: self.required_string(value, at, "name"),
                location: self
                    .string(value, at, "in")
                    .as_deref()
                    .and_then(ParameterIn::parse)
                    .unwrap_or(ParameterIn::Header),
            },
            Some("oauth2") => SecuritySchemeKind::OAuth2 {
                flows: Box::new(self.oauth_flows(value, at)),
            },
            Some("openIdConnect") => SecuritySchemeKind::OpenIdConnect {
                url: self.required_string(value, at, "openIdConnectUrl"),
            },
            Some("mutualTLS") => SecuritySchemeKind::MutualTls,
            Some(other) => {
                let other = other.to_owned();
                self.complain(
                    &at.push("type"),
                    format!("`{other}` is not a security scheme type"),
                );
                return None;
            }
            None => {
                self.complain(at, "`type` is required on a security scheme");
                return None;
            }
        };
        Some(SecurityScheme {
            kind,
            description: self.string(value, at, "description"),
            extensions: extensions_of(value),
        })
    }

    fn oauth_flows(&mut self, value: &Value, at: &At) -> OAuthFlows {
        let Some(flows) = crate::tree::field(value, "flows") else {
            self.complain(at, "`flows` is required on an oauth2 scheme");
            return OAuthFlows::default();
        };
        let at = at.push("flows");
        let mut read = |key: &str| -> Option<OAuthFlow> {
            let flow = crate::tree::field(flows, key)?;
            let at = at.push(key);
            Some(OAuthFlow {
                authorization_url: self.string(flow, &at, "authorizationUrl"),
                token_url: self.string(flow, &at, "tokenUrl"),
                refresh_url: self.string(flow, &at, "refreshUrl"),
                scopes: self.mapping(flow, &at, "scopes", |reader, scope, at| {
                    match as_str(scope) {
                        Some(text) => Some(text.to_owned()),
                        None => {
                            reader.complain(at, "a scope's description is a string");
                            None
                        }
                    }
                }),
            })
        };
        OAuthFlows {
            implicit: read("implicit"),
            password: read("password"),
            client_credentials: read("clientCredentials"),
            authorization_code: read("authorizationCode"),
        }
    }

    fn components(&mut self, root: &Value, at: &At) -> Components {
        let Some(value) = crate::tree::field(root, "components") else {
            return Components::default();
        };
        let at = at.push("components");
        Components {
            schemas: self.mapping(value, &at, "schemas", |reader, item, at| {
                let mut schema = reader.schema(item, at);
                schema.name = component_name(&at.pointer);
                Some(schema)
            }),
            responses: self.mapping(value, &at, "responses", |reader, item, at| {
                reader.referenced(item, at, Self::response)
            }),
            parameters: self.mapping(value, &at, "parameters", |reader, item, at| {
                reader.referenced(item, at, Self::parameter)
            }),
            examples: self.mapping(value, &at, "examples", |reader, item, at| {
                reader.referenced(item, at, Self::example)
            }),
            request_bodies: self.mapping(value, &at, "requestBodies", |reader, item, at| {
                reader.referenced(item, at, Self::request_body)
            }),
            headers: self.mapping(value, &at, "headers", |reader, item, at| {
                reader.referenced(item, at, Self::header)
            }),
            security_schemes: self.mapping(value, &at, "securitySchemes", |reader, item, at| {
                reader.security_scheme(item, at)
            }),
            links: self.mapping(value, &at, "links", |reader, item, at| {
                reader.referenced(item, at, Self::link)
            }),
            callbacks: self.mapping(value, &at, "callbacks", |reader, item, at| {
                reader.referenced(item, at, |reader, item, at| {
                    Callback(reader.mapping_of_path_items(item, at))
                })
            }),
            path_items: self.mapping(value, &at, "pathItems", |reader, item, at| {
                reader.referenced(item, at, Self::path_item)
            }),
            extensions: extensions_of(value),
        }
    }

    // ---- schemas ----

    fn maybe_schema(&mut self, value: &Value, at: &At) -> Option<Schema> {
        let found = crate::tree::field(value, "schema")?;
        Some(self.schema(found, &at.push("schema")))
    }

    /// Reads one schema, following a `$ref` and cutting a cycle into a named
    /// stub the reader interface expands on demand (API-11).
    pub fn schema(&mut self, value: &Value, at: &At) -> Schema {
        let (node, place, name) = match self.follow(value, at) {
            Followed::Node(node, place, name) => (node, place, name),
            Followed::Cycle(name) => {
                return Schema {
                    name,
                    ..Schema::default()
                };
            }
            Followed::Missing => return Schema::default(),
        };
        let mut schema = self.read_at(&place, &node, Self::schema_body);
        schema.name = name;
        schema
    }

    fn schema_body(&mut self, value: &Value, at: &At) -> Schema {
        // 2020-12 allows a bare boolean where a schema goes: `true` accepts
        // everything, `false` accepts nothing.
        if let Some(flag) = as_bool(value) {
            return if flag {
                Schema::default()
            } else {
                Schema {
                    not: Some(Box::new(Schema::default())),
                    ..Schema::default()
                }
            };
        }
        if as_map(value).is_none() {
            self.complain(at, "a schema is a mapping, or true or false");
            return Schema::default();
        }

        let extensions = extensions_of(value);
        let lower = self.bound(value, at, "minimum", "exclusiveMinimum");
        let upper = self.bound(value, at, "maximum", "exclusiveMaximum");
        let mut schema = Schema {
            types: self.types(value, at),
            format: self.string(value, at, "format"),
            title: self.string(value, at, "title"),
            description: self.string(value, at, "description"),
            default: crate::tree::field(value, "default").cloned(),
            examples: self.example_values(value, at),
            deprecated: self.flag(value, at, "deprecated"),
            read_only: self.flag(value, at, "readOnly"),
            write_only: self.flag(value, at, "writeOnly"),
            enumeration: crate::tree::field(value, "enum")
                .and_then(as_seq)
                .map(<[Value]>::to_vec)
                .unwrap_or_default(),
            constant: crate::tree::field(value, "const").cloned(),

            all_of: self.schema_list(value, at, "allOf"),
            one_of: self.schema_list(value, at, "oneOf"),
            any_of: self.schema_list(value, at, "anyOf"),
            not: crate::tree::field(value, "not")
                .map(|not| Box::new(self.schema(not, &at.push("not")))),
            discriminator: self.discriminator(value, at),

            properties: self.schema_map(value, at, "properties"),
            required: self.strings(value, at, "required"),
            additional_properties: self.additional_properties(value, at),
            pattern_properties: self.schema_map(value, at, "patternProperties"),
            property_names: crate::tree::field(value, "propertyNames")
                .map(|names| Box::new(self.schema(names, &at.push("propertyNames")))),
            min_properties: self.count(value, at, "minProperties"),
            max_properties: self.count(value, at, "maxProperties"),

            items: crate::tree::field(value, "items")
                .map(|items| Box::new(self.schema(items, &at.push("items")))),
            prefix_items: self.schema_list(value, at, "prefixItems"),
            min_items: self.count(value, at, "minItems"),
            max_items: self.count(value, at, "maxItems"),
            unique_items: self.flag(value, at, "uniqueItems"),

            min_length: self.count(value, at, "minLength"),
            max_length: self.count(value, at, "maxLength"),
            pattern: self.string(value, at, "pattern"),
            content_media_type: self.string(value, at, "contentMediaType"),
            content_encoding: self.string(value, at, "contentEncoding"),

            minimum: lower.0,
            maximum: upper.0,
            exclusive_minimum: lower.1,
            exclusive_maximum: upper.1,
            multiple_of: self.number(value, at, "multipleOf"),

            external_docs: self.external_docs(value, at),
            xml: self.xml(value, at),
            name: None,
            rest: unmodelled(value),
            extensions,
        };
        let hints = XLiyasa::read(&schema.extensions);
        if hints.title.is_some() {
            schema.title = hints.title;
        }
        if hints.description.is_some() {
            schema.description = hints.description;
        }

        // Every member was read by this same function, so each has already
        // folded its own `allOf`; only this level is left.
        for conflict in crate::allof::fold_here(&mut schema) {
            self.diagnostics.push(Diagnostic::new(
                code::E0508,
                format!("{at}{}: {}", conflict.pointer(), conflict.message),
            ));
        }
        schema
    }

    /// The inclusive and exclusive bound for one end of a numeric range.
    ///
    /// 3.0 wrote `exclusiveMinimum: true` beside `minimum`; 2020-12 writes the
    /// number itself. [`crate::normalize::v30`] rewrites the root document,
    /// but a fragment reached by `$ref` has no version of its own to rewrite
    /// from, so the boolean form is read here too rather than complained about.
    fn bound(
        &mut self,
        value: &Value,
        at: &At,
        inclusive: &str,
        exclusive: &str,
    ) -> (Option<Number>, Option<Number>) {
        match crate::tree::field(value, exclusive).and_then(as_bool) {
            Some(true) => (None, self.number(value, at, inclusive)),
            Some(false) => (self.number(value, at, inclusive), None),
            None => (
                self.number(value, at, inclusive),
                self.number(value, at, exclusive),
            ),
        }
    }

    fn types(&mut self, value: &Value, at: &At) -> Vec<SchemaType> {
        let nullable = crate::tree::field(value, "nullable").and_then(as_bool) == Some(true);
        let Some(found) = crate::tree::field(value, "type") else {
            return Vec::new();
        };
        let at = at.push("type");
        let names: Vec<&Value> = match found {
            Value::Sequence(items) => items.iter().collect(),
            other => vec![other],
        };
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            match as_str(name).and_then(SchemaType::parse) {
                Some(ty) if !out.contains(&ty) => out.push(ty),
                Some(_) => {}
                None => self.complain(&at, "`type` names a JSON Schema type"),
            }
        }
        // 3.0's `nullable`, for a fragment the root's normalizer never saw.
        if nullable && !out.is_empty() && !out.contains(&SchemaType::Null) {
            out.push(SchemaType::Null);
        }
        out
    }

    fn example_values(&mut self, value: &Value, at: &At) -> Vec<Value> {
        // 3.1 spells it `examples` (a list); 3.0's singular `example` is
        // lifted into the same list by the normalizer, but a hand-written 3.1
        // document sometimes still carries it.
        let mut out = crate::tree::field(value, "examples")
            .and_then(as_seq)
            .map(<[Value]>::to_vec)
            .unwrap_or_default();
        if let Some(single) = crate::tree::field(value, "example")
            && !out.contains(single)
        {
            out.push(single.clone());
        }
        let _ = at;
        out
    }

    fn schema_list(&mut self, value: &Value, at: &At, key: &str) -> Vec<Schema> {
        self.list(value, at, key, |reader, item, at| {
            Some(reader.schema(item, at))
        })
    }

    fn schema_map(&mut self, value: &Value, at: &At, key: &str) -> OrderedMap<Schema> {
        self.mapping(value, at, key, |reader, item, at| {
            Some(reader.schema(item, at))
        })
    }

    fn additional_properties(&mut self, value: &Value, at: &At) -> AdditionalProperties {
        let Some(found) = crate::tree::field(value, "additionalProperties") else {
            return AdditionalProperties::Unset;
        };
        match as_bool(found) {
            Some(true) => AdditionalProperties::Allowed,
            Some(false) => AdditionalProperties::Denied,
            None => AdditionalProperties::Schema(Box::new(
                self.schema(found, &at.push("additionalProperties")),
            )),
        }
    }

    fn discriminator(&mut self, value: &Value, at: &At) -> Option<Discriminator> {
        let found = crate::tree::field(value, "discriminator")?;
        let at = at.push("discriminator");
        Some(Discriminator {
            property_name: self.required_string(found, &at, "propertyName"),
            mapping: self.mapping(found, &at, "mapping", |reader, item, at| {
                match as_str(item) {
                    Some(text) => Some(text.to_owned()),
                    None => {
                        reader.complain(at, "a discriminator mapping's value is a string");
                        None
                    }
                }
            }),
        })
    }

    fn xml(&mut self, value: &Value, at: &At) -> Option<Xml> {
        let found = crate::tree::field(value, "xml")?;
        let at = at.push("xml");
        Some(Xml {
            name: self.string(found, &at, "name"),
            namespace: self.string(found, &at, "namespace"),
            prefix: self.string(found, &at, "prefix"),
            attribute: self.flag(found, &at, "attribute"),
            wrapped: self.flag(found, &at, "wrapped"),
        })
    }
}

/// What following a `$ref` produced.
enum Followed {
    /// The node, where it lives, and the component name it was reached by.
    Node(Value, At, Option<String>),
    /// The reference closed a cycle; only its name is known.
    Cycle(Option<String>),
    /// It did not resolve, and a diagnostic says so.
    Missing,
}

/// The last segment of `#/components/schemas/User`, which is the name a schema
/// page and an "expand" control show.
fn component_name(pointer: &Pointer) -> Option<String> {
    let segments: Vec<String> = pointer.segments().collect();
    match segments.as_slice() {
        [first, _kind, name] if first == "components" => Some(name.clone()),
        _ => None,
    }
}

fn extensions_of(value: &Value) -> Extensions {
    Extensions(
        crate::tree::entries(value)
            .filter(|(key, _)| Extensions::is_extension_key(key))
            .map(|(key, item)| (key.to_owned(), item.clone()))
            .collect(),
    )
}

/// Schema keywords Liyasa does not model, kept so the processed download stays
/// faithful to what the author wrote (API-50).
fn unmodelled(value: &Value) -> OrderedMap<Value> {
    const MODELLED: &[&str] = &[
        "type",
        "format",
        "title",
        "description",
        "default",
        "example",
        "examples",
        "deprecated",
        "readOnly",
        "writeOnly",
        "enum",
        "const",
        "allOf",
        "oneOf",
        "anyOf",
        "not",
        "discriminator",
        "properties",
        "required",
        "additionalProperties",
        "patternProperties",
        "propertyNames",
        "minProperties",
        "maxProperties",
        "items",
        "prefixItems",
        "minItems",
        "maxItems",
        "uniqueItems",
        "minLength",
        "maxLength",
        "pattern",
        "contentMediaType",
        "contentEncoding",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
        "externalDocs",
        "xml",
        "$ref",
        "nullable",
    ];
    crate::tree::entries(value)
        .filter(|(key, _)| !MODELLED.contains(key) && !Extensions::is_extension_key(key))
        .map(|(key, item)| (key.to_owned(), item.clone()))
        .collect()
}

/// Reads a document that needs no remote reference.
pub fn local(root: Value, id: &str, version: SpecVersion) -> (Spec, Diagnostics) {
    let docs = Documents::new(String::new(), root);
    let mut reader = Reader::new(&docs);
    let spec = reader.spec(id, version);
    (spec, reader.into_diagnostics())
}
