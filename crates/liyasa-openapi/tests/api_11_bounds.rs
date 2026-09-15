//! API-11, RFC 0805: what `$ref` expansion is allowed to cost.
//!
//! A reference is resolved by inlining, so a schema graph that is wide as well
//! as deep multiplies. Two bounds keep that finite — the depth a table is
//! drawn to, and a budget of references per document — and both produce the
//! same named stub a cycle does, which is the expand control of API-11.

use liyasa_openapi::field::{self, Field};
use liyasa_openapi::load;
use liyasa_openapi::model::Schema;

/// A chain of `depth` components, each holding the next under `next`.
fn nested(depth: usize) -> String {
    let mut spec = String::from(
        r##"
openapi: 3.1.0
info: { title: Deep, version: "1" }
paths:
  /deep:
    get:
      operationId: getDeep
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Level0" }
components:
  schemas:
"##,
    );
    for level in 0..depth {
        spec.push_str(&format!(
            "    Level{level}:\n      type: object\n      properties:\n"
        ));
        spec.push_str(&format!("        name{level}: {{ type: string }}\n"));
        if level + 1 < depth {
            spec.push_str(&format!(
                "        next: {{ $ref: \"#/components/schemas/Level{}\" }}\n",
                level + 1
            ));
        }
    }
    spec
}

fn root(spec: &str) -> Schema {
    let loaded = load::from_bytes("api", "api.yaml", spec.as_bytes()).expect("the spec loads");
    assert!(
        !loaded.diagnostics.has_errors(),
        "{:?}",
        loaded.diagnostics.as_slice()
    );
    let operation = loaded
        .spec
        .by_operation_id("getDeep")
        .expect("the operation is there");
    operation
        .operation
        .responses
        .values()
        .next()
        .and_then(|response| response.preferred())
        .and_then(|(_, media)| media.schema.clone())
        .expect("the response has a schema")
}

/// Walks `next` until it runs out, returning how many levels were inlined and
/// whether the last one is a stub.
fn walk(schema: &Schema) -> (usize, Option<String>) {
    let mut levels = 0;
    let mut current = schema;
    loop {
        if current.is_stub() {
            return (levels, current.name.clone());
        }
        match current.properties.get("next") {
            Some(next) => {
                levels += 1;
                current = next;
            }
            None => return (levels, None),
        }
    }
}

#[test]
fn a_chain_shorter_than_the_limit_is_inlined_whole() {
    let (levels, stub) = walk(&root(&nested(4)));
    assert_eq!(levels, 3, "three `next` hops to the last level");
    assert_eq!(stub, None, "nothing was cut");
}

#[test]
fn a_chain_deeper_than_the_table_becomes_a_named_stub() {
    let (levels, stub) = walk(&root(&nested(40)));
    assert!(
        levels <= field::DEPTH + 1,
        "expanded {levels} levels, deeper than a table is ever drawn"
    );
    assert!(
        stub.is_some(),
        "the cut names the component so the reader can expand it"
    );
    assert!(
        stub.as_deref()
            .is_some_and(|name| name.starts_with("Level")),
        "got {stub:?}"
    );
}

#[test]
fn the_row_for_a_cut_schema_offers_the_expand_control() {
    let schema = root(&nested(40));
    let row = Field::of_schema("body", &schema, true, field::DEPTH);
    let mut current = &row;
    loop {
        match current.children.iter().find(|child| child.name == "next") {
            Some(next) => current = next,
            None => break,
        }
    }
    assert!(
        current.truncated,
        "the deepest row a table draws says it is not the whole story"
    );
}

/// Wide as well as deep is the shape that multiplied: every property of every
/// level referred to the next level, so inlining was exponential.
#[test]
fn a_wide_and_deep_graph_stays_finite() {
    const WIDTH: usize = 8;
    const LEVELS: usize = 12;
    let mut spec = String::from(
        r##"
openapi: 3.1.0
info: { title: Wide, version: "1" }
paths:
  /wide:
    get:
      operationId: getDeep
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Level0" }
components:
  schemas:
"##,
    );
    for level in 0..LEVELS {
        spec.push_str(&format!(
            "    Level{level}:\n      type: object\n      properties:\n"
        ));
        for branch in 0..WIDTH {
            if level + 1 < LEVELS {
                spec.push_str(&format!(
                    "        branch{branch}: {{ $ref: \"#/components/schemas/Level{}\" }}\n",
                    level + 1
                ));
            } else {
                spec.push_str(&format!("        branch{branch}: {{ type: string }}\n"));
            }
        }
    }

    // Unbounded, this is 8^11 nodes. It has to finish, and quickly.
    let started = std::time::Instant::now();
    let schema = root(&spec);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "reading took {:?}",
        started.elapsed()
    );
    assert!(count(&schema, 0) < 500_000, "the model is not bounded");
}

fn count(schema: &Schema, depth: usize) -> usize {
    if depth > 40 {
        return 1;
    }
    1 + schema
        .properties
        .values()
        .map(|child| count(child, depth + 1))
        .sum::<usize>()
}
