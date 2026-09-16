---
title: Domains
description: Custom domains, certificates, base paths, and deployments that do not go live.
---

# Domains

Codes in the `E08xx` range come from the server: domains, certificates,
deployments, and the network policy. This article covers what usually goes
wrong with each.

## Adding a domain

```sh
liyasa domain add docs.acme.com
liyasa domain verify docs.acme.com
```

Verification checks that the domain resolves to the server and that a
certificate can be issued for it. Until it passes, the deployment is served on
its default hostname.

## Domain verification fails

[`E0801`](/errors/E0801) means the check did not pass. In order of frequency:

::::steps

:::step{title="DNS has not propagated"}
A record changed in the last few minutes may not be visible yet. Check what the
world sees rather than what your resolver has cached:

```sh
dig +short docs.acme.com @1.1.1.1
```
:::

:::step{title="The record points somewhere else"}
A `CNAME` to the wrong target, or an `A` record left over from a previous host.
A domain with both a `CNAME` and other records at the same name is invalid and
resolvers handle it inconsistently.
:::

:::step{title="A CNAME at the zone apex"}
`acme.com` cannot be a `CNAME`. Use a provider that supports `ALIAS`/`ANAME`
records, or put the documentation on a subdomain, which is the usual answer.
:::

:::step{title="CAA records forbid the certificate authority"}
A `CAA` record that lists other authorities prevents issuance. This one fails
silently in most tooling and is worth checking explicitly:

```sh
dig +short CAA acme.com
```
:::

::::

## Certificate issuance fails

[`E0802`](/errors/E0802) is issuance rather than verification: the domain
resolves, but the certificate could not be obtained.

- The domain must be reachable from the public internet during issuance. A
  firewall that only allows your office will fail the challenge.
- Rate limits at the certificate authority apply per domain and per week.
  Repeatedly recreating a domain is the usual way to hit them.
- A wildcard certificate needs a DNS challenge, which needs credentials for the
  zone.

## Base paths

A site served under a path rather than at the root of a domain needs
`build.basePath`:

```json
{ "build": { "basePath": "/docs" } }
```

Every emitted link, header rule, and redirect is rewritten under it. Symptoms of
getting this wrong are consistent: the home page loads, the stylesheet 404s, and
every internal link goes one level too high.

On GitHub Pages under a repository path, the base path is `/<repository>`, or a
subdirectory of it. These docs use `/liyasa/docs`.

## Deployments

[`E0804`](/errors/E0804) is a deployment that failed, and the build diagnostics
above it are the reason. A failed build changes nothing that is served: workers
write to object storage and the serving process only swaps the pointer once the
build succeeded.

[`E0805`](/errors/E0805) is a rollback to a deployment whose artifacts are no
longer retained. Retention is finite; rolling back to something from last year
is not possible, and the fix is to redeploy that commit.

```sh
liyasa deployments list
liyasa deployments status dep_01H
liyasa deployments rollback dep_01H
```

## Outbound requests are blocked

[`E0806`](/errors/E0806) is the network policy refusing a request Liyasa made on
your behalf: fetching an OpenAPI spec, refreshing a fact source, checking a
link.

The policy exists because those URLs come from configuration and content, which
means they are attacker-influenced on a multi-tenant server. It blocks private
address ranges, non-HTTPS schemes, and hosts that are not allow-listed, and it
re-validates after every redirect.

```json
{
  "network": {
    "allowHosts": { "factSources": ["status.acme.com"] }
  }
}
```

The message names which rule refused: the address class, the host, or a
redirect that landed somewhere the original request would not have been allowed
to go.

## Rate limiting

[`E0807`](/errors/E0807) is returned as HTTP 429 with a `Retry-After` header. The
limiter is keyed by IP address. If you are behind a proxy, the limiter needs
`server.trustedProxies` set, or every request appears to come from the proxy and
one reader can exhaust the limit for everyone.

:::warning{title="Trusted proxies default to none, on purpose"}
Until you list your proxy, forwarded headers are ignored rather than trusted. A
server that trusts `X-Forwarded-For` from anyone lets a reader claim any IP
address and any region.
:::

## Getting help

[Previews](/help/previews) covers per-branch deployments, and
[hosting](/guides/hosting) covers static hosts, which have no domain
verification of their own.
