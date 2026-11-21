// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The admission of a chain by the `on` that makes its runner: at every
//! boundary, what the shots need against what the part below offers,
//! wherever the brackets go.

use std::any::type_name;
use std::cell::Cell;
use std::rc::Rc;

use crate::prelude::*;
use crate::tests::composition::{faults, plain, records};
use crate::tests::fake_parts::{
    Bits, CoinCopy, Log, Offering, Pool, Profiler, Program, Renamed, ReplaysItself, Spread,
    in_each_bracketing,
};
use crate::{
    AdmissionFaults, BlockType, CompositionFaults, Needs, Offer, PartId, Runner, ShortOffer,
    ShotSet, UnmetNeed,
};

/// The qubits `needed` where `offered` allow fewer.
fn unmet_qubits(needed: u32, offered: u32) -> UnmetNeed {
    UnmetNeed::Blocks {
        block_type: BlockType::qubit(),
        needed,
        offered,
    }
}

/// The part of type `P` at `position`, named by its type.
fn part<P>(position: usize) -> PartId {
    PartId::new(position, type_name::<P>())
}

/// The offer of the part of type `P` at `position`, `offered`, falling
/// short of `needed` by `unmet`.
fn short_offer<P>(
    position: usize,
    needed: Needs,
    offered: Offer,
    unmet: Vec<UnmetNeed>,
) -> ShortOffer {
    ShortOffer {
        part: part::<P>(position),
        needed,
        offered,
        unmet,
    }
}

/// The parts `short_offers` name.
fn parts(short_offers: &[ShortOffer]) -> Vec<PartId> {
    short_offers
        .iter()
        .map(|short| short.part.clone())
        .collect()
}

/// The admission faults of the chain `composed` gave, which must not be
/// admitted.
fn admission<D: Driver, E: Executor>(
    composed: Result<Runner<D, E>, CompositionFaults>,
) -> AdmissionFaults {
    faults(composed)
        .admission()
        .cloned()
        .expect("an unmet need")
}

/// Checks, in each bracketing of the parts `$setup` binds, that the
/// chain's admission names `$expected`, as the row `$row`.
macro_rules! short_offers_in_each_bracketing {
    ($row:expr, [$($part:ident),+] from $setup:tt, $expected:expr $(,)?) => {
        let expected: Vec<ShortOffer> = $expected;
        in_each_bracketing!(
            [$($part),+] from $setup,
            |chain, brackets| assert_eq!(
                admission(chain).short_offers,
                expected,
                "{}, {brackets}",
                $row
            ),
        );
    };
}

/// A program needing exactly what the executor offers composes, and its
/// records are those on a larger executor.
#[test]
fn a_program_within_the_offer_runs_unchanged() {
    let log = Log::default();
    let shots = ShotSet::ids(0..32);
    let exact = CoinCopy::new().on(Bits::new(&log).offering(Offer::unlimited().max_qubits(2)));
    assert_eq!(records(exact, &shots), plain(&shots));
}

/// A program on 7 qubits, through an encoding of 7 qubits per qubit, over
/// 30 qubits: the `on` fails, naming the bits' offer of 30 for 49 qubits,
/// and each part's needs and offer.
#[test]
fn an_unmet_need_is_a_fault_naming_the_short_part_and_each_parts_needs_and_offer() {
    let log = Log::default();
    let offer = Offer::unlimited().max_qubits(30);
    let composed = Program::new(&[("mz", 0)])
        .needing(Needs::qubits(7))
        .through(Spread::new(7, &log))
        .on(Bits::new(&log).offering(offer.clone()));
    let faults = faults(composed);
    assert_eq!(faults.fit(), []);
    let admission = faults.admission().expect("an unmet need");
    assert_eq!(
        admission.short_offers,
        [short_offer::<Bits>(
            2,
            Needs::qubits(49),
            offer.clone(),
            vec![unmet_qubits(49, 30)]
        )]
    );
    assert_eq!(
        admission.needs_by_part,
        [
            (part::<Program>(0), Needs::qubits(7)),
            (part::<Spread>(1), Needs::qubits(49)),
        ]
    );
    assert_eq!(
        admission.offers_by_part,
        [
            (part::<Bits>(2), offer),
            (part::<Spread>(1), Offer::unlimited().max_qubits(4)),
        ]
    );
}

