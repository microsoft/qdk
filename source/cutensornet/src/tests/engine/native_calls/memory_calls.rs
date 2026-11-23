// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rules of `MemoryCalls`: the memory a workspace's slots are lent,
//! and when what a stream's work writes may be read, copied over or
//! freed.

use super::*;

/// cuTensorNet may ask for memory in any slot of a workspace (a device
/// cache, host scratch), and runs a call only once each slot that asks is
/// lent at least what it asks for. The one-byte-short cases run on the
/// fake alone: cuTensorNet 2.13 on an A100 ran a QR whose slot was lent one
/// byte short, though its documentation asks for at least the minimum, so
/// only the fake holds the algebra to it. The zero-slot case runs on the
/// fake alone too: its rule is wider than the evidence (below).
#[test]
fn a_workspace_slot_is_lent_what_it_asks() {
    let with_other_slots = || {
        let libraries = FakeLibraries::new();
        ask_in_every_other_slot(&libraries, 64);
        Device::new(libraries)
    };
    a_workspace_slot_is_lent_what_it_asks_on(&fake);
    a_workspace_slot_is_lent_what_it_asks_on(&with_other_slots);
    a_workspace_slot_lent_one_byte_short_is_refused(&fake);
    a_workspace_slot_lent_one_byte_short_is_refused(&with_other_slots);
    a_tensor_call_needs_memory_in_every_slot_that_asks_for_it();
    a_workspace_slot_that_asks_for_nothing_is_lent_nothing();
}

/// The cases of [`a_workspace_slot_is_lent_what_it_asks`] that run on the
/// devices `new` makes: a caller that leaves a slot that asks for memory
/// unlent is refused, whichever slot it is.
pub(super) fn a_workspace_slot_is_lent_what_it_asks_on<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    let mut asking = 0;
    for slot in WORKSPACE_SLOTS {
        let qr = TensorQr::sized(new(), 3);
        let device = &qr.device;
        let asked = device.asked(slot);
        if asked <= 0 {
            continue;
        }
        asking += 1;
        device.lend_all_but(slot);
        let refused = qr
            .run_on(device.stream)
            .expect_err("one slot is lent nothing");
        assert!(
            refused_for::<C>(&refused, FakeRule::NoMemoryLent),
            "{slot:?}: {refused}"
        );
        device.lend(slot);
        qr.run_on(device.stream)
            .expect("every slot is lent in full");
    }
    assert!(asking > 0, "the QR asks for memory in some slot");
}

/// cuTensorNet's QR takes a workspace whose size is "the same as or larger
/// than the minimum needed": a caller that lends a slot one byte short (an
/// old size, after the workspace was sized for a larger call) is caught on
/// the fake, whichever slot it is.
fn a_workspace_slot_lent_one_byte_short_is_refused(new: &dyn Fn() -> Device) {
    let mut asking = 0;
    for slot in WORKSPACE_SLOTS {
        let qr = TensorQr::sized(new(), 3);
        let device = &qr.device;
        let asked = device.asked(slot);
        if asked <= 0 {
            continue;
        }
        asking += 1;
        device.lend_all_but(slot);
        device.lend_bytes(slot, asked - 1);
        let refused = qr
            .run_on(device.stream)
            .expect_err("one slot is one byte short");
        assert!(
            refused_for::<FakeLibraries>(&refused, FakeRule::LessMemoryLent),
            "{slot:?}: {refused}"
        );
        device.lend(slot);
        qr.run_on(device.stream)
            .expect("every slot is lent in full");
    }
    assert!(asking > 0, "the QR asks for memory in some slot");
}

/// What a stream's work writes (a tensor call's outputs, a gate split's
/// report) is read, copied over or freed only once that stream is
/// synchronized. Its cases run on the fake alone: on a GPU, the misuse is
/// a race.
#[test]
fn data_written_on_a_stream_is_used_only_after_its_sync() {
    a_copy_to_the_host_needs_the_writing_stream_synchronized();
    a_copy_on_the_device_needs_the_stream_writing_its_buffers_synchronized();
    every_output_of_a_tensor_call_is_read_once_its_stream_is_synchronized();
    a_buffer_is_freed_once_every_stream_is_synchronized();
    an_svd_info_is_read_once_its_writing_stream_is_synchronized();
}

