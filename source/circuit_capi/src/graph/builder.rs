// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_qhdg::{
    Edge, EffectEvent, EffectResource, GraphBuilder, InterfaceBinding, NodeId, NodePayload,
    NodeValue, PortId, Real, RegionId, SymbolId,
};

use crate::{
    QDK_INVALID_STATE, QdkDiagnostic, QdkStatus,
    boundary::{Error, boundary, input_slice, output, release, required},
    handle::{Access, Handle},
};

use super::{
    QdkGraph,
    tags::{DIRECTIONS, DOMAINS, EDGES, MODES, PORTS, REGIONS, ROLES, STAGES, USAGES, decode},
    value::{QdkGraphAngle, QdkGraphOrigin, QdkGraphType, QdkString},
};

pub const QDK_PAYLOAD_NONE: u32 = 0;
pub const QDK_PAYLOAD_NUMBER: u32 = 1;
pub const QDK_PAYLOAD_CLASSICAL_BIT: u32 = 2;
pub const QDK_PAYLOAD_ANGLE: u32 = 3;

/// Owned single-use native graph builder. Free may discard unfinished work.
pub struct QdkGraphBuilder {
    inner: Handle<Option<GraphBuilder>>,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkOptionalId {
    /// Zero for absent, one for present (including ID zero).
    pub present: u32,
    pub value: u32,
}

impl QdkOptionalId {
    pub(super) fn read(self) -> Result<Option<u32>, Error> {
        Ok(boolean(self.present)?.then_some(self.value))
    }

    pub(super) fn of(value: Option<u32>) -> Self {
        Self {
            present: u32::from(value.is_some()),
            value: value.unwrap_or_default(),
        }
    }
}

#[repr(C)]
pub struct QdkGraphEffectInput {
    pub domain: u32,
    pub mode: u32,
    pub observable: u32,
    /// Zero for unknown, one for a named resource (possibly empty).
    pub has_resource: u32,
    pub resource: QdkString,
}

#[repr(C)]
pub struct QdkGraphPortInput {
    pub node: u32,
    pub direction: u32,
    pub kind: u32,
    pub value_type: *const QdkGraphType,
    pub stage: u32,
    pub usage: u32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkGraphEdge {
    pub source: u32,
    pub target: u32,
    pub kind: u32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct QdkGraphBinding {
    pub outer: u32,
    pub inner: u32,
    pub role: u32,
}

#[repr(C)]
pub struct QdkGraphPayload {
    pub kind: u32,
    pub number: f64,
    pub classical_bit: u32,
    /// Only read for `QDK_PAYLOAD_ANGLE`; the angle remains caller-owned.
    pub angle: *const QdkGraphAngle,
    pub origin: QdkGraphOrigin,
}

pub(super) fn boolean(value: u32) -> Result<bool, Error> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Error::invalid("boolean flag must be zero or one")),
    }
}

fn active<'a>(
    value: &'a mut Access<'_, Option<GraphBuilder>>,
) -> Result<&'a mut GraphBuilder, Error> {
    value
        .as_mut()
        .ok_or_else(|| Error::new(QDK_INVALID_STATE, "graph builder has already finished"))
}

impl From<qsc_qhdg::BuildError> for Error {
    fn from(error: qsc_qhdg::BuildError) -> Self {
        Self::invalid(error.to_string())
    }
}

/// Creates an owning single-use graph builder.
/// # Safety
/// Writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_new(
    out: *mut *mut QdkGraphBuilder,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides disjoint writable slots.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            *out = Box::into_raw(Box::new(QdkGraphBuilder {
                inner: Handle::new(Some(GraphBuilder::new())),
            }));
            Ok(())
        })
    }
}

/// Adds a region with an optional parent. Whole-graph validity is checked by
/// `qdk_graph_verify`, not by finish.
/// # Safety
/// Builder is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_region(
    builder: *mut QdkGraphBuilder,
    kind: u32,
    parent: QdkOptionalId,
    out: *mut u32,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides a live handle and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let mut value = required(builder, "graph builder")?.inner.access()?;
            let kind = decode(kind, REGIONS)?;
            let parent = parent.read()?.map(RegionId);
            *out = active(&mut value)?.add_region(parent, kind).0;
            Ok(())
        })
    }
}

/// Adds a node, copying its operation name and effects.
/// # Safety
/// Inputs cover their declared extents; builder is live; outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_node(
    builder: *mut QdkGraphBuilder,
    region: u32,
    operation: QdkString,
    effects: *const QdkGraphEffectInput,
    effect_count: usize,
    out: *mut u32,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies input extents and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let mut value = required(builder, "graph builder")?.inner.access()?;
            let operation = operation.read()?;
            let effects = input_slice(effects, effect_count)?
                .iter()
                .map(|effect| {
                    Ok(EffectEvent {
                        domain: decode(effect.domain, DOMAINS)?,
                        mode: decode(effect.mode, MODES)?,
                        observable: boolean(effect.observable)?,
                        resource: if boolean(effect.has_resource)? {
                            EffectResource::Named(effect.resource.read()?.to_owned())
                        } else {
                            EffectResource::Unknown
                        },
                    })
                })
                .collect::<Result<_, Error>>()?;
            *out = active(&mut value)?
                .add_node(RegionId(region), operation, effects)
                .map_err(Error::from)?
                .0;
            Ok(())
        })
    }
}