/// The same encodings as those whose needs come down, in
/// `composition.rs`, over 11 qubits: 3 come up through the lower, 1
/// through the upper, each offer listed bottom to top.
#[test]
fn offers_go_up_through_adapters() {
    let log = Log::default();
    let eleven = Offer::unlimited().max_qubits(11);
    let composed = CoinCopy::new()
        .through(Spread::new(2, &log))
        .through(Spread::new(3, &log))
        .on(Bits::new(&log).offering(eleven.clone()));
    assert_eq!(
        admission(composed).offers_by_part,
        [
            (part::<Bits>(3), eleven),
            (part::<Spread>(2), Offer::unlimited().max_qubits(3)),
            (part::<Spread>(1), Offer::unlimited().max_qubits(1)),
        ]
    );
}

/// A part that only passes a shortage up, given exactly what it needs
/// below, would offer what is needed above: the fault names the part
/// below, where the shortage is, alone, though the passing part's offer
/// is short too, in each bracketing, where composed parts read the offers
/// and needs another way.
#[test]
fn a_part_that_only_passes_a_shortage_up_is_not_named() {
    let counted = Rc::new(Cell::new(0));
    let program = |needs: Needs| Program::new(&[("mz", 0)]).needing(needs);
    let over =
        |qubits: u32, log: &Log| Bits::new(log).offering(Offer::unlimited().max_qubits(qubits));
    // A program on 7 qubits, through an encoding of 7 qubits per qubit,
    // over 30 qubits: the encoding's 4 for 7 only passes the bits' 30 for
    // 49 up; given the 49 it needs below, it would offer 7.
    short_offers_in_each_bracketing!(
        "an encoding",
        [d, a, e] from {
            let log = Log::default();
            let d = program(Needs::qubits(7));
            let a = Spread::new(7, &log);
            let e = over(30, &log);
        },
        vec![short_offer::<Bits>(
            2,
            Needs::qubits(49),
            Offer::unlimited().max_qubits(30),
            vec![unmet_qubits(49, 30)],
        )],
    );
    // A program on 9 qubits, through a profiler, which adapts neither
    // needs nor offers, over bits offering 8.
    short_offers_in_each_bracketing!(
        "a profiler",
        [d, a, e] from {
            let log = Log::default();
            let d = program(Needs::qubits(9));
            let a = Profiler::new(&counted, &log);
            let e = over(8, &log);
        },
        vec![short_offer::<Bits>(
            2,
            Needs::qubits(9),
            Offer::unlimited().max_qubits(8),
            vec![unmet_qubits(9, 8)],
        )],
    );
    // 1 qubit and 3 carbon blocks, drawn from one pool of qubits below,
    // over 5 qubits: the 7 qubits the pool draws fall short at the bits,
    // and the pool's 2 carbon blocks above them short of 3; but given the
    // 7 qubits it needs, the pool would offer 3 carbon blocks.
    short_offers_in_each_bracketing!(
        "a pool",
        [d, a, e] from {
            let log = Log::default();
            let d = program(Needs::qubits(1).blocks(Pool::carbon(), 3));
            let a = Pool::new(&log);
            let e = over(5, &log);
        },
        vec![short_offer::<Bits>(
            2,
            Needs::qubits(7),
            Offer::unlimited().max_qubits(5),
            vec![unmet_qubits(7, 5)],
        )],
    );
}

/// An adapter whose offer shrinks as the offer below grows, under a
/// program on 3 qubits using readout parities, over an executor of 20
/// qubits that does not resolve them: the adapter's offer, 1 qubit, falls
/// short of the 3, though it would offer them given exactly what it needs
/// below. The fallback names such an adapter only where the part below
/// meets what it needs below, so while the executor falls short, though of
/// another kind of need, only the executor is named, in each bracketing.
#[test]
fn the_fallback_names_no_adapter_over_a_short_part() {
    let no_parity = Offer::unlimited().max_qubits(20).without_readout_parity();
    let parities = Needs::qubits(3).with_readout_parity();
    short_offers_in_each_bracketing!(
        "an executor short of readout parities",
        [d, a, e] from {
            let log = Log::default();
            let d = Program::new(&[("mz", 0)]).needing(parities.clone());
            let a = Offering::contrary(&log);
            let e = Bits::new(&log).offering(no_parity.clone());
        },
        vec![short_offer::<Bits>(
            2,
            parities.clone(),
            no_parity.clone(),
            vec![UnmetNeed::ReadoutParity],
        )],
    );
}

