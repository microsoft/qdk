// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a node carries besides its name, and that it survives being persisted.

use crate::{
    FirOriginKind, Graph, GraphBuilder, NodeOrigin, NodePayload, NodeValue, Real, RegionKind,
    canonical_graph_snapshot, decode_graph_snapshot, graph_digest,
};

fn graph_carrying(payload: NodePayload) -> Graph {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let node = builder
        .add_node(region, "constant", Vec::new())
        .expect("node");
    builder.declare_payload(node, payload).expect("payload");
    builder.finish()
}

#[test]
fn a_payload_survives_the_snapshot_round_trip() {
    let payload = NodePayload {
        value: Some(NodeValue::Number(Real::new(std::f64::consts::FRAC_PI_4))),
        origin: Some(NodeOrigin::Instruction(7)),
    };
    let graph = graph_carrying(payload.clone());
    let restored =
        decode_graph_snapshot(&canonical_graph_snapshot(&graph).expect("encode")).expect("decode");
    assert_eq!(restored, graph);
    assert_eq!(
        restored.nodes().next().expect("node").payload.clone(),
        payload
    );
}

#[test]
fn a_node_carrying_no_payload_is_unchanged_by_its_presence() {
    let graph = graph_carrying(NodePayload::default());
    assert!(graph.nodes().next().expect("node").payload.is_empty());
    let restored =
        decode_graph_snapshot(&canonical_graph_snapshot(&graph).expect("encode")).expect("decode");
    assert_eq!(restored, graph);
}

#[test]
fn a_fir_origin_survives_the_snapshot_round_trip() {
    let payload = NodePayload {
        value: None,
        origin: Some(NodeOrigin::Fir {
            package: 2,
            kind: FirOriginKind::Expression,
            local_id: 41,
        }),
    };
    let graph = graph_carrying(payload.clone());
    let restored =
        decode_graph_snapshot(&canonical_graph_snapshot(&graph).expect("encode")).expect("decode");
    assert_eq!(
        restored.nodes().next().expect("node").payload.clone(),
        payload
    );
    assert_eq!(
        restored
            .nodes()
            .next()
            .expect("node")
            .payload
            .origin
            .and_then(NodeOrigin::fir),
        Some((2, FirOriginKind::Expression, 41))
    );
}

/// Circuit-level origins and FIR origins index different things, so a node
/// from one producer must never compare equal to a node from the other.
#[test]
fn a_circuit_origin_and_a_fir_origin_are_distinguishable() {
    let circuit = graph_carrying(NodePayload {
        value: None,
        origin: Some(NodeOrigin::Instruction(7)),
    });
    let fir = graph_carrying(NodePayload {
        value: None,
        origin: Some(NodeOrigin::Fir {
            package: 0,
            kind: FirOriginKind::Expression,
            local_id: 7,
        }),
    });
    assert_ne!(
        graph_digest(&circuit).expect("digest"),
        graph_digest(&fir).expect("digest")
    );
    assert_eq!(
        circuit
            .nodes()
            .next()
            .expect("node")
            .payload
            .origin
            .and_then(NodeOrigin::fir),
        None
    );
}

/// FIR positions use plain indices so source attribution does not invert the
/// graph's dependency on its compiler producers.
#[test]
fn this_crate_does_not_depend_on_fir() {
    let manifest = include_str!("../../Cargo.toml");
    assert!(
        !manifest.contains("qsc_fir"),
        "qsc_qhdg must not depend on qsc_fir; FIR origin conversion belongs at the compiler boundary"
    );
}

#[test]
fn two_graphs_differing_only_in_a_payload_have_different_digests() {
    let first = graph_carrying(NodePayload::number(0.25));
    let second = graph_carrying(NodePayload::number(0.5));
    assert_ne!(
        graph_digest(&first).expect("digest"),
        graph_digest(&second).expect("digest")
    );
    assert_eq!(
        graph_digest(&first).expect("digest"),
        graph_digest(&graph_carrying(NodePayload::number(0.25))).expect("digest")
    );
}

/// The reason a real is stored by bit pattern. The name encoding it replaces
/// used `{:?}`, which round-trips these; a float comparison would not.
#[test]
fn a_real_round_trips_the_values_a_float_comparison_would_lose() {
    for value in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        0.0,
        f64::MIN_POSITIVE,
        std::f64::consts::PI,
    ] {
        let restored = Real::new(value).get();
        assert_eq!(restored.to_bits(), value.to_bits());
    }
    assert_ne!(Real::new(0.0), Real::new(-0.0));
    assert_eq!(Real::new(f64::NAN), Real::new(f64::NAN));
}

#[test]
fn a_payload_reports_the_value_it_carries_and_nothing_else() {
    let number = NodePayload::number(1.5);
    assert_eq!(number.number_value(), Some(1.5));
    assert_eq!(number.classical_bit_value(), None);

    let bit = NodePayload::classical_bit(3);
    assert_eq!(bit.classical_bit_value(), Some(3));
    assert_eq!(bit.number_value(), None);
}
