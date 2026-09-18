<!-- Generated from crates/liyasa-build/src/hosting/MATRIX.md.
Included by docs/guides/hosting.md; edit the emulators, not this file.
Regenerate with: cargo run -p liyasa-tests --bin docs-reference -->

| Check | GitHub Pages | Cloudflare Pages | Netlify | Vercel | S3 + CloudFront | Web server |
|---|---|---|---|---|---|---|
| `upload-delivery` | yes | yes | yes | yes | yes | yes |
| `markdown-url-support` | yes | yes | yes | yes | manual | manual |
| `content-negotiation` | partial | partial | partial | partial | partial | partial |
| `cache-header-hygiene` | partial | partial | partial | partial | manual | manual |
| `http-status-codes` | yes | yes | yes | yes | partial | partial |
| `redirect-behavior` | manual | yes | yes | yes | manual | manual |
| `security-headers` | manual | yes | yes | yes | manual | manual |
| `content-security-policy` | manual | yes | yes | yes | manual | manual |
| `immutable-assets` | manual | yes | yes | yes | manual | manual |
| `trailing-slash` | yes | yes | yes | yes | partial | yes |

`yes`: the upload alone passes. `partial`: passes in part, see the note. `manual`: passes with configuration the hosting guide describes. `no`: cannot pass.

**GitHub Pages**

- `content-negotiation`: a static host cannot negotiate; the `.md` route is the documented alternative
- `cache-header-hygiene`: the host sends `max-age=600` and reads no header file
- `redirect-behavior`: no redirect file is read; the hosting guide gives the host's own rule format or the `<meta refresh>` fallback pages
- `security-headers`: no header file is read; the docs give the host's own header configuration
- `content-security-policy`: no header file is read; the docs give the host's own header configuration
- `immutable-assets`: no header file is read; set `Cache-Control` per prefix at upload or in the server block

**Cloudflare Pages**

- `content-negotiation`: a static host cannot negotiate; the `.md` route is the documented alternative
- `cache-header-hygiene`: no `Last-Modified`; validation is by `ETag` alone

**Netlify**

- `content-negotiation`: a static host cannot negotiate; the `.md` route is the documented alternative
- `cache-header-hygiene`: no `Last-Modified`; validation is by `ETag` alone

**Vercel**

- `content-negotiation`: a static host cannot negotiate; the `.md` route is the documented alternative
- `cache-header-hygiene`: no `Last-Modified`; validation is by `ETag` alone

**S3 + CloudFront**

- `markdown-url-support`: `.md` is served as `application/octet-stream`; set the type at upload or in the server's MIME table
- `content-negotiation`: a static host cannot negotiate; the `.md` route is the documented alternative
- `cache-header-hygiene`: `Cache-Control` comes from the upload metadata or the server block
- `http-status-codes`: 404 status with the host's own body; `404.html` needs the host's error-page setting
- `redirect-behavior`: no redirect file is read; the hosting guide gives the host's own rule format or the `<meta refresh>` fallback pages
- `security-headers`: no header file is read; the docs give the host's own header configuration
- `content-security-policy`: no header file is read; the docs give the host's own header configuration
- `immutable-assets`: no header file is read; set `Cache-Control` per prefix at upload or in the server block
- `trailing-slash`: the bucket's website endpoint answers 302, not 301

**Web server**

- `markdown-url-support`: `.md` is served as `application/octet-stream`; set the type at upload or in the server's MIME table
- `content-negotiation`: a static host cannot negotiate; the `.md` route is the documented alternative
- `cache-header-hygiene`: `Cache-Control` comes from the upload metadata or the server block
- `http-status-codes`: 404 status with the host's own body; `404.html` needs the host's error-page setting
- `redirect-behavior`: no redirect file is read; the hosting guide gives the host's own rule format or the `<meta refresh>` fallback pages
- `security-headers`: no header file is read; the docs give the host's own header configuration
- `content-security-policy`: no header file is read; the docs give the host's own header configuration
- `immutable-assets`: no header file is read; set `Cache-Control` per prefix at upload or in the server block
