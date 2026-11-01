// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an instruction type tells every boundary where its calls meet an
//! executor ([`Instruction`]): whether its calls name their instruction by
//! an opcode, which must then be in the executor's numbering, or by their
//! type ([`TypedInstruction`]), which no numbering concerns.

use std::sync::Arc;

#[cfg(doc)]
use crate::{BuildError, InstructionCall};
use crate::{Close, Executor, InstructionBatch, OpcodeMap, Renumbering};

/// An instruction type a driver or an adapter sends, and what each
/// boundary where its calls meet an executor needs: whether its calls name
/// their instruction by an opcode, and so must be in the executor's
/// numbering. [`compose`](fn@crate::compose) uses it at each boundary it
/// checks: between a driver and the executor it is composed with, and
/// between an adapter and the executor downstream of it.
///
/// The crate implements it for [`InstructionCall`] and for every
/// [`TypedInstruction`]: a typed instruction, which names its instruction
/// by itself (an enum's variant), implements that instead, in one line.
///
/// It is sealed, so no other type implements it. Another crate cannot
/// build the [`Renumbering`] that [`Instruction::renumbering`] returns, so
/// its own type whose calls carry an opcode could only return `None`, and
/// its calls would reach an executor that numbers them otherwise as they
/// are: the wrong instructions would run, with no error. Such a type is
/// not supported; its calls are [`InstructionCall`]s.
pub trait Instruction: sealed::Sealed + Sized {
    /// Whether a call names its instruction by an opcode, a number that
    /// means something only in a set: then whoever sends the calls states
    /// its set, and a chain that cannot check it is refused (a driver's
    /// [`UnmetNeed`](crate::UnmetNeed) of
    /// [`Instructions`](crate::Need::Instructions), an adapter's
    /// [`NoInstructionSetDownstream`] or
    /// [`UnstatedSends`](BuildError::UnstatedSends)).
    /// A typed call names it by its type, which the compiler checks, so
    /// a typed boundary needs no set.
    ///
    /// [`NoInstructionSetDownstream`]: BuildError::NoInstructionSetDownstream
    const BY_OPCODE: bool;

    /// What a driver or an adapter of this type calls: the executor `E`
    /// downstream of it, as the calls of this type reach it.
    type Downstream<E>: Executor<InstructionBatch<Self>, Reply = E::Reply>
        + Close<Report = E::Report>
    where
        E: Executor<InstructionBatch<Self>> + Close;

    /// How calls reach an executor whose set `map` matches the sender's set
    /// in: `None` when they reach it as they are.
    fn renumbering(map: OpcodeMap) -> Option<Renumbering>;

    /// `inner` as the calls of this type reach it: renumbered on their way
    /// by `renumbering` when there is one ([`Instruction::renumbering`]).
    fn downstream<E: Executor<InstructionBatch<Self>> + Close>(
        inner: E,
        renumbering: Option<&Arc<Renumbering>>,
    ) -> Self::Downstream<E>;
}

/// An instruction that names itself by its type (an enum's variant, a
/// struct of its own) rather than by an opcode, so that no numbering
/// concerns it: a driver of it calls the executor it is composed with
/// itself.
///
/// A type that carries an opcode must not implement it, since its calls
/// would reach the chain in the driver's numbering.
///
/// ```
/// #[derive(Clone, Copy)]
/// enum Op {
///     X(u32),
///     Measure(u32),
/// }
///
/// impl executor_protocol::TypedInstruction for Op {}
/// ```
pub trait TypedInstruction {}

/// Seals [`Instruction`]: only the crate can build a [`Renumbering`], so
/// only it says which instruction types a boundary connects.
///
/// No test pins the seal: a type of another crate implementing
/// [`Instruction`] is a compile error, which only a `compile_fail` doc
/// example could check. The crate has some (on `ShotOutcome` and
/// `Outcome`), but with `doctest = false` they run only under
/// `cargo test --doc`, which CI's plain `cargo test` (`build.py`) does not
/// run; the crate has no trybuild.
///
/// It also holds what the crate asks of an instruction and keeps off the
/// public API: no other crate can name the trait to call it.
mod sealed {
    use crate::{InstructionMismatch, InstructionSet, Operand};

    /// Implemented for the types [`Instruction`](super::Instruction) is.
    pub trait Sealed {
        /// Checks that this call is an instruction of `set`, as
        /// [`InstructionSet::check`] does. A typed call names its
        /// instruction by its type, which the compiler checks, so no set
        /// concerns it: it is in any.
        fn check_in(&self, set: &InstructionSet) -> Result<(), InstructionMismatch>;

        /// The tags of this call's readouts, in its operands' order. A
        /// typed call holds its tags in fields of its own, which the crate
        /// cannot read, so it lists none.
        fn readout_tags(&self) -> impl Iterator<Item = u32>;
    }

    impl Sealed for crate::InstructionCall {
        fn check_in(&self, set: &InstructionSet) -> Result<(), InstructionMismatch> {
            set.check(self)
        }

        fn readout_tags(&self) -> impl Iterator<Item = u32> {
            self.operands.iter().filter_map(|operand| match operand {
                Operand::Tag(tag) => Some(*tag),
                _ => None,
            })
        }
    }

    impl<T: super::TypedInstruction> Sealed for T {
        fn check_in(&self, _: &InstructionSet) -> Result<(), InstructionMismatch> {
            Ok(())
        }

        fn readout_tags(&self) -> impl Iterator<Item = u32> {
            std::iter::empty()
        }
    }
}

/// No numbering concerns a typed instruction: a driver, or an adapter,
/// calls the executor downstream of it itself.
impl<T: TypedInstruction> Instruction for T {
    type Downstream<E>
        = E
    where
        E: Executor<InstructionBatch<T>> + Close;

    const BY_OPCODE: bool = false;

    fn renumbering(_: OpcodeMap) -> Option<Renumbering> {
        None
    }

    fn downstream<E: Executor<InstructionBatch<T>> + Close>(
        inner: E,
        _: Option<&Arc<Renumbering>>,
    ) -> E {
        inner
    }
}
