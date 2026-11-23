// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One call across a boundary ([`BoundaryCrossing`]): the receiver as its
//! sender calls it, with the protocol's checks on what crosses.
//!
//! So that no part checks what it is sent, nor shapes the readouts it
//! fills: each call across a boundary of a chain goes through a boundary
//! crossing, built anew for it, both inside a joined part (`composed`)
//! and where the runner's driver meets its executor, with the
//! [`Boundary`] the joins' fit found there, with what arrives on the run. It
//! takes the batches in order, and for each, refuses it if it holds more
//! instructions than the receiver takes
//! ([`BatchFault::TooManyInstructions`]), checked or not, or, for a
//! batch its sender marked [`checked`](Batch::checked), makes the
//! refusals [`Batch::checked`] lists; then shapes its readouts from it, a
//! row per shot of a value per readout slot its sender stated, so that the
//! receiver only fills them and the part above indexes them without a
//! check of its own. An unchecked batch's instructions are not read. A
//! refused batch ends the `execute` before the receiver runs any batch;
//! the readouts are then unspecified, as after any error. It makes a
//! failure of the receiver's panic, naming the receiver at its position
//! (where a panic unwinds: on wasm32, it aborts); where its position is
//! not known, it lets the panic pass up to the boundary above, which names
//! the part at its own position.

use std::panic::resume_unwind;

use super::boundary::Boundary;
use super::part_panics::{catch_panic, panic_message};
use crate::roles::Sealed;
use crate::{
    Batch, BatchFault, BatchReadouts, Capabilities, ExecuteError, ExecuteRefusal, Executor,
    Failure, InstructionSet, Seed, ShotSet,
};

/// A call across a boundary to `inner`, its receiver, with the boundary
/// as the joins' fit found it, with what arrives on the run: each batch
/// is checked against what is accepted and provided across it, the count
/// of every batch, and the instructions of each checked one. The readouts
/// of each batch are shaped from the batch.
pub(crate) struct BoundaryCrossing<'a, E: Executor> {
    inner: &'a mut E,
    /// The boundary: the part that receives `inner`'s calls, if known,
    /// which a panic of `inner` names, what is accepted and provided across
    /// it, and the batch limits.
    boundary: &'a Boundary<E::Accepts>,
}

impl<'a, E: Executor> BoundaryCrossing<'a, E> {
    /// `inner`, below `boundary`.
    pub(crate) fn new(inner: &'a mut E, boundary: &'a Boundary<E::Accepts>) -> Self {
        Self { inner, boundary }
    }
}

impl<E: Executor> Executor for BoundaryCrossing<'_, E> {
    type Accepts = E::Accepts;

    /// The receiver's.
    fn name(&self) -> &str {
        self.inner.name()
    }

    /// What is accepted across the boundary, as the joins' fit read it,
    /// not the receiver's answer now: a receiver whose set changes later
    /// gets checked batches checked against the set read at composition.
    fn accepts(&self) -> &E::Accepts {
        &self.boundary.accepted
    }

    /// What is provided across the boundary, as the joins' fit read it.
    fn provision(&self, sealed: Sealed) -> Option<&Capabilities<E::Accepts>> {
        let _ = sealed;
        self.boundary.provided.as_ref()
    }

    /// Takes `batches` in order: refuses a batch that holds more
    /// instructions than the receiver takes, or a checked batch that is
    /// refused, ending the `execute` before the receiver runs any batch,
    /// with `readouts` unspecified; else shapes `readouts[i]` from
    /// `batches[i]`. Then runs them on the receiver, and makes a failure of
    /// its panic naming it, if its position is known, or else raises the
    /// panic again, for the boundary above (where a panic unwinds). An
    /// empty `execute` returns `Ok` at once, and does not reach the
    /// receiver.
    async fn execute(
        &mut self,
        batches: &[Batch<'_, E::Accepts>],
        seed: Seed,
        readouts: &mut [BatchReadouts<E::Accepts>],
    ) -> Result<(), ExecuteError> {
        assert_eq!(
            readouts.len(),
            batches.len(),
            "an `execute` takes one `BatchReadouts` per batch, as `Readouts::readouts_for` gives"
        );
        let Some(first) = batches.first() else {
            return Ok(());
        };
        for (index, (batch, batch_readouts)) in batches.iter().zip(readouts.iter_mut()).enumerate()
        {
            let refused = |fault| refused(index, batch, fault);
            self.boundary
                .batch_limits()
                .check_count(batch.instructions)
                .map_err(refused)?;
            if batch.is_checked() {
                self.boundary
                    .batch_limits()
                    .check(
                        &self.boundary.accepted,
                        batch.instructions,
                        batch.readout_slots(),
                    )
                    .map_err(refused)?;
            }
            batch_readouts.shape(batch.shots.len(), batch.readout_slots());
        }
        match catch_panic(|| self.inner.execute(batches, seed, readouts)).await {
            Ok(ran) => ran,
            Err(payload) => match &self.boundary.part {
                Some(part) => Err(ExecuteError::Failed {
                    shot: first.shots.first(),
                    failure: Failure::Panicked {
                        part: part.clone(),
                        message: panic_message(payload),
                    },
                }),
                None => resume_unwind(payload),
            },
        }
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.inner.end_shots(shots);
    }
}

/// The refusal of `batches[index]`, `batch`, for `fault`. Out of line
/// and cold, so that the loop over the batches stays as small as when no
/// batch is refused.
#[cold]
#[inline(never)]
fn refused<S: InstructionSet>(
    index: usize,
    batch: &Batch<'_, S>,
    fault: BatchFault,
) -> ExecuteError {
    ExecuteError::Refused(ExecuteRefusal::Batch {
        shot: batch.shots.first(),
        batch: index,
        fault,
    })
}
