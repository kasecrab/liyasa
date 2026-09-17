//! The slice of LSP 3.17 this server speaks.
//!
//! Every struct here is a structural subset: fields the client sends that we
//! do not declare are dropped by serde, and fields we do not send are optional
//! in the specification. A client that speaks the whole protocol cannot tell
//! the difference.
//!
//! TODO(rfc-3000): replaced wholesale by `lsp-types` if §6.2.1 gains a row.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---- positions ----

/// Zero-based, and `character` is counted in the units of the negotiated
/// [`PositionEncoding`] — not in bytes and not in characters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub const fn empty(at: Position) -> Self {
        Self { start: at, end: at }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

/// How a client counts `Position::character`. The specification's default is
/// `utf-16`; a client that can do better says so in its capabilities.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PositionEncoding {
    #[default]
    #[serde(rename = "utf-16")]
    Utf16,
    #[serde(rename = "utf-8")]
    Utf8,
    #[serde(rename = "utf-32")]
    Utf32,
}

// ---- lifecycle ----

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    #[serde(default)]
    pub root_uri: Option<String>,
    #[serde(default)]
    pub workspace_folders: Option<Vec<WorkspaceFolder>>,
    #[serde(default)]
    pub capabilities: ClientCapabilities,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFolder {
    pub uri: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(default)]
    pub general: Option<GeneralClientCapabilities>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralClientCapabilities {
    /// In client order of preference; the server picks the first it supports.
    #[serde(default)]
    pub position_encodings: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerInfo {
    pub name: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    pub position_encoding: PositionEncoding,
    pub text_document_sync: TextDocumentSyncKind,
    pub completion_provider: CompletionOptions,
    pub hover_provider: bool,
    pub definition_provider: bool,
}

/// Only `Full` is offered. Incremental sync saves bytes on a file the scanner
/// re-reads whole anyway, and a wrong range application is a class of bug this
/// server does not need to own.
#[derive(Debug, Clone, Copy, Serialize)]
#[repr(u8)]
pub enum TextDocumentSyncKind {
    None = 0,
    Full = 1,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionOptions {
    /// The characters that open a completable construct: `:` a directive, `{`
    /// a template expression or a prop list, `(` a link target, `/` a route
    /// segment, `.` a fact path, and a space a further prop.
    pub trigger_characters: Vec<String>,
}

// ---- documents ----

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentItem {
    pub uri: String,
    #[serde(default)]
    pub language_id: String,
    #[serde(default)]
    pub version: i32,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionedTextDocumentIdentifier {
    pub uri: String,
    #[serde(default)]
    pub version: i32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidOpenTextDocumentParams {
    pub text_document: TextDocumentItem,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidChangeTextDocumentParams {
    pub text_document: VersionedTextDocumentIdentifier,
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentContentChangeEvent {
    /// Absent under `TextDocumentSyncKind::Full`, which is all this server
    /// advertises; a client that sends one anyway is answered by replacing the
    /// whole document, because it was told not to send ranges.
    #[serde(default)]
    pub range: Option<Range>,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DidCloseTextDocumentParams {
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentPositionParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

// ---- diagnostics ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishDiagnosticsParams {
    pub uri: String,
    pub version: i32,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    /// The Liyasa code, `E0310`, which is what an author searches for.
    pub code: String,
    pub code_description: CodeDescription,
    pub source: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related_information: Vec<DiagnosticRelatedInformation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

/// The help article the code links to, so the editor shows `E0310` as a link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CodeDescription {
    pub href: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiagnosticRelatedInformation {
    pub location: Location,
    pub message: String,
}

// ---- completion ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionList {
    /// Always true: the lists are position-dependent, so a client must re-ask
    /// rather than filter the previous answer as the author keeps typing.
    pub is_incomplete: bool,
    pub items: Vec<CompletionItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<MarkupContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
    /// Sorting is the server's call, not the client's alphabet: required props
    /// come before optional ones and built-ins before user components.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_text: Option<String>,
}

impl CompletionItem {
    pub fn new(label: impl Into<String>, kind: CompletionItemKind) -> Self {
        Self {
            label: label.into(),
            kind,
            detail: None,
            documentation: None,
            insert_text: None,
            sort_text: None,
        }
    }

    #[must_use]
    pub fn detail(mut self, text: impl Into<String>) -> Self {
        self.detail = Some(text.into());
        self
    }

    #[must_use]
    pub fn documentation(mut self, markdown: impl Into<String>) -> Self {
        self.documentation = Some(MarkupContent::markdown(markdown));
        self
    }

    #[must_use]
    pub fn insert(mut self, text: impl Into<String>) -> Self {
        self.insert_text = Some(text.into());
        self
    }

    #[must_use]
    pub fn sort(mut self, text: impl Into<String>) -> Self {
        self.sort_text = Some(text.into());
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u8)]
pub enum CompletionItemKind {
    Field = 5,
    Variable = 6,
    Property = 10,
    Value = 12,
    Reference = 18,
    Folder = 19,
    Struct = 22,
}

// ---- hover ----

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hover {
    pub contents: MarkupContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarkupContent {
    pub kind: &'static str,
    pub value: String,
}

impl MarkupContent {
    pub fn markdown(value: impl Into<String>) -> Self {
        Self {
            kind: "markdown",
            value: value.into(),
        }
    }
}

// ---- preview ----

/// `liyasa/preview`, the one method outside the standard. The extension holds a
/// webview and asks for the HTML of the document it is showing; the server
/// renders the same Rendered AST the build would, so the preview is not a
/// second implementation of the language.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewParams {
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    pub uri: String,
    pub version: i32,
    pub html: String,
    /// Rendering a page with an error in it still produces HTML for everything
    /// around the error, and the preview says so rather than going blank.
    pub diagnostics: Vec<Diagnostic>,
}

// ---- window ----

#[derive(Debug, Clone, Serialize)]
pub struct ShowMessageParams {
    #[serde(rename = "type")]
    pub kind: MessageType,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[repr(u8)]
pub enum MessageType {
    Error = 1,
    Warning = 2,
    Info = 3,
    Log = 4,
}

/// The `result` of a request the specification lets answer with nothing.
pub fn null() -> Value {
    Value::Null
}
