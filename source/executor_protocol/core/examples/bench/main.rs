// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the protocol costs, per instruction and per `execute`, against a
//! driver that calls its executor itself, in four benches, each in its
//! file:
//!
//! - `chains`: one driver on four chains, batches of 1 to 10 000
//!   instructions, over an executor whose `execute` is an `async fn`, as
//!   users write it. A chain's cost over direct is the protocol's own;
//! - `checks`: a batch's checks, whether a boundary without them costs
//!   anything per instruction, building an instruction by name, and the
//!   count of a batch's instructions against a most provided;
//! - `lowering`: a lowering adapter, each logical instruction sent as
//!   three physical;
//! - `worker`: an executor that answers from a worker thread: the time
//!   of a call, which includes copying its batches to the worker.
//!
//! The executors only count what they get, so the time is the protocol's
//! and the parts' own. Each variant is checked on every run: it must count
//! what the program holds, received in batches checked as its flag asks
//! (or, in `lowering`, answer what the direct call answers, and its
//! composing must compose; in `checks`, building by name must build the
//! program's instructions, compared once before it is timed, and count
//! them on every run). It runs once untimed before it is timed, and keeps
//! the best of 7. `worker` has no untimed run, and keeps the best of up to
//! 5 runs' median call. `chains`, `lowering` and `worker` send every batch
//! unchecked, the default, or
//! [`checked`](executor_protocol_core::Batch::checked) with `--checked`;
//! `checks` runs both. `chains`, `checks` and `worker` draw the common
//! mix, or the parity-heavy one with `--parity-heavy`. Run it with
//! `cargo run --release --example bench`, followed by at most one of
//! `chains`, `checks`, `lowering` and `worker`, and the flags.
//!
//! The parts the benches compose are in `parts`, the mix they draw in
//! `workload`, and how a variant is timed in `timing`.

#![expect(
    clippy::unused_async_trait_impl,
    reason = "a part that never waits keeps the trait's `async fn` form, the form the protocol \
              documents; the `core::future::ready` the lint suggests would run the body when \
              the method is called, not when its future is polled"
)]

mod chains;
mod checks;
mod lowering;
mod parts;
mod timing;
mod worker;
mod workload;

use executor_protocol_core::Instruction;

use workload::{Mix, build, draws};

/// How much each bench runs.
struct Size {
    /// The mix's instructions, for `chains`, `checks` and `worker`.
    instructions: usize,
    /// The timed runs of each variant, of which the best is kept.
    repeats: usize,
    /// `lowering`'s logical rounds, of 6 instructions each.
    rounds: usize,
    /// `worker`'s most calls per batch size.
    most_calls: usize,
}

/// The size of the tables, which `main` runs.
const FULL: Size = Size {
    instructions: 4_000_000,
    repeats: 7,
    rounds: 2_000,
    most_calls: 20_000,
};

/// A size at which every variant runs and agrees in a debug test.
#[cfg(test)]
const TINY: Size = Size {
    instructions: 3_000,
    repeats: 1,
    rounds: 4,
    most_calls: 10,
};

/// The benches, by the names `main` takes.
const BENCHES: [&str; 4] = ["chains", "checks", "lowering", "worker"];

/// The flag that sends every batch checked.
const CHECKED: &str = "--checked";

/// The flag that draws the parity-heavy mix.
const PARITY_HEAVY: &str = "--parity-heavy";

fn main() {
    let (flags, benches): (Vec<String>, Vec<String>) = std::env::args()
        .skip(1)
        .partition(|arg| arg.starts_with("--"));
    let checked = flags.iter().any(|flag| flag == CHECKED);
    let mix = if flags.iter().any(|flag| flag == PARITY_HEAVY) {
        Mix::ParityHeavy
    } else {
        Mix::Common
    };
    let unknown = flags
        .iter()
        .find(|flag| ![CHECKED, PARITY_HEAVY].contains(&flag.as_str()));
    let only = benches.first();
    if let Some(flag) = unknown {
        eprintln!("no flag {flag:?}: the flags are {CHECKED} and {PARITY_HEAVY}");
        std::process::exit(2);
    }
    if let Some(only) = only
        && (benches.len() > 1 || !BENCHES.contains(&only.as_str()))
    {
        eprintln!(
            "no bench {only:?}: the benches are chains, checks, lowering and worker, one at most"
        );
        std::process::exit(2);
    }
    run(&FULL, only.map(String::as_str), checked, mix);
}

/// Runs the benches, or only the one named `only`, each batch
/// [`checked`](executor_protocol_core::Batch::checked) if `checked` (`checks`
/// runs both), `chains`, `checks` and `worker` on `mix`.
fn run(size: &Size, only: Option<&str>, checked: bool, mix: Mix) {
    let wants = |bench: &str| only.is_none_or(|only| only == bench);
    println!("an instruction is {} bytes", size_of::<Instruction>());
    if wants("chains") || wants("lowering") || wants("worker") {
        println!(
            "chains, lowering and worker: batches {}",
            if checked { "checked" } else { "unchecked" }
        );
    }
    if wants("chains") || wants("checks") || wants("lowering") {
        println!(
            "chains, checks and lowering: the best of {} runs",
            size.repeats
        );
    }
    if wants("chains") || wants("checks") || wants("worker") {
        println!(
            "chains, checks and worker: {} instructions of the {} mix",
            size.instructions,
            match mix {
                Mix::Common => "common",
                Mix::ParityHeavy => "parity-heavy",
            },
        );
        let draws = draws(size.instructions, mix);
        let program = build(&draws);
        if wants("chains") {
            chains::bench(size, &program, checked);
        }
        if wants("checks") {
            checks::bench(size, &draws, &program);
        }
        if wants("worker") {
            worker::bench(size, &program, checked);
        }
    }
    if wants("lowering") {
        lowering::bench(size, checked);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every bench, at a tiny size, on each mix, with its batches
    /// unchecked and checked: each variant must count, or answer, what the
    /// direct call does, so the tables compare the same work.
    #[test]
    fn every_variant_does_the_same_work_at_a_tiny_size() {
        for mix in [Mix::Common, Mix::ParityHeavy] {
            run(&TINY, None, false, mix);
            run(&TINY, None, true, mix);
        }
    }
}
