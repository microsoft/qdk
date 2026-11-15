// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The admission: the rule of one boundary, what the shots need against
//! what the part below offers, then the admission of a chain by `>>`, at
//! every boundary, wherever the brackets go.

use std::any::type_name;
use std::cell::Cell;
use std::rc::Rc;

use crate::tests::composition::{faults, plain, records};
use crate::tests::fake_parts::{
    Bits, CoinCopy, Log, Offering, Pool, Profiler, Program, Renamed, Spread, in_each_bracketing,
};
use crate::{
    Adapt, Adapter, AdmissionFaults, BlockType, Capabilities, ChainPart, Composed,
    CompositionFaults, Drive, Driver, Execute, Executor, Needs, Runner, ShortOffer, ShotSet,
    UnmetNeed,
};

// ---------------------------------------------------------------- one boundary

fn carbon() -> BlockType {
    BlockType::new("carbon")
}

fn unmet_blocks(block_type: BlockType, needed: u32, offered: u32) -> UnmetNeed {
    UnmetNeed::Blocks {
        block_type,
        needed,
        offered,
    }
}

/// What the shots need against what the part below offers: a need is met
/// exactly when the offer covers it.
#[test]
fn a_need_is_met_exactly_when_its_offer_covers_it() {
    let eight = Capabilities::unlimited().max_qubits(8);
    let no_parity = Capabilities::unlimited().without_readout_parity();
    for (row, needs, offer, met) in [
        // A program on 9 qubits, on an executor of 8: the qubits are
        // unmet, with both counts.
        (
            "more qubits than offered",
            Needs::qubits(9),
            &eight,
            Err(vec![unmet_blocks(BlockType::qubit(), 9, 8)]),
        ),
        // The most is inclusive.
        (
            "as many qubits as offered",
            Needs::qubits(8),
            &eight,
            Ok(()),
        ),
        // A block type the executor does not list is offered 0 times.
        (
            "a block type not offered",
            Needs::default().blocks(carbon(), 2),
            &Capabilities::unlimited(),
            Err(vec![unmet_blocks(carbon(), 2, 0)]),
        ),
        (
            "an unlimited block type, for any count",
            Needs::qubits(u32::MAX),
            &Capabilities::unlimited(),
            Ok(()),
        ),
        // A need of 0 blocks of a type is no need: an executor without
        // that type serves it.
        (
            "no block of a type not offered",
            Needs::qubits(1).blocks(carbon(), 0),
            &Capabilities::unlimited(),
            Ok(()),
        ),
        // Readout parities need an executor that resolves them; one that
        // does not serves needs without them.
        (
            "readout parities, not resolved",
            Needs::qubits(1).with_readout_parity(),
            &no_parity,
            Err(vec![UnmetNeed::ReadoutParity]),
        ),
        (
            "no readout parities, not resolved",
            Needs::qubits(1),
            &no_parity,
            Ok(()),
        ),
    ] {
        assert_eq!(needs.fit(offer), met, "{row}");
    }
}

/// Every unmet need is listed at once, the blocks by type, then the
/// parities.
#[test]
fn every_unmet_need_is_listed() {
    let needs = Needs::qubits(9).blocks(carbon(), 3).with_readout_parity();
    let offer = Capabilities::unlimited()
        .max_qubits(8)
        .max_blocks(carbon(), 2)
        .without_readout_parity();
    assert_eq!(
        needs.fit(&offer),
        Err(vec![
            unmet_blocks(carbon(), 3, 2),
            unmet_blocks(BlockType::qubit(), 9, 8),
            UnmetNeed::ReadoutParity,
        ])
    );
}

// --------------------------------------------------------------------- a chain

/// The qubits `needed` where `offered` allow fewer.
fn unmet_qubits(needed: u32, offered: u32) -> UnmetNeed {
    unmet_blocks(BlockType::qubit(), needed, offered)
}

/// The part of type `P` at `position`, named by its type.
fn part<P>(position: usize) -> ChainPart {
    ChainPart::new(position, type_name::<P>())
}

