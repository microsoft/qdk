// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How the bench times a variant: the best of its runs, each checked to
//! agree, or the best of their median calls; and a time per item, or
//! per call.

use std::fmt::Debug;
use std::hint::black_box;
use std::time::{Duration, Instant};

/// The best time, in seconds, of `repeats` runs of `sample`, each checked
/// to give `expected`, after an untimed run also checked.
pub(crate) fn best<T: PartialEq + Debug>(
    repeats: usize,
    expected: &T,
    mut sample: impl FnMut() -> T,
) -> f64 {
    assert_eq!(&sample(), expected, "the variant agrees before it is timed");
    (0..repeats)
        .map(|_| {
            let start = Instant::now();
            let got = black_box(sample());
            let elapsed = start.elapsed().as_secs_f64();
            assert_eq!(&got, expected, "the variant agrees when timed");
            elapsed
        })
        .fold(f64::INFINITY, f64::min)
}

/// `seconds` per item, in ns.
#[expect(clippy::cast_precision_loss, reason = "a count below 2^52, so exact")]
pub(crate) fn per(seconds: f64, items: usize) -> f64 {
    seconds * 1e9 / items as f64
}

/// The largest batch at which a difference of times is printed per call:
/// beyond it, a run makes few calls, and a difference within the runs'
/// spread, times the batch, would look like a cost per call.
pub(crate) const PER_CALL_UP_TO: usize = 100;

/// `time`, a difference of times over `calls` calls of `batch`
/// instructions, per call, or "—" for a batch beyond [`PER_CALL_UP_TO`].
pub(crate) fn per_call(time: f64, calls: usize, batch: usize) -> String {
    if batch <= PER_CALL_UP_TO {
        format!("({:>7.1} ns)", per(time, calls))
    } else {
        "—".to_owned()
    }
}

/// The best, over `repeats` runs, of a run's median call, in µs.
pub(crate) fn median_call(repeats: usize, mut run: impl FnMut() -> Vec<Duration>) -> f64 {
    (0..repeats)
        .map(|_| {
            let mut times = run();
            times.sort_unstable();
            times[times.len() / 2].as_secs_f64() * 1e6
        })
        .fold(f64::INFINITY, f64::min)
}

/// The median time, in µs, of a clock read before and after nothing: what
/// each call's time in the worker bench holds beside the call.
pub(crate) fn clock_floor() -> f64 {
    let mut times: Vec<Duration> = (0..10_001)
        .map(|_| black_box(Instant::now()).elapsed())
        .collect();
    times.sort_unstable();
    times[times.len() / 2].as_secs_f64() * 1e6
}
