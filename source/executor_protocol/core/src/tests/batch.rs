// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What crosses a boundary: the shot sets batches run on, and the readouts
//! that come back for them. A batch's checks are those of the boundary that
//! receives it, in `composition/boundary_crossing.rs`.

mod batch_readouts;
mod shot_set;

use std::panic::{AssertUnwindSafe, catch_unwind};

/// The message `call` panics with, in the case `row`.
fn panic_message(row: &str, call: impl FnOnce()) -> String {
    let panic = catch_unwind(AssertUnwindSafe(call)).expect_err(row);
    let Ok(message) = panic.downcast::<String>() else {
        panic!("{row}: the panic has a formatted message");
    };
    *message
}

/// Checks that `call` panics, in the case `row`, with a message saying
/// each of `said`.
fn panics_saying(row: &str, call: impl FnOnce(), said: &[&str]) {
    let message = panic_message(row, call);
    for said in said {
        assert!(message.contains(said), "{row}: {message:?} says {said:?}");
    }
}
