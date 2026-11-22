// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The buffer a caller keeps its readouts in reuses their memory from one
//! `execute` to the next.

use crate::tests::allocations;
use crate::tests::example_sets::{BitReadout, Measuring, ShotFlags};
use crate::{Batch, Readouts, ShotSet};

use BitReadout::One;

/// A caller keeps its readouts in one `Readouts` from round to round, as a
/// driver does, with rounds of more batches, then fewer, then more, as an
/// adapter sending a batch below per shot sends them, and of batches of
/// other shapes: once the readouts have held a round as large, a later
/// round allocates nothing, conditions included, though a round of fewer
/// batches came between.
#[test]
fn readouts_kept_from_a_larger_round_take_a_later_one_without_allocating() {
    let shots = ShotSet::ids(0..100);
    let batches = [Batch::<Measuring>::new(&shots, &[], 3); 5];
    let mut kept = Readouts::new();
    // Shapes and fills the readouts of a round of `count` batches of
    // `shots` shots and `slots` readout slots, as the boundary and an
    // executor do.
    let mut round = |count: usize, shots: usize, slots: u32| {
        for batch_readouts in kept.readouts_for(&batches[..count]) {
            batch_readouts.shape(shots, slots);
            batch_readouts.row_mut(0).fill(One);
            batch_readouts.set_conditions(shots - 1, ShotFlags::REJECTED);
        }
    };
    round(5, 100, 3);
    for (row, count, shots, slots) in [
        ("fewer batches", 3, 100, 3),
        ("as many batches again", 5, 100, 3),
        ("fewer shots of more slots", 5, 50, 6),
        ("fewer shots of fewer slots", 5, 10, 1),
    ] {
        assert_eq!(allocations(|| round(count, shots, slots)), 0, "{row}");
    }
}
