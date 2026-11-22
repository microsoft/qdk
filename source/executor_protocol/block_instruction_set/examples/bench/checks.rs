// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a batch's checks cost, and a boundary without them; building an
//! instruction by name; and the count of a batch's instructions against a
//! most provided.

use std::hint::black_box;

use block_instruction_set::BlockInstruction;

use crate::Size;
use crate::chains::{Chain, run_chain};
use crate::parts::{MixGates, Profile, slots_per_batch};
use crate::timing::{best, per, per_call};
use crate::workload::{Draw, build_one};

/// What a batch's checks cost, and a boundary without them, by difference,
/// since they run inside the protocol; building an instruction by name, which
/// checks it too; and the count of a batch's instructions against a most
/// provided. A boundary without the checks costs per call (shaping the
/// readouts, the catch of a panic, one `BoundaryCrossing` call), which the
/// chains bench's ≫P at batch 1 shows; its row here, at a large batch, shows
/// that it makes no pass per instruction.
pub(crate) fn bench(size: &Size, draws: &[Draw], program: &[BlockInstruction]) {
    println!("\nchecks: a batch's checks, ns/instr (per call, where given)");
    let time = |batch, chain, checked| {
        let slots = slots_per_batch(&MixGates::set(), program, batch);
        best(size.repeats, &Profile::of(program, checked), || {
            run_chain(program, &slots, batch, chain, checked)
        })
    };
    let n = program.len();
    let large = 10_000;
    let unchecked = time(large, Chain::Bare, false);
    let boundary = unchecked - time(large, Chain::Direct, false);
    let checked = time(large, Chain::Bare, true) - unchecked;
    println!(
        "  an unchecked boundary, cost per instruction, batch {large} (≫P minus direct) {:>6.2}",
        per(boundary, n)
    );
    println!("    (its cost per call: the chains bench's ≫P at batch 1)");
    println!(
        "  a batch's checks, batch {large} (≫P, checked minus unchecked)  {:>6.2}",
        per(checked, n)
    );
    let most = u32::try_from(large).expect("a small limit");
    // The count is made on every batch, checked or not.
    for checked in [false, true] {
        let kind = if checked { "checked" } else { "unchecked" };
        for batch in [1, large] {
            let limited =
                time(batch, Chain::Limited(most), checked) - time(batch, Chain::Bare, checked);
            println!(
                "  max_instructions {most} provided, {kind:>9}, batch {batch:>5} (minus unlimited) {:>6.2} {}",
                per(limited, n),
                per_call(limited, n.div_ceil(batch), batch)
            );
        }
    }
    // Into one buffer, reused, so that the row times building, not
    // allocating and freeing a program of instructions (their size printed
    // above). Each buffer is compared with the program's instructions it
    // stands for before the row is timed; every run's count is checked too.
    let set = MixGates::set();
    let mut buffer = Vec::with_capacity(BUILD_BUFFER);
    for (some, expected) in draws.chunks(BUILD_BUFFER).zip(program.chunks(BUILD_BUFFER)) {
        buffer.clear();
        buffer.extend(some.iter().map(|draw| build_one(&set, draw)));
        assert!(
            buffer == expected,
            "each buffer holds the program's instructions it stands for"
        );
    }
    let built = best(size.repeats, &program.len(), || {
        let mut count = 0;
        for some in draws.chunks(BUILD_BUFFER) {
            buffer.clear();
            buffer.extend(some.iter().map(|draw| build_one(&set, draw)));
            count += black_box(&buffer).len();
        }
        count
    });
    println!(
        "  building each instruction by name (BlockInstructionSet::instruction) {:>6.2}",
        per(built, n)
    );
}

/// The instructions the bench builds into its buffer at a time.
const BUILD_BUFFER: usize = 10_000;
