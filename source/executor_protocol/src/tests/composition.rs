// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Composing parts with `>>`: the fit at each boundary, the admission of
//! the whole chain, the laws of the brackets, and each part's
//! provisioning.

use std::any::type_name;
use std::cell::Cell;
use std::rc::Rc;

use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, CoinGates, FromThread, Later, Log, Pool, Profiler, Program, Spread,
    Values,
};
use crate::{
    Adapter, BlockType, Capabilities, CompositionFaults, Drive, Driver, Execute, Executor,
    FitFaults, InstructionDef, InstructionMismatch, InstructionSet, Meaning, Needs, Pauli, Runner,
    ShotSet, UnmetNeed,
};

const SEED: u64 = 7;

/// The instructions of [`CoinGates`] but `missing`, as the set `name`.
fn coin_gates_without(name: &str, missing: &str) -> InstructionSet {
    let defs = [("h", 0), ("x", 0), ("mz", 1)]
        .into_iter()
        .filter(|&(mnemonic, _)| mnemonic != missing)
        .map(|(mnemonic, outcomes)| {
            InstructionDef::new(mnemonic)
                .acts_on([BlockType::qubit()])
                .outcomes(outcomes)
        });
    InstructionSet::new(name, [(BlockType::qubit(), 1)], defs).expect("a valid set")
}

/// [`CoinGates`] without `x`.
fn no_x() -> InstructionSet {
    coin_gates_without("NoX", "x")
}

/// [`CoinGates`] without `mz`.
fn no_mz() -> InstructionSet {
    coin_gates_without("NoMz", "mz")
}

/// The records of `shots` on the chain `composed` gave, which must compose.
fn records<D: Drive, E: Execute>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    shots: &ShotSet,
) -> Vec<D::Output> {
    let Ok(mut runner) = composed else {
        panic!("the chain composes");
    };
    runner.run(shots, SEED).expect("the run succeeds")
}

/// The faults of the chain `composed` gave, which must not compose.
fn faults<D: Drive, E: Execute>(
    composed: Result<Runner<D, E>, CompositionFaults>,
) -> CompositionFaults {
    composed.err().expect("the chain does not compose")
}

/// The records of [`CoinCopy`] on bits alone, for `shots`.
fn plain(shots: &ShotSet) -> Vec<Values> {
    let log = Log::default();
    records(
        Driver::new(CoinCopy::new()) >> Executor::new(Bits::new(&log)),
        shots,
    )
}

/// `h`, `x` and `mz` on qubits, in a set `name`, `h` doing `h_does`.
fn gates(name: &str, h_does: Meaning) -> InstructionSet {
    let qubit = || [BlockType::qubit()];
    InstructionSet::new(
        name,
        [(BlockType::qubit(), 1)],
        [
            InstructionDef::new("h").acts_on(qubit()).meaning(h_does),
            InstructionDef::new("x").acts_on(qubit()),
            InstructionDef::new("mz").acts_on(qubit()).outcomes(1),
        ],
    )
    .expect("a valid set")
}

fn hadamard() -> Meaning {
    Meaning::declared().clifford([(Pauli::x(0), Pauli::z(0)), (Pauli::z(0), Pauli::x(0))])
}

/// The fit faults of `sent` against `accepted`, as the fit finds them.
fn fit_faults(sent: &InstructionSet, accepted: &InstructionSet) -> FitFaults {
    FitFaults {
        sent: sent.name().to_owned(),
        accepted: accepted.name().to_owned(),
        mismatches: match sent.fits(accepted) {
            Ok(_) => panic!("a boundary that does not fit"),
            Err(mismatches) => mismatches,
        },
    }
}

// ---------------------------------------------------------------- the fit

/// A code level whose sets do not fit, at two boundaries: the runner is
/// never made, and the fault holds each boundary, top to bottom, with both
/// sets named.
#[test]
fn every_boundary_that_does_not_fit_is_a_fault_of_the_chain() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let composed = Driver::new(CoinCopy::new())
        >> Adapter::new(Profiler::on(no_x(), &counted, &log))
        >> Executor::new(Bits::accepting(no_mz(), &log));
    assert_eq!(
        faults(composed).fit(),
        [
            fit_faults(&CoinGates::set(), &no_x()),
            fit_faults(&no_x(), &no_mz()),
        ]
    );
}

