//! Spec checks (API-53).
//!
//! These run after the model is read, so they are about what a spec *means*
//! rather than whether it parses: a path that templates a parameter it never
//! declares, two operations claiming one `operationId`, a body nobody can see
//! an example of. `liyasa validate --openapi` is this function and nothing
//! else.

use liyasa_core::diagnostics::{Diagnostic, Diagnostics, code};

use crate::model::{OperationRef, Parameter, ParameterIn, SecuritySchemeKind, Spec};
use crate::tree::Pointer;

/// Runs every check over one loaded spec.
pub fn spec(spec: &Spec) -> Diagnostics {
    let mut diagnostics = Diagnostics::new();
    operation_ids(spec, &mut diagnostics);
    for operation in spec.operations() {
        let at = pointer(&operation);
        path_template(spec, &operation, &at, &mut diagnostics);
        duplicate_parameters(&operation, &at, &mut diagnostics);
        responses(&operation, &at, &mut diagnostics);
        security(spec, &operation, &at, &mut diagnostics);
        examples(&operation, &at, &mut diagnostics);
    }
    unsupported(spec, &mut diagnostics);
    diagnostics
}

/// Where an operation is, written the way a spec author would search for it.
pub fn pointer(operation: &OperationRef<'_>) -> Pointer {
    Pointer::root()
        .push(if operation.webhook {
            "webhooks"
        } else {
            "paths"
        })
        .push(operation.path)
        .push(operation.method.lowercase())
}

fn operation_ids(spec: &Spec, diagnostics: &mut Diagnostics) {
    let mut seen: Vec<(&str, String)> = Vec::new();
    for operation in spec.operations() {
        let Some(id) = operation.operation.operation_id.as_deref() else {
            continue;
        };
        if let Some((_, first)) = seen.iter().find(|(other, _)| *other == id) {
            diagnostics.push(
                Diagnostic::new(
                    code::E0505,
                    format!(
                        "`{id}` is the operation id of both {first} and {}",
                        operation.selector()
                    ),
                )
                .help("an operation id names one operation; it is what links and samples refer to"),
            );
            continue;
        }
        seen.push((id, operation.selector()));
    }
}

/// Every `{name}` in a path has a parameter, and every path parameter is in
/// the path.
fn path_template(
    _spec: &Spec,
    operation: &OperationRef<'_>,
    at: &Pointer,
    diagnostics: &mut Diagnostics,
) {
    if operation.webhook {
        return;
    }
    let declared: Vec<&str> = operation
        .parameters()
        .iter()
        .filter(|p| p.location == ParameterIn::Path)
        .map(|p| p.name.as_str())
        .collect();
    let templated = variables(operation.path);

    for name in &templated {
        if !declared.contains(&name.as_str()) {
            diagnostics.push(
                Diagnostic::new(
                    code::E0501,
                    format!(
                        "{at}: the path templates `{{{name}}}` but declares no parameter for it"
                    ),
                )
                .help(format!(
                    "add `- name: {name}` with `in: path` and `required: true`"
                )),
            );
        }
    }
    for name in declared {
        if !templated.iter().any(|found| found == name) {
            diagnostics.push(
                Diagnostic::new(
                    code::E0501,
                    format!("{at}: `{name}` is a path parameter but the path does not template it"),
                )
                .help(format!(
                    "write the path as `{}`",
                    with(operation.path, name)
                )),
            );
        }
    }
}

/// The `{name}` variables of a path template, in order.
pub fn variables(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = path;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}').map(|at| open + at) else {
            break;
        };
        out.push(rest[open + 1..close].to_owned());
        rest = &rest[close + 1..];
    }
    out
}

fn with(path: &str, name: &str) -> String {
    format!("{}/{{{name}}}", path.trim_end_matches('/'))
}

/// Each list is checked on its own: an operation parameter that shadows a
/// path-item one of the same name is the spec's override rule, not a mistake,
/// but two in the same list is.
fn duplicate_parameters(operation: &OperationRef<'_>, at: &Pointer, diagnostics: &mut Diagnostics) {
    for parameters in [&operation.item.parameters, &operation.operation.parameters] {
        for (index, parameter) in parameters.iter().enumerate() {
            if parameters[..index]
                .iter()
                .any(|other| same(other, parameter))
            {
                diagnostics.push(Diagnostic::new(
                    code::E0501,
                    format!(
                        "{at}: `{}` is declared twice as a {} parameter",
                        parameter.name,
                        parameter.location.as_str()
                    ),
                ));
            }
        }
    }
}

fn same(a: &Parameter, b: &Parameter) -> bool {
    a.name == b.name && a.location == b.location
}

fn responses(operation: &OperationRef<'_>, at: &Pointer, diagnostics: &mut Diagnostics) {
    if operation.operation.responses.is_empty() {
        diagnostics.push(
            Diagnostic::new(
                code::E0501,
                format!("{at}: the operation declares no responses"),
            )
            .help("every operation answers something, even if only `default`"),
        );
        return;
    }
    for status in operation.operation.responses.keys() {
        if !is_status(status) {
            diagnostics.push(Diagnostic::new(
                code::E0501,
                format!("{at}: `{status}` is not a status code or `default`"),
            ));
        }
    }
}

