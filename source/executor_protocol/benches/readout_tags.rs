// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the check of a batch's readout tags costs ([`check_unique_tags`]),
//! which an executor that reports its readouts by tag runs on every batch.
//!
//! Measured on batches of one readout (a runtime that sends a batch at each
//! measurement), ten, and ten thousand, with tags in order (result ids as a
//! program numbers them) and scrambled. Against the crate's check (sorted
//! by tag), a map from each tag to the readout that first used it
//! (`BTreeMap`; the workspace forbids `std`'s `HashMap`), which stops at
//! the first repeat.
//!
//! Run with `cargo bench -p executor_protocol --bench readout_tags`. The
//! numbers depend on the machine and its load, so this asserts only that
//! the two checks agree, not the numbers: compare the rows of one run, not
//! runs on different machines.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::hint::black_box;
use std::time::Instant;

use executor_protocol::{Rejected, check_unique_tags};

/// The runs of each measurement; the fastest is reported.
const RUNS: usize = 30;

fn main() {
    println!("ns per batch, the fastest of {RUNS} runs; ratio: map / crate's");
    println!(
        "{:>9} {:>10} {:>12} {:>12} {:>8} {:>12}",
        "readouts", "tags", "crate's", "per readout", "map", "ratio"
    );
    for readouts in [1, 10, 10_000] {
        for (order, tag) in [
            ("in order", in_order as fn(u32) -> u32),
            ("scrambled", scrambled),
        ] {
            let batch: Vec<(usize, u32)> = (0..readouts).map(|i| (i as usize, tag(i))).collect();
            agree(&batch);
            let reps = 1_000_000 / readouts;
            let ours = per_batch(reps, || {
                check_unique_tags(black_box(&batch).iter().copied()).is_ok()
            });
            let map = per_batch(reps, || by_map(black_box(&batch).iter().copied()).is_ok());
            println!(
                "{readouts:>9} {order:>10} {ours:>12.1} {:>12.2} {map:>8.1} {:>12.2}",
                ours / f64::from(readouts),
                map / ours
            );
        }
    }
}

fn in_order(i: u32) -> u32 {
    i
}

/// A bijection of `u32`, so that the tags stay distinct.
fn scrambled(i: u32) -> u32 {
    i.wrapping_mul(0x9e37_79b9)
}

/// The two checks agree on `batch`, and on `batch` with its first tag
/// repeated at its middle and at its end.
fn agree(batch: &[(usize, u32)]) {
    let index = |result: Result<(), Rejected>| match result {
        Ok(()) => None,
        Err(Rejected::Instruction { at, .. }) => Some(at),
        Err(other) => panic!("a repeated tag is refused as `Instruction`, not `{other:?}`"),
    };
    let mut variants = vec![batch.to_vec()];
    if let [(_, first), ..] = *batch {
        for at in [batch.len() / 2, batch.len()] {
            let mut repeated = batch.to_vec();
            repeated.insert(at, (at, first));
            for (k, readout) in repeated.iter_mut().enumerate() {
                readout.0 = k;
            }
            variants.push(repeated);
        }
    }
    for variant in variants {
        assert_eq!(
            index(check_unique_tags(variant.iter().copied())),
            index(by_map(variant.iter().copied())),
            "the two checks refuse the same readout"
        );
    }
}

/// The alternative: a map from each tag to the readout that first used it,
/// in the batch's order.
fn by_map(readouts: impl Iterator<Item = (usize, u32)>) -> Result<(), Rejected> {
    let mut first = BTreeMap::new();
    for (at, tag) in readouts {
        match first.entry(tag) {
            Entry::Occupied(_) => {
                return Err(Rejected::Instruction {
                    at,
                    reason: String::new(),
                });
            }
            Entry::Vacant(vacant) => {
                vacant.insert(at);
            }
        }
    }
    Ok(())
}

/// The fastest of `RUNS` runs of `reps` calls of `f`, in ns per call.
fn per_batch(reps: u32, mut f: impl FnMut() -> bool) -> f64 {
    (0..RUNS)
        .map(|_| {
            let start = Instant::now();
            for _ in 0..reps {
                black_box(f());
            }
            start.elapsed().as_secs_f64() * 1e9 / f64::from(reps)
        })
        .fold(f64::INFINITY, f64::min)
}