/// A lowering whose `h` does something else than the `h` below: the `>>`
/// refuses it before any shot, and no part is provisioned.
#[test]
fn an_instruction_doing_something_else_below_is_a_fault_before_any_shot() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let sent = gates("Gates", hadamard());
    let accepted = gates("Gates", Meaning::declared().pauli(Pauli::x(0)));
    let lowered = Adapter::new(Profiler::on(sent.clone(), &counted, &log))
        >> Executor::new(Bits::accepting(accepted.clone(), &log));
    let expected = FitFaults {
        sent: "Gates".to_owned(),
        accepted: "Gates".to_owned(),
        mismatches: vec![InstructionMismatch::Meaning {
            mnemonic: "h".to_owned(),
        }],
    };
    assert_eq!(lowered.check(), Err(&[expected.clone()][..]));
    let faults = faults(Driver::new(CoinCopy::new()) >> lowered);
    assert_eq!(faults.fit().last(), Some(&expected));
    assert_eq!(log.entries(), Vec::<String>::new(), "nothing provisioned");
}

/// A join that does not make a runner keeps the faults of its boundary,
/// for its `check`; one that fits checks `Ok`.
#[test]
fn a_join_reports_the_faults_of_its_boundaries_on_check() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let fits = Adapter::new(Profiler::new(&counted, &log)) >> Executor::new(Bits::new(&log));
    assert_eq!(fits.check(), Ok(()));
    let driver = Driver::new(CoinCopy::new()) >> Adapter::new(Profiler::on(no_x(), &counted, &log));
    assert_eq!(
        driver.check(),
        Err(&[fit_faults(&CoinGates::set(), &no_x())][..])
    );
    let adapter = Adapter::new(Profiler::new(&counted, &log))
        >> Adapter::new(Profiler::on(no_x(), &counted, &log));
    assert_eq!(
        adapter.check(),
        Err(&[fit_faults(&CoinGates::set(), &no_x())][..])
    );
}

/// An executor whose opcodes are in another order than the driver's: the
/// driver writes in the executor's opcodes, so the records are those on
/// an executor of the driver's own set.
#[test]
fn a_sender_writes_in_the_receivers_opcodes() {
    let log = Log::default();
    let reordered = InstructionSet::new(
        "CoinGates",
        [(BlockType::qubit(), 1)],
        [
            InstructionDef::new("mz")
                .acts_on([BlockType::qubit()])
                .outcomes(1),
            InstructionDef::new("x").acts_on([BlockType::qubit()]),
            InstructionDef::new("h").acts_on([BlockType::qubit()]),
        ],
    )
    .expect("a valid set");
    let shots = ShotSet::ids(0..16);
    assert_ne!(reordered.opcode("h"), Some(CoinGates::H), "another order");
    let composed = Driver::new(CoinCopy::new()) >> Executor::new(Bits::accepting(reordered, &log));
    assert_eq!(records(composed, &shots), plain(&shots));
}

// ---------------------------------------------------------------- the laws

/// One driver, unchanged, on several builds of the chain below it: the
/// same records on each.
#[test]
fn one_driver_runs_on_every_chain_it_fits() {
    let log = Log::default();
    let shots = ShotSet::ids(0..32);
    let expected = plain(&shots);
    let driver = || Driver::new(CoinCopy::new());
    assert_eq!(
        records(driver() >> Executor::new(Later(Bits::new(&log))), &shots),
        expected,
        "answering later"
    );
    assert_eq!(
        records(
            driver() >> Executor::new(FromThread::new(Bits::new(&log), Default::default())),
            &shots
        ),
        expected,
        "answering from a thread"
    );
    assert_eq!(
        records(
            driver() >> (Adapter::new(Spread::new(2, &log)) >> Executor::new(Bits::new(&log))),
            &shots
        ),
        expected,
        "through an encoding"
    );
}

