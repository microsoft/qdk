// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How calls by opcode ([`InstructionCall`]s) reach an executor that
//! numbers their instructions otherwise: [`Renumbered`] is inserted where a
//! driver's calls meet the chain's executor, when a job is given to the
//! chain ([`Chain::run`](crate::Chain::run)), or where an adapter's meet the
//! executor downstream of it, when it is composed
//! ([`compose`](fn@crate::compose)), and they are numbered otherwise.

use std::sync::Arc;

#[cfg(doc)]
use crate::Capabilities;
use crate::{
    Close, Executor, Instruction, InstructionBatch, InstructionBatchRef, InstructionCall,
    InstructionMismatch, Opcode, OpcodeMap, Rejected,
};

/// How calls in one numbering reach an executor that numbers the same
/// instructions otherwise: where each of the sender's instructions is in
/// the executor's set ([`Instruction::renumbering`]).
#[derive(Debug)]
pub struct Renumbering {
    // Crate-visible, not public, so that only the crate builds one: the
    // seal of `Instruction` rests on it.
    pub(crate) map: OpcodeMap,
}

/// A driver's calls reach the chain's executor, and an adapter's reach the
/// executor downstream of it: unchanged when that executor numbers the
/// instructions as their set does, and otherwise through an executor that
/// renumbers each batch ([`Chain::run`](crate::Chain::run) chooses for a
/// driver, [`compose`](fn@crate::compose) for an adapter).
impl Instruction for InstructionCall {
    type Downstream<E>
        = Renumbered<E>
    where
        E: Executor<InstructionBatch<Self>> + Close;

    const BY_OPCODE: bool = true;

    fn renumbering(map: OpcodeMap) -> Option<Renumbering> {
        (!map.is_identity()).then_some(Renumbering { map })
    }

    fn downstream<E: Executor<InstructionBatch<Self>> + Close>(
        inner: E,
        renumbering: Option<&Arc<Renumbering>>,
    ) -> Renumbered<E> {
        Renumbered::new(inner, renumbering)
    }
}

/// The executor inserted where calls by opcode meet an executor that
/// numbers their instructions otherwise: between a driver and the chain's
/// executor ([`Chain::run`](crate::Chain::run)), or between an adapter and
/// the executor downstream of it ([`compose`](fn@crate::compose)). It
/// renumbers each batch into the numbering of the executor it wraps; where
/// the numberings agree, the sender is given this type too, which
/// then passes calls as they are, so that the sender's type is the same
/// either way. In what follows, "the sender" stands for whichever sends the
/// calls, a driver or an adapter. Planned: it becomes an adapter of its
/// own, once adapters map any executor.
///
/// An opcode means something only in its set, and a program's set may
/// declare the chain's instructions in another order, or only some of them:
/// the sender's `x` may be at the opcode of the chain's `reset`. The
/// instructions are matched once, when the job is given to the chain, or
/// the adapter composed
/// ([`InstructionSet::opcodes_in`](crate::InstructionSet::opcodes_in)).
/// When the numbering differs, each batch is copied with each opcode
/// replaced by its match, into a buffer kept from one call to the next, and
/// a call at an opcode the program's set lacks is refused, though the
/// chain's set may have one there; unless an earlier call is outside the
/// executor's set, which is then refused. So the numbering does not change
/// which call outside the executor's set is reported, for an executor that
/// checks each call's opcode and operands against its set before the tags
/// of its readouts: where it refuses a batch in the chain's numbering for
/// one of its instructions, it is at that call
/// (`a_renumbered_batch_is_refused_where_a_batch_in_the_chains_numbering_is`,
/// in `src/tests/instruction_set/renumbered.rs`), as with the executors of
/// the crate's tests. The crate ships no executor of instruction calls: an
/// [`executor_fn`](fn@crate::executor_fn) checks in the order its check
/// gives ([`InstructionSet::check_batch`](crate::InstructionSet::check_batch)
/// before [`check_unique_tags`](crate::check_unique_tags), say). One that
/// checks its tags first may refuse an earlier call there, as
/// [`Rejected::Instruction`] allows of an executor that checks for several
/// reasons in turn. For the order of this refusal against the
/// executor's own refusals, see [`Rejected::Instruction`].
///
/// What it costs, against the executor alone: when calls pass as they are,
/// a test per batch of whether to renumber, at most about 1 ns (measured on
/// an executor that only reads its calls), which any executor that does
/// work hides; when each batch is copied, about 2 ns per call, plus about
/// 4.6 ns per batch. On the toy of `benches/instruction_calls.rs`, whose
/// executor takes about 55 ns per batch of one call, the first is within
/// the noise, and the copy adds about a tenth in batches of one call and a
/// quarter in batches of many. A program that cannot afford the copy builds
/// its calls with the executor's set (its
/// [`Capabilities::instruction_set`], read before the program is made, since
/// the program sees the executor only through its calls), by mnemonic
/// ([`InstructionSet::call`](crate::InstructionSet::call), or
/// [`InstructionSet::opcode`](crate::InstructionSet::opcode) resolved
/// once), and states that set in its needs: its calls then pass as they
/// are.
//
// Known issue, open: it is unexported, so its name is not part of the API:
// a program of calls by opcode sees it only as an executor of its calls.
// Planned to become the renumbering adapter, once adapters map any
// executor.
//
// Why it costs what it does, from instruction counts and cycles under
// `perf` on one machine, against the executor alone:
// - Passing: `call` adds a load and a predictable branch, about 7
//   instructions. The 1 ns shows only when the executor's `call` is
//   inlined into the caller's loop: called through a function the
//   optimizer cannot see into, the executor costs the same with it as
//   without.
// - Copying, per call: about 33 instructions, 2 ns: the opcode looked up
//   in the map and bounds-checked, the kept slot's length checked, the
//   operands' holders compared, the operands copied, the loop's step.
// - Copying, per batch: about 58 instructions more, about 4.6 ns, none of
//   them a stall: `renumber`'s prologue saves 12 registers, since its loop
//   carries the heap paths of `Operands`; its `Result` is returned through
//   memory; the loop's setup; and the call site's checks of that `Result`
//   and of the kept buffer's length.
// - No cheap change helps. Inlining `renumber` costs the passing path about
//   28 instructions; marking it cold, copying in the caller, or one call
//   held on the stack saves at most about 1 ns per batch and slows large
//   batches. Renumbering the batch in place would save about 3 ns per batch
//   of one call, but `Executor::call` would have to take a mutable or owned
//   batch, a change to the protocol's core trait for under a twentieth.
//   What wrapped the toy executor's logic when this was measured (its
//   lifecycle, its shots' states) cost about 40 ns per batch of one call,
//   six times the copy: it is where to look first; `executor_fn`'s cost is
//   to be measured again (`docs/performance.md`).
pub struct Renumbered<E> {
    inner: E,
    /// Where the sender's instructions are in the set of the executor it
    /// wraps; `None` when calls reach it as they are.
    numbering: Option<Arc<Renumbering>>,
    /// The last batch, renumbered. Its calls are kept from one call to the
    /// next, so that copying a batch no larger than an earlier one
    /// allocates no slot; a call's operands are copied into the kept
    /// call's, which allocates when those are inline and the new ones on
    /// the heap, or the new ones outgrow them.
    batch: Vec<InstructionCall>,
}

