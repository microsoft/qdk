// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_qhdg::{
    EffectResource, ExitKind, NodeValue, Region,
    graph::{Node, Port},
};

use crate::{
    QDK_WRONG_VARIANT, QdkBytes, QdkDiagnostic, QdkStatus,
    boundary::{Error, boundary, copy_out, output, release, required},
    handle::Handle,
};

use super::{
    QdkGraph,
    builder::{
        QDK_PAYLOAD_ANGLE, QDK_PAYLOAD_CLASSICAL_BIT, QDK_PAYLOAD_NONE, QDK_PAYLOAD_NUMBER,
        QdkGraphBinding, QdkOptionalId,
    },
    tags::{DIRECTIONS, DOMAINS, MODES, PORTS, REGIONS, ROLES, STAGES, USAGES, encode},
    value::{QdkGraphAngle, QdkGraphOrigin, QdkGraphType},
};

pub const QDK_GRAPH_REGION: u32 = 1;
pub const QDK_GRAPH_NODE: u32 = 2;
pub const QDK_GRAPH_PORT: u32 = 3;
pub const QDK_GRAPH_OPERATION: u32 = 1;
pub const QDK_GRAPH_SYMBOL_PACKAGE: u32 = 2;
pub const QDK_GRAPH_SYMBOL_ITEM: u32 = 3;
pub const QDK_GRAPH_EFFECT_RESOURCE: u32 = 4;
pub const QDK_GRAPH_INPUTS: u32 = 1;
pub const QDK_GRAPH_OUTPUTS: u32 = 2;
pub const QDK_GRAPH_ARGUMENTS: u32 = 3;
pub const QDK_GRAPH_RESULTS: u32 = 4;
pub const QDK_EXIT_RETURN: u32 = 1;
pub const QDK_EXIT_ABORT: u32 = 2;
pub const QDK_EXIT_BRANCH: u32 = 3;
pub const QDK_EXIT_BACKEDGE: u32 = 4;

enum Record {
    Region(Region),
    Node(Node),
    Port(Port),
}

/// Independent copied record. IDs remain local to the source graph.
pub struct QdkGraphRecord {
    inner: Handle<Record>,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkGraphRegionInfo {
    pub id: u32,
    pub parent: QdkOptionalId,
    pub kind: u32,
    pub owner: QdkOptionalId,
    pub argument_count: usize,
    pub result_count: usize,
    pub exit_count: usize,
    pub binding_count: usize,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkGraphNodeInfo {
    pub id: u32,
    pub region: u32,
    pub input_count: usize,
    pub output_count: usize,
    pub effect_count: usize,
    pub has_symbol: u32,
    pub payload_kind: u32,
    pub number: f64,
    pub classical_bit: u32,
    pub origin: QdkGraphOrigin,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkGraphPortInfo {
    pub id: u32,
    pub node: u32,
    pub region: u32,
    pub direction: u32,
    pub kind: u32,
    pub stage: u32,
    pub usage: u32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkGraphEffectInfo {
    pub domain: u32,
    pub mode: u32,
    pub observable: u32,
    pub has_resource: u32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkGraphExit {
    pub kind: u32,
    pub port: u32,
}

fn wrong_record() -> Error {
    Error::new(
        QDK_WRONG_VARIANT,
        "query does not match the copied graph record kind",
    )
}

/// Copies the region/node/port at an enumeration position (not an ID).
/// Enumeration is in ascending local ID order; records survive graph release.
/// # Safety
/// Graph is live; writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_record(
    graph: *const QdkGraph,
    kind: u32,
    index: usize,
    out: *mut *mut QdkGraphRecord,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live handle and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let graph = required(graph, "graph")?.inner.access()?;
            let record = match kind {
                QDK_GRAPH_REGION => graph.regions().nth(index).cloned().map(Record::Region),
                QDK_GRAPH_NODE => graph.nodes().nth(index).cloned().map(Record::Node),
                QDK_GRAPH_PORT => graph.ports().nth(index).cloned().map(Record::Port),
                _ => return Err(Error::invalid("unknown graph record kind")),
            }
            .ok_or_else(|| Error::invalid("graph record index is out of range"))?;
            *out = Box::into_raw(Box::new(QdkGraphRecord {
                inner: Handle::new(record),
            }));
            Ok(())
        })
    }
}

/// Copies region fields and interface sizes.
/// # Safety
/// Record is live; writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_region_info(
    record: *const QdkGraphRecord,
    out: *mut QdkGraphRegionInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live record and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let record = required(record, "record")?.inner.access()?;
            let Record::Region(region) = &*record else {
                return Err(wrong_record());
            };
            *out = QdkGraphRegionInfo {
                id: region.id.0,
                parent: QdkOptionalId::of(region.parent.map(|id| id.0)),
                kind: encode(region.kind, REGIONS)?,
                owner: QdkOptionalId::of(region.owner.map(|id| id.0)),
                argument_count: region.interface.arguments.len(),
                result_count: region.interface.results.len(),
                exit_count: region.interface.exits.len(),
                binding_count: region.interface.bindings.len(),
            };
            Ok(())
        })
    }
}

