// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use crate::{
    AnalysisCache, AnalysisKey, AnalysisKind, AnalysisScope, AnalysisValue, CachedAnalysis,
    Direction, Edge, EdgeKind, EffectEvent, EffectResource, Graph, GraphBuilder, NodeId, PortId,
    PortKind, RegionKind, SnapshotError, Stage, Type, Usage, canonical_graph_snapshot,
    decode_graph_snapshot, graph_digest, graph_snapshot_text,
};
use qsc_semantic_contracts::{EffectDomain, EffectMode};

#[test]
fn graph_native_binary_matches_the_shared_fixture_and_digest() {
    let bytes = include_bytes!("../../../qsc_snapshot/tests/fixtures/graph.bin");
    let graph = decode_graph_snapshot(bytes).expect("decode");
    assert_eq!(canonical_graph_snapshot(&graph).expect("encode"), bytes);
    assert_eq!(
        graph_digest(&graph).expect("digest"),
        include_str!("../../../qsc_snapshot/tests/fixtures/graph.digest").trim()
    );
}

#[test]
fn graph_native_text_matches_the_shared_fixture() {
    let text = include_str!("../../../qsc_snapshot/tests/fixtures/graph.sexpr");
    let graph = decode_graph_snapshot(text.as_bytes()).expect("decode");
    assert_eq!(graph_snapshot_text(&graph).expect("text"), text);
    assert_eq!(
        canonical_graph_snapshot(&graph).expect("binary"),
        include_bytes!("../../../qsc_snapshot/tests/fixtures/graph.bin")
    );
}

fn effect() -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Quantum,
        mode: EffectMode::Write,
        resource: EffectResource::Named("q".to_string()),
        observable: false,
    }
}