/// A chain whose parts fall short, each on its own: the fault names each
/// part that, given exactly what it needs below, would still not offer
/// what reaches it, with that offer, bottom to top, in each bracketing,
/// where composed parts read the offers and needs another way.
#[test]
#[expect(clippy::too_many_lines, reason = "a table, one row per scene")]
fn the_fault_names_each_part_short_on_its_own() {
    let program = |needs: Needs| Program::new(&[("mz", 0)]).needing(needs);
    let over =
        |qubits: u32, log: &Log| Bits::new(log).offering(Offer::unlimited().max_qubits(qubits));
    let parities = Needs::qubits(1).with_readout_parity();
    let nine_and_parities = Needs::qubits(9).with_readout_parity();
    // An adapter's offer given exactly what it needs below, when that is
    // `qubits` qubits, with neither readout parities nor replay.
    let own = |qubits: u32| {
        Offer::unlimited()
            .max_qubits(qubits)
            .without_readout_parity()
            .not_replayable()
    };
    let capped = || short_offer::<Offering>(1, Needs::qubits(3), own(1), vec![unmet_qubits(3, 1)]);
    // A program on 3 qubits, through an encoding of 7 qubits per qubit
    // whose decoder holds 1 logical qubit: over 20 qubits, both the bits,
    // 20 for 21, and the encoding, 1 for 3 even given the 21 it needs, are
    // named, the encoding as over 21 qubits, where the bits are not short.
    short_offers_in_each_bracketing!(
        "a capped encoding over too few qubits",
        [d, a, e] from {
            let log = Log::default();
            let d = program(Needs::qubits(3));
            let a = Offering::capped(7, 1, &log);
            let e = over(20, &log);
        },
        vec![
            short_offer::<Bits>(
                2,
                Needs::qubits(21),
                Offer::unlimited().max_qubits(20),
                vec![unmet_qubits(21, 20)],
            ),
            capped(),
        ],
    );
    short_offers_in_each_bracketing!(
        "a capped encoding over enough qubits",
        [d, a, e] from {
            let log = Log::default();
            let d = program(Needs::qubits(3));
            let a = Offering::capped(7, 1, &log);
            let e = over(21, &log);
        },
        vec![capped()],
    );
    // A program on 3 qubits, through an adapter that offers 1 qubit over
    // more than 10 below, over 20 qubits: the bits serve the 3 qubits that
    // reach them, and the adapter, which would offer 3 given exactly
    // those, falls short of them over the 20, so it is named, with that
    // offer.
    short_offers_in_each_bracketing!(
        "an adapter short only over a larger offer below",
        [d, a, e] from {
            let log = Log::default();
            let d = program(Needs::qubits(3));
            let a = Offering::contrary(&log);
            let e = over(20, &log);
        },
        vec![short_offer::<Offering>(
            1,
            Needs::qubits(3),
            Offer::unlimited().max_qubits(1),
            vec![unmet_qubits(3, 1)],
        )],
    );
    // A program using readout parities, on bits that do not resolve them:
    // an unmet need, though the qubits fit. Two parts, so one bracketing.
    let log = Log::default();
    assert_eq!(
        admission(program(parities.clone()).on(Bits::new(&log))).short_offers,
        [short_offer::<Bits>(
            1,
            parities.clone(),
            Bits::new(&log).offers(),
            vec![UnmetNeed::ReadoutParity],
        )],
        "an executor not offering a need"
    );
    // A program using readout parities, through a lowering that cannot
    // carry them, over bits that resolve them: refused, naming the
    // lowering with its offer given exactly the 1 qubit and the parities
    // it needs below, since the bits would serve what reaches them.
    short_offers_in_each_bracketing!(
        "an adapter that cannot carry a need",
        [d, a, e] from {
            let log = Log::default();
            let d = program(parities.clone());
            let a = Offering::lowering_without_parity(&log);
            let e = Bits::new(&log).offering(Offer::unlimited());
        },
        vec![short_offer::<Offering>(
            1,
            parities.clone(),
            own(1),
            vec![UnmetNeed::ReadoutParity],
        )],
    );
    // 9 qubits and readout parities, through the lowering, over bits of 8
    // qubits that resolve parities: two shortages of different kinds, the
    // qubits at the bits and the parities at the lowering, each short on
    // its own, both named, bottom to top.
    short_offers_in_each_bracketing!(
        "shortages of different kinds",
        [d, a, e] from {
            let log = Log::default();
            let d = program(nine_and_parities.clone());
            let a = Offering::lowering_without_parity(&log);
            let e = over(8, &log);
        },
        vec![
            short_offer::<Bits>(
                2,
                nine_and_parities.clone(),
                Offer::unlimited().max_qubits(8),
                vec![unmet_qubits(9, 8)],
            ),
            short_offer::<Offering>(
                1,
                nine_and_parities.clone(),
                own(9),
                vec![UnmetNeed::ReadoutParity],
            ),
        ],
    );
}

