# liyasa-lsp

The Liyasa Markdown language server (PRD §15.7 ED-61) and the VS Code and
Cursor extension that drives it.

## What it answers

| Request | What it uses |
|---|---|
| `textDocument/publishDiagnostics` | the scanner, the expander and the parser's component validation, with the `codes.toml` code and its help URL |
| `textDocument/completion` | components and props from the registry, variables and facts from the project, snippets and routes from the tree |
| `textDocument/hover` | a component's own prop schema, a fact's own value, a page's own title |
| `textDocument/definition` | snippets and facts, narrowed to the line that declares the key; routes to the page |
| `liyasa/preview` | `liyasa_build::render::page` — the HTML the build would serve |

Everything comes from the code that builds the site. The server holds no second
component list, no second set of diagnostics, and no second renderer.

## `liyasa lsp` is not wired up yet

The command CLI-25 specifies does not exist. `crates/liyasa-cli/` belongs to
WP-09, and `plan/rfcs/3001-liyasa-lsp-is-wp-09-s-command.md` records why this
package did not edit it. Three additions wire it, and they are the whole of it:

1. In `crates/liyasa-cli/Cargo.toml`, a dependency row:

   ```toml
   liyasa-lsp = { path = "../liyasa-lsp", version = "0.1.0" }
   ```

2. In `crates/liyasa-cli/src/cli.rs`, a variant on the command enum:

   ```rust
   /// Language server for editors.
   Lsp,
   ```

3. In whichever `match` dispatches a command to its module, an arm:

   ```rust
   Command::Lsp => liyasa_lsp::serve_stdio().map(|()| ExitCode::Success),
   ```

`serve_stdio` takes no arguments, reads `stdin`, writes `stdout`, and writes
nothing to `stdout` that is not a protocol message. It returns `Err` with
`ErrorKind::ConnectionAborted` when the editor closes the connection without
`shutdown`, which is the LSP specification's failure case and CLI-31's exit
code 1.

## The extension

`editors/vscode/` is a self-contained VS Code extension; Cursor loads the same
package. It is a thin client by design — it starts the server, forwards the
buffer, and holds a webview for `liyasa/preview`. It knows nothing about
Liyasa Markdown itself.

```
cd crates/liyasa-lsp/editors/vscode
npm install
npx vsce package
```

Until `liyasa lsp` exists, point the extension at a binary that serves the
protocol by setting `liyasa.server.path` and `liyasa.server.args`.
