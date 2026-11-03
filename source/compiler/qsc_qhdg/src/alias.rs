// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use serde::{Deserialize, Serialize};

use crate::{ResourceId, ownership::OwnerPath};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AliasDecision {
    ProvenDisjoint,
    RuntimeCheck,
    Escape,
    Reject,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AliasFacts {
    pub statically_disjoint: bool,
    pub runtime_comparable: bool,
    pub may_escape: bool,
}

impl AliasFacts {
    #[must_use]
    pub const fn classify(self) -> AliasDecision {
        if self.statically_disjoint {
            AliasDecision::ProvenDisjoint
        } else if self.may_escape {
            AliasDecision::Escape
        } else if self.runtime_comparable {
            AliasDecision::RuntimeCheck
        } else {
            AliasDecision::Reject
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AliasView {
    pub paths: Vec<OwnerPath>,
    pub resources: Vec<ResourceId>,
    pub injective: bool,
}

impl AliasView {
    #[must_use]
    pub fn new(paths: Vec<OwnerPath>, resources: Vec<ResourceId>) -> Self {
        let mut unique = resources.clone();
        unique.sort_unstable();
        unique.dedup();
        Self {
            paths,
            injective: unique.len() == resources.len(),
            resources,
        }
    }

    #[must_use]
    pub fn classify(&self, other: &Self, predicate_proves_disjoint: bool) -> AliasDecision {
        if self
            .resources
            .iter()
            .all(|resource| !other.resources.contains(resource))
            || predicate_proves_disjoint
        {
            AliasDecision::ProvenDisjoint
        } else if self.injective && other.injective {
            AliasDecision::RuntimeCheck
        } else {
            AliasDecision::Reject
        }
    }
}
