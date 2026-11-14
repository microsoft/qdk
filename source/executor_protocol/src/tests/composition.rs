// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Composing parts with `>>`: the fit at each boundary, the laws of the
//! brackets, the fit and the admission reported together, and each part's
//! provisioning. The admission of a chain is in `admission.rs`.

use std::cell::Cell;
use std::rc::Rc;

use crate::tests::fake_parts::{
    BitFlip, Bits, CoinCopy, CoinGates, FromThread, Later, Log, Profiler, Program, Shifting,
    Spread, Values,
};
use crate::{
    Adapter, BlockType, Capabilities, CompositionFaults, Drive, Driver, Execute, Executor,
    FitFaults, InstructionDef, InstructionMismatch, InstructionSet, Meaning, Needs, Pauli, Runner,
    ShotSet,
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
pub(super) fn records<D: Drive, E: Execute>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    shots: &ShotSet,
) -> Vec<D::Output> {
    let Ok(mut runner) = composed else {
        panic!("the chain composes");
    };
    runner.run(shots, SEED).expect("the run succeeds")
}

/// The faults of the chain `composed` gave, which must not compose.
pub(super) fn faults<D: Drive, E: Execute>(
    composed: Result<Runner<D, E>, CompositionFaults>,
) -> CompositionFaults {
    composed.err().expect("the chain does not compose")
}

/// The records of [`CoinCopy`] on bits alone, for `shots`.
pub(super) fn plain(shots: &ShotSet) -> Vec<Values> {
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

// --------------------------------------------------------- the faults together

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

// ------------------------------------------------------------ the provisioning

/// What `composed` provisions over two runs, in order.
fn provisioned<D: Drive, E: Execute>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    log: &Log,
) -> Vec<String> {
    logged_over_two_runs(composed, log, "provisioned")
}

/// What `composed` logs over two runs, in order, of entries containing
/// `containing`.
fn logged_over_two_runs<D: Drive, E: Execute>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    log: &Log,
    containing: &str,
) -> Vec<String> {
    let Ok(mut runner) = composed else {
        panic!("the chain composes");
    };
    for _ in 0..2 {
        runner
            .run(&ShotSet::ids(0..4), SEED)
            .expect("the run succeeds");
    }
    log.entries()
        .into_iter()
        .filter(|entry| entry.contains(containing))
        .collect()
}

/// What a chain logs, to the log it is given, of what a test watches.
type Chain = fn(&Log) -> Vec<String>;

/// Two encodings over bits, in each bracketing that `>>` makes a runner
/// of: the driver's 2 qubits come down as 4, then 12, and each part is
/// provisioned with what reaches it, once, however many runs follow.
#[test]
fn needs_come_down_through_adapters_and_provision_each_part_once() {
    let chains: [(&str, Chain); 3] = [
        ("left", |log| {
            provisioned(
                Driver::new(CoinCopy::new())
                    >> Adapter::new(Spread::new(2, log))
                    >> Adapter::new(Spread::new(3, log))
                    >> Executor::new(Bits::new(log)),
                log,
            )
        }),
        ("right", |log| {
            provisioned(
                Driver::new(CoinCopy::new())
                    >> (Adapter::new(Spread::new(2, log))
                        >> (Adapter::new(Spread::new(3, log)) >> Executor::new(Bits::new(log)))),
                log,
            )
        }),
        ("adapters joined", |log| {
            provisioned(
                Driver::new(CoinCopy::new())
                    >> ((Adapter::new(Spread::new(2, log)) >> Adapter::new(Spread::new(3, log)))
                        >> Executor::new(Bits::new(log))),
                log,
            )
        }),
    ];
    for (bracketing, chain) in chains {
        assert_eq!(
            chain(&Log::default()),
            [
                "Spread provisioned for 2 qubits",
                "Spread provisioned for 4 qubits",
                "Bits provisioned for 12 qubits",
            ],
            "{bracketing}"
        );
    }
}