/// A chain of a driver, a profiler, a noise and bits, bracketed three
/// ways: the same records.
#[test]
fn where_the_brackets_go_changes_no_record() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let shots = ShotSet::ids(0..32);
    let d = || Driver::new(CoinCopy::new());
    let profiler = || Adapter::new(Profiler::new(&counted, &log));
    let noise = || Adapter::new(BitFlip::new(300, &log));
    let bits = || Executor::new(Bits::new(&log));
    let left = records(((d() >> profiler()) >> noise()) >> bits(), &shots);
    let right = records(d() >> (profiler() >> (noise() >> bits())), &shots);
    let middle = records((d() >> profiler()) >> (noise() >> bits()), &shots);
    assert_eq!(left, right);
    assert_eq!(left, middle);
}

/// The same chain, with two faults, bracketed both ways: the same faults.
#[test]
fn where_the_brackets_go_changes_no_fault() {
    let log = Log::default();
    let counted = Rc::new(Cell::new(0));
    let d = || Driver::new(CoinCopy::new());
    let profiler = || Adapter::new(Profiler::on(no_x(), &counted, &log));
    let bits = || Executor::new(Bits::accepting(no_mz(), &log));
    let left = faults((d() >> profiler()) >> bits());
    let right = faults(d() >> (profiler() >> bits()));
    assert_eq!(left.fit(), right.fit());
    assert_eq!(left.fit().len(), 2);
}

// ---------------------------------------------------------------- the admission

/// A program on 7 qubits, through an encoding of 7 qubits per qubit, over
/// 30 qubits: the `>>` fails, naming the needs that reach the bits (49
/// qubits), the bits' offer, and each part's.
#[test]
fn an_unmet_need_is_a_fault_naming_each_parts_needs_and_offer() {
    let log = Log::default();
    let offer = Capabilities::unlimited().max_qubits(30);
    let composed = Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(7)))
        >> Adapter::new(Spread::new(7, &log))
        >> Executor::new(Bits::new(&log).offering(offer.clone()));
    let faults = faults(composed);
    assert_eq!(faults.fit(), []);
    let admission = faults.admission().expect("an unmet need");
    assert_eq!(admission.needed, Needs::qubits(49));
    assert_eq!(admission.offered, offer);
    assert_eq!(
        admission.unmet,
        [UnmetNeed::Blocks {
            block_type: BlockType::qubit(),
            needed: 49,
            offered: 30,
        }]
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

/// The same chain, bracketed both ways: the same admission fault, with the
/// numbers where the needs reach the bits, 49 and 30.
#[test]
fn where_the_brackets_go_changes_no_admission_fault() {
    let log = Log::default();
    let offer = Capabilities::unlimited().max_qubits(30);
    let program = || Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(7)));
    let spread = || Adapter::new(Spread::new(7, &log));
    let bits = || Executor::new(Bits::new(&log).offering(offer.clone()));
    let left = faults((program() >> spread()) >> bits());
    let right = faults(program() >> (spread() >> bits()));
    let left = left.admission().expect("an unmet need");
    let right = right.admission().expect("an unmet need");
    assert_eq!(right.needed, Needs::qubits(49));
    assert_eq!(right.offered, offer);
    assert_eq!(
        right.unmet,
        [UnmetNeed::Blocks {
            block_type: BlockType::qubit(),
            needed: 49,
            offered: 30,
        }]
    );
    assert_eq!(left, right);
}

/// 3 qubits and 2 carbon blocks, drawn from one pool of qubits below, over
/// 5 qubits: 7 qubits are needed, so the chain is refused, bracketed
/// either way, though the pool's offer above (5 qubits, 2 carbon blocks)
/// would admit it.
#[test]
fn where_the_brackets_go_changes_no_verdict_even_through_a_pool() {
    let log = Log::default();
    let needs = Needs::qubits(3).blocks(Pool::carbon(), 2);
    let program = || Driver::new(Program::new(&[("mz", 0)]).needing(needs.clone()));
    let pool = || Adapter::new(Pool::new(&log));
    let bits = || Executor::new(Bits::new(&log).offering(Capabilities::unlimited().max_qubits(5)));
    let unmet = [UnmetNeed::Blocks {
        block_type: BlockType::qubit(),
        needed: 7,
        offered: 5,
    }];
    let left = faults((program() >> pool()) >> bits());
    let right = faults(program() >> (pool() >> bits()));
    assert_eq!(
        left.admission().expect("an unmet need").unmet,
        unmet,
        "left"
    );
    assert_eq!(
        right.admission().expect("an unmet need").unmet,
        unmet,
        "right"
    );
}

