// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The repetition code as a runtime that owns its backend, shaped as a
//! Qodec runtime is, and made an adapter by `owning_adapter_fn` without
//! being changed. A test double of that shape:
//!
//! - every runtime is a [`Backend`], for the runtime above, and owns the
//!   `Backend` below it: [`execute_batch`](Backend::execute_batch) gives
//!   one result per instruction that reads, in input order, with no shot
//!   in it;
//! - a plan is made once ([`CodePlan`]), and a shot's runtime is made from
//!   it over a backend it takes by value, for the shot's life
//!   ([`CodePlan::instantiate`]); it shares the plan by `Rc`, so it is not
//!   `Send`;
//! - the runtime buffers what a batch lowers to and flushes it through its
//!   backend inside `execute_batch`: before it lowers an instruction that
//!   follows a readout, as a runtime that feeds a readout forward must,
//!   and at the end of the batch, so that one batch may flush twice.
//!
//! The glue is the adapter's own code: the way below a state owns
//! ([`BelowHandle`]), wrapped in [`Lower`], is the runtime's backend, and
//! the logic calls the runtime and writes its results
//! ([`through_the_runtime`]). The wrapper is needed: the adapter's crate
//! owns neither `Backend`, which the runtime's crate defines, nor
//! `BelowHandle`, so it cannot implement one for the other.
//!
//! It runs a state per shot: a runtime's results name no shot, so one
//! state serving a call on several shots could not tell them apart. The
//! glue refuses reads below that do not match its readouts one to one, so
//! one state fails the call at its first batch that reads.

use std::rc::Rc;

use executor_protocol::{
    AdapterError, BelowHandle, CallInput, Outcome, Outcomes, Rejected, ShotContext, Value,
};

use crate::logical::Logical;
use crate::repetition3::gadget;
use crate::toy::Op;

/// What runs instructions `I`: a runtime, for the runtime above, or what
/// is below the last runtime.
pub trait Backend<I> {
    /// Why it cannot go on.
    type Error;

    /// Runs `instructions` and gives one result per instruction that
    /// reads, in input order.
    ///
    /// # Errors
    ///
    /// When it cannot go on.
    fn execute_batch(&mut self, instructions: &[I]) -> Result<BatchResult, Self::Error>;
}

/// The results of a batch: one per instruction that reads, in input order.
#[derive(Debug, Default)]
pub struct BatchResult {
    pub results: Vec<Value>,
}

/// The repetition code's plan, made once: each logical instruction's
/// gadget ([`gadget`]).
#[derive(Clone)]
pub struct CodePlan {
    gadget: Rc<dyn Fn(Logical) -> Vec<Op>>,
}

impl Default for CodePlan {
    fn default() -> Self {
        Self {
            gadget: Rc::new(gadget),
        }
    }
}

impl CodePlan {
    /// The runtime of one shot, over `backend`, which it owns for the
    /// shot's life.
    pub fn instantiate<B: Backend<Op>>(&self, backend: B) -> CodeRuntime<B> {
        CodeRuntime {
            plan: self.clone(),
            lower: backend,
            buffer: Vec::new(),
        }
    }
}

/// The runtime of one shot of the code: a [`Backend`] of logical
/// instructions over the backend of physical ones it owns.
pub struct CodeRuntime<B> {
    plan: CodePlan,
    lower: B,
    /// What the batch being run lowers to.
    buffer: Vec<Op>,
}

impl<B: Backend<Op>> CodeRuntime<B> {
    /// Runs what is buffered through the backend, if anything is, and adds
    /// what it reads to `votes`.
    fn flush(&mut self, votes: &mut Vec<Value>) -> Result<(), B::Error> {
        if !self.buffer.is_empty() {
            votes.extend(self.lower.execute_batch(&self.buffer)?.results);
            self.buffer.clear();
        }
        Ok(())
    }
}

impl<B: Backend<Op>> Backend<Logical> for CodeRuntime<B> {
    type Error = B::Error;

    /// Lowers `instructions` into its buffer, flushing it through its
    /// backend before an instruction that follows a readout and at the
    /// end, and gives each logical readout as the majority of its three.
    fn execute_batch(&mut self, instructions: &[Logical]) -> Result<BatchResult, B::Error> {
        let mut votes = Vec::new();
        for &instruction in instructions {
            if self.buffer.iter().any(|op| physical_tag(op).is_some()) {
                self.flush(&mut votes)?;
            }
            self.buffer.extend((self.plan.gadget)(instruction));
        }
        self.flush(&mut votes)?;
        let results = (votes.chunks(3))
            .map(|votes| (votes.iter().filter(|&&v| v == Value::One).count() >= 2).into())
            .collect();
        Ok(BatchResult { results })
    }
}

/// The tag `instruction` reads under, if it reads.
fn physical_tag(instruction: &Op) -> Option<u32> {
    match *instruction {
        Op::Measure { tag, .. } => Some(tag),
        _ => None,
    }
}

/// The way below a state owns, as its runtime's backend.
pub struct Lower(pub BelowHandle<Op>);

/// A batch that reads runs below, and its reads must match its readouts
/// one to one, as one shot's do, and come back in input order; one that
/// does not read is sent, to go below with the other shots' equal batches.
impl Backend<Op> for Lower {
    type Error = String;

    fn execute_batch(&mut self, instructions: &[Op]) -> Result<BatchResult, String> {
        let tags: Vec<_> = instructions.iter().filter_map(physical_tag).collect();
        if tags.is_empty() {
            self.0.send(instructions)?;
            return Ok(BatchResult::default());
        }
        let read = self.0.run(instructions)?;
        // With the tags of a batch distinct, as many reads as tags, each
        // found, match them one to one.
        if read.len() != tags.len() {
            return Err(format!(
                "readouts: {}, reads below: {}",
                tags.len(),
                read.len()
            ));
        }
        let results = (tags.iter())
            .map(|&tag| {
                let found = read.iter().find(|outcome| outcome.tag == tag);
                found.map(|outcome| outcome.value)
            })
            .collect::<Option<_>>()
            .ok_or("a readout below is missing")?;
        Ok(BatchResult { results })
    }
}

/// The state of a shot: its runtime, made from `plan` over the way below
/// it owns.
pub fn instantiated(
    plan: CodePlan,
) -> impl FnMut(ShotContext, BelowHandle<Op>) -> Result<CodeRuntime<Lower>, Rejected> {
    move |_, below| Ok(plan.instantiate(Lower(below)))
}

/// The logic of a shot: runs the call on its runtime, and writes each
/// result under the tag of the logical readout it answers, one result per
/// readout.
///
/// # Errors
///
/// When the runtime cannot go on, or gives other than one result per
/// logical readout, so that no shot is left short of its outcomes.
pub fn through_the_runtime<R: Backend<Logical, Error = String>>(
    runtime: &mut R,
    input: &CallInput<'_, Logical>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    let results = runtime.execute_batch(input.instructions())?.results;
    let tags: Vec<_> = (input.instructions().iter())
        .filter_map(|instruction| match *instruction {
            Logical::MeasureZ { tag, .. } => Some(tag),
            _ => None,
        })
        .collect();
    if results.len() != tags.len() {
        return Err(AdapterError::from(format!(
            "readouts: {}, results from the runtime: {}",
            tags.len(),
            results.len()
        )));
    }
    outcomes.extend(
        tags.into_iter()
            .zip(results)
            .map(|(tag, value)| Outcome::new(tag, value)),
    );
    Ok(())
}
