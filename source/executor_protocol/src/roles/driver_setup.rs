// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The driver's role at composition time, [`DriverSetup`].

use crate::composition::admission::by_part::ByPart;
use crate::{InstructionSet, Needs, Offer, OpcodeMap};

/// A driver at composition time: what `>>` reads from it and hands back to
/// it, once, before any run. At run time, it is a [`Drive`](crate::Drive).
///
/// Composition makes two kinds of calls. Declarations are queries whose
/// answers are fixed: [`name`](Self::name), [`sends`](Self::sends) and
/// [`needs`](Self::needs). A hand-back gives the driver what composition
/// made of them: [`bind`](Self::bind) gets the fit's opcode map from the
/// `>>` that joins it to the part below. Composition only decides, from
/// what the parts declare; what prepares a part to run belongs to each
/// run, which starts with [`ready`](crate::Drive::ready).
///
/// Its instruction set and its needs must not change once it is composed:
/// the fit, `bind`'s map and the admission are made once, from them, and
/// each part below gets ready, on every run, for the needs read then.
pub trait DriverSetup {
    /// As [`ExecutorSetup::name`](crate::ExecutorSetup::name).
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it sends, which must fit the part below.
    fn sends(&self) -> &InstructionSet;

    /// What its shots need of the chain below, such as their qubits, which
    /// the offer below it must meet, and, adapted by each adapter, the
    /// offer of each part further below. The `>>` that makes a runner reads
    /// it once.
    fn needs(&self) -> Needs;

    /// Receives, once, the translation of its opcodes into the part
    /// below's. An opcode is an instruction's place in its set, and the
    /// part below may number the same instructions differently, so every
    /// instruction it sends must carry the receiver's opcode,
    /// `below.map(opcode)`: an untranslated one names whatever instruction
    /// the receiver has at that place. `>>` calls it once, after the fit
    /// and before any run; a driver rewrites its prebuilt instructions
    /// then, or keeps the map to translate each one it sends.
    fn bind(&mut self, below: OpcodeMap);

    // Protocol hooks: sealed and hidden. Their parameter type, `ByPart`,
    // the admission's record (`src/composition/admission/by_part.rs`),
    // cannot be named outside this crate, so no other crate can call or
    // override them. They are in the trait because Rust cannot specialise
    // a default method: the protocol's composed parts (`Composed`, in
    // `src/composition/composed/`) override them to list each inner part,
    // so that a refusal names the inner part that was short, and to keep
    // the needs and the offer that cross each inner boundary.

    /// Its needs, listed as this part's; only the crate overrides it, to
    /// list each part it holds, and to keep the needs that cross each
    /// boundary it holds. The `>>` that makes a runner calls it once.
    #[doc(hidden)]
    fn needs_by_part(&mut self, by_part: &mut ByPart) -> Needs {
        let needs = self.needs();
        by_part.note_needs(DriverSetup::name(self), &needs);
        needs
    }

    /// The offer that reaches this driver from `below`, listing those of
    /// each adapter it holds; only the crate overrides it, to list them, and
    /// to keep the offer that crosses each boundary it holds. The `>>` that
    /// makes a runner calls it once.
    #[doc(hidden)]
    fn offers_by_part(&mut self, below: Offer, by_part: &mut ByPart) -> Offer {
        let _ = by_part;
        below
    }
}