/// The offer of the part of type `P` at `position`, `offered`, falling
/// short of `needed` by `unmet`.
fn short_offer<P>(
    position: usize,
    needed: Needs,
    offered: Capabilities,
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
fn parts(short_offers: &[ShortOffer]) -> Vec<ChainPart> {
    short_offers
        .iter()
        .map(|short| short.part.clone())
        .collect()
}

/// The admission faults of the chain `composed` gave, which must not be
/// admitted.
fn admission<D: Drive, E: Execute>(
    composed: Result<Runner<D, E>, CompositionFaults>,
) -> AdmissionFaults {
    faults(composed)
        .admission()
        .cloned()
        .expect("an unmet need")
}

/// A program needing exactly what the executor offers composes, and its
/// records are those on a larger executor.
#[test]
fn a_program_within_the_offer_runs_unchanged() {
    let log = Log::default();
    let shots = ShotSet::ids(0..32);
    let exact = Driver::new(CoinCopy::new())
        >> Executor::new(Bits::new(&log).offering(Capabilities::unlimited().max_qubits(2)));
    assert_eq!(records(exact, &shots), plain(&shots));
}

/// A program on 7 qubits, through an encoding of 7 qubits per qubit, over
/// 30 qubits: the `>>` fails, naming the bits' offer of 30 for 49 qubits,
/// and each part's needs and offer. The encoding's 4 for 7 only passes
/// that shortage up: given the 49 it needs below, it would offer 7, so it
/// is not named.
#[test]
fn an_unmet_need_is_a_fault_naming_the_short_part_and_each_parts_needs_and_offer() {
    let log = Log::default();
    let offer = Capabilities::unlimited().max_qubits(30);
    let composed = Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(7)))
        >> Adapter::new(Spread::new(7, &log))
        >> Executor::new(Bits::new(&log).offering(offer.clone()));
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
            (part::<Spread>(1), Capabilities::unlimited().max_qubits(4)),
        ]
    );
    assert_eq!(log.entries(), Vec::<String>::new(), "nothing provisioned");
}

/// The same encodings as the provisioning's, over 11 qubits: 3 come up
/// through the lower, 1 through the upper, each offer listed bottom to
/// top.
#[test]
fn offers_go_up_through_adapters() {
    let log = Log::default();
    let eleven = Capabilities::unlimited().max_qubits(11);
    let composed = Driver::new(CoinCopy::new())
        >> Adapter::new(Spread::new(2, &log))
        >> Adapter::new(Spread::new(3, &log))
        >> Executor::new(Bits::new(&log).offering(eleven.clone()));
    assert_eq!(
        admission(composed).offers_by_part,
        [
            (part::<Bits>(3), eleven),
            (part::<Spread>(2), Capabilities::unlimited().max_qubits(3)),
            (part::<Spread>(1), Capabilities::unlimited().max_qubits(1)),
        ]
    );
}

/// A part that only passes a shortage up, given exactly what it needs
/// below, would offer what is needed above: the fault names the part
/// below, where the shortage is, alone, though the passing part's offer
/// is short too.
#[test]
fn a_part_that_only_passes_a_shortage_up_is_not_named() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let eight = Capabilities::unlimited().max_qubits(8);
    let five = Capabilities::unlimited().max_qubits(5);
    for (row, admission, expected) in [
        // A program on 9 qubits, through a profiler, which adapts
        // neither needs nor offers, over bits offering 8.
        (
            "a profiler",
            admission(
                Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(9)))
                    >> Adapter::new(Profiler::new(&counted, &log))
                    >> Executor::new(Bits::new(&log).offering(eight.clone())),
            ),
            short_offer::<Bits>(2, Needs::qubits(9), eight, vec![unmet_qubits(9, 8)]),
        ),
        // 1 qubit and 3 carbon blocks, drawn from one pool of qubits
        // below, over 5 qubits: the 7 qubits the pool draws fall short at
        // the bits, and the pool's 2 carbon blocks above them short of 3;
        // but given the 7 qubits it needs, the pool would offer 3 carbon
        // blocks.
        (
            "a pool",
            admission(
                Driver::new(
                    Program::new(&[("mz", 0)]).needing(Needs::qubits(1).blocks(Pool::carbon(), 3)),
                ) >> Adapter::new(Pool::new(&log))
                    >> Executor::new(Bits::new(&log).offering(five.clone())),
            ),
            short_offer::<Bits>(2, Needs::qubits(7), five, vec![unmet_qubits(7, 5)]),
        ),
    ] {
        assert_eq!(admission.short_offers, [expected], "{row}");
    }
}

