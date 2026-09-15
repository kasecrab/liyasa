# Fetch one user

`GET /users/{id}`

Read the guide first.

## Path parameters

| Name | Type | Required | Description |
| --- | --- | --- | --- |
| `id` | string | yes |  |

Rate limits apply.

## Responses

### 200 — The user

## Request samples

### cURL

```bash
curl -X GET 'https://api.example.com/users/string'
```

### JavaScript

```javascript
const response = await fetch("https://api.example.com/users/string", {
  method: "GET",
});

const data = await response.json();
```

### Python

```python
import requests

url = "https://api.example.com/users/string"

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
	request, err := http.NewRequest("GET", "https://api.example.com/users/string", nil)
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

