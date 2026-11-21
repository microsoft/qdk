// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A lowering adapter, each logical instruction sent as three physical:
//! its cost per physical instruction, against the physical program sent
//! directly.

use std::hint::black_box;
use std::time::Instant;

use executor_protocol::prelude::*;
use executor_protocol::{Instruction, ReadoutValue};

use crate::Size;
use crate::parts::{
    BatchKinds, Direct, LogicalGates, PhysicalGates, ProgramInBatches, Repetition3, Sink, majority,
    run_once, slots_per_batch,
};
use crate::timing::{best, per};

/// A lowering adapter's cost per physical instruction, against the
/// physical program sent directly to the same executor.
pub(crate) fn bench(size: &Size, checked: bool) {
    let logical_gate = |mnemonic: &str, qubits: &[u32]| {
        LogicalGates::set()
            .instruction(mnemonic, qubits.iter().copied(), &[])
            .expect("an instruction of LogicalGates")
    };
    // A logical round: two qubits prepared, one flipped, copied, both read.
    let round = [
        logical_gate("prepare_z", &[0]),
        logical_gate("prepare_z", &[1]),
        logical_gate("x", &[0]),
        logical_gate("cx", &[0, 1]),
        logical_gate("measure_z", &[0]),
        logical_gate("measure_z", &[1]),
    ];
    let logical: Vec<Instruction> = round
        .iter()
        .cycle()
        .take(6 * size.rounds)
        .cloned()
        .collect();
    let physical = lowered(&logical, checked);
    let by_six = slots_per_batch(&LogicalGates::set(), &logical, 6);
    let compose = compose_time(size.repeats, || {
        (
            ProgramInBatches::new(LogicalGates::set(), &logical, &by_six, 2, 6),
            Repetition3::new(2),
            Sink::new(),
        )
    });
    println!(
        "\nlowering: repetition code of 3 over a sink, {} logical -> {} physical instructions; composing the chain {:.1} µs",
        logical.len(),
        physical.len(),
        compose * 1e6
    );
    println!(
        "  {:>12} {:>16} {:>16} {:>16} {:>22}",
        "logical/call", "physical direct", "physical ≫sink", "lowered", "overhead per physical"
    );
    let physical_by_six = slots_per_batch(&PhysicalGates::set(), &physical, 3 * 6);
    let direct_answer = sink_direct(&physical, &physical_by_six, 3 * 6, checked);
    assert_eq!(
        direct_answer.1,
        BatchKinds::only(checked),
        "the sink got batches checked as the flag asks"
    );
    let lowered_answer = (
        direct_answer.0,
        direct_answer.1,
        majority_of_threes(&direct_answer.2),
    );
    let n = physical.len();
    for batch in [6, 60, 600, 6_000] {
        let p_slots = slots_per_batch(&PhysicalGates::set(), &physical, 3 * batch);
        let l_slots = slots_per_batch(&LogicalGates::set(), &logical, batch);
        let direct = best(size.repeats, &direct_answer, || {
            sink_direct(&physical, &p_slots, 3 * batch, checked)
        });
        let bare = best(size.repeats, &direct_answer, || {
            let mut sink = Sink::new();
            let received = run_once(
                (ProgramInBatches::new(PhysicalGates::set(), &physical, &p_slots, 6, 3 * batch)
                    .checked_if(checked))
                .on(&mut sink),
            );
            (sink.seen, sink.kinds, received.values)
        });
        let lowered = best(size.repeats, &lowered_answer, || {
            let mut sink = Sink::new();
            let received = run_once(
                (ProgramInBatches::new(LogicalGates::set(), &logical, &l_slots, 2, batch)
                    .checked_if(checked))
                .through(Repetition3::new(2))
                .on(&mut sink),
            );
            (sink.seen, sink.kinds, received.values)
        });
        println!(
            "  {batch:>12} {:>13.2} ns {:>13.2} ns {:>13.2} ns {:>19.2} ns",
            per(direct, n),
            per(bare, n),
            per(lowered, n),
            per(lowered - direct, n)
        );
    }
}

/// The physical program the lowering sends for `logical`, sent whole, each
/// batch checked if `checked`: the baseline's program.
fn lowered(logical: &[Instruction], checked: bool) -> Vec<Instruction> {
    let mut recorder = Sink::recording();
    let whole = slots_per_batch(&LogicalGates::set(), logical, logical.len());
    run_once(
        (ProgramInBatches::new(LogicalGates::set(), logical, &whole, 2, logical.len())
            .checked_if(checked))
        .through(Repetition3::new(2))
        .on(&mut recorder),
    );
    recorder.record.expect("a recording sink")
}

/// The best time, in seconds, of `repeats` compositions of the three parts
/// `parts` builds, after an untimed one: only the joins are timed, not
/// building the parts.
fn compose_time<'p>(
    repeats: usize,
    mut parts: impl FnMut() -> (ProgramInBatches<'p>, Repetition3, Sink),
) -> f64 {
    (0..=repeats)
        .map(|_| {
            let (driver, adapter, executor) = parts();
            let start = Instant::now();
            let chain = black_box(driver.through(adapter).on(executor));
            let elapsed = start.elapsed().as_secs_f64();
            chain.map(drop).expect("the chain composes");
            elapsed
        })
        .skip(1)
        .fold(f64::INFINITY, f64::min)
}

/// The physical program sent straight to a sink, `batch` per `execute`,
/// each [`checked`](executor_protocol::Batch::checked) if `checked`: the
/// instructions the sink saw, the kinds of batches it got, and its
/// readouts.
fn sink_direct(
    physical: &[Instruction],
    slots: &[u32],
    batch: usize,
    checked: bool,
) -> (u64, BatchKinds, Vec<ReadoutValue>) {
    let mut sink = Sink::new();
    let in_batches =
        ProgramInBatches::new(PhysicalGates::set(), physical, slots, 6, batch).checked_if(checked);
    // No boundary sees a batch: `Direct` sends past the chain.
    let received = run_once(Direct(in_batches, &mut sink).on(Sink::new()));
    (sink.seen, sink.kinds, received.values)
}

/// Each three values' majority, as the lowering reads a logical qubit.
fn majority_of_threes(values: &[ReadoutValue]) -> Vec<ReadoutValue> {
    values.chunks(3).map(majority).collect()
}
