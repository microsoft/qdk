// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the rules the fake libraries hold the CUDA algebra to: each
//! table pins one of cuTensorNet's or CUDA's rules (tensor descriptors,
//! workspaces, a gate split's labels, a contraction's steps, objects'
//! owners and lifetimes, streams), with the cases the rule accepts and
//! those it refuses.
//!
//! The algebra's own tests make no misuse, so they reach only the accepted
//! side of each rule; only a direct caller of the native calls can make the
//! misuse a rule refuses, so these tests call them directly. A table whose
//! cases a GPU can run takes the device its cases run on, and runs on the
//! fake here and on cuTensorNet in `on_the_gpu` (ignored, run by hand on a
//! CUDA host), so that the fake is checked to refuse what the library
//! refuses. The other cases run on the fake alone, each table saying why:
//! on a GPU their misuse is undefined behaviour or goes undetected.

use crate::engine::{
    CudaAlgebraError,
    native_calls::{
        Complex64Abi, ContractionNetwork, GateSplitDescriptors, GateSplitOperands, MemoryCalls,
        NativeCalls, OpaqueHandle, SessionCalls, Stream, SvdSetting, TensorCalls, TensorModes,
        TensorOperand, WorkspaceSlot, fake_libraries::FakeLibraries,
    },
};
use num_complex::Complex64;
use std::{cell::RefCell, ptr::NonNull, sync::Arc};

/// cuTensorNet may ask for memory in any slot of a workspace (a device
/// cache, host scratch), and runs a call only once each slot that asks is
/// lent at least what it asks for. The zero-slot case runs on the fake
/// alone: its rule is wider than the evidence (below).
#[test]
fn a_workspace_slot_is_lent_what_it_asks() {
    a_workspace_slot_is_lent_what_it_asks_on(&fake);
    a_workspace_slot_is_lent_what_it_asks_on(&|| {
        let libraries = FakeLibraries::new();
        libraries.set_other_workspace_slot_bytes(64);
        Device::new(libraries)
    });
    a_tensor_call_needs_memory_in_every_slot_that_asks_for_it();
    a_workspace_slot_that_asks_for_nothing_is_lent_nothing();
}

/// The cases of [`a_workspace_slot_is_lent_what_it_asks`] that run on the
/// devices `new` makes: a caller that leaves a slot that asks for memory
/// unlent, or lends it one byte short (an old size, after the workspace
/// was sized for a larger call), is refused, whichever slot it is.
fn a_workspace_slot_is_lent_what_it_asks_on<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
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
            refused_for::<C>(&refused, "no memory lent"),
            "{slot:?}: {refused}"
        );
        device.lend_bytes(slot, asked - 1);
        let refused = qr
            .run_on(device.stream)
            .expect_err("one slot is one byte short");
        assert!(
            refused_for::<C>(&refused, "less memory lent"),
            "{slot:?}: {refused}"
        );
        device.lend(slot);
        qr.run_on(device.stream)
            .expect("every slot is lent in full");
    }
    assert!(asking > 0, "the QR asks for memory in some slot");
}

/// A tensor descriptor is checked when it is created: one extent per mode,
/// each at least 1, each mode named once, and an element count a buffer
/// could hold. The overflow case runs on the fake alone: no GPU has the
/// memory to reach it.
#[test]
fn a_malformed_descriptor_is_refused() {
    a_malformed_descriptor_is_refused_on(&fake);
    a_tensor_whose_element_count_overflows_is_refused();
}

/// The cases of [`a_malformed_descriptor_is_refused`] that run on the
/// devices `new` makes.
fn a_malformed_descriptor_is_refused_on<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    a_tensor_descriptor_needs_one_extent_per_mode(new);
    a_tensor_descriptor_refuses_an_extent_below_one(new);
    a_tensor_descriptor_refuses_a_repeated_mode(new);
}

/// The bond a factorization returns is at most the smaller side of its
/// matrix: a gate split's, between U and V, is at most the smaller side
/// of θ, and a QR's, which is thin, exactly the smaller side.
#[test]
fn a_factorizations_bond_is_at_most_its_smaller_side() {
    a_factorizations_bond_is_at_most_its_smaller_side_on(&fake);
}

/// The cases of [`a_factorizations_bond_is_at_most_its_smaller_side`], on
/// the devices `new` makes.
fn a_factorizations_bond_is_at_most_its_smaller_side_on<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    a_gate_split_needs_a_shared_extent_within_the_smaller_side(new);
    a_qr_needs_the_shared_extent_to_be_the_smaller_side(new);
}

/// A gate split writes U, V and the singular values to their buffers, at
/// the sizes their descriptors give, before it knows the reduced extent.
/// Its cases run on the fake alone: on a GPU, a buffer too small or freed
/// is overrun, not refused.
#[test]
fn a_gate_split_writes_only_live_buffers_of_its_descriptors_sizes() {
    a_gate_split_needs_u_and_v_buffers_of_their_descriptors_full_extents();
    a_gate_split_needs_a_singular_values_buffer_of_the_shared_extent();
    a_gate_split_needs_a_live_singular_values_buffer();
}

/// A gate split contracts A, B and the gate along the labels they share,
/// and keeps each other label on its side: A's in U, B's in V, the gate's
/// on either.
#[test]
fn a_gate_split_takes_only_its_topologys_labels() {
    a_gate_split_takes_only_its_topologys_labels_on(&fake);
}

/// The cases of [`a_gate_split_takes_only_its_topologys_labels`], on the
/// devices `new` makes.
fn a_gate_split_takes_only_its_topologys_labels_on<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    a_gate_split_refuses_its_shared_mode_on_an_input(new);
    a_gate_split_refuses_a_label_outside_its_topology(new);
    a_gate_split_needs_a_b_and_the_gate_pairwise_connected(new);
    a_gate_split_refuses_a_label_on_all_three_inputs(new);
    a_gate_split_refuses_a_contracted_label_kept_in_an_output(new);
    a_gate_split_refuses_its_shared_mode_on_a_single_input(new);
    a_gate_split_keeps_a_label_of_a_alone_in_u(new);
    a_gate_split_keeps_a_label_of_b_alone_in_v(new);
    a_gate_split_keeps_a_label_of_the_gate_alone_on_either_side(new);
    a_gate_split_takes_modes_of_extent_one(new);
}

/// cuTensorNet's QR splits its input's modes between its two factors, and
/// sums none: a caller whose input carries a mode that neither factor
/// keeps (a bond left on the input) is refused rather than summed.
#[test]
fn a_qr_splits_its_input_modes_between_its_two_factors() {
    a_qr_splits_its_input_modes_between_its_two_factors_on(&fake);
}

/// Every object is used only with its owner: a descriptor or config with
/// its handle, an optimizer info with its network. Its cases run on the
/// fake alone: cuTensorNet does not check an object's owner, so on a GPU
/// the misuse is undefined behaviour.
#[test]
fn an_object_is_used_only_with_its_owner() {
    a_gate_split_is_sized_under_a_config_of_its_handle();
    a_contraction_path_is_searched_with_an_info_of_its_network();
    a_contraction_workspace_is_sized_with_an_info_of_its_network();
    a_tensor_call_refuses_an_object_of_another_handle();
}

/// A contraction is optimized, sized, prepared with its device scratch
/// lent, and contracted, in that order, and again from its optimization
/// after a new path search.
#[test]
fn a_contraction_follows_its_steps_in_order() {
    a_contraction_follows_its_steps_in_order_on(&fake);
}

/// The cases of [`a_contraction_follows_its_steps_in_order`], on the
/// devices `new` makes.
fn a_contraction_follows_its_steps_in_order_on<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    a_contraction_needs_an_optimized_path(new);
    a_contraction_is_prepared_in_a_workspace_sized_for_its_path(new);
    a_contraction_is_prepared_with_its_device_scratch_lent(new);
    a_contraction_needs_at_least_the_scratch_lent_at_its_preparation(new);
    a_contraction_needs_a_network_prepared_for_its_path(new);
}

/// A path search makes the sizings for its network's old path stale, in
/// every workspace, and no other sizing. Its cases run on the fake alone:
/// on a GPU, a workspace sized for an old path is used unchecked.
#[test]
fn a_path_search_invalidates_only_its_networks_sizings() {
    a_new_path_search_needs_the_workspace_sized_again();
    a_new_path_search_needs_every_workspace_sized_again();
    a_path_search_keeps_the_sizing_for_a_qr();
    a_path_search_keeps_the_sizing_for_another_network();
}

/// A contraction's labels are consistent: each output label is an input
/// label, and each label has one extent.
#[test]
fn a_contractions_labels_are_consistent() {
    a_contractions_labels_are_consistent_on(&fake);
}

/// The cases of [`a_contractions_labels_are_consistent`], on the devices
/// `new` makes.
fn a_contractions_labels_are_consistent_on<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    a_contraction_output_label_must_be_an_input_label(new);
    a_contraction_label_has_one_extent(new);
}