/// A chain whose parts fall short, each on its own: the fault names each
/// part that, given exactly what it needs below, would still not offer
/// what reaches it, with that offer, bottom to top.
#[test]
#[expect(clippy::too_many_lines, reason = "a table, one row per scene")]
fn the_fault_names_each_part_short_on_its_own() {
    let log = Log::default();
    let program = |needs: Needs| Driver::new(Program::new(&[("mz", 0)]).needing(needs));
    let over = |qubits: u32| {
        Executor::new(Bits::new(&log).offering(Capabilities::unlimited().max_qubits(qubits)))
    };
    let parities = Needs::qubits(1).with_readout_parity();
    let nine_and_parities = Needs::qubits(9).with_readout_parity();
    let capped = short_offer::<Offering>(
        1,
        Needs::qubits(3),
        Capabilities::unlimited()
            .max_qubits(1)
            .without_readout_parity(),
        vec![unmet_qubits(3, 1)],
    );
    for (row, admission, expected) in [
        // A program on 3 qubits, through an encoding of 7 qubits per
        // qubit whose decoder holds 1 logical qubit: over 20 qubits, both
        // the bits, 20 for 21, and the encoding, 1 for 3 even given the
        // 21 it needs, are named, the encoding as over 21 qubits, where
        // the bits are not short.
        (
            "a capped encoding over too few qubits",
            admission(
                program(Needs::qubits(3)) >> Adapter::new(Offering::capped(7, 1, &log)) >> over(20),
            ),
            vec![
                short_offer::<Bits>(
                    2,
                    Needs::qubits(21),
                    Capabilities::unlimited().max_qubits(20),
                    vec![unmet_qubits(21, 20)],
                ),
                capped.clone(),
            ],
        ),
        (
            "a capped encoding over enough qubits",
            admission(
                program(Needs::qubits(3)) >> Adapter::new(Offering::capped(7, 1, &log)) >> over(21),
            ),
            vec![capped],
        ),
        // A program on 3 qubits, through an adapter that offers 1 qubit
        // over more than 10 below, over 20 qubits: the bits serve the 3
        // qubits that reach them, and the adapter, which would offer 3
        // given exactly those, falls short of them over the 20, so it is
        // named, with that offer.
        (
            "an adapter short only over a larger offer below",
            admission(
                program(Needs::qubits(3)) >> Adapter::new(Offering::contrary(&log)) >> over(20),
            ),
            vec![short_offer::<Offering>(
                1,
                Needs::qubits(3),
                Capabilities::unlimited().max_qubits(1),
                vec![unmet_qubits(3, 1)],
            )],
        ),
        // A program using readout parities, on bits that do not resolve
        // them: an unmet need, though the qubits fit.
        (
            "an executor not offering a need",
            admission(program(parities.clone()) >> Executor::new(Bits::new(&log))),
            vec![short_offer::<Bits>(
                1,
                parities.clone(),
                Bits::new(&log).offers(),
                vec![UnmetNeed::ReadoutParity],
            )],
        ),
        // A program using readout parities, through a lowering that
        // cannot carry them, over bits that resolve them: refused, naming
        // the lowering with its offer given exactly the 1 qubit and the
        // parities it needs below, since the bits would serve what reaches
        // them.
        (
            "an adapter that cannot carry a need",
            admission(
                program(parities.clone())
                    >> Adapter::new(Offering::lowering_without_parity(&log))
                    >> Executor::new(Bits::new(&log).offering(Capabilities::unlimited())),
            ),
            vec![short_offer::<Offering>(
                1,
                parities,
                Capabilities::unlimited()
                    .max_qubits(1)
                    .without_readout_parity(),
                vec![UnmetNeed::ReadoutParity],
            )],
        ),
        // 9 qubits and readout parities, through the lowering, over bits
        // of 8 qubits that resolve parities: two shortages of different
        // kinds, the qubits at the bits and the parities at the lowering,
        // each short on its own, both named, bottom to top.
        (
            "shortages of different kinds",
            admission(
                program(nine_and_parities.clone())
                    >> Adapter::new(Offering::lowering_without_parity(&log))
                    >> over(8),
            ),
            vec![
                short_offer::<Bits>(
                    2,
                    nine_and_parities.clone(),
                    Capabilities::unlimited().max_qubits(8),
                    vec![unmet_qubits(9, 8)],
                ),
                short_offer::<Offering>(
                    1,
                    nine_and_parities,
                    Capabilities::unlimited()
                        .max_qubits(9)
                        .without_readout_parity(),
                    vec![UnmetNeed::ReadoutParity],
                ),
            ],
        ),
    ] {
        assert_eq!(admission.short_offers, expected, "{row}");
    }
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
    let composed = Driver::new(CoinCopy::new())
        >> Adapter::new(Offering::workspace(&log))
        >> Executor::new(Bits::new(&log).offering(Capabilities::unlimited().max_qubits(20)));
    assert_eq!(records(composed, &shots), plain(&shots));
}

