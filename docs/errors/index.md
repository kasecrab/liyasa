---
title: Error codes
description: "Every diagnostic Liyasa can print, by range, with the page that explains it."
---

# Error codes

Every user-facing failure in Liyasa is a diagnostic with a code, a severity, and a page. Codes beginning `E` are errors and codes beginning `W` are warnings; a policy may promote or demote one within the limits its registry row allows.

The code in a terminal is also a link: every diagnostic carries the URL of its page, so `liyasa build --json` gives a machine the same reference a person gets.

:::tip{title="Looking for the cause rather than the code?"}
[The help center](/help) is organised by what went wrong rather than by number.
:::

## CLI and I/O

`0001`–`0099`, raised by `liyasa-cli`.

| Code | Severity | Meaning |
|---|---|---|
| [`E0001`](/errors/E0001) | Error | Project root not found (no `liyasa.json` in this or any parent directory) |
| [`E0002`](/errors/E0002) | Error | Cannot read file (permission or encoding) |
| [`E0003`](/errors/E0003) | Error | Companion runtime required for this feature and not installed |
| [`E0004`](/errors/E0004) | Error | Sandbox (Docker or Podman) required and not available |

## Configuration

`0100`–`0199`, raised by `liyasa-config`.

| Code | Severity | Meaning |
|---|---|---|
| [`E0101`](/errors/E0101) | Error | `liyasa.json` is not valid JSON |
| [`E0102`](/errors/E0102) | Error | Config does not match schema (path, expected, found) |
| [`E0103`](/errors/E0103) | Error | Unknown config key |
| [`E0104`](/errors/E0104) | Error | Navigation references a page that does not exist |
| [`E0105`](/errors/E0105) | Error | Duplicate route |
| [`E0106`](/errors/E0106) | Error | Conflicting redirects |
| [`E0107`](/errors/E0107) | Error | Colour fails the contrast check |
| [`E0108`](/errors/E0108) | Error | No default version or locale declared |
| [`E0109`](/errors/E0109) | Error | Redirect destination is absolute and not in `redirects.externalAllow`, or a parameter appears in its host |
| [`E0110`](/errors/E0110) | Error | Config key not in the schema (`schemas/liyasa.schema.json` is the source of truth) |
| [`E0120`](/errors/E0120) | Error | `public: false` requires `liyasa serve` (available from 0.5) |
| [`E0121`](/errors/E0121) | Error | Config schema version newer than this CLI |
| [`W0130`](/errors/W0130) | Warning | Page not reachable from navigation |
| [`W0131`](/errors/W0131) | Warning | No `seo.canonicalOrigin`; absolute URLs cannot be generated |
| [`E0132`](/errors/E0132) | Error | Colour value is not one Liyasa can read |
| [`E0133`](/errors/E0133) | Error | Navigation binds a subtree to a version, locale, product, or spec that is not declared |
| [`W0134`](/errors/W0134) | Warning | Trust-plane section read from the deploy branch, not from the branch being built |
| [`E0135`](/errors/E0135) | Error | Remote spec source is not one the deploy branch's config names |

## Templating and Source Document

`0200`–`0299`, raised by `liyasa-markdown`.

| Code | Severity | Meaning |
|---|---|---|
| [`E0201`](/errors/E0201) | Error | Undefined template variable |
| [`E0202`](/errors/E0202) | Error | Template syntax error |
| [`E0203`](/errors/E0203) | Error | Unknown filter or function |
| [`E0204`](/errors/E0204) | Error | Template budget exceeded (time, iterations, output size, depth) |
| [`E0205`](/errors/E0205) | Error | Include or snippet not found |
| [`E0206`](/errors/E0206) | Error | Snippet cycle |
| [`E0207`](/errors/E0207) | Error | Required snippet prop missing or wrong type |
| [`E0208`](/errors/E0208) | Error | `reader.*` used on a page without `personalized: true` |
| [`E0209`](/errors/E0209) | Error | Fact referenced in template does not exist |
| [`E0210`](/errors/E0210) | Error | Template block statement is not well-formed with respect to Markdown structure |
| [`E0211`](/errors/E0211) | Error | `env()` used with a variable not allow-listed in `build.env` |
| [`E0212`](/errors/E0212) | Error | Private-use Unicode character in content (reserved for expansion sentinels) |
| [`E0213`](/errors/E0213) | Error | `link()` or `page()` names a page that does not exist |
| [`E0214`](/errors/E0214) | Error | `asset()` names a file the build did not produce |
| [`E0215`](/errors/E0215) | Error | `openapi()` names a spec or an operation that does not exist |
| [`E0216`](/errors/E0216) | Error | Template asked for a build value this build did not supply (`now()`, `region_available()`) |

