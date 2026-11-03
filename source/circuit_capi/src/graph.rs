// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod builder;
mod record;
mod tags;
mod value;

pub use builder::*;
pub use record::*;
pub use tags::*;
pub use value::*;

use std::collections::BTreeMap;

use qdk_circuit_api::Circuit;
use qsc_codegen::circuit_qhdg::graph_to_circuit_with_origins;
use qsc_qhdg::{
    Diagnostic, Graph, canonical_graph_snapshot, decode_graph_snapshot, graph_snapshot_text,
};

use crate::{
    QDK_FORMAT_QASM, QDK_FORMAT_QIR, QdkBytes, QdkCircuit, QdkDiagnostic, QdkStatus,
    boundary::{Error, boundary, copy_out, input_slice, output, release, required},
    handle::Handle,
};

pub const QDK_FORMAT_GRAPH_BINARY: u32 = 8;
pub const QDK_FORMAT_GRAPH_SEXPR: u32 = 10;

/// Owning read-only graph. Transformations return independent objects.
pub struct QdkGraph {
    pub(crate) inner: Handle<Graph>,
}

impl QdkGraph {
    pub(crate) fn owned(graph: Graph) -> *mut Self {
        Box::into_raw(Box::new(Self {
            inner: Handle::new(graph),
        }))
    }
}

#[derive(Default)]
#[repr(C)]
pub struct QdkGraphInfo {
    pub region_count: usize,
    pub node_count: usize,
    pub port_count: usize,
    pub edge_count: usize,
    pub revision: u64,
}

#[repr(C)]
pub struct QdkGraphParameterBinding {
    pub name: QdkString,
    pub value: f64,
}

/// Independent copied verifier results.
pub struct QdkGraphDiagnostics {
    inner: Handle<Vec<Diagnostic>>,
}

/// Independent positional origins returned by graph raising.
pub struct QdkGraphOrigins {
    inner: Handle<Vec<QdkGraphOrigin>>,
}

/// Lowers a closed circuit using the existing supported graph crossing.
/// # Safety
/// Circuit is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_circuit_to_graph(
    circuit: *const QdkCircuit,
    out: *mut *mut QdkGraph,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies live inputs and disjoint writable outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let circuit = required(circuit, "circuit")?.inner.access()?;
            *out = QdkGraph::owned(circuit.to_graph().map_err(Error::from)?);
            Ok(())
        })
    }
}

/// Raises a graph and returns positional origins. Registers are not recovered.
/// Both owning outputs are required and remain NULL on failure.
/// # Safety
/// Graph is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_to_circuit(
    graph: *const QdkGraph,
    out: *mut *mut QdkCircuit,
    origins: *mut *mut QdkGraphOrigins,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live graph and disjoint writable outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let origins = output(origins)?;
            let graph = required(graph, "graph")?.inner.access()?;
            let raised = graph_to_circuit_with_origins(&graph)
                .map_err(|error| Error::invalid(error.to_string()))?;
            let copied = raised
                .origins
                .into_iter()
                .map(|origin| QdkGraphOrigin::of(origin.map(Into::into)))
                .collect::<Result<_, _>>()?;
            let circuit = Box::new(QdkCircuit {
                inner: Handle::new(Circuit::of(raised.circuit)),
            });
            let provenance = Box::new(QdkGraphOrigins {
                inner: Handle::new(copied),
            });
            *out = Box::into_raw(circuit);
            *origins = Box::into_raw(provenance);
            Ok(())
        })
    }
}

/// Clones a graph without borrowing or changing its source.
/// # Safety
/// Graph is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_clone(
    graph: *const QdkGraph,
    out: *mut *mut QdkGraph,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live graph and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let graph = required(graph, "graph")?.inner.access()?;
            *out = QdkGraph::owned(graph.clone());
            Ok(())
        })
    }
}

/// Copies graph dimensions and revision.
/// # Safety
/// Graph is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_info(
    graph: *const QdkGraph,
    out: *mut QdkGraphInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live graph and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let graph = required(graph, "graph")?.inner.access()?;
            *out = QdkGraphInfo {
                region_count: graph.regions().count(),
                node_count: graph.nodes().count(),
                port_count: graph.ports().count(),
                edge_count: graph.edges().len(),
                revision: graph.revision(),
            };
            Ok(())
        })
    }
}

/// Partially rebinds named graph parameters into a new graph, retaining
/// untouched symbols and node/port/edge identity. Last duplicate name wins.
/// # Safety
/// Graph and binding input extents are live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_bind(
    graph: *const QdkGraph,
    bindings: *const QdkGraphParameterBinding,
    count: usize,
    out: *mut *mut QdkGraph,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies live inputs and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let source = required(graph, "graph")?.inner.access()?;
            let bindings = input_slice(bindings, count)?
                .iter()
                .map(|binding| Ok((binding.name.read()?.to_owned(), binding.value)))
                .collect::<Result<BTreeMap<_, _>, Error>>()?;
            let mut graph = source.clone();
            graph
                .rebind_parameters(&bindings)
                .map_err(|error| Error::invalid(error.to_string()))?;
            *out = QdkGraph::owned(graph);
            Ok(())
        })
    }
}