/// Two capabilities threaded from an allocation to a release, so the graph has
/// enough structure that edge order could differ between builds.
fn graph(reversed: bool) -> Graph {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let mut pairs = Vec::new();
    for _ in 0..2 {
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
    if reversed {
        pairs.reverse();
    }
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

#[test]
fn a_snapshot_round_trip_restores_the_graph_exactly() {
    let graph = graph(false);
    let restored =
        decode_graph_snapshot(&canonical_graph_snapshot(&graph).expect("encode")).expect("decode");
    assert_eq!(restored, graph);
}

#[test]
fn a_snapshot_round_trip_keeps_identity_ownership_and_revision() {
    let mut graph = graph(false);
    let mut transaction = graph.transaction();
    let region = transaction.graph().regions().next().expect("region").id;
    let doomed = transaction
        .add_node(region, "quantum.barrier", vec![])
        .expect("node");
    transaction.retire_node(doomed).expect("retire");
    transaction.commit().expect("commit");

    let restored =
        decode_graph_snapshot(&canonical_graph_snapshot(&graph).expect("encode")).expect("decode");
    assert_eq!(restored.revision(), graph.revision());
    assert_eq!(
        restored.retired_identities().collect::<Vec<_>>(),
        graph.retired_identities().collect::<Vec<_>>()
    );
    for node in graph.nodes() {
        assert_eq!(restored.stable_id(node.id), graph.stable_id(node.id));
    }
    assert_eq!(restored, graph);
}

#[test]
fn the_same_graph_built_in_a_different_order_serializes_identically() {
    assert_eq!(
        canonical_graph_snapshot(&graph(false)).expect("encode"),
        canonical_graph_snapshot(&graph(true)).expect("encode")
    );
}

#[test]
fn snapshot_bytes_are_stable_across_repeated_encoding() {
    let graph = graph(false);
    let first = canonical_graph_snapshot(&graph).expect("encode");
    assert_eq!(first, canonical_graph_snapshot(&graph).expect("encode"));
    let restored = decode_graph_snapshot(&first).expect("decode");
    assert_eq!(canonical_graph_snapshot(&restored).expect("encode"), first);
}

#[test]
fn a_different_graph_has_a_different_digest() {
    let first = graph(false);
    let mut second = graph(false);
    let region = second.regions().next().expect("region").id;
    second
        .add_node(region, "quantum.barrier", vec![])
        .expect("node");
    assert_ne!(
        graph_digest(&first).expect("digest"),
        graph_digest(&second).expect("digest")
    );
}

#[test]
fn a_tampered_snapshot_is_refused() {
    let graph = graph(false);
    let text = graph_snapshot_text(&graph).expect("text");
    let tampered = text.replace("quantum.allocate", "quantum.reallocate");
    assert_ne!(tampered, text);
    assert_eq!(
        decode_graph_snapshot(tampered.as_bytes()),
        Err(SnapshotError::DigestMismatch)
    );
}

#[test]
fn a_snapshot_of_an_unsupported_schema_is_refused() {
    let graph = graph(false);
    let text = graph_snapshot_text(&graph).expect("text");
    let bumped = text.replace("(version 1)", "(version 99)");
    assert_ne!(bumped, text);
    assert_eq!(
        decode_graph_snapshot(bumped.as_bytes()),
        Err(SnapshotError::SchemaVersion)
    );
}

#[test]
fn a_text_snapshot_restores_the_same_canonical_binary_graph() {
    let graph = graph(false);
    let text = graph_snapshot_text(&graph).expect("text");
    assert!(text.starts_with("QDKQHDG!(\n(qdk 1 "));
    let restored = decode_graph_snapshot(text.as_bytes()).expect("decode");
    assert_eq!(restored, graph);
    assert_eq!(
        canonical_graph_snapshot(&restored).expect("encode"),
        canonical_graph_snapshot(&graph).expect("encode")
    );
}

#[test]
fn a_binary_snapshot_is_not_json() {
    assert!(
        canonical_graph_snapshot(&graph(false))
            .expect("encode")
            .starts_with(b"QDKQHDG!\x01\n")
    );
    assert!(decode_graph_snapshot(b"{\"payload\":{}}").is_err());
}

#[test]
fn text_whitespace_does_not_change_the_snapshot_digest() {
    let graph = graph(false);
    let text = graph_snapshot_text(&graph)
        .expect("text")
        .replace("(node ", "(node\n  ");
    assert_eq!(
        decode_graph_snapshot(text.as_bytes()).expect("decode"),
        graph
    );
}

#[test]
fn an_unknown_required_snapshot_feature_is_rejected() {
    let text = graph_snapshot_text(&graph(false)).expect("text");
    let changed = text.replace(
        "(requiredFeatures (list))",
        "(requiredFeatures (list \"future\"))",
    );
    assert_ne!(changed, text);
    assert_eq!(
        decode_graph_snapshot(changed.as_bytes()),
        Err(SnapshotError::UnknownRequiredFeature("future".into()))
    );
}

#[test]
fn an_unverified_graph_cannot_be_restored_even_with_a_valid_digest() {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let node = builder
        .add_node(region, "quantum.allocate", vec![effect()])
        .expect("node");
    capability(&mut builder, node, Direction::Output);
    let graph = builder.finish();
    assert_eq!(
        decode_graph_snapshot(&canonical_graph_snapshot(&graph).expect("encode")),
        Err(SnapshotError::Verification)
    );
}

fn simple_graph() -> Graph {
    let mut graph = Graph::default();
    let region = graph.add_region(None, RegionKind::ModuleOmega);
    for _ in 0..4 {
        graph
            .add_node(region, "vendor.custom.operation", vec![])
            .expect("node");
    }
    graph
}

fn both_round_trips(graph: &Graph) -> [Graph; 2] {
    [
        decode_graph_snapshot(&canonical_graph_snapshot(graph).expect("binary"))
            .expect("binary decode"),
        decode_graph_snapshot(graph_snapshot_text(graph).expect("text").as_bytes())
            .expect("text decode"),
    ]
}

#[test]
fn checkpoint_resumes_free_stack_generations_and_borrows_with_fresh_handles() {
    use crate::{Authority, BorrowKind, CleanupState, Epoch, OwnerPath, ResourceId};
    let mut graph = simple_graph();
    let live_handle = graph.handle(NodeId(0)).expect("handle");
    let stale_handle = graph.handle(NodeId(1)).expect("handle");
    graph.retire_node(NodeId(1)).expect("retire");
    graph.retire_node(NodeId(3)).expect("retire");
    let path = OwnerPath::root(42)
        .field("member")
        .index(2, None)
        .expect("path");
    graph
        .ownership_mut()
        .add_authority(Authority {
            path: path.clone(),
            resources: BTreeSet::from([ResourceId(18)]),
            epoch: Epoch(4),
            cleanup: CleanupState::PendingFragment {
                delta: 9,
                fragment_count: 2,
                history: vec!["first".into(), "second".into()],
            },
        })
        .expect("authority");
    let ended = graph
        .ownership_mut()
        .borrow(&path, BorrowKind::Shared)
        .expect("borrow");
    graph.ownership_mut().end_borrow(&ended).expect("end");
    let held = graph
        .ownership_mut()
        .borrow(&path, BorrowKind::Shared)
        .expect("borrow");
    graph.revision = 8;
    let mut restored = both_round_trips(&graph);
    for copy in &restored {
        assert_eq!(copy, &graph);
        assert!(copy.node_by_handle(live_handle).is_none());
        assert!(copy.node_by_handle(stale_handle).is_none());
        assert!(!std::sync::Arc::ptr_eq(
            &copy.cache_identity,
            &graph.cache_identity
        ));
    }
    let region = graph.regions().next().expect("region").id;
    for expected_slot in [NodeId(3), NodeId(1), NodeId(4)] {
        let id = graph.add_node(region, "next", vec![]).expect("allocate");
        assert_eq!(id, expected_slot);
        for copy in &mut restored {
            let id = copy.add_node(region, "next", vec![]).expect("allocate");
            assert_eq!(id, expected_slot);
            assert_eq!(copy.stable_id(id), graph.stable_id(id));
        }
    }
    graph.ownership_mut().end_borrow(&held).expect("end held");
    let next = graph
        .ownership_mut()
        .borrow(&path, BorrowKind::Exclusive)
        .expect("next");
    for copy in &mut restored {
        copy.ownership_mut().end_borrow(&held).expect("end held");
        assert_eq!(
            copy.ownership_mut()
                .borrow(&path, BorrowKind::Exclusive)
                .expect("next"),
            next
        );
        assert_eq!(copy, &graph);
    }
}

#[test]
fn graph_values_preserve_real_bits_symbols_origins_and_recursive_angles() {
    use crate::{
        Angle, AngleOp, FirOriginKind, NodeOrigin, NodePayload, NodeValue, Real, SymbolId,
    };
    let values = [
        0u64,
        1,
        0x8000_0000_0000_0000,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff8_1234_5678_9abc,
    ];
    for bits in values {
        let mut graph = simple_graph();
        let real = Real::new(f64::from_bits(bits));
        graph
            .declare_payload(
                NodeId(0),
                NodePayload {
                    value: Some(NodeValue::Number(real)),
                    origin: Some(NodeOrigin::Instruction(0)),
                },
            )
            .expect("payload");
        graph
            .declare_payload(
                NodeId(1),
                NodePayload {
                    value: Some(NodeValue::Angle(Angle::Binary {
                        op: AngleOp::Div,
                        left: Box::new(Angle::Neg {
                            operand: Box::new(Angle::Parameter {
                                name: "θ \" \n".into(),
                            }),
                        }),
                        right: Box::new(Angle::Literal { value: real }),
                    })),
                    origin: Some(NodeOrigin::GlobalPhase),
                },
            )
            .expect("payload");
        graph
            .declare_payload(
                NodeId(2),
                NodePayload {
                    value: Some(NodeValue::ClassicalBit(17)),
                    origin: Some(NodeOrigin::Fir {
                        package: 9,
                        kind: FirOriginKind::Pattern,
                        local_id: 3,
                    }),
                },
            )
            .expect("payload");
        graph
            .declare_symbol(
                NodeId(2),
                SymbolId {
                    package: String::new(),
                    item: "nested".into(),
                },
            )
            .expect("symbol");
        for copy in both_round_trips(&graph) {
            assert_eq!(copy, graph);
        }
    }
}

fn altered_binary(
    graph: &Graph,
    alter: impl FnOnce(qsc_snapshot::snapshot_capnp::graph_checkpoint::Builder<'_>),
) -> Vec<u8> {
    use qsc_snapshot::{binary, snapshot_capnp as wire};
    let mut message = binary::Message::new_default();
    let mut document = message.init_root::<wire::document::Builder>();
    document.set_version(qsc_snapshot::MODEL_VERSION);
    let mut snapshot = document.init_graph();
    crate::graph_snapshot_binary::write_payload(
        snapshot.reborrow().init_payload(),
        graph,
        graph.revision(),
        &BTreeSet::new(),
        &BTreeSet::new(),
    )
    .expect("payload");
    alter(
        snapshot
            .reborrow()
            .get_payload()
            .expect("payload")
            .get_checkpoint()
            .expect("checkpoint"),
    );
    let digest = crate::graph_snapshot::hash::<wire::graph_payload::Owned>(
        b"qdk.graph.snapshot.v1\0",
        snapshot
            .reborrow_as_reader()
            .get_payload()
            .expect("payload"),
    )
    .expect("hash");
    snapshot.set_digest(&digest);
    binary::encode(qsc_snapshot::Kind::Graph, &message).expect("encode")
}

#[test]
fn invalid_private_identity_bookkeeping_is_rejected_before_publication() {
    let mut graph = simple_graph();
    graph.retire_node(NodeId(1)).expect("retire");
    graph.retire_node(NodeId(3)).expect("retire");
    for case in 0..8 {
        let bytes = altered_binary(&graph, |mut checkpoint| {
            let mut identity = checkpoint.reborrow().get_identity().expect("identity");
            match case {
                0 => identity.set_next_slot(1),
                1 => identity.set_next_stable(1),
                2 => identity
                    .reborrow()
                    .get_live()
                    .expect("live")
                    .get(1)
                    .set_stable(0),
                3 => identity.reborrow().get_free().expect("free").set(1, 1),
                4 => identity.reborrow().get_free().expect("free").set(1, 0),
                5 => identity
                    .reborrow()
                    .get_retired()
                    .expect("retired")
                    .set(0, 0),
                6 => identity
                    .reborrow()
                    .get_generations()
                    .expect("generations")
                    .get(0)
                    .set_node(2),
                _ => identity
                    .reborrow()
                    .get_generations()
                    .expect("generations")
                    .get(1)
                    .set_generation(0),
            }
        });
        assert!(decode_graph_snapshot(&bytes).is_err(), "case {case}");
    }
    let text = graph_snapshot_text(&graph).expect("text");
    for (from, to) in [
        ("(nextSlot 4)", "(nextSlot 1)"),
        ("(free (list 1 3))", "(free (list 1 1))"),
        ("(generation 1)", "(generation 0)"),
    ] {
        let changed = text.replace(from, to);
        assert_ne!(changed, text);
        assert!(decode_graph_snapshot(changed.as_bytes()).is_err());
    }
}

#[test]
fn invalid_borrow_ids_and_counters_are_rejected() {
    use crate::{Authority, BorrowKind, CleanupState, Epoch, OwnerPath, ResourceId};
    let mut graph = simple_graph();
    let path = OwnerPath::root(0);
    graph
        .ownership_mut()
        .add_authority(Authority {
            path: path.clone(),
            resources: BTreeSet::from([ResourceId(3)]),
            epoch: Epoch(0),
            cleanup: CleanupState::CleanZ,
        })
        .expect("authority");
    for _ in 0..2 {
        graph
            .ownership_mut()
            .borrow(&path, BorrowKind::Shared)
            .expect("borrow");
    }
    for case in 0..4 {
        let bytes = altered_binary(&graph, |mut checkpoint| {
            let mut state = checkpoint.reborrow().get_ownership().expect("ownership");
            match case {
                0 => state.set_next_borrow(1),
                1 => state
                    .reborrow()
                    .get_borrows()
                    .expect("borrows")
                    .get(1)
                    .set_id(0),
                2 => state
                    .reborrow()
                    .get_borrows()
                    .expect("borrows")
                    .get(0)
                    .set_epoch(8),
                _ => state
                    .reborrow()
                    .get_borrows()
                    .expect("borrows")
                    .get(0)
                    .get_path()
                    .expect("path")
                    .set_root(7),
            }
        });
        assert!(decode_graph_snapshot(&bytes).is_err(), "case {case}");
    }
}

#[test]
fn checked_graph_references_reject_orphans_duplicates_and_cycles() {
    let graph = graph(false);
    for case in 0..8 {
        let bytes = altered_binary(&graph, |mut checkpoint| match case {
            0 => checkpoint
                .reborrow()
                .get_nodes()
                .expect("nodes")
                .get(0)
                .set_region(99),
            1 => checkpoint
                .reborrow()
                .get_nodes()
                .expect("nodes")
                .get(1)
                .set_id(0),
            2 => checkpoint
                .reborrow()
                .get_ports()
                .expect("ports")
                .get(0)
                .set_node(99),
            3 => checkpoint
                .reborrow()
                .get_ports()
                .expect("ports")
                .get(0)
                .set_direction(qsc_snapshot::snapshot_capnp::Direction::Input),
            4 => checkpoint
                .reborrow()
                .get_regions()
                .expect("regions")
                .get(0)
                .get_parent()
                .expect("parent")
                .set_some(0),
            5 => checkpoint.reborrow().init_demands(1).set(0, 99),
            6 => checkpoint.set_next_port(1),
            _ => checkpoint
                .reborrow()
                .get_edges()
                .expect("edges")
                .get(0)
                .set_target(99),
        });
        assert!(decode_graph_snapshot(&bytes).is_err(), "case {case}");
    }
}

#[test]
fn semantic_depth_limit_is_shared_by_encodings_and_fingerprints() {
    use crate::{Angle, NodePayload, NodeValue};
    let mut graph = simple_graph();
    let mut angle = Angle::literal(0.0);
    for _ in 0..qsc_snapshot::MAX_DEPTH {
        angle = Angle::Neg {
            operand: Box::new(angle),
        };
    }
    graph
        .declare_payload(
            NodeId(0),
            NodePayload {
                value: Some(NodeValue::Angle(angle.clone())),
                origin: None,
            },
        )
        .expect("payload");
    for copy in both_round_trips(&graph) {
        assert_eq!(copy, graph);
    }
    graph
        .declare_payload(
            NodeId(0),
            NodePayload {
                value: Some(NodeValue::Angle(Angle::Neg {
                    operand: Box::new(angle),
                })),
                origin: None,
            },
        )
        .expect("payload");
    assert_eq!(
        canonical_graph_snapshot(&graph),
        Err(SnapshotError::Encoding(qsc_snapshot::Error::DepthLimit))
    );
    assert_eq!(
        graph_snapshot_text(&graph),
        Err(SnapshotError::Encoding(qsc_snapshot::Error::DepthLimit))
    );
    assert_eq!(
        graph_digest(&graph),
        Err(SnapshotError::Encoding(qsc_snapshot::Error::DepthLimit))
    );
    assert_eq!(
        crate::region_digest(
            &graph,
            &crate::Adjacency::new(&graph),
            &BTreeSet::from([graph.stable_id(NodeId(0)).expect("stable")])
        ),
        Err(SnapshotError::Encoding(qsc_snapshot::Error::DepthLimit))
    );
}

#[test]
fn snapshot_integrity_and_content_digests_have_distinct_revision_scopes() {
    let mut graph = simple_graph();
    let bytes = canonical_graph_snapshot(&graph).expect("snapshot");
    let content = graph_digest(&graph).expect("digest");
    let local = crate::region_digest(
        &graph,
        &crate::Adjacency::new(&graph),
        &BTreeSet::from([crate::StableNodeId(0)]),
    )
    .expect("local");
    graph.revision += 1;
    assert_ne!(canonical_graph_snapshot(&graph).expect("snapshot"), bytes);
    assert_eq!(graph_digest(&graph).expect("digest"), content);
    assert_eq!(
        crate::region_digest(
            &graph,
            &crate::Adjacency::new(&graph),
            &BTreeSet::from([crate::StableNodeId(0)])
        )
        .expect("local"),
        local
    );
    assert_ne!(local, content);
    let text = graph_snapshot_text(&graph)
        .expect("text")
        .replace("(revision 1)", "(revision 2)");
    assert_eq!(
        decode_graph_snapshot(text.as_bytes()),
        Err(SnapshotError::DigestMismatch)
    );
}

#[test]
fn canonical_payload_hash_ignores_segment_layout() {
    use qsc_snapshot::{binary, snapshot_capnp as wire};
    let graph = graph(false);
    let mut single = binary::Message::new_default();
    let mut segmented = capnp::message::Builder::new(
        capnp::message::HeapAllocator::new()
            .first_segment_words(1)
            .allocation_strategy(capnp::message::AllocationStrategy::FixedSize),
    );
    for message in [&mut single, &mut segmented] {
        crate::graph_snapshot_binary::write_payload(
            message.init_root(),
            &graph,
            graph.revision(),
            &BTreeSet::new(),
            &BTreeSet::new(),
        )
        .expect("payload");
    }
    assert!(segmented.get_segments_for_output().len() > 1);
    let digest = |message: &binary::Message| {
        crate::graph_snapshot::hash::<wire::graph_payload::Owned>(
            b"qdk.graph.snapshot.v1\0",
            message
                .get_root_as_reader::<wire::graph_payload::Reader>()
                .expect("root"),
        )
        .expect("hash")
    };
    assert_eq!(digest(&single), digest(&segmented));
}

#[test]
fn local_digest_ignores_unmatched_graph_size_but_tracks_incident_state() {
    let mut graph = graph(false);
    let matched = BTreeSet::from([graph.stable_id(NodeId(0)).expect("stable")]);
    let before =
        crate::region_digest(&graph, &crate::Adjacency::new(&graph), &matched).expect("local");
    let region = graph.regions().next().expect("region").id;
    for _ in 0..1000 {
        graph.add_node(region, "unmatched", vec![]).expect("node");
    }
    assert_eq!(
        crate::region_digest(&graph, &crate::Adjacency::new(&graph), &matched).expect("local"),
        before
    );
    graph.ports.get_mut(&PortId(0)).expect("port").stage = Stage::Residual;
    assert_ne!(
        crate::region_digest(&graph, &crate::Adjacency::new(&graph), &matched).expect("local"),
        before
    );
    graph.ports.get_mut(&PortId(0)).expect("port").stage = Stage::Generation;
    graph.edges[0].target = PortId(3);
    assert_ne!(
        crate::region_digest(&graph, &crate::Adjacency::new(&graph), &matched).expect("local"),
        before
    );
}

#[test]
fn graph_internal_hashing_does_not_apply_the_transport_byte_limit() {
    let mut graph = simple_graph();
    graph
        .declare_symbol(
            NodeId(0),
            crate::SymbolId {
                package: "x".repeat(qsc_snapshot::MAX_SNAPSHOT_BYTES + 1),
                item: String::new(),
            },
        )
        .expect("symbol");
    assert!(graph_digest(&graph).is_ok());
    let bytes = canonical_graph_snapshot(&graph).expect("encode");
    assert!(bytes.len() > qsc_snapshot::MAX_SNAPSHOT_BYTES);
    assert_eq!(
        decode_graph_snapshot(&bytes),
        Err(SnapshotError::Encoding(qsc_snapshot::Error::ByteLimit))
    );
}

#[test]
fn typed_text_rejects_duplicate_unknown_missing_and_wrong_variant_fields() {
    let text = graph_snapshot_text(&simple_graph()).expect("text");
    for (from, to) in [
        ("(version 1)", "(version 1) (version 1)"),
        ("(version 1)", "(version 1) (future 3)"),
        ("(version 1)", ""),
        ("(origin (none))", "(origin (none 1))"),
        ("(kind moduleOmega)", "(kind future)"),
    ] {
        let changed = text.replace(from, to);
        assert_ne!(changed, text);
        assert!(decode_graph_snapshot(changed.as_bytes()).is_err());
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn typed_codecs_preserve_all_checkpoint_variants_even_for_unverified_ir() {
    use crate::{
        Angle, AngleOp, Authority, CleanupState, Epoch, ExitKind, FirOriginKind, InterfaceBinding,
        InterfaceRole, NodeOrigin, NodePayload, NodeValue, OwnerPath, PathSelector, RegionId,
        ResourceId,
    };
    let mut graph = Graph::default();
    let kinds = [
        RegionKind::ModuleOmega,
        RegionKind::Lambda,
        RegionKind::Gamma,
        RegionKind::Theta,
        RegionKind::Recursion,
        RegionKind::Handler,
        RegionKind::Stage,
        RegionKind::ResidualCfg,
    ];
    let types = [
        Type::Unit,
        Type::Bool,
        Type::Int,
        Type::Double,
        Type::String,
        Type::Result,
        Type::Qubit,
        Type::Unknown,
        Type::Tuple(vec![
            Type::Bool,
            Type::Array(Box::new(Type::Location(Box::new(Type::Int)))),
        ]),
    ];
    let domains = [
        EffectDomain::Memory,
        EffectDomain::Quantum,
        EffectDomain::Result,
        EffectDomain::Allocation,
        EffectDomain::Output,
        EffectDomain::Abort,
        EffectDomain::Host,
        EffectDomain::Noise,
        EffectDomain::Unknown,
    ];
    let modes = [
        EffectMode::Read,
        EffectMode::Write,
        EffectMode::Irreversible,
        EffectMode::Allocate,
        EffectMode::Release,
        EffectMode::Emit,
        EffectMode::Abort,
        EffectMode::Unknown,
    ];
    let origins = [
        NodeOrigin::Qubit(0),
        NodeOrigin::Clbit(0),
        NodeOrigin::Fir {
            package: 0,
            kind: FirOriginKind::Package,
            local_id: 0,
        },
        NodeOrigin::Fir {
            package: 1,
            kind: FirOriginKind::Item,
            local_id: 1,
        },
        NodeOrigin::Fir {
            package: 1,
            kind: FirOriginKind::Block,
            local_id: 2,
        },
        NodeOrigin::Fir {
            package: 1,
            kind: FirOriginKind::Expression,
            local_id: 3,
        },
        NodeOrigin::Fir {
            package: 1,
            kind: FirOriginKind::Pattern,
            local_id: 4,
        },
        NodeOrigin::Fir {
            package: 1,
            kind: FirOriginKind::Statement,
            local_id: 5,
        },
    ];
    for (index, kind) in kinds.into_iter().enumerate() {
        let region = graph.add_region((index != 0).then_some(RegionId(0)), kind);
        let node = graph
            .add_node(
                region,
                "any.operation",
                domains
                    .iter()
                    .enumerate()
                    .map(|(index, domain)| EffectEvent {
                        domain: *domain,
                        mode: modes[index % modes.len()],
                        resource: EffectResource::Unknown,
                        observable: index % 2 == 0,
                    })
                    .collect(),
            )
            .expect("node");
        graph
            .declare_payload(
                node,
                NodePayload {
                    value: Some(NodeValue::Angle(Angle::Binary {
                        op: [AngleOp::Add, AngleOp::Sub, AngleOp::Mul, AngleOp::Div][index % 4],
                        left: Box::new(Angle::literal(-0.0)),
                        right: Box::new(Angle::literal(2.0)),
                    })),
                    origin: Some(origins[index]),
                },
            )
            .expect("payload");
        for (index, ty) in types.iter().enumerate() {
            graph
                .add_port(
                    node,
                    if index % 2 == 0 {
                        Direction::Input
                    } else {
                        Direction::Output
                    },
                    [
                        PortKind::Value,
                        PortKind::Capability,
                        PortKind::HardEffect,
                        PortKind::ControlExit,
                    ][index % 4],
                    ty.clone(),
                    Stage::Residual,
                    if *ty == Type::Qubit {
                        Usage::Affine
                    } else {
                        [Usage::Copyable, Usage::Affine, Usage::Linear][index % 3]
                    },
                )
                .expect("port");
        }
    }
    graph
        .regions
        .get_mut(&RegionId(0))
        .expect("region")
        .interface = crate::RegionInterface {
        arguments: vec![PortId(0), PortId(2)],
        results: vec![PortId(3), PortId(1)],
        exits: [
            ExitKind::Return,
            ExitKind::Abort,
            ExitKind::Branch,
            ExitKind::Backedge,
        ]
        .into_iter()
        .map(|kind| (kind, PortId(0)))
        .collect(),
        bindings: [
            InterfaceRole::Argument,
            InterfaceRole::Result,
            InterfaceRole::Exit,
            InterfaceRole::Capture,
            InterfaceRole::Carried,
        ]
        .into_iter()
        .map(|role| InterfaceBinding {
            outer: PortId(0),
            inner: PortId(1),
            role,
        })
        .collect(),
    };
    graph.regions.get_mut(&RegionId(1)).expect("region").owner = Some(NodeId(0));
    graph.edges = [
        EdgeKind::CopyableValue,
        EdgeKind::AffineCapability,
        EdgeKind::HardEffect,
        EdgeKind::ControlExit,
    ]
    .into_iter()
    .map(|kind| Edge {
        source: PortId(1),
        target: PortId(0),
        kind,
    })
    .collect();
    let states = [
        CleanupState::CleanZ,
        CleanupState::PendingRestore {
            delta: 4,
            history: vec!["a".into(), "b".into()],
        },
        CleanupState::PendingFragment {
            delta: 5,
            fragment_count: 2,
            history: vec![],
        },
        CleanupState::Restored { delta: 5 },
        CleanupState::LiveUnknown,
        CleanupState::Escaped,
        CleanupState::Released,
    ];
    for (index, cleanup) in states.into_iter().enumerate() {
        graph.ownership_mut().authorities.push(Authority {
            path: OwnerPath {
                root: index as u64,
                selectors: vec![
                    PathSelector::Field(String::new()),
                    PathSelector::Index(0),
                    PathSelector::DynamicIndex {
                        symbol: "i".into(),
                        predicate: "i >= 0".into(),
                    },
                    PathSelector::Slice {
                        start: 4,
                        end: 0,
                        step: -1,
                    },
                ],
            },
            resources: BTreeSet::from([ResourceId(index as u64)]),
            epoch: Epoch(index as u64),
            cleanup,
        });
    }
    let bytes = canonical_graph_snapshot(&graph).expect("unverified binary export");
    let (_, packed) = qsc_snapshot::payload(qsc_snapshot::Kind::Graph, &bytes).expect("envelope");
    let (message, mut budget) =
        qsc_snapshot::binary::decode(qsc_snapshot::Kind::Graph, packed).expect("binary");
    let root = message
        .get_root::<qsc_snapshot::snapshot_capnp::document::Reader>()
        .expect("document");
    let qsc_snapshot::snapshot_capnp::document::Graph(snapshot) = root.which().expect("kind")
    else {
        panic!("graph");
    };
    let restored = crate::graph_snapshot_binary::read_payload(
        snapshot.expect("snapshot").get_payload().expect("payload"),
        &mut budget,
    )
    .expect("model");
    assert_eq!(restored.graph, graph);
    let text = graph_snapshot_text(&graph).expect("unverified text export");
    assert_eq!(
        crate::graph_snapshot_text::decode(text.as_bytes())
            .expect("model")
            .0
            .graph,
        graph
    );
    assert!(decode_graph_snapshot(&bytes).is_err());
    assert!(decode_graph_snapshot(text.as_bytes()).is_err());
}

fn cached(
    kind: AnalysisKind,
    scope: AnalysisScope,
    graph: &Graph,
) -> (AnalysisKey, CachedAnalysis) {
    (
        AnalysisKey { kind, scope },
        CachedAnalysis {
            revision: graph.revision(),
            dependencies: BTreeSet::new(),
            value: AnalysisValue::Nodes(BTreeSet::new()),
        },
    )
}

#[test]
fn an_analysis_is_retained_while_the_graph_stands_still() {
    let graph = graph(false);
    let mut cache = AnalysisCache::default();
    let (key, entry) = cached(AnalysisKind::Effects, AnalysisScope::Graph, &graph);
    cache.insert(&graph, key.clone(), entry);
    assert!(cache.get(&graph, &key).is_some());
}

#[test]
fn an_analysis_keyed_to_an_older_graph_is_not_served() {
    let mut graph = graph(false);
    let mut cache = AnalysisCache::default();
    let (key, entry) = cached(AnalysisKind::Effects, AnalysisScope::Graph, &graph);
    cache.insert(&graph, key.clone(), entry);

    let region = graph.regions().next().expect("region").id;
    let mut transaction = graph.transaction();
    transaction
        .add_node(region, "quantum.barrier", vec![])
        .expect("node");
    transaction.commit().expect("commit");

    assert!(cache.get(&graph, &key).is_none());
}

#[test]
fn a_committed_mutation_invalidates_the_analyses_it_touches() {
    let mut graph = graph(false);
    let region = graph.regions().next().expect("region").id;
    let untouched_region = graph.add_region(None, RegionKind::Lambda);

    let mut cache = AnalysisCache::default();
    for (kind, scope) in [
        (AnalysisKind::Demand, AnalysisScope::Graph),
        (
            AnalysisKind::Effects,
            AnalysisScope::Region(u64::from(region.0)),
        ),
        (
            AnalysisKind::Effects,
            AnalysisScope::Region(u64::from(untouched_region.0)),
        ),
    ] {
        let (key, entry) = cached(kind, scope, &graph);
        cache.insert(&graph, key, entry);
    }

    let mut transaction = graph.transaction();
    transaction
        .add_node(region, "quantum.barrier", vec![])
        .expect("node");
    let footprint = transaction.commit().expect("commit");
    let invalid = cache.invalidate(&footprint);

    assert!(invalid.contains(&AnalysisKey {
        kind: AnalysisKind::Demand,
        scope: AnalysisScope::Graph,
    }));
    assert!(invalid.contains(&AnalysisKey {
        kind: AnalysisKind::Effects,
        scope: AnalysisScope::Region(u64::from(region.0)),
    }));
    assert!(
        !invalid.contains(&AnalysisKey {
            kind: AnalysisKind::Effects,
            scope: AnalysisScope::Region(u64::from(untouched_region.0)),
        }),
        "an untouched region keeps its analysis"
    );
}

#[test]
fn an_analysis_of_an_untouched_node_survives_a_mutation_elsewhere() {
    let mut graph = graph(false);
    let region = graph.regions().next().expect("region").id;
    let bystander = graph
        .nodes()
        .next()
        .and_then(|node| graph.stable_id(node.id))
        .expect("a live node");

    let mut cache = AnalysisCache::default();
    let (key, entry) = cached(
        AnalysisKind::Ownership,
        AnalysisScope::Node(bystander),
        &graph,
    );
    cache.insert(&graph, key.clone(), entry);

    let mut transaction = graph.transaction();
    transaction
        .add_node(region, "quantum.barrier", vec![])
        .expect("node");
    let footprint = transaction.commit().expect("commit");
    assert!(!cache.invalidate(&footprint).contains(&key));
}
