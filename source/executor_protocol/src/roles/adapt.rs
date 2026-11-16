// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The adapter's role, [`Adapt`]: a part between a driver and an executor.

use crate::admission::by_part::ByPart;
use crate::{
    AppendOnlyReadouts, Batch, Capabilities, Execute, ExecuteError, InstructionSet, Needs,
    OpcodeMap, RunRefusal, Seed, ShotSet,
};

/// Answers each `execute` it receives, in the instructions it accepts, by
/// calling the part below, in the instructions it sends, as many times as
/// it needs, and maps the readouts back.
///
/// Such as a code that turns each logical instruction into physical ones,
/// a noise model that adds instructions, or a debugger that splits a batch
/// to stop between its instructions. The protocol checks the shape of the
/// answer to each `execute` it receives and each it sends, and each batch
/// of both its sender marked [`checked`](Batch::checked), so it writes
/// only its own logic.
pub trait Adapt {
    // Composition time.

    /// As [`Execute::name`].
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it receives, which the part above must fit.
    ///
    /// Its answer must not change once the part is composed: the fit and
    /// `bind`'s map are made once, by `>>`, and the set each checked batch
    /// it receives is checked against is read once, before it is
    /// provisioned.
    fn accepts(&self) -> &InstructionSet;

    /// The instructions it sends below, which must fit the part below.
    ///
    /// Its answer must not change once the part is composed: the fit and
    /// `bind`'s map are made once, by `>>`.
    fn sends(&self) -> &InstructionSet;

    /// Takes the receiver's opcode for each instruction it sends; `>>`
    /// calls it once, after the fit.
    fn bind(&mut self, below: OpcodeMap);

    /// What it needs below to serve `above`, the needs of the part above
    /// it; by default the same.
    ///
    /// Its answer is enough: when this adapter can serve `above` at all, an
    /// offer below that meets exactly this answer lets it serve them, so
    /// its [`offers_above`](Self::offers_above) of that offer meets
    /// `above`. It states every block it takes below, such as a decoder's
    /// workspace qubits. The verdict of the admission never depends on this
    /// rule, but which parts a refusal names does: an adapter that asks
    /// for less than it needs may be named in a refusal another part
    /// causes.
    ///
    /// Its answer for a given `above` must not change once the adapter is
    /// composed, not even by its own `provision`: the admission checks it,
    /// and each part below is provisioned for it, as read before that
    /// `provision`.
    fn needs_below(&self, above: Needs) -> Needs {
        above
    }

    /// What it offers above, given what is offered `below`; by default the
    /// same. The admission checks the needs of the part above against it,
    /// and each batch the part above sends it is checked against its most
    /// instructions. Where an offer cannot be exact, such as for two block
    /// types drawn from one pool below, the admission still checks what
    /// [`needs_below`](Self::needs_below) passes down against the
    /// executor's own offer.
    ///
    /// Its most shots and whether a run may fix its seed can lower the
    /// run's limits, such as for a job queue, but never raise them: each
    /// run is checked against the fewest shots any part offers, and may fix
    /// its seed only when every part is seedable. So an offer restated from
    /// `Capabilities::unlimited()` keeps the shots and seedability of the
    /// parts below.
    ///
    /// A refusal names this adapter when it is short on its own: when its
    /// answer for an offer below that meets exactly what `needs_below`
    /// asks (each block type at that count, and readout parities resolved
    /// only if needed) does not meet the needs of the part above, such as
    /// readout parities it cannot lower, or more qubits than its decoder
    /// takes. Or, by a fallback, when that answer meets them, but its
    /// answer for the real offer below does not, while the real offer
    /// below meets what `needs_below` asks. So an adapter that only passes
    /// a shortage below up is not named. The `>>` that makes a runner asks
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

    /// Answers `(batches, seed)` by appending their readouts to `readouts`,
    /// as [`Execute::execute`] does: entry `start + i` answers `batches[i]`,
    /// `start` being the length of `readouts` before. It calls
    /// `below.execute`, the rest of the chain as one executor, any number
    /// of times, each with the shots it chooses, `seed` passed on
    /// unchanged, and a view of a buffer of its choice: `readouts` itself,
    /// when what below appends is its own answer, or a view of a
    /// [`Readouts`](crate::Readouts) it keeps to reuse, when it maps the
    /// readouts below to its own, or drops an answer that succeeded below.
    /// After a failure below, it may resend through `readouts`: the
    /// protocol removed the failed answer.
    ///
    /// Each batch it sends below holds only shots of the batches it got: a
    /// rule the adapter keeps, which the protocol does not check, and
    /// whose breach [`Drive::drive`](crate::Drive::drive) states.
    ///
    /// It decides whether each batch it sends below is
    /// [`checked`](Batch::checked). The guideline is to carry the checks of
    /// the batch it adapts, with
    /// [`checked_if(batch.is_checked())`](Batch::checked_if), so that the
    /// checks its sender chose reach every boundary below.
    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> impl Future<Output = Result<(), ExecuteError>>;

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
        readouts: &mut AppendOnlyReadouts<'_>,
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        (**self).adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        (**self).end_shots(shots);
    }
}
