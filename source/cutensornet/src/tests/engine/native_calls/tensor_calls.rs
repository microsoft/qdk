// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rules of `TensorCalls`: tensor descriptors, the labels and bonds
//! of gate splits and QRs, a contraction's steps and labels, objects'
//! owners, and the workspace a call is sized for.

use super::*;

/// A tensor descriptor's modes are checked when it is created: one extent
/// per mode, each at least 1. Its element count is checked when a call uses
/// it: a count no buffer could hold is refused. The overflow case runs on
/// the fake alone: no GPU has the memory to reach it. So does
/// the repeated-mode case: cuTensorNet 2.13 on an A100 creates a
/// descriptor that names one mode twice, and its documentation says
/// nothing of it, but the fake's dense tensors give each label one mode,
/// so the fake refuses a tensor it would misread.
#[test]
fn a_malformed_descriptor_is_refused() {
    a_malformed_descriptor_is_refused_on(&fake);
    a_tensor_whose_element_count_overflows_is_refused();
    a_tensor_descriptor_refuses_a_repeated_mode();
}

/// The cases of [`a_malformed_descriptor_is_refused`] that run on the
/// devices `new` makes.
pub(super) fn a_malformed_descriptor_is_refused_on<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    a_tensor_descriptor_needs_one_extent_per_mode(new);
    a_tensor_descriptor_refuses_an_extent_below_one(new);
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
pub(super) fn a_factorizations_bond_is_at_most_its_smaller_side_on<C: TestedCalls>(
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
/// on either. Three cases run on the fake alone, since cuTensorNet 2.13 on
/// an A100 splits the gates they refuse. Its documentation partitions the
/// inputs' uncontracted modes onto U and V, which share one mode of their
/// own, so the fake holds the algebra to a new label for that mode, not
/// A's and B's bond; and it expects U's uncontracted modes "consistent
/// with" A and the gate, V's with B and the gate, so the fake holds the
/// algebra to A's own labels in U and B's in V.
#[test]
fn a_gate_split_takes_only_its_topologys_labels() {
    a_gate_split_takes_only_its_topologys_labels_on(&fake);
    a_gate_split_refuses_its_shared_mode_on_an_input();
    a_gate_split_keeps_a_label_of_a_alone_in_u();
    a_gate_split_keeps_a_label_of_b_alone_in_v();
}

/// The cases of [`a_gate_split_takes_only_its_topologys_labels`] that run
/// on the devices `new` makes.
pub(super) fn a_gate_split_takes_only_its_topologys_labels_on<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    a_gate_split_refuses_a_label_outside_its_topology(new);
    a_gate_split_needs_a_b_and_the_gate_pairwise_connected(new);
    a_gate_split_sums_a_label_on_all_three_inputs(new);
    a_gate_split_refuses_a_contracted_label_kept_in_an_output(new);
    a_gate_split_refuses_its_shared_mode_on_a_single_input(new);
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
/// lent, and contracted, in that order.
#[test]
fn a_contraction_follows_its_steps_in_order() {
    a_contraction_follows_its_steps_in_order_on(&fake);
}

/// The cases of [`a_contraction_follows_its_steps_in_order`], on the
/// devices `new` makes.
pub(super) fn a_contraction_follows_its_steps_in_order_on<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    a_contraction_needs_an_optimized_path(new);
    a_contraction_is_prepared_in_a_workspace_sized_for_its_path(new);
    a_contraction_is_prepared_with_its_device_scratch_lent(new);
    a_contraction_needs_at_least_the_scratch_lent_at_its_preparation(new);
    a_contraction_needs_a_prepared_network(new);
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

/// A contraction's labels are consistent: each label has one extent. The
/// case of an output label no input has runs on the fake alone:
/// cuTensorNet 2.13 on an A100 creates such a network, and its
/// documentation says nothing of it, but the fake takes each output
/// extent from the inputs, so it refuses a network it has no extent for.
#[test]
fn a_contractions_labels_are_consistent() {
    a_contractions_labels_are_consistent_on(&fake);
    a_contraction_output_label_must_be_an_input_label();
}

/// The cases of [`a_contractions_labels_are_consistent`] that run on the
/// devices `new` makes.
pub(super) fn a_contractions_labels_are_consistent_on<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
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

/// A discarded-weight cutoff is a fraction of the total weight below 1, as
/// on cuTensorNet, which refuses a cutoff of 1: a caller that passes one
/// outside [0, 1) is refused.
#[test]
fn an_svd_cutoff_outside_zero_to_one_is_refused() {
    an_svd_cutoff_outside_zero_to_one_is_refused_on(&fake);
}

/// cuTensorNet checks a descriptor's shape when it is created: one extent
/// per mode.
fn a_tensor_descriptor_needs_one_extent_per_mode<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let device = new();
    let create = |extents: &[i64], modes: &[i32]| device.descriptor(TensorModes { extents, modes });
    create(&[2, 3], &[1]).expect_err("two extents for one mode");
    create(&[2, 3], &[1, 2]).expect("an extent for each mode");
}

