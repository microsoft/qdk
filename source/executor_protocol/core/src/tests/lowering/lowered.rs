// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Lowered`]: a lowering run as an adapter, on a driver of `WithToffoli`
//! over an executor of `WithoutToffoli`: what it sends below, the readouts
//! and conditions it reads back, a checked batch it lowers, what it
//! requires below, and the memory it reuses.

use std::any::type_name;
use std::cell::Cell;
use std::future::poll_fn;
use std::pin::pin;
use std::rc::Rc;
use std::task::Poll;

use crate::prelude::*;
use crate::tests::allocations;
use crate::tests::example_sets::{BitReadout, ShotFlags};
use crate::tests::example_sets::{
    CliffordT, Qubits, ToffoliGate, TooFewQubits, WithToffoli, WithoutToffoli,
};
use crate::tests::fake_parts::{CliffordTRegister, Log, ReadThrice, ToffoliFree, ToffoliProgram};
use crate::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, Lowered, PartId, Readouts,
    RunError, RunRefusal, RunRequirement, Seed, ShotSet,
};

use BitReadout::{One, Zero};
use ToffoliGate::{Ccz, H, Mz};

/// The outputs of the run of shots `shots` of `program`, lowered by
/// `lowering`, over a register of 3 qubits writing `reading` into each
/// row; and the register's log.
fn run<L: Lowering<From = WithToffoli, To = WithoutToffoli> + 'static>(
    program: ToffoliProgram,
    lowering: L,
    reading: &[BitReadout],
    shots: &ShotSet,
) -> (Vec<(Vec<BitReadout>, ShotFlags)>, Log) {
    let log = Log::default();
    let Ok(mut runner) = program
        .through(Lowered::new(lowering))
        .on(CliffordTRegister::new(3, reading, &log))
    else {
        panic!("the chain composes");
    };
    let outputs = runner.run(shots, 7).expect("a run");
    (outputs, log)
}