/// A read or a free waits for the stream last queued to write what it
/// touches, and for no other. Its cases run on the fake alone: they pin
/// what the fake tracks, which a GPU has no counterpart of.
#[test]
fn a_read_or_free_waits_only_for_the_last_writer() {
    a_read_waits_only_for_the_stream_that_wrote_it();
    a_read_waits_for_the_last_stream_that_wrote_it();
    a_free_waits_only_for_the_last_writer_of_each_pending_write();
}

/// A caller that lends device scratch alone is caught on the fake whether
/// it contracts or splits a gate, as when it runs a QR.
fn a_tensor_call_needs_memory_in_every_slot_that_asks_for_it() {
    let libraries = FakeLibraries::new();
    ask_in_every_other_slot(&libraries, 64);
    let contraction = Contraction::dot_product(Device::new(libraries));
    needs_every_slot(&contraction.device, || contraction.contract());
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]));
    ask_in_every_other_slot(&split.device.calls, 64);
    split.size_workspace(split.config);
    needs_every_slot(&split.device, || {
        split.run(split.config, split.device.stream)
    });
}

/// The fake refuses memory for a slot that asks for none, a rule wider
/// than the evidence (on an A100, cuTensorNet refuses a zero-size
/// workspace with a pointer): a caller that lends every slot regardless of
/// its size is caught on the fake.
fn a_workspace_slot_that_asks_for_nothing_is_lent_nothing() {
    let device = fake();
    let (fake, handle, workspace) = (&*device.calls, device.handle, device.workspace);
    let memory = device.host(64);
    fake.set_workspace(handle, workspace, WorkspaceSlot::HostCache, memory, 64)
        .expect_err("the host cache asks for nothing");
    ask_in_every_other_slot(fake, 64);
    fake.set_workspace(handle, workspace, WorkspaceSlot::HostCache, memory, 64)
        .expect("the host cache asks for 64 bytes");
}

/// A tensor call writes its output on its stream, which a copy to the host
/// does not wait for: a caller that copies a contraction's output before
/// synchronizing that stream, or after synchronizing only another one, is
/// caught on the fake.
fn a_copy_to_the_host_needs_the_writing_stream_synchronized() {
    let contraction = Contraction::dot_product(fake());
    let device = &contraction.device;
    let other_stream = device.another_stream().expect("the fake has a device");
    contraction.contract().expect("the network is prepared");
    let mut output = [Complex64Abi::default()];
    device
        .calls
        .copy_to_host(&mut output, contraction.output)
        .expect_err("the stream may still be writing the output");
    device
        .calls
        .synchronize_stream(other_stream)
        .expect("the other stream is live");
    device
        .calls
        .copy_to_host(&mut output, contraction.output)
        .expect_err("only a stream that did not write the output is synchronized");
    device
        .calls
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    device
        .calls
        .copy_to_host(&mut output, contraction.output)
        .expect("the stream is synchronized");
    assert_eq!(Complex64::from(output[0]), Complex64::new(2.0, 0.0));
}

/// The copies on the device run on CUDA's legacy stream, which a
/// non-blocking stream's work does not wait for: a caller that copies from
/// a contraction's output, or into it, from the host or on the device,
/// before synchronizing the stream writing it, is caught on the fake.
fn a_copy_on_the_device_needs_the_stream_writing_its_buffers_synchronized() {
    type CopyCall = fn(&Device, OpaqueHandle, OpaqueHandle) -> Result<(), CudaAlgebraError>;
    let copies: [(&str, CopyCall); 3] = [
        ("from the output", |device, output, other| {
            device.calls.copy_on_device(other, output, 16)
        }),
        ("into the output", |device, output, other| {
            device.calls.copy_on_device(output, other, 16)
        }),
        ("from the host into the output", |device, output, _| {
            device
                .calls
                .copy_to_device(output, &[Complex64Abi::default()])
        }),
    ];
    for (case, copy) in copies {
        let contraction = Contraction::dot_product(fake());
        let device = &contraction.device;
        let other = device.zeros(&[1]);
        device
            .calls
            .synchronize_copies()
            .expect("the copy was queued");
        contraction.contract().expect("the network is prepared");
        copy(device, contraction.output, other).expect_err(&format!(
            "{case}: the stream may still be writing the output"
        ));
        device
            .calls
            .synchronize_stream(device.stream)
            .expect("the stream is live");
        copy(device, contraction.output, other)
            .unwrap_or_else(|error| panic!("{case}: the stream is synchronized: {error}"));
    }
}

