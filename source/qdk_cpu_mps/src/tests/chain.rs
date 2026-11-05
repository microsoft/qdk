// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The state's own promise: a shot's state may move between threads. Its
//! sites' rules are pinned through the engine, in the engine's tests.

use crate::CanonicalMps;

/// A shot may run on another thread than the one that prepared its state.
#[test]
fn the_state_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<CanonicalMps>();
}