/// The capped encoding over 20 qubits, then the same with a profiler above
/// and below the encoding: the same parts named, with the same figures and
/// unmet needs, at the positions the profilers shift.
#[test]
fn inserting_a_profiler_changes_only_the_positions_of_the_short_offers() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let program = || Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(3)));
    let profiler = || Adapter::new(Profiler::new(&counted, &log));
    let capped = || Adapter::new(Offering::capped(7, 1, &log));
    let bits = || Executor::new(Bits::new(&log).offering(Capabilities::unlimited().max_qubits(20)));
    let without = admission(program() >> capped() >> bits());
    let with = admission(program() >> profiler() >> capped() >> profiler() >> bits());
    assert_eq!(
        parts(&without.short_offers),
        [part::<Bits>(2), part::<Offering>(1)]
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
/// 13 swapped. Each part `>>` makes of them, in each bracketing, offers
/// what the admission reads at the boundary above it, whether it is an
/// executor or an adapter over the bits: each adapter's offer applied to
/// the offer below it, the lower adapter's first.
#[test]
fn a_composed_part_offers_what_the_admission_reads_above_it() {
    let log = Log::default();
    let encoding = || Spread::new(7, &log);
    let ancilla = || Offering::reserving_an_ancilla(&log);
    let bits = || Bits::new(&log).offering(Capabilities::unlimited().max_qubits(100));
    let read = admission(
        Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(15)))
            >> Adapter::new(encoding())
            >> Adapter::new(ancilla())
            >> Executor::new(bits()),
    )
    .offers_by_part;
    let [_, (_, above_the_bits), (_, above_both)] = read.as_slice() else {
        panic!("the offers of the bits, the ancilla and the encoding: {read:?}");
    };
    assert_eq!(
        above_both,
        &Capabilities::unlimited().max_qubits(14),
        "the admission reads the ancilla's offer first"
    );
    for (row, offered, read) in [
        (
            "ancilla >> bits",
            Composed::new(ancilla(), bits()).offers(),
            above_the_bits,
        ),
        (
            "encoding >> (ancilla >> bits)",
            Composed::new(encoding(), Composed::new(ancilla(), bits())).offers(),
            above_both,
        ),
        (
            "(encoding >> ancilla) >> bits",
            Composed::new(Composed::new(encoding(), ancilla()), bits()).offers(),
            above_both,
        ),
        (
            "encoding >> ancilla, over the bits",
            Composed::new(encoding(), ancilla()).offers_above(bits().offers()),
            above_both,
        ),
    ] {
        assert_eq!(&offered, read, "{row}");
    }
}