/// Copies node metadata, typed payload and origin.
/// # Safety
/// Record is live; writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_node_info(
    record: *const QdkGraphRecord,
    out: *mut QdkGraphNodeInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live record and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let record = required(record, "record")?.inner.access()?;
            let Record::Node(node) = &*record else {
                return Err(wrong_record());
            };
            let mut info = QdkGraphNodeInfo {
                id: node.id.0,
                region: node.region.0,
                input_count: node.inputs.len(),
                output_count: node.outputs.len(),
                effect_count: node.effects.len(),
                has_symbol: u32::from(node.symbol.is_some()),
                origin: QdkGraphOrigin::of(node.payload.origin)?,
                ..Default::default()
            };
            match &node.payload.value {
                None => info.payload_kind = QDK_PAYLOAD_NONE,
                Some(NodeValue::Number(number)) => {
                    info.payload_kind = QDK_PAYLOAD_NUMBER;
                    info.number = number.get();
                }
                Some(NodeValue::ClassicalBit(bit)) => {
                    info.payload_kind = QDK_PAYLOAD_CLASSICAL_BIT;
                    info.classical_bit = *bit;
                }
                Some(NodeValue::Angle(_)) => info.payload_kind = QDK_PAYLOAD_ANGLE,
            }
            *out = info;
            Ok(())
        })
    }
}

/// Copies port metadata. Retrieve its independent type with `qdk_graph_record_type`.
/// # Safety
/// Record is live; writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_port_info(
    record: *const QdkGraphRecord,
    out: *mut QdkGraphPortInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live record and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let record = required(record, "record")?.inner.access()?;
            let Record::Port(port) = &*record else {
                return Err(wrong_record());
            };
            *out = QdkGraphPortInfo {
                id: port.id.0,
                node: port.node.0,
                region: port.region.0,
                direction: encode(port.direction, DIRECTIONS)?,
                kind: encode(port.kind, PORTS)?,
                stage: encode(port.stage, STAGES)?,
                usage: encode(port.usage, USAGES)?,
            };
            Ok(())
        })
    }
}

/// Copies a node operation, symbol package/item, or named effect resource.
/// Index is used only for the effect resource. Absent fields return `WrongVariant`.
/// # Safety
/// Record is live; writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_record_text(
    record: *const QdkGraphRecord,
    field: u32,
    index: usize,
    out: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live record and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let record = required(record, "record")?.inner.access()?;
            let Record::Node(node) = &*record else {
                return Err(wrong_record());
            };
            let text = match field {
                QDK_GRAPH_OPERATION => &node.operation.0,
                QDK_GRAPH_SYMBOL_PACKAGE => &node.symbol.as_ref().ok_or_else(wrong_record)?.package,
                QDK_GRAPH_SYMBOL_ITEM => &node.symbol.as_ref().ok_or_else(wrong_record)?.item,
                QDK_GRAPH_EFFECT_RESOURCE => {
                    let effect = node
                        .effects
                        .get(index)
                        .ok_or_else(|| Error::invalid("effect index is out of range"))?;
                    let EffectResource::Named(name) = &effect.resource else {
                        return Err(wrong_record());
                    };
                    name
                }
                _ => return Err(Error::invalid("unknown graph text field")),
            };
            *out = text.as_bytes().to_vec().into();
            Ok(())
        })
    }
}

