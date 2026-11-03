// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a rewrite knows about an operation before it rewrites it.
//!
//! These facts are stated about node identities rather than about graph
//! structure, so they are independent of how the graph is shaped. The driver,
//! the rules, and the patch contract live with the graph it rewrites.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::identity::{ResourceId, StableNodeId};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OperationClass {
    #[default]
    Unknown,
    PureClassical,
    Unitary,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GlobalPhasePolicy {
    #[default]
    Unproved,
    Exact,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum EffectOrderProof {
    #[default]
    Unproved,
    Preserved,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OperationFacts {
    pub class: OperationClass,
    pub resources: BTreeSet<ResourceId>,
    pub inverse_of: Option<String>,
    pub global_phase: GlobalPhasePolicy,
    pub effect_order: EffectOrderProof,
}

/// What a program is allowed to be observed doing.
///
/// Enumerated rather than free-form. A declaration over an open set of strings
/// is a declaration nothing can check: every string is admissible, so a
/// misspelling reads as a different claim rather than as an error, and two
/// passes can believe they agree while naming different things.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Observation {
    ClassicalResults,
    Failure,
    FinalLiveQuantumState,
    MeasurementDistribution,
    OrderedOutput,
    ResourceLifetimes,
    Termination,
}

impl Observation {
    /// Everything a program can be observed doing.
    #[must_use]
    pub fn all() -> BTreeSet<Self> {
        BTreeSet::from([
            Self::ClassicalResults,
            Self::Failure,
            Self::FinalLiveQuantumState,
            Self::MeasurementDistribution,
            Self::OrderedOutput,
            Self::ResourceLifetimes,
            Self::Termination,
        ])
    }
}

#[derive(Clone, Debug)]
pub struct RewriteContext {
    pub demanded_values: BTreeSet<StableNodeId>,
    pub demanded_effects: BTreeSet<StableNodeId>,
    pub demanded_ownership: BTreeSet<StableNodeId>,
    pub demanded_exits: BTreeSet<StableNodeId>,
    pub cleanup_obligations: BTreeSet<StableNodeId>,
    pub operation_facts: BTreeMap<StableNodeId, OperationFacts>,
    pub observations: BTreeSet<Observation>,
}

impl Default for RewriteContext {
    fn default() -> Self {
        Self {
            demanded_values: BTreeSet::new(),
            demanded_effects: BTreeSet::new(),
            demanded_ownership: BTreeSet::new(),
            demanded_exits: BTreeSet::new(),
            cleanup_obligations: BTreeSet::new(),
            operation_facts: BTreeMap::new(),
            observations: Observation::all(),
        }
    }
}

impl RewriteContext {
    #[must_use]
    pub fn is_demanded(&self, node: StableNodeId) -> bool {
        self.demanded_values.contains(&node)
            || self.demanded_effects.contains(&node)
            || self.demanded_ownership.contains(&node)
            || self.demanded_exits.contains(&node)
            || self.cleanup_obligations.contains(&node)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RewriteDiagnostic {
    pub code: String,
    pub rule: Option<String>,
    pub node: Option<StableNodeId>,
    pub message: String,
}
