// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    CleanupState, OwnerPath, StableNodeId, graph::Graph, graph_adjacency::Adjacency,
    graph_transaction::GraphFootprint, region::RegionId,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnalysisKind {
    Demand,
    Effects,
    Staging,
    Ownership,
    Alias,
    Cleanup,
}

impl AnalysisKind {
    const fn invalidation_name(self) -> &'static str {
        match self {
            Self::Demand => "demand",
            Self::Effects => "effects",
            Self::Staging => "staging",
            Self::Ownership => "ownership",
            Self::Alias => "alias",
            Self::Cleanup => "cleanup",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case", tag = "scope", content = "identity")]
pub enum AnalysisScope {
    Graph,
    Region(u64),
    Node(StableNodeId),
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AnalysisKey {
    pub kind: AnalysisKind,
    pub scope: AnalysisScope,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "value")]
pub enum AnalysisValue {
    Nodes(BTreeSet<StableNodeId>),
    Effects(BTreeSet<String>),
    Stage(String),
    Ownership(BTreeSet<OwnerPath>),
    Alias(String),
    Cleanup(CleanupState),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CachedAnalysis {
    pub revision: u64,
    pub dependencies: BTreeSet<AnalysisKey>,
    pub value: AnalysisValue,
}

#[derive(Clone, Debug, Default)]
pub struct AnalysisCache {
    entries: BTreeMap<AnalysisKey, (Arc<()>, CachedAnalysis)>,
}

impl AnalysisCache {
    pub fn insert(&mut self, graph: &Graph, key: AnalysisKey, entry: CachedAnalysis) {
        self.entries
            .insert(key, (Arc::clone(&graph.cache_identity), entry));
    }

    /// The value held for this key, if it describes this graph's content and revision.
    ///
    /// Retaining the token prevents address recycling. Clones may share facts
    /// until a mutator detaches their tokens, without hashing graph content or
    /// changing serialized program identity.
    #[must_use]
    pub fn get(&self, graph: &Graph, key: &AnalysisKey) -> Option<&AnalysisValue> {
        let (identity, entry) = self.entries.get(key)?;
        (Arc::ptr_eq(identity, &graph.cache_identity) && entry.revision == graph.revision())
            .then_some(&entry.value)
    }

    pub fn invalidate(&mut self, footprint: &GraphFootprint) -> BTreeSet<AnalysisKey> {
        let mut invalid = BTreeSet::new();
        loop {
            let newly_invalid: Vec<_> = self
                .entries
                .iter()
                .filter(|(key, (_, entry))| {
                    !invalid.contains(*key)
                        && (invalidated_by(key, footprint)
                            || entry
                                .dependencies
                                .iter()
                                .any(|dependency| invalid.contains(dependency)))
                })
                .map(|(key, _)| key.clone())
                .collect();
            if newly_invalid.is_empty() {
                break;
            }
            invalid.extend(newly_invalid);
        }
        for key in &invalid {
            self.entries.remove(key);
        }
        invalid
    }
}

/// Whether an edit disturbs what this entry holds.
///
/// A graph-scoped entry sees the whole program, so any edit at all disturbs it,
/// and the footprint's invalidation names catch the edits that change no node —
/// an ownership change is the one that does.
///
/// A scoped entry is disturbed only by an edit that reaches its scope. The
/// invalidation names say which analyses an edit disturbs, not where, so
/// applying them to a scoped entry would invalidate a region on every edit
/// anywhere and leave the scope doing no work.
fn invalidated_by(key: &AnalysisKey, footprint: &GraphFootprint) -> bool {
    match &key.scope {
        AnalysisScope::Graph => {
            !footprint.is_empty()
                || footprint
                    .invalidations
                    .contains(key.kind.invalidation_name())
        }
        scope => scope_intersects(scope, footprint),
    }
}

fn scope_intersects(scope: &AnalysisScope, footprint: &GraphFootprint) -> bool {
    match scope {
        AnalysisScope::Graph => !footprint.is_empty(),
        AnalysisScope::Region(region) => {
            u32::try_from(*region).is_ok_and(|id| footprint.regions.contains(&RegionId(id)))
        }
        AnalysisScope::Node(node) => footprint.nodes.contains(node),
    }
}

/// Which kind of scope an analysis is computed at.
///
/// Separate from [`AnalysisScope`], which names one particular scope. The kind
/// is a property of the analysis; the identity is a property of the request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisScopeKind {
    Graph,
    Region,
    Node,
}

impl AnalysisScopeKind {
    const fn admits(self, scope: &AnalysisScope) -> bool {
        matches!(
            (self, scope),
            (Self::Graph, AnalysisScope::Graph)
                | (Self::Region, AnalysisScope::Region(_))
                | (Self::Node, AnalysisScope::Node(_))
        )
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AnalysisError {
    #[error("this analysis is not computed at scope {0:?}")]
    ScopeMismatch(AnalysisScope),
    #[error("the analysis scope names a region or node the graph does not have")]
    UnknownScope,
    #[error("the cached entry does not hold the value this analysis declares")]
    ValueMismatch,
}

/// A value an analysis produces that the cache can hold.
///
/// The cache stores one enum over every analysis and an analysis produces one
/// typed thing. These are the conversions between them, so a consumer reads its
/// own type rather than matching a variant it already knows the answer to.
pub trait AnalysisOutput: Clone + Send + Sync + Sized {
    fn into_value(self) -> AnalysisValue;

    fn from_value(value: &AnalysisValue) -> Option<Self>;
}

impl AnalysisOutput for BTreeSet<StableNodeId> {
    fn into_value(self) -> AnalysisValue {
        AnalysisValue::Nodes(self)
    }

    fn from_value(value: &AnalysisValue) -> Option<Self> {
        match value {
            AnalysisValue::Nodes(nodes) => Some(nodes.clone()),
            _ => None,
        }
    }
}

/// What an analysis is given to compute from.
pub struct AnalysisContext<'a> {
    graph: &'a Graph,
    adjacency: &'a Adjacency,
}

impl<'a> AnalysisContext<'a> {
    #[must_use]
    pub const fn new(graph: &'a Graph, adjacency: &'a Adjacency) -> Self {
        Self { graph, adjacency }
    }

    #[must_use]
    pub const fn graph(&self) -> &Graph {
        self.graph
    }

    #[must_use]
    pub const fn adjacency(&self) -> &Adjacency {
        self.adjacency
    }
}

/// Something computed about a graph that the cache can hold and reuse.
///
/// The declaration side of this contract was built before the computation side:
/// [`AnalysisCache::invalidate`] already walks [`Analysis::dependencies`]
/// transitively, and until this trait existed nothing filled the entries it
/// walked.
pub trait Analysis {
    type Output: AnalysisOutput;

    const KIND: AnalysisKind;
    const SCOPE: AnalysisScopeKind;

    /// The analyses this one reads. The cache walks these transitively, so an
    /// analysis whose input is invalidated is invalidated with it.
    fn dependencies() -> &'static [AnalysisKey];

    /// # Errors
    ///
    /// Returns an error when the scope names something the graph does not hold.
    fn compute(
        context: &AnalysisContext<'_>,
        scope: &AnalysisScope,
    ) -> Result<Self::Output, AnalysisError>;
}

/// Reads analyses, computing one only when the cache cannot serve it.
///
/// There is no registration step to forget. An analysis is reachable because a
/// caller names its type.
#[derive(Debug, Default)]
pub struct AnalysisManager {
    cache: AnalysisCache,
    computations: usize,
}

impl AnalysisManager {
    #[must_use]
    pub fn new(cache: AnalysisCache) -> Self {
        Self {
            cache,
            computations: 0,
        }
    }

    /// How many analyses this manager has computed rather than served. Reuse is
    /// a claim about this number, so it is counted rather than described.
    #[must_use]
    pub const fn computations(&self) -> usize {
        self.computations
    }

    pub fn invalidate(&mut self, footprint: &GraphFootprint) -> BTreeSet<AnalysisKey> {
        self.cache.invalidate(footprint)
    }

    /// # Errors
    ///
    /// Returns an error when the scope is not the kind the analysis runs at,
    /// when a cached entry holds another analysis's value, or when the analysis
    /// itself refuses the scope.
    pub fn get<A: Analysis>(
        &mut self,
        context: &AnalysisContext<'_>,
        scope: AnalysisScope,
    ) -> Result<A::Output, AnalysisError> {
        if !A::SCOPE.admits(&scope) {
            return Err(AnalysisError::ScopeMismatch(scope));
        }
        let key = AnalysisKey {
            kind: A::KIND,
            scope,
        };
        let revision = context.graph.revision();
        if let Some(value) = self.cache.get(context.graph, &key) {
            return A::Output::from_value(value).ok_or(AnalysisError::ValueMismatch);
        }
        let output = A::compute(context, &key.scope)?;
        self.computations += 1;
        self.cache.insert(
            context.graph,
            key,
            CachedAnalysis {
                revision,
                dependencies: A::dependencies().iter().cloned().collect(),
                value: output.clone().into_value(),
            },
        );
        Ok(output)
    }
}
