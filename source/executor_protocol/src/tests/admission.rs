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
    Bits, CoinCopy, Log, LoweringWithoutParity, Pool, Profiler, Program, Spread,
};
use crate::{
    Adapter, AdmissionFaults, BlockType, Capabilities, CompositionFaults, Drive, Driver, Execute,
    Executor, Needs, Runner, ShortOffer, ShotSet, UnmetNeed,
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

/// A program on 9 qubits, on an executor of 8: the qubits are unmet, with
/// both counts.
#[test]
fn more_blocks_than_offered_are_unmet() {
    let offer = Capabilities::unlimited().max_qubits(8);
    assert_eq!(
        Needs::qubits(9).fit(&offer),
        Err(vec![unmet_blocks(BlockType::qubit(), 9, 8)])
    );
}

/// As many qubits as offered fit: the most is inclusive.
#[test]
fn as_many_blocks_as_offered_fit() {
    let offer = Capabilities::unlimited().max_qubits(8);
    assert_eq!(Needs::qubits(8).fit(&offer), Ok(()));
}

/// A block type the executor does not list is offered 0 times.
#[test]
fn a_block_type_not_offered_is_offered_none() {
    let needs = Needs::default().blocks(carbon(), 2);
    assert_eq!(
        needs.fit(&Capabilities::unlimited()),
        Err(vec![unmet_blocks(carbon(), 2, 0)])
    );
}

/// An unlimited block type serves any count.
#[test]
fn an_unlimited_block_type_serves_any_count() {
    assert_eq!(
        Needs::qubits(u32::MAX).fit(&Capabilities::unlimited()),
        Ok(())
    );
}

/// A need of 0 blocks of a type is no need: an executor without that type
/// serves it.
#[test]
fn needing_no_block_of_a_type_needs_nothing_of_it() {
    let needs = Needs::qubits(1).blocks(carbon(), 0);
    assert_eq!(needs.fit(&Capabilities::unlimited()), Ok(()));
}

/// Readout parities need an executor that resolves them; one that does not
/// serves needs without them.
#[test]
fn readout_parities_need_an_executor_resolving_them() {
    let offer = Capabilities::unlimited().without_readout_parity();
    assert_eq!(
        Needs::qubits(1).with_readout_parity().fit(&offer),
        Err(vec![UnmetNeed::ReadoutParity])
    );
    assert_eq!(Needs::qubits(1).fit(&offer), Ok(()));
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

/// The offer of `part`, `offered`, falling short of `needed` by `unmet`.
fn short_offer<P>(needed: Needs, offered: Capabilities, unmet: Vec<UnmetNeed>) -> ShortOffer {
    ShortOffer {
        part: type_name::<P>(),
        needed,
        offered,
        unmet,
    }
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
/// where the shortage starts, and each part's needs and offer. The
/// encoding's 4 for 7 only passes that shortage up, so it is not named.
#[test]
fn an_unmet_need_is_a_fault_naming_where_it_starts_and_each_parts_needs_and_offer() {
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
            Needs::qubits(49),
            offer.clone(),
            vec![unmet_qubits(49, 30)]
        )]
    );
    assert_eq!(
        admission.needs_by_part,
        [
            (type_name::<Program>(), Needs::qubits(7)),
            (type_name::<Spread>(), Needs::qubits(49)),
        ]
    );
    assert_eq!(
        admission.offers_by_part,
        [
            (type_name::<Bits>(), offer),
            (
                type_name::<Spread>(),
                Capabilities::unlimited().max_qubits(4)
            ),
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
            (type_name::<Bits>(), eleven),
            (
                type_name::<Spread>(),
                Capabilities::unlimited().max_qubits(3)
            ),
            (
                type_name::<Spread>(),
                Capabilities::unlimited().max_qubits(1)
            ),
        ]
    );
}

/// A program on 9 qubits, through a profiler, which adapts neither needs
/// nor offers, over bits offering 8: the fault names the bits alone,
/// though the profiler's offer, the bits', is short too.
#[test]
fn a_part_that_only_passes_a_shortage_up_is_not_named() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let eight = Capabilities::unlimited().max_qubits(8);
    let composed = Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(9)))
        >> Adapter::new(Profiler::new(&counted, &log))
        >> Executor::new(Bits::new(&log).offering(eight.clone()));
    assert_eq!(
        admission(composed).short_offers,
        [short_offer::<Bits>(
            Needs::qubits(9),
            eight,
            vec![unmet_qubits(9, 8)]
        )]
    );
}

