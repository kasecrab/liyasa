//! Resolving a directive name to the component that renders it.
//!
//! One map holds canonical names and aliases together, so `card-group` and
//! `cards` cost the same lookup. Registering a name twice replaces it, which is
//! how a `components/card.jinja` overrides a built-in (CMP-94).

use std::collections::BTreeMap;
use std::sync::Arc;

use liyasa_core::components::{Component, ComponentRegistry};

use crate::render::Render;

/// A component that can also render, which every component in this crate is.
///
/// The frozen `ComponentRegistry::get` hands back a `&dyn Component`, and
/// `Component` has no usable render method yet (RFC 0004), so the registry
/// keeps the wider type and `resolve` returns it.
pub trait AnyComponent: Component + Render {}

impl<T: Component + Render + ?Sized> AnyComponent for T {}

#[derive(Clone, Default)]
pub struct Registry {
    entries: Vec<Arc<dyn AnyComponent>>,
    /// Canonical names and aliases alike, pointing into `entries`.
    by_name: BTreeMap<&'static str, usize>,
    canonical: Vec<&'static str>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every built-in component (PRD §9).
    pub fn builtins() -> Self {
        let mut registry = Self::new();
        crate::components::register_builtins(&mut registry);
        registry
    }

    pub fn register(&mut self, component: Arc<dyn AnyComponent>) -> &mut Self {
        let name = component.name();
        let aliases = component.aliases();
        let at = match self.by_name.get(name) {
            Some(at) => {
                self.entries[*at] = component;
                *at
            }
            None => {
                self.entries.push(component);
                self.canonical.push(name);
                self.canonical.sort_unstable();
                self.entries.len() - 1
            }
        };
        self.by_name.insert(name, at);
        for alias in aliases {
            self.by_name.insert(alias, at);
        }
        self
    }

    pub fn add<T: AnyComponent + 'static>(&mut self, component: T) -> &mut Self {
        self.register(Arc::new(component))
    }

    /// The component behind a name or alias, with its render methods.
    pub fn resolve(&self, name: &str) -> Option<&dyn AnyComponent> {
        let at = *self.by_name.get(name)?;
        self.entries.get(at).map(Arc::as_ref)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Canonical names and aliases, for the "unknown component" suggestion.
    pub fn all_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.by_name.keys().copied()
    }

    /// The registered name closest to `typo`, for `E0313`.
    pub fn suggest(&self, typo: &str) -> Option<&'static str> {
        self.all_names()
            .map(|name| (edit_distance(typo, name), name))
            .filter(|(distance, _)| *distance * 3 <= typo.len().max(3))
            .min()
            .map(|(_, name)| name)
    }
}

impl ComponentRegistry for Registry {
    fn get(&self, name: &str) -> Option<&dyn Component> {
        self.resolve(name)
            .map(|component| component as &dyn Component)
    }

    fn names(&self) -> Vec<&str> {
        self.canonical.to_vec()
    }
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registry")
            .field("components", &self.canonical)
            .finish()
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b_chars: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b_chars.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            let next = (row[j] + 1).min(row[j + 1] + 1).min(diagonal + cost);
            diagonal = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b_chars.len()]
}
