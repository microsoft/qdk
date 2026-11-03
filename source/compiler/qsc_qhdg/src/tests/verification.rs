use qsc_semantic_contracts::{EffectDomain, EffectMode};

use crate::{
    Direction, Edge, EdgeKind, EffectEvent, EffectResource, GraphBuilder, PortKind, RegionKind,
    RuleCode, Stage, Type, Usage, verify,
};

#[test]
fn an_observable_effect_without_a_demand_is_rejected() {
    let mut builder = GraphBuilder::new();
    let root = builder.add_region(None, RegionKind::ModuleOmega);
    builder
        .add_node(
            root,
            "qdk.quantum.gate",
            vec![EffectEvent {
                domain: EffectDomain::Quantum,
                mode: EffectMode::Write,
                resource: EffectResource::Named("q0".to_string()),
                observable: true,
            }],
        )
        .expect("effect node");
    let diagnostics = verify(&builder.finish());
    assert_eq!(diagnostics[0].code, RuleCode::EffectReachability);
}

#[test]
fn an_affine_capability_with_two_consumers_is_rejected() {
    let mut builder = GraphBuilder::new();
    let root = builder.add_region(None, RegionKind::ModuleOmega);
    let source = builder.add_node(root, "owner", vec![]).expect("owner");
    let first = builder.add_node(root, "use-1", vec![]).expect("use 1");
    let second = builder.add_node(root, "use-2", vec![]).expect("use 2");
    let output = builder
        .add_port(
            source,
            Direction::Output,
            PortKind::Capability,
            Type::Qubit,
            Stage::Residual,
            Usage::Affine,
        )
        .expect("owner output");
    for target in [first, second] {
        let input = builder
            .add_port(
                target,
                Direction::Input,
                PortKind::Capability,
                Type::Qubit,
                Stage::Residual,
                Usage::Affine,
            )
            .expect("owner input");
        builder
            .connect(Edge {
                source: output,
                target: input,
                kind: EdgeKind::AffineCapability,
            })
            .expect("authority edge");
    }
    let diagnostics = verify(&builder.finish());
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == RuleCode::OwnershipFanout)
    );
}

#[test]
fn an_undeclared_value_cycle_is_rejected() {
    let mut builder = GraphBuilder::new();
    let root = builder.add_region(None, RegionKind::ModuleOmega);
    let left = builder.add_node(root, "left", vec![]).expect("left");
    let right = builder.add_node(root, "right", vec![]).expect("right");
    let left_out = builder
        .add_port(
            left,
            Direction::Output,
            PortKind::Value,
            Type::Int,
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("left out");
    let left_in = builder
        .add_port(
            left,
            Direction::Input,
            PortKind::Value,
            Type::Int,
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("left in");
    let right_out = builder
        .add_port(
            right,
            Direction::Output,
            PortKind::Value,
            Type::Int,
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("right out");
    let right_in = builder
        .add_port(
            right,
            Direction::Input,
            PortKind::Value,
            Type::Int,
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("right in");
    builder
        .connect(Edge {
            source: left_out,
            target: right_in,
            kind: EdgeKind::CopyableValue,
        })
        .expect("forward");
    builder
        .connect(Edge {
            source: right_out,
            target: left_in,
            kind: EdgeKind::CopyableValue,
        })
        .expect("back");
    let diagnostics = verify(&builder.finish());
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == RuleCode::RegionCycle)
    );
}

#[test]
fn a_cross_region_type_mismatch_is_rejected() {
    let mut builder = GraphBuilder::new();
    let root = builder.add_region(None, RegionKind::ModuleOmega);
    let child = builder.add_region(Some(root), RegionKind::Gamma);
    let outer_node = builder.add_node(root, "outer", vec![]).expect("outer");
    let inner_node = builder.add_node(child, "inner", vec![]).expect("inner");
    let outer = builder
        .add_port(
            outer_node,
            Direction::Output,
            PortKind::Value,
            Type::Int,
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("outer port");
    let inner = builder
        .add_port(
            inner_node,
            Direction::Input,
            PortKind::Value,
            Type::Bool,
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("inner port");
    builder.connect_unchecked(Edge {
        source: outer,
        target: inner,
        kind: EdgeKind::CopyableValue,
    });
    let diagnostics = verify(&builder.finish());
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == RuleCode::RegionType)
    );
}

#[test]
fn an_effect_only_cycle_reports_the_expected_diagnostic() {
    let mut builder = GraphBuilder::new();
    let root = builder.add_region(None, RegionKind::ModuleOmega);
    let left = builder.add_node(root, "left", vec![]).expect("left");
    let right = builder.add_node(root, "right", vec![]).expect("right");
    let left_out = builder
        .add_port(
            left,
            Direction::Output,
            PortKind::HardEffect,
            Type::Unit,
            Stage::Residual,
            Usage::Linear,
        )
        .expect("left output");
    let left_in = builder
        .add_port(
            left,
            Direction::Input,
            PortKind::HardEffect,
            Type::Unit,
            Stage::Residual,
            Usage::Linear,
        )
        .expect("left input");
    let right_out = builder
        .add_port(
            right,
            Direction::Output,
            PortKind::HardEffect,
            Type::Unit,
            Stage::Residual,
            Usage::Linear,
        )
        .expect("right output");
    let right_in = builder
        .add_port(
            right,
            Direction::Input,
            PortKind::HardEffect,
            Type::Unit,
            Stage::Residual,
            Usage::Linear,
        )
        .expect("right input");
    for (source, target) in [(left_out, right_in), (right_out, left_in)] {
        builder
            .connect(Edge {
                source,
                target,
                kind: EdgeKind::HardEffect,
            })
            .expect("effect edge");
    }

    assert_eq!(
        verify(&builder.finish()),
        vec![crate::Diagnostic {
            code: RuleCode::EffectCycle,
            node: None,
            message: "hard-effect dependency cycle has no legal schedule".to_string(),
        }]
    );
}
