// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Commands and responses exchanged by adaptive control and execution targets.

use crate::{MeasurementResult, OutputRecord, QubitID};

use super::QuantumEvolutionRegion;

/// Stable identity assigned to a prepared quantum evolution region.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RegionId(u32);

impl RegionId {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeasurementKind {
    MeasureZ,
    MeasureResetZ,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MeasurementRequest {
    pub kind: MeasurementKind,
    pub qubit: QubitID,
    pub result_id: usize,
}

/// Work issued by adaptive control to the execution target.
#[derive(Clone, Debug, PartialEq)]
pub enum AdaptiveCommand {
    ExecuteRegion {
        region_id: RegionId,
        region: QuantumEvolutionRegion,
    },
    Measure(MeasurementRequest),
    Complete(Vec<OutputRecord>),
}

/// Completion data returned before adaptive control can continue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdaptiveResponse {
    RegionComplete,
    Measurement(MeasurementResult),
}
