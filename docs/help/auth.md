---
title: Authentication
description: Private sites, single sign-on, reader groups, tokens, and what to check when a reader sees the wrong thing.
---

# Authentication

A public documentation site needs none of this. Everything here applies to
sites that are private, partly gated, or personalized.

## The modes

| Mode | Who it is for | Needs |
|---|---|---|
| Public | Most documentation | Nothing |
| Password | A small private site, a customer preview | A server |
| JWT | An existing application that already knows the reader | A server and a shared key |
| OIDC | An organisation with single sign-on | A server and an identity provider |

`public: false` requires `liyasa serve`: a static host cannot enforce access,
and configuring it as though it could is [`E0120`](/errors/E0120).

## Provider configuration is invalid

[`E0803`](/errors/E0803) means the provider could not be configured or its
metadata could not be read. What to check, in order:

::::steps

:::step{title="The discovery document is reachable"}
An OIDC provider is configured by its issuer URL, and Liyasa reads
`/.well-known/openid-configuration` from it. If that request is blocked by the
network policy, you get [`E0806`](/errors/E0806) instead; allow-list the issuer
host.
:::

:::step{title="The issuer matches exactly"}
The `iss` claim in issued tokens must equal the configured issuer, character for
character. A trailing slash is a mismatch, and it is the single most common
cause of this error.
:::

:::step{title="The redirect URI is registered"}
The callback URL must be registered with the provider, including its scheme,
host, port, and path. Providers differ in whether they allow wildcards; most do
not.
:::

:::step{title="The client secret is present"}
Secrets come from the environment, never from `liyasa.json`. A secret that is
empty produces a provider error that reads like a configuration error.
:::

::::

## Readers see the wrong content

Gating happens at four levels, and content that appears or disappears
unexpectedly is nearly always a mismatch between two of them:

- **Page**, from `groups` or `regions` in front matter
- **Navigation node**, from `groups` on the node
- **Block**, from `:::region` or a visibility component
- **Fact-driven**, from an availability matrix

A page visible in navigation but refused on arrival means the node's groups and
the page's groups disagree. Check the page first: the node is what most people
edit and forget.

To reproduce what a particular reader sees:

```sh
liyasa dev --groups beta,internal --region eu
```

## Tokens

`liyasa login` uses a device-code flow and stores the token in the operating
system keychain. `liyasa status` says who you are and against which server.

```sh
liyasa login
liyasa status
```

In CI there is no keychain and no browser. Use a token from the environment
instead, and scope it to deployment rather than to full access.

:::warning{title="A token in `liyasa.json` is a token in your git history"}
Configuration is committed. Every secret Liyasa reads comes from the
environment or from the keychain for this reason. If one has been committed,
rotate it: removing it from the working tree does not remove it from history.
:::

## JWT validation

For sites that hand off from an existing application, the rules are strict on
purpose:

- The signature must verify against the configured key or JWKS endpoint.
- `exp` must be in the future and `nbf`, if present, in the past.
- `aud` and `iss` must match the configuration.
- The algorithm must be one the configuration names. A token asking for `none`
  is rejected regardless of anything else.

A token that fails any of these is refused rather than downgraded to anonymous,
because silently serving a public view to a reader who thought they were
authenticated is the worse failure.

## Personalization

A page that reads `reader.*` depends on who is asking, so it cannot be one
static file. Mark it:

```yaml
---
title: Your plan
personalized: true
---
```

Without that, reading reader context is [`E0208`](/errors/E0208). With it, the
page is rendered per request within a budget; exceeding that budget is
[`E0810`](/errors/E0810) and the default variant is served instead.

Pages that read a *bounded* set of values — a version, a locale, a region — are
pre-rendered as variants rather than rendered per request, which is why those
work on a static host and free-form reader fields do not.

## Getting help

[Regions and localization](/guides/regions-and-localization) covers region
gating, and [domains](/help/domains) covers the trusted-proxy setting that
region and rate-limit detection both depend on.
