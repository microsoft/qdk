// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of [`crate::mps_simulator`], one file per file with behaviour,
//! all on `tensornet`'s exact reference engine, or on `reference_spy.rs`'s
//! spy over it, which records the calls and stands in for an engine that
//! truncates or fails; `programs.rs` writes the QIR programs they run.

mod fusion;
mod programs;
mod reference_spy;
mod report;
mod resumer;

use crate::mps_simulator::resumer::splitmix64::SplitMix64;

/// A value of `stream` below `n`, for the tests' random cases.
fn below(stream: &mut SplitMix64, n: usize) -> usize {
    let n = u64::try_from(n).expect("a usize fits a u64");
    usize::try_from(stream.next_u64() % n).expect("below a usize")
}
