// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The adapter's role at composition time, [`AdapterSetup`].

use crate::composition::admission::by_part::ByPart;
use crate::{InstructionSet, Needs, Offer, OpcodeMap};

/// An adapter at composition time: what `>>` reads from it and hands back
/// to it, once, before any run. At run time, it is an
/// [`Adapt`](crate::Adapt).
///
/// Composition makes two kinds of calls. Declarations are queries whose
/// answers are fixed: [`name`](Self::name), [`accepts`](Self::accepts),
/// [`sends`](Self::sends), [`needs_below`](Self::needs_below) and
/// [`offers_above`](Self::offers_above). A hand-back gives the adapter
/// what composition made of them: [`bind`](Self::bind) gets the fit's
/// opcode map from the `>>` that joins it to the part below. Composition
/// only decides, from what the parts declare; what prepares a part to run
/// belongs to each run, which starts with [`ready`](crate::Adapt::ready).
///
/// Its instruction sets, the needs it passes below and its offer above
/// must not change once it is composed: the fit, `bind`'s map and the
/// admission are made once, from them, and each part gets ready, on every
/// run, for the needs read then.
pub trait AdapterSetup {
    /// As [`ExecutorSetup::name`](crate::ExecutorSetup::name).
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it receives, which the part above must fit. Each
    /// checked batch it receives is checked against this set as read once,
    /// by the `>>` that makes a runner.
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
    /// The `>>` that makes a runner reads it once.
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
    /// up is not named. The `>>` that makes a runner asks it for both
    /// answers.
    ///
    /// The `>>` that makes a runner reads it once, and the admission and
    /// every check of a run use that answer. It answers the same each
    /// time: the default [`adapt_stream`](crate::Adapt::adapt_stream)
    /// reads it again, to cut a stream into pieces of at most its most
    /// instructions, or of 1 when that most is 0, which its `adapt` may
    /// refuse.
    fn offers_above(&self, below: Offer) -> Offer {
        below
    }

    /// Receives, once, the translation of the opcodes it sends into the
    /// part below's. An opcode is an instruction's place in its set, and
    /// the part below may number the same instructions differently, so
    /// every instruction it sends must carry the receiver's opcode,
    /// `below.map(opcode)`: an untranslated one names whatever instruction
    /// the receiver has at that place. `>>` calls it once, after the fit
    /// and before any run; an adapter keeps the map to translate each
    /// instruction it forwards or adds.
    fn bind(&mut self, below: OpcodeMap);

    // Protocol hooks: sealed and hidden. Their parameter type, `ByPart`,
    // the admission's record (`src/composition/admission/by_part.rs`),
    // cannot be named outside this crate, so no other crate can call or
    // override them. They are in the trait because Rust cannot specialise
    // a default method: the protocol's composed parts (`Composed`, in
    // `src/composition/composed/`) override them to list each inner part,
    // so that a refusal names the inner part that was short, and to keep
    // the needs and the offer that cross each inner boundary.

    /// Its needs below, listed as this part's; only the crate overrides it,
    /// to list each part it holds, and to keep the needs that cross each
    /// boundary it holds. The `>>` that makes a runner calls it once.
    #[doc(hidden)]
    fn needs_by_part(&mut self, above: Needs, by_part: &mut ByPart) -> Needs {
        let below = self.needs_below(above);
        by_part.note_needs(AdapterSetup::name(self), &below);
        below
    }

    /// Its offer above, listed as this part's; only the crate overrides it,
    /// to list each part it holds, and to keep the offer that crosses each
    /// boundary it holds. The `>>` that makes a runner calls it once.
    #[doc(hidden)]
    fn offers_by_part(&mut self, below: Offer, by_part: &mut ByPart) -> Offer {
        let above = self.offers_above(below.clone());
        by_part.note_adapter_offer(AdapterSetup::name(self), &above, &below, |exactly| {
            self.offers_above(exactly)
        });
        above
    }
}

impl<A: AdapterSetup + ?Sized> AdapterSetup for &mut A {
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

    fn needs_by_part(&mut self, above: Needs, by_part: &mut ByPart) -> Needs {
        (**self).needs_by_part(above, by_part)
    }

    fn offers_by_part(&mut self, below: Offer, by_part: &mut ByPart) -> Offer {
        (**self).offers_by_part(below, by_part)
    }
}
