use liyasa_core::ids::Route;

use super::Docowners;

const FILE: &str = "\
# Everything, unless something below says otherwise.
/**                  docs@example.com

/api/**              api@example.com  platform@example.com
/api/webhooks/**     webhooks@example.com
/pricing             revenue@example.com

# A directory nobody is on the hook for yet.
/experiments/
";

fn owners(route: &str) -> Option<Vec<String>> {
    Docowners::parse(FILE)
        .owners_of(&Route::new(route))
        .map(<[String]>::to_vec)
}

#[test]
fn the_last_matching_rule_wins_so_the_specific_override_goes_below() {
    assert_eq!(
        owners("/getting-started"),
        Some(vec!["docs@example.com".into()])
    );
    assert_eq!(
        owners("/api/pets"),
        Some(vec![
            "api@example.com".into(),
            "platform@example.com".into()
        ])
    );
    assert_eq!(
        owners("/api/webhooks/events"),
        Some(vec!["webhooks@example.com".into()]),
        "the deeper rule is written later and wins"
    );
    assert_eq!(owners("/pricing"), Some(vec!["revenue@example.com".into()]));
}

#[test]
fn a_rule_that_names_nobody_is_deliberately_unowned_and_not_unmatched() {
    // `/**` matches it too, and would have named docs@example.com, but the
    // later rule takes it away. That is the point of the rule existing.
    assert_eq!(owners("/experiments/new-thing"), Some(Vec::new()));
    assert_eq!(owners("/experiments"), Some(Vec::new()));
}

#[test]
fn a_file_with_no_catch_all_leaves_a_route_unmatched_rather_than_unowned() {
    let narrow = Docowners::parse("/api/**  api@example.com\n");
    assert_eq!(narrow.owners_of(&Route::new("/pricing")), None);
    assert_eq!(
        narrow.owners_of(&Route::new("/api/pets")),
        Some(["api@example.com".to_owned()].as_slice())
    );
}

#[test]
fn one_star_spans_one_segment_and_two_span_any_number() {
    let file = Docowners::parse("/guides/*  one@example.com\n/deep/**  many@example.com\n");
    assert_eq!(
        file.owners_of(&Route::new("/guides/install")),
        Some(["one@example.com".to_owned()].as_slice())
    );
    assert_eq!(
        file.owners_of(&Route::new("/guides/install/linux")),
        None,
        "`*` does not cross a slash"
    );
    assert_eq!(
        file.owners_of(&Route::new("/deep/a/b/c")),
        Some(["many@example.com".to_owned()].as_slice())
    );
    assert_eq!(
        file.owners_of(&Route::new("/deep")),
        Some(["many@example.com".to_owned()].as_slice()),
        "`**` matches nothing as well as something"
    );
}

#[test]
fn comments_blank_lines_and_a_trailing_comment_are_not_rules() {
    let file =
        Docowners::parse("\n# a comment\n\n/pricing  revenue@example.com  # the money page\n   \n");
    assert_eq!(file.rules().len(), 1);
    assert_eq!(
        file.owners_of(&Route::new("/pricing")),
        Some(["revenue@example.com".to_owned()].as_slice()),
        "the comment is not an owner"
    );
}

#[test]
fn an_empty_file_owns_nothing_rather_than_everything() {
    let empty = Docowners::parse("");
    assert!(empty.is_empty());
    assert_eq!(empty.owners_of(&Route::new("/anything")), None);
}

#[test]
fn a_pattern_with_no_wildcard_matches_that_route_and_not_its_children() {
    let file = Docowners::parse("/pricing  revenue@example.com\n");
    assert!(file.owners_of(&Route::new("/pricing")).is_some());
    assert_eq!(file.owners_of(&Route::new("/pricing/enterprise")), None);
}
