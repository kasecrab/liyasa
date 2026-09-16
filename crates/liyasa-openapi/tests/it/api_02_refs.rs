//! API-02: `$ref` resolves, and one that does not is `E0502` with the pointer
//! of the reference rather than of wherever the failure was noticed.

use liyasa_core::diagnostics::code;
use liyasa_openapi::model::{Method, SchemaType};
use liyasa_openapi::read;
use liyasa_openapi::tree;
use liyasa_openapi::version::SpecVersion;

fn load(source: &str) -> (liyasa_openapi::Spec, liyasa_core::Diagnostics) {
    let root = tree::parse(source.as_bytes(), "api.yaml").expect("the fixture parses");
    read::local(root, "api", SpecVersion::V3_1("3.1.0".to_owned()))
}

const LOCAL: &str = r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths:
  /users/{id}:
    get:
      operationId: getUser
      parameters:
        - $ref: "#/components/parameters/Id"
      responses:
        "200":
          description: One user
          content:
            application/json:
              schema: { $ref: "#/components/schemas/User" }
components:
  parameters:
    Id:
      name: id
      in: path
      required: true
      schema: { type: string }
  schemas:
    User:
      type: object
      properties:
        id: { type: string }
        manager: { $ref: "#/components/schemas/User" }
"##;

#[test]
fn a_local_reference_resolves_to_what_it_names() {
    let (spec, diagnostics) = load(LOCAL);
    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.as_slice());

    let operation = spec
        .operation(Method::Get, "/users/{id}")
        .expect("the operation is there");
    let parameters = operation.parameters();
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].name, "id");
    assert!(
        parameters[0]
            .schema
            .as_ref()
            .is_some_and(|s| s.is(SchemaType::String)),
        "the referenced parameter brought its schema"
    );
}

#[test]
fn a_referenced_schema_keeps_the_component_name_it_was_reached_by() {
    let (spec, _) = load(LOCAL);
    let operation = spec
        .operation(Method::Get, "/users/{id}")
        .expect("the operation is there");
    let response = operation
        .operation
        .responses
        .get("200")
        .expect("a 200 response");
    let (_, media) = response.preferred().expect("a response body");
    let schema = media.schema.as_ref().expect("the body has a schema");
    assert_eq!(schema.name.as_deref(), Some("User"));
}

#[test]
fn a_recursive_schema_stops_at_a_named_stub_rather_than_expanding_forever() {
    let (spec, diagnostics) = load(LOCAL);
    assert!(!diagnostics.has_errors(), "recursion is not an error");

    let mut schema = spec
        .components
        .schemas
        .get("User")
        .expect("the component is read")
        .clone();
    let mut depth = 0;
    while let Some(manager) = schema.properties.get("manager") {
        schema = manager.clone();
        depth += 1;
        assert!(depth < 100, "the expansion did not terminate");
        if schema.properties.is_empty() {
            break;
        }
    }
    assert!(depth > 0, "the reference was followed at least once");
    assert_eq!(
        schema.name.as_deref(),
        Some("User"),
        "the stub still says which schema to expand"
    );
}

#[test]
fn a_broken_reference_is_e0502_and_names_the_pointer() {
    let (_, diagnostics) = load(
        r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths:
  /a:
    get:
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema: { $ref: "#/components/schemas/Missing" }
components:
  schemas: {}
"##,
    );
    let broken: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == code::E0502)
        .collect();
    assert_eq!(broken.len(), 1, "{:?}", diagnostics.as_slice());
    assert!(
        broken[0]
            .message
            .contains("/paths/~1a/get/responses/200/content/application~1json/schema"),
        "the pointer is of the reference: {}",
        broken[0].message
    );
    assert!(broken[0].message.contains("#/components/schemas/Missing"));
}

#[test]
fn a_reference_into_a_document_that_was_not_loaded_is_e0502() {
    let (_, diagnostics) = load(
        r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths: {}
components:
  schemas:
    User: { $ref: "common.yaml#/User" }
"##,
    );
    let out: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == code::E0502)
        .collect();
    assert_eq!(out.len(), 1, "{:?}", diagnostics.as_slice());
    assert!(
        out[0].message.contains("common.yaml#/User"),
        "{}",
        out[0].message
    );
}

#[test]
fn a_type_error_in_a_spec_is_e0501_with_the_pointer_and_the_rest_still_reads() {
    let (spec, diagnostics) = load(
        r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths:
  /a:
    get:
      summary: [not, a, string]
      operationId: listA
      responses:
        "200": { description: ok }
"##,
    );
    let out: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == code::E0501)
        .collect();
    assert_eq!(out.len(), 1, "{:?}", diagnostics.as_slice());
    assert!(
        out[0].message.contains("/paths/~1a/get/summary"),
        "{}",
        out[0].message
    );
    assert!(
        spec.by_operation_id("listA").is_some(),
        "one bad field does not lose the operation"
    );
}

