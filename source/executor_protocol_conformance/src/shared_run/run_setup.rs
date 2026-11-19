// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`RunSetup`]: how a check sets a run up, which the parts of its chain
//! read: the script the kit's driver sends, how the run fails or treats
//! the part, and the conditions the kit's executor raises.

use executor_protocol::{ShotConditions, ShotId};

use super::raised_conditions::{Raise, Raised, drawn_kinds, raised_in_turn};
use crate::script::Script;

/// How a check sets a run up.
#[derive(Clone, Default)]
pub(crate) struct RunSetup {
    /// What the kit's driver sends.
    pub(crate) script: Script,
    /// Whether the kit's driver sends its batches unchecked.
    pub(crate) unchecked: bool,
    /// The `execute` of the script after which the kit's driver fails
    /// the run, by an error, once the part answered it, if any.
    pub(crate) fails_after: Option<usize>,
    /// The run's index among the prefill runs of a check that the part
    /// writes every readout, if it is one: the wrapper then prefills each
    /// slot before the part runs.
    pub(crate) prefill_run: Option<u32>,
    /// Whether the wrapper calls the part once and drops its future
    /// unpolled before each `execute` it waits for.
    pub(crate) drops_a_call_unpolled: bool,
    /// Whether the wrapper keeps every `end_shots` from the part, the kit's
    /// driver's and the runner's.
    pub(crate) withholds_end_shots: bool,
    /// The conditions the kit's executor raises.
    pub(crate) raises: Vec<Raise>,
}

impl RunSetup {
    /// A run of `script`, with nothing else set.
    pub(crate) fn of(script: Script) -> Self {
        Self {
            script,
            ..Self::default()
        }
    }

    /// The conditions the kit's executor raises on `shot` in its call
    /// `call` of `execute`.
    pub(crate) fn raised(&self, execute: usize, call: usize, shot: ShotId) -> ShotConditions {
        self.raises
            .iter()
            .filter(|raise| raise.execute == execute && raise.shot == shot)
            .fold(ShotConditions::NONE, |all, raise| {
                all | match raise.raised {
                    Raised::InEveryCall(conditions) => conditions,
                    Raised::InFirstCall(conditions) if call == 0 => conditions,
                    Raised::InFirstCall(_) => ShotConditions::NONE,
                    Raised::InTurn { position } => raised_in_turn(call, position),
                    Raised::Drawn { key } => drawn_kinds(key, execute, call, shot),
                }
            })
    }
}
