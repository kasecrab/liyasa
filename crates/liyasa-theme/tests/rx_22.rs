//! RX-22: breadcrumbs match the tree, and previous and next follow navigation
//! order across groups and tabs.

use liyasa_theme::context::RenderContext;
use liyasa_theme::nav::{Breadcrumbs, Group, Item, Navigation, Tab};
use liyasa_theme::theme::Theme;

fn navigation() -> Navigation {
    Navigation {
        tabs: vec![
            Tab {
                title: "Guides".to_owned(),
                href: Some("/".to_owned()),
                groups: vec![Group {
                    title: "Get started".to_owned(),
                    expanded: true,
                    items: vec![
                        Item {
                            title: "Introduction".to_owned(),
                            route: "/introduction".to_owned(),
                            ..Item::default()
                        },
                        Item {
                            title: "Install".to_owned(),
                            route: "/install".to_owned(),
                            children: vec![Item {
                                title: "Docker".to_owned(),
                                route: "/install/docker".to_owned(),
                                ..Item::default()
                            }],
                            ..Item::default()
                        },
                    ],
                    ..Group::default()
                }],
                ..Tab::default()
            },
            Tab {
                title: "API".to_owned(),
                href: Some("/api".to_owned()),
                groups: vec![Group {
                    title: "Reference".to_owned(),
                    items: vec![Item {
                        title: "Pages".to_owned(),
                        route: "/api/pages".to_owned(),
                        ..Item::default()
                    }],
                    ..Group::default()
                }],
                ..Tab::default()
            },
        ],
        ..Navigation::default()
    }
}

fn page(route: &str) -> RenderContext {
    let navigation = navigation();
    let (previous, next) = navigation.neighbours(route);
    let mut context = RenderContext::sample();
    context.page.route = route.to_owned();
    context.page.breadcrumbs = navigation.trail(route);
    context.page.previous = previous;
    context.page.next = next;
    context.nav.active_route = route.to_owned();
    context.nav.navigation = navigation;
    context.nav.breadcrumbs = Breadcrumbs::Path;
    context
}

fn render(route: &str) -> String {
    Theme::new()
        .expect("the theme builds")
        .render_page(&page(route))
        .expect("the page renders")
}

#[test]
fn breadcrumbs_follow_the_tree_down_to_the_page() {
    let html = render("/install/docker");
    let trail = html
        .split("data-liyasa=\"breadcrumbs\"")
        .nth(1)
        .and_then(|rest| rest.split("</nav>").next())
        .expect("the breadcrumbs render");
    assert!(trail.contains(">Guides<"));
    assert!(trail.contains(">Get started<"));
    assert!(trail.contains("href=\"/install\""));
    assert!(trail.contains("aria-current=\"page\""));
    assert_eq!(
        trail.matches("aria-current=\"page\"").count(),
        1,
        "only the page itself is the current one"
    );
}

#[test]
fn previous_and_next_cross_a_tab_boundary_in_the_markup() {
    let html = render("/install/docker");
    let pagination = html
        .split("data-liyasa=\"pagination\"")
        .nth(1)
        .and_then(|rest| rest.split("</nav>").next())
        .expect("the pagination renders");
    assert!(pagination.contains("href=\"/install\""));
    assert!(pagination.contains("rel=\"prev\""));
    assert!(pagination.contains("href=\"/api/pages\""));
    assert!(pagination.contains("rel=\"next\""));
    assert!(pagination.contains(">Pages<"));
}

#[test]
fn the_first_page_has_no_previous_link() {
    let html = render("/introduction");
    let pagination = html
        .split("data-liyasa=\"pagination\"")
        .nth(1)
        .and_then(|rest| rest.split("</nav>").next())
        .expect("the pagination renders");
    assert!(!pagination.contains("rel=\"prev\""));
    assert!(pagination.contains("rel=\"next\""));
}

#[test]
fn the_eyebrow_replaces_the_trail_when_configured() {
    let mut context = page("/install/docker");
    context.nav.breadcrumbs = Breadcrumbs::Eyebrow;
    context.page.eyebrow = Some("Get started".to_owned());
    let html = Theme::new()
        .expect("the theme builds")
        .render_page(&context)
        .expect("the page renders");
    assert!(html.contains("data-liyasa=\"eyebrow\""));
    assert!(!html.contains("data-liyasa=\"breadcrumbs\""));
}

#[test]
fn the_active_item_and_its_ancestor_are_marked() {
    let html = render("/install/docker");
    assert!(html.contains("href=\"/install/docker\" aria-current=\"page\""));
    assert!(
        html.contains("data-ly-ancestor=\"true\""),
        "the parent of the active page is marked so the sidebar can highlight it"
    );
}
