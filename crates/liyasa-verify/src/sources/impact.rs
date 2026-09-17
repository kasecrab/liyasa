//! What a changed value reaches (§14.12, VER-12, VER-23).
//!
//! The walk itself is [`MemoryGraph::paths_to`], which RFC 2001 put beside
//! `GraphStore` because the frozen trait cannot express it: `dependents`
//! returns origins with no edges attached, so a caller chaining two calls has
//! already lost the evidence `Impact::blocks` is declared to carry. This module
//! is what turns a diff into that answer.
//!
//! A fact change reaches a block two ways. Directly, through the `Reads` edge
//! the extractor mints for `fact("plan.pro.price")`, and indirectly through the
//! fact's *source*: a page that documents a source as a whole moves when
//! anything the source produces moves (RFC 2031).

use std::collections::BTreeMap;

use liyasa_core::document::{DepTarget, Edge, EdgeOrigin};
use liyasa_core::verify::{FactChange, GraphStore, Impact, ImpactQuery, StoreError};

use crate::graph::MemoryGraph;

use super::spec::SourceSet;

/// A spec change to one operation, and what changed about it (VER-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationChange {
    pub spec: String,
    pub op: String,
    /// What moved: `parameters`, `responses`, `auth`. VER-12 flags a page with
    /// the diff, so the diff travels with the change.
    pub diff: Vec<String>,
}

/// One operation change and the origins it reaches, with the evidence path.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationImpact {
    pub change: OperationChange,
    pub blocks: Vec<(EdgeOrigin, Vec<Edge>)>,
}

/// [`ImpactQuery`] over the graph it is constructed with (RFC 2032).
pub struct PathImpact<'g> {
    graph: &'g MemoryGraph,
    sources: Option<&'g SourceSet>,
}

impl<'g> PathImpact<'g> {
    pub fn new(graph: &'g MemoryGraph) -> Self {
        Self {
            graph,
            sources: None,
        }
    }

    /// With the source set, a fact change also reaches whatever depends on the
    /// source that produces it.
    pub fn with_sources(graph: &'g MemoryGraph, sources: &'g SourceSet) -> Self {
        Self {
            graph,
            sources: Some(sources),
        }
    }

    /// The query without the parameter the frozen trait insists on.
    ///
    /// Every change is answered, including one that reaches nothing: a fact
    /// that moved and that no page reads is a real answer, and it is the
    /// caller's policy — not this query's — whether that deserves a record.
    pub fn impact_of(&self, changes: &[FactChange]) -> Result<Vec<Impact>, StoreError> {
        changes
            .iter()
            .map(|change| {
                let mut targets = vec![DepTarget::Fact(change.fact.clone())];
                if let Some(source) = self.sources.and_then(|set| set.source_of(&change.fact)) {
                    targets.push(DepTarget::Source(source.to_owned()));
                }
                Ok(Impact {
                    change: change.clone(),
                    blocks: self.reached(&targets)?,
                })
            })
            .collect()
    }

    /// VER-12: exactly the origins that depend on the changed operation.
    pub fn operation_impact(
        &self,
        changes: &[OperationChange],
    ) -> Result<Vec<OperationImpact>, StoreError> {
        changes
            .iter()
            .map(|change| {
                let target = DepTarget::Operation {
                    spec: change.spec.clone(),
                    op: change.op.clone(),
                };
                Ok(OperationImpact {
                    change: change.clone(),
                    blocks: self.reached(std::slice::from_ref(&target))?,
                })
            })
            .collect()
    }

    /// The union over several targets, one shortest path kept per origin.
    fn reached(&self, targets: &[DepTarget]) -> Result<Vec<(EdgeOrigin, Vec<Edge>)>, StoreError> {
        let mut found: BTreeMap<EdgeOrigin, Vec<Edge>> = BTreeMap::new();
        for target in targets {
            for (origin, path) in self.graph.paths_to(target)? {
                match found.get(&origin) {
                    Some(shorter) if shorter.len() <= path.len() => {}
                    _ => {
                        found.insert(origin, path);
                    }
                }
            }
        }
        Ok(found.into_iter().collect())
    }
}

impl ImpactQuery for PathImpact<'_> {
    /// `graph` is checked, not read: the answer comes from the `MemoryGraph`
    /// this query was built with, because `GraphStore` cannot express the walk
    /// (RFC 2001). Handing it a different store is a caller error rather than a
    /// silently different answer, so it is `Conflict` (RFC 2032).
    fn impact(
        &self,
        changes: &[FactChange],
        graph: &dyn GraphStore,
    ) -> Result<Vec<Impact>, StoreError> {
        // TODO(rfc-2032): drop the check once `paths_to` has a trait to live on.
        let handed = std::ptr::from_ref(graph) as *const ();
        let held = std::ptr::from_ref(self.graph) as *const ();
        if handed != held {
            return Err(StoreError::Conflict);
        }
        self.impact_of(changes)
    }
}

#[cfg(test)]
mod tests;
