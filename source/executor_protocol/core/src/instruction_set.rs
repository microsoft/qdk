// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the protocol asks of an instruction set: [`InstructionSet`], and
//! what a sender binds when its set fits the receiver's ([`Binding`]).

use std::error::Error;
use std::fmt;

use crate::{BoundaryCompatible, Capabilities};

/// An instruction set, as the protocol sees it: the instructions a batch
/// holds, when a sender's set fits a receiver's, the set's own
/// capabilities, and how one instruction is checked.
///
/// Each part names the set it sends or accepts as an associated type, so
/// two parts join only where the sender's set *type* is the receiver's,
/// checked at compile time; their set *values* are checked at the join,
/// by [`fits`](BoundaryCompatible::fits), whose [`Binding`] the sender is
/// given. Where two sets do not fit, a [`Lowering`](crate::Lowering),
/// joined as a [`Lowered`](crate::Lowered) adapter, writes one set's
/// instructions in the other's.
///
/// A set states two kinds of its own capabilities: what parts state and
/// compare at composition ([`Capabilities`](Self::Capabilities), such as
/// the qubits a shot holds), and what each batch's instructions are
/// checked against on a run ([`Requirement`](Self::Requirement)), prepared
/// from the first once per run so that the per-instruction check is cheap.
///
/// A fit's mismatch and an instruction's fault are the set's own typed
/// errors; the protocol's shared errors hold them boxed, so a caller reads
/// them with `downcast_ref`.
pub trait InstructionSet:
    BoundaryCompatible<Mismatch: Error + Send + Sync + 'static> + Clone + 'static
{
    /// One instruction of the set, as a batch holds it.
    type Instruction: Clone + Send + Sync + 'static;

    /// The set's own capabilities, beside the [`Resources`](crate::Resources)
    /// and [`Features`](crate::Features) every chain states: required
    /// above, provided below, compared by
    /// [`fits`](BoundaryCompatible::fits) with the same rule as theirs.
    /// Equal values are what a part passes below unchanged, which a
    /// composition fault traces to the part that first required them.
    type Capabilities: BoundaryCompatible<Fit = (), Mismatch: Error + Send + Sync + 'static>
        + Clone
        + PartialEq
        + fmt::Debug
        + Send
        + Sync
        + 'static;

    /// What each instruction of a checked batch is checked against on a
    /// run, prepared once per run by [`requirement`](Self::requirement).
    /// A set whose check reads its capabilities as they are may use
    /// [`Capabilities`](Self::Capabilities) itself.
    type Requirement: Send + Sync;

    /// Why [`check`](Self::check) refuses an instruction.
    type Fault: Error + Send + Sync + 'static;

    /// No limit and no need: the core's unconstrained resources and
    /// features, with the set's capabilities that state nothing. What a
    /// part's default `provides` lends, so a set keeps it in a `static`.
    fn unconstrained() -> &'static Capabilities<Self>;

    /// The readout slots `instruction` writes: what a sender sums into a
    /// batch's [`readout_slots`](crate::Batch::readout_slots).
    fn readout_slots(&self, instruction: &Self::Instruction) -> u32;

    /// What a checked batch's instructions are checked against, given
    /// `required`, the set's capabilities arriving at the receiver: built
    /// once per run, at each boundary whose receiver accepts this set.
    fn requirement(&self, required: &Self::Capabilities) -> Self::Requirement;

    /// Checks `instruction` against this set, the receiver's, and against
    /// `required`: its readout slots, or why it is refused.
    ///
    /// # Errors
    ///
    /// The set's [`Fault`](Self::Fault) when the receiver does not accept
    /// `instruction`.
    fn check(
        &self,
        instruction: &Self::Instruction,
        required: &Self::Requirement,
    ) -> Result<u32, Self::Fault>;
}

/// What a sender is given when its set fits the receiver's: the fit's
/// result, such as the receiver's encoding of each sent instruction, or
/// `()` where both share it.
pub type Binding<S> = <S as BoundaryCompatible>::Fit;
