# Replace a widget

`PUT /widgets/{id}`

**Deprecated.** This operation is deprecated.

Replaces the whole widget.

## Servers

- `https://api.example.com/v1` — Production

## Authentication

- `bearer` (bearer) — A token (scopes: `write`)

## Path parameters

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | string | yes | Which widget |

## Query parameters

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `dryRun` | boolean | no | Default `false`. |

## Headers

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `If-Match` | string | yes |  |

## Cookies

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `session` | string | no |  |

## Request body

Required.

The replacement

### `application/json`

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | string | yes |  |
| `size` | integer | no | at least 1, at most 10. |

#### Example

```json
{
  "name": "Bolt",
  "size": 1
}
```

## Responses

### 200 — Replaced

| Header | Type | Required | Description |
| --- | --- | --- | --- |
| `X-Rate-Limit` | integer | no | Calls left |

#### `application/json`

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | string | no |  |

##### Example

```json
{
  "id": "string"
}
```

- `widget` → `GET /widgets/{id}/history` — Read it back

### 412 — Precondition failed

## Callbacks

- `onChange`: `POST {$request.body#/callbackUrl}` — Widget changed

## Request samples

### cURL

```bash
curl -X PUT 'https://api.example.com/v1/widgets/string?dryRun=false' \
  -H 'If-Match: string' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer $ACCESS_TOKEN' \
  -H 'Cookie: session=string' \
  -d '{
  "name": "Bolt",
  "size": 1
}'
```

### JavaScript

```javascript
const response = await fetch("https://api.example.com/v1/widgets/string?dryRun=false", {
  method: "PUT",
  headers: {
    "If-Match": "string",
    "Content-Type": "application/json",
    "Authorization": "Bearer $ACCESS_TOKEN",
    "Cookie": "session=string",
  },
  body: JSON.stringify({
    "name": "Bolt",
    "size": 1
  }),
});

const data = await response.json();
```

### Python

```python
import requests

url = "https://api.example.com/v1/widgets/string?dryRun=false"
headers = {
    "If-Match": "string",
    "Content-Type": "application/json",
    "Authorization": "Bearer $ACCESS_TOKEN",
    "Cookie": "session=string",
}
payload = {
    "name": "Bolt",
    "size": 1
}

response = requests.request("PUT", url, headers=headers, json=payload)
print(response.json())
```

### Go

```go
package main

import (
	"fmt"
	"io"
	"net/http"
	"strings"
)

func main() {
	body := strings.NewReader("{\n  \"name\": \"Bolt\",\n  \"size\": 1\n}")
	request, err := http.NewRequest("PUT", "https://api.example.com/v1/widgets/string?dryRun=false", body)
	if err != nil {
		panic(err)
	}
	request.Header.Set("If-Match", "string")
	request.Header.Set("Content-Type", "application/json")
	request.Header.Set("Authorization", "Bearer $ACCESS_TOKEN")
	request.Header.Set("Cookie", "session=string")

	response, err := http.DefaultClient.Do(request)
	if err != nil {
		panic(err)
	}
	defer response.Body.Close()

	payload, err := io.ReadAll(response.Body)
	if err != nil {
		panic(err)
	}
	fmt.Println(string(payload))
}
```

## Response samples

### 200 `application/json`

```json
{
  "id": "string"
}
```