## Markdown, directives

`0300`–`0349`, raised by `liyasa-markdown`.

| Code | Severity | Meaning |
|---|---|---|
| [`E0301`](/errors/E0301) | Error | Unclosed code fence |
| [`W0302`](/errors/W0302) | Warning | Unknown code fence attribute |
| [`E0303`](/errors/E0303) | Error | Raw HTML disabled by config |
| [`E0304`](/errors/E0304) | Error | HTML element or attribute not allowed by the sanitizer |
| [`E0305`](/errors/E0305) | Error | Image without alt text |
| [`W0306`](/errors/W0306) | Warning | Heading level skipped |
| [`E0307`](/errors/E0307) | Error | Page Markdown exceeds 100,000 characters |
| [`W0308`](/errors/W0308) | Warning | Page Markdown exceeds 50,000 characters |
| [`E0310`](/errors/E0310) | Error | Container directive not closed |
| [`E0311`](/errors/E0311) | Error | Directive close without matching open |
| [`E0312`](/errors/E0312) | Error | Directive prop syntax error |
| [`E0313`](/errors/E0313) | Error | Unknown component (with suggestion) |
| [`E0314`](/errors/E0314) | Error | Missing required prop |
| [`E0315`](/errors/E0315) | Error | Prop type mismatch |
| [`W0316`](/errors/W0316) | Warning | Unknown prop |
| [`E0317`](/errors/E0317) | Error | Directive inside an inline context |
| [`E0318`](/errors/E0318) | Error | Explicit block ID duplicated on the page |
| [`W0319`](/errors/W0319) | Warning | Literal directive marker prefix (`<!--ly:`) found in source; escaped |
| [`E0320`](/errors/E0320) | Error | Untrusted (below `operator`) value contains a line break and was rejected from interpolation |
| [`W0321`](/errors/W0321) | Warning | Machine-generated bulk elements dominate an oversized page (spec check `embedded-data-serialization`) |
| [`E0322`](/errors/E0322) | Error | Page nests blocks or inlines deeper than the parser will walk |

## Components

`0350`–`0399`, raised by `liyasa-components`.

| Code | Severity | Meaning |
|---|---|---|
| [`E0350`](/errors/E0350) | Error | Component slot not recognized |
| [`E0351`](/errors/E0351) | Error | User component template error |
| [`E0352`](/errors/E0352) | Error | User component prop schema invalid |
| [`E0353`](/errors/E0353) | Error | Component prop URL uses a scheme the sanitizer does not permit |
| [`E0354`](/errors/E0354) | Error | Component child not allowed by the parent component |
| [`W0355`](/errors/W0355) | Warning | Component prop ignored because another prop takes precedence |
| [`E0356`](/errors/E0356) | Error | User component file is not a valid component definition |
| [`E0357`](/errors/E0357) | Error | Embed provider is not in the allow list |
| [`W0358`](/errors/W0358) | Warning | Component prop value is outside the range the schema documents |

## Navigation and links

`0400`–`0499`, raised by `liyasa-build`.

| Code | Severity | Meaning |
|---|---|---|
| [`E0401`](/errors/E0401) | Error | Broken internal link |
| [`E0402`](/errors/E0402) | Error | Broken heading anchor |
| [`E0403`](/errors/E0403) | Error | Missing image or asset |
| [`W0404`](/errors/W0404) | Warning | External link unreachable at build time (scheduled checks escalate to drift) |
| [`W0405`](/errors/W0405) | Warning | Link text is non-descriptive |
| [`W0406`](/errors/W0406) | Warning | Link in Markdown output is not absolute; agent pipelines lose the base URL |
| [`E0407`](/errors/E0407) | Error | Generated agent resource declares its continuation as a trailing note instead of an opening header |
| [`W0408`](/errors/W0408) | Warning | Custom `llms.txt` links to a route the build cannot resolve |
| [`W0409`](/errors/W0409) | Warning | `llms.txt` does not cover every indexable page |
| [`W0410`](/errors/W0410) | Warning | Implemented spec check set differs from the tracked `agents.specVersion` |
| [`W0411`](/errors/W0411) | Warning | Agent-readiness run reports an interaction effect rather than the underlying check failures |
| [`W0412`](/errors/W0412) | Warning | Agent-readiness run computed from a partial sample; more than 20% of page fetches failed |

