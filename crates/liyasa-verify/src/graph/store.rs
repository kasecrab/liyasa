//! The dependency table and the queries over it (§14.12).
//!
//! `liyasa-verify` has no database — PRD §6.2.1 puts `sqlx` in `liyasa-store`
//! alone — so the table is in memory and [`DependencyRecord`] is the row shape
//! the server's migration persists (`plan/rfcs/2002-where-the-graph-tables-live.md`).
//! A CLI build has one process and one graph, which is exactly this.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::RwLock;

use liyasa_core::document::{DepTarget, Edge, EdgeKind, EdgeOrigin};
use liyasa_core::ids::{BuildId, PageId, Route};
use liyasa_core::verify::{GraphDiff, GraphStore, StoreError};

/// One row of the dependency table: an edge, the page whose walk produced it,
/// and the build that walk belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyRecord {
    pub build: BuildId,
    pub page: Route,
    pub edge: Edge,
}

/// `Edge` is not `Ord`, but each of its three fields is; this is the key that
/// keeps every answer in a stable order and makes set arithmetic possible.
type Key = (EdgeOrigin, DepTarget, EdgeKind);

fn key(edge: &Edge) -> Key {
    (edge.from.clone(), edge.to.clone(), edge.kind)
}

fn unkey(key: Key) -> Edge {
    Edge {
        from: key.0,
        to: key.1,
        kind: key.2,
    }
}

#[derive(Default)]
struct Table {
    edges: BTreeMap<BuildId, BTreeMap<Route, Vec<Edge>>>,
    /// Builds in the order they were first written to; the last is current.
    order: Vec<BuildId>,
    /// `replace_page_edges` is handed a `Route` while the edges it carries name
    /// a `PageId`, and nothing in the frozen contracts relates the two. The
    /// store is the one place that sees both, so it remembers the pairing;
    /// [`MemoryGraph::paths_to`] cannot follow a page-to-page edge without it.
    routes: BTreeMap<PageId, Route>,
}

impl Table {
    fn current(&self) -> Option<&BTreeMap<Route, Vec<Edge>>> {
        self.edges.get(self.order.last()?)
    }

    fn page_of(&self, origin: &EdgeOrigin) -> Option<DepTarget> {
        let (EdgeOrigin::Block(page, _) | EdgeOrigin::Page(page)) = origin;
        self.routes.get(page).cloned().map(DepTarget::Page)
    }
}

#[derive(Default)]
pub struct MemoryGraph {
    table: RwLock<Table>,
}

impl MemoryGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// States which route a page ID serves, for a caller that knows both.
    /// `replace_page_edges` infers the same pairing from the edges it is given,
    /// which is right whenever a page's edges originate on that page; this is
    /// how a caller corrects it.
    pub fn bind(&self, page: PageId, route: Route) -> Result<(), StoreError> {
        self.write()?.routes.insert(page, route);
        Ok(())
    }

    /// The dependency table, in build then page then edge order.
    pub fn rows(&self) -> Result<Vec<DependencyRecord>, StoreError> {
        let table = self.read()?;
        Ok(table
            .edges
            .iter()
            .flat_map(|(build, pages)| {
                pages.iter().flat_map(move |(page, edges)| {
                    edges.iter().map(move |edge| DependencyRecord {
                        build: *build,
                        page: page.clone(),
                        edge: edge.clone(),
                    })
                })
            })
            .collect())
    }

    /// The build the point queries read: the one most recently written.
    pub fn current_build(&self) -> Result<Option<BuildId>, StoreError> {
        Ok(self.read()?.order.last().copied())
    }

    /// Every origin the target reaches, with the edges that prove it, shortest
    /// path first. This is `dependents` followed as far as it goes, which is
    /// what `ImpactQuery` needs and what `GraphStore` cannot express: its
    /// `dependents` returns origins with no path attached, so a caller chaining
    /// calls loses the evidence between them.
    ///
    /// Only a `DepTarget::Page` can chain, because a page is the one target
    /// that is also an origin, and only for a page whose route this store has
    /// paired with its ID.
    pub fn paths_to(&self, target: &DepTarget) -> Result<Vec<(EdgeOrigin, Vec<Edge>)>, StoreError> {
        let table = self.read()?;
        let Some(pages) = table.current() else {
            return Ok(Vec::new());
        };
        let mut found: BTreeMap<EdgeOrigin, Vec<Edge>> = BTreeMap::new();
        let mut seen: BTreeSet<DepTarget> = BTreeSet::from([target.clone()]);
        let mut queue: VecDeque<(DepTarget, Vec<Edge>)> =
            VecDeque::from([(target.clone(), vec![])]);
        while let Some((to, tail)) = queue.pop_front() {
            for edge in pages.values().flatten().filter(|edge| edge.to == to) {
                let mut path = Vec::with_capacity(tail.len() + 1);
                path.push(edge.clone());
                path.extend_from_slice(&tail);
                if let Some(next) = table.page_of(&edge.from)
                    && seen.insert(next.clone())
                {
                    queue.push_back((next, path.clone()));
                }
                // Breadth first, so the first path to an origin is a shortest
                // one and every later one is no shorter.
                found.entry(edge.from.clone()).or_insert(path);
            }
        }
        Ok(found.into_iter().collect())
    }

    fn read(&self) -> Result<std::sync::RwLockReadGuard<'_, Table>, StoreError> {
        self.table.read().map_err(|_| poisoned())
    }

    fn write(&self) -> Result<std::sync::RwLockWriteGuard<'_, Table>, StoreError> {
        self.table.write().map_err(|_| poisoned())
    }
}