/// A three-digit code, a wildcard such as `2XX`, or `default`.
fn is_status(key: &str) -> bool {
    if key == "default" {
        return true;
    }
    let bytes = key.as_bytes();
    bytes.len() == 3
        && bytes[0].is_ascii_digit()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_digit() || *b == b'X' || *b == b'x')
}

fn security(
    spec: &Spec,
    operation: &OperationRef<'_>,
    at: &Pointer,
    diagnostics: &mut Diagnostics,
) {
    for requirement in operation.security(spec) {
        for (name, _) in requirement.schemes() {
            if spec.components.security_schemes.get(name).is_none() {
                diagnostics.push(
                    Diagnostic::new(
                        code::E0501,
                        format!("{at}: security names `{name}`, which is not a declared scheme"),
                    )
                    .help("declare it under `components.securitySchemes`"),
                );
            }
        }
    }
}

/// An operation whose bodies have no example anywhere: not in the media type,
/// not in its schema, not in `x-liyasa`.
fn examples(operation: &OperationRef<'_>, at: &Pointer, diagnostics: &mut Diagnostics) {
    if operation.operation.liyasa.hidden || !operation.operation.liyasa.examples.is_empty() {
        return;
    }
    let mut missing = Vec::new();
    if let Some(body) = &operation.operation.request_body
        && let Some((media_type, media)) = body.preferred()
        && !has_example(media)
    {
        missing.push(format!("the {media_type} request body"));
    }
    for (status, response) in operation.operation.responses.iter() {
        if !status.starts_with('2') {
            continue;
        }
        if let Some((media_type, media)) = response.preferred()
            && !has_example(media)
        {
            missing.push(format!("the {status} {media_type} response"));
        }
    }
    if missing.is_empty() {
        return;
    }
    diagnostics.push(
        Diagnostic::new(
            code::W0510,
            format!("{at}: no example for {}", missing.join(" or ")),
        )
        .help(
            "add `example` or `examples` to the media type, or to the schema; \
             Liyasa can synthesize one, but the spec's own is what a reader trusts",
        ),
    );
}

fn has_example(media: &crate::model::MediaType) -> bool {
    media.example.is_some()
        || !media.examples.is_empty()
        || media
            .schema
            .as_ref()
            .is_some_and(|schema| crate::example::written(schema).is_some())
}

/// Features a spec may declare that this release reads but does not render.
fn unsupported(spec: &Spec, diagnostics: &mut Diagnostics) {
    for (name, scheme) in spec.components.security_schemes.iter() {
        if matches!(scheme.kind, SecuritySchemeKind::MutualTls) {
            diagnostics.push(
                Diagnostic::new(
                    code::W0511,
                    format!(
                        "/components/securitySchemes/{name}: the playground cannot present a \
                         client certificate"
                    ),
                )
                .help("the operation is documented; its \"Try it\" form is not offered"),
            );
        }
    }
    for (name, example) in spec.components.examples.iter() {
        if example.value.is_none() && example.external_value.is_some() {
            diagnostics.push(
                Diagnostic::new(
                    code::W0511,
                    format!(
                        "/components/examples/{name}: `externalValue` is not fetched at build time"
                    ),
                )
                .help("inline the value with `value`, or link to it from the description"),
            );
        }
    }
}

