//! The corpus every generator is run over (API-30).
//!
//! One operation per shape a request can take, so a template that forgets
//! cookies or sends a multipart body as JSON fails on the case that covers it
//! rather than on whichever real spec happens to reach it first.

/// The `operationId` of each case, which is also the name of its golden file.
pub const CASES: &[&str] = &[
    "pathParams",
    "queryParams",
    "headerAndCookie",
    "jsonBody",
    "formBody",
    "multipart",
    "bearerAuth",
    "apiKeyAuth",
];

/// The spec the cases live in.
pub const SPEC: &str = r##"
openapi: 3.1.0
info: { title: Codegen corpus, version: "1.0.0" }
servers:
  - url: https://api.example.com/v1
paths:
  /users/{id}/notes/{noteId}:
    get:
      operationId: pathParams
      parameters:
        - { name: id, in: path, required: true, schema: { type: string }, example: "u_1" }
        - { name: noteId, in: path, required: true, schema: { type: integer }, example: 7 }
      responses:
        "200": { description: ok }
  /search:
    get:
      operationId: queryParams
      parameters:
        - { name: q, in: query, required: true, schema: { type: string }, example: "red widget" }
        - { name: limit, in: query, schema: { type: integer, minimum: 10 } }
        - name: tags
          in: query
          style: form
          explode: false
          schema: { type: array, items: { type: string, examples: ["new"] } }
      responses:
        "200": { description: ok }
  /trace:
    get:
      operationId: headerAndCookie
      parameters:
        - { name: X-Request-Id, in: header, required: true, schema: { type: string, format: uuid } }
        - { name: session, in: cookie, required: true, schema: { type: string }, example: "s_1" }
      responses:
        "200": { description: ok }
  /notes:
    post:
      operationId: jsonBody
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [text]
              properties:
                text: { type: string, examples: ["It's here"] }
                pinned: { type: boolean }
                tags: { type: array, items: { type: string, examples: ["new"] } }
      responses:
        "201": { description: made }
  /subscribe:
    post:
      operationId: formBody
      requestBody:
        required: true
        content:
          application/x-www-form-urlencoded:
            schema:
              type: object
              required: [email]
              properties:
                email: { type: string, format: email }
                plan: { type: string, enum: [free, pro] }
      responses:
        "204": { description: done }
  /uploads:
    post:
      operationId: multipart
      requestBody:
        required: true
        content:
          multipart/form-data:
            schema:
              type: object
              required: [file]
              properties:
                file: { type: string, format: binary }
                caption: { type: string, examples: ["A photo"] }
      responses:
        "201": { description: made }
  /me:
    get:
      operationId: bearerAuth
      security:
        - bearer: []
      responses:
        "200": { description: ok }
  /keys:
    get:
      operationId: apiKeyAuth
      security:
        - apiKey: []
      responses:
        "200": { description: ok }
components:
  securitySchemes:
    bearer: { type: http, scheme: bearer }
    apiKey: { type: apiKey, name: X-API-Key, in: header }
"##;