/// A chain refused at its admission, in each bracketing of its parts: the
/// same admission fault, whatever the brackets.
#[test]
fn where_the_brackets_go_changes_no_admission_verdict() {
    /// The one fault of the bracketings `faults`, which must all be equal.
    fn one_verdict(row: &str, faults: &[(&str, AdmissionFaults)]) -> AdmissionFaults {
        let (_, first) = &faults[0];
        for (brackets, fault) in faults {
            assert_eq!(fault, first, "{row}, {brackets}");
        }
        first.clone()
    }
    let program = |needs: Needs| Driver::new(Program::new(&[("mz", 0)]).needing(needs));
    let over = |qubits: u32, log: &Log| {
        Executor::new(Bits::new(log).offering(Capabilities::unlimited().max_qubits(qubits)))
    };
    // A program on 7 qubits, through an encoding of 7 qubits per qubit,
    // over 30 qubits: refused at the bits.
    let mut faults = Vec::new();
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let d = program(Needs::qubits(7));
            let a = Adapter::new(Spread::new(7, &log));
            let e = over(30, &log);
        },
        |chain, brackets| faults.push((brackets, admission(chain)))
    );
    let row = "an encoding";
    let encoding = one_verdict(row, &faults);
    assert_eq!(parts(&encoding.short_offers), [part::<Bits>(2)], "{row}");
    // 3 qubits and 2 carbon blocks, drawn from one pool of qubits below,
    // over 5 qubits: 7 qubits are needed, so the chain is refused at the
    // bits, though the pool's offer above (5 qubits, 2 carbon blocks)
    // admits it.
    let mut faults = Vec::new();
    in_each_bracketing!(
        [d, a, e] from {
            let log = Log::default();
            let d = program(Needs::qubits(3).blocks(Pool::carbon(), 2));
            let a = Adapter::new(Pool::new(&log));
            let e = over(5, &log);
        },
        |chain, brackets| faults.push((brackets, admission(chain)))
    );
    let row = "a pool";
    assert_eq!(
        one_verdict(row, &faults).short_offers,
        [short_offer::<Bits>(
            2,
            Needs::qubits(7),
            Capabilities::unlimited().max_qubits(5),
            vec![unmet_qubits(7, 5)]
        )],
        "{row}"
    );
    // The two shortages of different kinds, 9 qubits and readout
    // parities through a lowering over bits of 8 qubits, under a
    // profiler: naming the bits and the lowering.
    let mut faults = Vec::new();
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = program(Needs::qubits(9).with_readout_parity());
            let a = Adapter::new(Profiler::new(&counted, &log));
            let b = Adapter::new(Offering::lowering_without_parity(&log));
            let e = over(8, &log);
        },
        |chain, brackets| faults.push((brackets, admission(chain)))
    );
    let row = "an adapter's short offer";
    assert_eq!(
        parts(&one_verdict(row, &faults).short_offers),
        [part::<Bits>(3), part::<Offering>(2)],
        "{row}"
    );
    // The capped encoding over 20 qubits under a profiler: naming the
    // bits and the encoding, each short on its own.
    let mut faults = Vec::new();
    in_each_bracketing!(
        [d, a, b, e] from {
            let log = Log::default();
            let counted = Rc::new(Cell::new(0));
            let d = program(Needs::qubits(3));
            let a = Adapter::new(Profiler::new(&counted, &log));
            let b = Adapter::new(Offering::capped(7, 1, &log));
            let e = over(20, &log);
        },
        |chain, brackets| faults.push((brackets, admission(chain)))
    );
    let row = "a part short on its own";
    assert_eq!(
        parts(&one_verdict(row, &faults).short_offers),
        [part::<Bits>(3), part::<Offering>(2)],
        "{row}"
    );
}