/// Copies node inputs/outputs or region arguments/results in native order.
/// # Safety
/// Record is live; output buffers cover capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_record_ports(
    record: *const QdkGraphRecord,
    field: u32,
    data: *mut u32,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live input and disjoint writable extents.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            let record = required(record, "record")?.inner.access()?;
            let ports = match (&*record, field) {
                (Record::Node(node), QDK_GRAPH_INPUTS) => &node.inputs,
                (Record::Node(node), QDK_GRAPH_OUTPUTS) => &node.outputs,
                (Record::Region(region), QDK_GRAPH_ARGUMENTS) => &region.interface.arguments,
                (Record::Region(region), QDK_GRAPH_RESULTS) => &region.interface.results,
                _ => return Err(wrong_record()),
            };
            copy_out(
                &ports.iter().map(|id| id.0).collect::<Vec<_>>(),
                data,
                capacity,
                needed,
            )
        })
    }
}

/// Copies a node's effect metadata. Named resources use `qdk_graph_record_text`.
/// # Safety
/// Record is live; output buffers cover capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_record_effects(
    record: *const QdkGraphRecord,
    data: *mut QdkGraphEffectInfo,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live input and disjoint writable extents.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            let record = required(record, "record")?.inner.access()?;
            let Record::Node(node) = &*record else {
                return Err(wrong_record());
            };
            let effects = node
                .effects
                .iter()
                .map(|effect| {
                    Ok(QdkGraphEffectInfo {
                        domain: encode(effect.domain, DOMAINS)?,
                        mode: encode(effect.mode, MODES)?,
                        observable: u32::from(effect.observable),
                        has_resource: u32::from(matches!(
                            effect.resource,
                            EffectResource::Named(_)
                        )),
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            copy_out(&effects, data, capacity, needed)
        })
    }
}

/// Copies a region's interface bindings.
/// # Safety
/// Record is live; output buffers cover capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_record_bindings(
    record: *const QdkGraphRecord,
    data: *mut QdkGraphBinding,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live input and disjoint writable extents.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            let record = required(record, "record")?.inner.access()?;
            let Record::Region(region) = &*record else {
                return Err(wrong_record());
            };
            let bindings = region
                .interface
                .bindings
                .iter()
                .map(|binding| {
                    Ok(QdkGraphBinding {
                        outer: binding.outer.0,
                        inner: binding.inner.0,
                        role: encode(binding.role, ROLES)?,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            copy_out(&bindings, data, capacity, needed)
        })
    }
}

/// Copies a region's typed control exits.
/// # Safety
/// Record is live; output buffers cover capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_record_exits(
    record: *const QdkGraphRecord,
    data: *mut QdkGraphExit,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live input and disjoint writable extents.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            let record = required(record, "record")?.inner.access()?;
            let Record::Region(region) = &*record else {
                return Err(wrong_record());
            };
            let exits = region
                .interface
                .exits
                .iter()
                .map(|(kind, port)| QdkGraphExit {
                    kind: match kind {
                        ExitKind::Return => QDK_EXIT_RETURN,
                        ExitKind::Abort => QDK_EXIT_ABORT,
                        ExitKind::Branch => QDK_EXIT_BRANCH,
                        ExitKind::Backedge => QDK_EXIT_BACKEDGE,
                    },
                    port: port.0,
                })
                .collect::<Vec<_>>();
            copy_out(&exits, data, capacity, needed)
        })
    }
}

/// Copies a port's type into an independent handle.
/// # Safety
/// Record is live; writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_record_type(
    record: *const QdkGraphRecord,
    out: *mut *mut QdkGraphType,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live record and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let record = required(record, "record")?.inner.access()?;
            let Record::Port(port) = &*record else {
                return Err(wrong_record());
            };
            *out = Box::into_raw(Box::new(QdkGraphType {
                inner: Handle::new(port.ty.clone()),
            }));
            Ok(())
        })
    }
}

/// Copies a symbolic node payload into an independent named angle.
/// # Safety
/// Record is live; writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_record_angle(
    record: *const QdkGraphRecord,
    out: *mut *mut QdkGraphAngle,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live record and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let record = required(record, "record")?.inner.access()?;
            let Record::Node(node) = &*record else {
                return Err(wrong_record());
            };
            let Some(NodeValue::Angle(angle)) = &node.payload.value else {
                return Err(wrong_record());
            };
            *out = Box::into_raw(Box::new(QdkGraphAngle {
                inner: Handle::new(angle.clone()),
            }));
            Ok(())
        })
    }
}

/// Releases an independent graph record and clears its slot.
/// # Safety
/// Slot contains NULL or a live exclusively owned record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_record_free(
    value: *mut *mut QdkGraphRecord,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and its live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}
