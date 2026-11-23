// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One driver on four chains, batches of 1 to 10 000 instructions, over
//! an executor whose `execute` is an `async fn`, as users write it: a
//! chain's cost over direct is the protocol's own. Each run is timed
//! whole, from building its parts through composing and readying them to
//! its last batch, so a chain's cost per `execute` is the whole run's
//! difference from direct, the chain's setup included, spread over its
//! `execute` calls.

use std::hint::black_box;

use block_instruction_set::{BlockInstruction, BlockInstructionSet};
use executor_protocol_core::Resources;
use executor_protocol_core::prelude::*;

use crate::Size;
use crate::parts::{
    ChainBypass, Forward, MixGates, Profile, Profiler, ProgramInBatches, Received, run_once,
    slots_per_batch,
};
use crate::timing::{PER_CALL_UP_TO, best, per, per_call};
use crate::workload::QUBITS;

/// Each chain, per batch size, in ns per instruction and, beside each
/// chain, its whole run's cost over direct per `execute`, at batches of
/// at most [`PER_CALL_UP_TO`] instructions.
pub(crate) fn bench(size: &Size, program: &[BlockInstruction], checked: bool) {
    println!(
        "\nchains: one driver on four chains, ns/instr (whole run vs direct, per call, batch ≤ {PER_CALL_UP_TO})"
    );
    println!(
        "  ≫P = Driver ≫ Executor(profiler); ≫F≫P with an adapter that forwards; ≫F≫F≫P with two"
    );
    let expected = Profile::of(program, checked);
    println!(
        "  {:>7} {:>8} {:>20} {:>20} {:>20}",
        "batch", "direct", "≫P", "≫F≫P", "≫F≫F≫P"
    );
    for batch in [1, 10, 100, 1_000, 10_000] {
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
    /// A driver on an executor: one boundary.
    Bare,
    /// A driver through an adapter that forwards, on an executor.
    OneAdapter,
    /// A driver through two adapters, both forwarding, on an executor.
    TwoAdapters,
    /// `Bare`, with the executor providing this many instructions per batch.
    Limited(u32),
}

/// Sends `program` down `chain`, `batch` instructions per `execute`, each
/// [`checked`](executor_protocol_core::Batch::checked) if `checked`, to a
/// profiler, and returns what it counted.
pub(crate) fn run_chain(
    program: &[BlockInstruction],
    slots: &[u32],
    batch: usize,
    chain: Chain,
    checked: bool,
) -> Profile {
    let mut profiler = Profiler::new(match chain {
        Chain::Limited(most) => Resources::nothing_stated().with_instructions_per_batch(most),
        _ => Resources::nothing_stated(),
    });
    let in_batches =
        ProgramInBatches::new(MixGates::set(), program, slots, QUBITS, batch).checked_if(checked);
    black_box(send_down(chain, in_batches, &mut profiler));
    profiler.part.profile
}

/// Runs `in_batches` down `chain`, over `executor`.
fn send_down<E: Executor<Accepts = BlockInstructionSet>>(
    chain: Chain,
    in_batches: ProgramInBatches<'_>,
    executor: E,
) -> Received {
    match chain {
        Chain::Direct => {
            // The chain's executor is sent no batch: `ChainBypass` sends past it.
            let unused = Profiler::new(Resources::nothing_stated());
            run_once(
                ChainBypass(in_batches, executor)
                    .stating_limits()
                    .on(unused),
            )
        }
        Chain::Bare | Chain::Limited(_) => run_once(in_batches.stating_limits().on(executor)),
        Chain::OneAdapter => run_once(
            in_batches
                .stating_limits()
                .through(Forward::new())
                .on(executor),
        ),
        Chain::TwoAdapters => run_once(
            in_batches
                .stating_limits()
                .through(Forward::new())
                .through(Forward::new())
                .on(executor),
        ),
    }
}