/// A mode of extent 0 holds no element: a caller whose bond was sized to
/// nothing is caught when it creates the descriptor.
fn a_tensor_descriptor_refuses_an_extent_below_one<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let device = new();
    let create = |extents: &[i64]| {
        device.descriptor(TensorModes {
            extents,
            modes: &[1, 2],
        })
    };
    create(&[2, 0]).expect_err("the second mode has extent 0");
    create(&[2, 1]).expect("every extent is at least 1");
}

/// A label names one mode of a fake's tensor: a caller that gives a tensor
/// one label twice is caught on the fake when it creates the descriptor.
fn a_tensor_descriptor_refuses_a_repeated_mode() {
    let device = fake();
    let create = |modes: &[i32]| {
        device.descriptor(TensorModes {
            extents: &[2, 2, 2],
            modes,
        })
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
        refused_for::<C>(&error, FakeRule::SharedExtentExceedsSmallerSide),
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
            refused_for::<C>(&refused, FakeRule::SharedExtentNotSmallerSide),
            "{extent}: {refused}"
        );
    }
    qr_on(new(), 3, |device| device.stream).expect("Q and R share the smaller side");
}

/// A gate split's U and V share one mode, which no input carries: a caller
/// that labels A's and B's bond as the shared mode, which the split would
/// sum and keep at once, is caught on the fake.
fn a_gate_split_refuses_its_shared_mode_on_an_input() {
    let theta = matrix([[3.0, 0.0], [0.0, 1.0]]);
    let with_bond = |bond| {
        let mut split = GateSplit::on(fake(), theta);
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
        refused_for::<FakeLibraries>(&refused, FakeRule::LabelOutsideGateSplitTopology),
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
        refused_for::<C>(&refused, FakeRule::LabelOutsideGateSplitTopology),
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
    let not_connected = FakeRule::GateSplitNotPairwiseConnected;
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

/// A gate split sums each label its inputs share, as a network sums every
/// label its tensors share, however many carry it: a caller whose A, B and
/// gate all carry one label (a hyperedge), kept in neither output, gets its
/// split.
fn a_gate_split_sums_a_label_on_all_three_inputs<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
    let hyperedge = OUTER + 1;
    split_with_labels(
        new,
        &[P, BOND, hyperedge],
        &[BOND, Q, hyperedge],
        &[P_OUT, Q_OUT, P, Q, hyperedge],
        &[P_OUT, S],
        &[S, Q_OUT],
    )
    .expect("a label on all three inputs is summed");
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
        refused_for::<C>(&refused, FakeRule::LabelOutsideGateSplitTopology),
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
        refused_for::<C>(&refused, FakeRule::LabelOutsideGateSplitTopology),
        "{refused}"
    );
}

/// A gate split keeps A's site in U: a caller that puts a label of A alone
/// (its outer bond) in V is caught on the fake.
fn a_gate_split_keeps_a_label_of_a_alone_in_u() {
    let (a, b) = ([OUTER, P, BOND], [BOND, Q]);
    split_with_labels(&fake, &a, &b, &GATE, &[OUTER, P_OUT, S], &[S, Q_OUT])
        .expect("A's outer bond is in U");
    let refused = split_with_labels(&fake, &a, &b, &GATE, &[P_OUT, S], &[S, Q_OUT, OUTER])
        .expect_err("A's outer bond is in V");
    assert!(
        refused_for::<FakeLibraries>(&refused, FakeRule::LabelOutsideGateSplitTopology),
        "{refused}"
    );
}

