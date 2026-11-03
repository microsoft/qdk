// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Explicit typed wire projection; no Serde representation participates.

use crate::{
    Adjacency, Angle, AngleOp, Authority, Borrow, BorrowKind, CleanupState, Direction, Edge,
    EdgeKind, EffectDomain, EffectEvent, EffectMode, EffectResource, Epoch, ExitKind,
    FirOriginKind, InterfaceBinding, InterfaceRole, NodeId, NodeOrigin, NodePayload, NodeValue,
    OperationIdentity, OwnerPath, OwnershipState, PathSelector, PortId, PortKind, Real, Region,
    RegionId, RegionInterface, RegionKind, ResourceId, StableNodeId, Stage, SymbolId, Type, Usage,
    graph::{Graph, Node, Port},
    graph_snapshot::{Payload, SnapshotError, unique_map, unique_set},
    identity::NodeIdentity,
};
use qsc_snapshot::{Budget, Error, depth, length, snapshot_capnp as w};
use std::{collections::BTreeSet, sync::Arc};

macro_rules! enumeration {
    ($encode:ident, $decode:ident, $domain:ident, $wire:ident, {$($a:ident => $b:ident),+ $(,)?}) => {
        fn $encode(value: $domain) -> w::$wire { match value { $($domain::$a => w::$wire::$b),+ } }
        fn $decode(value: w::$wire) -> Result<$domain, Error> {
            match value { $(w::$wire::$b => Ok($domain::$a),)+ _ => Err(Error::malformed("invalid enum")) }
        }
    };
}
enumeration!(region_kind, read_region_kind, RegionKind, RegionKind, {ModuleOmega=>ModuleOmega,Lambda=>Lambda,Gamma=>Gamma,Theta=>Theta,Recursion=>Recursion,Handler=>Handler,Stage=>Stage,ResidualCfg=>ResidualCfg});
enumeration!(exit_kind, read_exit_kind, ExitKind, ExitKind, {Return=>Return,Abort=>Abort,Branch=>Branch,Backedge=>Backedge});
enumeration!(role, read_role, InterfaceRole, InterfaceRole, {Argument=>Argument,Result=>Result,Exit=>Exit,Capture=>Capture,Carried=>Carried});
enumeration!(direction, read_direction, Direction, Direction, {Input=>Input,Output=>Output});
enumeration!(port_kind, read_port_kind, PortKind, PortKind, {Value=>Value,Capability=>Capability,HardEffect=>HardEffect,ControlExit=>ControlExit});
enumeration!(stage, read_stage, Stage, Stage, {Generation=>Generation,Residual=>Residual});
enumeration!(usage, read_usage, Usage, Usage, {Copyable=>Copyable,Affine=>Affine,Linear=>Linear});
enumeration!(edge_kind, read_edge_kind, EdgeKind, EdgeKind, {CopyableValue=>CopyableValue,AffineCapability=>AffineCapability,HardEffect=>HardEffect,ControlExit=>ControlExit});
enumeration!(effect_domain, read_effect_domain, EffectDomain, EffectDomain, {Memory=>Memory,Quantum=>Quantum,Result=>Result,Allocation=>Allocation,Output=>Output,Abort=>Abort,Host=>Host,Noise=>Noise,Unknown=>Unknown});
enumeration!(effect_mode, read_effect_mode, EffectMode, EffectMode, {Read=>Read,Write=>Write,Irreversible=>Irreversible,Allocate=>Allocate,Release=>Release,Emit=>Emit,Abort=>Abort,Unknown=>Unknown});
enumeration!(angle_op, read_angle_op, AngleOp, AngleOp, {Add=>Add,Sub=>Sub,Mul=>Mul,Div=>Div});
enumeration!(fir_kind, read_fir_kind, FirOriginKind, FirOriginKind, {Package=>Package,Item=>Item,Block=>Block,Expression=>Expression,Pattern=>Pattern,Statement=>Statement});
enumeration!(borrow_kind, read_borrow_kind, BorrowKind, BorrowKind, {Shared=>Shared,Exclusive=>Exclusive});

