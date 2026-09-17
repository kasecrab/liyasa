//! The lifecycle, the open documents, and the dispatch.
//!
//! The loop is synchronous and answers in order. A language server's
//! concurrency requirement is that it not block on I/O it does not control;
//! this one reads its own workspace and nothing else, so one thread reading the
//! stream and answering as it goes satisfies the protocol. `$/cancelRequest`
//! is accepted and ignored, which the specification permits for a server that
//! has already finished the work by the time the cancellation arrives.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use liyasa_config::vfs::{MemVfs, OsVfs};
use liyasa_core::vfs::Vfs;
use serde_json::{Value, json};

use crate::analysis::Analysis;
use crate::jsonrpc::{self, Error, Incoming, Notification, ReadError, Response};
use crate::protocol::*;
use crate::workspace::Workspace;
use crate::{completion, definition, diagnostics, hover, uri};

pub const NAME: &str = "liyasa-lsp";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One message on its way to the client.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    Response(Response),
    Notification(Notification),
}

impl Outgoing {
    fn write(&self, output: &mut impl Write) -> std::io::Result<()> {
        match self {
            Self::Response(message) => jsonrpc::write(output, message),
            Self::Notification(message) => jsonrpc::write(output, message),
        }
    }
}

struct Open {
    version: i32,
    analysis: Analysis,
}

pub struct Server {
    initialized: bool,
    shutdown: bool,
    exiting: bool,
    encoding: PositionEncoding,
    root: Option<PathBuf>,
    vfs: Arc<dyn Vfs>,
    workspace: Workspace,
    documents: BTreeMap<String, Open>,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            initialized: false,
            shutdown: false,
            exiting: false,
            encoding: PositionEncoding::Utf16,
            root: None,
            vfs: Arc::new(MemVfs::new()),
            workspace: Workspace::new(),
            documents: BTreeMap::new(),
        }
    }
}

impl Server {
    pub fn new() -> Self {
        Self::default()
    }

    /// A server whose project is already loaded, for a caller that has one — a
    /// test, or an editor host that reads the tree itself.
    pub fn with_workspace(root: Option<PathBuf>, vfs: Arc<dyn Vfs>) -> Self {
        let workspace = Workspace::load(vfs.as_ref());
        Self {
            initialized: true,
            root,
            vfs,
            workspace,
            ..Self::default()
        }
    }

    pub fn is_exiting(&self) -> bool {
        self.exiting
    }

    /// The specification: `exit` after `shutdown` is success, `exit` without it
    /// is a failure, because the client abandoned the session.
    pub fn exit_code(&self) -> i32 {
        i32::from(!self.shutdown)
    }

    pub fn handle(&mut self, message: &Incoming) -> Vec<Outgoing> {
        let method = message.method.as_str();
        match (message.id.clone(), method) {
            (Some(id), "initialize") => vec![self.initialize(id, message)],
            (Some(id), _) if !self.initialized => {
                vec![Outgoing::Response(Response::err(
                    id,
                    Error::not_initialized(),
                ))]
            }
            (Some(id), "shutdown") => {
                self.shutdown = true;
                vec![Outgoing::Response(Response::ok(id, Value::Null))]
            }
            (Some(id), "textDocument/completion") => self.answer(id, message, Self::completion),
            (Some(id), "textDocument/hover") => self.answer(id, message, Self::hover),
            (Some(id), "textDocument/definition") => self.answer(id, message, Self::definition),
            (Some(id), "liyasa/preview") => self.answer(id, message, Self::preview),
            (Some(id), _) => vec![Outgoing::Response(Response::err(
                id,
                Error::method_not_found(method),
            ))],
            (None, "exit") => {
                self.exiting = true;
                Vec::new()
            }
            (None, _) if !self.initialized => Vec::new(),
            (None, "textDocument/didOpen") => self.did_open(message),
            (None, "textDocument/didChange") => self.did_change(message),
            (None, "textDocument/didClose") => self.did_close(message),
            // `initialized`, `$/cancelRequest`, `$/setTrace` and every other
            // notification: the protocol requires that an unknown notification
            // be ignored rather than answered.
            (None, _) => Vec::new(),
        }
    }

    fn answer(
        &mut self,
        id: jsonrpc::Id,
        message: &Incoming,
        f: impl Fn(&mut Self, &Incoming) -> Result<Value, Error>,
    ) -> Vec<Outgoing> {
        vec![Outgoing::Response(match f(self, message) {
            Ok(result) => Response::ok(id, result),
            Err(error) => Response::err(id, error),
        })]
    }