/// A gate split keeps B's site in V: a caller that puts a label of B alone
/// (its outer bond) in U is caught on the fake.
fn a_gate_split_keeps_a_label_of_b_alone_in_v() {
    let (a, b) = ([P, BOND], [BOND, Q, OUTER]);
    split_with_labels(&fake, &a, &b, &GATE, &[P_OUT, S], &[S, Q_OUT, OUTER])
        .expect("B's outer bond is in V");
    let refused = split_with_labels(&fake, &a, &b, &GATE, &[OUTER, P_OUT, S], &[S, Q_OUT])
        .expect_err("B's outer bond is in U");
    assert!(
        refused_for::<FakeLibraries>(&refused, FakeRule::LabelOutsideGateSplitTopology),
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
    let config = device.svd_config().expect("the handle is live");
    for setting in ALGEBRA_SETTINGS {
        device
            .calls
            .configure_svd(device.handle, config, setting)
            .expect("the setting is in range");
    }
    let info = device.svd_info().expect("the handle is live");
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
pub(super) fn a_qr_splits_its_input_modes_between_its_two_factors_on<C: TestedCalls>(
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
    let network = device
        .network(DOT_PRODUCT_INPUTS, DOT_PRODUCT_OUTPUT.modes)
        .expect("the labels are consistent")
        .descriptor;
    let info = device.optimizer_info(network).expect("the network is live");
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
    let network = device
        .network(DOT_PRODUCT_INPUTS, DOT_PRODUCT_OUTPUT.modes)
        .expect("the labels are consistent")
        .descriptor;
    let info = device.optimizer_info(network).expect("the network is live");
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

/// cuTensorNet contracts a network once it is prepared: a caller that
/// contracts an optimized network it never prepared is refused.
fn a_contraction_needs_a_prepared_network<C: TestedCalls>(new: &dyn Fn() -> Device<C>) {
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
    assert!(
        refused_for::<C>(&refused, FakeRule::NetworkNotPrepared),
        "{refused}"
    );
    contraction
        .prepare()
        .expect("the workspace is sized for the path");
    contraction.contract().expect("the network is prepared");
}

/// The fake takes each output mode's extent from the inputs: a caller
/// whose output names a label no input has is caught on the fake.
fn a_contraction_output_label_must_be_an_input_label() {
    let device = fake();
    device
        .network(DOT_PRODUCT_INPUTS, &[2])
        .expect_err("no input has the label 2");
    device
        .network(DOT_PRODUCT_INPUTS, &[1])
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
        .network(inputs(&[3]), &[])
        .expect_err("the label 1 has the extents 2 and 3");
    device
        .network(inputs(&[2]), &[])
        .expect("the label 1 has the extent 2 in both inputs");
}

/// The cases of [`an_svd_cutoff_outside_zero_to_one_is_refused`], on the
/// devices `new` makes.
pub(super) fn an_svd_cutoff_outside_zero_to_one_is_refused_on<C: TestedCalls>(
    new: &dyn Fn() -> Device<C>,
) {
    let device = new();
    let config = device.svd_config().expect("the handle is live");
    let configure = |cutoff| {
        device.calls.configure_svd(
            device.handle,
            config,
            SvdSetting::DiscardedWeightCutoff(cutoff),
        )
    };
    configure(-0.1).expect_err("the cutoff is below 0");
    configure(1.5).expect_err("the cutoff is above 1");
    configure(1.0).expect_err("1 is outside [0, 1)");
    configure(0.0).expect("0 is in range");
    configure(f64::next_down(1.0)).expect("the largest f64 below 1 is in range");
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
    split.device.disown(split.operands.singular_values);
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
    let other_handle = split
        .device
        .another_handle()
        .expect("the fake has a device");
    let foreign = split
        .device
        .svd_config_on(other_handle)
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
    let other_handle = device.another_handle().expect("the fake has a device");
    let descriptor = |handle, extents: &[i64], modes: &[i32]| {
        device
            .descriptor_on(handle, TensorModes { extents, modes })
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
    contraction.device.disown(contraction.info);
    contraction.device.disown(contraction.network.descriptor);
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
        refused_for::<FakeLibraries>(&refused, FakeRule::WorkspaceNotSizedForTheCall),
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
    let second = device.another_workspace().expect("the handle is live");
    fake.contraction_workspace_sizes(handle, network, contraction.info, second)
        .expect("the path is optimized");
    let bytes = fake
        .workspace_size(handle, second, WorkspaceSlot::DeviceScratch)
        .expect("the workspace is live");
    let memory = device
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
            refused_for::<FakeLibraries>(&refused, FakeRule::WorkspaceNotSizedForTheCall),
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
    let other = device.svd_config().expect("the handle is live");
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
        .descriptor(TensorModes {
            extents: &[2, 2],
            modes: &[P_OUT, S],
        })
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
        refused_for::<FakeLibraries>(&refused, FakeRule::WorkspaceNotSizedForTheCall),
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
        refused_for::<FakeLibraries>(&refused, FakeRule::WorkspaceNotSizedForTheCall),
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
        refused_for::<FakeLibraries>(&refused, FakeRule::WorkspaceNotSizedForTheCall),
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
