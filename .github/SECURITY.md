# Security policy

Liyasa builds documentation sites from untrusted content, untrusted templates
and, in server mode, untrusted requests. A bug that lets any of those three
reach further than it should is a security bug, and this page says how to report
one and what happens next.

## Reporting a vulnerability

Use GitHub's **private vulnerability reporting** on this repository: the
Security tab, then "Report a vulnerability". It opens a private thread with the
maintainers; nothing is public until there is a fix.

Please do not open a public issue for a vulnerability, and please do not post a
proof of concept anywhere public before a release carries the fix.

A report is most useful with:

- the version, or the commit, and how Liyasa was running (`build`, `dev`, the
  server, or the editor's WebAssembly preview);
- the smallest input that shows it — a page, a config, a request;
- what an attacker gets out of it.

## What to expect

| Step | Target |
|---|---|
| Acknowledgement | 3 working days |
| First assessment, with a severity | 10 working days |
| Fix released, or a written plan and a date | 90 days |

If a report goes unanswered past those windows, it is a failure of this process
and you are free to disclose.

Coordinated disclosure is the default: the advisory, the CVE where one applies,
and the credit go out together with the release that fixes it. Reporters are
credited by name unless they ask not to be.

There is no bug bounty today. NFR-14 commits to one when the hosted service
reaches general availability, and this page will say so when it exists rather
than implying it in the meantime.

## Scope

In scope, in roughly the order they matter:

- **Untrusted content.** Cross-site scripting through Markdown, embedded HTML,
  or a component's props, and anything that escapes the sanitizer's allow list.
- **Untrusted templates.** Escape from the sandboxed minijinja environment, or
  a way past its resource limits.
- **Untrusted code in verification.** Code from a page reaching the host, the
  network, or another run's files.
- **Prompt injection reaching a model**, where it crosses a trust level or
  leaks a document the reader could not otherwise read.
- **The agent**: a tool call outside its declared privileges, a secret in a
  prompt or a log, a budget that does not bound.
- **Outbound requests**: server-side request forgery past the policy in
  `liyasa-net`.
- **Multi-tenancy**: reading or writing another project's rows, or another
  tenant's secrets.
- **Authentication**: session fixation, cross-site request forgery on a
  state-changing route, a token that outlives its revocation.
- **Supply chain**: a dependency or a bundled asset that is not what the lock
  file says it is.

Out of scope:

- findings from a scanner with no demonstrated impact;
- missing hardening headers on a page that carries no privilege;
- rate limits on an endpoint a self-hoster has deliberately opened;
- social engineering, physical access, or denial of service by volume alone;
- vulnerabilities in a dependency that are already public and already tracked
  by `cargo deny` — report those upstream, and tell us if we are slow to pick
  the fix up.

## What the project does on its side

- `cargo deny` runs on every pull request: advisories, licences, bans and
  sources.
- Unsafe code is forbidden for the whole workspace, and
  `cargo run -p xtask -- lints` checks that no crate has quietly opted out of
  the lint by leaving it out of its manifest.
- Builds are deterministic: every pull request builds the reference site twice
  in one process and diffs it, and builds it again on a second operating
  system and diffs the two. That does not by itself prove an artifact is
  untampered — it is what makes rebuilding one and comparing meaningful.
- Dependency updates are grouped and monthly rather than continuous, so a
  security update is not the fortieth pull request in a queue nobody reads.

A documented threat model, signed releases and an SBOM are committed to and not
yet built. This page will describe them when they exist rather than before.
