// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor's role at composition time, [`ExecutorSetup`].

use crate::composition::admission::by_part::ByPart;
use crate::{InstructionSet, Needs, Offer};

/// An executor at composition time: what `>>` reads from it and hands back
/// to it, once, before any run. At run time, it is an
/// [`Execute`](crate::Execute).
///
/// Composition makes two kinds of calls. Declarations are queries whose
/// answers are fixed: [`name`](Self::name), [`accepts`](Self::accepts) and
/// [`offers`](Self::offers). It gets no hand-back: it sends nothing, so it
/// gets no opcode map. Composition only decides, from what the parts
/// declare; what prepares a part to run belongs to each run, which starts
/// with [`ready`](crate::Execute::ready), given the needs the admission
/// checked.
///
/// Its instruction set and its offer must not change once it is composed:
/// the fit and the admission are made once, from them.
pub trait ExecutorSetup {
    /// The part's name in messages, such as an admission fault or a
    /// run's failure; by default its type's, `std::any::type_name`.
    ///
    /// A part overrides it when its type does not say what it is: a part
    /// built at run time, such as a runtime made from an instruction set's
    /// description, has one Rust type for every configuration; and the
    /// default's text is a module path, whose format the standard library
    /// does not guarantee. Two parts of one name are told apart by their
    /// position ([`PartId`](crate::PartId)). Parts composed by `>>`
    /// answer the name of their top part, the one that receives their
    /// calls. The protocol's messages use the name read by the `>>` that
    /// makes a runner.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it runs, which the part above must fit. Each
    /// checked batch it receives is checked against this set as read once,
    /// by the `>>` that makes a runner.
    fn accepts(&self) -> &InstructionSet;

    /// What it can ever serve, such as its most qubits, which the driver's
    /// needs, as adapted down to it, must fit. The `>>` that makes a runner
    /// reads it once, and the admission and every check of a run use that
    /// answer. It answers the same each time: the default
    /// [`execute_stream`](crate::Execute::execute_stream) and
    /// [`adapt_stream`](crate::Adapt::adapt_stream) read it again, to cut
    /// a stream into pieces of at most its most instructions. Its most
    /// shots and whether it is seedable bound every run of the chain: no
    /// adapter above can raise them.
    fn offers(&self) -> Offer;

    // Protocol hooks: sealed and hidden. Their parameter type, `ByPart`,
    // the admission's record (`src/composition/admission/by_part.rs`),
    // cannot be named outside this crate, so no other crate can call or
    // override them. They are in the trait because Rust cannot specialise
    // a default method: the protocol's composed parts (`Composed`, in
    // `src/composition/composed/`) override them to list each inner part,
    // so that a refusal names the inner part that was short, and to keep
    // the needs and the offer that cross each inner boundary.

    /// The needs that reach this executor from `above`, listing those of
    /// each adapter it holds; only the crate overrides it, to list them, and
    /// to keep the needs that cross each boundary it holds. The `>>` that
    /// makes a runner calls it once.
    #[doc(hidden)]
    fn needs_by_part(&mut self, above: Needs, by_part: &mut ByPart) -> Needs {
        let _ = by_part;
        above
    }

    /// Its offer, listed as this part's; only the crate overrides it, to
    /// list each adapter's too, and to keep the offer that crosses each
    /// boundary it holds. The `>>` that makes a runner calls it once.
    #[doc(hidden)]
    fn offers_by_part(&mut self, by_part: &mut ByPart) -> Offer {
        let offer = self.offers();
        by_part.note_executor_offer(ExecutorSetup::name(self), &offer);
        offer
    }
}

impl<E: ExecutorSetup + ?Sized> ExecutorSetup for &mut E {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn accepts(&self) -> &InstructionSet {
        (**self).accepts()
    }

    fn offers(&self) -> Offer {
        (**self).offers()
    }

    fn needs_by_part(&mut self, above: Needs, by_part: &mut ByPart) -> Needs {
        (**self).needs_by_part(above, by_part)
    }

    fn offers_by_part(&mut self, by_part: &mut ByPart) -> Offer {
        (**self).offers_by_part(by_part)
    }
}
