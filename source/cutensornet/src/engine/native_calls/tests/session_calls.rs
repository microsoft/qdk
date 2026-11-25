// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rules of `SessionCalls`: a stream is synchronized, and a tensor
//! call queued on it, only while it is live.

use super::*;

/// A stream is synchronized, and a tensor call queued on it, only while
/// it is live. Its cases run on the fake alone: on a GPU, a destroyed
/// stream is undefined behaviour.
#[test]
fn a_stream_is_used_only_while_live() {
    a_stream_is_synchronized_only_while_live();
    a_tensor_call_runs_on_a_live_stream();
}

/// A stream is synchronized while it exists: a caller that synchronizes one
/// it destroyed is caught on the fake.
fn a_stream_is_synchronized_only_while_live() {
    let device = fake();
    let stream = device.calls.create_stream().expect("the fake has a device");
    device
        .calls
        .synchronize_stream(stream)
        .expect("the stream is live");
    device
        .calls
        .destroy_stream(stream)
        .expect("the stream is live");
    device
        .calls
        .synchronize_stream(stream)
        .expect_err("the stream is destroyed");
}

/// A gate split, a QR and a contraction are queued on a stream: a caller
/// that passes a destroyed one is caught on the fake.
fn a_tensor_call_runs_on_a_live_stream() {
    let destroyed_stream = |device: &Device| {
        let stream = device.calls.create_stream().expect("the fake has a device");
        device
            .calls
            .destroy_stream(stream)
            .expect("the stream is live");
        stream
    };
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]));
    split.size(split.config);
    split
        .run(split.config, destroyed_stream(&split.device))
        .expect_err("the gate split's stream is destroyed");
    split
        .run(split.config, split.device.stream)
        .expect("the gate split's stream is live");
    qr_on(fake(), 3, destroyed_stream).expect_err("the QR's stream is destroyed");
    let contraction = Contraction::dot_product(fake());
    contraction
        .contract_on(destroyed_stream(&contraction.device))
        .expect_err("the contraction's stream is destroyed");
    contraction
        .contract()
        .expect("the contraction's stream is live");
}
