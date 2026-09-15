# POST /pets

`POST /pets`

## Request body

Required.

### `application/json`

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `body` | one of | yes |  |
| `body (cat).kind` | string | yes |  |
| `body (cat).livesLeft` | integer · nullable | no |  |
| `body (dog).kind` | string | yes |  |
| `body (dog).goodBoy` | boolean | no |  |

#### Example

```json
{
  "kind": "cat",
  "livesLeft": 1
}
```

## Responses

### 201 — made

#### `application/json`

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `label` | string · nullable | no |  |
| `children` | array of Node | no | Nested; see `Node`. |

##### Example

```json
{
  "label": "string",
  "children": []
}
```

## Request samples

### cURL

```bash
curl -X POST 'https://api.example.com/pets' \
  -H 'Content-Type: application/json' \
  -d '{
  "kind": "cat",
  "livesLeft": 1
}'
```

### JavaScript

```javascript
const response = await fetch("https://api.example.com/pets", {
  method: "POST",
  headers: {
    "Content-Type": "application/json",
  },
  body: JSON.stringify({
    "kind": "cat",
    "livesLeft": 1
  }),
});

const data = await response.json();
```

### Python

```python
import requests

url = "https://api.example.com/pets"
headers = {
    "Content-Type": "application/json",
}
payload = {
    "kind": "cat",
    "livesLeft": 1
}

response = requests.request("POST", url, headers=headers, json=payload)
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
	body := strings.NewReader("{\n  \"kind\": \"cat\",\n  \"livesLeft\": 1\n}")
	request, err := http.NewRequest("POST", "https://api.example.com/pets", body)
	if err != nil {
		panic(err)
	}
	request.Header.Set("Content-Type", "application/json")

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

### 201 `application/json`

```json
{
  "label": "string",
  "children": []
}
```