/// A workspace holds the sizes of the call it was last sized for, and a
/// tensor call runs in a workspace sized for it: its config, its
/// descriptors, its kind, its network. Its cases run on the fake alone: on
/// a GPU, a workspace sized for another call is used unchecked.
#[test]
fn a_tensor_call_runs_in_a_workspace_sized_for_it() {
    a_gate_split_needs_a_workspace_sized_for_its_config();
    a_gate_split_needs_a_workspace_sized_for_its_descriptors();
    a_qr_needs_a_workspace_sized_for_it();
    a_contraction_needs_a_workspace_sized_for_its_path();
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

/// A stream is synchronized, and a tensor call queued on it, only while
/// it is live. Its cases run on the fake alone: on a GPU, a destroyed
/// stream is undefined behaviour.
#[test]
fn a_stream_is_used_only_while_live() {
    a_stream_is_synchronized_only_while_live();
    a_tensor_call_runs_on_a_live_stream();
}

/// A discarded-weight cutoff is a fraction of the total weight: a caller
/// that passes one outside [0, 1] is refused.
#[test]
fn an_svd_cutoff_outside_zero_to_one_is_refused() {
    an_svd_cutoff_outside_zero_to_one_is_refused_on(&fake);
}

/// cuTensorNet checks a descriptor's shape when it is created: one extent
/// per mode.
fn a_tensor_descriptor_needs_one_extent_per_mode<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let device = new();
    let create = |extents: &[i64], modes: &[i32]| {
        device
            .calls
            .create_tensor_descriptor(device.handle, TensorModes { extents, modes })
    };
    create(&[2, 3], &[1]).expect_err("two extents for one mode");
    create(&[2, 3], &[1, 2]).expect("an extent for each mode");
}

/// A mode of extent 0 holds no element: a caller whose bond was sized to
/// nothing is caught when it creates the descriptor.
fn a_tensor_descriptor_refuses_an_extent_below_one<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let device = new();
    let create = |extents: &[i64]| {
        device.calls.create_tensor_descriptor(
            device.handle,
            TensorModes {
                extents,
                modes: &[1, 2],
            },
        )
    };
    create(&[2, 0]).expect_err("the second mode has extent 0");
    create(&[2, 1]).expect("every extent is at least 1");
}

/// A label names one mode of a tensor: a caller that gives a tensor one
/// label twice is caught when it creates the descriptor.
fn a_tensor_descriptor_refuses_a_repeated_mode<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let device = new();
    let create = |modes: &[i32]| {
        device.calls.create_tensor_descriptor(
            device.handle,
            TensorModes {
                extents: &[2, 2, 2],
                modes,
            },
        )
    };
    create(&[1, 2, 1]).expect_err("the label 1 names two modes");
    create(&[1, 2, 3]).expect("every label names one mode");
}

/// cuTensorNet's shared extent "shall not exceed" the smaller side of the
/// split: a caller that sizes U and V at its bond cap without clamping it
/// to θ's sides is refused. A 2×2 θ splits at an extent of 2,
/// and is refused at 3, under a cutoff that would otherwise reduce it.
fn a_gate_split_needs_a_shared_extent_within_the_smaller_side<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    let theta = matrix([[2.0, 1.0], [0.5, -1.5]]);
    GateSplit::on(new(), theta)
        .split()
        .expect("the shared extent is the smaller side");
    let zero = Complex64::default();
    let mut above = GateSplit::on(new(), theta);
    let device = &above.device;
    above.operands.u = device.operand(&[2, 3], &[P_OUT, S], &[zero; 6]);
    above.operands.v = device.operand(&[3, 2], &[S, Q_OUT], &[zero; 6]);
    device
        .calls
        .synchronize_copies()
        .expect("the copies were queued");
    let error = above
        .split()
        .expect_err("the shared extent of 3 exceeds θ's 2×2");
    assert!(
        refused_for::<C>(&error, "exceeds the split's smaller side"),
        "{error}"
    );
}

/// The QR is thin: a caller whose descriptors share more than the smaller
/// side (4 rows, 3 columns), or less, is refused.
fn a_qr_needs_the_shared_extent_to_be_the_smaller_side<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    for extent in [4, 2] {
        let refused = qr_on(new(), extent, |device| device.stream)
            .expect_err("Q and R do not share the smaller side");
        assert!(
            refused_for::<C>(&refused, "not the smaller side"),
            "{extent}: {refused}"
        );
    }
    qr_on(new(), 3, |device| device.stream).expect("Q and R share the smaller side");
}

/// A gate split's U and V share one mode, which no input carries: a caller
/// that labels A's and B's bond as the shared mode, which the split would
/// sum and keep at once, is refused.
fn a_gate_split_refuses_its_shared_mode_on_an_input<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let theta = matrix([[3.0, 0.0], [0.0, 1.0]]);
    let with_bond = |bond| {
        let mut split = GateSplit::on(new(), theta);
        let device = &split.device;
        split.operands.a = device.operand(&[2, 2], &[P, bond], &values(4, 0.5));
        split.operands.b = device.operand(&[2, 2], &[bond, Q], &values(4, -0.25));
        device
            .calls
            .synchronize_copies()
            .expect("the copies were queued");
        split
    };
    let refused = with_bond(S)
        .split()
        .expect_err("A's and B's bond is labelled as U's and V's shared mode");
    assert!(
        refused_for::<C>(&refused, "outside a gate split's topology"),
        "{refused}"
    );
    with_bond(BOND)
        .split()
        .expect("A's and B's bond has its own label");
}

