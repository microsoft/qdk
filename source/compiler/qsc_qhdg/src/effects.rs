// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_semantic_contracts::{EffectDomain, EffectMode};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EffectResource {
    Named(String),
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EffectEvent {
    pub domain: EffectDomain,
    pub mode: EffectMode,
    pub resource: EffectResource,
    pub observable: bool,
}

impl EffectEvent {
    #[must_use]
    pub fn conflicts_with(&self, other: &Self) -> bool {
        let overlaps = self.domain == other.domain
            && (self.resource == other.resource
                || self.resource == EffectResource::Unknown
                || other.resource == EffectResource::Unknown);
        let commuting_reads = self.mode == EffectMode::Read && other.mode == EffectMode::Read;
        overlaps && !commuting_reads
    }
}
