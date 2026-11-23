// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Lowered`]: a lowering run as an adapter, on a driver of `WithToffoli`
//! over an executor of `WithoutToffoli`: what it sends below, the readouts
//! and conditions it reads back, a checked batch it lowers, a refusal from
//! below it passes up, what it requires below, and the memory it reuses.

use std::any::type_name;
use std::cell::Cell;
use std::rc::Rc;

use crate::prelude::*;
use crate::tests::example_sets::{BitReadout, NoCapabilities, ShotFlags, Wide};
use crate::tests::example_sets::{
    CliffordT, QubitBeyond, Qubits, ToffoliGate, TooFewQubits, WithToffoli, WithoutToffoli,
};
use crate::tests::fake_parts::{
    CliffordTPassThrough, CliffordTRegister, Log, ReadThrice, RoundsIntoItsReadouts, ToffoliFree,
    ToffoliProgram, Zeros, in_each_bracketing,
};
use crate::tests::runner;
use crate::{
    Batch, BatchFault, Capabilities, CompositionFaults, DriveError, ExecuteRefusal, Failure,
    Lowered, PartId, Readouts, ResourceMismatch, RunError, RunRefusal, RunRequirement, Runner,
    Seed, ShotId, ShotSet, StatesLimits,
};

use BitReadout::{One, Zero};
use ToffoliGate::{Ccz, H, Mz};

/// The outputs of the run of shots `shots` of `program`, lowered by
/// `lowering`, over a register of 3 qubits writing `reading` into each
/// row; and the register's log.
fn run<L: Lowering<From = WithToffoli, To = WithoutToffoli> + 'static>(
    program: StatesLimits<ToffoliProgram>,
    lowering: L,
    reading: &[BitReadout],
    shots: &ShotSet,
) -> (Vec<(Vec<BitReadout>, ShotFlags)>, Log) {
    let log = Log::default();
    let mut runner = runner(
        program
            .through(Lowered::new(lowering))
            .on(CliffordTRegister::new(3, reading, &log)),
    );
    let outputs = runner.run(shots, 7).expect("a run");
    (outputs, log)
}

/// What `log` saw of a run of `shots` on the chain `composed` gave, which
/// must compose.
fn received<D: Driver, E: Executor<Accepts = D::Sends>>(
    composed: Result<Runner<D, E>, CompositionFaults>,
    shots: &ShotSet,
    log: &Log,
) -> Vec<String> {
    let mut runner = runner(composed);
    runner.run(shots, 7).expect("a run");
    log.entries()
}

/// A program of an `h`, a `ccz` and an `mz`, through a lowering rewriting
/// `ccz` as seven `t` and `tdg` and six `cx` with 2 ancillas, over a
/// register of 5 qubits: the register receives each instruction's
/// lowering, in order, in one batch. So it does wherever the brackets go,
/// with an adapter of the lowered set below the lowering or none, and with
/// the lowering lent by `&mut`.
#[test]
fn each_instruction_runs_below_as_its_lowering() {
    let program = || ToffoliProgram::new(&[H(0), Ccz(0, 1, 2), Mz(2)], 3);
    let with_ancillas = || Lowered::new(ToffoliFree::with_ancillas(2));
    let register = |log: &Log| CliffordTRegister::new(5, &[Zero], log);
    let shots = ShotSet::ids([0]);
    let mut lowered = vec![CliffordT::H(0)];
    lowered.extend(ToffoliFree::ccz(0, 1, 2));
    lowered.push(CliffordT::Mz(2));
    let expected = [format!("runs {lowered:?}, checked: false")];
    in_each_bracketing!([d, lowering, e] from {
        let log = Log::default();
        let d = program();
        let lowering = with_ancillas();
        let e = register(&log);
    }, |chain, brackets| assert_eq!(received(chain, &shots, &log), expected, "{brackets}"));
    in_each_bracketing!([d, lowering, a, e] from {
        let log = Log::default();
        let d = program();
        let lowering = with_ancillas();
        let a = CliffordTPassThrough;
        let e = register(&log);
    }, |chain, brackets| assert_eq!(received(chain, &shots, &log), expected, "{brackets}"));
    let mut lent = with_ancillas();
    let log = Log::default();
    assert_eq!(
        received(program().on((&mut lent).on(register(&log))), &shots, &log),
        expected,
        "on a lent lowering"
    );
    let log = Log::default();
    let lent_through = (&mut lent).through(CliffordTPassThrough);
    assert_eq!(
        received(
            program().through(lent_through).on(register(&log)),
            &shots,
            &log
        ),
        expected,
        "through a lent lowering"
    );
}

