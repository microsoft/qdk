use crate::{
    BuildError, Direction, Edge, EdgeKind, GraphBuilder, InterfaceBinding, InterfaceRole, PortKind,
    RegionKind, Stage, Type, Usage,
};

#[test]
fn explicit_region_binding_allows_typed_crossing() {
    let mut builder = GraphBuilder::new();
    let root = builder.add_region(None, RegionKind::ModuleOmega);
    let child = builder.add_region(Some(root), RegionKind::Gamma);
    let outer_node = builder
        .add_node(root, "qdk.value.constant", vec![])
        .expect("root node");
    let inner_node = builder
        .add_node(child, "qdk.value.tuple", vec![])
        .expect("child node");
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
            Type::Int,
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("inner port");
    builder
        .bind_interface(
            child,
            InterfaceBinding {
                outer,
                inner,
                role: InterfaceRole::Argument,
            },
        )
        .expect("binding");
    builder
        .connect(Edge {
            source: outer,
            target: inner,
            kind: EdgeKind::CopyableValue,
        })
        .expect("edge");
}

#[test]
fn builder_rejects_undeclared_region_crossing() {
    let mut builder = GraphBuilder::new();
    let root = builder.add_region(None, RegionKind::ModuleOmega);
    let child = builder.add_region(Some(root), RegionKind::Lambda);
    let outer_node = builder.add_node(root, "outer", vec![]).expect("outer node");
    let inner_node = builder
        .add_node(child, "inner", vec![])
        .expect("inner node");
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
            Type::Int,
            Stage::Generation,
            Usage::Copyable,
        )
        .expect("inner port");
    assert_eq!(
        builder
            .connect(Edge {
                source: outer,
                target: inner,
                kind: EdgeKind::CopyableValue,
            })
            .expect_err("crossing must have a binding"),
        BuildError::InterfaceMismatch
    );
}
