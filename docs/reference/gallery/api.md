---
title: API
description: "Parameters, response fields, examples, and endpoints, for API pages written by hand rather than generated."
---

# API

Parameters, response fields, examples, and endpoints, for API pages written by hand rather than generated.

Every example below is rendered by this page, not pasted in as a picture of one: the source is shown and then the same source runs.

## `param`

A container component. Also written as `param-field`, `ParamField`, `Param`.

````markdown
:::param{name="limit" in="query" type="integer" default="50" min=1 max=200 example="100"}
How many deployments to return in one page of results.
:::
````

:::param{name="limit" in="query" type="integer" default="50" min=1 max=200 example="100"}
How many deployments to return in one page of results.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `name` | string | yes | — | Parameter name, as it appears in the request. |
| `in` | `query` \| `path` \| `body` \| `header` \| `cookie` | — | `"query"` | Where the parameter goes. `body` is for manual API pages; a spec-backed page emits body fields as response-field rows. |
| `type` | string | — | — | Type as the API documents it, e.g. `integer` or `string[]`. |
| `required` | boolean | — | — | Marks the parameter as required. |
| `deprecated` | boolean | — | — | Marks the parameter as deprecated. |
| `default` | string | — | — | Value used when the parameter is omitted. |
| `placeholder` | string | — | — | Example value shown in the playground's input. |
| `enum` | string[] | — | — | The values the parameter accepts. |
| `min` | number | — | — | Smallest accepted value or length. |
| `max` | number | — | — | Largest accepted value or length. |
| `example` | string | — | — | A value that works, shown beside the row. |

## `response-field`

A container component. Also written as `ResponseField`.

````markdown
:::response-field{name="created_at" type="string" required example="2026-09-01T12:00:00Z"}
When the deployment was created, as an ISO 8601 timestamp with an offset.
:::
````

:::response-field{name="created_at" type="string" required example="2026-09-01T12:00:00Z"}
When the deployment was created, as an ISO 8601 timestamp with an offset.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `name` | string | yes | — | Property name, as it appears in the response. |
| `type` | string | — | — | Type as the API documents it. |
| `required` | boolean | — | — | Marks the property as always present. |
| `deprecated` | boolean | — | — | Marks the property as deprecated. |
| `default` | string | — | — | Value the property takes when the API omits it. |
| `example` | string | — | — | A value that occurs, shown beside the row. |

## `request-example`

A container component. Also written as `RequestExample`.

````markdown
:::request-example{lang="curl" title="List deployments"}
```sh
curl https://api.acme.com/v1/deployments \
  -H "Authorization: Bearer $ACME_TOKEN"
```
:::
````

:::request-example{lang="curl" title="List deployments"}
```sh
curl https://api.acme.com/v1/deployments \
  -H "Authorization: Bearer $ACME_TOKEN"
```
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `lang` | string | — | — | Language of the example, e.g. `curl` or `python`. |
| `title` | string | — | — | Title shown above the example. |
| `status` | string | — | — | HTTP status this example illustrates. |

## `response-example`

A container component. Also written as `ResponseExample`.

````markdown
:::response-example{lang="json" status="200"}
```json
{ "deployments": [{ "id": "dep_01H", "status": "ready" }] }
```
:::
````

:::response-example{lang="json" status="200"}
```json
{ "deployments": [{ "id": "dep_01H", "status": "ready" }] }
```
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `lang` | string | — | — | Language of the example, e.g. `json`. |
| `title` | string | — | — | Title shown above the example. |
| `status` | string | — | — | HTTP status this example illustrates. |

## `endpoint`

A container component. Also written as `Endpoint`.

````markdown
:::endpoint{method="get" path="/v1/deployments/{id}"}
Returns one deployment by its identifier.
:::
````

:::endpoint{method="get" path="/v1/deployments/{id}"}
Returns one deployment by its identifier.
:::

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `method` | `get` \| `post` \| `put` \| `patch` \| `delete` \| `head` \| `options` \| `trace` | — | `"get"` | HTTP method. |
| `path` | string | — | — | Request path, with `{parameters}` in braces. |
| `spec` | string | — | — | Spec this endpoint is documented in; with `operation`, the header is pulled from it. |
| `operation` | string | — | — | `operationId` in that spec. |

## `openapi-schema`

A leaf component. Also written as `OpenApiSchema`, `OpenAPISchema`.

````markdown
::openapi-schema{spec="api" schema="Deployment"}
````

::openapi-schema{spec="api" schema="Deployment"}

| Prop | Type | Required | Default | What it does |
|---|---|---|---|---|
| `spec` | string | yes | — | Spec the schema lives in. |
| `schema` | string | yes | — | Name of the schema object, as in `components.schemas`. |

Props and types above are generated from each component's own schema, which is what the build validates against: a missing required prop is [`E0314`](/errors/E0314), a wrong type is [`E0315`](/errors/E0315), and an unknown prop is [`W0316`](/errors/W0316).

See [the component gallery](/reference/gallery) for the other groups.