// ---- remote references ----

mod remote {
    use std::time::Duration;

    use liyasa_core::conformance::block_on;
    use liyasa_core::conformance::fixtures::MemoryVfs;
    use liyasa_core::diagnostics::code;
    use liyasa_core::net::{
        BoxFut, DenyReason, HostPattern, HostSet, HttpClient, HttpPolicy, HttpRequest,
        HttpResponse, NetError, Purpose,
    };
    use liyasa_core::vfs::Bytes;
    use liyasa_openapi::load;
    use liyasa_openapi::model::SchemaType;
    use liyasa_openapi::source::{Fetcher, Location};

    /// Serves canned documents, and refuses a host the policy does not list so
    /// the test exercises the policy rather than a second allow list.
    struct Canned(Vec<(&'static str, &'static str)>);

    impl HttpClient for Canned {
        fn fetch<'a>(
            &'a self,
            request: HttpRequest,
            policy: &'a HttpPolicy,
        ) -> BoxFut<'a, Result<HttpResponse, NetError>> {
            let url = request.url;
            Box::pin(async move {
                let host = url.host_str().unwrap_or_default().to_owned();
                if !policy.allow_hosts.matches(&host) {
                    return Err(NetError::PolicyDenied {
                        reason: DenyReason::HostNotAllowed(host),
                    });
                }
                let body = self
                    .0
                    .iter()
                    .find(|(at, _)| *at == url.as_str())
                    .map(|(_, body)| *body)
                    .ok_or(NetError::Status(404))?;
                Ok(HttpResponse {
                    status: 200,
                    headers: Vec::new(),
                    body: Bytes::from_static(body.as_bytes()),
                    final_url: url,
                })
            })
        }
    }

    fn policy() -> HttpPolicy {
        HttpPolicy {
            allow_hosts: HostSet(vec![HostPattern::Exact("schemas.example.com".to_owned())]),
            deny_hosts: HostSet::default(),
            allow_private: false,
            max_redirects: 2,
            max_bytes: 1 << 20,
            timeout: Duration::from_secs(5),
            purpose: Purpose::SpecRef,
        }
    }

    const ROOT: &str = r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths:
  /a:
    get:
      operationId: getA
      parameters:
        - $ref: "shared/params.yaml#/Id"
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: object
                properties:
                  allowed: { $ref: "https://schemas.example.com/user.yaml#/User" }
                  refused: { $ref: "https://elsewhere.example.net/evil.yaml#/X" }
                  broken: { $ref: "shared/params.yaml#/Nope" }
"##;

    fn load_spec(
        http: Option<&dyn HttpClient>,
    ) -> (liyasa_openapi::Spec, liyasa_core::Diagnostics) {
        let vfs = MemoryVfs::new().with("openapi/api.yaml", ROOT).with(
            "openapi/shared/params.yaml",
            "Id:\n  name: id\n  in: query\n  schema: { type: string }\n",
        );
        let policy = policy();
        let fetcher = Fetcher::new(&vfs, http, &policy);
        let at = Location::parse("openapi/api.yaml");
        let loaded = block_on(load::from_source("api", &at, &fetcher)).expect("the spec loads");
        (loaded.spec, loaded.diagnostics)
    }

    fn property(spec: &liyasa_openapi::Spec, name: &str) -> Option<liyasa_openapi::Schema> {
        let operation = spec.by_operation_id("getA")?;
        let response = operation.operation.responses.get("200")?;
        let (_, media) = response.preferred()?;
        media.schema.as_ref()?.properties.get(name).cloned()
    }

    #[test]
    fn a_reference_into_a_sibling_file_resolves_through_the_vfs() {
        let client = Canned(vec![(
            "https://schemas.example.com/user.yaml",
            "User: { type: object, properties: { id: { type: string } } }\n",
        )]);
        let (spec, _) = load_spec(Some(&client));
        let operation = spec
            .by_operation_id("getA")
            .expect("the operation is there");
        let parameters = operation.parameters();
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0].name, "id");
    }

    #[test]
    fn a_remote_reference_inside_the_allow_list_resolves() {
        let client = Canned(vec![(
            "https://schemas.example.com/user.yaml",
            "User: { type: object, properties: { id: { type: string } } }\n",
        )]);
        let (spec, _) = load_spec(Some(&client));
        let allowed = property(&spec, "allowed").expect("the allowed property reads");
        assert!(allowed.is(SchemaType::Object));
        assert!(allowed.properties.get("id").is_some());
    }

    #[test]
    fn a_remote_reference_outside_the_allow_list_is_e0806() {
        let client = Canned(vec![(
            "https://schemas.example.com/user.yaml",
            "User: { type: object }\n",
        )]);
        let (_, diagnostics) = load_spec(Some(&client));
        let denied: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == code::E0806)
            .collect();
        assert_eq!(denied.len(), 1, "{:?}", diagnostics.as_slice());
        assert!(
            denied[0].message.contains("elsewhere.example.net"),
            "{}",
            denied[0].message
        );
    }

    #[test]
    fn a_pointer_that_names_nothing_is_e0502_with_the_pointer_of_the_reference() {
        let client = Canned(vec![(
            "https://schemas.example.com/user.yaml",
            "User: { type: object }\n",
        )]);
        let (_, diagnostics) = load_spec(Some(&client));
        let broken: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == code::E0502 && d.message.contains("shared/params.yaml#/Nope"))
            .collect();
        assert_eq!(broken.len(), 1, "{:?}", diagnostics.as_slice());
        assert!(
            broken[0].message.contains("/properties/broken"),
            "{}",
            broken[0].message
        );
    }

    #[test]
    fn local_schema_refuses_every_remote_reference_without_reaching_the_network() {
        let (_, diagnostics) = load_spec(None);
        let refused: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == code::E0806)
            .collect();
        assert_eq!(
            refused.len(),
            2,
            "both hosts are refused: {:?}",
            diagnostics.as_slice()
        );
        assert!(
            refused.iter().all(|d| d
                .help
                .as_deref()
                .is_some_and(|h| h.contains("--local-schema"))),
            "the diagnostic names the flag that caused it"
        );
    }
}

