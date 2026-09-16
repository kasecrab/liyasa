---
title: Fact modelling
description: Choosing what deserves to be a fact, where its truth lives, and how to reference it so a page cannot disagree with the product.
---

# Fact modelling

A **fact** is a named, typed value with a source of truth. `pricing.pro.monthly_usd`
sourced from `facts/pricing.json`. `ui.button.export` sourced from the product's
own translation file. `integration.slack.status` sourced from a health endpoint.

Once a value is a fact, prose references it instead of restating it, and the
build compares the source with reality on a schedule. The page can no longer be
wrong on its own.

## What deserves to be a fact

The test: **would this change without anyone thinking about the documentation?**

::::columns{cols=2}

:::column
**Good candidates**

- Prices, plan limits, quotas
- Version numbers and release dates
- Model identifiers, context windows
- Endpoint paths and required scopes
- Package versions and install commands
- Default values of settings
- UI labels you tell readers to click
- Region and feature availability
:::

:::column
**Poor candidates**

- Explanations and rationale
- Procedure order
- Anything where the number is an example rather than a commitment
- Values that appear exactly once in a page nobody maintains separately
- Anything whose "source" would be a file you update by hand at the same time
  as the docs
:::

::::

That last exclusion is the one people get wrong. A fact whose source is a file
only the documentation author edits is not verified, it is indirected. It has
the cost of a fact and none of the benefit.

## Declaring sources

Sources live in `facts/sources.toml`. Each declares where truth comes from and
how often to re-read it.

```toml
[[source]]
id = "pricing"
kind = "file"
path = "facts/pricing.json"
schema = "facts/pricing.schema.json"

[[source]]
id = "ui-strings"
kind = "repo"
repo = "acme/web-app"
path = "src/i18n/en.json"
ref = "main"
map = { "ui.button.export" = "$.settings.export.button" }

[[source]]
id = "status-slack"
kind = "url"
url = "https://status.acme.com/api/integrations/slack"
select = "$.status"
refresh = "1h"

[[source]]
id = "plan-limits"
kind = "command"
command = "./scripts/export-limits.sh"
refresh = "24h"
```

| Kind | Truth lives in | Use when |
|---|---|---|
| `file` | A JSON, YAML, or TOML file in the repository | The value is generated into the repository by a build step |
| `repo` | A file in a connected repository | The product's own source is the authority |
| `url` | An HTTP endpoint | The value is live and the service exposes it |
| `openapi` | An OpenAPI document | The claim is about an API's shape |
| `command` | The output of a command run in the sandbox | Truth needs computing |
| `screenshot` | A captured image | The claim is visual |
| `manual` | A person's attestation, with an expiry | Nothing machine-readable exists |

`manual` is the honest escape hatch. It records who asserted the value and when
it stops being trusted; an expired attestation is
[`E0606`](/errors/E0606) rather than a value that quietly ages.

## Referencing facts in prose

Two forms, depending on whether you need formatting:

```markdown
The Pro plan costs {{ facts.pricing.pro.monthly_usd | currency }} per month.

The Pro plan allows :fact[pricing.pro.requests_per_minute] requests per minute.
```

Both create a dependency edge from that block to that fact. Changing the source
invalidates exactly the pages that read it, which is what makes drift reporting
precise rather than "something in the pricing section changed".

A reference to a fact that does not exist is [`E0209`](/errors/E0209), caught at
build time rather than rendering as an empty span.

## Schemas

Give a fact source a `schema` and its data is validated on every refresh. A
source whose shape changed underneath you is [`E0605`](/errors/E0605), which is
a much better failure than a page rendering `undefined`.

This matters most for `url` and `command` sources, where the other end can
change without warning.

## Naming

Facts are namespaced by dots and read like the product's own vocabulary:

```
pricing.pro.monthly_usd
limits.api.requests_per_minute
models.sonnet.context_window
availability.eu.sso
```

Keep the namespace stable even when the value moves. Renaming a fact breaks
every page that reads it, which the build catches, but it also breaks the drift
history, which it does not.

## Availability as facts

Region gating reads from the same place, so availability lives in one file
rather than in per-page front matter scattered across the site:

```json
{ "regions": { "eu": { "sso": true, "byok": false } } }
```

```markdown
{% if region_available("sso") %}
Single sign-on is available on your plan.
{% endif %}
```

See [regions and localization](/guides/regions-and-localization).

## Rollout

Start with the values that appear on the most pages, because a fact's value is
proportional to how many places it is repeated. Grep for your own pricing
numbers; the count is usually a surprise.

```sh
liyasa verify --only facts --refresh
```

## Next steps

[Verification](/guides/verification) covers the other check kinds, and
[maintenance](/guides/maintenance) covers acting on drift once it is reported.
