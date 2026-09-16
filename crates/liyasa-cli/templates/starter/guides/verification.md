---
title: Verification
description: How Liyasa checks that the claims on a page are still true.
---

# Verification

A documentation site is a set of claims about a system. Liyasa records where
each claim came from and re-checks it.

## Facts

A fact is a value with a source. `facts/pricing.json` holds this site's sample:

```json
{
  "starter_price_usd": 0,
  "team_price_usd": 20,
  "seats_included": 5
}
```

`facts/sources.toml` says where those numbers are supposed to come from. When
the source moves, the fact drifts, and `liyasa verify` says so instead of the
page quietly lying.

## Verified code

A fenced block with `verify` is compiled or run in a sandbox:

```rust verify="compile"
pub fn total_cents(seats: u32, price_cents: u32) -> u32 {
    seats.saturating_mul(price_cents)
}
```

Add `verify="skip"` to opt a block out.

## Running it

```bash
liyasa verify
liyasa verify --only links
liyasa verify --changed main
```

:::info
Code runners that need a container require Docker or Podman. Run
`liyasa doctor` to see what this machine can do.
:::
