// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The amounts every chain states, whatever its instruction set:
//! [`Resources`], and each one provided short ([`ResourceMismatch`]).

use std::fmt;

/// Amounts: the most shots a run holds, and the most instructions a batch
/// holds.
///
/// Required, each is the most the part's runs or batches reach; provided,
/// the most the part takes. A part that does not state one (`None`)
/// neither needs nor limits it.
///
/// Built from [`nothing_stated`](Self::nothing_stated), which states
/// nothing, with the builders below, or by setting its fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resources {
    /// The shots a run holds.
    pub shots: Option<u32>,
    /// The instructions a batch holds.
    pub instructions_per_batch: Option<u32>,
}

impl Resources {
    /// No amount stated: required, no need of any; provided, no limit on
    /// any. What a part leaves unstated, composition cannot refuse a chain
    /// on: a part that needs more shots or instructions a batch than this
    /// one holds is joined with it all the same.
    #[must_use]
    pub const fn nothing_stated() -> Self {
        Self {
            shots: None,
            instructions_per_batch: None,
        }
    }

    /// These resources, with `n` shots per run.
    #[must_use]
    pub const fn with_shots(self, n: u32) -> Self {
        Self {
            shots: Some(n),
            ..self
        }
    }

    /// These resources, with `n` instructions per batch.
    #[must_use]
    pub const fn with_instructions_per_batch(self, n: u32) -> Self {
        Self {
            instructions_per_batch: Some(n),
            ..self
        }
    }

    /// Each amount `self` requires above what `provided` states, in the
    /// fields' order.
    pub(crate) fn mismatches(&self, provided: &Self) -> Vec<ResourceMismatch> {
        let shots = short(self.shots, provided.shots)
            .map(|(required, provided)| ResourceMismatch::Shots { required, provided });
        let instructions = short(self.instructions_per_batch, provided.instructions_per_batch).map(
            |(required, provided)| ResourceMismatch::InstructionsPerBatch { required, provided },
        );
        shots.into_iter().chain(instructions).collect()
    }
}

/// `required` and `provided`, when both are stated and `required` is the
/// larger.
fn short(required: Option<u32>, provided: Option<u32>) -> Option<(u32, u32)> {
    let (required, provided) = (required?, provided?);
    (required > provided).then_some((required, provided))
}

/// An amount required above the one provided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResourceMismatch {
    /// More shots per run required than provided.
    Shots {
        /// The shots required.
        required: u32,
        /// The shots provided.
        provided: u32,
    },
    /// More instructions per batch required than provided.
    InstructionsPerBatch {
        /// The instructions required.
        required: u32,
        /// The instructions provided.
        provided: u32,
    },
}

/// `shots per run: 9 required, 8 provided`.
impl fmt::Display for ResourceMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (what, required, provided) = match *self {
            ResourceMismatch::Shots { required, provided } => ("shots per run", required, provided),
            ResourceMismatch::InstructionsPerBatch { required, provided } => {
                ("instructions per batch", required, provided)
            }
        };
        write!(f, "{what}: {required} required, {provided} provided")
    }
}
