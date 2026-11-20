// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the multi-shot form of `execute` costs per call, for a driver that
//! sends one shot at a time, such as a real-time one: the chains bench's
//! direct column, beside the same executor work called with one shot's
//! instructions and readouts alone, awaited or as a plain call.

use std::hint::black_box;

use executor_protocol::{Driver, Executor, Instruction, Offer, ReadoutValue};

use crate::Size;
use crate::parts::{
    Direct, MixGates, OneShotDirect, OneShotForm, Profile, Profiler, ProgramInBatches, run_once,
    slots_per_batch,
};
use crate::timing::{best, per, per_call};
use crate::workload::QUBITS;

/// Each calling form, per batch size, in ns per instruction, and the
/// differences between neighbouring forms per call. Batches are always
/// unchecked: the one-shot forms have no flag to check them.
pub(crate) fn bench(size: &Size, program: &[Instruction]) {
    println!(
        "\ncalling_form: one shot, the direct column's work behind three calling forms, ns/instr (difference per call), batches unchecked"
    );
    println!(
        "  today = execute(&[Batch], Seed, &mut [BatchReadouts]).await; one-shot = execute_one(&[Instruction], &mut [ReadoutValue]), awaited or a plain fn"
    );
    let expected = (Profile::of(program, false), Vec::new());
    println!(
        "  {:>7} {:>8} {:>20} {:>20}",
        "batch", "today", "one-shot async", "one-shot sync"
    );
    for batch in [1, 10, 100] {
        let slots = slots_per_batch(&MixGates::set(), program, batch);
        let time = |form| {
            best(size.repeats, &expected, || {
                run_in_form(program, &slots, batch, form)
            })
        };
        let today = time(Form::Today);
        let awaited = time(Form::OneShot(OneShotForm::Async));
        let plain = time(Form::OneShot(OneShotForm::Sync));
        let calls = program.len().div_ceil(batch);
        // Each one-shot column's difference per call is from the column on
        // its left: the multi-shot form, then async.
        println!(
            "  {batch:>7} {:>8.2} {:>8.2} {:>11} {:>8.2} {:>11}",
            per(today, program.len()),
            per(awaited, program.len()),
            per_call(today - awaited, calls, batch),
            per(plain, program.len()),
            per_call(awaited - plain, calls, batch),
        );
    }
}

/// How the driver calls the counting executor, past the chain.
#[derive(Clone, Copy)]
enum Form {
    /// The chains bench's direct column: `Execute::execute`, awaited.
    Today,
    /// One shot's instructions and readouts alone.
    OneShot(OneShotForm),
}

/// Sends `program`, `batch` instructions per call, unchecked, to a
/// profiler past the chain, in `form`, and returns what it counted and the
/// readouts the driver got.
fn run_in_form(
    program: &[Instruction],
    slots: &[u32],
    batch: usize,
    form: Form,
) -> (Profile, Vec<ReadoutValue>) {
    let mut profiler = Profiler::new(Offer::unlimited());
    let in_batches = ProgramInBatches::new(MixGates::set(), program, slots, QUBITS, batch);
    // The chain's executor is sent no batch: each form sends past it.
    let unused = Profiler::new(Offer::unlimited());
    let received = match form {
        Form::Today => {
            run_once(Driver::new(Direct(in_batches, &mut profiler)) >> Executor::new(unused))
        }
        Form::OneShot(form) => run_once(
            Driver::new(OneShotDirect {
                in_batches,
                profiler: &mut profiler,
                form,
            }) >> Executor::new(unused),
        ),
    };
    let values = black_box(received).values;
    (profiler.profile, values)
}
