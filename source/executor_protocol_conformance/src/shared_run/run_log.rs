// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`RunLog`]: what the parts of a check's chain saw in a run, which the
//! check reads to judge the part: each row the part answered
//! ([`RowRecord`]), each batch the kit's executor got ([`BatchBelow`]), and
//! the faults the wrappers found on the way.

use executor_protocol::{ReadoutValue, ShotConditions, ShotId};

use crate::nonconformance::{Fault, Place};

/// What a run's parts saw.
#[derive(Clone, Default)]
pub(crate) struct RunLog {
    /// The name of the part under test.
    pub(crate) part: String,
    /// Whether the part under test offers shots that are not replayable
    /// ([`Offer::replayable`](executor_protocol::Offer::replayable)), as
    /// the kit read when it mounted the part: an adapter's offer above the
    /// kit's executor's.
    pub(crate) not_replayable: bool,
    /// Each row the part under test answered, in order, for a batch of
    /// the right shape.
    pub(crate) rows: Vec<RowRecord>,
    /// Each batch the part answered with readouts of another shape.
    pub(crate) shape_faults: Vec<Fault>,
    /// Each `end_shots` the part panicked in, told to end shots some of
    /// which it did not hold.
    pub(crate) end_faults: Vec<Fault>,
    /// Whether the run went without the shot its script never names: the
    /// script names shot `u32::MAX`, or the chain refused a run of one
    /// shot more.
    pub(crate) without_unsent: bool,
    /// Each time the part's `accepts` or `offers` read otherwise than
    /// before it was composed.
    pub(crate) accepts_and_offers_faults: Vec<Fault>,
    /// Each batch the kit's executor got.
    pub(crate) below: Vec<BatchBelow>,
    /// `seed.shot(0)` of each `execute` the adapter under test got.
    pub(crate) seeds_above: Vec<(usize, u64)>,
    /// `seed.shot(0)` of the seed the kit's driver got from the runner.
    pub(crate) seed_from_runner: Option<u64>,
    /// The `execute`s the part under test got so far.
    pub(super) executes: usize,
    /// The calls the kit's executor got in the current `execute`.
    pub(super) calls: usize,
}

/// A row the part under test answered.
#[derive(Clone, Debug)]
pub(crate) struct RowRecord {
    pub(crate) execute: usize,
    pub(crate) batch: usize,
    pub(crate) row: usize,
    pub(crate) shot: ShotId,
    pub(crate) values: Vec<ReadoutValue>,
    /// The values the wrapper wrote before the part ran, in a prefill run.
    pub(crate) prefills: Vec<ReadoutValue>,
    pub(crate) conditions: ShotConditions,
}

/// A batch the kit's executor got.
#[derive(Clone, Debug)]
pub(crate) struct BatchBelow {
    pub(crate) execute: usize,
    pub(crate) call: usize,
    pub(crate) batch: usize,
    pub(crate) shots: Vec<ShotId>,
    /// The readout slots its sender stated.
    pub(crate) stated: u32,
    /// The readout slots its instructions count, or `None` for an
    /// instruction outside the set the kit's executor accepts.
    pub(crate) counted: Option<u32>,
    pub(crate) checked: bool,
    /// `seed.shot(0)` of its call.
    pub(crate) seed: u64,
}

impl BatchBelow {
    /// Where it is, for a report.
    pub(crate) fn place(&self) -> Place {
        Place::BatchBelow {
            execute: self.execute,
            call: self.call,
            batch: self.batch,
        }
    }
}