fn optional32(mut out: w::optional_id32::Builder<'_>, value: Option<u32>) {
    match value {
        None => out.set_none(()),
        Some(value) => out.set_some(value),
    }
}
fn read_optional32(value: w::optional_id32::Reader<'_>) -> Result<Option<u32>, Error> {
    Ok(match value.which()? {
        w::optional_id32::None(()) => None,
        w::optional_id32::Some(value) => Some(value),
    })
}
fn optional64(mut out: w::optional_id64::Builder<'_>, value: Option<u64>) {
    match value {
        None => out.set_none(()),
        Some(value) => out.set_some(value),
    }
}
fn read_optional64(value: w::optional_id64::Reader<'_>) -> Result<Option<u64>, Error> {
    Ok(match value.which()? {
        w::optional_id64::None(()) => None,
        w::optional_id64::Some(value) => Some(value),
    })
}
fn ids(
    mut out: capnp::primitive_list::Builder<'_, u32>,
    values: impl Iterator<Item = u32>,
) -> Result<(), Error> {
    for (index, value) in values.enumerate() {
        out.set(length(index)?, value);
    }
    Ok(())
}
fn read_ids<T>(
    values: capnp::primitive_list::Reader<'_, u32>,
    budget: &mut Budget,
    wrap: impl Fn(u32) -> T,
) -> Result<Vec<T>, Error> {
    budget.charge(values.len() as usize)?;
    Ok(values.iter().map(wrap).collect())
}
fn strings(
    mut out: capnp::text_list::Builder<'_>,
    values: impl Iterator<Item = impl AsRef<str>>,
) -> Result<(), Error> {
    for (index, value) in values.enumerate() {
        out.set(length(index)?, value.as_ref());
    }
    Ok(())
}
fn read_strings(
    values: capnp::text_list::Reader<'_>,
    budget: &mut Budget,
) -> Result<Vec<String>, Error> {
    budget.charge(values.len() as usize)?;
    values
        .iter()
        .map(|value| qsc_snapshot::string(value?))
        .collect()
}

pub(crate) fn write_payload(
    mut out: w::graph_payload::Builder<'_>,
    graph: &Graph,
    revision: u64,
    required: &BTreeSet<String>,
    optional: &BTreeSet<String>,
) -> Result<(), Error> {
    out.set_version(super::graph_snapshot::GRAPH_SNAPSHOT_SCHEMA_VERSION);
    out.set_registry_version(super::graph_snapshot::CONTRACT_REGISTRY_VERSION);
    strings(
        out.reborrow()
            .init_required_features(length(required.len())?),
        required.iter(),
    )?;
    strings(
        out.reborrow()
            .init_optional_features(length(optional.len())?),
        optional.iter(),
    )?;
    write_graph(out.init_checkpoint(), graph, revision)
}

pub(crate) fn read_payload(
    value: w::graph_payload::Reader<'_>,
    budget: &mut Budget,
) -> Result<Payload, SnapshotError> {
    let version = value.get_version();
    let registry_version = value.get_registry_version();
    let required_features = unique_set(read_strings(
        value.get_required_features().map_err(Error::from)?,
        budget,
    )?)?;
    super::graph_snapshot::check_metadata(version, registry_version, &required_features)?;
    Ok(Payload {
        version,
        registry_version,
        required_features,
        optional_features: unique_set(read_strings(
            value.get_optional_features().map_err(Error::from)?,
            budget,
        )?)?,
        graph: read_graph(value.get_checkpoint().map_err(Error::from)?, budget)?,
    })
}

fn write_graph(
    mut out: w::graph_checkpoint::Builder<'_>,
    graph: &Graph,
    revision: u64,
) -> Result<(), Error> {
    let mut regions = out.reborrow().init_regions(length(graph.regions.len())?);
    for (index, region) in graph.regions.values().enumerate() {
        write_region(regions.reborrow().get(length(index)?), region)?;
    }
    let mut nodes = out.reborrow().init_nodes(length(graph.nodes.len())?);
    for (index, node) in graph.nodes.values().enumerate() {
        write_node(nodes.reborrow().get(length(index)?), node)?;
    }
    let mut ports = out.reborrow().init_ports(length(graph.ports.len())?);
    for (index, port) in graph.ports.values().enumerate() {
        write_port(ports.reborrow().get(length(index)?), port)?;
    }
    let mut edges = out.reborrow().init_edges(length(graph.edges.len())?);
    for (index, edge) in graph.edges.iter().enumerate() {
        write_edge(edges.reborrow().get(length(index)?), edge);
    }
    ids(
        out.reborrow()
            .init_demands(length(graph.demanded_nodes.len())?),
        graph.demanded_nodes.iter().map(|id| id.0),
    )?;
    write_identity(out.reborrow().init_identity(), &graph.identity)?;
    write_ownership(out.reborrow().init_ownership(), &graph.ownership)?;
    out.set_revision(revision);
    out.set_next_region(graph.next_region);
    out.set_next_port(graph.next_port);
    Ok(())
}