/// A gate split and a QR write each of their outputs on their stream (the
/// split's U, V and singular values, the QR's Q and R): a caller that
/// copies any of them to the host before synchronizing it is caught on the
/// fake.
fn every_output_of_a_tensor_call_is_read_once_its_stream_is_synchronized() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]));
    let device = &split.device;
    split.size(split.config);
    split
        .run(split.config, device.stream)
        .expect("the workspace is sized and lent");
    let zero = Complex64::default();
    let input = device.operand(&[2, 3], &[1, 2], &values(6, 0.5));
    let q = device.operand(&[2, 2], &[1, 3], &[zero; 4]);
    let r = device.operand(&[2, 3], &[3, 2], &[zero; 6]);
    device
        .calls
        .synchronize_copies()
        .expect("the copies were queued");
    device
        .calls
        .qr_workspace_sizes(
            device.handle,
            input.descriptor,
            q.descriptor,
            r.descriptor,
            device.workspace,
        )
        .expect("the descriptors are live");
    device.lend(WorkspaceSlot::DeviceScratch);
    device
        .calls
        .tensor_qr(device.handle, input, q, r, device.workspace, device.stream)
        .expect("the workspace is sized and lent");
    let outputs = [
        split.operands.u.data,
        split.operands.v.data,
        split.operands.singular_values,
        q.data,
        r.data,
    ];
    let mut values = [Complex64Abi::default()];
    for output in outputs {
        device
            .calls
            .copy_to_host(&mut values, output)
            .expect_err("the stream may still be writing the output");
    }
    device
        .calls
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    for output in outputs {
        device
            .calls
            .copy_to_host(&mut values, output)
            .expect("the stream is synchronized");
    }
}

/// The fake does not track which buffers a stream's work reads: a caller
/// that contracts on two streams and frees an input once only one of them
/// is synchronized, while a GPU may still be reading it on the other, is
/// caught on the fake.
fn a_buffer_is_freed_once_every_stream_is_synchronized() {
    let contraction = Contraction::dot_product(fake());
    let device = &contraction.device;
    let second = device.another_stream().expect("the fake has a device");
    let second_output = device.zeros(&[]);
    device
        .calls
        .synchronize_copies()
        .expect("the copy was queued");
    contraction.contract().expect("the network is prepared");
    contraction
        .contract_buffers(contraction.inputs, second_output, second)
        .expect("the network is prepared");
    device
        .calls
        .free(contraction.inputs[0])
        .expect_err("both contractions may still be reading the input");
    device
        .calls
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    device
        .calls
        .free(contraction.inputs[0])
        .expect_err("the second contraction may still be reading the input");
    device
        .calls
        .synchronize_stream(second)
        .expect("the second stream is live");
    device
        .calls
        .free(contraction.inputs[0])
        .expect("every stream is synchronized");
    device.disown(contraction.inputs[0]);
}

/// cuTensorNet writes a gate split's report on the split's stream: a
/// caller that reads it before synchronizing that stream, or after
/// synchronizing only another one, is caught on the fake.
fn an_svd_info_is_read_once_its_writing_stream_is_synchronized() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]));
    let device = &split.device;
    let other_stream = device.another_stream().expect("the fake has a device");
    split.size(split.config);
    split
        .run(split.config, device.stream)
        .expect("the workspace is sized and lent");
    device
        .calls
        .svd_truncation(device.handle, split.info)
        .expect_err("the stream may still be writing the info");
    device
        .calls
        .synchronize_stream(other_stream)
        .expect("the other stream is live");
    device
        .calls
        .svd_truncation(device.handle, split.info)
        .expect_err("only a stream that did not write the info is synchronized");
    device
        .calls
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    device
        .calls
        .svd_truncation(device.handle, split.info)
        .expect("the stream is synchronized");
}

