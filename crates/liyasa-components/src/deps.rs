//! The truth-graph edges a component instance contributes (§14.12).
//!
//! Almost every component's edges follow from its schema: an [`Asset`] prop is
//! an asset it embeds, a [`Route`] prop is a link it follows. Components with
//! an edge the schema cannot express — a fact ID, a spec operation, a snippet
//! region — add to what [`from_schema`] returns rather than replacing it.
//!
//! [`Asset`]: liyasa_core::components::PropType::Asset
//! [`Route`]: liyasa_core::components::PropType::Route

use liyasa_core::components::{ComponentInst, PropSchema, PropType};
use liyasa_core::document::{Dep, DepTarget, Edge, EdgeKind, EdgeOrigin};
use liyasa_core::ids::{PageId, Route};

/// The edge origin for an instance.
///
/// §34.9 hands `Component::deps` an instance and no page, so the page is left
/// nil here and the build rewrites it once, when it knows which page it is
/// walking. `EdgeOrigin::Block` keeps the block ID, which is the half only the
/// component knows.
pub fn origin(inst: &ComponentInst) -> EdgeOrigin {
    EdgeOrigin::Block(PageId(ulid::Ulid::nil()), inst.id)
}

/// Every edge the prop schema implies.
pub fn from_schema(inst: &ComponentInst, schema: &PropSchema) -> Vec<Dep> {
    let from = origin(inst);
    let reader = crate::props::Reader::of(inst, schema);
    let mut edges = vec![Edge {
        from: from.clone(),
        to: DepTarget::Component(inst.name.clone()),
        kind: EdgeKind::Documents,
    }];
    for def in &schema.props {
        let values = match def.ty {
            PropType::Asset | PropType::Route => reader.list(def.name),
            _ => continue,
        };
        for value in values {
            let Some(target) = target_of(&def.ty, &value) else {
                continue;
            };
            edges.push(Edge {
                from: from.clone(),
                to: target,
                kind: match def.ty {
                    PropType::Asset => EdgeKind::Embeds,
                    _ => EdgeKind::Links,
                },
            });
        }
    }
    edges
}

fn target_of(ty: &PropType, value: &str) -> Option<DepTarget> {
    if value.is_empty() || value.starts_with('#') {
        return None;
    }
    let external = value.starts_with("http://") || value.starts_with("https://");
    Some(match (ty, external) {
        (PropType::Asset, true) => DepTarget::ExternalUrl(value.to_owned()),
        (PropType::Asset, false) => DepTarget::Asset(value.to_owned()),
        (_, true) => DepTarget::ExternalUrl(value.to_owned()),
        (_, false) => DepTarget::Page(Route::new(value)),
    })
}

/// A `Reads` edge on a fact, for the components that name one.
pub fn fact(inst: &ComponentInst, id: &str) -> Dep {
    Edge {
        from: origin(inst),
        to: DepTarget::Fact(liyasa_core::ids::FactId::new(id)),
        kind: EdgeKind::Reads,
    }
}

/// An `Includes` edge on a snippet or a spec operation.
pub fn includes(inst: &ComponentInst, target: DepTarget) -> Dep {
    Edge {
        from: origin(inst),
        to: target,
        kind: EdgeKind::Includes,
    }
}

/// An `Embeds` edge the schema could not express, e.g. a provider URL.
pub fn embeds(inst: &ComponentInst, target: DepTarget) -> Dep {
    Edge {
        from: origin(inst),
        to: target,
        kind: EdgeKind::Embeds,
    }
}
