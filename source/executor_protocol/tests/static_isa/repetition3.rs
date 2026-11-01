// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The repetition code as an executor, [`Repetition3`]: each logical qubit is
//! a block of three physical qubits; each logical instruction goes down as
//! its gadget, a fixed list of physical instructions ([`gadget`]); and each
//! logical measurement comes back up as the majority of its block's three
//! readouts ([`Decode`]).

use std::collections::BTreeMap;
use std::task::Waker;

use executor_protocol::{
    Close, Executor, InstructionBatch, InstructionBatchRef, InstructionSet, OperandKind, Rejected,
    Reply, ReplyStatus, ShotId, ShotOutcome, Value,
};

use crate::logical::Logical;
use crate::toy::{self, Op};

/// The description of what [`gadget`] sends, so that the code is checked,
/// when it is composed, against the executor downstream. Written by hand,
/// so it must name each instruction of each gadget, and declare what it
/// does: the toy's [`toy::action`].
pub fn gadget_instructions() -> InstructionSet {
    use OperandKind::{Block, Tag};
    InstructionSet::new("gadgets")
        .with_action("x", [Block], toy::action("x"))
        .with_action("coin", [Block], toy::action("coin"))
        .with_action("cx", [Block, Block], toy::action("cx"))
        .with_action("measure", [Block, Tag], toy::action("measure"))
}

/// What a logical instruction becomes: block `b` is physical qubits `3b`,
/// `3b + 1` and `3b + 2`, and logical tag `t` is read as physical tags `3t`,
/// `3t + 1` and `3t + 2`.
pub fn gadget(instruction: Logical) -> Vec<Op> {
    let q = 3 * instruction.block();
    match instruction {
        Logical::X(_) => vec![Op::X(q), Op::X(q + 1), Op::X(q + 2)],
        Logical::Coin(_) => vec![Op::Coin(q), Op::Cx(q, q + 1), Op::Cx(q, q + 2)],
        Logical::MeasureZ { tag, .. } => (0..3)
            .map(|i| Op::Measure {
                qubit: q + i,
                tag: 3 * tag + i,
            })
            .collect(),
    }
}

/// The repetition code as the executor its adapter makes, once, over the
/// whole executor downstream: an executor of logical batches, on any of the
/// shots, upstream of an executor of physical ones.
pub struct Repetition3<E> {
    pub inner: E,
    pub blocks: u32,
}

impl<E: Executor<InstructionBatch<Op>>> Executor<InstructionBatch<Logical>> for Repetition3<E> {
    type Reply = Decode<E::Reply>;

    /// Sends the gadgets of `batch` down as one physical batch, and returns
    /// its reply, decoded.
    ///
    /// # Errors
    ///
    /// [`Rejected::Instruction`] at the first instruction on a block it does
    /// not have, before anything is sent; the inner executor's rejection
    /// otherwise, with an instruction's index mapped back to the logical
    /// instruction whose gadget holds it.
    fn call(&mut self, batch: InstructionBatchRef<'_, Logical>) -> Result<Self::Reply, Rejected> {
        if let Some(at) = (batch.instructions.iter()).position(|i| i.block() >= self.blocks) {
            let reason = format!("{} logical qubits", self.blocks);
            return Err(Rejected::Instruction { at, reason });
        }
        let (mut physical, mut origin) = (Vec::new(), Vec::new());
        for (at, &instruction) in batch.instructions.iter().enumerate() {
            physical.extend(gadget(instruction));
            origin.resize(physical.len(), at);
        }
        let reply = self
            .inner
            .call(InstructionBatchRef {
                shots: batch.shots,
                instructions: &physical,
            })
            .map_err(|rejected| rejected.at_origin(|at| origin[at]))?;
        Ok(Decode {
            inner: reply,
            readouts: Vec::new(),
            votes: BTreeMap::new(),
        })
    }
}

impl<E: Close> Close for Repetition3<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.inner.close()
    }
}

/// The decoding: the physical reply's readouts, three per logical tag of a
/// shot, as logical outcomes. A logical outcome is delivered as soon as its
/// third readout arrives, and its value is 1 when at least two of the three are
/// (a readout with no value counts as no vote for 1). The physical reply's end
/// is the logical reply's end.
pub struct Decode<R> {
    inner: R,
    readouts: Vec<ShotOutcome>,
    votes: BTreeMap<(ShotId, u32), Vec<Value>>,
}

impl<R: Reply<Item = ShotOutcome>> Reply for Decode<R> {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        // The caller's waker goes down: the inner reply wakes the caller.
        let status = self.inner.poll_events(&mut self.readouts, waker);
        for ShotOutcome {
            shot, tag, value, ..
        } in self.readouts.drain(..)
        {
            let logical = (shot, tag / 3);
            let votes = self.votes.entry(logical).or_default();
            votes.push(value);
            if votes.len() == 3 {
                let ones = votes.iter().filter(|&&v| v == Value::One).count();
                out.push(ShotOutcome::new(shot, tag / 3, (ones >= 2).into()));
                self.votes.remove(&logical);
            }
        }
        status
    }
}
