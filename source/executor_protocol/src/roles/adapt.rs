// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The adapter's role at run time, [`Adapt`]: a part between a driver and
//! an executor.

use std::slice;

use super::await_part::await_part;
use super::execute::give_every_row;
use super::stream_pieces::{StreamPieces, most_per_piece};
use crate::{
    AdapterSetup, Batch, BatchReadouts, Execute, ExecuteError, FinalRow, Instruction,
    InstructionStream, Needs, NotReady, Seed, ShotSet,
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

    /// As [`adapt`](Self::adapt), and gives `each` each of its rows as
    /// soon as it is final, as
    /// [`Execute::execute_each`](crate::Execute::execute_each)
    /// says, with the same rules. An adapter that gives rows as they are
    /// final calls `below.execute_each` and, as each row below arrives,
    /// writes and gives the rows above that it completes; by default, it is
    /// `adapt`, then, once it answers `Ok`, every row, batch by batch, each
    /// batch in its shots' order, so every adapter serves it, and the rows
    /// of the parts below it reach the caller when it answers; an `adapt`
    /// that fails gives no row.
    fn adapt_each(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
        each: &mut impl FnMut(FinalRow<'_>),
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        async move {
            await_part(self.adapt(batches, seed, below, readouts)).await?;
            give_every_row(batches, readouts, each);
            Ok(())
        }
    }

    /// Answers `stream` as
    /// [`Execute::execute_stream`](crate::Execute::execute_stream) says,
    /// with the same rules, calling `below` as [`adapt`](Self::adapt) does.
    /// By default, it runs the stream in pieces, each one `adapt` of one
    /// batch of at most the most instructions it offers above (its
    /// [`offers_above`](AdapterSetup::offers_above) `below`'s offer, or 1 if
    /// that is 0) and at most
    /// [`MAX_INSTRUCTIONS_PER_PIECE`](crate::MAX_INSTRUCTIONS_PER_PIECE),
    /// so every adapter serves it, holding what the default
    /// `execute_stream` holds. An
    /// adapter whose instructions below follow from those it receives one
    /// by one, such as a code that lowers each logical instruction,
    /// overrides it, to stream its own instructions below with
    /// `below.execute_stream`, so that a stream reaches the executor as one.
    /// An override keeps `execute_stream`'s two rules: its `Ok` means it
    /// took the stream whole, which the protocol's boundary checks, and it
    /// stays cancellable, awaiting at least once per piece's worth of
    /// instructions.
    fn adapt_stream(
        &mut self,
        stream: InstructionStream<'_, impl Iterator<Item = Instruction>>,
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut BatchReadouts,
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        async move {
            let most = most_per_piece(self.offers_above(below.offers()).max_instructions);
            StreamPieces::run(
                stream,
                most,
                self,
                Self::accepts,
                async |adapter, batch, piece_readouts| {
                    await_part(adapter.adapt(
                        slice::from_ref(batch),
                        seed,
                        below,
                        slice::from_mut(piece_readouts),
                    ))
                    .await
                },
                readouts,
            )
            .await
        }
    }

    /// As [`Execute::end_shots`].
    fn end_shots(&mut self, shots: &ShotSet) {
        let _ = shots;
    }
}

// Every method forwarded: one left at its default would not reach `A`'s
// own (a default `end_shots` does nothing; a default `adapt_each` gives no
// row before `adapt` answers; a default `adapt_stream` cuts the stream into
// batches `A` may stream below in one call).
#[deny(clippy::missing_trait_methods)]
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

    fn adapt_each(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut [BatchReadouts],
        each: &mut impl FnMut(FinalRow<'_>),
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        (**self).adapt_each(batches, seed, below, readouts, each)
    }

    fn adapt_stream(
        &mut self,
        stream: InstructionStream<'_, impl Iterator<Item = Instruction>>,
        seed: Seed,
        below: &mut impl Execute,
        readouts: &mut BatchReadouts,
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        (**self).adapt_stream(stream, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        (**self).end_shots(shots);
    }
}
