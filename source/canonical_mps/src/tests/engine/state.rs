// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The state's own guarantees, apart from the contract's.

use super::*;

/// A runtime that moves each shot's state to a worker thread: the state is
/// `Send` when its algebra's tensors are, as the CPU algebra's are.
#[test]
fn a_state_on_send_tensors_is_send() {
    fn send<T: Send>() {}
    send::<State>();
}