/// The encoding of 7 over 30 qubits, then the same with a profiler above
/// and below the encoding: the same short offers, and the same message.
#[test]
fn inserting_a_profiler_changes_no_short_offer() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let thirty = || Capabilities::unlimited().max_qubits(30);
    let program = || Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(7)));
    let profiler = || Adapter::new(Profiler::new(&counted, &log));
    let spread = || Adapter::new(Spread::new(7, &log));
    let bits = || Executor::new(Bits::new(&log).offering(thirty()));
    let without = admission(program() >> spread() >> bits());
    let with = admission(program() >> profiler() >> spread() >> profiler() >> bits());
    assert_eq!(with.short_offers, without.short_offers);
    assert_eq!(with.to_string(), without.to_string());
}

/// The same chain, bracketed both ways: the same admission fault.
#[test]
fn where_the_brackets_go_changes_no_admission_fault() {
    let log = Log::default();
    let offer = Capabilities::unlimited().max_qubits(30);
    let program = || Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(7)));
    let spread = || Adapter::new(Spread::new(7, &log));
    let bits = || Executor::new(Bits::new(&log).offering(offer.clone()));
    let left = admission((program() >> spread()) >> bits());
    let right = admission(program() >> (spread() >> bits()));
    assert_eq!(right.short_offers.len(), 1, "{right}");
    assert_eq!(left, right);
}

/// 3 qubits and 2 carbon blocks, drawn from one pool of qubits below, over
/// 5 qubits: 7 qubits are needed, so the chain is refused at the bits,
/// bracketed either way, though the pool's offer above (5 qubits, 2
/// carbon blocks) admits it.
#[test]
fn where_the_brackets_go_changes_no_verdict_even_through_a_pool() {
    let log = Log::default();
    let needs = Needs::qubits(3).blocks(Pool::carbon(), 2);
    let five = Capabilities::unlimited().max_qubits(5);
    let program = || Driver::new(Program::new(&[("mz", 0)]).needing(needs.clone()));
    let pool = || Adapter::new(Pool::new(&log));
    let bits = || Executor::new(Bits::new(&log).offering(five.clone()));
    let left = admission((program() >> pool()) >> bits());
    let right = admission(program() >> (pool() >> bits()));
    assert_eq!(
        right.short_offers,
        [short_offer::<Bits>(
            Needs::qubits(7),
            five,
            vec![unmet_qubits(7, 5)]
        )]
    );
    assert_eq!(left, right);
}

/// A program using readout parities, on bits that do not resolve them:
/// an unmet need, though the qubits fit.
#[test]
fn a_need_the_executor_does_not_offer_is_a_fault() {
    let log = Log::default();
    let needs = Needs::qubits(1).with_readout_parity();
    let composed = Driver::new(Program::new(&[("mz", 0)]).needing(needs.clone()))
        >> Executor::new(Bits::new(&log));
    assert_eq!(
        admission(composed).short_offers,
        [short_offer::<Bits>(
            needs,
            Bits::new(&log).offers(),
            vec![UnmetNeed::ReadoutParity]
        )]
    );
}

/// A program using readout parities, through a lowering that cannot carry
/// them, over bits that resolve them: refused, naming the lowering, since
/// the bits would serve what reaches them.
#[test]
fn an_adapter_that_cannot_carry_a_need_is_named_by_the_fault() {
    let log = Log::default();
    let needs = Needs::qubits(1).with_readout_parity();
    let composed = Driver::new(Program::new(&[("mz", 0)]).needing(needs.clone()))
        >> Adapter::new(LoweringWithoutParity::new(&log))
        >> Executor::new(Bits::new(&log).offering(Capabilities::unlimited()));
    assert_eq!(
        admission(composed).short_offers,
        [short_offer::<LoweringWithoutParity>(
            needs,
            Capabilities::unlimited().without_readout_parity(),
            vec![UnmetNeed::ReadoutParity]
        )]
    );
}

