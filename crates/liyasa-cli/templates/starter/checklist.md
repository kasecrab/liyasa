---
title: Checklist
description: What to change before this scaffold becomes your documentation site.
---

# Checklist

Work down this list and delete each line as you go.

## Make it yours

- Set `name` and `description` in `liyasa.json`.
- Set `seo.canonicalOrigin` to the URL this site will actually be served from.
- Pick a `theme.preset`: `aurora`, `atlas`, `meadow`, `slate`, `ember`,
  `harbor`, `quill`, `signal`, or `lumen`.
- Replace `index.md` with your own landing page.

## Replace the samples

- `guides/quickstart.md` is a worked example. Rewrite it for your product.
- `openapi/api.yaml` is a toy specification. Point `openapi[].source` at yours.
- `facts/pricing.json` and `facts/sources.toml` are a sample fact and its
  source. Delete them if you are not verifying facts yet.
- `variables` in `liyasa.json` holds values reused across pages as `vars.*`.

## Before you deploy

- `liyasa validate` reports nothing.
- `liyasa test --agents` passes, so machines can read the site as well as
  people.
- `liyasa build` succeeds and `dist/` looks right served locally.
- Decide where it is hosted. The static output works on GitHub Pages,
  Cloudflare Pages, Netlify, Vercel, and any web server.

## Useful commands

| Command | What it does |
|---|---|
| `liyasa dev` | Live-reloading preview |
| `liyasa build` | Production build into `dist/` |
| `liyasa validate` | Config, content, links, specs |
| `liyasa verify` | Re-check the claims |
| `liyasa score` | Quality score with the top things to fix |
| `liyasa doctor` | What this machine can do |
