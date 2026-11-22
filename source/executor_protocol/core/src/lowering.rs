// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How one instruction set runs on another: a [`Lowering`] rewrites each
//! instruction of its `From` set as instructions of its `To` set, and reads
//! the instruction's readouts back from theirs; [`Lowered`] is the adapter
//! that runs any lowering in a chain.
//!
//! A sender whose set is not the receiver's cannot be joined to it: `on`
//! and `through` require the sender's set type to be the receiver's. A
//! lowering is what makes them compatible, placed explicitly between them,
//! as `driver.through(Lowered::new(lowering)).on(executor)`, so its writer
//! states only what is particular to it: each instruction's rewriting, the
//! readouts read back, and what it requires below. `Lowered` keeps the
//! protocol's rules for it (each batch sent checked if it arrived checked,
//! its readout slots stated, its shots' conditions passed up) and reuses
//! its memory from one `execute` to the next.

mod lowered;

pub use lowered::Lowered;

use crate::{Binding, Capabilities, InstructionSet, NotReady};

/// Instructions of the set [`From`](Self::From) run as instructions of the
/// set [`To`](Self::To), one instruction at a time, with their readouts
/// read back: what [`Lowered`] runs as an adapter.
///
/// Such as a set with a three-qubit gate lowered to one without it, the
/// gate rewritten as a sequence of the other's gates, or a logical qubit
/// lowered to the three physical qubits of a repetition code, a logical
/// readout read as the majority of its three.
pub trait Lowering {
    /// The set of the instructions it lowers.
    type From: InstructionSet;

    /// The set of the instructions it lowers them to.
    type To: InstructionSet;

    /// The instructions it lowers: what the part above must send, as
    /// [`TranslatingAdapter::accepts`](crate::TranslatingAdapter::accepts).
    fn accepts(&self) -> &Self::From;

    /// The instructions it lowers them to: what the part below must
    /// accept, as [`TranslatingAdapter::sends`](crate::TranslatingAdapter::sends).
    fn sends(&self) -> &Self::To;

    /// Its fixed limits on what arrives from above, as
    /// [`TranslatingAdapter::provides`](crate::TranslatingAdapter::provides):
    /// by default its `From` set's
    /// [`unconstrained`](InstructionSet::unconstrained), no limits.
    fn provides(&self) -> &Capabilities<Self::From> {
        Self::From::unconstrained()
    }

    /// What it requires below, given what arrives from above, `above`, as
    /// [`TranslatingAdapter::requires`](crate::TranslatingAdapter::requires):
    /// the same resources and features, but for the instructions a batch
    /// holds, and the set's own capabilities its rewriting needs, such as
    /// three physical qubits for each logical one.
    ///
    /// [`Lowered`] sends one lowered batch for each batch that arrives, so a
    /// lowering that rewrites an instruction as several requires that many
    /// more instructions a batch below, such as 13 a batch for each one
    /// above when its longest rewriting is 13 instructions; required
    /// unchanged, a chain whose receiver takes fewer is joined, and its
    /// lowered batches are refused at run time.
    fn requires(&self, above: &Capabilities<Self::From>) -> Capabilities<Self::To>;

    /// As [`TranslatingAdapter::ready`](crate::TranslatingAdapter::ready),
    /// for `above`, what arrives from above: by default ready.
    fn ready(
        &mut self,
        above: &Capabilities<Self::From>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        let _ = above;
        async { Ok(()) }
    }

    /// Keeps how its instructions are encoded in the receiver's set: the
    /// binding of the fit of [`sends`](Self::sends) into what the part
    /// below accepts, as
    /// [`TranslatingAdapter::bind`](crate::TranslatingAdapter::bind),
    /// given before any `lower`.
    fn bind(&mut self, below: Binding<Self::To>);

    /// Appends to `into` the instructions that run `instruction` below, in
    /// order, encoded as [`bind`](Self::bind) gave.
    fn lower(
        &self,
        instruction: &<Self::From as InstructionSet>::Instruction,
        into: &mut Vec<<Self::To as InstructionSet>::Instruction>,
    );

    /// Writes `instruction`'s readouts, `into`, one value per readout slot
    /// of it in its set, from `lowered`, those of the instructions
    /// [`lower`](Self::lower) gave it, in their order. Called for each shot
    /// and each instruction with a readout slot.
    ///
    /// Such as a logical readout read as the majority of three physical
    /// ones; a lowering whose readouts are its lowering's, one for one and
    /// of the same type, copies them (`into.copy_from_slice(lowered)`).
    fn read(
        &self,
        instruction: &<Self::From as InstructionSet>::Instruction,
        lowered: &[<Self::To as InstructionSet>::Readout],
        into: &mut [<Self::From as InstructionSet>::Readout],
    );

    /// A shot's conditions above, from `lowered`, its conditions below.
    /// Called for each shot of each batch.
    fn read_conditions(
        &self,
        lowered: <Self::To as InstructionSet>::ShotConditions,
    ) -> <Self::From as InstructionSet>::ShotConditions;
}
