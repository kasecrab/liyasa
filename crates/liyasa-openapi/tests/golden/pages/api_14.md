# List widgets

`GET /widgets`

Widgets, newest first.

## Servers

- `https://api.example.com/v1`

## Query parameters

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `limit` | integer | no | How many Default `20`. at least 1, at most 100. |
| `cursor` | string | no |  |

## Responses

### 200 — A page of widgets

#### `application/json`

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `items` | array of object | yes |  |
| `items.id` | string | no |  |
| `next` | string · nullable | no |  |

##### Example

```json
{
  "items": [
    {
      "id": "w_1"
    }
  ],
  "next": "string"
}
```

## Request samples

### cURL

```bash
curl -X GET 'https://api.example.com/v1/widgets?limit=20&cursor=string'
```

### JavaScript

```javascript
const response = await fetch("https://api.example.com/v1/widgets?limit=20&cursor=string", {
  method: "GET",
});

const data = await response.json();
```

### Python

```python
import requests

url = "https://api.example.com/v1/widgets?limit=20&cursor=string"

response = requests.request("GET", url)
print(response.json())
```

### Go

```go
package main

import (
	"fmt"
	"io"
	"net/http"
)

func main() {
	request, err := http.NewRequest("GET", "https://api.example.com/v1/widgets?limit=20&cursor=string", nil)
	if err != nil {
		panic(err)
	}

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
  "items": [
    {
      "id": "w_1"
    }
  ],
  "next": "string"
}
```

