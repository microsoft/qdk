// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The bottom backend: it records every instruction it receives and makes up
//! the bits of instructions that report outcomes or flags.

use crate::syntax::{Ins, render_instruction};
use qodec::InstructionSet;
use qodec_runtime::{
    Backend, BackendInstruction, BasicReadout, BatchResult, Bit, OwnedParam, Readout, Status,
};
use smallvec::SmallVec;
use std::{cell::RefCell, fmt, rc::Rc, sync::Arc};

/// Where the bottom backend's outcome and flag bits come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcomes {
    /// Every bit is 0.
    Zero,
    /// Every bit is 1.
    One,
    /// Pseudo-random bits from a seeded xorshift generator.
    Random { seed: u64, state: u64 },
    /// Asks the user for every bit.
    Ask,
}

impl Outcomes {
    pub fn random(seed: u64) -> Self {
        // xorshift needs a non-zero state.
        Outcomes::Random {
            seed,
            state: seed.max(1),
        }
    }
}

impl fmt::Display for Outcomes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcomes::Zero => write!(f, "zero"),
            Outcomes::One => write!(f, "one"),
            Outcomes::Random { seed, .. } => write!(f, "random {seed}"),
            Outcomes::Ask => write!(f, "ask"),
        }
    }
}

/// Asks the user for the bit described by its argument. `Err` aborts the shot.
pub type AskFn = Box<dyn FnMut(&str) -> Result<Bit, String>>;

/// Produces the bits, as set with `:outcomes`. Shared by the session, which
/// changes the mode, and the bottom backend, which draws from it.
pub struct Oracle {
    pub mode: Outcomes,
    pub ask: AskFn,
}

impl Oracle {
    fn next(&mut self, describe: impl FnOnce() -> String) -> Result<Bit, String> {
        Ok(Some(match &mut self.mode {
            Outcomes::Zero => false,
            Outcomes::One => true,
            Outcomes::Random { state, .. } => {
                *state ^= *state << 13;
                *state ^= *state >> 7;
                *state ^= *state << 17;
                *state & 1 == 1
            }
            Outcomes::Ask => return (self.ask)(&describe()),
        }))
    }
}

/// A bottom instruction and the result it returned, if it reports any.
pub struct Executed {
    pub instruction: Ins,
    pub readout: Option<BasicReadout>,
}

/// The batches the bottom backend received, in order.
pub type Log = Vec<Vec<Executed>>;

/// The bottom backend.
pub struct Recorder {
    pub isa: Arc<InstructionSet>,
    pub log: Rc<RefCell<Log>>,
    pub oracle: Rc<RefCell<Oracle>>,
}

impl Backend<usize, OwnedParam, BasicReadout> for Recorder {
    type Error = String;

    fn execute_batch(
        &mut self,
        instructions: &[BackendInstruction<usize, OwnedParam>],
    ) -> Result<BatchResult<BasicReadout>, String> {
        let mut batch = Vec::with_capacity(instructions.len());
        let mut results = SmallVec::new();
        let mut oracle = self.oracle.borrow_mut();
        // Recorded even when it fails half way, to show what was lowered.
        let outcome = (|| -> Result<(), String> {
            for ins in instructions {
                let decl = self
                    .isa
                    .instructions
                    .get(ins.instruction_id)
                    .ok_or_else(|| format!("unknown instruction id {}", ins.instruction_id))?;
                let (observes, flags) = (decl.observe_count(), decl.flags.len());
                let readout = if observes + flags > 0 {
                    let text = render_instruction(&self.isa, ins);
                    let outcomes = (0..observes)
                        .map(|i| oracle.next(|| format!("{text}: outcome {i}")))
                        .collect::<Result<Vec<_>, _>>()?;
                    let flags = decl
                        .flags
                        .iter()
                        .map(|name| oracle.next(|| format!("{text}: flag {name}")))
                        .collect::<Result<Vec<_>, _>>()?;
                    let readout = BasicReadout::new(&outcomes, &flags, Status::default());
                    results.push(readout.clone());
                    Some(readout)
                } else {
                    None
                };
                batch.push(Executed {
                    instruction: ins.clone(),
                    readout,
                });
            }
            Ok(())
        })();
        self.log.borrow_mut().push(batch);
        outcome.map(|()| BatchResult { results })
    }
}