/// Exports the native safe snapshot, direct graph QIR, or QASM via circuit
/// raising. Only QIR reads `profile_tag`. No new graph vocabulary is admitted.
/// # Safety
/// Graph is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_export(
    graph: *const QdkGraph,
    format: u32,
    profile_tag: u32,
    out: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live graph and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let graph = required(graph, "graph")?.inner.access()?;
            let bytes = match format {
                QDK_FORMAT_GRAPH_BINARY => canonical_graph_snapshot(&graph)
                    .map_err(|error| Error::invalid(error.to_string()))?,
                QDK_FORMAT_GRAPH_SEXPR => graph_snapshot_text(&graph)
                    .map_err(|error| Error::invalid(error.to_string()))?
                    .into_bytes(),
                QDK_FORMAT_QIR => {
                    qsc_codegen::graph::graph_to_qir(&graph, crate::output::profile(profile_tag)?)
                        .map_err(|error| Error::invalid(error.to_string()))?
                        .into_bytes()
                }
                QDK_FORMAT_QASM => {
                    let circuit = qsc_codegen::circuit_qhdg::graph_to_circuit(&graph)
                        .map_err(|error| Error::invalid(error.to_string()))?;
                    qsc_codegen::circuit_qasm::circuit_to_qasm(&circuit)
                        .map_err(|error| Error::invalid(error.to_string()))?
                        .into_bytes()
                }
                _ => {
                    return Err(Error::invalid(
                        "graph export requires a graph snapshot, QIR or QASM",
                    ));
                }
            };
            *out = bytes.into();
            Ok(())
        })
    }
}

/// Imports a native graph snapshot with its existing limits, digest, schema
/// and verification checks. Other program formats are not accepted here.
/// # Safety
/// Input covers len initialized bytes; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_import(
    data: *const u8,
    len: usize,
    out: *mut *mut QdkGraph,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies the input extent and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let graph = decode_graph_snapshot(input_slice(data, len)?)
                .map_err(|error| Error::invalid(error.to_string()))?;
            *out = QdkGraph::owned(graph);
            Ok(())
        })
    }
}

/// Copies graph edges without partial writes on insufficient capacity.
/// # Safety
/// Graph is live; output buffers cover capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_edges(
    graph: *const QdkGraph,
    data: *mut QdkGraphEdge,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies live graph and disjoint output extents.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            let graph = required(graph, "graph")?.inner.access()?;
            let edges = graph
                .edges()
                .iter()
                .map(|edge| {
                    Ok(QdkGraphEdge {
                        source: edge.source.0,
                        target: edge.target.0,
                        kind: tags::encode(edge.kind, tags::EDGES)?,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            copy_out(&edges, data, capacity, needed)
        })
    }
}

/// Runs whole-graph verification, returning an independent diagnostic list.
/// A nonempty list is a successful query, not a verified graph.
/// # Safety
/// Graph is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_verify(
    graph: *const QdkGraph,
    out: *mut *mut QdkGraphDiagnostics,
    count: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live graph and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let count = output(count)?;
            let graph = required(graph, "graph")?.inner.access()?;
            let diagnostics = qsc_qhdg::verify(&graph);
            let length = diagnostics.len();
            *out = Box::into_raw(Box::new(QdkGraphDiagnostics {
                inner: Handle::new(diagnostics),
            }));
            *count = length;
            Ok(())
        })
    }
}

/// Copies one verifier diagnostic's rule code, message and optional node ID.
/// # Safety
/// Diagnostic list is live; all writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_diagnostic(
    diagnostics: *const QdkGraphDiagnostics,
    index: usize,
    code: *mut QdkBytes,
    message: *mut QdkBytes,
    node: *mut QdkOptionalId,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies live input and disjoint writable slots.
    unsafe {
        boundary(error, None, || {
            let code = output(code)?;
            let message = output(message)?;
            let node = output(node)?;
            let diagnostics = required(diagnostics, "diagnostics")?.inner.access()?;
            let diagnostic = diagnostics
                .get(index)
                .ok_or_else(|| Error::invalid("diagnostic index is out of range"))?;
            let code_bytes = diagnostic.code.as_str().as_bytes().to_vec();
            let message_bytes = diagnostic.message.as_bytes().to_vec();
            *code = code_bytes.into();
            *message = message_bytes.into();
            *node = QdkOptionalId::of(diagnostic.node.map(|node| node.0));
            Ok(())
        })
    }
}

/// Copies positional raised-circuit origins. NONE denotes an unattributed
/// instruction. Insufficient capacity performs no partial writes.
/// # Safety
/// Origins are live; output buffers cover capacity and are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_origins_copy(
    origins: *const QdkGraphOrigins,
    data: *mut QdkGraphOrigin,
    capacity: usize,
    needed: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies live origins and disjoint output extents.
    unsafe {
        boundary(error, None, || {
            *output(needed)? = 0;
            let origins = required(origins, "origins")?.inner.access()?;
            copy_out(&origins, data, capacity, needed)
        })
    }
}

/// Releases a graph and clears its owning slot.
/// # Safety
/// Slot contains NULL or a live exclusively owned graph.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_free(
    value: *mut *mut QdkGraph,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and its live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}

/// Releases a copied diagnostic list and clears its slot.
/// # Safety
/// Slot contains NULL or a live exclusively owned diagnostic list.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_diagnostics_free(
    value: *mut *mut QdkGraphDiagnostics,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and its live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}

/// Releases copied origins and clears their slot.
/// # Safety
/// Slot contains NULL or live exclusively owned origins.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_graph_origins_free(
    value: *mut *mut QdkGraphOrigins,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and its live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}