macro_rules! read_records {
    ($values:expr, $budget:expr, $read:ident) => {{
        let values = $values?;
        $budget.charge(values.len() as usize)?;
        values
            .iter()
            .map(|value| $read(value, $budget))
            .collect::<Result<Vec<_>, Error>>()?
    }};
}

fn read_graph(value: w::graph_checkpoint::Reader<'_>, budget: &mut Budget) -> Result<Graph, Error> {
    Ok(Graph {
        cache_identity: Arc::new(()),
        regions: unique_map(
            read_records!(value.get_regions(), budget, read_region),
            |region| region.id,
        )?,
        nodes: unique_map(
            read_records!(value.get_nodes(), budget, read_node),
            |node| node.id,
        )?,
        ports: unique_map(
            read_records!(value.get_ports(), budget, read_port),
            |port| port.id,
        )?,
        edges: read_records!(value.get_edges(), budget, read_edge),
        demanded_nodes: read_ids(value.get_demands()?, budget, NodeId)?,
        identity: read_identity(value.get_identity()?, budget)?,
        ownership: read_ownership(value.get_ownership()?, budget)?,
        revision: value.get_revision(),
        next_region: value.get_next_region(),
        next_port: value.get_next_port(),
    })
}

fn write_region(mut out: w::region::Builder<'_>, region: &Region) -> Result<(), Error> {
    out.set_id(region.id.0);
    out.set_kind(region_kind(region.kind));
    optional32(out.reborrow().init_parent(), region.parent.map(|id| id.0));
    optional32(out.reborrow().init_owner(), region.owner.map(|id| id.0));
    let mut interface = out.init_interface();
    ids(
        interface
            .reborrow()
            .init_arguments(length(region.interface.arguments.len())?),
        region.interface.arguments.iter().map(|id| id.0),
    )?;
    ids(
        interface
            .reborrow()
            .init_results(length(region.interface.results.len())?),
        region.interface.results.iter().map(|id| id.0),
    )?;
    let mut exits = interface
        .reborrow()
        .init_exits(length(region.interface.exits.len())?);
    for (index, (kind, port)) in region.interface.exits.iter().enumerate() {
        let mut out = exits.reborrow().get(length(index)?);
        out.set_kind(exit_kind(*kind));
        out.set_port(port.0);
    }
    let mut bindings = interface.init_bindings(length(region.interface.bindings.len())?);
    for (index, binding) in region.interface.bindings.iter().enumerate() {
        let mut out = bindings.reborrow().get(length(index)?);
        out.set_outer(binding.outer.0);
        out.set_inner(binding.inner.0);
        out.set_role(role(binding.role));
    }
    Ok(())
}
fn read_region(value: w::region::Reader<'_>, budget: &mut Budget) -> Result<Region, Error> {
    let interface = value.get_interface()?;
    let exits = interface.get_exits()?;
    budget.charge(exits.len() as usize)?;
    let exits = exits
        .iter()
        .map(|exit| Ok((read_exit_kind(exit.get_kind()?)?, PortId(exit.get_port()))))
        .collect::<Result<_, Error>>()?;
    let bindings = interface.get_bindings()?;
    budget.charge(bindings.len() as usize)?;
    let bindings = bindings
        .iter()
        .map(|binding| {
            Ok(InterfaceBinding {
                outer: PortId(binding.get_outer()),
                inner: PortId(binding.get_inner()),
                role: read_role(binding.get_role()?)?,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(Region {
        id: RegionId(value.get_id()),
        parent: read_optional32(value.get_parent()?)?.map(RegionId),
        kind: read_region_kind(value.get_kind()?)?,
        owner: read_optional32(value.get_owner()?)?.map(NodeId),
        interface: RegionInterface {
            arguments: read_ids(interface.get_arguments()?, budget, PortId)?,
            results: read_ids(interface.get_results()?, budget, PortId)?,
            exits,
            bindings,
        },
    })
}
fn write_node(mut out: w::node::Builder<'_>, node: &Node) -> Result<(), Error> {
    out.set_id(node.id.0);
    out.set_region(node.region.0);
    out.set_operation(node.operation.0.as_str());
    ids(
        out.reborrow().init_inputs(length(node.inputs.len())?),
        node.inputs.iter().map(|id| id.0),
    )?;
    ids(
        out.reborrow().init_outputs(length(node.outputs.len())?),
        node.outputs.iter().map(|id| id.0),
    )?;
    write_effects(
        out.reborrow().init_effects(length(node.effects.len())?),
        &node.effects,
    )?;
    write_symbol(out.reborrow().init_symbol(), node.symbol.as_ref());
    write_payload_value(out.init_payload(), &node.payload)
}
fn read_node(value: w::node::Reader<'_>, budget: &mut Budget) -> Result<Node, Error> {
    Ok(Node {
        id: NodeId(value.get_id()),
        region: RegionId(value.get_region()),
        operation: OperationIdentity(qsc_snapshot::string(value.get_operation()?)?),
        inputs: read_ids(value.get_inputs()?, budget, PortId)?,
        outputs: read_ids(value.get_outputs()?, budget, PortId)?,
        effects: read_effects(value.get_effects()?, budget)?,
        symbol: read_symbol(value.get_symbol()?)?,
        payload: read_payload_value(value.get_payload()?, budget)?,
    })
}
fn write_symbol(mut out: w::optional_symbol::Builder<'_>, symbol: Option<&SymbolId>) {
    if let Some(symbol) = symbol {
        let mut value = out.init_some();
        value.set_package(symbol.package.as_str());
        value.set_item(symbol.item.as_str());
    } else {
        out.set_none(());
    }
}
fn read_symbol(value: w::optional_symbol::Reader<'_>) -> Result<Option<SymbolId>, Error> {
    Ok(match value.which()? {
        w::optional_symbol::None(()) => None,
        w::optional_symbol::Some(value) => {
            let value = value?;
            Some(SymbolId {
                package: qsc_snapshot::string(value.get_package()?)?,
                item: qsc_snapshot::string(value.get_item()?)?,
            })
        }
    })
}
fn write_port(mut out: w::port::Builder<'_>, port: &Port) -> Result<(), Error> {
    out.set_id(port.id.0);
    out.set_node(port.node.0);
    out.set_region(port.region.0);
    out.set_direction(direction(port.direction));
    out.set_kind(port_kind(port.kind));
    out.set_stage(stage(port.stage));
    out.set_usage(usage(port.usage));
    optional64(out.reborrow().init_resource(), port.resource.map(|id| id.0));
    write_type(out.init_type(), &port.ty, 0)
}
fn read_port(value: w::port::Reader<'_>, budget: &mut Budget) -> Result<Port, Error> {
    Ok(Port {
        id: PortId(value.get_id()),
        node: NodeId(value.get_node()),
        region: RegionId(value.get_region()),
        direction: read_direction(value.get_direction()?)?,
        kind: read_port_kind(value.get_kind()?)?,
        ty: read_type(value.get_type()?, budget, 0)?,
        stage: read_stage(value.get_stage()?)?,
        usage: read_usage(value.get_usage()?)?,
        resource: read_optional64(value.get_resource()?)?.map(ResourceId),
    })
}
fn write_type(mut out: w::type_::Builder<'_>, ty: &Type, nesting: usize) -> Result<(), Error> {
    depth(nesting)?;
    match ty {
        Type::Unit => out.set_unit(()),
        Type::Bool => out.set_boolean(()),
        Type::Int => out.set_integer(()),
        Type::Double => out.set_double(()),
        Type::String => out.set_string(()),
        Type::Result => out.set_result(()),
        Type::Qubit => out.set_qubit(()),
        Type::Unknown => out.set_unknown(()),
        Type::Tuple(items) => {
            let mut list = out.init_tuple(length(items.len())?);
            for (index, item) in items.iter().enumerate() {
                write_type(list.reborrow().get(length(index)?), item, nesting + 1)?;
            }
        }
        Type::Array(item) => write_type(out.init_array(), item, nesting + 1)?,
        Type::Location(item) => write_type(out.init_location(), item, nesting + 1)?,
    }
    Ok(())
}
fn read_type(
    value: w::type_::Reader<'_>,
    budget: &mut Budget,
    nesting: usize,
) -> Result<Type, Error> {
    depth(nesting)?;
    Ok(match value.which()? {
        w::type_::Unit(()) => Type::Unit,
        w::type_::Boolean(()) => Type::Bool,
        w::type_::Integer(()) => Type::Int,
        w::type_::Double(()) => Type::Double,
        w::type_::String(()) => Type::String,
        w::type_::Result(()) => Type::Result,
        w::type_::Qubit(()) => Type::Qubit,
        w::type_::Unknown(()) => Type::Unknown,
        w::type_::Tuple(items) => {
            let items = items?;
            budget.charge(items.len() as usize)?;
            Type::Tuple(
                items
                    .iter()
                    .map(|item| read_type(item, budget, nesting + 1))
                    .collect::<Result<_, _>>()?,
            )
        }
        w::type_::Array(item) => {
            budget.charge(1)?;
            Type::Array(Box::new(read_type(item?, budget, nesting + 1)?))
        }
        w::type_::Location(item) => {
            budget.charge(1)?;
            Type::Location(Box::new(read_type(item?, budget, nesting + 1)?))
        }
        w::type_::Invalid(()) => return Err(Error::malformed("invalid type")),
    })
}
fn write_edge(mut out: w::edge::Builder<'_>, edge: &Edge) {
    out.set_source(edge.source.0);
    out.set_target(edge.target.0);
    out.set_kind(edge_kind(edge.kind));
}
fn read_edge(value: w::edge::Reader<'_>, _: &mut Budget) -> Result<Edge, Error> {
    Ok(Edge {
        source: PortId(value.get_source()),
        target: PortId(value.get_target()),
        kind: read_edge_kind(value.get_kind()?)?,
    })
}
fn write_effects(
    mut out: capnp::struct_list::Builder<'_, w::effect::Owned>,
    effects: &[EffectEvent],
) -> Result<(), Error> {
    for (index, effect) in effects.iter().enumerate() {
        let mut value = out.reborrow().get(length(index)?);
        value.set_domain(effect_domain(effect.domain));
        value.set_mode(effect_mode(effect.mode));
        value.set_observable(effect.observable);
        let mut resource = value.init_resource();
        match &effect.resource {
            EffectResource::Named(name) => resource.set_named(name.as_str()),
            EffectResource::Unknown => resource.set_unknown(()),
        }
    }
    Ok(())
}
fn read_effects(
    values: capnp::struct_list::Reader<'_, w::effect::Owned>,
    budget: &mut Budget,
) -> Result<Vec<EffectEvent>, Error> {
    budget.charge(values.len() as usize)?;
    values
        .iter()
        .map(|value| {
            Ok(EffectEvent {
                domain: read_effect_domain(value.get_domain()?)?,
                mode: read_effect_mode(value.get_mode()?)?,
                observable: value.get_observable(),
                resource: match value.get_resource()?.which()? {
                    w::effect_resource::Unknown(()) => EffectResource::Unknown,
                    w::effect_resource::Named(name) => {
                        EffectResource::Named(qsc_snapshot::string(name?)?)
                    }
                },
            })
        })
        .collect()
}
fn write_payload_value(
    mut out: w::node_payload::Builder<'_>,
    payload: &NodePayload,
) -> Result<(), Error> {
    let mut value = out.reborrow().init_value();
    match &payload.value {
        None => value.set_none(()),
        Some(NodeValue::Number(real)) => value.init_number().set_bits(real.get().to_bits()),
        Some(NodeValue::ClassicalBit(id)) => value.set_classical_bit(*id),
        Some(NodeValue::Angle(angle)) => write_angle(value.init_angle(), angle, 0)?,
    }
    let mut origin = out.init_origin();
    match payload.origin {
        None => origin.set_none(()),
        Some(NodeOrigin::Instruction(id)) => origin.set_instruction(id),
        Some(NodeOrigin::Qubit(id)) => origin.set_qubit(id),
        Some(NodeOrigin::Clbit(id)) => origin.set_clbit(id),
        Some(NodeOrigin::GlobalPhase) => origin.set_global_phase(()),
        Some(NodeOrigin::Fir {
            package,
            kind,
            local_id,
        }) => {
            let mut value = origin.init_fir();
            value.set_package(package);
            value.set_kind(fir_kind(kind));
            value.set_local_id(local_id);
        }
    }
    Ok(())
}
fn read_payload_value(
    value: w::node_payload::Reader<'_>,
    budget: &mut Budget,
) -> Result<NodePayload, Error> {
    Ok(NodePayload {
        value: match value.get_value()?.which()? {
            w::node_value::None(()) => None,
            w::node_value::Number(value) => Some(NodeValue::Number(Real::new(f64::from_bits(
                value?.get_bits(),
            )))),
            w::node_value::ClassicalBit(value) => Some(NodeValue::ClassicalBit(value)),
            w::node_value::Angle(value) => Some(NodeValue::Angle(read_angle(value?, budget, 0)?)),
        },
        origin: match value.get_origin()?.which()? {
            w::node_origin::None(()) => None,
            w::node_origin::Instruction(id) => Some(NodeOrigin::Instruction(id)),
            w::node_origin::Qubit(id) => Some(NodeOrigin::Qubit(id)),
            w::node_origin::Clbit(id) => Some(NodeOrigin::Clbit(id)),
            w::node_origin::GlobalPhase(()) => Some(NodeOrigin::GlobalPhase),
            w::node_origin::Fir(value) => {
                let value = value?;
                Some(NodeOrigin::Fir {
                    package: value.get_package(),
                    kind: read_fir_kind(value.get_kind()?)?,
                    local_id: value.get_local_id(),
                })
            }
        },
    })
}
fn write_angle(
    mut out: w::graph_angle::Builder<'_>,
    angle: &Angle,
    nesting: usize,
) -> Result<(), Error> {
    depth(nesting)?;
    match angle {
        Angle::Literal { value } => out.init_literal().set_bits(value.get().to_bits()),
        Angle::Parameter { name } => out.set_parameter(name.as_str()),
        Angle::Neg { operand } => write_angle(out.init_neg(), operand, nesting + 1)?,
        Angle::Binary { op, left, right } => {
            let mut value = out.init_binary();
            value.set_op(angle_op(*op));
            write_angle(value.reborrow().init_left(), left, nesting + 1)?;
            write_angle(value.init_right(), right, nesting + 1)?;
        }
    }
    Ok(())
}
fn read_angle(
    value: w::graph_angle::Reader<'_>,
    budget: &mut Budget,
    nesting: usize,
) -> Result<Angle, Error> {
    depth(nesting)?;
    budget.charge(1)?;
    Ok(match value.which()? {
        w::graph_angle::Literal(value) => Angle::Literal {
            value: Real::new(f64::from_bits(value?.get_bits())),
        },
        w::graph_angle::Parameter(name) => Angle::Parameter {
            name: qsc_snapshot::string(name?)?,
        },
        w::graph_angle::Neg(value) => Angle::Neg {
            operand: Box::new(read_angle(value?, budget, nesting + 1)?),
        },
        w::graph_angle::Binary(value) => {
            let value = value?;
            Angle::Binary {
                op: read_angle_op(value.get_op()?)?,
                left: Box::new(read_angle(value.get_left()?, budget, nesting + 1)?),
                right: Box::new(read_angle(value.get_right()?, budget, nesting + 1)?),
            }
        }
        w::graph_angle::Invalid(()) => return Err(Error::malformed("invalid angle")),
    })
}
fn write_identity(
    mut out: w::identity_state::Builder<'_>,
    identity: &NodeIdentity,
) -> Result<(), Error> {
    let (live, generations, retired, free, next_slot, next_stable) = identity.snapshot_parts();
    let mut entries = out.reborrow().init_live(length(live.len())?);
    for (index, (node, stable)) in live.iter().enumerate() {
        let mut value = entries.reborrow().get(length(index)?);
        value.set_node(node.0);
        value.set_stable(stable.0);
    }
    let mut entries = out.reborrow().init_generations(length(generations.len())?);
    for (index, (node, generation)) in generations.iter().enumerate() {
        let mut value = entries.reborrow().get(length(index)?);
        value.set_node(node.0);
        value.set_generation(*generation);
    }
    let mut entries = out.reborrow().init_retired(length(retired.len())?);
    for (index, id) in retired.iter().enumerate() {
        entries.set(length(index)?, id.0);
    }
    ids(
        out.reborrow().init_free(length(free.len())?),
        free.iter().map(|id| id.0),
    )?;
    out.set_next_slot(next_slot);
    out.set_next_stable(next_stable);
    Ok(())
}
fn read_identity(
    value: w::identity_state::Reader<'_>,
    budget: &mut Budget,
) -> Result<NodeIdentity, Error> {
    let live = value.get_live()?;
    budget.charge(live.len() as usize)?;
    let live = unique_map(
        live.iter()
            .map(|entry| (NodeId(entry.get_node()), StableNodeId(entry.get_stable())))
            .collect(),
        |entry| entry.0,
    )?
    .into_iter()
    .map(|(node, (_, stable))| (node, stable))
    .collect();
    let generations = value.get_generations()?;
    budget.charge(generations.len() as usize)?;
    let generations = unique_map(
        generations
            .iter()
            .map(|entry| (NodeId(entry.get_node()), entry.get_generation()))
            .collect(),
        |entry| entry.0,
    )?
    .into_iter()
    .map(|(node, (_, generation))| (node, generation))
    .collect();
    let retired = value.get_retired()?;
    budget.charge(retired.len() as usize)?;
    NodeIdentity::from_snapshot(
        live,
        generations,
        unique_set(retired.iter().map(StableNodeId).collect())?,
        read_ids(value.get_free()?, budget, NodeId)?,
        value.get_next_slot(),
        value.get_next_stable(),
    )
}
fn write_path(mut out: w::owner_path::Builder<'_>, path: &OwnerPath) -> Result<(), Error> {
    out.set_root(path.root);
    let mut values = out.init_selectors(length(path.selectors.len())?);
    for (index, selector) in path.selectors.iter().enumerate() {
        let mut value = values.reborrow().get(length(index)?);
        match selector {
            PathSelector::Field(name) => value.set_field(name.as_str()),
            PathSelector::Index(index) => value.set_index(*index),
            PathSelector::DynamicIndex { symbol, predicate } => {
                let mut value = value.init_dynamic();
                value.set_symbol(symbol.as_str());
                value.set_predicate(predicate.as_str());
            }
            PathSelector::Slice { start, end, step } => {
                let mut value = value.init_slice();
                value.set_start(*start);
                value.set_end(*end);
                value.set_step(*step);
            }
        }
    }
    Ok(())
}
fn read_path(value: w::owner_path::Reader<'_>, budget: &mut Budget) -> Result<OwnerPath, Error> {
    let selectors = value.get_selectors()?;
    budget.charge(selectors.len() as usize)?;
    Ok(OwnerPath {
        root: value.get_root(),
        selectors: selectors
            .iter()
            .map(|value| {
                Ok(match value.which()? {
                    w::path_selector::Field(name) => {
                        PathSelector::Field(qsc_snapshot::string(name?)?)
                    }
                    w::path_selector::Index(index) => PathSelector::Index(index),
                    w::path_selector::Dynamic(value) => {
                        let value = value?;
                        PathSelector::DynamicIndex {
                            symbol: qsc_snapshot::string(value.get_symbol()?)?,
                            predicate: qsc_snapshot::string(value.get_predicate()?)?,
                        }
                    }
                    w::path_selector::Slice(value) => {
                        let value = value?;
                        PathSelector::Slice {
                            start: value.get_start(),
                            end: value.get_end(),
                            step: value.get_step(),
                        }
                    }
                    w::path_selector::Invalid(()) => {
                        return Err(Error::malformed("invalid path selector"));
                    }
                })
            })
            .collect::<Result<_, Error>>()?,
    })
}
fn write_cleanup(
    mut out: w::cleanup_state::Builder<'_>,
    state: &CleanupState,
) -> Result<(), Error> {
    match state {
        CleanupState::CleanZ => out.set_clean_z(()),
        CleanupState::LiveUnknown => out.set_live_unknown(()),
        CleanupState::Escaped => out.set_escaped(()),
        CleanupState::Released => out.set_released(()),
        CleanupState::Restored { delta } => out.set_restored(*delta),
        CleanupState::PendingRestore { delta, history } => {
            let mut value = out.init_pending_restore();
            value.set_delta(*delta);
            strings(value.init_history(length(history.len())?), history.iter())?;
        }
        CleanupState::PendingFragment {
            delta,
            fragment_count,
            history,
        } => {
            let mut value = out.init_pending_fragment();
            value.set_delta(*delta);
            value.set_fragment_count(u64::try_from(*fragment_count).map_err(Error::malformed)?);
            strings(value.init_history(length(history.len())?), history.iter())?;
        }
    }
    Ok(())
}
fn read_cleanup(
    value: w::cleanup_state::Reader<'_>,
    budget: &mut Budget,
) -> Result<CleanupState, Error> {
    Ok(match value.which()? {
        w::cleanup_state::CleanZ(()) => CleanupState::CleanZ,
        w::cleanup_state::LiveUnknown(()) => CleanupState::LiveUnknown,
        w::cleanup_state::Escaped(()) => CleanupState::Escaped,
        w::cleanup_state::Released(()) => CleanupState::Released,
        w::cleanup_state::Restored(delta) => CleanupState::Restored { delta },
        w::cleanup_state::PendingRestore(value) => {
            let value = value?;
            CleanupState::PendingRestore {
                delta: value.get_delta(),
                history: read_strings(value.get_history()?, budget)?,
            }
        }
        w::cleanup_state::PendingFragment(value) => {
            let value = value?;
            CleanupState::PendingFragment {
                delta: value.get_delta(),
                fragment_count: usize::try_from(value.get_fragment_count())
                    .map_err(Error::malformed)?,
                history: read_strings(value.get_history()?, budget)?,
            }
        }
        w::cleanup_state::Invalid(()) => return Err(Error::malformed("invalid cleanup state")),
    })
}
fn resources(
    mut out: capnp::primitive_list::Builder<'_, u64>,
    values: &BTreeSet<ResourceId>,
) -> Result<(), Error> {
    for (index, id) in values.iter().enumerate() {
        out.set(length(index)?, id.0);
    }
    Ok(())
}
fn read_resources(
    values: capnp::primitive_list::Reader<'_, u64>,
    budget: &mut Budget,
) -> Result<BTreeSet<ResourceId>, Error> {
    budget.charge(values.len() as usize)?;
    unique_set(values.iter().map(ResourceId).collect())
}
fn write_ownership(
    mut out: w::ownership_state::Builder<'_>,
    state: &OwnershipState,
) -> Result<(), Error> {
    out.set_next_borrow(state.snapshot_next_borrow());
    let mut values = out
        .reborrow()
        .init_authorities(length(state.authorities.len())?);
    for (index, authority) in state.authorities.iter().enumerate() {
        let mut value = values.reborrow().get(length(index)?);
        write_path(value.reborrow().init_path(), &authority.path)?;
        resources(
            value
                .reborrow()
                .init_resources(length(authority.resources.len())?),
            &authority.resources,
        )?;
        value.set_epoch(authority.epoch.0);
        write_cleanup(value.init_cleanup(), &authority.cleanup)?;
    }
    let mut values = out.init_borrows(length(state.borrows.len())?);
    for (index, borrow) in state.borrows.iter().enumerate() {
        let mut value = values.reborrow().get(length(index)?);
        value.set_id(borrow.id);
        value.set_kind(borrow_kind(borrow.kind));
        value.set_epoch(borrow.epoch.0);
        write_path(value.reborrow().init_path(), &borrow.path)?;
        resources(
            value.init_resources(length(borrow.resources.len())?),
            &borrow.resources,
        )?;
    }
    Ok(())
}
fn read_ownership(
    value: w::ownership_state::Reader<'_>,
    budget: &mut Budget,
) -> Result<OwnershipState, Error> {
    let authorities = value.get_authorities()?;
    budget.charge(authorities.len() as usize)?;
    let authorities = authorities
        .iter()
        .map(|value| {
            Ok(Authority {
                path: read_path(value.get_path()?, budget)?,
                resources: read_resources(value.get_resources()?, budget)?,
                epoch: Epoch(value.get_epoch()),
                cleanup: read_cleanup(value.get_cleanup()?, budget)?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let borrows = value.get_borrows()?;
    budget.charge(borrows.len() as usize)?;
    let borrows = borrows
        .iter()
        .map(|value| {
            Ok(Borrow {
                id: value.get_id(),
                kind: read_borrow_kind(value.get_kind()?)?,
                path: read_path(value.get_path()?, budget)?,
                resources: read_resources(value.get_resources()?, budget)?,
                epoch: Epoch(value.get_epoch()),
            })
        })
        .collect::<Result<_, Error>>()?;
    OwnershipState::from_snapshot(authorities, borrows, value.get_next_borrow())
}

pub(crate) fn write_match(
    mut out: w::region_fingerprint::Builder<'_>,
    graph: &Graph,
    adjacency: &Adjacency,
    matched: &BTreeSet<StableNodeId>,
) -> Result<(), Error> {
    let mut nodes = out.reborrow().init_nodes(length(matched.len())?);
    let mut incident = BTreeSet::new();
    for (index, id) in matched.iter().enumerate() {
        let mut entry = nodes.reborrow().get(length(index)?);
        entry.set_stable(id.0);
        if let Some(node) = graph.node_by_stable_id(*id) {
            let mut content = entry.init_present();
            content.set_operation(node.operation.0.as_str());
            write_effects(
                content.reborrow().init_effects(length(node.effects.len())?),
                &node.effects,
            )?;
            write_payload_value(content.reborrow().init_payload(), &node.payload)?;
            write_symbol(content.reborrow().init_symbol(), node.symbol.as_ref());
            let mut ports = content.init_ports(length(node.inputs.len() + node.outputs.len())?);
            for (index, id) in node.inputs.iter().chain(&node.outputs).enumerate() {
                let mut entry = ports.reborrow().get(length(index)?);
                entry.set_id(id.0);
                if let Some(port) = graph.port(*id) {
                    write_port(entry.init_present(), port)?;
                } else {
                    entry.set_absent(());
                }
            }
            incident.extend(adjacency.incoming(*id).iter().copied());
            incident.extend(adjacency.outgoing(*id).iter().copied());
        } else {
            entry.set_absent(());
        }
    }
    let mut edges = out.init_edges(length(incident.len())?);
    for (index, edge) in incident.iter().enumerate() {
        write_edge(edges.reborrow().get(length(index)?), edge);
    }
    Ok(())
}
