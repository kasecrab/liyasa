//! A [`VectorStore`] held in memory.
//!
//! §6.8 forbids an in-process vector index in a SERVED topology, so this is not
//! a deployment option: it is what the assistant's own tests run against, and
//! what `liyasa build` uses when it embeds a static export that has no database
//! behind it. It implements the same two-stage retrieval as the SQL backends —
//! over-fetch, then filter — so a test that passes here is a test of the
//! ordering rule and not of one backend's SQL.

use std::collections::BTreeMap;
use std::sync::Mutex;

use liyasa_core::ids::{ChunkId, IndexId, Route};
use liyasa_core::net::BoxFut;

use super::{
    Backend, ChunkQuery, ChunkRecord, Hit, IndexDescriptor, IndexError, OVERFETCH, VectorStore,
    cosine,
};
use crate::config::ModelRef;

#[derive(Debug, Default)]
struct Index {
    descriptor: Option<IndexDescriptor>,
    rows: BTreeMap<ChunkId, (ChunkRecord, Vec<f32>)>,
}

#[derive(Debug, Default)]
struct State {
    indexes: BTreeMap<IndexId, Index>,
    active: Option<IndexId>,
    next: u32,
}

#[derive(Debug, Default)]
pub struct MemoryStore {
    state: Mutex<State>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many chunks `index` holds. Used by the re-index progress report.
    pub fn len(&self, index: &IndexId) -> usize {
        self.with(|state| state.indexes.get(index).map_or(0, |i| i.rows.len()))
    }

    pub fn is_empty(&self, index: &IndexId) -> bool {
        self.len(index) == 0
    }

    /// Every index that still exists, oldest first. A swap drops the one it
    /// replaced, so this is normally one entry.
    pub fn indexes(&self) -> Vec<IndexId> {
        self.with(|state| state.indexes.keys().cloned().collect())
    }

    fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        // A poisoned lock means a test panicked while holding it; the data is
        // still consistent because every write here is a single statement.
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut state)
    }
}

fn ready<T>(value: T) -> BoxFut<'static, T>
where
    T: Send + 'static,
{
    Box::pin(std::future::ready(value))
}

impl VectorStore for MemoryStore {
    fn active(&self) -> BoxFut<'_, Result<Option<IndexDescriptor>, IndexError>> {
        ready(Ok(self.with(|state| {
            state
                .active
                .as_ref()
                .and_then(|id| state.indexes.get(id))
                .and_then(|index| index.descriptor.clone())
        })))
    }

    fn create<'a>(
        &'a self,
        model: &'a ModelRef,
        dims: usize,
    ) -> BoxFut<'a, Result<IndexDescriptor, IndexError>> {
        let descriptor = self.with(|state| {
            state.next += 1;
            let id = IndexId::new(format!("mem-{}", state.next));
            let descriptor = IndexDescriptor {
                id: id.clone(),
                model: model.clone(),
                dims,
                backend: Backend::SqliteVec,
                extension_version: None,
                created_at: 0,
            };
            state.indexes.insert(
                id,
                Index {
                    descriptor: Some(descriptor.clone()),
                    rows: BTreeMap::new(),
                },
            );
            descriptor
        });
        ready(Ok(descriptor))
    }

    fn upsert<'a>(
        &'a self,
        index: &'a IndexId,
        chunks: &'a [(ChunkRecord, Vec<f32>)],
    ) -> BoxFut<'a, Result<(), IndexError>> {
        ready(self.with(|state| {
            let Some(target) = state.indexes.get_mut(index) else {
                return Err(IndexError::Unknown(index.clone()));
            };
            let dims = target.descriptor.as_ref().map(|d| d.dims);
            for (record, vector) in chunks {
                if let Some(dims) = dims
                    && vector.len() != dims
                {
                    return Err(IndexError::Dimension {
                        expected: dims,
                        found: vector.len(),
                    });
                }
                target
                    .rows
                    .insert(record.id.clone(), (record.clone(), vector.clone()));
            }
            Ok(())
        }))
    }

    fn hashes<'a>(
        &'a self,
        index: &'a IndexId,
        route: &'a Route,
    ) -> BoxFut<'a, Result<Vec<(ChunkId, String)>, IndexError>> {
        ready(self.with(|state| {
            let Some(target) = state.indexes.get(index) else {
                return Err(IndexError::Unknown(index.clone()));
            };
            Ok(target
                .rows
                .values()
                .filter(|(record, _)| &record.route == route)
                .map(|(record, _)| (record.id.clone(), record.content_hash.clone()))
                .collect())
        }))
    }

    fn delete<'a>(
        &'a self,
        index: &'a IndexId,
        ids: &'a [ChunkId],
    ) -> BoxFut<'a, Result<(), IndexError>> {
        ready(self.with(|state| {
            let Some(target) = state.indexes.get_mut(index) else {
                return Err(IndexError::Unknown(index.clone()));
            };
            for id in ids {
                target.rows.remove(id);
            }
            Ok(())
        }))
    }

    fn query<'a>(
        &'a self,
        v: &'a [f32],
        k: usize,
        filter: &'a ChunkQuery,
    ) -> BoxFut<'a, Result<Vec<Hit>, IndexError>> {
        ready(self.with(|state| {
            let Some(active) = state.active.as_ref().and_then(|id| state.indexes.get(id)) else {
                return Err(IndexError::NoActiveIndex);
            };
            if let Some(dims) = active.descriptor.as_ref().map(|d| d.dims)
                && v.len() != dims
            {
                return Err(IndexError::Dimension {
                    expected: dims,
                    found: v.len(),
                });
            }

            // Stage one: nearest neighbours over the whole index, k over-fetched.
            let mut candidates: Vec<Hit> = active
                .rows
                .values()
                .map(|(record, vector)| Hit {
                    record: record.clone(),
                    score: cosine(v, vector),
                })
                .collect();
            candidates.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.record.id.cmp(&b.record.id))
            });
            candidates.truncate(k.saturating_mul(OVERFETCH));

            // Stage two: metadata.
            candidates.retain(|hit| filter.admits(&hit.record));
            candidates.truncate(k);
            Ok(candidates)
        }))
    }

    fn swap_active<'a>(&'a self, index: &'a IndexId) -> BoxFut<'a, Result<(), IndexError>> {
        ready(self.with(|state| {
            if !state.indexes.contains_key(index) {
                return Err(IndexError::Unknown(index.clone()));
            }
            let replaced = state.active.replace(index.clone());
            if let Some(old) = replaced.filter(|old| old != index) {
                state.indexes.remove(&old);
            }
            Ok(())
        }))
    }
}