/// A program using readout parities, through a profiler that adapts
/// neither needs nor offers: refused over bits that do not resolve them,
/// naming the bits for the parity the profiler carried down, and admitted
/// over bits that do, bracketed either way.
#[test]
fn a_readout_parity_need_passes_through_an_adapter() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let needs = Needs::qubits(1).with_readout_parity();
    let program = || Driver::new(Program::new(&[("mz", 0)]).needing(needs.clone()));
    let profiler = || Adapter::new(Profiler::new(&counted, &log));
    let without = || Executor::new(Bits::new(&log));
    let with = || Executor::new(Bits::new(&log).offering(Capabilities::unlimited()));
    let refused = [
        admission((program() >> profiler()) >> without()),
        admission(program() >> (profiler() >> without())),
    ];
    for admission in refused {
        assert_eq!(
            admission.needs_by_part,
            [
                (part::<Program>(0), needs.clone()),
                (part::<Profiler>(1), needs.clone()),
            ]
        );
        assert_eq!(
            admission.short_offers,
            [short_offer::<Bits>(
                2,
                needs.clone(),
                Bits::new(&log).offers(),
                vec![UnmetNeed::ReadoutParity]
            )]
        );
    }
    assert!(((program() >> profiler()) >> with()).is_ok(), "left");
    assert!((program() >> (profiler() >> with())).is_ok(), "right");
}

/// 9 qubits and readout parities, over bits offering 8 qubits and no
/// parities: the message names the bits and both unmet needs.
#[test]
fn an_admission_fault_says_which_part_and_every_unmet_need() {
    let log = Log::default();
    let offer = Capabilities::unlimited()
        .max_qubits(8)
        .without_readout_parity();
    let composed =
        Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(9).with_readout_parity()))
            >> Executor::new(Bits::new(&log).offering(offer));
    assert_eq!(
        admission(composed).to_string(),
        format!(
            "the chain is not admitted: `{}` (part 1) cannot serve the needs that reach it \
             (needs 9 of block type `qubit`, where the offer allows at most 8; uses readout \
             parities, and the offer does not resolve them)",
            type_name::<Bits>()
        )
    );
}

/// The two shortages of different kinds, with each part named as one
/// built at run time is, by what it was built from: the fault names each
/// part by that name, in its fields and its message.
#[test]
fn an_admission_fault_names_each_part_by_its_name() {
    let log = Log::default();
    let needs = Needs::qubits(9).with_readout_parity();
    let composed = Driver::new(Renamed::new(
        Program::new(&[("mz", 0)]).needing(needs.clone()),
        "h2 program",
    )) >> Adapter::new(Renamed::new(
        Offering::lowering_without_parity(&log),
        "parity-free lowering",
    )) >> Executor::new(Renamed::new(
        Bits::new(&log).offering(Capabilities::unlimited().max_qubits(8)),
        "qodec runtime",
    ));
    let admission = admission(composed);
    let named = |position, name| ChainPart::new(position, name);
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
    assert_eq!(
        admission.to_string(),
        "the chain is not admitted: `qodec runtime` (part 2) cannot serve the needs that \
         reach it (needs 9 of block type `qubit`, where the offer allows at most 8); \
         `parity-free lowering` (part 1) cannot serve the needs that reach it (uses readout \
         parities, and the offer does not resolve them)"
    );
}

/// A program using readout parities, through two lowerings that cannot
/// carry them, over bits that resolve them: both lowerings are named, of
/// one type, told apart by their positions.
#[test]
fn two_parts_of_one_type_are_told_apart_by_position() {
    let log = Log::default();
    let needs = Needs::qubits(1).with_readout_parity();
    let composed = Driver::new(Program::new(&[("mz", 0)]).needing(needs))
        >> Adapter::new(Offering::lowering_without_parity(&log))
        >> Adapter::new(Offering::lowering_without_parity(&log))
        >> Executor::new(Bits::new(&log).offering(Capabilities::unlimited()));
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
