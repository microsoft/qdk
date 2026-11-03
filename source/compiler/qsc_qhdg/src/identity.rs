// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::graph::NodeId;

static NEXT_GRAPH_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct GraphId(u64);

impl GraphId {
    /// Draws a graph identity no other live graph holds, so a handle cannot be
    /// carried from one graph into another.
    pub(crate) fn allocate() -> Self {
        Self(NEXT_GRAPH_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct StableNodeId(pub u64);

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct SymbolId {
    pub package: String,
    pub item: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ResourceId(pub u64);

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct Handle {
    graph: GraphId,
    slot: u32,
    generation: u32,
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeHandle(pub(crate) Handle);

impl fmt::Debug for NodeHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_tuple("NodeHandle").field(&self.0).finish()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdentityOperation {
    Import,
    Clone,
    Split,
    Fuse,
    Replace,
    CfgDestruction,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct IdentityTransition {
    pub operation: IdentityOperation,
    pub preserved: Vec<StableNodeId>,
    pub created: Vec<StableNodeId>,
    pub retired: Vec<StableNodeId>,
}

impl IdentityTransition {
    #[must_use]
    pub fn checked(
        operation: IdentityOperation,
        mut preserved: Vec<StableNodeId>,
        mut created: Vec<StableNodeId>,
        mut retired: Vec<StableNodeId>,
    ) -> Option<Self> {
        preserved.sort_unstable();
        preserved.dedup();
        created.sort_unstable();
        created.dedup();
        retired.sort_unstable();
        retired.dedup();
        let disjoint = preserved.iter().all(|id| !retired.contains(id))
            && created
                .iter()
                .all(|id| !preserved.contains(id) && !retired.contains(id));
        let shape_is_valid = match operation {
            IdentityOperation::Import => preserved.is_empty() && retired.is_empty(),
            IdentityOperation::Clone => {
                preserved.len() == 1 && created.len() == 1 && retired.is_empty()
            }
            IdentityOperation::Split => {
                preserved.len() == 1 && !created.is_empty() && retired.is_empty()
            }
            IdentityOperation::Fuse => {
                preserved.is_empty() && created.len() == 1 && retired.len() >= 2
            }
            IdentityOperation::Replace | IdentityOperation::CfgDestruction => {
                created.is_empty() && preserved.len() == 1 && retired.is_empty()
            }
        };
        (disjoint && shape_is_valid).then_some(Self {
            operation,
            preserved,
            created,
            retired,
        })
    }
}

impl Handle {
    pub(crate) const fn new(graph: GraphId, slot: u32, generation: u32) -> Self {
        Self {
            graph,
            slot,
            generation,
        }
    }

    pub(crate) const fn graph(self) -> GraphId {
        self.graph
    }

    pub(crate) const fn slot(self) -> u32 {
        self.slot
    }

    pub(crate) const fn generation(self) -> u32 {
        self.generation
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Handle")
            .field("slot", &self.slot)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

/// Why an identity claim about a graph does not hold.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum IdentityError {
    #[error("stable identity {0:?} is not live in this graph")]
    NotLive(StableNodeId),
    #[error("stable identity {0:?} has not been retired from this graph")]
    NotRetired(StableNodeId),
}

/// Durable node identity for a port-level graph.
///
/// A [`NodeId`] is an arena slot that is handed out again once its node is
/// retired. A [`StableNodeId`] is never reissued. Keeping both here is what
/// stops an arena offset from being mistaken for durable identity.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct NodeIdentity {
    #[serde(skip, default = "GraphId::allocate")]
    graph: GraphId,
    stable: BTreeMap<NodeId, StableNodeId>,
    live: BTreeMap<StableNodeId, NodeId>,
    generations: BTreeMap<NodeId, u32>,
    retired: BTreeSet<StableNodeId>,
    free: Vec<NodeId>,
    next_slot: u32,
    next_stable: u64,
}

type IdentityParts<'a> = (
    &'a BTreeMap<NodeId, StableNodeId>,
    &'a BTreeMap<NodeId, u32>,
    &'a BTreeSet<StableNodeId>,
    &'a [NodeId],
    u32,
    u64,
);

impl Default for NodeIdentity {
    fn default() -> Self {
        Self {
            graph: GraphId::allocate(),
            stable: BTreeMap::new(),
            live: BTreeMap::new(),
            generations: BTreeMap::new(),
            retired: BTreeSet::new(),
            free: Vec::new(),
            next_slot: 0,
            next_stable: 0,
        }
    }
}

impl PartialEq for NodeIdentity {
    fn eq(&self, other: &Self) -> bool {
        // The graph identity names a handle domain rather than graph content,
        // so two separately built but identical graphs stay equal.
        self.stable == other.stable
            && self.live == other.live
            && self.generations == other.generations
            && self.retired == other.retired
            && self.free == other.free
            && self.next_slot == other.next_slot
            && self.next_stable == other.next_stable
    }
}

impl Eq for NodeIdentity {}

impl NodeIdentity {
    pub(crate) fn snapshot_parts(&self) -> IdentityParts<'_> {
        (
            &self.stable,
            &self.generations,
            &self.retired,
            &self.free,
            self.next_slot,
            self.next_stable,
        )
    }

    pub(crate) fn from_snapshot(
        stable: BTreeMap<NodeId, StableNodeId>,
        generations: BTreeMap<NodeId, u32>,
        retired: BTreeSet<StableNodeId>,
        free: Vec<NodeId>,
        next_slot: u32,
        next_stable: u64,
    ) -> Result<Self, qsc_snapshot::Error> {
        let live: BTreeMap<_, _> = stable
            .iter()
            .map(|(node, stable)| (*stable, *node))
            .collect();
        let free_set: BTreeSet<_> = free.iter().copied().collect();
        if live.len() != stable.len()
            || free_set.len() != free.len()
            || generations.len() as u64 != u64::from(next_slot)
            || stable.len() + free.len() != generations.len()
            || stable.len() as u64 + retired.len() as u64 != next_stable
            || generations.values().try_fold(0u64, |sum, generation| {
                sum.checked_add(u64::from(*generation))
            }) != Some(retired.len() as u64)
            || generations.keys().any(|node| node.0 >= next_slot)
            || stable.iter().any(|(node, id)| {
                !generations.contains_key(node) || id.0 >= next_stable || retired.contains(id)
            })
            || retired.iter().any(|id| id.0 >= next_stable)
            || free.iter().any(|node| {
                stable.contains_key(node)
                    || generations
                        .get(node)
                        .is_none_or(|generation| *generation == 0)
            })
        {
            return Err(qsc_snapshot::Error::malformed(
                "inconsistent node identity checkpoint",
            ));
        }
        Ok(Self {
            graph: GraphId::allocate(),
            stable,
            live,
            generations,
            retired,
            free,
            next_slot,
            next_stable,
        })
    }

    /// Reserves an arena slot and a stable identity for a new node.
    pub(crate) fn allocate(&mut self) -> (NodeId, StableNodeId) {
        let node = self.free.pop().unwrap_or_else(|| {
            let node = NodeId(self.next_slot);
            self.next_slot += 1;
            node
        });
        let stable = StableNodeId(self.next_stable);
        self.next_stable += 1;
        self.stable.insert(node, stable);
        self.live.insert(stable, node);
        self.generations.entry(node).or_insert(0);
        (node, stable)
    }

    /// Retires a node, returning its arena slot for reuse while its stable
    /// identity stays retired for good.
    pub(crate) fn retire(&mut self, node: NodeId) -> Option<StableNodeId> {
        let stable = self.stable.remove(&node)?;
        self.live.remove(&stable);
        self.retired.insert(stable);
        let generation = self.generations.entry(node).or_insert(0);
        *generation = generation
            .checked_add(1)
            .expect("node generation exhausted");
        self.free.push(node);
        Some(stable)
    }

    pub(crate) fn stable_id(&self, node: NodeId) -> Option<StableNodeId> {
        self.stable.get(&node).copied()
    }

    pub(crate) fn node(&self, stable: StableNodeId) -> Option<NodeId> {
        self.live.get(&stable).copied()
    }

    pub(crate) fn handle(&self, node: NodeId) -> Option<NodeHandle> {
        if !self.stable.contains_key(&node) {
            return None;
        }
        let generation = *self.generations.get(&node)?;
        Some(NodeHandle(Handle::new(self.graph, node.0, generation)))
    }

    pub(crate) fn resolve(&self, handle: NodeHandle) -> Option<NodeId> {
        let handle = handle.0;
        if handle.graph() != self.graph {
            return None;
        }
        let node = NodeId(handle.slot());
        if !self.stable.contains_key(&node) {
            return None;
        }
        (self.generations.get(&node) == Some(&handle.generation())).then_some(node)
    }

    pub(crate) fn retired(&self) -> impl Iterator<Item = StableNodeId> + '_ {
        self.retired.iter().copied()
    }

    /// Checks a transition against what this graph actually holds. The shape of
    /// the transition is already settled by [`IdentityTransition::checked`];
    /// this is the part that shape alone cannot decide.
    pub(crate) fn check(&self, transition: &IdentityTransition) -> Result<(), IdentityError> {
        for id in transition.preserved.iter().chain(transition.created.iter()) {
            if !self.live.contains_key(id) {
                return Err(IdentityError::NotLive(*id));
            }
        }
        for id in &transition.retired {
            if !self.retired.contains(id) {
                return Err(IdentityError::NotRetired(*id));
            }
        }
        Ok(())
    }
}
