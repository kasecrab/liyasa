// The whole extension: start the server, and hold a webview for its preview.
//
// Everything an author sees comes from the server over LSP. The extension owns
// no knowledge of Liyasa Markdown — no second component list, no second set of
// diagnostics — because a copy here would fall behind the crate that renders
// the site.

const { workspace, window, commands, ViewColumn } = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

/** @type {LanguageClient | undefined} */
let client;
/** The language identifiers this extension serves. */
const SERVED = new Set(["markdown", "liyasa-markdown"]);

/** @type {import("vscode").WebviewPanel | undefined} */
let preview;

function serverOptions() {
  const settings = workspace.getConfiguration("liyasa");
  const command = settings.get("server.path", "liyasa");
  const args = settings.get("server.args", ["lsp"]);
  const run = { command, args, transport: TransportKind.stdio };
  return { run, debug: run };
}

async function start(context) {
  client = new LanguageClient(
    "liyasa",
    "Liyasa Markdown",
    serverOptions(),
    {
      // `.md` is VS Code's own `markdown` language; only `.mdx` is ours.
      // Selecting both is what makes an ordinary page served at all.
      documentSelector: [
        { scheme: "file", language: "markdown" },
        { scheme: "file", language: "liyasa-markdown" },
      ],
      synchronize: {
        // The server reads these to index components, variables, facts,
        // snippets and routes; a change to one changes what it can offer.
        fileEvents: workspace.createFileSystemWatcher(
          "**/{liyasa.json,facts/**,snippets/**}"
        ),
      },
      outputChannel: window.createOutputChannel("Liyasa"),
    }
  );
  try {
    await client.start();
  } catch (error) {
    client = undefined;
    const settings = workspace.getConfiguration("liyasa");
    window.showErrorMessage(
      `Liyasa: could not start the language server ` +
        `\`${settings.get("server.path", "liyasa")} ` +
        `${settings.get("server.args", ["lsp"]).join(" ")}\`. ` +
        `Set liyasa.server.path and liyasa.server.args, then run ` +
        `"Liyasa: Restart Language Server". (${error.message})`
    );
    return;
  }
  context.subscriptions.push(client);
}

async function openPreview() {
  const editor = window.activeTextEditor;
  if (!editor || !client) {
    return;
  }
  if (!preview) {
    preview = window.createWebviewPanel(
      "liyasa.preview",
      "Liyasa Preview",
      ViewColumn.Beside,
      { enableScripts: false }
    );
    preview.onDidDispose(() => {
      preview = undefined;
    });
  }
  await refresh(editor.document);
}

async function refresh(document) {
  if (!preview || !client || !SERVED.has(document.languageId)) {
    return;
  }
  // `liyasa/preview` is this server's one method outside the standard: it
  // returns the HTML the build would serve for the buffer as it stands, and
  // the diagnostics that render raised.
  const result = await client.sendRequest("liyasa/preview", {
    textDocument: { uri: document.uri.toString() },
  });
  if (!result) {
    return;
  }
  const errors = (result.diagnostics || []).filter((d) => d.severity === 1);
  preview.title = errors.length
    ? `Liyasa Preview (${errors.length} error${errors.length > 1 ? "s" : ""})`
    : "Liyasa Preview";
  preview.webview.html = page(result.html, errors);
}

// A page with no script and no remote content. The preview shows what the build
// would serve; it does not run it.
function page(html, errors) {
  const banner = errors.length
    ? `<ul class="errors">${errors
        .map((d) => `<li><code>${escape(d.code)}</code> ${escape(d.message)}</li>`)
        .join("")}</ul>`
    : "";
  return `<!doctype html>
<html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy"
      content="default-src 'none'; style-src 'unsafe-inline'; img-src data:;">
<style>
  body { font: 15px/1.6 var(--vscode-font-family); padding: 1rem 1.5rem; }
  .errors { background: var(--vscode-inputValidation-errorBackground);
            border-left: 3px solid var(--vscode-errorForeground);
            padding: .5rem 1rem; margin: 0 0 1rem; list-style: none; }
  pre { overflow-x: auto; }
</style></head>
<body>${banner}${html}</body></html>`;
}

function escape(text) {
  return String(text).replace(/[&<>"]/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]
  );
}

async function activate(context) {
  // Commands first: a server that will not start must still leave the author a
  // way to retry once they have pointed the setting at one that does.
  context.subscriptions.push(
    commands.registerCommand("liyasa.preview", openPreview),
    commands.registerCommand("liyasa.restart", async () => {
      await deactivate();
      await start(context);
    }),
    workspace.onDidChangeTextDocument((event) => refresh(event.document)),
    window.onDidChangeActiveTextEditor((editor) => {
      if (editor) {
        refresh(editor.document);
      }
    })
  );
  await start(context);
}

async function deactivate() {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

module.exports = { activate, deactivate, page };
