use serde_json::json;

use super::*;

fn petstore(list_pets: serde_json::Value, extra: serde_json::Value) -> Value {
    json!({
        "openapi": "3.1.0",
        "paths": {
            "/pets": { "get": list_pets },
            "/users": { "get": extra }
        }
    })
}

fn list_users() -> serde_json::Value {
    json!({ "operationId": "listUsers", "responses": { "200": { "description": "ok" } } })
}

fn changed(old: serde_json::Value, new: serde_json::Value) -> Vec<OperationChange> {
    operation_changes(
        "petstore",
        &petstore(old, list_users()),
        &petstore(new, list_users()),
    )
}

#[test]
fn a_changed_parameter_flags_exactly_that_operation() {
    let changes = changed(
        json!({
            "operationId": "listPets",
            "parameters": [{ "name": "limit", "in": "query" }],
            "responses": { "200": { "description": "ok" } }
        }),
        json!({
            "operationId": "listPets",
            "parameters": [{ "name": "limit", "in": "query", "required": true }],
            "responses": { "200": { "description": "ok" } }
        }),
    );
    assert_eq!(changes.len(), 1, "{changes:#?}");
    assert_eq!(changes[0].op, "listPets");
    assert_eq!(changes[0].diff, ["parameters"]);
    assert_eq!(changes[0].spec, "petstore");
}

#[test]
fn a_changed_response_and_a_changed_auth_are_named_separately() {
    let base = json!({
        "operationId": "listPets",
        "responses": { "200": { "description": "ok" } },
        "security": [{ "apiKey": [] }]
    });
    let responses = changed(
        base.clone(),
        json!({
            "operationId": "listPets",
            "responses": { "200": { "description": "ok" }, "429": { "description": "slow down" } },
            "security": [{ "apiKey": [] }]
        }),
    );
    assert_eq!(responses[0].diff, ["responses"]);

    let auth = changed(
        base,
        json!({
            "operationId": "listPets",
            "responses": { "200": { "description": "ok" } },
            "security": [{ "oauth2": ["read:pets"] }]
        }),
    );
    assert_eq!(auth[0].diff, ["auth"]);
}

#[test]
fn an_operation_that_did_not_move_is_not_reported() {
    let same =
        json!({ "operationId": "listPets", "responses": { "200": { "description": "ok" } } });
    assert!(changed(same.clone(), same).is_empty());
}

#[test]
fn a_change_the_operation_only_inherits_still_reaches_it() {
    // The document's `security` applies to an operation that states none, so
    // removing it changes how the operation is called.
    let operation = json!({ "operationId": "listPets", "responses": {} });
    let mut before = petstore(operation.clone(), list_users());
    before["security"] = json!([{ "apiKey": [] }]);
    let after = petstore(operation, list_users());

    let changes = operation_changes("petstore", &before, &after);
    assert_eq!(changes.len(), 2, "both operations inherit it: {changes:#?}");
    assert!(changes.iter().all(|change| change.diff == ["auth"]));
}

#[test]
fn a_path_level_parameter_belongs_to_every_operation_under_it() {
    let before = json!({
        "paths": { "/pets": {
            "parameters": [{ "name": "tenant", "in": "path" }],
            "get": { "operationId": "listPets", "responses": {} }
        }}
    });
    let after = json!({
        "paths": { "/pets": {
            "parameters": [{ "name": "tenant", "in": "header" }],
            "get": { "operationId": "listPets", "responses": {} }
        }}
    });
    let changes = operation_changes("petstore", &before, &after);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].diff, ["parameters"]);
}

#[test]
fn an_operation_with_no_id_is_named_by_its_method_and_path() {
    let before = json!({ "paths": { "/pets": { "get": { "responses": {} } } } });
    let after = json!({
        "paths": { "/pets": { "get": { "responses": { "200": { "description": "ok" } } } } }
    });
    let changes = operation_changes("petstore", &before, &after);
    assert_eq!(changes[0].op, "GET /pets");
}

#[test]
fn an_added_or_removed_operation_is_reported_as_one() {
    let empty = json!({ "paths": {} });
    let one = json!({
        "paths": { "/pets": { "get": { "operationId": "listPets", "responses": {} } } }
    });
    let added = operation_changes("petstore", &empty, &one);
    assert_eq!(added[0].diff, ["added"]);
    let removed = operation_changes("petstore", &one, &empty);
    assert_eq!(removed[0].diff, ["removed"]);
}

#[test]
fn a_document_with_no_paths_yields_no_changes() {
    assert!(operation_changes("petstore", &json!({}), &json!({ "openapi": "3.1.0" })).is_empty());
}