/// A program using readout parities, on bits that do not resolve them:
/// an unmet need, though the qubits fit.
#[test]
fn a_need_the_executor_does_not_offer_is_a_fault() {
    let log = Log::default();
    let composed =
        Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(1).with_readout_parity()))
            >> Executor::new(Bits::new(&log));
    let faults = faults(composed);
    let admission = faults.admission().expect("an unmet need");
    assert_eq!(admission.unmet, [UnmetNeed::ReadoutParity]);
}

/// 9 qubits and readout parities, over bits offering 8 qubits and no
/// parities: the message names the bits and both unmet needs.
#[test]
fn an_admission_fault_says_which_executor_and_every_unmet_need() {
    let log = Log::default();
    let offer = Capabilities::unlimited()
        .max_qubits(8)
        .without_readout_parity();
    let composed =
        Driver::new(Program::new(&[("mz", 0)]).needing(Needs::qubits(9).with_readout_parity()))
            >> Executor::new(Bits::new(&log).offering(offer));
    let faults = faults(composed);
    let admission = faults.admission().expect("an unmet need");
    assert_eq!(
        admission.to_string(),
        format!(
            "the executor `{}` cannot serve the needs that reach it: needs 9 of block type \
             `qubit`, where the offer allows at most 8; uses readout parities, and the offer \
             does not resolve them",
            type_name::<Bits>()
        )
    );
}

/// A chain that does not fit and whose needs are unmet: both, at once.
#[test]
fn the_fit_and_the_admission_are_reported_together() {
    let log = Log::default();
    let composed = Driver::new(CoinCopy::new())
        >> Executor::new(
            Bits::accepting(no_mz(), &log).offering(Capabilities::unlimited().max_qubits(1)),
        );
    let faults = faults(composed);
    assert_eq!(faults.fit(), [fit_faults(&CoinGates::set(), &no_mz())]);
    assert!(faults.admission().is_some(), "{faults}");
    let message = faults.to_string();
    assert!(message.contains("does not fit"), "{message}");
    assert!(message.contains("needs 2 of block type"), "{message}");
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

/// Two encodings over bits: the driver's 2 qubits come down as 4, then
/// 12, and each part is provisioned with what reaches it, once, however
/// many runs follow.
#[test]
fn needs_come_down_through_adapters_and_provision_each_part_once() {
    let log = Log::default();
    let composed = Driver::new(CoinCopy::new())
        >> Adapter::new(Spread::new(2, &log))
        >> Adapter::new(Spread::new(3, &log))
        >> Executor::new(Bits::new(&log));
    let Ok(mut runner) = composed else {
        panic!("the chain composes");
    };
    for _ in 0..2 {
        runner
            .run(&ShotSet::ids(0..4), SEED)
            .expect("the run succeeds");
    }
    let provisioned: Vec<String> = log
        .entries()
        .into_iter()
        .filter(|entry| entry.contains("provisioned"))
        .collect();
    assert_eq!(
        provisioned,
        [
            "Spread provisioned for 2 qubits",
            "Spread provisioned for 4 qubits",
            "Bits provisioned for 12 qubits",
        ]
    );
}

/// The same encodings over 11 qubits: 3 come up through the lower, 1
/// through the upper, each offer listed bottom to top; the driver's 2
/// qubits reach the bits as 12, so they are unmet.
#[test]
fn offers_go_up_through_adapters() {
    let log = Log::default();
    let eleven = Capabilities::unlimited().max_qubits(11);
    let composed = Driver::new(CoinCopy::new())
        >> Adapter::new(Spread::new(2, &log))
        >> Adapter::new(Spread::new(3, &log))
        >> Executor::new(Bits::new(&log).offering(eleven.clone()));
    let faults = faults(composed);
    let admission = faults.admission().expect("an unmet need");
    assert_eq!(
        admission.offers_by_part,
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
    assert_eq!(
        admission.unmet,
        [UnmetNeed::Blocks {
            block_type: BlockType::qubit(),
            needed: 12,
            offered: 11,
        }]
    );
}