/// Coin copies, through a toy decoder that takes a workspace from the
/// qubits below without asking for it, over 20 qubits: given exactly the
/// qubits it asks for below, the decoder would be short on its own, but
/// over the 20 it offers them all, so every real boundary fits, and the
/// chain is admitted and runs unchanged.
#[test]
fn a_part_short_only_on_its_own_does_not_refuse_its_chain() {
    let log = Log::default();
    let shots = ShotSet::ids(0..32);
    let composed = CoinCopy::new()
        .through(Offering::workspace(&log))
        .on(Bits::new(&log).offering(Offer::unlimited().max_qubits(20)));
    assert_eq!(records(composed, &shots), plain(&shots));
}

/// The capped encoding over 20 qubits, then the same with a profiler above
/// and below the encoding: the same parts named, with the same figures and
/// unmet needs, at the positions the profilers shift.
#[test]
fn inserting_a_profiler_changes_only_the_positions_of_the_short_offers() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let program = || Program::new(&[("mz", 0)]).needing(Needs::qubits(3));
    let profiler = || Profiler::new(&counted, &log);
    let capped = || Offering::capped(7, 1, &log);
    let bits = || Bits::new(&log).offering(Offer::unlimited().max_qubits(20));
    let without = admission(program().through(capped()).on(bits()));
    let with = admission(
        program()
            .through(profiler())
            .through(capped())
            .through(profiler())
            .on(bits()),
    );
    assert_eq!(
        parts(&with.short_offers),
        [part::<Bits>(4), part::<Offering>(2)]
    );
    let unplaced = |faults: AdmissionFaults| {
        faults
            .short_offers
            .into_iter()
            .map(|short| (short.part.name, short.needed, short.offered, short.unmet))
            .collect::<Vec<_>>()
    };
    assert_eq!(unplaced(with), unplaced(without));
}

/// An encoding of 7 qubits per qubit over a part reserving an ancilla,
/// over 100 qubits, whose offers do not commute: 14 qubits in this order,
/// 13 swapped. Each part the joins make of them, in each bracketing, offers
/// what the admission reads at the boundary above it, whether it is an
/// executor or an adapter over the bits: each adapter's offer applied to
/// the offer below it, the lower adapter's first.
#[test]
fn a_composed_part_offers_what_the_admission_reads_above_it() {
    let log = Log::default();
    let encoding = || Spread::new(7, &log);
    let ancilla = || Offering::reserving_an_ancilla(&log);
    let bits = || Bits::new(&log).offering(Offer::unlimited().max_qubits(100));
    let read = admission(
        Program::new(&[("mz", 0)])
            .needing(Needs::qubits(15))
            .through(encoding())
            .through(ancilla())
            .on(bits()),
    )
    .offers_by_part;
    let [_, (_, above_the_bits), (_, above_both)] = read.as_slice() else {
        panic!("the offers of the bits, the ancilla and the encoding: {read:?}");
    };
    assert_eq!(
        above_both,
        &Offer::unlimited().max_qubits(14),
        "the admission reads the ancilla's offer first"
    );
    for (row, offered, read) in [
        (
            "ancilla.on(bits)",
            ancilla().on(bits()).offers(),
            above_the_bits,
        ),
        (
            "encoding.on(ancilla.on(bits))",
            encoding().on(ancilla().on(bits())).offers(),
            above_both,
        ),
        (
            "encoding.through(ancilla).on(bits)",
            encoding().through(ancilla()).on(bits()).offers(),
            above_both,
        ),
        (
            "encoding.through(ancilla), over the bits",
            encoding().through(ancilla()).offers_above(bits().offers()),
            above_both,
        ),
    ] {
        assert_eq!(&offered, read, "{row}");
    }
}