/// A program stating 1 qubit, then 3 once provisioned, which its rule
/// forbids, through two encodings of 1 over bits, in each of the five
/// bracketings: every part is provisioned for the 1 qubit the admission
/// checked, since the driver's needs are read before it is provisioned.
#[test]
fn every_part_is_provisioned_for_the_needs_read_before_the_drivers_provision() {
    fn program() -> Driver<Program> {
        Driver::new(
            Program::new(&[("mz", 0)])
                .needing(Needs::qubits(1))
                .needing_once_provisioned(Needs::qubits(3)),
        )
    }
    let chains: [(&str, Chain); 5] = [
        ("left", |log| {
            let spread = || Adapter::new(Spread::new(1, log));
            provisioned(
                ((program() >> spread()) >> spread()) >> Executor::new(Bits::new(log)),
                log,
            )
        }),
        ("right", |log| {
            let spread = || Adapter::new(Spread::new(1, log));
            provisioned(
                program() >> (spread() >> (spread() >> Executor::new(Bits::new(log)))),
                log,
            )
        }),
        ("middle", |log| {
            let spread = || Adapter::new(Spread::new(1, log));
            provisioned(
                (program() >> spread()) >> (spread() >> Executor::new(Bits::new(log))),
                log,
            )
        }),
        ("adapters joined", |log| {
            let spread = || Adapter::new(Spread::new(1, log));
            provisioned(
                program() >> ((spread() >> spread()) >> Executor::new(Bits::new(log))),
                log,
            )
        }),
        ("adapters joined to the driver", |log| {
            let spread = || Adapter::new(Spread::new(1, log));
            provisioned(
                (program() >> (spread() >> spread())) >> Executor::new(Bits::new(log)),
                log,
            )
        }),
    ];
    for (bracketing, chain) in chains {
        assert_eq!(
            chain(&Log::default()),
            [
                "Spread provisioned for 1 qubits",
                "Spread provisioned for 1 qubits",
                "Bits provisioned for 1 qubits",
            ],
            "{bracketing}"
        );
    }
}

/// Two adapters whose offers narrow from 2 instructions a batch to 1 once
/// provisioned, which their rule forbids, over bits, under a program
/// sending batches of 2, in each of the five bracketings: every boundary
/// checks each batch against the offer read before any part was
/// provisioned, and the upper adapter is told that offer, so the run
/// succeeds alike.
#[test]
fn every_boundary_answers_the_offer_read_before_any_part_is_provisioned() {
    fn program() -> Driver<Program> {
        Driver::new(Program::new(&[("x", 0), ("mz", 0)]))
    }
    /// The offers each `Shifting` is told over two runs.
    fn told<D: Drive, E: Execute>(
        composed: Result<Runner<D, E>, CompositionFaults>,
        log: &Log,
    ) -> Vec<String> {
        logged_over_two_runs(composed, log, "is offered")
    }
    let chains: [(&str, Chain); 5] = [
        ("left", |log| {
            let shifting = || Adapter::new(Shifting::new(log));
            told(
                ((program() >> shifting()) >> shifting()) >> Executor::new(Bits::new(log)),
                log,
            )
        }),
        ("right", |log| {
            let shifting = || Adapter::new(Shifting::new(log));
            told(
                program() >> (shifting() >> (shifting() >> Executor::new(Bits::new(log)))),
                log,
            )
        }),
        ("middle", |log| {
            let shifting = || Adapter::new(Shifting::new(log));
            told(
                (program() >> shifting()) >> (shifting() >> Executor::new(Bits::new(log))),
                log,
            )
        }),
        ("adapters joined", |log| {
            let shifting = || Adapter::new(Shifting::new(log));
            told(
                program() >> ((shifting() >> shifting()) >> Executor::new(Bits::new(log))),
                log,
            )
        }),
        ("adapters joined to the driver", |log| {
            let shifting = || Adapter::new(Shifting::new(log));
            told(
                (program() >> (shifting() >> shifting())) >> Executor::new(Bits::new(log)),
                log,
            )
        }),
    ];
    for (bracketing, chain) in chains {
        assert_eq!(
            chain(&Log::default()),
            [
                "Shifting is offered Some(2) instructions a batch",
                "Shifting is offered None instructions a batch",
            ]
            .repeat(2),
            "{bracketing}"
        );
    }
}
