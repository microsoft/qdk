// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The adapter's role, [`Adapter`]: a part between a driver and an
//! executor.

use crate::composition::chain_check::ChainCheck;
use crate::{
    Batch, BatchReadouts, ExecuteError, Executor, InstructionSet, Needs, NotReady, Offer,
    OpcodeMap, Seed, ShotSet,
};

/// Answers each `execute` it receives, in the instructions it accepts, by
/// calling the part below, in the instructions it sends, as many times as
/// it needs, and maps the readouts back.
///
/// Such as a code that turns each logical instruction into physical ones,
/// a noise model that adds instructions, or a debugger that splits a batch
/// to stop between its instructions. The protocol shapes the readouts of
/// each `execute` it receives and each it sends, and checks each batch of
/// both its sender marked [`checked`](Batch::checked), so it writes only
/// its own logic.
///
/// # Declarations
///
/// The joins ([`DriverJoins`](crate::DriverJoins) and
/// [`AdapterJoins`](crate::AdapterJoins)) read them once, before any run:
/// [`name`](Self::name), [`accepts`](Self::accepts),
/// [`sends`](Self::sends), and, written only by an adapter that changes
/// what the chain needs or offers, [`needs_below`](Self::needs_below) and
/// [`offers_above`](Self::offers_above). The join to the part below hands
/// back what it made of them: [`bind`](Self::bind) gets the fit's opcode
/// map. A join only decides, from what the parts declare; what prepares a
/// part to run belongs to each run.
///
/// Its instruction sets, the needs it passes below and its offer above
/// must not change once it is joined: the fit, `bind`'s map and the
/// admission are made once, from them, and each part gets ready, on every
/// run, for the needs read then.
///
/// # Run
///
/// On every run, the runner gets it ready ([`ready`](Self::ready)) and
/// ends its shots ([`end_shots`](Self::end_shots)), and the part above
/// calls it ([`adapt`](Self::adapt)).
pub trait Adapter {
    /// As [`Executor::name`].
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it receives, which the part above must fit. Each
    /// checked batch it receives is checked against this set as read once,
    /// by the [`on`](crate::DriverJoins::on) that makes a runner.
    fn accepts(&self) -> &InstructionSet;

    /// The instructions it sends below, which must fit the part below.
    fn sends(&self) -> &InstructionSet;

    /// What it needs below to serve `above`, the needs of the part above
    /// it; by default the same.
    ///
    /// Its answer is enough: when this adapter can serve `above` at all, an
    /// offer below that meets exactly this answer lets it serve them, so
    /// its [`offers_above`](Self::offers_above) of that offer meets
    /// `above`. It states every block it takes below, such as a decoder's
    /// workspace qubits, and passes a need for replay down unless it
    /// replays its shots itself, and then offers replay above
    /// ([`offers_above`](Self::offers_above)), which meets the need only
    /// where that offer is seedable too. The verdict of the admission
    /// never depends on this rule, but which parts a refusal names does: an
    /// adapter that asks for less than it needs may be named in a refusal
    /// another part causes.
    ///
    /// The [`on`](crate::DriverJoins::on) that makes a runner reads it
    /// once.
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
    /// its seed only when every part is seedable. So, for a run, an offer
    /// restated from `Offer::unlimited()` keeps the shots and seedability
    /// of the parts below. But it claims replay, which is checked at each
    /// boundary against this offer, its seedability included: an adapter
    /// that does not replay its shots itself keeps `below.replayable`.
    ///
    /// A refusal names this adapter when it is short on its own: when its
    /// answer for an offer below that meets exactly what `needs_below`
    /// asks, as [`ShortOffer::offered`](crate::ShortOffer::offered) says,
    /// does not meet the needs of the part above, such as readout parities
    /// it cannot lower, or more qubits than its decoder takes. Or, by a
    /// fallback, when that answer meets them, but its answer for the real
    /// offer below does not, while the real offer below meets what
    /// `needs_below` asks. So an adapter that only passes a shortage below
    /// up is not named. The [`on`](crate::DriverJoins::on) that makes a
    /// runner asks it for both answers.
    ///
    /// The [`on`](crate::DriverJoins::on) that makes a runner reads it
    /// once, and the admission and every check of a run use that answer.
    fn offers_above(&self, below: Offer) -> Offer {
        below
    }

    /// Receives, once, the translation of the opcodes it sends into the
    /// part below's. An opcode is an instruction's place in its set, and
    /// the part below may number the same instructions differently, so
    /// every instruction it sends must carry the receiver's opcode,
    /// `below.map(opcode)`: an untranslated one names whatever instruction
    /// the receiver has at that place. The join to the part below calls it
    /// once, after the fit and before any run; an adapter keeps the map to
    /// translate each instruction it forwards or adds.
    fn bind(&mut self, below: OpcodeMap);

