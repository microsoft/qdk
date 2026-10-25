// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Commands issued by adaptive control, the responses that let it go on, and
//! the region ISA: the instructions the Runtime adapter sends to a terminal.

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
    /// Resets `qubit` to |0⟩ without recording a result.
    Reset {
        qubit: QubitID,
    },
    Complete(Vec<OutputRecord>),
}

/// Completion data returned before adaptive control can continue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdaptiveResponse {
    RegionComplete,
    Measurement(MeasurementResult),
    ResetComplete,
}

/// An instruction of the region ISA: what an L3 terminal of regions runs.
///
/// A batch of them is the walk's commands up to and including the next
/// measurement, or the end of the program: the walk needs each measurement's
/// result before it goes on, and nothing else from the terminal, so a
/// measurement is where a batch must end. The terminal applies them in order
/// to the shot's state, which carries over from earlier batches. Its reply has
/// one [`Outcome`](executor_protocol::Outcome) per measurement, tagged with
/// the measurement's `result_id`.
#[derive(Clone, Debug, PartialEq)]
pub enum RegionInstruction {
    /// Applies the region's operations in order.
    ExecuteRegion {
        region_id: RegionId,
        region: QuantumEvolutionRegion,
    },
    /// Measures a qubit into a result, which the reply reports.
    Measure(MeasurementRequest),
    /// Resets `qubit` to |0⟩ without recording a result.
    Reset { qubit: QubitID },
}
