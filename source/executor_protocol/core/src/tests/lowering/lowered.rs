// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Lowered`]: a lowering run as an adapter, on a driver of `WithToffoli`
//! over an executor of `WithoutToffoli`: what it sends below, the readouts
//! and conditions it reads back, a checked batch it lowers, and what it
//! requires below.

use std::any::type_name;

use crate::prelude::*;
use crate::tests::example_sets::{
    CliffordT, ToffoliGate, TooFewQubits, WithToffoli, WithoutToffoli,
};
use crate::tests::fake_parts::{CliffordTRegister, Log, ReadThrice, ToffoliFree, ToffoliProgram};
use crate::{Lowered, PartId, ReadoutValue, ShotConditions, ShotSet};

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
    let mismatch = fault.mismatch.capabilities.instructions.as_ref();
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
