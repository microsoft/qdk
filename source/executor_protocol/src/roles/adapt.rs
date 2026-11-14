// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The adapter's role, [`Adapt`]: a part between a driver and an executor.

use crate::admission::by_part::ByPart;
use crate::{
    Batch, BatchReadouts, Capabilities, Execute, ExecuteError, InstructionSet, Needs, OpcodeMap,
    RunRefusal, Seed, ShotSet,
};

/// Answers each `execute` it receives, in the instructions it accepts, by
/// calling the part below, in the instructions it sends, as many times as
/// it needs, and maps the readouts back.
///
/// Such as a code that turns each logical instruction into physical ones,
/// a noise model that adds instructions, or a debugger that splits a batch
/// to stop between its instructions. The protocol checks each `execute` it
/// receives and each it sends, so it writes only its own logic.
pub trait Adapt {
    // Composition time.

    /// The part's name in messages, such as an admission fault or a
    /// run's failure; by default its type's, `std::any::type_name`.
    ///
    /// A part overrides it when its type does not say what it is: a part
    /// built at run time, such as a runtime made from an instruction set's
    /// description, has one Rust type for every configuration; and the
    /// default's text is a module path, whose format the standard library
    /// does not guarantee.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it receives, which the part above must fit.
    fn accepts(&self) -> &InstructionSet;

    /// The instructions it sends below, which must fit the part below.
    fn sends(&self) -> &InstructionSet;

    /// Takes the receiver's opcode for each instruction it sends; `>>`
    /// calls it once, after the fit.
    fn bind(&mut self, below: OpcodeMap);

    /// What it needs below to serve `above`, the needs of the part above
    /// it; by default the same.
    fn needs_below(&self, above: Needs) -> Needs {
        above
    }

    /// What it offers above, given what is offered `below`; by default the
    /// same. The admission checks the needs of the part above against it;
    /// the part above also counts on its most shots and instructions, and
    /// on whether a run may fix its seed. Where an offer cannot be exact,
    /// such as for two block types drawn from one pool below, the admission
    /// still checks what [`needs_below`](Self::needs_below) passes down
    /// against the executor's own offer.
    ///
    /// A refusal names this adapter when it is short on its own: when its
    /// answer for an offer below that meets exactly what `needs_below`
    /// asks (each block type at that count, and readout parities resolved
    /// only if needed) does not meet the needs of the part above, such as
    /// readout parities it cannot lower, or more qubits than its decoder
    /// takes; or when its answer for the real offer below, which meets
    /// those needs below too, does not. So an adapter that only passes a
    /// shortage below up is not named. The `>>` that makes a runner asks
    /// it for both answers.
    ///
    /// Its answer must not change once the adapter is composed, not even
    /// by its own `provision`: the `>>` that makes a runner reads it once,
    /// before any part is provisioned, and the admission and every check of
    /// a run use that answer.
    fn offers_above(&self, below: Capabilities) -> Capabilities {
        below
    }

    /// Provisions this adapter for `above`, the needs of the part above it,
    /// once the chain is admitted, such as a layout for its logical qubits.
    /// The `>>` that makes a runner calls it once; by default it does
    /// nothing.
    fn provision(&mut self, above: Needs) {
        let _ = above;
    }

    /// Its needs below, listed as this part's; only the crate overrides it,
    /// to list each part it holds.
    #[doc(hidden)]
    fn needs_by_part(&self, above: Needs, by_part: &mut ByPart) -> Needs {
        let below = self.needs_below(above);
        by_part.note_needs(Adapt::name(self), &below);
        below
    }

    /// Its offer above, listed as this part's; only the crate overrides it,
    /// to list each part it holds, and to keep the offer that crosses each
    /// boundary it holds. The `>>` that makes a runner calls it once, before
    /// any part is provisioned.
    #[doc(hidden)]
    fn offers_by_part(&mut self, below: Capabilities, by_part: &mut ByPart) -> Capabilities {
        let above = self.offers_above(below.clone());
        by_part.note_adapter_offer(Adapt::name(self), &above, &below, |exactly| {
            self.offers_above(exactly)
        });
        above
    }

    // Run time.

    /// As [`Execute::ready`].
    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
        async { Ok(()) }
    }

    /// Answers `(batches, seed)` with their readouts: `readouts[i]` answers
    /// `batches[i]`, a row per shot, in its set's order, of a value per
    /// readout slot. It calls `below.execute`, the rest of the chain as one
    /// executor, any number of times, each with the shots it chooses and
    /// `seed` passed on unchanged.
    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
    ) -> impl Future<Output = Result<Vec<BatchReadouts>, ExecuteError>>;

    /// As [`Execute::end_shots`].
    fn end_shots(&mut self, shots: &ShotSet) {
        let _ = shots;
    }
}

impl<A: Adapt + ?Sized> Adapt for &mut A {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn accepts(&self) -> &InstructionSet {
        (**self).accepts()
    }

    fn sends(&self) -> &InstructionSet {
        (**self).sends()
    }

    fn bind(&mut self, below: OpcodeMap) {
        (**self).bind(below);
    }

    fn needs_below(&self, above: Needs) -> Needs {
        (**self).needs_below(above)
    }

    fn offers_above(&self, below: Capabilities) -> Capabilities {
        (**self).offers_above(below)
    }

    fn provision(&mut self, above: Needs) {
        (**self).provision(above);
    }

    fn needs_by_part(&self, above: Needs, by_part: &mut ByPart) -> Needs {
        (**self).needs_by_part(above, by_part)
    }

    fn offers_by_part(&mut self, below: Capabilities, by_part: &mut ByPart) -> Capabilities {
        (**self).offers_by_part(below, by_part)
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
        (**self).ready()
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
    ) -> impl Future<Output = Result<Vec<BatchReadouts>, ExecuteError>> {
        (**self).adapt(batches, seed, below)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        (**self).end_shots(shots);
    }
}
