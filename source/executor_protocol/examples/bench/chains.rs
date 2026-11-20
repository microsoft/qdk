// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One driver on four chains, batches of 1 to 10 000 instructions, over
//! an executor whose `execute` is an `async fn`, as users write it: a
//! chain's cost over direct is the protocol's own.

use std::hint::black_box;

use executor_protocol::{Adapter, Driver, Execute, Executor, Instruction, Offer};

use crate::Size;
use crate::parts::{
    Direct, Forward, MixGates, Profile, Profiler, ProgramInBatches, Received, run_once,
    slots_per_batch,
};
use crate::timing::{PER_CALL_UP_TO, best, per, per_call};
use crate::workload::QUBITS;

/// Each chain, per batch size of `batches`, in ns per instruction and, beside each
/// chain, its cost per `execute` over the direct call, at batches of at
/// most [`PER_CALL_UP_TO`] instructions.
pub(crate) fn bench(size: &Size, program: &[Instruction], checked: bool, batches: &[usize]) {
    println!(
        "\nchains: one driver on four chains, ns/instr (overhead per call vs direct, batch ≤ {PER_CALL_UP_TO})"
    );
    println!(
        "  ≫P = Driver ≫ Executor(profiler); ≫F≫P with an adapter that forwards; ≫F≫F≫P with two"
    );
    let expected = Profile::of(program, checked);
    println!(
        "  {:>7} {:>8} {:>20} {:>20} {:>20}",
        "batch", "direct", "≫P", "≫F≫P", "≫F≫F≫P"
    );
    for &batch in batches {
        let slots = slots_per_batch(&MixGates::set(), program, batch);
        let time = |chain| {
            best(size.repeats, &expected, || {
                run_chain(program, &slots, batch, chain, checked)
            })
        };
        let direct = time(Chain::Direct);
        let calls = program.len().div_ceil(batch);
        let cell = |t: f64| {
            format!(
                "{:>8.2} {:>11}",
                per(t, program.len()),
                per_call(t - direct, calls, batch)
            )
        };
        println!(
            "  {batch:>7} {:>8.2} {:>20} {:>20} {:>20}",
            per(direct, program.len()),
            cell(time(Chain::Bare)),
            cell(time(Chain::OneAdapter)),
            cell(time(Chain::TwoAdapters))
        );
    }
}

/// One driver on each chain under measure, over the same counting executor.
#[derive(Clone, Copy)]
pub(crate) enum Chain {
    /// The driver calls the executor itself, past the protocol: the baseline.
    Direct,
    /// `Driver >> Executor`: one boundary.
    Bare,
    /// `Driver >> Adapter >> Executor`, an adapter that forwards.
    OneAdapter,
    /// `Driver >> Adapter >> Adapter >> Executor`, both forwarding.
    TwoAdapters,
    /// `Bare`, with the executor offering `max_instructions` this many.
    Limited(u32),
}

/// Sends `program` down `chain`, `batch` instructions per `execute`, each
/// [`checked`](executor_protocol::Batch::checked) if `checked`, to a
/// profiler, and returns what it counted.
pub(crate) fn run_chain(
    program: &[Instruction],
    slots: &[u32],
    batch: usize,
    chain: Chain,
    checked: bool,
) -> Profile {
    let mut profiler = Profiler::new(match chain {
        Chain::Limited(most) => Offer::unlimited().max_instructions(most),
        _ => Offer::unlimited(),
    });
    let in_batches =
        ProgramInBatches::new(MixGates::set(), program, slots, QUBITS, batch).checked_if(checked);
    black_box(send_down(chain, in_batches, &mut profiler));
    profiler.profile
}

/// Runs `in_batches` down `chain`, over `executor`.
fn send_down<E: Execute>(chain: Chain, in_batches: ProgramInBatches<'_>, executor: E) -> Received {
    match chain {
        Chain::Direct => {
            // The chain's executor is sent no batch: `Direct` sends past it.
            let unused = Profiler::new(Offer::unlimited());
            run_once(Driver::new(Direct(in_batches, executor)) >> Executor::new(unused))
        }
        Chain::Bare | Chain::Limited(_) => {
            run_once(Driver::new(in_batches) >> Executor::new(executor))
        }
        Chain::OneAdapter => run_once(
            Driver::new(in_batches) >> Adapter::new(Forward::new()) >> Executor::new(executor),
        ),
        Chain::TwoAdapters => run_once(
            Driver::new(in_batches)
                >> Adapter::new(Forward::new())
                >> Adapter::new(Forward::new())
                >> Executor::new(executor),
        ),
    }
}