/// A gate split contracts A, B and the gate along the labels they share,
/// and keeps the others in U or V: a caller whose gate carries a label no
/// other tensor has, which the split would sum alone, is refused.
fn a_gate_split_refuses_a_label_outside_its_topology<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let theta = matrix([[3.0, 0.0], [0.0, 1.0]]);
    let mut split = GateSplit::on(new(), theta);
    split.split().expect("every label is shared or kept");
    let extra_label = 7;
    let mut gate = values(16, 0.25);
    gate[..4].copy_from_slice(&theta);
    split.operands.gate =
        split
            .device
            .operand(&[2, 2, 2, 2, 1], &[P_OUT, Q_OUT, P, Q, extra_label], &gate);
    split
        .device
        .calls
        .synchronize_copies()
        .expect("the copies were queued");
    let refused = split
        .split()
        .expect_err("the gate's extra label is in no other tensor");
    assert!(
        refused_for::<C>(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// cuTensorNet's gate split takes A, B and the gate as a fully connected
/// graph: a caller whose A and B share no bond, so that the split is not a
/// two-site gate on neighbouring sites, or whose gate acts on one site
/// alone, sharing no label with B's site or none with A's, is refused.
fn a_gate_split_needs_a_b_and_the_gate_pairwise_connected<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    let not_connected = "not pairwise connected";
    split_with_labels(new, &[P, BOND], &[BOND, Q], &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect("each pair shares a label");
    let refused = split_with_labels(new, &[P], &[Q], &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect_err("A and B share no label");
    assert!(refused_for::<C>(&refused, not_connected), "{refused}");
    let refused = split_with_labels(
        new,
        &[P, BOND],
        &[BOND, Q],
        &[Q_OUT, Q],
        &[P, S],
        &[S, Q_OUT],
    )
    .expect_err("A and the gate share no label");
    assert!(refused_for::<C>(&refused, not_connected), "{refused}");
    let refused = split_with_labels(
        new,
        &[P, BOND],
        &[BOND, Q],
        &[P_OUT, P],
        &[P_OUT, S],
        &[S, Q],
    )
    .expect_err("B and the gate share no label");
    assert!(refused_for::<C>(&refused, not_connected), "{refused}");
}

/// A gate split contracts each label two inputs share: a caller whose A, B
/// and gate all carry one label, which the split could neither contract
/// pairwise nor keep, is refused.
fn a_gate_split_refuses_a_label_on_all_three_inputs<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let hyperedge = OUTER + 1;
    split_with_labels(new, &[P, BOND], &[BOND, Q], &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect("each label is on one or two inputs");
    let refused = split_with_labels(
        new,
        &[P, BOND, hyperedge],
        &[BOND, Q, hyperedge],
        &[P_OUT, Q_OUT, P, Q, hyperedge],
        &[P_OUT, S],
        &[S, Q_OUT],
    )
    .expect_err("a label is on all three inputs");
    assert!(
        refused_for::<C>(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// A gate split contracts the labels two inputs share: a caller that also
/// keeps A's and B's bond in U, where the split would sum it and keep it at
/// once, is refused.
fn a_gate_split_refuses_a_contracted_label_kept_in_an_output<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    let (a, b) = ([P, BOND], [BOND, Q]);
    split_with_labels(new, &a, &b, &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect("A's and B's bond is contracted only");
    let refused = split_with_labels(new, &a, &b, &GATE, &[P_OUT, BOND, S], &[S, Q_OUT])
        .expect_err("A's and B's bond is also kept in U");
    assert!(
        refused_for::<C>(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// U's and V's shared mode is made by the split: a caller that also labels
/// a mode of A alone with it, which no output could keep, is refused.
fn a_gate_split_refuses_its_shared_mode_on_a_single_input<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    let b = [BOND, Q];
    split_with_labels(
        new,
        &[P, BOND, OUTER],
        &b,
        &GATE,
        &[OUTER, P_OUT, S],
        &[S, Q_OUT],
    )
    .expect("A's outer bond has its own label");
    let refused = split_with_labels(new, &[P, BOND, S], &b, &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect_err("A's outer bond is labelled as U's and V's shared mode");
    assert!(
        refused_for::<C>(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// A gate split keeps A's site in U: a caller that puts a label of A alone
/// (its outer bond) in V is refused.
fn a_gate_split_keeps_a_label_of_a_alone_in_u<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let (a, b) = ([OUTER, P, BOND], [BOND, Q]);
    split_with_labels(new, &a, &b, &GATE, &[OUTER, P_OUT, S], &[S, Q_OUT])
        .expect("A's outer bond is in U");
    let refused = split_with_labels(new, &a, &b, &GATE, &[P_OUT, S], &[S, Q_OUT, OUTER])
        .expect_err("A's outer bond is in V");
    assert!(
        refused_for::<C>(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// A gate split keeps B's site in V: a caller that puts a label of B alone
/// (its outer bond) in U is refused.
fn a_gate_split_keeps_a_label_of_b_alone_in_v<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let (a, b) = ([P, BOND], [BOND, Q, OUTER]);
    split_with_labels(new, &a, &b, &GATE, &[P_OUT, S], &[S, Q_OUT, OUTER])
        .expect("B's outer bond is in V");
    let refused = split_with_labels(new, &a, &b, &GATE, &[OUTER, P_OUT, S], &[S, Q_OUT])
        .expect_err("B's outer bond is in U");
    assert!(
        refused_for::<C>(&refused, "outside a gate split's topology"),
        "{refused}"
    );
}

/// A gate's outputs belong to neither site until the split places them: a
/// caller that keeps the gate's second output in U and its first in V, as
/// a swap of the two sites does, gets its split.
fn a_gate_split_keeps_a_label_of_the_gate_alone_on_either_side<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    let (a, b) = ([P, BOND], [BOND, Q]);
    split_with_labels(new, &a, &b, &GATE, &[P_OUT, S], &[S, Q_OUT])
        .expect("the gate's first output is in U, its second in V");
    split_with_labels(new, &a, &b, &GATE, &[Q_OUT, S], &[S, P_OUT])
        .expect("the gate's second output is in U, its first in V");
}

/// A site of a product state, or one at the chain's end, has bonds of
/// extent 1: a caller whose every mode, the gate's too, has extent 1 still
/// gets the product of A, B and the gate, at a reduced and full extent of
/// 1.
fn a_gate_split_takes_modes_of_extent_one<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let device = new();
    let config = device
        .calls
        .create_svd_config(device.handle)
        .expect("the handle is live");
    for setting in ALGEBRA_SETTINGS {
        device
            .calls
            .configure_svd(device.handle, config, setting)
            .expect("the setting is in range");
    }
    let info = device
        .calls
        .create_svd_info(device.handle)
        .expect("the handle is live");
    let a_value = Complex64::new(1.0, 2.0);
    let b_value = Complex64::new(0.5, -0.5);
    let gate_value = Complex64::new(-1.0, 0.25);
    let zero = Complex64::default();
    let operands = GateSplitOperands {
        a: device.operand(&[1, 1], &[P, BOND], &[a_value]),
        b: device.operand(&[1, 1], &[BOND, Q], &[b_value]),
        gate: device.operand(&[1, 1, 1, 1], &[P_OUT, Q_OUT, P, Q], &[gate_value]),
        u: device.operand(&[1, 1], &[P_OUT, S], &[zero]),
        v: device.operand(&[1, 1], &[S, Q_OUT], &[zero]),
        singular_values: device.buffer(&[zero]),
    };
    device
        .calls
        .synchronize_copies()
        .expect("the copies were queued");
    let descriptors = GateSplitDescriptors {
        a: operands.a.descriptor,
        b: operands.b.descriptor,
        gate: operands.gate.descriptor,
        u: operands.u.descriptor,
        v: operands.v.descriptor,
    };
    device
        .calls
        .gate_split_workspace_sizes(device.handle, &descriptors, config, device.workspace)
        .expect("the descriptors and config are live");
    device.lend(WorkspaceSlot::DeviceScratch);
    device
        .calls
        .gate_split(
            device.handle,
            &operands,
            config,
            info,
            device.workspace,
            device.stream,
        )
        .expect("every label is shared or kept on its side");
    let (u, v) = (
        device.read(operands.u.data, 1),
        device.read(operands.v.data, 1),
    );
    let theta = a_value * b_value * gate_value;
    assert_close(&[u[0] * v[0]], &[theta / theta.norm()]);
    let report = device
        .calls
        .svd_truncation(device.handle, info)
        .expect("the stream is synchronized");
    assert_eq!(
        (
            report.reduced_extent,
            report.full_extent,
            report.discarded_weight
        ),
        (1, 1, 0.0)
    );
}

/// The cases of [`a_qr_splits_its_input_modes_between_its_two_factors`],
/// on the devices `new` makes.
fn a_qr_splits_its_input_modes_between_its_two_factors_on<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    const ELSEWHERE: i32 = 9;
    let run = |extents: &[i64], modes: &[i32]| {
        let qr = TensorQr::described(new(), 3, extents, modes);
        qr.size()?;
        qr.device.lend(WorkspaceSlot::DeviceScratch);
        qr.run_on(qr.device.stream)
    };
    run(&[2, 2, 3, 2], &[QR_I, QR_K, QR_J, ELSEWHERE])
        .expect_err("the input carries a mode neither factor keeps");
    run(&[2, 2, 3], &[QR_I, QR_K, QR_J]).expect("each input mode is kept by one factor");
}

/// A caller that sizes a workspace for, or prepares, a contraction before
/// its network's path is optimized is refused.
fn a_contraction_needs_an_optimized_path<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let device = new();
    device.lend(WorkspaceSlot::DeviceScratch);
    let (calls, handle, workspace) = (&*device.calls, device.handle, device.workspace);
    let network = calls
        .create_contraction_network(handle, DOT_PRODUCT_INPUTS, DOT_PRODUCT_OUTPUT.modes)
        .expect("the labels are consistent")
        .descriptor;
    let info = calls
        .create_optimizer_info(handle, network)
        .expect("the network is live");
    calls
        .contraction_workspace_sizes(handle, network, info, workspace)
        .expect_err("the path is not optimized");
    calls
        .prepare_contraction(handle, network, workspace)
        .expect_err("the path is not optimized");
    calls
        .optimize_contraction(handle, network, 1 << 20, info)
        .expect("the info is the network's");
    calls
        .contraction_workspace_sizes(handle, network, info, workspace)
        .expect("the path is optimized");
    device.lend(WorkspaceSlot::DeviceScratch);
    calls
        .prepare_contraction(handle, network, workspace)
        .expect("the workspace is sized for the path");
}

/// cuTensorNet prepares a network in the workspace sized for its path: a
/// caller that prepares an optimized network in a workspace it never sized
/// is refused.
fn a_contraction_is_prepared_in_a_workspace_sized_for_its_path<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    let device = new();
    device.lend(WorkspaceSlot::DeviceScratch);
    let (calls, handle, workspace) = (&*device.calls, device.handle, device.workspace);
    let network = calls
        .create_contraction_network(handle, DOT_PRODUCT_INPUTS, DOT_PRODUCT_OUTPUT.modes)
        .expect("the labels are consistent")
        .descriptor;
    let info = calls
        .create_optimizer_info(handle, network)
        .expect("the network is live");
    calls
        .optimize_contraction(handle, network, 1 << 20, info)
        .expect("the info is the network's");
    calls
        .prepare_contraction(handle, network, workspace)
        .expect_err("the workspace is not sized");
    calls
        .contraction_workspace_sizes(handle, network, info, workspace)
        .expect("the path is optimized");
    device.lend(WorkspaceSlot::DeviceScratch);
    calls
        .prepare_contraction(handle, network, workspace)
        .expect("the workspace is sized for the path");
}

/// cuTensorNet checks the device scratch's size when it prepares a
/// network: a caller that prepares before lending the workspace its memory
/// is refused.
fn a_contraction_is_prepared_with_its_device_scratch_lent<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    let ones = [Complex64::new(1.0, 0.0); 2];
    let contraction = Contraction::sized(
        new(),
        DOT_PRODUCT_INPUTS,
        [&ones, &ones],
        DOT_PRODUCT_OUTPUT,
    );
    contraction
        .prepare()
        .expect_err("the device scratch is not lent");
    contraction.device.lend(WorkspaceSlot::DeviceScratch);
    contraction
        .prepare()
        .expect("the device scratch is lent what it asks for");
}

/// cuTensorNet contracts with at least the device scratch it was prepared
/// with: a caller that prepares a network with a large scratch, then lends
/// a smaller one to contract it, is refused.
fn a_contraction_needs_at_least_the_scratch_lent_at_its_preparation<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    let ones = [Complex64::new(1.0, 0.0); 2];
    let contraction = Contraction::sized(
        new(),
        DOT_PRODUCT_INPUTS,
        [&ones, &ones],
        DOT_PRODUCT_OUTPUT,
    );
    let device = &contraction.device;
    let asked = device.asked(WorkspaceSlot::DeviceScratch);
    assert!(asked > 0, "the contraction asks for device scratch");
    device.lend_bytes(WorkspaceSlot::DeviceScratch, 2 * asked);
    contraction
        .prepare()
        .expect("the device scratch is lent more than it asks for");
    device.lend_bytes(WorkspaceSlot::DeviceScratch, 2 * asked - 1);
    contraction
        .contract()
        .expect_err("the device scratch is one byte short of the preparation's");
    device.lend_bytes(WorkspaceSlot::DeviceScratch, 2 * asked);
    contraction
        .contract()
        .expect("the device scratch is the preparation's");
}

/// cuTensorNet contracts a network as its last preparation set it up, for
/// the path it had then: a caller that contracts an optimized network it
/// never prepared, or did not prepare again after optimizing it again
/// (sizing the workspace again, so that only the preparation is missing),
/// is refused.
fn a_contraction_needs_a_network_prepared_for_its_path<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    let ones = [Complex64::new(1.0, 0.0); 2];
    let contraction = Contraction::sized(
        new(),
        DOT_PRODUCT_INPUTS,
        [&ones, &ones],
        DOT_PRODUCT_OUTPUT,
    );
    let device = &contraction.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    let refused = contraction
        .contract()
        .expect_err("the network is not prepared");
    assert!(refused_for::<C>(&refused, "not prepared"), "{refused}");
    contraction
        .prepare()
        .expect("the workspace is sized for the path");
    contraction.contract().expect("the network is prepared");
    let network = contraction.network.descriptor;
    device
        .calls
        .optimize_contraction(device.handle, network, 1 << 20, contraction.info)
        .expect("the info is the network's");
    device
        .calls
        .contraction_workspace_sizes(device.handle, network, contraction.info, device.workspace)
        .expect("the path is optimized");
    let refused = contraction
        .contract()
        .expect_err("the network is not prepared for its new path");
    assert!(refused_for::<C>(&refused, "not prepared"), "{refused}");
    contraction
        .prepare()
        .expect("the workspace is sized for the path");
    contraction
        .contract()
        .expect("the network is prepared again");
}

/// cuTensorNet takes each output mode's extent from the inputs: a caller
/// whose output names a label no input has is refused.
fn a_contraction_output_label_must_be_an_input_label<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let device = new();
    device
        .calls
        .create_contraction_network(device.handle, DOT_PRODUCT_INPUTS, &[2])
        .expect_err("no input has the label 2");
    device
        .calls
        .create_contraction_network(device.handle, DOT_PRODUCT_INPUTS, &[1])
        .expect("both inputs have the label 1");
}

/// A label names one mode of the network: a caller whose two inputs give
/// one label two extents is refused.
fn a_contraction_label_has_one_extent<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let device = new();
    let inputs = |extent| {
        [
            DOT_PRODUCT_INPUTS[0],
            TensorModes {
                extents: extent,
                modes: &[1],
            },
        ]
    };
    device
        .calls
        .create_contraction_network(device.handle, inputs(&[3]), &[])
        .expect_err("the label 1 has the extents 2 and 3");
    device
        .calls
        .create_contraction_network(device.handle, inputs(&[2]), &[])
        .expect("the label 1 has the extent 2 in both inputs");
}

/// The cases of [`an_svd_cutoff_outside_zero_to_one_is_refused`], on the
/// devices `new` makes.
fn an_svd_cutoff_outside_zero_to_one_is_refused_on<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let device = new();
    let config = device
        .calls
        .create_svd_config(device.handle)
        .expect("the handle is live");
    let configure = |cutoff| {
        device.calls.configure_svd(
            device.handle,
            config,
            SvdSetting::DiscardedWeightCutoff(cutoff),
        )
    };
    configure(-0.1).expect_err("the cutoff is below 0");
    configure(1.5).expect_err("the cutoff is above 1");
    configure(0.0).expect("0 is a fraction");
    configure(1.0).expect("1 is a fraction");
}

/// A caller that lends device scratch alone is caught on the fake whether
/// it contracts or splits a gate, as when it runs a QR.
fn a_tensor_call_needs_memory_in_every_slot_that_asks_for_it() {
    let libraries = FakeLibraries::new();
    libraries.set_other_workspace_slot_bytes(64);
    let contraction = Contraction::dot_product(Device::new(libraries));
    needs_every_slot(&contraction.device, || contraction.contract());
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]));
    split.device.calls.set_other_workspace_slot_bytes(64);
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
    fake.set_other_workspace_slot_bytes(64);
    fake.set_workspace(handle, workspace, WorkspaceSlot::HostCache, memory, 64)
        .expect("the host cache asks for 64 bytes");
}

