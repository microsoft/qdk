// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An executor that answers from a worker thread: the time of a call,
//! which includes copying its batches to the worker.

use block_instruction_set::BlockInstruction;
use executor_protocol_core::Resources;
use executor_protocol_core::prelude::*;

use crate::Size;
use crate::parts::{
    FromWorker, MixGates, Profile, Profiler, ProgramInBatches, run_once, slots_per_batch,
};
use crate::timing::{clock_floor, median_call};
use crate::workload::QUBITS;

/// The time of a call to an executor that answers from a worker thread,
/// beside the same call answered on this thread. The worker's call also
/// copies its batches, for the worker to own (a `Vec` of the instructions
/// and a clone of the shots per batch), which the call on this thread does
/// not, so the difference grows with the batch, beyond the handoff's cost
/// that batch 1 shows. Each call's time holds a clock read before and
/// after it, whose cost the bench prints, so the columns compare above
/// that floor (batch 1 is near it).
pub(crate) fn bench(size: &Size, program: &[BlockInstruction], checked: bool) {
    let repeats = size.repeats.min(5);
    println!(
        "\nworker: an executor answering from a worker thread, median call, µs, best of {repeats}"
    );
    println!(
        "  each call's time holds two clock reads, {:.2} µs here (an empty pair's median)",
        clock_floor()
    );
    println!(
        "  the worker's call copies its batches to the worker; the call on this thread does not"
    );
    println!(
        "  {:>7} {:>7} {:>14} {:>14}",
        "batch", "calls", "worker", "this thread"
    );
    for batch in [1, 100, 10_000, 100_000] {
        let program = &program[..program.len().min(batch * size.most_calls)];
        let expected = Profile::of(program, checked);
        let slots = slots_per_batch(&MixGates::set(), program, batch);
        let worker = median_call(repeats, || {
            let mut worker = FromWorker::spawn();
            let mut in_batches =
                ProgramInBatches::new(MixGates::set(), program, &slots, QUBITS, batch)
                    .checked_if(checked);
            in_batches.timed = true;
            let received = run_once(in_batches.stating_limits().on(&mut worker));
            assert_eq!(worker.finish(), expected, "the worker counted the program");
            received.call_times
        });
        let here = median_call(repeats, || {
            let mut profiler = Profiler::new(Resources::unconstrained());
            let mut in_batches =
                ProgramInBatches::new(MixGates::set(), program, &slots, QUBITS, batch)
                    .checked_if(checked);
            in_batches.timed = true;
            let received = run_once(in_batches.stating_limits().on(&mut profiler));
            assert_eq!(
                profiler.part.profile, expected,
                "the profiler counted the program"
            );
            received.call_times
        });
        let calls = program.len().div_ceil(batch);
        println!("  {batch:>7} {calls:>7} {worker:>14.2} {here:>14.2}");
    }
}
