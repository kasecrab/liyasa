---
title: security
description: "Content Security Policy, frame ancestors, and upload rules (§30.2)."
sidebarTitle: security
---

# `security`

Content Security Policy, frame ancestors, and upload rules (§30.2).

Specified by CFG-85.

| Key | Type | Default | What it does |
|---|---|---|---|
| `security.csp.extraFrameSrc` | string[] | — | Extra sources that may be framed in a page. |
| `security.csp.extraImgSrc` | string[] | — | Extra sources images may load from. |
| `security.csp.extraScriptSrc` | string[] | — | Extra sources scripts may load from. |
| `security.csp.imgHostsAllow` | string[] | — | Remote image hosts a page may load from, added to `img-src`. A host listed here also stops the build warning about the remote image it serves. |
| `security.csp.reportUri` | string | — | Where the browser posts a report when the policy blocks something. |
| `security.frameAncestors` | string[] | — | Hosts that may embed this site's pages in a frame. Empty, none may. |
| `security.hstsPreload` | boolean | `false` | Add `preload` to `Strict-Transport-Security` (RX-112). Off by default: preload is a one-way submission to browser lists. |
| `security.styleAttribute` | `allowlist` \| `off` | — | What happens to a `style` attribute in content: `allowlist` keeps the declarations that cannot load or leak anything, `off` removes the attribute. |
| `security.txt` | boolean \| object | — | RFC 9116 `security.txt` fields, emitted at `/.well-known/security.txt`. |
| `security.uploads.allowTypes` | string[] | — | Media types accepted. Anything else is refused at upload rather than served later. |
| `security.uploads.svg` | `sanitize` \| `attachment` | — | `sanitize` strips the script and external references out of an uploaded SVG; `attachment` serves it as a download so no browser executes it. |

Every key above is generated from `schemas/liyasa.schema.json`, which is the single source of truth for configuration: a key that is not in the schema is [`E0103`](/errors/E0103), and a value that does not match it is [`E0102`](/errors/E0102).

See [the configuration reference](/reference/config) for the other sections.
