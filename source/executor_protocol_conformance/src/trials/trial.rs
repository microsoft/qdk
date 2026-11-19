// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Trial`]: one trial of a check, the one a report names, to replay.

use std::fmt;

use crate::splitmix64::mix;

/// A trial of a check: the seed its patterns are drawn from, and its
/// index. Trial 0's patterns are fixed, whatever the seed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Trial {
    /// The seed of the trials it belongs to.
    pub seed: u64,
    /// Its index among them, from 0.
    pub index: u32,
}

/// The seed of every run of a check's trial 0, on the fixed patterns.
const RUN_SEED: u64 = 0x0C0F_FEE5_EED0_0001;

impl Trial {
    /// Whether the trial runs the fixed patterns: trial 0.
    pub(crate) fn is_fixed(self) -> bool {
        self.index == 0
    }

    /// The state of the trial's draws, a function of its seed and index.
    pub(crate) fn draws_seed(self) -> u64 {
        mix(self.seed ^ mix(u64::from(self.index)))
    }

    /// The seed of every run of the trial: the kit's fixed one in trial 0,
    /// else one drawn for the trial ([`Trials`](crate::Trials) says why).
    pub(crate) fn run_seed(self) -> u64 {
        if self.is_fixed() {
            RUN_SEED
        } else {
            mix(self.draws_seed() ^ RUN_SEED)
        }
    }
}

impl fmt::Display for Trial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { seed, index } = self;
        if self.is_fixed() {
            f.write_str("trial 0, on the fixed patterns")
        } else {
            write!(f, "trial {index} of seed {seed:#x}")
        }
    }
}