    fn initialize(&mut self, id: jsonrpc::Id, message: &Incoming) -> Outgoing {
        let params: InitializeParams = match message.parse() {
            Ok(params) => params,
            Err(error) => return Outgoing::Response(Response::err(id, error)),
        };

        self.encoding = negotiate(params.capabilities.general.as_ref());
        self.root = params
            .root_uri
            .as_deref()
            .or_else(|| {
                params
                    .workspace_folders
                    .as_ref()
                    .and_then(|folders| folders.first())
                    .map(|folder| folder.uri.as_str())
            })
            .and_then(uri::to_path);

        if let Some(root) = &self.root {
            self.vfs = Arc::new(OsVfs::new(root));
            self.workspace = Workspace::load(self.vfs.as_ref());
        }
        self.initialized = true;

        Outgoing::Response(Response::ok(
            id,
            serde_json::to_value(InitializeResult {
                capabilities: ServerCapabilities {
                    position_encoding: self.encoding,
                    text_document_sync: TextDocumentSyncKind::Full,
                    completion_provider: CompletionOptions {
                        trigger_characters: [":", "{", "(", "/", ".", " ", "\""]
                            .iter()
                            .map(|c| (*c).to_owned())
                            .collect(),
                    },
                    hover_provider: true,
                    definition_provider: true,
                },
                server_info: ServerInfo {
                    name: NAME,
                    version: VERSION,
                },
            })
            .unwrap_or(Value::Null),
        ))
    }

    fn did_open(&mut self, message: &Incoming) -> Vec<Outgoing> {
        let Ok(params) = message.parse::<DidOpenTextDocumentParams>() else {
            return Vec::new();
        };
        let item = params.text_document;
        self.analyse(item.uri.clone(), item.version, &item.text);
        self.publish(&item.uri)
    }

    fn did_change(&mut self, message: &Incoming) -> Vec<Outgoing> {
        let Ok(params) = message.parse::<DidChangeTextDocumentParams>() else {
            return Vec::new();
        };
        // Full sync is all this server advertises, so the last change carries
        // the whole document however many the client sent.
        let Some(change) = params.content_changes.last() else {
            return Vec::new();
        };
        let uri = params.text_document.uri;
        self.analyse(uri.clone(), params.text_document.version, &change.text);
        self.publish(&uri)
    }

    fn did_close(&mut self, message: &Incoming) -> Vec<Outgoing> {
        let Ok(params) = message.parse::<DidCloseTextDocumentParams>() else {
            return Vec::new();
        };
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        // A closed file's diagnostics are cleared: the server no longer has the
        // buffer they were computed from, and a stale list in the problems pane
        // is a list nobody can act on.
        vec![Outgoing::Notification(Notification::new(
            "textDocument/publishDiagnostics",
            json!({ "uri": uri, "version": 0, "diagnostics": [] }),
        ))]
    }

    fn analyse(&mut self, uri: String, version: i32, text: &str) {
        let path = self.relative(&uri);
        let analysis = Analysis::of(&path, text, &self.workspace);
        self.documents.insert(uri, Open { version, analysis });
    }

    /// The path as the project spells it, so a diagnostic and a source map
    /// entry name `guides/install.md` rather than an absolute path.
    fn relative(&self, uri: &str) -> String {
        let Some(path) = uri::to_path(uri) else {
            return uri.to_owned();
        };
        match &self.root {
            Some(root) => path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned(),
            None => path.to_string_lossy().into_owned(),
        }
    }

    fn publish(&self, uri: &str) -> Vec<Outgoing> {
        let Some(open) = self.documents.get(uri) else {
            return Vec::new();
        };
        let published = diagnostics::convert_all(
            open.analysis.diagnostics.clone(),
            uri,
            &open.analysis.text,
            self.encoding,
        );
        vec![Outgoing::Notification(Notification::new(
            "textDocument/publishDiagnostics",
            serde_json::to_value(PublishDiagnosticsParams {
                uri: uri.to_owned(),
                version: open.version,
                diagnostics: published,
            })
            .unwrap_or(Value::Null),
        ))]
    }