/// Each instruction's readouts are read back from those of its lowering,
/// wherever they lie in the row below: copied one for one, for a lowering
/// that copies them, `mz` lowered as `mz`, past a `ccz` that reads none;
/// and as the lowering reads them, for `mz` read three times, its readout
/// their majority. A row copied unread, or read at another instruction's
/// slots, fails. So it is in each batch of an `execute` of several, of
/// different shots and instructions, and in a second round of fewer
/// batches and instructions: a batch lowered in another's place, read
/// from another's rows, or read at slots kept from the first round, fails.
#[test]
fn a_lowering_reads_each_instructions_readouts() {
    let copied = run(
        ToffoliProgram::new(&[Mz(0), Ccz(0, 1, 2), Mz(1)], 3),
        ToffoliFree::new(),
        &[One, Zero],
        &ShotSet::ids([0]),
    );
    assert_eq!(copied.0[0].0, [One, Zero], "copied");
    let majority = run(
        ToffoliProgram::new(&[Mz(0), H(1), Mz(1)], 3),
        ReadThrice::new(),
        &[Zero, One, One, One, Zero, Zero],
        &ShotSet::ids([0]),
    );
    assert_eq!(majority.0[0].0, [One, Zero], "the majority of each three");

    let first_round = [
        (vec![Mz(0), Mz(1)], vec![0, 2]),
        (vec![H(0), Ccz(0, 1, 2), Mz(2)], vec![1, 3]),
    ];
    let second_round = [(vec![Mz(2)], vec![0, 1, 2, 3])];
    let log = Log::default();
    let mut runner = runner(
        ToffoliBatches(vec![first_round.to_vec(), second_round.to_vec()])
            .stating_limits()
            .through(Lowered::new(ReadThrice::new()))
            .on(CliffordTRegister::with_readings(
                3,
                &[&[One, One, Zero, Zero, Zero, One], &[Zero, Zero, One]],
                &log,
            )),
    );
    let outputs = runner.run(&ShotSet::ids(0..4), 7).expect("a run");
    assert_eq!(
        outputs,
        [
            vec![
                (vec![One, Zero], ShotFlags::NONE),
                (vec![One], ShotFlags::NONE)
            ],
            vec![
                (vec![Zero], ShotFlags::NONE),
                (vec![One], ShotFlags::REJECTED)
            ],
            vec![
                (vec![One, Zero], ShotFlags::REJECTED),
                (vec![One], ShotFlags::NONE)
            ],
            vec![
                (vec![Zero], ShotFlags::REJECTED),
                (vec![One], ShotFlags::REJECTED)
            ],
        ],
        "each shot's readouts and conditions in each round"
    );
    let mut second = vec![CliffordT::H(0)];
    second.extend(ToffoliFree::ccz(0, 1, 2));
    second.extend([CliffordT::Mz(2); 3]);
    let sent = [
        [[CliffordT::Mz(0); 3], [CliffordT::Mz(1); 3]].concat(),
        second,
        vec![CliffordT::Mz(2); 3],
    ];
    assert_eq!(
        log.entries(),
        sent.map(|lowered| format!("runs {lowered:?}, checked: false"))
    );
}

/// A driver of 3 qubits sending its rounds in turn, each in one `execute`
/// of a batch per entry: its gates, to the shots at its positions in the
/// run's. Each shot's output is its readouts and conditions in each round.
struct ToffoliBatches(Vec<Vec<(Vec<ToffoliGate>, Vec<usize>)>>);

impl DriverRequirements for ToffoliBatches {
    fn requires(&self, run: &RunRequirement) -> Capabilities<WithToffoli> {
        Capabilities::new(run.resources, run.features, Qubits(3))
    }
}

