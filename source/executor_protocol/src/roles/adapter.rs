// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The adapter's role, [`Adapter`]: a part between a driver and an
//! executor.

use crate::composition::DeclaredParts;
use crate::part_id::ChainPosition;
use crate::{
    Batch, BatchReadouts, Capabilities, ExecuteError, Executor, NotReady, OpcodeMap, Seed, ShotSet,
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
/// [`AdapterJoins`](crate::AdapterJoins)) read them before any run:
/// [`name`](Self::name), [`provides`](Self::provides) and
/// [`requires`](Self::requires). The join to the part below hands back
/// what it made of them: [`bind`](Self::bind) gets the fit's opcode map.
/// A join only decides, from what the parts declare; what prepares a part
/// to run belongs to each run.
///
/// What it provides and requires must not change once it is joined: the
/// fits and `bind`'s map are made once, from them, each part gets ready,
/// on every run, for the requirements read then, and a joined part reads
/// their resources and features again on each run, for its boundaries
/// and a joined driver's check of the run.
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

    /// What it provides the part above: the instructions it receives, and
    /// each resource and feature it serves itself. A resource or a feature
    /// it states in neither its provision nor its requirement is what the
    /// part below provides, forwarded, as [`Capabilities`] says; one it
    /// requires is not forwarded, as the adapter serves it itself: the
    /// part above is checked against its provision of it alone, or nothing.
    /// An adapter states only what it changes, such as a decoder's most
    /// logical qubits, or replay it does itself.
    fn provides(&self) -> &Capabilities;

    /// What it requires of the part below: the instructions it sends, and
    /// each resource and feature it uses itself, such as a code's physical
    /// qubits. A resource or a feature it does not state is what the part
    /// above requires, forwarded, as [`Capabilities`] says.
    fn requires(&self) -> &Capabilities;

    /// Receives, once, the translation of the opcodes it sends into the
    /// part below's. An opcode is an instruction's place in its set, and
    /// the part below may number the same instructions differently, so
    /// every instruction it sends must carry the receiver's opcode,
    /// `below.map(opcode)`: an untranslated one names whatever instruction
    /// the receiver has at that place. The join to the part below calls it
    /// once, after the fit and before any run; an adapter keeps the map to
    /// translate each instruction it forwards or adds.
    fn bind(&mut self, below: OpcodeMap);

    /// As [`Executor::ready`], for `required`, what the part above
    /// requires of it, such as for a layout of its logical qubits.
    fn ready(&mut self, required: &Capabilities) -> impl Future<Output = Result<(), NotReady>> {
        let _ = required;
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

    // Protocol hooks: sealed and hidden, as `Driver`'s are.

    /// Adds this part, its name, what it requires and what it provides, to
    /// `parts`. Only the crate overrides it, to add each part it holds,
    /// top to bottom. The [`on`](crate::DriverJoins::on) that makes a
    /// runner calls it once.
    #[doc(hidden)]
    fn declare<'a>(&'a self, parts: &mut DeclaredParts<'a>) {
        parts.push(
            Adapter::name(self),
            Some(self.requires()),
            Some(self.provides()),
        );
    }

    /// [`ready`](Self::ready), for this part at `at` in its chain. Only
    /// the crate overrides it, to name a refusal of a part it holds by
    /// that part's position.
    #[doc(hidden)]
    fn ready_at(
        &mut self,
        required: &Capabilities,
        at: ChainPosition<'_>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        let _ = at;
        self.ready(required)
    }
}

// A lent adapter is the adapter itself: every method forwards, and the
// lint fails the build if a new one falls back to its default instead.
#[deny(clippy::missing_trait_methods)]
impl<A: Adapter + ?Sized> Adapter for &mut A {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn provides(&self) -> &Capabilities {
        (**self).provides()
    }

    fn requires(&self) -> &Capabilities {
        (**self).requires()
    }

    fn bind(&mut self, below: OpcodeMap) {
        (**self).bind(below);
    }

    fn ready(&mut self, required: &Capabilities) -> impl Future<Output = Result<(), NotReady>> {
        (**self).ready(required)
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

    fn declare<'a>(&'a self, parts: &mut DeclaredParts<'a>) {
        (**self).declare(parts);
    }

    fn ready_at(
        &mut self,
        required: &Capabilities,
        at: ChainPosition<'_>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        (**self).ready_at(required, at)
    }
}