/// Everything `liyasa validate --openapi` reports for one spec: what loading
/// it had to say, and then the checks above.
pub fn all(loaded: &crate::load::Loaded) -> Diagnostics {
    let mut diagnostics = loaded.diagnostics.clone();
    diagnostics.extend(spec(&loaded.spec));
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load;

    fn check(source: &str) -> Diagnostics {
        let loaded = load::from_bytes("api", "api.yaml", source.as_bytes()).expect("it loads");
        spec(&loaded.spec)
    }

    fn codes(diagnostics: &Diagnostics) -> Vec<&'static str> {
        diagnostics.iter().map(|d| d.code.as_str()).collect()
    }

    const HEAD: &str = "openapi: 3.1.0\ninfo: { title: T, version: \"1\" }\n";

    #[test]
    fn a_templated_path_with_no_parameter_is_reported_with_its_pointer() {
        let found = check(&format!(
            "{HEAD}paths:\n  /users/{{id}}:\n    get:\n      responses:\n        \"200\": {{ description: ok }}\n"
        ));
        let missing: Vec<_> = found
            .iter()
            .filter(|d| d.message.contains("templates `{id}`"))
            .collect();
        assert_eq!(missing.len(), 1, "{:?}", found.as_slice());
        assert!(
            missing[0].message.contains("/paths/~1users~1{id}/get"),
            "{}",
            missing[0].message
        );
    }

    #[test]
    fn a_path_parameter_the_path_does_not_template_is_reported() {
        let found = check(&format!(
            "{HEAD}paths:\n  /users:\n    get:\n      parameters:\n        - {{ name: id, in: path, required: true, schema: {{ type: string }} }}\n      responses:\n        \"200\": {{ description: ok }}\n"
        ));
        assert!(
            found
                .iter()
                .any(|d| d.message.contains("does not template it")),
            "{:?}",
            found.as_slice()
        );
    }

    #[test]
    fn two_operations_with_one_id_are_e0505_naming_both() {
        let found = check(&format!(
            "{HEAD}paths:\n  /a:\n    get:\n      operationId: same\n      responses:\n        \"200\": {{ description: ok }}\n  /b:\n    get:\n      operationId: same\n      responses:\n        \"200\": {{ description: ok }}\n"
        ));
        let duplicates: Vec<_> = found.iter().filter(|d| d.code == code::E0505).collect();
        assert_eq!(duplicates.len(), 1, "{:?}", found.as_slice());
        assert!(
            duplicates[0].message.contains("GET /a"),
            "{}",
            duplicates[0].message
        );
        assert!(
            duplicates[0].message.contains("GET /b"),
            "{}",
            duplicates[0].message
        );
    }

    #[test]
    fn an_operation_with_no_responses_is_reported() {
        let found = check(&format!(
            "{HEAD}paths:\n  /a:\n    get:\n      summary: x\n"
        ));
        assert!(
            found
                .iter()
                .any(|d| d.message.contains("declares no responses")),
            "{:?}",
            found.as_slice()
        );
    }

    #[test]
    fn a_response_key_that_is_not_a_status_is_reported_and_a_wildcard_is_not() {
        let found = check(&format!(
            "{HEAD}paths:\n  /a:\n    get:\n      responses:\n        \"2XX\": {{ description: ok }}\n        oops: {{ description: no }}\n        default: {{ description: other }}\n"
        ));
        let bad: Vec<_> = found
            .iter()
            .filter(|d| d.message.contains("is not a status code"))
            .collect();
        assert_eq!(bad.len(), 1, "{:?}", found.as_slice());
        assert!(bad[0].message.contains("`oops`"), "{}", bad[0].message);
    }

    #[test]
    fn security_that_names_no_declared_scheme_is_reported() {
        let found = check(&format!(
            "{HEAD}paths:\n  /a:\n    get:\n      security:\n        - missing: []\n      responses:\n        \"200\": {{ description: ok }}\n"
        ));
        assert!(
            found
                .iter()
                .any(|d| d.message.contains("not a declared scheme")),
            "{:?}",
            found.as_slice()
        );
    }

    #[test]
    fn a_body_with_no_example_anywhere_is_w0510() {
        let found = check(&format!(
            "{HEAD}paths:\n  /a:\n    post:\n      requestBody:\n        content:\n          application/json:\n            schema: {{ type: object }}\n      responses:\n        \"200\": {{ description: ok }}\n"
        ));
        assert!(codes(&found).contains(&"W0510"), "{:?}", found.as_slice());
    }

    #[test]
    fn an_example_on_the_schema_is_enough_to_satisfy_the_check() {
        let found = check(&format!(
            "{HEAD}paths:\n  /a:\n    post:\n      requestBody:\n        content:\n          application/json:\n            schema: {{ type: object, examples: [{{}}] }}\n      responses:\n        \"200\": {{ description: ok }}\n"
        ));
        assert!(!codes(&found).contains(&"W0510"), "{:?}", found.as_slice());
    }

    #[test]
    fn mutual_tls_is_w0511_because_the_playground_cannot_drive_it() {
        let found = check(&format!(
            "{HEAD}paths: {{}}\ncomponents:\n  securitySchemes:\n    mtls: {{ type: mutualTLS }}\n"
        ));
        assert!(codes(&found).contains(&"W0511"), "{:?}", found.as_slice());
    }

    #[test]
    fn a_duplicate_parameter_is_reported_once() {
        let found = check(&format!(
            "{HEAD}paths:\n  /a:\n    get:\n      parameters:\n        - {{ name: q, in: query, schema: {{ type: string }} }}\n        - {{ name: q, in: query, schema: {{ type: string }} }}\n      responses:\n        \"200\": {{ description: ok }}\n"
        ));
        let duplicates: Vec<_> = found
            .iter()
            .filter(|d| d.message.contains("declared twice"))
            .collect();
        assert_eq!(duplicates.len(), 1, "{:?}", found.as_slice());
    }

    #[test]
    fn a_clean_spec_produces_nothing() {
        let found = check(&format!(
            "{HEAD}paths:\n  /users/{{id}}:\n    get:\n      operationId: getUser\n      parameters:\n        - {{ name: id, in: path, required: true, schema: {{ type: string }} }}\n      responses:\n        \"204\": {{ description: gone }}\n"
        ));
        assert!(found.is_empty(), "{:?}", found.as_slice());
    }

    #[test]
    fn the_variables_of_a_path_are_read_in_order() {
        assert_eq!(
            variables("/a/{x}/b/{y}"),
            vec!["x".to_owned(), "y".to_owned()]
        );
        assert_eq!(variables("/a/{unclosed"), Vec::<String>::new());
        assert_eq!(variables("/plain"), Vec::<String>::new());
    }
}