impl Driver for ToffoliBatches {
    type Sends = WithToffoli;
    type Output = Vec<(Vec<BitReadout>, ShotFlags)>;

    fn sends(&self) -> &WithToffoli {
        &WithToffoli
    }

    fn bind(&mut self, (): ()) {}

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = WithToffoli>,
    ) -> Result<Vec<Self::Output>, DriveError> {
        let mut outputs = vec![Vec::new(); shots.len()];
        let mut readouts = Readouts::new();
        for round in &self.0 {
            let selected = round
                .iter()
                .map(|(_, positions)| shots.subset(positions).expect("shots of the run"))
                .collect::<Vec<_>>();
            let batches = round
                .iter()
                .zip(&selected)
                .map(|((gates, _), selected)| {
                    let readout_slots = gates.iter().map(|gate| WithToffoli.readout_slots(gate));
                    Batch::new(selected, gates, readout_slots.sum())
                })
                .collect::<Vec<_>>();
            let batch_readouts = readouts.readouts_for(&batches);
            below.execute(&batches, seed, batch_readouts).await?;
            for ((_, positions), batch) in round.iter().zip(batch_readouts.iter()) {
                for (shot, &position) in positions.iter().enumerate() {
                    let row = batch.row(shot).to_vec();
                    outputs[position].push((row, batch.conditions(shot)));
                }
            }
        }
        Ok(outputs)
    }
}

/// Each shot keeps the conditions the part below gave it once its
/// readouts are read back: of three shots, the second, rejected below, is
/// rejected above, and the others are not.
#[test]
fn each_shot_keeps_its_conditions_once_lowered() {
    let (outputs, _) = run(
        ToffoliProgram::new(&[Mz(0)], 3),
        ReadThrice::new(),
        &[One; 3],
        &ShotSet::ids([4, 7, 2]),
    );
    let conditions: Vec<_> = outputs.iter().map(|(_, conditions)| *conditions).collect();
    assert_eq!(
        conditions,
        [ShotFlags::NONE, ShotFlags::REJECTED, ShotFlags::NONE]
    );
}

/// A lowered batch is sent below checked if the batch it lowers arrived
/// checked, stating the readout slots of its lowering, which the boundary
/// below checks: a checked `mz` read three times runs; an unchecked one is
/// sent unchecked.
#[test]
fn a_lowered_batch_is_checked_as_the_batch_it_lowers() {
    for checked in [true, false] {
        let (outputs, log) = run(
            ToffoliProgram::new(&[H(0), Mz(0)], 3).checked_if(checked),
            ReadThrice::new(),
            &[One; 3],
            &ShotSet::ids([0]),
        );
        assert_eq!(outputs[0].0, [One], "checked: {checked}");
        let sent = log.entries();
        assert!(
            sent[0].ends_with(&format!("checked: {checked}")),
            "{sent:?}"
        );
    }
}

