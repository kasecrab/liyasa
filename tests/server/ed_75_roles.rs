//! ED-75's acceptance test, and the fixture the editor's own suite reads.
//!
//! > Given a contributor role; when publish is attempted; then it is refused
//! > and suggest works.
//!
//! The subject is `liyasa_server::auth::roles`, which is AUTH-30's table and
//! the only place the answer lives. The editor has to gate its Publish button
//! on the same table — a button that is enabled and then refused by the server
//! is a worse experience than one that is never enabled, and a button that is
//! *disabled* when the server would have allowed it is a person who cannot do
//! their job. Neither is discoverable by reading the TypeScript.
//!
//! So this test also writes `web/editor/src/role-table.ts` and fails when the
//! checked-in copy drifts, the same way `liyasa-wasm`'s TypeScript declaration
//! test does. `web/editor/src/roles.ts` reads that file and holds no table of
//! its own. It is TypeScript rather than JSON because `web/reader/build.mjs`
//! bundles relative `.ts` imports and nothing else (RFC 1100).

use std::path::PathBuf;

use liyasa_server::auth::roles::{Grant, Permission, Role};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/editor/src/role-table.ts")
}

fn generated() -> String {
    let mut out = String::new();
    out.push_str(
        "// Generated from `liyasa_server::auth::roles` by `tests/server/ed_75_roles.rs`.\n\
         // Do not edit: that test rewrites it and fails when this file and AUTH-30's\n\
         // table disagree.\n\n",
    );

    out.push_str("export const PERMISSION_NAMES = [\n");
    for permission in Permission::ALL {
        out.push_str(&format!("  \"{}\",\n", permission.as_str()));
    }
    out.push_str("] as const;\n\n");

    out.push_str("export const ROLE_NAMES = [\n");
    for role in Role::ALL {
        out.push_str(&format!("  \"{}\",\n", role.as_str()));
    }
    out.push_str("] as const;\n\n");

    out.push_str("export const ROLE_PERMISSIONS: Record<string, string[]> = {\n");
    for role in Role::ALL {
        let granted: Vec<String> = role
            .permissions()
            .iter()
            .map(|permission| format!("\"{}\"", permission.as_str()))
            .collect();
        out.push_str(&format!("  {}: [{}],\n", role.as_str(), granted.join(", ")));
    }
    out.push_str("};\n");
    out
}

#[test]
fn the_checked_in_role_table_is_the_servers() {
    let path = fixture();
    let fresh = generated();
    let committed = std::fs::read_to_string(&path).unwrap_or_default();
    if committed != fresh {
        let _ = std::fs::write(&path, &fresh);
        panic!(
            "{} did not match `liyasa_server::auth::roles`; it has been rewritten, \
             commit it with the change that moved it",
            path.display()
        );
    }
}

#[test]
fn a_contributor_may_draft_and_may_not_publish() {
    let grant = Grant::role(Role::Contributor);
    let permissions = grant.role.permissions();
    assert!(
        permissions.contains(&Permission::ContentDraft),
        "a contributor can suggest an edit"
    );
    assert!(
        !permissions.contains(&Permission::ContentPublish),
        "a contributor cannot publish"
    );
    assert!(
        !permissions.contains(&Permission::ProposalReview),
        "a contributor cannot approve their own suggestion either"
    );
}

#[test]
fn an_editor_publishes_and_a_reviewer_approves_and_neither_does_the_others_job() {
    // The two are separate on purpose: ED-75's point is that a company can open
    // editing to everybody, which only works if the roles below `editor` really
    // cannot ship.
    let editor = Role::Editor.permissions();
    assert!(editor.contains(&Permission::ContentPublish));
    assert!(!editor.contains(&Permission::ProposalReview));

    let reviewer = Role::Reviewer.permissions();
    assert!(reviewer.contains(&Permission::ProposalReview));
    assert!(!reviewer.contains(&Permission::ContentPublish));
}

#[test]
fn a_reader_may_not_even_draft() {
    assert!(Role::Reader.permissions().is_empty());
    assert!(
        !Role::Viewer
            .permissions()
            .contains(&Permission::ContentDraft)
    );
}

#[test]
fn every_role_below_owner_lacks_at_least_one_permission_owner_has() {
    // A table where every role happened to hold every permission would pass
    // each assertion above that names a specific absence, and grant everybody
    // everything.
    for role in Role::ALL {
        if *role == Role::Owner {
            continue;
        }
        assert!(
            role.permissions().len() < Permission::ALL.len(),
            "`{}` holds every permission",
            role.as_str()
        );
    }
    assert_eq!(Role::Owner.permissions().len(), Permission::ALL.len());
}
