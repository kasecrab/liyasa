---
title: Quickstart
description: Install the client, make one authenticated request, and read the response.
---

# Quickstart

Three steps, about two minutes.

:::steps

### Install the client

:::tabs

:::tab{title="npm"}
```bash
npm install @acme/client
```
:::

:::tab{title="cargo"}
```bash
cargo add acme-client
```
:::

:::

### Set your key

Create a key in the dashboard, then put it in your environment. Never commit
it.

```bash
export ACME_API_KEY="sk_live_..."
```

### Make a request

The block below is checked on every build: if this snippet stops compiling,
the build fails rather than the reader.

```rust verify="compile"
fn main() {
    let client = acme_client::Client::from_env();
    let pets = client.list_pets().limit(10).send();
    println!("{pets:?}");
}
```

:::

:::note
A request without a key returns `401` with a body explaining which header was
missing. See the [API reference](/checklist) for the full error list.
:::

## What just happened

The client read `ACME_API_KEY`, signed a request to the pets endpoint, and
decoded the response into a typed list. Nothing was cached and nothing was
retried; both are opt-in.

Next: [configuration](/guides/configuration).