    /// As [`Executor::ready`], for `above`, the needs of the part above it,
    /// as the admission checked them, such as for a layout of its logical
    /// qubits.
    fn ready(&mut self, above: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        let _ = above;
        async { Ok(()) }
    }

    /// Answers `(batches, seed)` by filling `readouts`, shaped by the
    /// protocol as [`Executor::execute`] says: `readouts[i]` holds those of
    /// `batches[i]`. It calls `below.execute`, the rest of the chain as one
    /// executor, any number of times, each with the shots it chooses,
    /// `seed` passed on unchanged, and readouts of its choice: `readouts`
    /// itself, when it sends below batches of the same shots and readout
    /// slots as those it got, or a [`Readouts`](crate::Readouts) it keeps
    /// to reuse, when it maps the readouts below to its own. Passed below,
    /// `readouts` are shaped again by the boundary below, so it writes into
    /// them only after `below.execute` answers. After a failure below, it
    /// may resend with the same readouts: the protocol shapes them again.
    /// As an executor does, it writes every value of `readouts` and hands
    /// each batch's up in that batch's shape, which the protocol does not
    /// check once it answers; a conformance kit, planned, will check both
    /// in the implementer's tests.
    ///
    /// Each batch it sends below holds only shots of the batches it got: a
    /// rule the adapter keeps, which the protocol does not check, and
    /// whose breach [`Driver::drive`](crate::Driver::drive) states.
    ///
    /// Each batch it sends below for a [`checked`](Batch::checked) batch is
    /// checked too, as
    /// [`checked_if(batch.is_checked())`](Batch::checked_if) marks it, so
    /// that the checks its sender chose reach every boundary below; it may
    /// mark checked a batch it sends for an unchecked one. It is a rule the
    /// adapter keeps, which the protocol does not check; a conformance kit,
    /// planned, will check it in the implementer's tests.
    ///
    /// An `adapt` does its work, calling `below` included, when its future
    /// is first polled, not when it is called, as Rust's futures do by
    /// convention (an `async fn adapt`'s does), and as
    /// [`Executor::execute`] asks of the `execute` the adapter answers for
    /// the chain it heads: a future dropped before its first poll has then
    /// sent nothing below. It is a rule for implementers, which the
    /// protocol does not check.
    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>>;

    /// As [`Executor::end_shots`].
    fn end_shots(&mut self, shots: &ShotSet) {
        let _ = shots;
    }

    // Protocol hook: sealed and hidden. Its parameter type, `ChainCheck`
    // (`src/composition/chain_check.rs`), cannot be named outside this
    // crate, so no other crate can call or override it. It is in the trait
    // because Rust cannot specialise a default method: the protocol's
    // joined parts (`Composed`, in `src/composition/composed/`) override it
    // to add each inner part's facts, so that a refusal names the inner
    // part that was short, and to keep the needs and the offer that cross
    // each inner boundary.

    /// Adds its needs below, given `above`, as this part's, to `chain`; has
    /// the chain `below` add its facts for them; and adds and gives its
    /// offer above, given the offer that comes back. Only the crate
    /// overrides it, to add each part it holds, with the fit faults of
    /// each boundary it holds, top to bottom, and to keep the needs and the
    /// offer that cross each of them. The [`on`](crate::DriverJoins::on)
    /// that makes a runner calls it once.
    #[doc(hidden)]
    fn check_chain(
        &mut self,
        above: Needs,
        below: &mut dyn FnMut(Needs, &mut ChainCheck) -> Offer,
        chain: &mut ChainCheck,
    ) -> Offer {
        let needs_below = self.needs_below(above);
        chain.add_needs(Adapter::name(self), &needs_below);
        let offer_below = below(needs_below, chain);
        let offer_above = self.offers_above(offer_below.clone());
        chain.add_adapter_offer(Adapter::name(self), &offer_above, &offer_below, |exactly| {
            self.offers_above(exactly)
        });
        offer_above
    }
}

impl<A: Adapter + ?Sized> Adapter for &mut A {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn accepts(&self) -> &InstructionSet {
        (**self).accepts()
    }

    fn sends(&self) -> &InstructionSet {
        (**self).sends()
    }

    fn needs_below(&self, above: Needs) -> Needs {
        (**self).needs_below(above)
    }

    fn offers_above(&self, below: Offer) -> Offer {
        (**self).offers_above(below)
    }

    fn bind(&mut self, below: OpcodeMap) {
        (**self).bind(below);
    }

    fn ready(&mut self, above: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        (**self).ready(above)
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Executor,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        (**self).adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        (**self).end_shots(shots);
    }

    fn check_chain(
        &mut self,
        above: Needs,
        below: &mut dyn FnMut(Needs, &mut ChainCheck) -> Offer,
        chain: &mut ChainCheck,
    ) -> Offer {
        (**self).check_chain(above, below, chain)
    }
}
