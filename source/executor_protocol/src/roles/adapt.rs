// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The adapter's role at run time, [`Adapt`]: a part between a driver and
//! an executor.

use crate::{
    AdapterSetup, Batch, BatchReadouts, Execute, ExecuteError, Needs, NotReady, Seed, ShotSet,
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
/// This is the adapter at run time: on every run, the runner gets it
/// ready ([`ready`](Self::ready)) and ends its shots, and the part above
/// calls it, once `>>` has set it up as an [`AdapterSetup`].
pub trait Adapt: AdapterSetup {
    /// As [`Execute::ready`], for `above`, the needs of the part above it,
    /// as the admission checked them, such as for a layout of its logical
    /// qubits.
    fn ready(&mut self, above: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        let _ = above;
        async { Ok(()) }
    }

    /// Answers `(batches, seed)` by filling `readouts`, shaped by the
    /// protocol as [`Execute::execute`] says: `readouts[i]` holds those of
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
    /// whose breach [`Drive::drive`](crate::Drive::drive) states.
    ///
    /// Each batch it sends below for a [`checked`](Batch::checked) batch is
    /// checked too, as
    /// [`checked_if(batch.is_checked())`](Batch::checked_if) marks it, so
    /// that the checks its sender chose reach every boundary below; it may
    /// mark checked a batch it sends for an unchecked one. It is a rule the
    /// adapter keeps, which the protocol does not check; a conformance kit,
    /// planned, will check it in the implementer's tests.
    ///
    /// Once a call below has run for this `adapt`, answering `Ok` or
    /// failing, it does not answer with a refusal whose
    /// [`retry_class`](crate::ExecuteRefusal::retry_class) is
    /// [`Later`](crate::RetryClass::Later), today
    /// [`ExecuteRefusal::NotReady`](crate::ExecuteRefusal::NotReady): not a
    /// later call's, passed up as is (as `?` does), nor its own. It turns
    /// it into a failure, [`ExecuteError::Failed`] with
    /// [`Failure::Refused`](crate::Failure::Refused), because a part above
    /// that retries a `Later` refusal would send the whole `execute` again,
    /// and the calls that ran would run twice on the same shots. It is a
    /// rule the adapter keeps, which the protocol does not check.
    ///
    /// An `adapt` does its work, calling `below` included, when its future
    /// is first polled, not when it is called, as Rust's futures do by
    /// convention (an `async fn adapt`'s does), and as
    /// [`Execute::execute`] asks of the `execute` the adapter answers for
    /// the chain it heads: a future dropped before its first poll has then
    /// sent nothing below. It is a rule for implementers, which the
    /// protocol does not check.
    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>>;

    /// As [`Execute::end_shots`].
    fn end_shots(&mut self, shots: &ShotSet) {
        let _ = shots;
    }
}

impl<A: Adapt + ?Sized> Adapt for &mut A {
    fn ready(&mut self, above: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        (**self).ready(above)
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        (**self).adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        (**self).end_shots(shots);
    }
}
