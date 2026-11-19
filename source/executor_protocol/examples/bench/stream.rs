// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A program streamed in: one driver sends the whole program as one
//! [`InstructionStream`](executor_protocol::InstructionStream), which the
//! protocol cuts into pieces of the most instructions the executor takes,
//! 10 to 10 000 or none set, and at most its own piece size, against the
//! same instructions in batches of that size that the driver cuts itself:
//! the stream's cost over them is
//! the protocol's cutting. Both make each instruction as it is pulled, a
//! copy of the program's, which the batches of `chains` send from the
//! program as it is held: the difference is the cost of making owned
//! instructions, which any sender that makes its program as it goes
//! pays.

use std::hint::black_box;

use executor_protocol::{
    Adapter, Driver, Execute, Executor, Instruction, MAX_INSTRUCTIONS_PER_PIECE, Offer,
};

use crate::Size;
use crate::parts::{
    Forward, MixGates, Profile, Profiler, ProgramInBatches, ProgramMadeAsPulled, Received,
    run_once, slots_per_batch,
};
use crate::timing::{best, per};
use crate::workload::QUBITS;

/// The most instructions per batch the executor offers, then none, where
/// the protocol picks the piece size itself.
const MOSTS: [Option<u32>; 5] = [Some(10), Some(100), Some(1_000), Some(10_000), None];

/// The protocol's own piece size: the most instructions of a piece
/// whatever the executor's most, so the batches are compared at the smaller
/// of the two, the stream and the batches being of one size.
const PIECE: usize = MAX_INSTRUCTIONS_PER_PIECE as usize;

/// Per piece size, the program in batches as held, cut by the driver, and
/// as a stream, in ns per instruction, and the stream's time over the
/// driver's cutting.
pub(crate) fn bench(size: &Size, program: &[Instruction], checked: bool) {
    println!(
        "\nstream: one program as one stream, cut into pieces of the executor's most, at most {PIECE}, against batches of that size, ns/instr"
    );
    println!(
        "  ≫P = Driver ≫ Executor(profiler); ≫F≫P with an adapter that forwards; held: batches of the program as held;"
    );
    println!(
        "  cut: batches the driver cuts from its made instructions; ratio = stream ≫F≫P / cut ≫F≫P"
    );
    let expected = Profile::of(program, checked);
    println!(
        "  {:>6} {:>6} {:>11} {:>11} {:>12} {:>14} {:>7}",
        "most", "piece", "held ≫F≫P", "cut ≫F≫P", "stream ≫P", "stream ≫F≫P", "ratio"
    );
    for most in MOSTS {
        let batch = most.map_or(PIECE, |most| (most as usize).min(PIECE));
        let slots = slots_per_batch(&MixGates::set(), program, batch);
        let in_batches = (batch, slots.as_slice());
        let time = |sending| {
            best(size.repeats, &expected, || {
                run_sending(program, in_batches, most, sending, checked)
            })
        };
        let cut = time(Sending::CutThroughForward);
        let stream = time(Sending::StreamThroughForward);
        println!(
            "  {:>6} {:>6} {:>11.2} {:>11.2} {:>12.2} {:>14.2} {:>7.2}",
            most.map_or_else(|| "none".to_owned(), |most| most.to_string()),
            batch,
            per(time(Sending::HeldThroughForward), program.len()),
            per(cut, program.len()),
            per(time(Sending::Stream), program.len()),
            per(stream, program.len()),
            stream / cut
        );
    }
}

/// How the program goes below.
#[derive(Clone, Copy)]
enum Sending {
    /// In batches of the program as held, `Driver >> Adapter >> Executor`,
    /// an adapter that forwards.
    HeldThroughForward,
    /// In batches the driver cuts from the instructions it makes,
    /// `Driver >> Adapter >> Executor`.
    CutThroughForward,
    /// As one stream, `Driver >> Executor`.
    Stream,
    /// As one stream, `Driver >> Adapter >> Executor`, an adapter that
    /// forwards.
    StreamThroughForward,
}

/// Sends `program` as `sending` says, each batch or the stream
/// [`checked`](executor_protocol::Batch::checked) if `checked`, to a
/// profiler offering `most` instructions per batch, and returns what it
/// counted. `in_batches` is the batch size and each batch's readout
/// slots, counted before a timed run, as in `chains`.
fn run_sending(
    program: &[Instruction],
    in_batches: (usize, &[u32]),
    most: Option<u32>,
    sending: Sending,
    checked: bool,
) -> Profile {
    let offer = most.map_or_else(Offer::unlimited, |most| {
        Offer::unlimited().max_instructions(most)
    });
    let mut profiler = Profiler::new(offer);
    black_box(send_down(
        program,
        in_batches,
        sending,
        checked,
        &mut profiler,
    ));
    profiler.profile
}

/// Runs `program` down the chain `sending` names, over `executor`.
fn send_down<E: Execute>(
    program: &[Instruction],
    (batch, slots): (usize, &[u32]),
    sending: Sending,
    checked: bool,
    executor: E,
) -> Received {
    let as_one_stream =
        || ProgramMadeAsPulled::as_one_stream(MixGates::set(), program, QUBITS).checked_if(checked);
    match sending {
        Sending::CutThroughForward => {
            let in_batches =
                ProgramMadeAsPulled::in_batches_of(MixGates::set(), program, QUBITS, batch)
                    .checked_if(checked);
            run_once(
                Driver::new(in_batches) >> Adapter::new(Forward::new()) >> Executor::new(executor),
            )
        }
        Sending::HeldThroughForward => {
            let in_batches = ProgramInBatches::new(MixGates::set(), program, slots, QUBITS, batch)
                .checked_if(checked);
            run_once(
                Driver::new(in_batches) >> Adapter::new(Forward::new()) >> Executor::new(executor),
            )
        }
        Sending::Stream => run_once(Driver::new(as_one_stream()) >> Executor::new(executor)),
        Sending::StreamThroughForward => run_once(
            Driver::new(as_one_stream()) >> Adapter::new(Forward::new()) >> Executor::new(executor),
        ),
    }
}
