//! CMP-40 to CMP-43 and CMP-85: parameters, response fields, examples,
//! endpoints, and spec schemas.

use liyasa_components::{inst, nodes};
use liyasa_core::document::PropValue;

use crate::support::Gallery;

fn str(value: &str) -> PropValue {
    PropValue::Str(value.to_owned())
}

#[test]
fn param() {
    Gallery::new("param")
        .case(
            "default",
            inst::new("param")
                .prop("name", str("limit"))
                .child(nodes::paragraph("How many items to return."))
                .build(),
        )
        .case(
            "every prop",
            inst::new("param")
                .prop("name", str("limit"))
                .prop("in", str("query"))
                .prop("type", str("integer"))
                .prop("required", PropValue::Bool(true))
                .prop("deprecated", PropValue::Bool(true))
                .prop("default", str("20"))
                .prop("placeholder", str("50"))
                .prop(
                    "enum",
                    PropValue::List(vec![str("10"), str("20"), str("50")]),
                )
                .prop("min", PropValue::Num(1.0))
                .prop("max", PropValue::Num(100.0))
                .prop("example", str("25"))
                .child(nodes::paragraph("How many items to return."))
                .build(),
        )
        .case("missing required prop", inst::new("param").build())
        .case(
            "invalid location",
            inst::new("param")
                .prop("name", str("payload"))
                .prop("in", str("everywhere"))
                .build(),
        )
        .case(
            "nested expandable",
            inst::new("param")
                .prop("name", str("filter"))
                .prop("type", str("object"))
                .child(inst::nested(
                    inst::new("expandable")
                        .prop("title", str("filter properties"))
                        .child(inst::nested(
                            inst::new("response-field")
                                .prop("name", str("status"))
                                .prop("type", str("string"))
                                .child(nodes::paragraph("Only items in this status.")),
                        )),
                ))
                .build(),
        )
        .check();
}

#[test]
fn a_run_of_fields_is_one_table() {
    Gallery::new("param-run")
        .case(
            "three fields",
            inst::new("panel")
                .child(inst::nested(
                    inst::new("param")
                        .prop("name", str("limit"))
                        .prop("type", str("integer"))
                        .prop("required", PropValue::Bool(true))
                        .child(nodes::paragraph("How many items to return.")),
                ))
                .child(inst::nested(
                    inst::new("param")
                        .prop("name", str("cursor"))
                        .prop("type", str("string"))
                        .child(nodes::paragraph("Where to resume from.")),
                ))
                .child(inst::nested(
                    inst::new("response-field")
                        .prop("name", str("items"))
                        .prop("type", str("object[]"))
                        .child(nodes::paragraph("The page of results.")),
                ))
                .build(),
        )
        .check();
}

#[test]
fn response_field() {
    Gallery::new("response-field")
        .case(
            "default",
            inst::new("response-field")
                .prop("name", str("id"))
                .prop("type", str("string"))
                .child(nodes::paragraph("The item's identifier."))
                .build(),
        )
        .case(
            "every prop",
            inst::new("response-field")
                .prop("name", str("id"))
                .prop("type", str("string"))
                .prop("required", PropValue::Bool(true))
                .prop("deprecated", PropValue::Bool(true))
                .prop("default", str("null"))
                .prop("example", str("itm_1"))
                .build(),
        )
        .check();
}

#[test]
fn examples() {
    Gallery::new("examples")
        .case(
            "request",
            inst::new("request-example")
                .prop("lang", str("curl"))
                .prop("title", str("cURL"))
                .child(nodes::code_block(
                    Some("sh"),
                    "curl https://api.example.com\n",
                ))
                .build(),
        )
        .case(
            "response",
            inst::new("response-example")
                .prop("lang", str("json"))
                .prop("status", str("200"))
                .child(nodes::code_block(Some("json"), "{\"ok\": true}\n"))
                .build(),
        )
        .case("empty request", inst::new("request-example").build())
        .check();
}

#[test]
fn endpoint() {
    Gallery::new("endpoint")
        .case(
            "default",
            inst::new("endpoint")
                .prop("method", str("post"))
                .prop("path", str("/v1/items/{id}"))
                .build(),
        )
        .case(
            "every prop",
            inst::new("endpoint")
                .prop("method", str("delete"))
                .prop("path", str("/v1/items/{id}"))
                .prop("spec", str("main"))
                .prop("operation", str("deleteItem"))
                .child(nodes::paragraph("Removes an item."))
                .build(),
        )
        .case(
            "invalid method",
            inst::new("endpoint").prop("method", str("fetch")).build(),
        )
        .check();
}

#[test]
fn openapi_schema() {
    Gallery::new("openapi-schema")
        .case(
            "default",
            inst::new("openapi-schema")
                .prop("spec", str("main"))
                .prop("schema", str("Item"))
                .build(),
        )
        .case(
            "missing required props",
            inst::new("openapi-schema").build(),
        )
        .check();
}
