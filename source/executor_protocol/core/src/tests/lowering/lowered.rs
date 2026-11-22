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
use crate::tests::example_sets::{
    CliffordT, ToffoliGate, TooFewQubits, WithToffoli, WithoutToffoli,
};
use crate::tests::fake_parts::{CliffordTRegister, Log, ReadThrice, ToffoliFree, ToffoliProgram};
use crate::{
    Batch, BatchReadouts, DriveError, ExecuteError, Lowered, PartId, ReadoutValue, Readouts, Seed,
    ShotConditions, ShotSet,
};

use ReadoutValue::{One, Zero};
use ToffoliGate::{Ccz, H, Mz};

/// The outputs of the run of shots `shots` of `program`, lowered by
/// `lowering`, over a register of 3 qubits writing `reading` into each
/// row; and the register's log.
fn run<L: Lowering<From = WithToffoli, To = WithoutToffoli> + 'static>(
    program: ToffoliProgram,
    lowering: L,
    reading: &[ReadoutValue],
    shots: &ShotSet,
) -> (Vec<(Vec<ReadoutValue>, ShotConditions)>, Log) {
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
/// wherever they lie in the row below: copied one for one by default, for
/// `mz` lowered as `mz`, past a `ccz` that reads none; and as the lowering
/// reads them, for `mz` read three times, its readout their majority. A
/// row copied unread, or read at another instruction's slots, fails.
#[test]
fn a_lowering_reads_each_instruction_s_readouts() {
    let copied = run(
        ToffoliProgram::new(&[Mz(0), Ccz(0, 1, 2), Mz(1)], 3),
        ToffoliFree::new(),
        &[One, Zero],
        &ShotSet::ids([0]),
    );
    assert_eq!(copied.0[0].0, [One, Zero], "copied by default");
    let majority = run(
        ToffoliProgram::new(&[Mz(0), H(1), Mz(1)], 3),
        ReadThrice,
        &[Zero, One, One, One, Zero, Zero],
        &ShotSet::ids([0]),
    );
    assert_eq!(majority.0[0].0, [One, Zero], "the majority of each three");
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
        [
            ShotConditions::NONE,
            ShotConditions::REJECTED,
            ShotConditions::NONE
        ]
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
        readouts: &mut [BatchReadouts],
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
