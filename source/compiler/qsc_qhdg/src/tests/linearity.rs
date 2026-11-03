// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The linearity rules stated as tests rather than as probes.
//!
//! No-cloning and no-deleting are the two halves of what a quantum capability
//! owes, and each half is exercised here against the smallest graph that can
//! violate it.

use crate::{
    Direction, Edge, EdgeKind, Graph, GraphBuilder, NodeId, PortId, PortKind, RegionKind, RuleCode,
    Stage, Type, Usage, verify,
};

fn codes(graph: &Graph) -> Vec<RuleCode> {
    verify(graph)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
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

/// One capability output reaching two consumers, which is the shape no-cloning
/// forbids and the one the rule already catches. It is the control for the two
/// holes below: a change that closes them and silences this has broken more
/// than it fixed.
#[test]
fn a_capability_reaching_two_consumers_is_reported() {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let producer = builder
        .add_node(region, "quantum.allocate", Vec::new())
        .expect("producer");
    let source = capability(&mut builder, producer, Direction::Output);
    for operation in ["quantum.h", "quantum.release"] {
        let consumer = builder
            .add_node(region, operation, Vec::new())
            .expect("consumer");
        let target = capability(&mut builder, consumer, Direction::Input);
        builder
            .connect(Edge {
                source,
                target,
                kind: EdgeKind::AffineCapability,
            })
            .expect("capability edge");
    }

    assert_eq!(codes(&builder.finish()), vec![RuleCode::OwnershipFanout]);
}

/// A capability allocated, operated on, and then dropped. Affine typing permits
/// the drop and quantum mechanics does not: the qubit is still allocated and
/// still entangled with whatever it touched.
#[test]
fn a_capability_that_is_produced_and_never_consumed_is_reported() {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let producer = builder
        .add_node(region, "quantum.allocate", Vec::new())
        .expect("producer");
    let gate = builder
        .add_node(region, "quantum.h", Vec::new())
        .expect("gate");
    let allocated = capability(&mut builder, producer, Direction::Output);
    let consumed = capability(&mut builder, gate, Direction::Input);
    let dropped = capability(&mut builder, gate, Direction::Output);
    builder
        .connect(Edge {
            source: allocated,
            target: consumed,
            kind: EdgeKind::AffineCapability,
        })
        .expect("capability edge");

    let graph = builder.finish();
    assert_eq!(codes(&graph), vec![RuleCode::OwnershipDrop]);
    assert_eq!(
        verify(&graph)[0].node,
        Some(graph.port(dropped).expect("dropped port").node)
    );
}

/// The same capability, released rather than dropped. The circuit lowering
/// emits this release for every live qubit, which is why every graph a circuit
/// produces stays clean under the rule above.
#[test]
fn a_capability_that_reaches_a_release_is_not_reported() {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let producer = builder
        .add_node(region, "quantum.allocate", Vec::new())
        .expect("producer");
    let sink = builder
        .add_node(region, "quantum.release", Vec::new())
        .expect("sink");
    let allocated = capability(&mut builder, producer, Direction::Output);
    let released = capability(&mut builder, sink, Direction::Input);
    builder
        .connect(Edge {
            source: allocated,
            target: released,
            kind: EdgeKind::AffineCapability,
        })
        .expect("capability edge");

    assert!(verify(&builder.finish()).is_empty());
}

/// A qubit inside an array is still a qubit. The fan-out rule used to key on
/// the port kind the producer chose, so declaring the same array as a copyable
/// value moved it outside the rule without changing what it carries.
#[test]
fn a_compound_type_carrying_a_qubit_cannot_be_fanned_out() {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let producer = builder
        .add_node(region, "quantum.allocate_array", Vec::new())
        .expect("producer");
    let array = Type::Array(Box::new(Type::Qubit));
    let source = builder
        .add_port(
            producer,
            Direction::Output,
            PortKind::Value,
            array.clone(),
            Stage::Generation,
            Usage::Affine,
        )
        .expect("array port");
    for operation in ["quantum.apply_all", "quantum.release_array"] {
        let consumer = builder
            .add_node(region, operation, Vec::new())
            .expect("consumer");
        let target = builder
            .add_port(
                consumer,
                Direction::Input,
                PortKind::Value,
                array.clone(),
                Stage::Generation,
                Usage::Affine,
            )
            .expect("array port");
        builder
            .connect(Edge {
                source,
                target,
                kind: EdgeKind::CopyableValue,
            })
            .expect("value edge");
    }

    assert_eq!(codes(&builder.finish()), vec![RuleCode::OwnershipFanout]);
}

/// A type that carries no qubit is copied freely, so the rule above narrows
/// what is permitted rather than replacing it.
#[test]
fn a_compound_type_carrying_no_qubit_is_fanned_out_freely() {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let producer = builder
        .add_node(region, "qdk.value.constant", Vec::new())
        .expect("producer");
    let array = Type::Array(Box::new(Type::Int));
    let source = builder
        .add_port(
            producer,
            Direction::Output,
            PortKind::Value,
            array.clone(),
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("array port");
    for operation in ["qdk.value.use", "qdk.value.use_again"] {
        let consumer = builder
            .add_node(region, operation, Vec::new())
            .expect("consumer");
        let target = builder
            .add_port(
                consumer,
                Direction::Input,
                PortKind::Value,
                array.clone(),
                Stage::Generation,
                Usage::Copyable,
            )
            .expect("array port");
        builder
            .connect(Edge {
                source,
                target,
                kind: EdgeKind::CopyableValue,
            })
            .expect("value edge");
    }

    assert!(verify(&builder.finish()).is_empty());
}
