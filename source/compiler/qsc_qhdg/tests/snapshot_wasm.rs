// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![cfg(target_arch = "wasm32")]

use qsc_qhdg::{
    Direction, Edge, EdgeKind, EffectEvent, EffectResource, Graph, GraphBuilder, NodeId, PortId,
    PortKind, RegionKind, Stage, Type, Usage, canonical_graph_snapshot, decode_graph_snapshot,
    graph_snapshot_text,
};
use qsc_semantic_contracts::{EffectDomain, EffectMode};
use wasm_bindgen_test::wasm_bindgen_test;

fn effect() -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Quantum,
        mode: EffectMode::Write,
        resource: EffectResource::Named("q".to_string()),
        observable: false,
    }
}

fn capability(builder: &mut GraphBuilder, node: NodeId, direction: Direction) -> PortId {
    builder
        .add_port(
            node,
            direction,
            PortKind::Capability,
            Type::Qubit,
            Stage::Generation,
            Usage::Affine,
        )
        .expect("capability port")
}

/// The same graph every time, with `rotation` deciding which threaded pair is
/// connected first. Construction order is the only thing that varies.
fn fixture(threads: usize, rotation: usize) -> Graph {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let mut pairs = Vec::new();
    for _ in 0..threads {
        let producer = builder
            .add_node(region, "quantum.allocate", vec![effect()])
            .expect("producer");
        let consumer = builder
            .add_node(region, "quantum.release", vec![effect()])
            .expect("consumer");
        let source = capability(&mut builder, producer, Direction::Output);
        let target = capability(&mut builder, consumer, Direction::Input);
        pairs.push((source, target));
    }
    pairs.rotate_left(rotation % threads.max(1));
    for (source, target) in pairs {
        builder
            .connect(Edge {
                source,
                target,
                kind: EdgeKind::AffineCapability,
            })
            .expect("capability edge");
    }
    builder.finish()
}

#[wasm_bindgen_test]
fn deterministic_wasm_snapshot_matches_all_construction_orders() {
    let expected = canonical_graph_snapshot(&fixture(8, 0)).expect("encode");
    for rotation in 0..8 {
        assert_eq!(
            canonical_graph_snapshot(&fixture(8, rotation)).expect("encode"),
            expected
        );
    }

    let restored = decode_graph_snapshot(&expected).expect("wasm snapshot decodes");
    assert_eq!(
        canonical_graph_snapshot(&restored).expect("encode"),
        expected
    );
}

#[wasm_bindgen_test]
fn wasm_text_snapshot_matches_the_binary_model() {
    let graph = fixture(2, 0);
    let text = graph_snapshot_text(&graph).expect("text");
    let restored = decode_graph_snapshot(text.as_bytes()).expect("decode text");
    assert_eq!(
        canonical_graph_snapshot(&restored).expect("encode"),
        canonical_graph_snapshot(&graph).expect("encode")
    );
}

#[wasm_bindgen_test]
fn graph_binary_matches_the_native_fixture_and_digest() {
    let bytes = include_bytes!("../../qsc_snapshot/tests/fixtures/graph.bin");
    let graph = decode_graph_snapshot(bytes).expect("decode");
    assert_eq!(canonical_graph_snapshot(&graph).expect("encode"), bytes);
    assert_eq!(
        qsc_qhdg::graph_digest(&graph).expect("digest"),
        include_str!("../../qsc_snapshot/tests/fixtures/graph.digest").trim()
    );
}

#[wasm_bindgen_test]
fn graph_text_matches_the_native_fixture() {
    let text = include_str!("../../qsc_snapshot/tests/fixtures/graph.sexpr");
    let graph = decode_graph_snapshot(text.as_bytes()).expect("decode");
    assert_eq!(graph_snapshot_text(&graph).expect("encode"), text);
    assert_eq!(
        canonical_graph_snapshot(&graph).expect("binary"),
        include_bytes!("../../qsc_snapshot/tests/fixtures/graph.bin")
    );
}