## OpenAPI and API docs

`0500`–`0599`, raised by `liyasa-openapi`.

| Code | Severity | Meaning |
|---|---|---|
| [`E0501`](/errors/E0501) | Error | OpenAPI document invalid (JSON pointer) |
| [`E0502`](/errors/E0502) | Error | Unresolvable `$ref` |
| [`E0503`](/errors/E0503) | Error | Spec source unreachable (with DNS, TLS, or policy detail) |
| [`E0504`](/errors/E0504) | Error | Unsupported OpenAPI version |
| [`E0505`](/errors/E0505) | Error | Duplicate operation ID |
| [`E0506`](/errors/E0506) | Error | Operation referenced in navigation not found in spec |
| [`E0507`](/errors/E0507) | Error | Overlay failed to apply |
| [`E0508`](/errors/E0508) | Error | `allOf` members conflict (type, enum, or bounds) |
| [`W0509`](/errors/W0509) | Warning | Swagger 2.0 document converted to OpenAPI 3.1 |
| [`W0510`](/errors/W0510) | Warning | Operation has no example for its request or response body |
| [`W0511`](/errors/W0511) | Warning | Spec feature not rendered by this release |
| [`W0512`](/errors/W0512) | Warning | Overlay action matched nothing in the spec |
| [`W0513`](/errors/W0513) | Warning | Manual API page differs from the spec that describes the same path |

## Verification and facts

`0600`–`0699`, raised by `liyasa-verify`.

| Code | Severity | Meaning |
|---|---|---|
| [`E0601`](/errors/E0601) | Error | Verification check failed |
| [`E0602`](/errors/E0602) | Error | Runner not available for language |
| [`E0603`](/errors/E0603) | Error | Runner timed out |
| [`E0604`](/errors/E0604) | Error | Fact source refresh failed |
| [`E0605`](/errors/E0605) | Error | Fact schema validation failed |
| [`E0606`](/errors/E0606) | Error | Manual attestation expired |
| [`E0607`](/errors/E0607) | Error | Drift detected (severity in payload) |
| [`E0608`](/errors/E0608) | Error | Screenshot mismatch beyond tolerance |
| [`E0620`](/errors/E0620) | Error | `local` sandbox rejected by the server |
| [`E0621`](/errors/E0621) | Error | `command` source not in the server allow list or hash mismatch |
| [`W0622`](/errors/W0622) | Warning | Verification deploy budget exceeded; remaining checks queued |
| [`W0630`](/errors/W0630) | Warning | Page has no `description` |
| [`W0631`](/errors/W0631) | Warning | Prose lint rule matched |
| [`W0632`](/errors/W0632) | Warning | Word is not in the project dictionary |
| [`E0633`](/errors/E0633) | Error | Prose rule package could not be read |
| [`E0634`](/errors/E0634) | Error | `verify.policy` names a check class Liyasa does not know |
| [`E0635`](/errors/E0635) | Error | `verify` setting is not a value Liyasa can read |
| [`W0636`](/errors/W0636) | Warning | Prose rules Liyasa does not implement did not run |

## Build, cache, assets, determinism

`0700`–`0799`, raised by `liyasa-build`.