/// A tensor's element count can overflow the host's addresses, which no
/// buffer could hold: a caller whose extents multiply past `usize` is
/// refused with an error, not a panic.
fn a_tensor_whose_element_count_overflows_is_refused() {
    let device = fake();
    let zero = Complex64::default();
    let qr_of = |extent: i64, elements: usize| {
        let input = device.operand(&[extent, 3], &[1, 2], &values(elements, 0.5));
        let q = device.operand(&[extent, 3], &[1, 4], &vec![zero; elements]);
        let r = device.operand(&[3, 3], &[4, 2], &[zero; 9]);
        device
            .calls
            .synchronize_copies()
            .expect("the copies were queued");
        device.calls.qr_workspace_sizes(
            device.handle,
            input.descriptor,
            q.descriptor,
            r.descriptor,
            device.workspace,
        )?;
        device.lend(WorkspaceSlot::DeviceScratch);
        device
            .calls
            .tensor_qr(device.handle, input, q, r, device.workspace, device.stream)
    };
    let refused = qr_of(i64::MAX, 1).expect_err("the input's element count overflows");
    assert!(
        matches!(refused, CudaAlgebraError::ResourceSizeOverflow { .. }),
        "{refused:?}"
    );
    qr_of(3, 9).expect("each buffer holds its tensor's 9 elements");
}

/// Under a cutoff, cuTensorNet writes U and V before it knows the reduced
/// extent, so a caller that sizes their buffers by the bond it expects
/// overruns them on a GPU; the fake refuses it, for U and for V, even
/// where the reduced bond (1, for a rank-1 θ) would fit.
fn a_gate_split_needs_u_and_v_buffers_of_their_descriptors_full_extents() {
    let zero = Complex64::default();
    let mut split = GateSplit::of(matrix([[1.0, 1.0], [1.0, 1.0]]));
    let full = split.operands.u.data;
    let reduced = split.device.buffer(&[zero; 2]);
    split
        .device
        .calls
        .synchronize_copies()
        .expect("the copies were queued");
    split.operands.u.data = reduced;
    split
        .split()
        .expect_err("U's buffer holds the reduced bond only");
    split.operands.u.data = full;
    let full = split.operands.v.data;
    split.operands.v.data = reduced;
    split
        .split()
        .expect_err("V's buffer holds the reduced bond only");
    split.operands.v.data = full;
    split
        .split()
        .expect("U's and V's buffers hold the full extents");
}

/// cuTensorNet may write the singular values to their buffer, as many as
/// the shared mode's extent: a caller that sized it by an earlier, smaller
/// bond is caught on the fake. Two sites with outer bonds of 2 make θ 4×4,
/// split at a shared extent of 4.
fn a_gate_split_needs_a_singular_values_buffer_of_the_shared_extent() {
    const OUTER_RIGHT: i32 = 8;
    let zero = Complex64::default();
    let mut split = GateSplit::of(matrix([[2.0, 1.0], [0.5, -1.5]]));
    let device = &split.device;
    split.operands.a = device.operand(&[2, 2, 1], &[OUTER, P, BOND], &values(4, 0.5));
    split.operands.b = device.operand(&[1, 2, 2], &[BOND, Q, OUTER_RIGHT], &values(4, -0.25));
    split.operands.u = device.operand(&[2, 2, 4], &[OUTER, P_OUT, S], &[zero; 16]);
    split.operands.v = device.operand(&[4, 2, 2], &[S, Q_OUT, OUTER_RIGHT], &[zero; 16]);
    let bond_two = device.buffer(&[zero; 1]);
    let bond_four = device.buffer(&[zero; 2]);
    device
        .calls
        .synchronize_copies()
        .expect("the copies were queued");
    split.operands.singular_values = bond_two;
    split
        .split()
        .expect_err("the buffer holds 2 doubles of the 4 the split may write");
    split.operands.singular_values = bond_four;
    split.split().expect("the buffer holds 4 doubles");
}

/// cuTensorNet may write the singular values to their buffer: a caller
/// that freed it is caught on the fake.
fn a_gate_split_needs_a_live_singular_values_buffer() {
    let mut split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]));
    split
        .device
        .calls
        .free(split.operands.singular_values)
        .expect("the buffer is live");
    split
        .split()
        .expect_err("the singular values' buffer is freed");
    split.operands.singular_values = split.device.buffer(&[Complex64::default(); 2]);
    split
        .device
        .calls
        .synchronize_copies()
        .expect("the copies were queued");
    split.split().expect("the singular values' buffer is live");
}

