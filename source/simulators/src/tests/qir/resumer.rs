// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `expect_ready` gives the output of a ready future, and refuses one that
//! waits.

use std::future::{pending, ready};

use crate::qir::expect_ready;

#[test]
fn expect_ready_gives_a_ready_futures_output() {
    assert_eq!(expect_ready(ready(7)), 7);
}

#[test]
#[should_panic(expected = "the future is pending")]
fn expect_ready_refuses_a_pending_future() {
    expect_ready(pending::<()>());
}
