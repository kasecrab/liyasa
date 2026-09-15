# Configuration

A site is configured by one file, `liyasa.json`, at the project root. Every key
is documented by a published JSON Schema, so an editor with schema support
completes and validates the file as you type.

```json
{
  "$schema": "https://kasecrab.github.io/liyasa/schema/v1/liyasa.schema.json",
  "name": "Acme Docs",
  "theme": { "preset": "aurora" },
  "navigation": { "tabs": [{ "title": "Guides", "groups": [] }] }
}
```

## Where values come from

Configuration resolves in a fixed order, and the first source that sets a key
wins. Knowing the order matters when a value looks wrong: it is almost always
being set somewhere earlier than you are looking.

| Order | Source | Typical use |
|---|---|---|
| 1 | Command-line flags | One-off overrides in CI |
| 2 | Environment variables | Secrets and deployment targets |
| 3 | `liyasa.local.json` | A single developer's preferences |
| 4 | `liyasa.json` | The project's own configuration |
| 5 | Built-in defaults | Everything you did not set |

An unknown key is a warning rather than an error, and the warning names the
closest key it knows. A key with the wrong type is an error with the span of
the offending value, because a silently ignored type is how a site ends up
configured differently from how it reads.

## Site identity

`name`, `description`, and `seo.canonicalOrigin` describe the site. The
canonical origin is the one key worth setting before anything else: without it
the build cannot generate absolute URLs, so Open Graph tags, `llms.txt`, and
the Markdown links an agent follows all degrade to relative paths.

```json
{
  "name": "Acme Docs",
  "description": "Guides and API reference for the Acme platform.",
  "seo": { "canonicalOrigin": "https://docs.acme.example" }
}
```

## Theme

`theme.preset` selects one of the nine bundled presets. Every other key under
`theme` overrides a token in the preset, so a brand colour is one line rather
than a stylesheet.

```json
{
  "theme": {
    "preset": "aurora",
    "colors": { "primary": "#3b5bdb" },
    "fonts": { "body": "Inter", "mono": "JetBrains Mono" },
    "appearance": { "default": "system" }
  }
}
```

Set `theme.appearance.strict` when a site must render in one scheme only. The
toggle disappears and the module that would have wired it up is left out of
the bundle, which is a few hundred bytes a reader no longer downloads.

## Navigation

`navigation` describes the sidebar, the tabs above it, and the order pages
appear in. Groups nest, items carry icons and tags, and a subtree can be bound
to a version, a locale, a product, or an API specification.

```json
{
  "navigation": {
    "tabs": [
      {
        "title": "Guides",
        "groups": [
          {
            "title": "Get started",
            "expanded": true,
            "items": ["/guide/install", "/guide/configuration"]
          }
        ]
      }
    ]
  }
}
```

Previous and next links are computed from this order across groups and tabs,
so a page never has to declare its own neighbours.

## Content

`content` controls what the Markdown parser accepts. Math and raw HTML are off
by default; wiki links and definition lists are on.

| Key | Default | What it does |
|---|---|---|
| `content.math` | `false` | `$…$` and `$$…$$` render to MathML |
| `content.html` | `"allowlist"` | Raw HTML: `off`, `allowlist`, or `all` |
| `content.wikilinks` | `true` | `[[Page]]` resolves against the route table |
| `content.codeTheme` | `"auto"` | Syntax highlighting theme per scheme |

`content.html` deserves a moment. `allowlist` keeps the elements and
attributes a documentation page needs and drops the rest, including every
event handler and every `javascript:` URL. `off` strips raw HTML entirely.
`all` disables the sanitizer and is only sensible when every author of every
page is trusted, which is rarely true once a site accepts contributions.

## Search

Static sites search in the browser; private and hybrid sites search on the
server. Both implement the same ranking rules and are tested against the same
query corpus, so a reader's results do not change when a site moves from one
to the other.

```json
{
  "search": {
    "provider": "builtin",
    "shards": ["tab", "version"],
    "stopWords": "auto"
  }
}
```

## Security headers

`security` produces the Content Security Policy and the header files static
hosts read. The defaults are strict: `default-src 'self'`, no inline script
without the build nonce, and no framing. Add the sources an integration needs
rather than relaxing the policy as a whole.

```json
{
  "security": {
    "csp": { "extraImgSrc": ["https://images.acme.example"] },
    "frameAncestors": ["'none'"]
  }
}
```

## Checking the file

`liyasa config check` validates the file against the schema, prints the
resolved configuration with the source of every value, and exits non-zero on
an error. Run it in CI next to the build; a configuration mistake that only
surfaces as a missing sidebar three deploys later is expensive to find.