impl<E> Renumbered<E> {
    /// `inner`, its calls renumbered by `numbering` when there is one.
    fn new(inner: E, numbering: Option<&Arc<Renumbering>>) -> Self {
        Self {
            inner,
            numbering: numbering.map(Arc::clone),
            batch: Vec::new(),
        }
    }
}

/// The refusal of the call at `at`, whose `opcode` the program's set lacks.
fn outside(at: usize, opcode: Opcode) -> Rejected {
    Rejected::Instruction {
        at,
        reason: InstructionMismatch::UnknownOpcode { opcode }.to_string(),
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for Renumbered<E>
{
    type Reply = E::Reply;

    /// Runs `batch`, its calls numbered by the program's set, on the
    /// executor.
    ///
    /// # Errors
    ///
    /// When the numbering differs, [`Rejected::Instruction`] at the first
    /// call at an opcode the program's set lacks, before any of the batch
    /// runs, or at an earlier call whose operands the executor's set
    /// refuses ([`InstructionSet::check_batch`]), so that the numbering
    /// does not change which call outside the executor's set is reported,
    /// for an executor that checks its set before its tags (see the type's
    /// doc); for the order against the executor's own refusals, see
    /// [`Rejected::Instruction`]. Otherwise the executor's rejection, whose
    /// index is the call's in `batch`.
    ///
    /// [`InstructionSet::check_batch`]: crate::InstructionSet::check_batch
    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        let instructions = match &self.numbering {
            None => batch.instructions,
            Some(numbering) => {
                renumber(numbering, batch.instructions, &mut self.batch)
                    .map_err(|lacked| first_outside(numbering, &self.batch, lacked))?;
                &self.batch[..batch.instructions.len()]
            }
        };
        self.inner.call(InstructionBatchRef {
            shots: batch.shots,
            instructions,
        })
    }
}

/// `batch` with each opcode replaced by its match in `numbering`, into
/// `kept`, whose calls are reused.
///
/// # Errors
///
/// [`Rejected::Instruction`] at the first call at an opcode the sender's
/// set lacks.
// Out of line: inlined, it slows both paths of `call`, the one that passes
// calls as they are and this copy (`benches/instruction_calls.rs`).
#[inline(never)]
fn renumber(
    numbering: &Renumbering,
    batch: &[InstructionCall],
    kept: &mut Vec<InstructionCall>,
) -> Result<(), Rejected> {
    for (at, call) in batch.iter().enumerate() {
        let opcode = numbering
            .map
            .get(call.opcode)
            .ok_or_else(|| outside(at, call.opcode))?;
        match kept.get_mut(at) {
            Some(slot) => {
                slot.opcode = opcode;
                slot.operands.clone_from(&call.operands);
            }
            None => kept.push(InstructionCall {
                opcode,
                operands: call.operands.clone(),
            }),
        }
    }
    Ok(())
}

/// The refusal of a batch refused by [`renumber`] (`lacked`), at the first
/// call outside: `renumbered` holds the batch's calls renumbered up to the
/// refused one, and the first of them that the executor's set refuses, if
/// any, is refused instead. Off the path of a batch that runs.
#[cold]
fn first_outside(
    numbering: &Renumbering,
    renumbered: &[InstructionCall],
    lacked: Rejected,
) -> Rejected {
    let Rejected::Instruction { at, .. } = lacked else {
        return lacked;
    };
    let accepted = &numbering.map.accepted;
    accepted
        .check_batch(&renumbered[..at])
        .err()
        .unwrap_or(lacked)
}

impl<E: Close> Close for Renumbered<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.inner.close()
    }
}