/// A gate split is sized for its config: a caller that sizes it under an
/// SVD config of another handle is caught on the fake.
fn a_gate_split_is_sized_under_a_config_of_its_handle() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]));
    let fake = &split.device.calls;
    let other_handle = fake.create_handle().expect("the fake has a device");
    let foreign = fake
        .create_svd_config(other_handle)
        .expect("the other handle is live");
    split
        .size_under(foreign)
        .expect_err("the config belongs to another handle");
    split
        .size_under(split.config)
        .expect("the config belongs to the split's handle");
}

/// A caller that caches a path per shape and searches one network's path
/// into another's optimizer info, mixing up two cache entries, is caught
/// on the fake.
fn a_contraction_path_is_searched_with_an_info_of_its_network() {
    let device = fake();
    let (fake, handle) = (&*device.calls, device.handle);
    let [(first, first_info), (_, second_info)] = two_networks(&device, false);
    fake.optimize_contraction(handle, first, 1 << 20, second_info)
        .expect_err("the info is the second network's");
    fake.optimize_contraction(handle, first, 1 << 20, first_info)
        .expect("the info is the first network's");
}

/// A caller that sizes a workspace for one network with the optimized path
/// of another, mixing up two cache entries, is caught on the fake.
fn a_contraction_workspace_is_sized_with_an_info_of_its_network() {
    let device = fake();
    let (fake, handle, workspace) = (&*device.calls, device.handle, device.workspace);
    let [(first, first_info), (_, second_info)] = two_networks(&device, true);
    fake.contraction_workspace_sizes(handle, first, second_info, workspace)
        .expect_err("the info is the second network's");
    fake.contraction_workspace_sizes(handle, first, first_info, workspace)
        .expect("the info is the first network's");
}

/// Every object belongs to the handle it was created with: a caller that
/// mixes a descriptor of one handle into a call on another is caught on
/// the fake.
fn a_tensor_call_refuses_an_object_of_another_handle() {
    let device = fake();
    let (fake, handle) = (&*device.calls, device.handle);
    let other_handle = fake.create_handle().expect("the fake has a device");
    let descriptor = |handle, extents: &[i64], modes: &[i32]| {
        fake.create_tensor_descriptor(handle, TensorModes { extents, modes })
            .expect("the extents and modes match")
    };
    let q = descriptor(handle, &[2, 2], &[1, 3]);
    let r = descriptor(handle, &[2, 3], &[3, 2]);
    let foreign = descriptor(other_handle, &[2, 3], &[1, 2]);
    fake.qr_workspace_sizes(handle, foreign, q, r, device.workspace)
        .expect_err("the input belongs to another handle");
    let own = descriptor(handle, &[2, 3], &[1, 2]);
    fake.qr_workspace_sizes(handle, own, q, r, device.workspace)
        .expect("every descriptor belongs to the handle");
}

/// cuTensorNet's optimizer infos refer to their network: a caller that
/// destroys a network first is caught on the fake. On the fake alone: on
/// a GPU, the misuse is undefined behaviour.
#[test]
fn a_network_outlives_its_optimizer_infos() {
    let contraction = Contraction::dot_product(fake());
    let fake = &contraction.device.calls;
    fake.destroy_contraction_network(contraction.network.descriptor)
        .expect_err("its optimizer info is live");
    fake.destroy_optimizer_info(contraction.info)
        .expect("the info is live");
    fake.destroy_contraction_network(contraction.network.descriptor)
        .expect("nothing of the network is live");
}

/// cuTensorNet's optimizer may find another path at each search, even into
/// the same optimizer info, and a workspace sized for the old path may not
/// hold the new one's: a caller that searches a prepared network's path
/// again and prepares it without sizing the workspace again is caught on
/// the fake.
fn a_new_path_search_needs_the_workspace_sized_again() {
    let contraction = Contraction::dot_product(fake());
    let device = &contraction.device;
    let network = contraction.network.descriptor;
    device
        .calls
        .optimize_contraction(device.handle, network, 1 << 20, contraction.info)
        .expect("the info is the network's");
    let refused = contraction
        .prepare()
        .expect_err("the workspace is sized for the earlier search's path");
    assert!(
        refused_for::<FakeLibraries>(&refused, "not sized for this call"),
        "{refused}"
    );
    device
        .calls
        .contraction_workspace_sizes(device.handle, network, contraction.info, device.workspace)
        .expect("the path is optimized");
    contraction
        .prepare()
        .expect("the workspace is sized for the new path");
    contraction
        .contract()
        .expect("the network is prepared for its new path");
}

/// A new path search changes the path every workspace sized for the network
/// was sized for: a caller that keeps two workspaces sized for one network,
/// searches its path again and prepares it in either without sizing that
/// one again is caught on the fake.
fn a_new_path_search_needs_every_workspace_sized_again() {
    let contraction = Contraction::dot_product(fake());
    let device = &contraction.device;
    let (fake, handle) = (&*device.calls, device.handle);
    let network = contraction.network.descriptor;
    let second = fake.create_workspace(handle).expect("the handle is live");
    fake.contraction_workspace_sizes(handle, network, contraction.info, second)
        .expect("the path is optimized");
    let bytes = fake
        .workspace_size(handle, second, WorkspaceSlot::DeviceScratch)
        .expect("the workspace is live");
    let memory = fake
        .allocate(usize::try_from(bytes).expect("a size is not negative"))
        .expect("the fake has memory");
    fake.set_workspace(handle, second, WorkspaceSlot::DeviceScratch, memory, bytes)
        .expect("the buffer holds the bytes lent");
    fake.optimize_contraction(handle, network, 1 << 20, contraction.info)
        .expect("the info is the network's");
    for workspace in [device.workspace, second] {
        let refused = fake
            .prepare_contraction(handle, network, workspace)
            .expect_err("the workspace is sized for the earlier search's path");
        assert!(
            refused_for::<FakeLibraries>(&refused, "not sized for this call"),
            "{refused}"
        );
    }
    for workspace in [device.workspace, second] {
        fake.contraction_workspace_sizes(handle, network, contraction.info, workspace)
            .expect("the path is optimized");
        fake.prepare_contraction(handle, network, workspace)
            .expect("the workspace is sized for the new path");
    }
}

/// A path search changes its network's path, not a workspace's sizing for
/// a call of another kind: a caller that searches a new network's path
/// between sizing a QR and running it runs the QR without sizing again.
fn a_path_search_keeps_the_sizing_for_a_qr() {
    let qr = TensorQr::sized(fake(), 3);
    let device = &qr.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    two_networks(device, true);
    qr.run_on(device.stream)
        .expect("the path searches were for other networks");
}

/// A path search changes its own network's path alone: a caller that keeps
/// one network per shape, and searches a new shape's path between sizing a
/// workspace for one network and preparing it, prepares it without sizing
/// again.
fn a_path_search_keeps_the_sizing_for_another_network() {
    let ones = [Complex64::new(1.0, 0.0); 2];
    let contraction = Contraction::sized(
        fake(),
        DOT_PRODUCT_INPUTS,
        [&ones, &ones],
        DOT_PRODUCT_OUTPUT,
    );
    let device = &contraction.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    two_networks(device, true);
    contraction
        .prepare()
        .expect("the path searches were for other networks");
}

/// A workspace holds the sizes of the call it was last sized for: a caller
/// that runs a gate split under another SVD config, without sizing again,
/// is caught on the fake.
fn a_gate_split_needs_a_workspace_sized_for_its_config() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]));
    let device = &split.device;
    let other = device
        .calls
        .create_svd_config(device.handle)
        .expect("the handle is live");
    for setting in ALGEBRA_SETTINGS {
        device
            .calls
            .configure_svd(device.handle, other, setting)
            .expect("the setting is in range");
    }
    split.size(split.config);
    split
        .run(other, device.stream)
        .expect_err("the workspace is sized for the other config");
    split.size(other);
    split
        .run(other, device.stream)
        .expect("the workspace is sized for this config");
}

/// A workspace is sized for a gate split's descriptors, not their shapes: a
/// caller that sizes it with a new descriptor of U's shape, then runs the
/// split with U's own, is caught on the fake.
fn a_gate_split_needs_a_workspace_sized_for_its_descriptors() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]));
    let (device, operands) = (&split.device, &split.operands);
    let same_shape_as_u = device
        .calls
        .create_tensor_descriptor(
            device.handle,
            TensorModes {
                extents: &[2, 2],
                modes: &[P_OUT, S],
            },
        )
        .expect("the extents and modes match");
    device
        .calls
        .gate_split_workspace_sizes(
            device.handle,
            &GateSplitDescriptors {
                a: operands.a.descriptor,
                b: operands.b.descriptor,
                gate: operands.gate.descriptor,
                u: same_shape_as_u,
                v: operands.v.descriptor,
            },
            split.config,
            device.workspace,
        )
        .expect("the descriptors and config are live");
    device.lend(WorkspaceSlot::DeviceScratch);
    let refused = split
        .run(split.config, device.stream)
        .expect_err("the workspace is sized for another U descriptor");
    assert!(
        refused_for::<FakeLibraries>(&refused, "not sized for this call"),
        "{refused}"
    );
    split.size_workspace(split.config);
    split
        .run(split.config, device.stream)
        .expect("the workspace is sized for these descriptors");
}