    /// The open document a positional request names, and the byte offset its
    /// position points at.
    fn at(&self, message: &Incoming) -> Result<(&Open, u32), Error> {
        let params: TextDocumentPositionParams = message.parse()?;
        let open = self
            .documents
            .get(&params.text_document.uri)
            .ok_or_else(|| {
                Error::request_failed(format!("`{}` is not open", params.text_document.uri))
            })?;
        Ok((
            open,
            open.analysis.text.offset_of(params.position, self.encoding),
        ))
    }

    fn completion(&mut self, message: &Incoming) -> Result<Value, Error> {
        let (open, offset) = self.at(message)?;
        let items = completion::at(&open.analysis.text, &self.workspace, offset, self.encoding);
        Ok(serde_json::to_value(CompletionList {
            // Position-dependent: the author's next keystroke can change which
            // list applies, so the client must ask again rather than filter.
            is_incomplete: true,
            items,
        })
        .unwrap_or(Value::Null))
    }

    fn hover(&mut self, message: &Incoming) -> Result<Value, Error> {
        let (open, offset) = self.at(message)?;
        Ok(
            match hover::at(&open.analysis.text, &self.workspace, offset, self.encoding) {
                Some(hover) => serde_json::to_value(hover).unwrap_or(Value::Null),
                None => Value::Null,
            },
        )
    }

    fn definition(&mut self, message: &Incoming) -> Result<Value, Error> {
        let (open, offset) = self.at(message)?;
        Ok(
            match definition::at(
                &open.analysis.text,
                &self.workspace,
                self.vfs.as_ref(),
                self.root.as_deref(),
                offset,
            ) {
                Some(location) => serde_json::to_value(location).unwrap_or(Value::Null),
                None => Value::Null,
            },
        )
    }

    fn preview(&mut self, message: &Incoming) -> Result<Value, Error> {
        let params: PreviewParams = message.parse()?;
        let uri = params.text_document.uri;
        let open = self
            .documents
            .get(&uri)
            .ok_or_else(|| Error::request_failed(format!("`{uri}` is not open")))?;
        let published = diagnostics::convert_all(
            open.analysis.diagnostics.clone(),
            &uri,
            &open.analysis.text,
            self.encoding,
        );
        Ok(serde_json::to_value(PreviewResult {
            uri: uri.clone(),
            version: open.version,
            html: open.analysis.html.clone(),
            diagnostics: published,
        })
        .unwrap_or(Value::Null))
    }
}

/// The first encoding the client offers that this server counts in. The
/// specification's default when the client offers nothing is UTF-16.
fn negotiate(general: Option<&GeneralClientCapabilities>) -> PositionEncoding {
    let offered = general.and_then(|general| general.position_encodings.as_ref());
    let Some(offered) = offered else {
        return PositionEncoding::Utf16;
    };
    for encoding in offered {
        match encoding.as_str() {
            "utf-8" => return PositionEncoding::Utf8,
            "utf-16" => return PositionEncoding::Utf16,
            "utf-32" => return PositionEncoding::Utf32,
            _ => {}
        }
    }
    PositionEncoding::Utf16
}

/// Reads until the client says `exit` or closes the pipe, and returns the exit
/// code the specification asks for.
pub fn serve(input: &mut impl BufRead, output: &mut impl Write) -> std::io::Result<i32> {
    let mut server = Server::new();
    loop {
        match jsonrpc::read(input) {
            Ok(message) => {
                for outgoing in server.handle(&message) {
                    outgoing.write(output)?;
                }
                if server.is_exiting() {
                    return Ok(server.exit_code());
                }
            }
            // A frame that is not a message is reported and the session goes
            // on; the specification wants a parse error, not a dropped client.
            Err(ReadError::Malformed(message)) => {
                jsonrpc::write(
                    output,
                    &Notification::new(
                        "window/logMessage",
                        serde_json::to_value(ShowMessageParams {
                            kind: MessageType::Error,
                            message: format!("could not read a message: {message}"),
                        })
                        .unwrap_or(Value::Null),
                    ),
                )?;
            }
            // A client that closes the pipe without `exit` has abandoned the
            // session, which the specification treats as a failure.
            Err(ReadError::Eof) => return Ok(1),
            Err(ReadError::Io(error)) => return Err(error),
        }
    }
}

/// `liyasa lsp`'s whole body. See
/// `plan/rfcs/3001-liyasa-lsp-is-wp-09-s-command.md` for the three lines that
/// wire it up, which belong to `crates/liyasa-cli/`.
pub fn serve_stdio() -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    match serve(&mut input, &mut output)? {
        0 => Ok(()),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            "the editor closed the connection without `shutdown`",
        )),
    }
}