/// Adds a typed port, retaining no input handle.
/// # Safety
/// Builder, descriptor and type handle are live; outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_port(
    builder: *mut QdkGraphBuilder,
    input: *const QdkGraphPortInput,
    out: *mut u32,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies live inputs and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let mut value = required(builder, "graph builder")?.inner.access()?;
            let input = required(input, "port input")?;
            let ty = required(input.value_type, "value type")?
                .inner
                .access()?
                .clone();
            *out = active(&mut value)?
                .add_port(
                    NodeId(input.node),
                    decode(input.direction, DIRECTIONS)?,
                    decode(input.kind, PORTS)?,
                    ty,
                    decode(input.stage, STAGES)?,
                    decode(input.usage, USAGES)?,
                )
                .map_err(Error::from)?
                .0;
            Ok(())
        })
    }
}

/// Connects ports with the native builder's local edge checks.
/// # Safety
/// Builder is live; optional diagnostic output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_connect(
    builder: *mut QdkGraphBuilder,
    edge: QdkGraphEdge,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live handle and disjoint error slot.
    unsafe {
        boundary(error, None, || {
            let mut value = required(builder, "graph builder")?.inner.access()?;
            active(&mut value)?
                .connect(Edge {
                    source: PortId(edge.source),
                    target: PortId(edge.target),
                    kind: decode(edge.kind, EDGES)?,
                })
                .map_err(Error::from)
        })
    }
}

/// Copies a node's source symbol into the graph under construction.
/// # Safety
/// Builder and input byte extents are live; error output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_symbol(
    builder: *mut QdkGraphBuilder,
    node: u32,
    package: QdkString,
    item: QdkString,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies valid inputs and disjoint output.
    unsafe {
        boundary(error, None, || {
            let mut value = required(builder, "graph builder")?.inner.access()?;
            active(&mut value)?
                .declare_symbol(
                    NodeId(node),
                    SymbolId {
                        package: package.read()?.to_owned(),
                        item: item.read()?.to_owned(),
                    },
                )
                .map_err(Error::from)
        })
    }
}

/// Declares one typed value and an independent optional origin.
/// # Safety
/// Builder, descriptor and selected angle are live; error output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_payload(
    builder: *mut QdkGraphBuilder,
    node: u32,
    payload: *const QdkGraphPayload,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides live input handles and descriptor.
    unsafe {
        boundary(error, None, || {
            let mut builder = required(builder, "graph builder")?.inner.access()?;
            let payload = required(payload, "payload")?;
            let value = match payload.kind {
                QDK_PAYLOAD_NONE => None,
                QDK_PAYLOAD_NUMBER => Some(NodeValue::Number(Real::new(payload.number))),
                QDK_PAYLOAD_CLASSICAL_BIT => Some(NodeValue::ClassicalBit(payload.classical_bit)),
                QDK_PAYLOAD_ANGLE => Some(NodeValue::Angle(
                    required(payload.angle, "angle")?.inner.access()?.clone(),
                )),
                _ => return Err(Error::invalid("unknown graph payload kind")),
            };
            active(&mut builder)?
                .declare_payload(
                    NodeId(node),
                    NodePayload {
                        value,
                        origin: payload.origin.native()?,
                    },
                )
                .map_err(Error::from)
        })
    }
}

/// Binds a child port across a parent boundary using native local checks.
/// # Safety
/// Builder is live; optional error output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_bind(
    builder: *mut QdkGraphBuilder,
    region: u32,
    binding: QdkGraphBinding,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live builder and disjoint output.
    unsafe {
        boundary(error, None, || {
            let mut value = required(builder, "graph builder")?.inner.access()?;
            active(&mut value)?
                .bind_interface(
                    RegionId(region),
                    InterfaceBinding {
                        outer: PortId(binding.outer),
                        inner: PortId(binding.inner),
                        role: decode(binding.role, ROLES)?,
                    },
                )
                .map_err(Error::from)
        })
    }
}

/// Declares the node that owns a region.
/// # Safety
/// Builder is live; optional error output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_region_owner(
    builder: *mut QdkGraphBuilder,
    region: u32,
    node: u32,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live builder and disjoint output.
    unsafe {
        boundary(error, None, || {
            let mut value = required(builder, "graph builder")?.inner.access()?;
            active(&mut value)?
                .declare_region_owner(RegionId(region), NodeId(node))
                .map_err(Error::from)
        })
    }
}

/// Marks a node demanded, using the existing native demand semantics.
/// # Safety
/// Builder is live; optional error output is disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_demand(
    builder: *mut QdkGraphBuilder,
    node: u32,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live builder and disjoint output.
    unsafe {
        boundary(error, None, || {
            let mut value = required(builder, "graph builder")?.inner.access()?;
            active(&mut value)?.demand(NodeId(node));
            Ok(())
        })
    }
}

/// Produces a graph and spends the builder. Success does not imply whole-graph
/// verification; use `qdk_graph_verify`. Free the spent builder separately.
/// # Safety
/// Builder is live; writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_finish(
    builder: *mut QdkGraphBuilder,
    out: *mut *mut QdkGraph,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live builder and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let mut value = required(builder, "graph builder")?.inner.access()?;
            let builder = value.take().ok_or_else(|| {
                Error::new(QDK_INVALID_STATE, "graph builder has already finished")
            })?;
            *out = QdkGraph::owned(builder.finish());
            Ok(())
        })
    }
}

/// Releases a graph builder, discarding any unreturned graph.
/// # Safety
/// Slot contains NULL or a live exclusively owned builder.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_builder_free(
    value: *mut *mut QdkGraphBuilder,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns the slot and live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}