/// Which part a fault at `on` names, and as what.
enum Named<'p> {
    Requirer(&'p PartId),
    Provider(&'p PartId),
}

/// A lowering states what it requires below, given what arrives, and
/// limits what arrives from above, as an adapter does, both fit at `on`
/// against its neighbours, with the set's own mismatch where qubits fall
/// short. Below, a fault names the part that first required the value: a
/// decomposition using 2 ancillas on a program of 3 qubits requires 5, so
/// a register of 4 is refused, naming the lowering's adapter; a batch of
/// one instruction, whose `ccz` is lowered as 13, requires 13 a batch, so
/// a register taking 12 is refused, naming the lowering's adapter; a
/// decomposition using no ancilla passes the program's 3 qubits below
/// unchanged, so a register of 2 is refused naming the program. Above, a
/// program of 3 qubits through a lowering taking at most 2 is refused, the
/// lowering's adapter named as the provider. A register of 5 qubits,
/// taking 13 instructions a batch, runs each lowered `ccz`, and a lowering
/// taking 3 qubits runs the program.
#[test]
fn what_a_lowering_states_is_fit_at_on() {
    let program = || ToffoliProgram::new(&[Ccz(0, 1, 2)], 3);
    let lowering = PartId::new(1, type_name::<Lowered<ToffoliFree>>());
    let driver = PartId::new(0, type_name::<ToffoliProgram>());
    let log = Log::default();
    let cases = [
        (
            "2 ancillas, on 4 qubits",
            ToffoliFree::with_ancillas(2),
            CliffordTRegister::new(4, &[], &log),
            Named::Requirer(&lowering),
            vec![],
            Some(TooFewQubits {
                required: 5,
                provided: 4,
            }),
        ),
        (
            "a ccz lowered as 13, on 12 a batch",
            ToffoliFree::with_ancillas(2),
            CliffordTRegister::new(5, &[], &log).taking(12),
            Named::Requirer(&lowering),
            vec![ResourceMismatch::InstructionsPerBatch {
                required: 13,
                provided: 12,
            }],
            None,
        ),
        (
            "no ancilla, on 2 qubits",
            ToffoliFree::with_ancillas(0),
            CliffordTRegister::new(2, &[], &log),
            Named::Requirer(&driver),
            vec![],
            Some(TooFewQubits {
                required: 3,
                provided: 2,
            }),
        ),
        (
            "3 qubits, through a lowering taking 2",
            ToffoliFree::for_qubits(2),
            CliffordTRegister::new(5, &[], &log),
            Named::Provider(&lowering),
            vec![],
            Some(TooFewQubits {
                required: 3,
                provided: 2,
            }),
        ),
    ];
    for (case, lowered, register, named, resources, set_capabilities) in cases {
        let faults = program()
            .through(Lowered::new(lowered))
            .on(register)
            .err()
            .unwrap_or_else(|| panic!("{case}: refused"));
        let [fault] = faults.faults() else {
            panic!("{case}: one fault: {faults}");
        };
        match named {
            Named::Requirer(part) => assert_eq!(&fault.requirer, part, "{case}"),
            Named::Provider(part) => assert_eq!(&fault.provider, part, "{case}"),
        }
        let mismatch = &fault.mismatch.capabilities;
        assert_eq!(mismatch.resources, resources, "{case}");
        let set = mismatch.set_capabilities.as_ref();
        assert_eq!(
            set.and_then(|mismatch| mismatch.downcast_ref()),
            set_capabilities.as_ref(),
            "{case}"
        );
    }
    for (case, lowered, register) in [
        (
            "5 qubits of 5, 13 instructions a batch of 13",
            ToffoliFree::with_ancillas(2),
            CliffordTRegister::new(5, &[], &log).taking(13),
        ),
        (
            "3 qubits of 3",
            ToffoliFree::for_qubits(3),
            CliffordTRegister::new(5, &[], &log),
        ),
    ] {
        let Ok(mut runner) = program().through(Lowered::new(lowered)).on(register) else {
            panic!("{case} compose");
        };
        runner
            .run(&ShotSet::ids([0]), 7)
            .unwrap_or_else(|error| panic!("{case}: {error:?}"));
    }
}

/// A lowering gets ready on each run as an adapter does: one not ready,
/// while calibrating, refuses the run, its adapter named, and nothing is
/// sent below.
#[test]
fn a_lowering_not_ready_refuses_the_run() {
    let log = Log::default();
    let mut runner = runner(
        ToffoliProgram::new(&[Mz(0)], 3)
            .through(Lowered::new(ToffoliFree::not_ready("calibrating")))
            .on(CliffordTRegister::new(3, &[Zero], &log)),
    );
    let Err(RunError::Refused(RunRefusal::PartNotReady { part, not_ready })) =
        runner.run(&ShotSet::ids([0]), 7)
    else {
        panic!("a lowering not ready refuses the run");
    };
    assert_eq!(part, PartId::new(1, type_name::<Lowered<ToffoliFree>>()));
    assert_eq!(not_ready.why().to_string(), "calibrating");
    assert!(
        log.entries_starting_with("runs").is_empty(),
        "{:?}",
        log.entries()
    );
}

/// A refusal from below reaches the driver unchanged, in the terms of the
/// set below: a checked program of 3 qubits, an `mz` then an `h`, through
/// a lowering reading `mz` three times and writing the `h` on qubit 4, a
/// qubit it does not require, is refused below at position 3 of the
/// lowered batch, where the `h` is at position 1 of the batch above, with
/// the set below's fault, not as a failure. So is a checked program of two
/// `mz`, through a lowering writing each as one instruction of 2^31
/// readout slots: refused below at the second, where their slots no longer
/// fit in a `u32`, not by a panic of the lowering's adapter.
#[test]
fn a_refusal_below_reaches_the_driver_unchanged() {
    let log = Log::default();
    let error = runner(
        ToffoliProgram::new(&[Mz(0), H(1)], 3)
            .checked_if(true)
            .through(Lowered::new(ReadThrice::using_unstated_qubits()))
            .on(CliffordTRegister::new(5, &[Zero], &log)),
    )
    .run(&ShotSet::ids([0]), 7)
    .expect_err("the lowered `h` is beyond the qubits required");
    let RunError::Failed(DriveError {
        failure:
            Failure::Refused(ExecuteRefusal::Batch {
                shot,
                batch,
                fault: BatchFault::Instruction { at, fault },
            }),
        ..
    }) = error
    else {
        panic!("a batch refused below: {error:?}");
    };
    assert_eq!((shot, batch, at), (ShotId::new(0), 0, 3), "{fault}");
    assert_eq!(
        fault.downcast_ref::<QubitBeyond>(),
        Some(&QubitBeyond {
            qubit: 4,
            qubits: 3
        })
    );

    let error = runner(
        ToffoliProgram::new(&[Mz(0), Mz(1)], 3)
            .checked_if(true)
            .through(Lowered::new(MeasuresHalfOfAU32))
            .on(Zeros::new(Wide)),
    )
    .run(&ShotSet::ids([0]), 7)
    .expect_err("2^31 readout slots twice do not fit in a `u32`");
    let RunError::Failed(DriveError {
        failure:
            Failure::Refused(ExecuteRefusal::Batch {
                shot,
                batch,
                fault: BatchFault::TooManyReadoutSlots { at },
            }),
        ..
    }) = error
    else {
        panic!("a batch refused below for its readout slots: {error:?}");
    };
    assert_eq!((shot, batch, at), (ShotId::new(0), 0, 1));
}

/// Each `mz` written as one [`Wide`] instruction of 2^31 readout slots,
/// every other gate as none: a lowering whose readout slots below pass a
/// `u32` from a batch's second `mz`. It requires below what arrives.
struct MeasuresHalfOfAU32;

impl Lowering for MeasuresHalfOfAU32 {
    type From = WithToffoli;
    type To = Wide;

    fn accepts(&self) -> &WithToffoli {
        &WithToffoli
    }

    fn sends(&self) -> &Wide {
        &Wide
    }

    fn requires(&self, above: &Capabilities<WithToffoli>) -> Capabilities<Wide> {
        Capabilities::new(*above.resources(), *above.features(), NoCapabilities)
    }

    fn bind(&mut self, (): ()) {}

    fn lower(&self, gate: &ToffoliGate, into: &mut Vec<u32>) {
        if let Mz(_) = gate {
            into.push(1 << 31);
        }
    }

    fn read(&self, _gate: &ToffoliGate, _lowered: &[BitReadout], into: &mut [BitReadout]) {
        into.fill(Zero);
    }

    fn read_conditions(&self, lowered: ShotFlags) -> ShotFlags {
        lowered
    }
}

/// A driver sending rounds of five batches, each a `ccz` and an `mz`,
/// through a lowering rewriting `ccz` as thirteen gates and reading `mz`
/// three times: the second round, of the size of the first, allocates
/// only the list of the batches the lowering sends below.
#[test]
fn a_lowering_reuses_its_memory_from_one_round_to_the_next() {
    let allocated = Rc::new(Cell::new(None));
    runner(
        RoundsIntoItsReadouts::new(&[Ccz(0, 1, 2), Mz(2)], &allocated)
            .through(Lowered::new(ReadThrice::new()))
            .on(Zeros::new(WithoutToffoli)),
    )
    .run(&ShotSet::ids(0..5), 7)
    .expect("a run");
    assert_eq!(allocated.get(), Some(1));
}