/// A program of an `h`, a `ccz` and an `mz`, through a lowering rewriting
/// `ccz` as seven `t` and `tdg` and six `cx`: the register receives each
/// instruction's lowering, in order, in one batch.
#[test]
fn each_instruction_runs_below_as_its_lowering() {
    let program = ToffoliProgram::new(&[H(0), Ccz(0, 1, 2), Mz(2)], 3);
    let (_, log) = run(program, ToffoliFree::new(), &[Zero], &ShotSet::ids([0]));
    let mut lowered = vec![CliffordT::H(0)];
    lowered.extend(ToffoliFree::ccz(0, 1, 2));
    lowered.push(CliffordT::Mz(2));
    assert_eq!(log.entries(), [format!("runs {lowered:?}, checked: false")]);
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
fn a_lowering_reads_each_instruction_s_readouts() {
    let copied = run(
        ToffoliProgram::new(&[Mz(0), Ccz(0, 1, 2), Mz(1)], 3),
        ToffoliFree::new(),
        &[One, Zero],
        &ShotSet::ids([0]),
    );
    assert_eq!(copied.0[0].0, [One, Zero], "copied");
    let majority = run(
        ToffoliProgram::new(&[Mz(0), H(1), Mz(1)], 3),
        ReadThrice,
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
    let Ok(mut runner) = ToffoliBatches(vec![first_round.to_vec(), second_round.to_vec()])
        .through(Lowered::new(ReadThrice))
        .on(CliffordTRegister::with_readings(
            3,
            &[&[One, One, Zero, Zero, Zero, One], &[Zero, Zero, One]],
            &log,
        ))
    else {
        panic!("the chain composes");
    };
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

impl Driver for ToffoliBatches {
    type Sends = WithToffoli;
    type Output = Vec<(Vec<BitReadout>, ShotFlags)>;

    fn sends(&self) -> &WithToffoli {
        &WithToffoli
    }

    fn requires(&self, run: &RunRequirement) -> Capabilities<WithToffoli> {
        Capabilities::new(run.resources, run.features, Qubits(3))
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
                .map(|(_, positions)| shots.select(positions).expect("shots of the run"))
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
        ReadThrice,
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
            ReadThrice,
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

/// A lowering requires below what it states, given what arrives, which is
/// fit against the executor at `on`: a decomposition using 2 ancillas on a
/// program of 3 qubits requires 5, so a register of 4 is refused, naming
/// the lowering's adapter, with the set's own mismatch; a register of 5
/// runs it.
#[test]
fn what_a_lowering_requires_below_is_fit_at_on() {
    let program = || ToffoliProgram::new(&[Ccz(0, 1, 2)], 3);
    let log = Log::default();
    let faults = program()
        .through(Lowered::new(ToffoliFree::with_ancillas(2)))
        .on(CliffordTRegister::new(4, &[], &log))
        .err()
        .expect("4 qubits are short of 5");
    let [fault] = faults.faults() else {
        panic!("one fault: {faults}");
    };
    assert_eq!(
        fault.requirer,
        PartId::new(1, type_name::<Lowered<ToffoliFree>>())
    );
    let mismatch = fault.mismatch.capabilities.set_capabilities.as_ref();
    assert_eq!(
        mismatch.and_then(|mismatch| mismatch.downcast_ref()),
        Some(&TooFewQubits {
            required: 5,
            provided: 4
        })
    );
    let Ok(mut runner) = program()
        .through(Lowered::new(ToffoliFree::with_ancillas(2)))
        .on(CliffordTRegister::new(5, &[], &log))
    else {
        panic!("5 qubits of 5 compose");
    };
    runner.run(&ShotSet::ids([0]), 7).expect("5 qubits of 5");
}

/// A lowering limits what arrives from above as an adapter does, fit at
/// `on`: a program of 3 qubits through a lowering taking at most 2 is
/// refused, the lowering's adapter named as the provider, with the set's
/// own mismatch; through one taking 3, it runs.
#[test]
fn what_a_lowering_provides_is_fit_at_on() {
    let program = || ToffoliProgram::new(&[Ccz(0, 1, 2)], 3);
    let log = Log::default();
    let faults = program()
        .through(Lowered::new(ToffoliFree::for_qubits(2)))
        .on(CliffordTRegister::new(5, &[], &log))
        .err()
        .expect("3 qubits are more than 2");
    let [fault] = faults.faults() else {
        panic!("one fault: {faults}");
    };
    assert_eq!(
        fault.provider,
        PartId::new(1, type_name::<Lowered<ToffoliFree>>())
    );
    let mismatch = fault.mismatch.capabilities.set_capabilities.as_ref();
    assert_eq!(
        mismatch.and_then(|mismatch| mismatch.downcast_ref()),
        Some(&TooFewQubits {
            required: 3,
            provided: 2
        })
    );
    let Ok(mut runner) = program()
        .through(Lowered::new(ToffoliFree::for_qubits(3)))
        .on(CliffordTRegister::new(5, &[], &log))
    else {
        panic!("3 qubits of 3 compose");
    };
    runner.run(&ShotSet::ids([0]), 7).expect("3 qubits of 3");
}

/// A lowering gets ready on each run as an adapter does: one not ready,
/// while calibrating, refuses the run, its adapter named, and nothing is
/// sent below.
#[test]
fn a_lowering_not_ready_refuses_the_run() {
    let log = Log::default();
    let Ok(mut runner) = ToffoliProgram::new(&[Mz(0)], 3)
        .through(Lowered::new(ToffoliFree::not_ready("calibrating")))
        .on(CliffordTRegister::new(3, &[Zero], &log))
    else {
        panic!("the chain composes");
    };
    let Err(RunError::Refused(RunRefusal::PartNotReady { part, not_ready })) =
        runner.run(&ShotSet::ids([0]), 7)
    else {
        panic!("a lowering not ready refuses the run");
    };
    assert_eq!(part, PartId::new(1, type_name::<Lowered<ToffoliFree>>()));
    assert_eq!(not_ready.why().to_string(), "calibrating");
    assert!(
        log.entries().iter().all(|entry| !entry.starts_with("runs")),
        "{:?}",
        log.entries()
    );
}

/// A driver sending a round at a time, a batch of `gates` per shot, in one
/// `execute`, into the readouts it keeps; it notes the allocations of its
/// second round.
struct RoundsOfToffoli {
    gates: Vec<ToffoliGate>,
    allocated: Rc<Cell<Option<usize>>>,
}

impl Driver for RoundsOfToffoli {
    type Sends = WithToffoli;
    type Output = ();

    fn sends(&self) -> &WithToffoli {
        &WithToffoli
    }

    fn bind(&mut self, (): ()) {}

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = WithToffoli>,
    ) -> Result<Vec<()>, DriveError> {
        let readout_slots = self
            .gates
            .iter()
            .map(|gate| WithToffoli.readout_slots(gate))
            .sum();
        let each = (0..shots.len())
            .map(|at| shots.select(&[at]).expect("a shot of the run"))
            .collect::<Vec<_>>();
        let batches = each
            .iter()
            .map(|shot| Batch::new(shot, &self.gates, readout_slots))
            .collect::<Vec<_>>();
        let mut kept = Readouts::new();
        below
            .execute(&batches, seed, kept.readouts_for(&batches))
            .await?;
        let mut execute = pin!(below.execute(&batches, seed, kept.readouts_for(&batches)));
        let (count, answered) = poll_fn(|context| {
            let mut answered = Poll::Pending;
            let count = allocations(|| answered = execute.as_mut().poll(context));
            Poll::Ready((count, answered))
        })
        .await;
        let Poll::Ready(ran) = answered else {
            panic!("the `execute` answers at its first poll");
        };
        ran?;
        self.allocated.set(Some(count));
        Ok(vec![(); shots.len()])
    }
}

/// An executor of [`WithoutToffoli`] writing `Zero` in every slot of
/// every row, and allocating nothing.
struct Zeros;

impl Executor for Zeros {
    type Accepts = WithoutToffoli;

    fn accepts(&self) -> &WithoutToffoli {
        &WithoutToffoli
    }

    async fn execute(
        &mut self,
        _batches: &[Batch<'_, WithoutToffoli>],
        _seed: Seed,
        readouts: &mut [BatchReadouts<WithoutToffoli>],
    ) -> Result<(), ExecuteError> {
        for batch_readouts in readouts {
            for position in 0..batch_readouts.shots() {
                batch_readouts.row_mut(position).fill(Zero);
            }
        }
        Ok(())
    }
}

/// A driver sending rounds of five batches, each a `ccz` and an `mz`,
/// through a lowering rewriting `ccz` as thirteen gates and reading `mz`
/// three times: the second round, of the size of the first, allocates
/// only the list of the batches the lowering sends below.
#[test]
fn a_lowering_reuses_its_memory_from_one_round_to_the_next() {
    let allocated = Rc::new(Cell::new(None));
    let Ok(mut runner) = (RoundsOfToffoli {
        gates: vec![Ccz(0, 1, 2), Mz(2)],
        allocated: Rc::clone(&allocated),
    })
    .through(Lowered::new(ReadThrice))
    .on(Zeros) else {
        panic!("the chain composes");
    };
    runner.run(&ShotSet::ids(0..5), 7).expect("a run");
    assert_eq!(allocated.get(), Some(1));
}