/// 9 qubits and readout parities, through the lowering, over bits of 8
/// qubits that resolve parities: two shortages of different kinds, the
/// qubits starting at the bits and the parities at the lowering, both
/// named, bottom to top.
#[test]
fn shortages_of_different_kinds_name_each_part_where_one_starts() {
    let log = Log::default();
    let needs = Needs::qubits(9).with_readout_parity();
    let eight = Capabilities::unlimited().max_qubits(8);
    let composed = Driver::new(Program::new(&[("mz", 0)]).needing(needs.clone()))
        >> Adapter::new(LoweringWithoutParity::new(&log))
        >> Executor::new(Bits::new(&log).offering(eight.clone()));
    assert_eq!(
        admission(composed).short_offers,
        [
            short_offer::<Bits>(needs.clone(), eight.clone(), vec![unmet_qubits(9, 8)]),
            short_offer::<LoweringWithoutParity>(
                needs,
                eight.without_readout_parity(),
                vec![UnmetNeed::ReadoutParity]
            ),
        ]
    );
}

/// 1 qubit and 3 carbon blocks, drawn from one pool of qubits below, over
/// 5 qubits: the 7 qubits the pool draws fall short at the bits, and the
/// pool's 2 carbon blocks above them short of 3; a shortage of another
/// block type starts at the pool, so both are named.
#[test]
fn a_shortage_of_another_block_type_is_named_where_it_starts() {
    let log = Log::default();
    let needs = Needs::qubits(1).blocks(Pool::carbon(), 3);
    let five = Capabilities::unlimited().max_qubits(5);
    let composed = Driver::new(Program::new(&[("mz", 0)]).needing(needs.clone()))
        >> Adapter::new(Pool::new(&log))
        >> Executor::new(Bits::new(&log).offering(five.clone()));
    assert_eq!(
        admission(composed).short_offers,
        [
            short_offer::<Bits>(Needs::qubits(7), five.clone(), vec![unmet_qubits(7, 5)]),
            short_offer::<Pool>(
                needs,
                five.max_blocks(Pool::carbon(), 2),
                vec![unmet_blocks(Pool::carbon(), 3, 2)]
            ),
        ]
    );
}

/// The two shortages of different kinds under a profiler, in each of the
/// five bracketings of four parts: the same admission fault, naming the
/// bits and the lowering.
#[test]
fn where_the_brackets_go_changes_no_adapters_short_offer() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let needs = Needs::qubits(9).with_readout_parity();
    let program = || Driver::new(Program::new(&[("mz", 0)]).needing(needs.clone()));
    let profiler = || Adapter::new(Profiler::new(&counted, &log));
    let lowering = || Adapter::new(LoweringWithoutParity::new(&log));
    let bits = || Executor::new(Bits::new(&log).offering(Capabilities::unlimited().max_qubits(8)));
    let left = admission(((program() >> profiler()) >> lowering()) >> bits());
    let bracketed = [
        admission((program() >> profiler()) >> (lowering() >> bits())),
        admission(program() >> (profiler() >> (lowering() >> bits()))),
        admission(program() >> ((profiler() >> lowering()) >> bits())),
        admission((program() >> (profiler() >> lowering())) >> bits()),
    ];
    assert_eq!(
        left.short_offers
            .iter()
            .map(|short| short.part)
            .collect::<Vec<_>>(),
        [type_name::<Bits>(), type_name::<LoweringWithoutParity>()]
    );
    for (i, admission) in bracketed.iter().enumerate() {
        assert_eq!(*admission, left, "bracketing {i}");
    }
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
                (type_name::<Program>(), needs.clone()),
                (type_name::<Profiler>(), needs.clone()),
            ]
        );
        assert_eq!(
            admission.short_offers,
            [short_offer::<Bits>(
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
            "the chain is not admitted: `{}` cannot serve the needs that reach it (needs 9 of \
             block type `qubit`, where the offer allows at most 8; uses readout parities, and \
             the offer does not resolve them)",
            type_name::<Bits>()
        )
    );
}

/// The two shortages of different kinds: the message names each part
/// where one starts, with its unmet need, bottom to top.
#[test]
fn an_admission_fault_names_every_short_part() {
    let log = Log::default();
    let composed =
        Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(9).with_readout_parity()))
            >> Adapter::new(LoweringWithoutParity::new(&log))
            >> Executor::new(Bits::new(&log).offering(Capabilities::unlimited().max_qubits(8)));
    assert_eq!(
        admission(composed).to_string(),
        format!(
            "the chain is not admitted: `{}` cannot serve the needs that reach it (needs 9 of \
             block type `qubit`, where the offer allows at most 8); `{}` cannot serve the \
             needs that reach it (uses readout parities, and the offer does not resolve them)",
            type_name::<Bits>(),
            type_name::<LoweringWithoutParity>()
        )
    );
}