/// A caller that shares one workspace between its tensor calls, sizes it
/// for a contraction, then runs a QR without sizing it again is caught on
/// the fake.
fn a_qr_needs_a_workspace_sized_for_it() {
    let qr = TensorQr::sized(fake(), 3);
    let device = &qr.device;
    device.lend(WorkspaceSlot::DeviceScratch);
    let [(network, info), _] = two_networks(device, true);
    device
        .calls
        .contraction_workspace_sizes(device.handle, network, info, device.workspace)
        .expect("the path is optimized");
    let refused = qr
        .run_on(device.stream)
        .expect_err("the workspace is sized for a contraction");
    assert!(
        refused_for::<FakeLibraries>(&refused, "not sized for this call"),
        "{refused}"
    );
    qr.size().expect("the descriptors are live");
    qr.run_on(device.stream)
        .expect("the workspace is sized for the QR");
}

/// A caller that keeps one prepared network per shape in one workspace,
/// sizes that workspace for another network, then contracts the first
/// without sizing again is caught on the fake.
fn a_contraction_needs_a_workspace_sized_for_its_path() {
    let contraction = Contraction::dot_product(fake());
    let device = &contraction.device;
    let [(other, other_info), _] = two_networks(device, true);
    device
        .calls
        .contraction_workspace_sizes(device.handle, other, other_info, device.workspace)
        .expect("the path is optimized");
    let refused = contraction
        .contract()
        .expect_err("the workspace is sized for another network");
    assert!(
        refused_for::<FakeLibraries>(&refused, "not sized for this call"),
        "{refused}"
    );
    device
        .calls
        .contraction_workspace_sizes(
            device.handle,
            contraction.network.descriptor,
            contraction.info,
            device.workspace,
        )
        .expect("the path is optimized");
    contraction
        .contract()
        .expect("the workspace is sized for the network's path");
}

/// A tensor call writes its output on its stream, which a copy to the host
/// does not wait for: a caller that copies a contraction's output before
/// synchronizing that stream, or after synchronizing only another one, is
/// caught on the fake.
fn a_copy_to_the_host_needs_the_writing_stream_synchronized() {
    let contraction = Contraction::dot_product(fake());
    let device = &contraction.device;
    let other_stream = device.calls.create_stream().expect("the fake has a device");
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
    let second = device.calls.create_stream().expect("the fake has a device");
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
}

