---
title: Configuration
description: "Every key of liyasa.json, generated from the published JSON Schema."
---

# Configuration

A site is configured by one file, `liyasa.json`, at the project root. Every key below is generated from `schemas/liyasa.schema.json`, which the build validates against and editors autocomplete from.

```json
{
  "$schema": "https://kasecrab.github.io/liyasa/schema/v1/liyasa.schema.json",
  "name": "Acme docs",
  "seo": { "canonicalOrigin": "https://docs.acme.com" }
}
```

Pointing `$schema` at the published URL is what gives you completion and inline validation in an editor. `liyasa schema config` prints the same document for a local copy.

| Section | What it configures |
|---|---|
| [`agents`](/reference/config/agents) | Agent surfaces: llms.txt, Markdown output, skills, MCP (§24, §25). |
| [`ai`](/reference/config/ai) | Model routing and assistant, agent, and reindex settings. |
| [`analytics`](/reference/config/analytics) | Collection, retention, and bot classification (§26). |
| [`api`](/reference/config/api) | Manual API pages (API-20). |
| [`asyncapi`](/reference/config/asyncapi) | AsyncAPI specifications bound to this site. |
| [`auth`](/reference/config/auth) | Authentication modes and session policy (§19). |
| [`authors`](/reference/config/authors) | Author handles resolved from front matter `authors`. |
| [`automations`](/reference/config/automations) | Automation definitions and untrusted-run budgets. |
| [`banner`](/reference/config/banner) | Site-wide banner (§8.8). |
| [`build`](/reference/config/build) | Build outputs, budgets, caps, and determinism inputs (§6.6). |
| [`content`](/reference/config/content) | Markdown, templating, and image behaviour (§8.9). |
| [`contextRepos`](/reference/config/contextRepos) | CFG-99, GIT-11 |
| [`description`](/reference/config/description) | The `description` setting. |
| [`dimensions`](/reference/config/dimensions) | Custom content dimensions such as product (§7.12). |
| [`editor`](/reference/config/editor) | Editor settings (§15). |
| [`errors`](/reference/config/errors) | Error page behaviour (§8.8). |
| [`favicon`](/reference/config/favicon) | The `favicon` setting. |
| [`feeds`](/reference/config/feeds) | Changelog and update feeds. |
| [`footer`](/reference/config/footer) | Footer socials, link columns, branding, and legal line (§8.5). |
| [`graphql`](/reference/config/graphql) | GraphQL schemas bound to this site. |
| [`integrations`](/reference/config/integrations) | Third-party scripts and consent (§26.8). |
| [`locales`](/reference/config/locales) | Languages this site is published in (§7.11). |
| [`localization`](/reference/config/localization) | Locale fallback and visitor routing (§7.11). |
| [`logo`](/reference/config/logo) | The `logo` setting. |
| [`name`](/reference/config/name) | Site name. The only required key. |
| [`navbar`](/reference/config/navbar) | Top navigation bar (§8.3). |
| [`navigation`](/reference/config/navigation) | The navigation tree, a file that holds it, or an object carrying tree options (§8.4). |
| [`network`](/reference/config/network) | Outbound network allow lists, consumed by `liyasa-net` (§30.2.3). |
| [`openapi`](/reference/config/openapi) | OpenAPI specifications bound to this site (§13). |
| [`pageActions`](/reference/config/pageActions) | Copy, view, and open-in-assistant actions (§8.8). |
| [`playground`](/reference/config/playground) | API playground behaviour (§13). |
| [`public`](/reference/config/public) | `false` requires an access mode under `auth` and a server; rejected with E0120 before 0.5. |
| [`redirects`](/reference/config/redirects) | Redirect rules, and the hosts an absolute destination may point at (E0109). |
| [`regions`](/reference/config/regions) | Region gating (§19.6). |
| [`root`](/reference/config/root) | Content root, relative to the config file. |
| [`search`](/reference/config/search) | Search behaviour and the browser index's shard sizing (§8.6). |
| [`security`](/reference/config/security) | Content Security Policy, frame ancestors, and upload rules (§30.2). |
| [`seo`](/reference/config/seo) | Meta tags, JSON-LD, indexing, sitemap, robots, and crawler directives (§8.7). |
| [`server`](/reference/config/server) | Server-mode settings that are not secrets; only meaningful to `liyasa serve`. |
| [`skills`](/reference/config/skills) | Agent skill files and groups (§25). |
| [`social`](/reference/config/social) | Open Graph thumbnail generation (§8.8). |
| [`theme`](/reference/config/theme) | Presets, colours, fonts, icons, appearance, layout, and overrides (§8.2). |
| [`variables`](/reference/config/variables) | Site-wide template variables, available as `vars.<key>` (§7.8). |
| [`variations`](/reference/config/variations) | Named content variations a page may belong to. |
| [`verify`](/reference/config/verify) | Verification engine settings (§14). |
| [`versions`](/reference/config/versions) | Documentation versions (§7.10). |
