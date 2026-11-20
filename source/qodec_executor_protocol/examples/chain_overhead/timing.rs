// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The time a variant takes: the best of a few rounds, so that a round
//! the machine slowed down does not count.

use std::hint::black_box;
use std::time::{Duration, Instant};

/// The shortest time `run` took over `rounds` rounds; what it gives is
/// kept from the optimizer, so that the work is done.
pub fn best_of<T>(rounds: usize, mut run: impl FnMut() -> T) -> Duration {
    (0..rounds)
        .map(|_| {
            let start = Instant::now();
            black_box(run());
            start.elapsed()
        })
        .min()
        .expect("at least one round")
}