/// cuTensorNet writes a gate split's report on the split's stream: a
/// caller that reads it before synchronizing that stream, or after
/// synchronizing only another one, is caught on the fake.
fn an_svd_info_is_read_once_its_writing_stream_is_synchronized() {
    let split = GateSplit::of(matrix([[3.0, 0.0], [0.0, 1.0]]));
    let device = &split.device;
    let other_stream = device.calls.create_stream().expect("the fake has a device");
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
    let qr_stream = device.calls.create_stream().expect("the fake has a device");
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
    let second = device.calls.create_stream().expect("the fake has a device");
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
    let second = device.calls.create_stream().expect("the fake has a device");
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

/// The tables that run on a GPU, on cuTensorNet: a case the fake refuses
/// and the library accepts shows a rule of the fake wider than the
/// library's. Their refusals are not matched by reason, which the
/// library's statuses do not name.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod on_the_gpu {
    use super::{
        Device, TestedCalls, a_contraction_follows_its_steps_in_order_on,
        a_contractions_labels_are_consistent_on,
        a_factorizations_bond_is_at_most_its_smaller_side_on,
        a_gate_split_takes_only_its_topologys_labels_on, a_malformed_descriptor_is_refused_on,
        a_qr_splits_its_input_modes_between_its_two_factors_on,
        a_workspace_slot_is_lent_what_it_asks_on, an_svd_cutoff_outside_zero_to_one_is_refused_on,
    };
    use crate::{engine::native_calls::SessionCalls, libraries::LoadedLibraries};
    use std::sync::Arc;

    impl TestedCalls for LoadedLibraries {
        const NAMES_ITS_RULE: bool = false;
    }

    /// Runs `table` on devices of the libraries discovery opens, on CUDA
    /// device 0.
    fn on_the_gpu(table: fn(&dyn Fn() -> Device<LoadedLibraries>)) {
        let libraries = Arc::clone(
            crate::discover()
                .expect("this host has a CUDA device and the libraries")
                .libraries(),
        );
        table(&|| {
            libraries.set_device(0).expect("the host has device 0");
            Device::on(Arc::clone(&libraries))
        });
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_workspace_slot_is_lent_what_it_asks() {
        on_the_gpu(a_workspace_slot_is_lent_what_it_asks_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_malformed_descriptor_is_refused() {
        on_the_gpu(a_malformed_descriptor_is_refused_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_factorizations_bond_is_at_most_its_smaller_side() {
        on_the_gpu(a_factorizations_bond_is_at_most_its_smaller_side_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_gate_split_takes_only_its_topologys_labels() {
        on_the_gpu(a_gate_split_takes_only_its_topologys_labels_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_qr_splits_its_input_modes_between_its_two_factors() {
        on_the_gpu(a_qr_splits_its_input_modes_between_its_two_factors_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_contraction_follows_its_steps_in_order() {
        on_the_gpu(a_contraction_follows_its_steps_in_order_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn a_contractions_labels_are_consistent() {
        on_the_gpu(a_contractions_labels_are_consistent_on);
    }

    #[test]
    #[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
    fn an_svd_cutoff_outside_zero_to_one_is_refused() {
        on_the_gpu(an_svd_cutoff_outside_zero_to_one_is_refused_on);
    }
}

/// The native calls a table runs on: the fake libraries in every host test,
/// the loaded libraries in the GPU tests (`on_the_gpu`).
trait TestedCalls: NativeCalls {
    /// Whether a refusal names the rule it enforces, as the fake's do, so
    /// that a case checks it was refused for that rule; cuTensorNet's
    /// statuses name none.
    const NAMES_ITS_RULE: bool;
}

impl TestedCalls for FakeLibraries {
    const NAMES_ITS_RULE: bool = true;
}

/// Whether `refused` is a refusal for `reason`, on calls that name their
/// rule; any refusal, on calls that do not.
fn refused_for<C: TestedCalls>(refused: &CudaAlgebraError, reason: &str) -> bool {
    !C::NAMES_ITS_RULE || refused.to_string().contains(reason)
}

/// What every tensor call needs: a handle, a stream and a workspace, made
/// through `calls`.
struct Device<C = FakeLibraries> {
    calls: Arc<C>,
    handle: OpaqueHandle,
    stream: Stream,
    workspace: OpaqueHandle,
    /// The host memory lent to host slots, kept as long as the device.
    host_memory: RefCell<Vec<Vec<u8>>>,
}

/// A device on new fake libraries.
fn fake() -> Device {
    Device::new(FakeLibraries::new())
}

impl Device {
    fn new(fake: FakeLibraries) -> Self {
        Self::on(Arc::new(fake))
    }
}

impl<C: TestedCalls> Device<C> {
    /// A device whose handle, stream and workspace `calls` makes, on the
    /// device the calling thread has selected.
    fn on(calls: Arc<C>) -> Self {
        let handle = calls.create_handle().expect("a device is selected");
        let stream = calls.create_stream().expect("a device is selected");
        let workspace = calls.create_workspace(handle).expect("the handle is live");
        Self {
            calls,
            handle,
            stream,
            workspace,
            host_memory: RefCell::default(),
        }
    }

    /// A device buffer holding `values`, once the copies are synchronized.
    fn buffer(&self, values: &[Complex64]) -> OpaqueHandle {
        let buffer = self
            .calls
            .allocate(values.len() * size_of::<Complex64Abi>())
            .expect("the device has memory");
        self.load(buffer, values);
        buffer
    }

    /// Copies `values` to the start of `buffer`.
    fn load(&self, buffer: OpaqueHandle, values: &[Complex64]) {
        let values: Vec<Complex64Abi> = values.iter().map(|&value| value.into()).collect();
        self.calls
            .copy_to_device(buffer, &values)
            .expect("the buffer holds the values");
    }

    /// A descriptor of `extents` and `modes`, and a buffer holding `values`.
    fn operand(&self, extents: &[i64], modes: &[i32], values: &[Complex64]) -> TensorOperand {
        TensorOperand {
            descriptor: self
                .calls
                .create_tensor_descriptor(self.handle, TensorModes { extents, modes })
                .expect("the extents and modes match"),
            data: self.buffer(values),
        }
    }

    /// Lends `slot` the bytes the workspace asks for, if it asks for any:
    /// a slot that asks for nothing is lent nothing.
    fn lend(&self, slot: WorkspaceSlot) {
        let asked = self.asked(slot);
        if asked > 0 {
            self.lend_bytes(slot, asked);
        }
    }

    /// Lends every slot but `unlent` what it asks for.
    fn lend_all_but(&self, unlent: WorkspaceSlot) {
        for slot in WORKSPACE_SLOTS {
            if slot != unlent {
                self.lend(slot);
            }
        }
    }

    /// The bytes the workspace asks for in `slot`.
    fn asked(&self, slot: WorkspaceSlot) -> i64 {
        self.calls
            .workspace_size(self.handle, self.workspace, slot)
            .expect("the workspace is live")
    }

    /// Lends `slot` `bytes` of memory.
    fn lend_bytes(&self, slot: WorkspaceSlot, bytes: i64) {
        self.try_lend_bytes(slot, bytes)
            .expect("the buffer holds the bytes lent");
    }

    /// Lends `slot` `bytes` of memory where the slot's memory lives: a
    /// device allocation for a device slot, host memory for a host slot.
    fn try_lend_bytes(&self, slot: WorkspaceSlot, bytes: i64) -> Result<(), CudaAlgebraError> {
        let size = usize::try_from(bytes).expect("a size is not negative");
        let memory = match slot {
            WorkspaceSlot::DeviceScratch | WorkspaceSlot::DeviceCache => {
                self.calls.allocate(size).expect("the device has memory")
            }
            WorkspaceSlot::HostScratch | WorkspaceSlot::HostCache => self.host(size),
        };
        self.calls
            .set_workspace(self.handle, self.workspace, slot, memory, bytes)
    }

    /// `bytes` of host memory, kept as long as the device.
    fn host(&self, bytes: usize) -> OpaqueHandle {
        let mut memory = vec![0_u8; bytes.max(1)];
        let start = NonNull::from(memory.as_mut_slice()).cast();
        self.host_memory.borrow_mut().push(memory);
        start
    }

    /// A buffer of zeros for a tensor of `extents`, once the copies are
    /// synchronized.
    fn zeros(&self, extents: &[i64]) -> OpaqueHandle {
        let elements = extents.iter().product::<i64>();
        self.buffer(&vec![
            Complex64::default();
            usize::try_from(elements).unwrap_or(0)
        ])
    }

    /// The first `elements` values of `buffer`, once the stream is done.
    fn read(&self, buffer: OpaqueHandle, elements: usize) -> Vec<Complex64> {
        self.calls
            .synchronize_stream(self.stream)
            .expect("the stream is live");
        let mut values = vec![Complex64Abi::default(); elements];
        self.calls
            .copy_to_host(&mut values, buffer)
            .expect("the buffer holds the values");
        values.into_iter().map(Complex64::from).collect()
    }
}

/// A contraction of two tensors: its device, network and buffers.
struct Contraction<C = FakeLibraries> {
    device: Device<C>,
    network: ContractionNetwork,
    info: OpaqueHandle,
    inputs: [OpaqueHandle; 2],
    output: OpaqueHandle,
}

impl<C: TestedCalls> Contraction<C> {
    /// The network of `inputs`, holding `data`, and `output`, on `device`,
    /// optimized, in a workspace sized for its path, lent nothing yet; not
    /// prepared.
    fn sized(
        device: Device<C>,
        inputs: [TensorModes<'_>; 2],
        data: [&[Complex64]; 2],
        output: TensorModes<'_>,
    ) -> Self {
        let (calls, handle) = (&device.calls, device.handle);
        let network = calls
            .create_contraction_network(handle, inputs, output.modes)
            .expect("the labels are consistent");
        let info = calls
            .create_optimizer_info(handle, network.descriptor)
            .expect("the network is live");
        calls
            .optimize_contraction(handle, network.descriptor, 1 << 20, info)
            .expect("the info is the network's");
        calls
            .contraction_workspace_sizes(handle, network.descriptor, info, device.workspace)
            .expect("the path is optimized");
        let inputs = [device.buffer(data[0]), device.buffer(data[1])];
        let output = device.zeros(output.extents);
        calls.synchronize_copies().expect("the copies were queued");
        Self {
            device,
            network,
            info,
            inputs,
            output,
        }
    }

    /// The dot product of two vectors of ones, of 2 elements, as
    /// [`sized`](Self::sized).
    fn dot_product_sized(device: Device<C>) -> Self {
        let ones = [Complex64::new(1.0, 0.0); 2];
        Self::sized(
            device,
            DOT_PRODUCT_INPUTS,
            [&ones, &ones],
            DOT_PRODUCT_OUTPUT,
        )
    }

    /// As [`dot_product_sized`](Self::dot_product_sized), its device
    /// scratch lent, and prepared.
    fn dot_product(device: Device<C>) -> Self {
        let contraction = Self::dot_product_sized(device);
        contraction.device.lend(WorkspaceSlot::DeviceScratch);
        contraction
            .prepare()
            .expect("the workspace is sized for the path and lent");
        contraction
    }

    fn prepare(&self) -> Result<(), CudaAlgebraError> {
        let device = &self.device;
        device
            .calls
            .prepare_contraction(device.handle, self.network.descriptor, device.workspace)
    }

    fn contract(&self) -> Result<(), CudaAlgebraError> {
        self.contract_on(self.device.stream)
    }

    fn contract_on(&self, stream: Stream) -> Result<(), CudaAlgebraError> {
        self.contract_buffers(self.inputs, self.output, stream)
    }

    fn contract_buffers(
        &self,
        inputs: [OpaqueHandle; 2],
        output: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), CudaAlgebraError> {
        let device = &self.device;
        device.calls.contract(
            device.handle,
            self.network,
            inputs,
            output,
            device.workspace,
            stream,
        )
    }
}

/// The inputs of a dot product of two vectors of 2 elements.
const DOT_PRODUCT_INPUTS: [TensorModes<'static>; 2] = [
    TensorModes {
        extents: &[2],
        modes: &[1],
    },
    TensorModes {
        extents: &[2],
        modes: &[1],
    },
];

/// The output of a dot product: a scalar.
const DOT_PRODUCT_OUTPUT: TensorModes<'static> = TensorModes {
    extents: &[],
    modes: &[],
};

/// Two dot-product networks on one handle, each with its optimizer info;
/// `optimized` says whether each info's path is searched.
fn two_networks<C: TestedCalls>(
    device: &Device<C>,
    optimized: bool,
) -> [(OpaqueHandle, OpaqueHandle); 2] {
    let (calls, handle) = (&device.calls, device.handle);
    [(); 2].map(|()| {
        let network = calls
            .create_contraction_network(handle, DOT_PRODUCT_INPUTS, DOT_PRODUCT_OUTPUT.modes)
            .expect("the labels are consistent")
            .descriptor;
        let info = calls
            .create_optimizer_info(handle, network)
            .expect("the network is live");
        if optimized {
            calls
                .optimize_contraction(handle, network, 1 << 20, info)
                .expect("the info is the network's");
        }
        (network, info)
    })
}

// The labels of a gate split's modes: A[p, b], B[b, q] and the gate
// G[p', q', p, q] in, U[p', s] and V[s, q'] out.
const P: i32 = 1;
const BOND: i32 = 2;
const Q: i32 = 3;
const P_OUT: i32 = 4;
const Q_OUT: i32 = 5;
const S: i32 = 6;
/// An outer bond of A or B, kept in U or V.
const OUTER: i32 = 7;
/// The gate of a two-site split, G[p', q', p, q].
const GATE: [i32; 4] = [P_OUT, Q_OUT, P, Q];
/// The settings `CudaAlgebra` gives a gate split's SVD config, the only
/// ones the fake splits under: LAPACK's SVD, the kept values normalized
/// and absorbed into U, under a cutoff that drops only values of
/// rounding's size.
const ALGEBRA_SETTINGS: [SvdSetting; 4] = [
    SvdSetting::AlgorithmGesvd,
    SvdSetting::NormalizationL2,
    SvdSetting::PartitionUs,
    SvdSetting::DiscardedWeightCutoff(1e-12),
];

/// A two-site gate applied to two qubits in |0⟩: A and B hold |0⟩ at bond
/// extent 1, so the product θ[p', q'] is the gate's first column
/// G[p', q', 0, 0], a 2×2 matrix, which U and V factor at a shared extent
/// of 2.
struct GateSplit<C = FakeLibraries> {
    device: Device<C>,
    config: OpaqueHandle,
    info: OpaqueHandle,
    operands: GateSplitOperands,
}

impl GateSplit {
    /// [`on`](Self::on) the fake libraries.
    fn of(theta: [Complex64; 4]) -> Self {
        Self::on(fake(), theta)
    }
}

impl<C: TestedCalls> GateSplit<C> {
    /// A split on `device` of `theta`, column-major, under the algebra's
    /// settings ([`ALGEBRA_SETTINGS`]).
    fn on(device: Device<C>, theta: [Complex64; 4]) -> Self {
        let (one, zero) = (Complex64::new(1.0, 0.0), Complex64::default());
        let config = device
            .calls
            .create_svd_config(device.handle)
            .expect("the handle is live");
        for setting in ALGEBRA_SETTINGS {
            device
                .calls
                .configure_svd(device.handle, config, setting)
                .expect("the setting is in range");
        }
        let info = device
            .calls
            .create_svd_info(device.handle)
            .expect("the handle is live");
        let operands = GateSplitOperands {
            a: device.operand(&[2, 1], &[P, BOND], &[one, zero]),
            b: device.operand(&[1, 2], &[BOND, Q], &[one, zero]),
            gate: device.operand(&[2, 2, 2, 2], &GATE, &values(16, 0.25)),
            u: device.operand(&[2, 2], &[P_OUT, S], &[zero; 4]),
            v: device.operand(&[2, 2], &[S, Q_OUT], &[zero; 4]),
            singular_values: device.buffer(&[zero; 2]),
        };
        let split = Self {
            device,
            config,
            info,
            operands,
        };
        split.device.load(split.operands.gate.data, &theta);
        split
            .device
            .calls
            .synchronize_copies()
            .expect("the copies were queued");
        split
    }

    /// Sizes the workspace for a split under `config`, and lends its device
    /// scratch what it asks for.
    fn size(&self, config: OpaqueHandle) {
        self.size_workspace(config);
        self.device.lend(WorkspaceSlot::DeviceScratch);
    }

    /// Sizes the workspace for a split under `config`, and lends it
    /// nothing.
    fn size_workspace(&self, config: OpaqueHandle) {
        self.size_under(config)
            .expect("the descriptors and config are live");
    }

    /// Sizes the workspace for a split under `config`.
    fn size_under(&self, config: OpaqueHandle) -> Result<(), CudaAlgebraError> {
        let Self {
            device, operands, ..
        } = self;
        device.calls.gate_split_workspace_sizes(
            device.handle,
            &descriptors(operands),
            config,
            device.workspace,
        )
    }

    /// Queues the split under `config` on `stream`, in the workspace as
    /// last sized.
    fn run(&self, config: OpaqueHandle, stream: Stream) -> Result<(), CudaAlgebraError> {
        let device = &self.device;
        device.calls.gate_split(
            device.handle,
            &self.operands,
            config,
            self.info,
            device.workspace,
            stream,
        )
    }

    /// Sizes the workspace for the split, lends it, runs the split and
    /// waits for it; a refusal at any step is returned.
    fn split(&self) -> Result<(), CudaAlgebraError> {
        self.size_under(self.config)?;
        self.device.lend(WorkspaceSlot::DeviceScratch);
        self.run(self.config, self.device.stream)?;
        self.device
            .calls
            .synchronize_stream(self.device.stream)
            .expect("the stream is live");
        Ok(())
    }
}

/// The descriptors of a gate split's `operands`.
fn descriptors(operands: &GateSplitOperands) -> GateSplitDescriptors {
    GateSplitDescriptors {
        a: operands.a.descriptor,
        b: operands.b.descriptor,
        gate: operands.gate.descriptor,
        u: operands.u.descriptor,
        v: operands.v.descriptor,
    }
}

/// A gate split, under a cutoff, on a device `new` makes, of A, B and the
/// gate carrying the labels `a`, `b` and `gate`, into U and V carrying `u`
/// and `v`; every mode has extent 2 but the bond between A and B, of
/// extent 1.
fn split_with_labels<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
    a: &[i32],
    b: &[i32],
    gate: &[i32],
    u: &[i32],
    v: &[i32],
) -> Result<(), CudaAlgebraError> {
    let mut split = GateSplit::on(new(), matrix([[3.0, 0.0], [0.0, 1.0]]));
    let device = &split.device;
    let extents = |modes: &[i32]| -> Vec<i64> {
        modes
            .iter()
            .map(|&mode| if mode == BOND { 1 } else { 2 })
            .collect()
    };
    let elements = |modes: &[i32]| 1 << (modes.len() - usize::from(modes.contains(&BOND)));
    split.operands.a = device.operand(&extents(a), a, &values(elements(a), 0.5));
    split.operands.b = device.operand(&extents(b), b, &values(elements(b), -0.25));
    split.operands.gate = device.operand(&extents(gate), gate, &values(elements(gate), 0.25));
    let zeros = |modes: &[i32]| vec![Complex64::default(); elements(modes)];
    split.operands.u = device.operand(&extents(u), u, &zeros(u));
    split.operands.v = device.operand(&extents(v), v, &zeros(v));
    device
        .calls
        .synchronize_copies()
        .expect("the copies were queued");
    split.split()
}

/// Every slot of a workspace.
const WORKSPACE_SLOTS: [WorkspaceSlot; 4] = [
    WorkspaceSlot::DeviceScratch,
    WorkspaceSlot::DeviceCache,
    WorkspaceSlot::HostScratch,
    WorkspaceSlot::HostCache,
];

// The labels of the QR's input modes i, k and j.
const QR_I: i32 = 1;
const QR_K: i32 = 2;
const QR_J: i32 = 3;

/// A QR of T[i, k, j] (2×2×3) into Q[k, s, i] and R[j, s], whose
/// descriptors list the modes in another order than the input's: the
/// matrix's rows are (i, k), its columns j, and Q and R share an extent.
struct TensorQr<C = FakeLibraries> {
    device: Device<C>,
    input: TensorOperand,
    q: TensorOperand,
    r: TensorOperand,
}

impl<C: TestedCalls> TensorQr<C> {
    /// The QR on `device` at a shared extent of `extent`, in a workspace
    /// sized for it, lent nothing yet.
    fn sized(device: Device<C>, extent: i64) -> Self {
        Self::sized_with_input(device, extent, &[2, 2, 3], &[QR_I, QR_K, QR_J])
    }

    /// As [`sized`](Self::sized), with an input of `input_extents` and
    /// `input_modes` instead of T[i, k, j].
    fn sized_with_input(
        device: Device<C>,
        extent: i64,
        input_extents: &[i64],
        input_modes: &[i32],
    ) -> Self {
        let qr = Self::described(device, extent, input_extents, input_modes);
        qr.size().expect("the descriptors are live");
        qr
    }

    /// As [`sized_with_input`](Self::sized_with_input), the workspace not
    /// sized yet.
    fn described(
        device: Device<C>,
        extent: i64,
        input_extents: &[i64],
        input_modes: &[i32],
    ) -> Self {
        let [mode_i, mode_k, mode_j, mode_s] = [QR_I, QR_K, QR_J, 4];
        let shared = usize::try_from(extent).expect("an extent is positive");
        let zero = Complex64::default();
        let elements = input_extents
            .iter()
            .map(|&extent| usize::try_from(extent).expect("an extent is positive"))
            .product();
        let input = device.operand(input_extents, input_modes, &values(elements, 0.5));
        let q = device.operand(
            &[2, extent, 2],
            &[mode_k, mode_s, mode_i],
            &vec![zero; 4 * shared],
        );
        let r = device.operand(&[3, extent], &[mode_j, mode_s], &vec![zero; 3 * shared]);
        device
            .calls
            .synchronize_copies()
            .expect("the copies were queued");
        Self {
            device,
            input,
            q,
            r,
        }
    }

    /// Sizes the workspace for the QR.
    fn size(&self) -> Result<(), CudaAlgebraError> {
        let device = &self.device;
        device.calls.qr_workspace_sizes(
            device.handle,
            self.input.descriptor,
            self.q.descriptor,
            self.r.descriptor,
            device.workspace,
        )
    }

    /// Queues the QR on `stream`.
    fn run_on(&self, stream: Stream) -> Result<(), CudaAlgebraError> {
        let device = &self.device;
        device.calls.tensor_qr(
            device.handle,
            self.input,
            self.q,
            self.r,
            device.workspace,
            stream,
        )
    }
}

/// Runs the QR of [`TensorQr`] on `device` at a shared extent of `extent`:
/// sizes the workspace, lends its device scratch and queues the QR on the
/// stream `stream_of` gives; a refusal at any step is returned.
fn qr_on<C: TestedCalls>(
    device: Device<C>,
    extent: i64,
    stream_of: impl FnOnce(&Device<C>) -> Stream,
) -> Result<(), CudaAlgebraError> {
    let qr = TensorQr::described(device, extent, &[2, 2, 3], &[QR_I, QR_K, QR_J]);
    qr.size()?;
    qr.device.lend(WorkspaceSlot::DeviceScratch);
    qr.run_on(stream_of(&qr.device))
}

/// Lends every slot of `device`'s workspace but the host cache, which asks
/// for memory too, and checks that `run` is refused until it is lent.
fn needs_every_slot(device: &Device, run: impl Fn() -> Result<(), CudaAlgebraError>) {
    device.lend_all_but(WorkspaceSlot::HostCache);
    run().expect_err("the host cache asks for memory and has none");
    device.lend(WorkspaceSlot::HostCache);
    run().expect("every slot is lent");
}

/// Asserts that `actual` equals `expected` to rounding.
fn assert_close(actual: &[Complex64], expected: &[Complex64]) {
    assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
    for (actual_value, expected_value) in actual.iter().zip(expected) {
        assert!(
            (actual_value - expected_value).norm() < 1e-12,
            "{actual:?} differs from {expected:?}"
        );
    }
}

/// A real 2×2 matrix, column-major, from its rows.
fn matrix(rows: [[f64; 2]; 2]) -> [Complex64; 4] {
    [rows[0][0], rows[1][0], rows[0][1], rows[1][1]].map(|value| Complex64::new(value, 0.0))
}

/// Distinct values for `elements` elements, so a misplaced one shows.
fn values(elements: usize, seed: f64) -> Vec<Complex64> {
    (0..u32::try_from(elements).expect("a test tensor is small"))
        .map(|at| {
            let at = f64::from(at);
            Complex64::new(seed + at, 1.0 - at / 2.0)
        })
        .collect()
}