| Code | Severity | Meaning |
|---|---|---|
| [`E0701`](/errors/E0701) | Error | Build failed (aggregate) |
| [`W0702`](/errors/W0702) | Warning | Cache corrupted; rebuilt |
| [`E0703`](/errors/E0703) | Error | Asset processing failed |
| [`E0704`](/errors/E0704) | Error | Font download failed at build |
| [`E0705`](/errors/E0705) | Error | Template budget exceeded for the build |
| [`E0706`](/errors/E0706) | Error | Non-deterministic output detected by `--check-determinism` |
| [`W0707`](/errors/W0707) | Warning | Build clock fell back to the wall clock; build is not reproducible |
| [`W0710`](/errors/W0710) | Warning | Page exceeds the per-page variant cap and is marked dynamic |
| [`E0711`](/errors/E0711) | Error | Site exceeds the total variant cap |
| [`E0712`](/errors/E0712) | Error | Variant discovery did not converge within `build.variantDiscoveryIterations` |
| [`W0713`](/errors/W0713) | Warning | Page has no `id`; block references will be keyed by route until one is added |
| [`W0714`](/errors/W0714) | Warning | Image dimensions unknown; layout shift possible |
| [`W0715`](/errors/W0715) | Warning | Page is rendered on demand because it reads free-form `reader.*` fields |
| [`W0716`](/errors/W0716) | Warning | `theme.fonts.subset` requested but subsetting is not available in this release |
| [`E0717`](/errors/E0717) | Error | Dynamic include or snippet name on a page that is not rendered on demand |
| [`W0718`](/errors/W0718) | Warning | Build cache invalidated because an allow-listed environment variable's value changed |
| [`W0719`](/errors/W0719) | Warning | New remote image or media host added to the CSP from content |
| [`W0720`](/errors/W0720) | Warning | Served HTML response exceeds 1 MB (spec check `page-size-transfer` warn band) |
| [`E0721`](/errors/E0721) | Error | Served HTML response exceeds 10 MB, above documented agent fetch-buffer caps |
| [`E0722`](/errors/E0722) | Error | CSP source in `security.csp` or `network.allowHosts.embeds` is not a source expression |

## Server, auth, deployments, network

`0800`–`0899`, raised by `liyasa-server`.

| Code | Severity | Meaning |
|---|---|---|
| [`E0801`](/errors/E0801) | Error | Domain verification failed |
| [`E0802`](/errors/E0802) | Error | Certificate issuance failed |
| [`E0803`](/errors/E0803) | Error | Auth provider configuration invalid |
| [`E0804`](/errors/E0804) | Error | Deployment failed |
| [`E0805`](/errors/E0805) | Error | Rollback target not retained |
| [`E0806`](/errors/E0806) | Error | Outbound request blocked by network policy (host, address class, or redirect) |
| [`E0807`](/errors/E0807) | Error | Rate limit exceeded (returned as 429) |
| [`E0808`](/errors/E0808) | Error | Webhook signature, timestamp, or delivery ID rejected |
| [`E0809`](/errors/E0809) | Error | Build queue full; job rejected or deferred |
| [`E0810`](/errors/E0810) | Error | Dynamic page render exceeded the request-path budget; default variant served |
| [`W0811`](/errors/W0811) | Warning | Ingest queue dropped events |
| [`E0812`](/errors/E0812) | Error | Uploaded asset rejected (type not allowed or SVG sanitization failed) |
| [`W0813`](/errors/W0813) | Warning | Bot-protection interference observed during a sustained agent-readiness scan |

## AI, agent, automations

`0900`–`0999`, raised by `liyasa-ai`.

| Code | Severity | Meaning |
|---|---|---|
| [`E0901`](/errors/E0901) | Error | Model provider error |
| [`E0902`](/errors/E0902) | Error | Model budget exceeded for the run |
| [`E0903`](/errors/E0903) | Error | Agent tool rejected by policy (trust level) |
| [`E0904`](/errors/E0904) | Error | Proposal rejected by output gate (reason) |
| [`E0905`](/errors/E0905) | Error | Automation trigger payload failed validation |
| [`W0906`](/errors/W0906) | Warning | Embedding model changed; full re-index scheduled |

## Search and the browser index

`1000`–`1099`, raised by `liyasa-search`.

| Code | Severity | Meaning |
|---|---|---|
| [`W1001`](/errors/W1001) | Warning | Locale has no Snowball algorithm; indexed without stemming |
| [`E1002`](/errors/E1002) | Error | Search index format is newer than this reader understands |
| [`E1003`](/errors/E1003) | Error | Search index is truncated or corrupt |
| [`E1004`](/errors/E1004) | Error | Search query is not well-formed (unbalanced quote or unknown field filter) |
| [`W1005`](/errors/W1005) | Warning | `search.boost` or `search.exclude` pattern matched no route |
| [`E1006`](/errors/E1006) | Error | CJK dictionary not installed for this locale (`liyasa add dictionary <lang>`) |

