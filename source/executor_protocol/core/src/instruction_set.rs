// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the protocol asks of an instruction set: [`InstructionSet`]; what
//! a sender binds when its set fits the receiver's ([`Binding`]); and
//! whether two parts state one need of the set alike ([`SameNeed`]).

use std::error::Error;
use std::fmt;
use std::ops::BitOrAssign;

use crate::{BoundaryCompatible, ReadoutSlots};

/// An instruction set, as the protocol sees it: the instructions a batch
/// holds, when a sender's set fits a receiver's, the set's own
/// capabilities, and how one instruction is checked.
///
/// Each part names the set it sends or accepts as an associated type, so
/// two parts join only where the sender's set *type* is the receiver's,
/// checked at compile time; their set *values* are checked at the join,
/// by [`fits`](BoundaryCompatible::fits), whose [`Binding`] the sender is
/// given. Where two sets do not fit, a
/// [`TranslatingAdapter`](crate::TranslatingAdapter) writes one set's
/// instructions in the other's.
///
/// A set states two kinds of its own capabilities: what parts state and
/// compare at composition ([`SetCapabilities`](Self::SetCapabilities),
/// such as the qubits a shot holds), and what each batch's instructions
/// are checked against on a run ([`Requirement`](Self::Requirement)),
/// prepared from the first once per run so that the per-instruction check
/// is cheap.
///
/// A set is a declaration that the parts and the runner share, so it is
/// `Send` and `Sync`, as its instructions, readouts and capabilities are:
/// a run's future then needs only its parts to be `Send` to be `Send`.
///
/// A fit's mismatch and an instruction's fault are the set's own typed
/// errors; the protocol's shared errors hold them boxed, so a caller reads
/// them with `downcast_ref`.
pub trait InstructionSet:
    BoundaryCompatible<Mismatch: Error + Send + Sync + 'static> + Clone + Send + Sync + 'static
{
    /// One instruction of the set, as a batch holds it.
    type Instruction: Clone + Send + Sync + 'static;

    /// The set's own capabilities, beside the [`Resources`](crate::Resources)
    /// and [`Features`](crate::Features) every chain states: required
    /// above, provided below, compared by
    /// [`fits`](BoundaryCompatible::fits) with the same rule as theirs.
    /// Equal values are what a part passes below unchanged, which a
    /// composition fault traces to the part that first required them;
    /// each piece of a mismatch is traced on its own, by the
    /// [`SameNeed`] that [`split_mismatch`](Self::split_mismatch) gives it.
    ///
    /// A value reads per side: as a requirement, what the part above needs
    /// (an unstated field, no need); as a provision, the part's limits (an
    /// unstated field, no limit). It fits when every need is within the
    /// limits, as the crate's `docs/capabilities.md` says.
    type SetCapabilities: BoundaryCompatible<Fit = (), Mismatch: Error + Send + Sync + 'static>
        + Clone
        + PartialEq
        + fmt::Debug
        + Send
        + Sync
        + 'static;

    /// What each instruction of a checked batch is checked against on a
    /// run, prepared once per run by [`requirement`](Self::requirement).
    /// A set whose check reads its capabilities as they are may use
    /// [`SetCapabilities`](Self::SetCapabilities) itself.
    type Requirement: Send + Sync;

    /// Why [`check`](Self::check) refuses an instruction.
    type Fault: Error + Send + Sync + 'static;

    /// The value of one readout slot, such as a measured bit, or why the
    /// readout has none: what an executor writes in each slot of a batch's
    /// [`BatchReadouts`](crate::BatchReadouts).
    type Readout: Copy + PartialEq + fmt::Debug + Send + Sync;

    /// What the set's parts note of a shot beside its readouts, such as a
    /// shot they found suspect: `Default` is a shot with nothing noted, and
    /// `|=` joins what two notes hold.
    type ShotConditions: Copy + PartialEq + Default + BitOrAssign + fmt::Debug + Send + Sync;

    /// The value of a readout slot no part wrote, so that it reads as no
    /// value, never as one made up for it.
    const UNWRITTEN_READOUT: Self::Readout;

    /// The set's capabilities that need nothing: what a requirement that
    /// states nothing of the set holds, such as no qubit, and what a driver
    /// that states nothing requires of the set. It fits every provision,
    /// stated or not, as a need of nothing is within any limits: a chain of
    /// parts that state nothing composes and runs on any part below. A
    /// part that limits nothing states no provision at all (`None`), never
    /// this value, which as a provision may limit, such as to no qubit.
    fn needs_nothing() -> Self::SetCapabilities;

    /// `mismatch`, a fit's of the set's capabilities, split into one piece
    /// per need, each with how two parts' capabilities state the same need
    /// ([`SameNeed`]): so that a composition fault names, for each need,
    /// the part that stated it, as it does for each resource and feature.
    /// A need a part passes below unchanged is the first stater's: where a
    /// driver needs 10 qubits and an adapter below it adds a need of
    /// reset, an executor of 8 qubits and no reset falls short of the
    /// driver's qubits and of the adapter's reset, two faults.
    ///
    /// By default, the whole mismatch is one piece, a need two parts state
    /// alike when their capabilities are equal: a set whose capabilities
    /// hold several independent needs overrides it, or its faults name the
    /// last part to change any of them. An override puts each need of
    /// `mismatch` in exactly one piece, and gives at least one piece: the
    /// protocol, which does not check it, relies on it to name a part for
    /// each need, and to refuse every mismatch with a fault.
    fn split_mismatch(
        mismatch: <Self::SetCapabilities as BoundaryCompatible>::Mismatch,
    ) -> Vec<(
        <Self::SetCapabilities as BoundaryCompatible>::Mismatch,
        SameNeed<Self>,
    )> {
        vec![(mismatch, Box::new(|a, b| a == b))]
    }

    /// The readout slots `instruction` writes: what a sender sums into a
    /// batch's [`readout_slots`](crate::Batch::readout_slots).
    fn readout_slots(&self, instruction: &Self::Instruction) -> ReadoutSlots;

    /// What a checked batch's instructions are checked against, given
    /// `required`, the set's capabilities arriving at the receiver: built
    /// once per run, at each boundary whose receiver accepts this set, as
    /// the run starts.
    fn requirement(&self, required: &Self::SetCapabilities) -> Self::Requirement;

    /// Checks `instruction` against this set, the receiver's, and against
    /// `required`: its readout slots, or why it is refused. The slots it
    /// answers are [`readout_slots`](Self::readout_slots)`(instruction)`,
    /// since a boundary checks a batch's stated slots against their sum.
    ///
    /// # Errors
    ///
    /// The set's [`Fault`](Self::Fault) when the receiver does not accept
    /// `instruction`, or `required` does not allow it.
    fn check(
        &self,
        instruction: &Self::Instruction,
        required: &Self::Requirement,
    ) -> Result<ReadoutSlots, Self::Fault>;
}

/// What a sender is given when its set fits the receiver's: the fit's
/// result, such as the receiver's encoding of each sent instruction, or
/// `()` where both share it.
pub type Binding<S> = <S as BoundaryCompatible>::Fit;

/// Whether two parts' capabilities of the set `S` state the same need, for
/// one piece of a mismatch that
/// [`split_mismatch`](InstructionSet::split_mismatch) gives: such as the
/// same count of qubits, whatever else each needs.
pub type SameNeed<S> = Box<
    dyn Fn(
        &<S as InstructionSet>::SetCapabilities,
        &<S as InstructionSet>::SetCapabilities,
    ) -> bool,
>;