/// Work queued on one stream does not hold back a read of what another,
/// synchronized stream wrote: a caller that runs a QR on a second stream
/// still reads a gate split's factors and report once the split's stream
/// is synchronized, and is refused only the QR's factors.
fn a_read_waits_only_for_the_stream_that_wrote_it() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]));
    let device = &split.device;
    split.size(split.config);
    split
        .run(split.config, device.stream)
        .expect("the workspace is sized and lent");
    device
        .calls
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    let zero = Complex64::default();
    let input = device.operand(&[2, 3], &[1, 2], &values(6, 0.5));
    let q = device.operand(&[2, 2], &[1, 3], &[zero; 4]);
    let r = device.operand(&[2, 3], &[3, 2], &[zero; 6]);
    device
        .calls
        .synchronize_copies()
        .expect("the copies were queued");
    device
        .calls
        .qr_workspace_sizes(
            device.handle,
            input.descriptor,
            q.descriptor,
            r.descriptor,
            device.workspace,
        )
        .expect("the descriptors are live");
    device.lend(WorkspaceSlot::DeviceScratch);
    let qr_stream = device.another_stream().expect("the fake has a device");
    device
        .calls
        .tensor_qr(device.handle, input, q, r, device.workspace, qr_stream)
        .expect("the workspace is sized and lent");
    let mut values = [Complex64Abi::default()];
    device
        .calls
        .svd_truncation(device.handle, split.info)
        .expect("the split's stream is synchronized");
    device
        .calls
        .copy_to_host(&mut values, split.operands.u.data)
        .expect("the split's stream is synchronized");
    device
        .calls
        .copy_to_host(&mut values, q.data)
        .expect_err("the QR's stream may still be writing Q");
}

/// The fake records, for what is written, the stream last queued to write
/// it: a caller that contracts into one output on two streams and copies it
/// to the host once only the first stream is synchronized is caught on the
/// fake.
fn a_read_waits_for_the_last_stream_that_wrote_it() {
    let contraction = Contraction::dot_product(fake());
    let device = &contraction.device;
    let second = device.another_stream().expect("the fake has a device");
    contraction.contract().expect("the first write");
    contraction.contract_on(second).expect("the second write");
    device
        .calls
        .synchronize_stream(device.stream)
        .expect("the first stream is live");
    let mut output = [Complex64Abi::default()];
    device
        .calls
        .copy_to_host(&mut output, contraction.output)
        .expect_err("the second stream may still be writing the output");
    device
        .calls
        .synchronize_stream(second)
        .expect("the second stream is live");
    device
        .calls
        .copy_to_host(&mut output, contraction.output)
        .expect("the last writer is synchronized");
}

/// The fake records one writer per pending write, the stream last queued to
/// write it: once a second stream contracts into the first one's output and
/// is synchronized, the fake no longer holds `free` for the first stream's
/// contraction, which a GPU may still be running. A caller's test does not
/// catch that misuse on the fake.
fn a_free_waits_only_for_the_last_writer_of_each_pending_write() {
    let contraction = Contraction::dot_product(fake());
    let device = &contraction.device;
    let second = device.another_stream().expect("the fake has a device");
    contraction.contract().expect("the first write");
    contraction.contract_on(second).expect("the second write");
    device
        .calls
        .synchronize_stream(second)
        .expect("the second stream is live");
    device
        .calls
        .free(contraction.inputs[0])
        .expect("the first stream is no longer the output's recorded writer");
    device.disown(contraction.inputs[0]);
}

/// CUDA lets a stream's work run on after `cudaStreamDestroy`: a caller
/// that destroys the stream of a contraction and then frees the
/// contraction's input is caught on the fake. On the fake alone: on a GPU,
/// the misuse is a race.
#[test]
fn a_destroyed_stream_keeps_its_writes_pending() {
    let contraction = Contraction::dot_product(fake());
    let device = &contraction.device;
    let doomed = device.calls.create_stream().expect("the fake has a device");
    contraction
        .contract_on(doomed)
        .expect("the network is prepared");
    device
        .calls
        .synchronize_stream(device.stream)
        .expect("the stream is live");
    device
        .calls
        .free(contraction.inputs[0])
        .expect_err("the contraction may still be reading its input");
    device
        .calls
        .destroy_stream(doomed)
        .expect("the stream is live");
    device
        .calls
        .free(contraction.inputs[0])
        .expect_err("the destroyed stream's work may still run");
    device.disown_buffers();
}