/// A program using readout parities, or running a shot again alone,
/// through a profiler that adapts neither needs nor offers: refused over
/// bits that do not offer the need (for replay, shots replayable or a
/// seedable run), naming the bits for the need the profiler carried down
/// and listing the bits' offer as the profiler's, and admitted over bits
/// that do, bracketed either way.
#[test]
fn a_need_passes_through_an_adapter_that_adapts_neither_needs_nor_offers() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    for (row, needs, short, unmet) in [
        (
            "readout parities",
            Needs::qubits(1).with_readout_parity(),
            Bits::new(&log).offers(),
            UnmetNeed::ReadoutParity,
        ),
        (
            "replay, not replayable",
            Needs::qubits(1).with_replay(),
            Offer::unlimited().not_replayable(),
            UnmetNeed::Replayable,
        ),
        (
            "replay, not seedable",
            Needs::qubits(1).with_replay(),
            Offer::unlimited().not_seedable(),
            UnmetNeed::Replayable,
        ),
    ] {
        let program = || Program::new(&[("mz", 0)]).needing(needs.clone());
        let profiler = || Profiler::new(&counted, &log);
        let without = || Bits::new(&log).offering(short.clone());
        let with = || Bits::new(&log).offering(Offer::unlimited());
        let refused = [
            admission(program().through(profiler()).on(without())),
            admission(program().on(profiler().on(without()))),
        ];
        for admission in refused {
            assert_eq!(
                admission.needs_by_part,
                [
                    (part::<Program>(0), needs.clone()),
                    (part::<Profiler>(1), needs.clone()),
                ],
                "{row}"
            );
            assert_eq!(
                admission.offers_by_part,
                [
                    (part::<Bits>(2), short.clone()),
                    (part::<Profiler>(1), short.clone()),
                ],
                "{row}"
            );
            assert_eq!(
                admission.short_offers,
                [short_offer::<Bits>(
                    2,
                    needs.clone(),
                    short.clone(),
                    vec![unmet.clone()]
                )],
                "{row}"
            );
        }
        assert!(
            program().through(profiler()).on(with()).is_ok(),
            "{row}: left"
        );
        assert!(program().on(profiler().on(with())).is_ok(), "{row}: right");
    }
}

/// A debugger running a shot again alone, through an adapter that replays
/// its shots itself, over bits whose shots are not replayable: the adapter
/// passes no replay down, so every boundary fits and the chain is
/// admitted, bracketed either way.
#[test]
fn an_adapter_that_replays_its_shots_itself_needs_no_replayable_part_below() {
    let log = Log::default();
    let needs = Needs::qubits(1).with_replay();
    let program = || Program::new(&[("mz", 0)]).needing(needs.clone());
    let replaying = || ReplaysItself::new(&log);
    let bits = || Bits::new(&log).offering(Offer::unlimited().not_replayable());
    assert!(program().through(replaying()).on(bits()).is_ok(), "left");
    assert!(program().on(replaying().on(bits())).is_ok(), "right");
}

/// A debugger running a shot again alone, through an adapter that replays
/// its shots itself and keeps the seedability below: replay needs a
/// seedable run, which the adapter cannot offer over bits that are not, so
/// the chain is refused and the adapter named, short on its own, even when
/// the bits have room for the program, and beside the bits when they are
/// too small; over small seedable bits only the bits are named. Bracketed
/// either way.
#[test]
fn an_adapter_that_replays_its_shots_itself_needs_seedable_parts_below() {
    let log = Log::default();
    let needs = Needs::qubits(1).with_replay();
    let small = Offer::unlimited().max_qubits(0);
    let unseedable = small.clone().not_seedable();
    let roomy_unseedable = Offer::unlimited().not_seedable();
    let below = Needs::qubits(1);
    let no_qubit = vec![unmet_qubits(1, 0)];
    // Its own offer: exactly the one qubit it needs below, no readout
    // parity, replayable shots, and the bits' seedability.
    let own = Offer::unlimited()
        .max_qubits(1)
        .without_readout_parity()
        .not_seedable();
    for (row, bits, short) in [
        (
            "not seedable, room enough",
            &roomy_unseedable,
            vec![short_offer::<ReplaysItself>(
                1,
                needs.clone(),
                own.clone(),
                vec![UnmetNeed::Replayable],
            )],
        ),
        (
            "not seedable",
            &unseedable,
            vec![
                short_offer::<Bits>(2, below.clone(), unseedable.clone(), no_qubit.clone()),
                short_offer::<ReplaysItself>(1, needs.clone(), own, vec![UnmetNeed::Replayable]),
            ],
        ),
        (
            "seedable",
            &small,
            vec![short_offer::<Bits>(
                2,
                below.clone(),
                small.clone(),
                no_qubit.clone(),
            )],
        ),
    ] {
        let program = || Program::new(&[("mz", 0)]).needing(needs.clone());
        let replaying = || ReplaysItself::new(&log);
        let bits = || Bits::new(&log).offering(bits.clone());
        for admission in [
            admission(program().through(replaying()).on(bits())),
            admission(program().on(replaying().on(bits()))),
        ] {
            assert_eq!(admission.short_offers, short, "{row}");
        }
    }
}