// ---- the allOf policy ----

#[test]
fn all_of_merges_by_property_union() {
    let (spec, diagnostics) = load(
        r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths: {}
components:
  schemas:
    Base:
      type: object
      required: [id]
      properties:
        id: { type: string }
    Widget:
      allOf:
        - $ref: "#/components/schemas/Base"
        - type: object
          required: [name]
          properties:
            name: { type: string }
"##,
    );
    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.as_slice());
    let widget = spec
        .components
        .schemas
        .get("Widget")
        .expect("the component reads");
    assert!(widget.all_of.is_empty(), "the members were folded");
    assert_eq!(
        widget.properties.keys().collect::<Vec<_>>(),
        vec!["id", "name"]
    );
    assert_eq!(widget.required, vec!["id".to_owned(), "name".to_owned()]);
}

#[test]
fn conflicting_all_of_members_are_e0508_with_the_pointer() {
    let (_, diagnostics) = load(
        r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths: {}
components:
  schemas:
    Impossible:
      allOf:
        - { type: string }
        - { type: integer }
"##,
    );
    let conflicts: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == code::E0508)
        .collect();
    assert_eq!(conflicts.len(), 1, "{:?}", diagnostics.as_slice());
    assert!(
        conflicts[0]
            .message
            .contains("/components/schemas/Impossible"),
        "{}",
        conflicts[0].message
    );
}

#[test]
fn a_conflicting_property_names_the_property_in_its_pointer() {
    let (_, diagnostics) = load(
        r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths: {}
components:
  schemas:
    Impossible:
      allOf:
        - { type: object, properties: { size: { type: integer, minimum: 60 } } }
        - { type: object, properties: { size: { type: integer, maximum: 50 } } }
"##,
    );
    let conflicts: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == code::E0508)
        .collect();
    assert_eq!(conflicts.len(), 1, "{:?}", diagnostics.as_slice());
    assert!(
        conflicts[0]
            .message
            .contains("/components/schemas/Impossible/properties/size"),
        "{}",
        conflicts[0].message
    );
}

#[test]
fn one_of_is_never_merged_into_its_siblings() {
    let (spec, diagnostics) = load(
        r##"
openapi: 3.1.0
info: { title: Test, version: "1" }
paths: {}
components:
  schemas:
    Either:
      allOf:
        - { type: object, properties: { id: { type: string } } }
        - oneOf:
            - { type: object, properties: { a: { type: string } } }
            - { type: object, properties: { b: { type: string } } }
"##,
    );
    assert!(!diagnostics.has_errors(), "{:?}", diagnostics.as_slice());
    let either = spec
        .components
        .schemas
        .get("Either")
        .expect("the component reads");
    assert_eq!(either.all_of.len(), 1, "the alternative stayed a member");
    assert_eq!(either.all_of[0].one_of.len(), 2);
    assert!(either.properties.get("id").is_some());
}