fn poisoned() -> StoreError {
    StoreError::Io("the dependency table's lock is poisoned".to_owned())
}

fn flatten(pages: &BTreeMap<Route, Vec<Edge>>) -> BTreeSet<Key> {
    pages.values().flatten().map(key).collect()
}

impl GraphStore for MemoryGraph {
    fn replace_page_edges(
        &self,
        build: BuildId,
        page: &Route,
        edges: &[Edge],
    ) -> Result<(), StoreError> {
        let mut table = self.write()?;
        if !table.edges.contains_key(&build) {
            table.order.push(build);
        }
        for edge in edges {
            let (EdgeOrigin::Block(id, _) | EdgeOrigin::Page(id)) = &edge.from;
            table.routes.entry(*id).or_insert_with(|| page.clone());
        }
        let mut keys: Vec<Key> = edges.iter().map(key).collect();
        keys.sort();
        keys.dedup();
        table
            .edges
            .entry(build)
            .or_default()
            .insert(page.clone(), keys.into_iter().map(unkey).collect());
        Ok(())
    }

    fn dependents(&self, target: &DepTarget) -> Result<Vec<EdgeOrigin>, StoreError> {
        let table = self.read()?;
        let Some(pages) = table.current() else {
            return Ok(Vec::new());
        };
        Ok(pages
            .values()
            .flatten()
            .filter(|edge| &edge.to == target)
            .map(|edge| edge.from.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }

    fn dependencies(&self, origin: &EdgeOrigin) -> Result<Vec<Edge>, StoreError> {
        let table = self.read()?;
        let Some(pages) = table.current() else {
            return Ok(Vec::new());
        };
        Ok(pages
            .values()
            .flatten()
            .filter(|edge| &edge.from == origin)
            .map(key)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(unkey)
            .collect())
    }

    /// A build this store never saw is [`StoreError::NotFound`], not an empty
    /// edge set: "everything was added" is the answer a mistyped build ID would
    /// get, and it would rebuild the site rather than fail.
    fn diff(&self, from: BuildId, to: BuildId) -> Result<GraphDiff, StoreError> {
        let table = self.read()?;
        let before = flatten(table.edges.get(&from).ok_or(StoreError::NotFound)?);
        let after = flatten(table.edges.get(&to).ok_or(StoreError::NotFound)?);
        Ok(GraphDiff {
            added: after.difference(&before).cloned().map(unkey).collect(),
            removed: before.difference(&after).cloned().map(unkey).collect(),
        })
    }
}

#[cfg(test)]
mod tests;