/// 9 qubits, readout parities and replay, over bits offering 8 qubits, no
/// parities and no replay: the message names the bits, by their name and
/// position, and every unmet need.
#[test]
fn an_admission_fault_says_which_part_and_every_unmet_need() {
    let log = Log::default();
    let offer = Offer::unlimited()
        .max_qubits(8)
        .without_readout_parity()
        .not_replayable();
    let needs = Needs::qubits(9).with_readout_parity().with_replay();
    let composed = Program::new(&[("mz", 0)])
        .needing(needs)
        .on(Bits::new(&log).offering(offer));
    let message = admission(composed).to_string();
    assert!(message.contains(&part::<Bits>(1).to_string()), "{message}");
    for unmet in [
        unmet_qubits(9, 8),
        UnmetNeed::ReadoutParity,
        UnmetNeed::Replayable,
    ] {
        assert!(message.contains(&unmet.to_string()), "{unmet:?}: {message}");
    }
}

/// The two shortages of different kinds, with each part named as one
/// built at run time is, by what it was built from: the fault names each
/// part by that name, in its fields and its message.
#[test]
fn an_admission_fault_names_each_part_by_its_name() {
    let log = Log::default();
    let needs = Needs::qubits(9).with_readout_parity();
    let composed = Renamed::new(
        Program::new(&[("mz", 0)]).needing(needs.clone()),
        "h2 program",
    )
    .through(Renamed::new(
        Offering::lowering_without_parity(&log),
        "parity-free lowering",
    ))
    .on(Renamed::new(
        Bits::new(&log).offering(Offer::unlimited().max_qubits(8)),
        "qodec runtime",
    ));
    let admission = admission(composed);
    let named = |position, name| PartId::new(position, name);
    assert_eq!(
        admission
            .needs_by_part
            .iter()
            .map(|(part, _)| part.clone())
            .collect::<Vec<_>>(),
        [named(0, "h2 program"), named(1, "parity-free lowering")]
    );
    assert_eq!(
        admission
            .offers_by_part
            .iter()
            .map(|(part, _)| part.clone())
            .collect::<Vec<_>>(),
        [named(2, "qodec runtime"), named(1, "parity-free lowering")]
    );
    assert_eq!(
        parts(&admission.short_offers),
        [named(2, "qodec runtime"), named(1, "parity-free lowering")]
    );
    let message = admission.to_string();
    for name in ["qodec runtime", "parity-free lowering"] {
        assert!(message.contains(name), "{name}: {message}");
    }
}

/// A program using readout parities, through two lowerings that cannot
/// carry them, over bits that resolve them: both lowerings are named, of
/// one type, told apart by their positions.
#[test]
fn two_parts_of_one_type_are_told_apart_by_position() {
    let log = Log::default();
    let needs = Needs::qubits(1).with_readout_parity();
    let composed = Program::new(&[("mz", 0)])
        .needing(needs)
        .through(Offering::lowering_without_parity(&log))
        .through(Offering::lowering_without_parity(&log))
        .on(Bits::new(&log).offering(Offer::unlimited()));
    let admission = admission(composed);
    assert_eq!(
        parts(&admission.short_offers),
        [part::<Offering>(2), part::<Offering>(1)]
    );
    let message = admission.to_string();
    let name = type_name::<Offering>();
    assert!(message.contains(&format!("`{name}` (part 2)")), "{message}");
    assert!(message.contains(&format!("`{name}` (part 1)")), "{message}");
}
