// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    ptr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use qdk_circuit_api::Circuit;
use qsc_circuit_store::CircuitStore;
use qsc_codegen::{
    graph_pass::PassOutcome,
    pass_anchor::PassAnchor,
    pass_pipeline::{
        CircuitPass, GraphPass, OptimizationLevel, PassClaim, PassContext, PassEntry, PassOutput,
        PassPipeline, PipelineRun,
    },
};
use qsc_qhdg::Graph;

use crate::{
    QDK_ALREADY_SET, QDK_CALLBACK_ERROR, QdkBytes, QdkCircuit, QdkContext, QdkContextDestructor,
    QdkDiagnostic, QdkGraph, QdkGraphCallback, QdkGraphPassOptions, QdkStatus, QdkString,
    boundary::{Error, boundary, output, release, required, slot},
    handle::Handle,
    pass::{ForeignContext, ForeignGraphPass, callback_status},
};

pub const QDK_ANCHOR_CIRCUIT_BUILT: u32 = 1;
pub const QDK_ANCHOR_GRAPH_LOWERED: u32 = 2;
pub const QDK_ANCHOR_GRAPH_REWRITTEN: u32 = 3;
pub const QDK_ANCHOR_CIRCUIT_RAISED: u32 = 4;
pub const QDK_OPTIMIZATION_NONE: u32 = 0;
pub const QDK_OPTIMIZATION_DEFAULT: u32 = 1;
pub const QDK_OPTIMIZATION_AGGRESSIVE: u32 = 2;

/// Callback-duration borrowed scope. Only `qdk_callback_offer` operates on it.
pub struct QdkCircuitCallbackScope {
    inner: Handle<Option<Box<QdkCircuit>>>,
}

/// The working circuit is borrowed for this call. Its dependent handles are
/// callback-scoped and disposed child-first on return. Independent closed
/// clones may escape. No foreign panic/exception may cross this ABI.
pub type QdkCircuitCallback = Option<
    unsafe extern "C" fn(
        context: QdkContext,
        scope: *mut QdkCircuitCallbackScope,
        working: *mut QdkCircuit,
        error: *mut *mut QdkDiagnostic,
    ) -> QdkStatus,
>;

/// Owning native pass pipeline. Registration and execution are non-reentrant.
pub struct QdkPipeline {
    inner: Handle<Pipeline>,
}

struct Pipeline {
    native: PassPipeline,
    shared: Arc<Shared>,
}

/// Independent native pipeline output and report.
pub struct QdkPipelineResult {
    inner: Handle<Run>,
}

struct Run {
    native: PipelineRun,
    crossings: usize,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkPassEntryInfo {
    pub anchor: u32,
    pub built_in: u32,
    /// Owned UTF-8 bytes.
    pub name: QdkBytes,
}

#[derive(Default)]
#[repr(C)]
pub struct QdkPipelineResultInfo {
    pub executed_count: usize,
    pub changed_count: usize,
    pub diagnostic_count: usize,
    pub analyses_computed: usize,
    /// Counts circuit callbacks and per-match graph callbacks.
    pub callback_count: usize,
}

#[derive(Default)]
struct Shared {
    failure: Mutex<Option<Error>>,
    crossings: AtomicUsize,
}

impl Shared {
    fn fail(&self, error: Error) -> String {
        let message = error.message.clone();
        match self.failure.lock() {
            Ok(mut failure) => *failure = Some(error),
            Err(error) => return format!("callback failure state is poisoned: {error}; {message}"),
        }
        message
    }

    fn take_failure(&self) -> Result<Option<Error>, Error> {
        self.failure
            .lock()
            .map(|mut failure| failure.take())
            .map_err(|error| {
                Error::new(
                    crate::QDK_INTERNAL_ERROR,
                    format!("callback failure state is poisoned: {error}"),
                )
            })
    }
}

struct CircuitAdapter {
    name: String,
    callback: unsafe extern "C" fn(
        QdkContext,
        *mut QdkCircuitCallbackScope,
        *mut QdkCircuit,
        *mut *mut QdkDiagnostic,
    ) -> QdkStatus,
    context: Arc<ForeignContext>,
    shared: Arc<Shared>,
}

impl CircuitAdapter {
    fn call(&self, store: CircuitStore) -> Result<CircuitStore, Error> {
        let mut working = Box::new(QdkCircuit {
            inner: Handle::new(Circuit::of(store)),
        });
        working.inner.lifetime.borrowed.set(true);
        let _children = working.inner.callback_children();
        let mut scope = QdkCircuitCallbackScope {
            inner: Handle::new(None),
        };
        let mut diagnostic = ptr::null_mut();
        self.shared.crossings.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the stack scope and borrowed working circuit live through the
        // synchronous callback; no native references to their values are held.
        let status = unsafe {
            (self.callback)(
                self.context.cookie,
                &raw mut scope,
                &raw mut *working,
                &raw mut diagnostic,
            )
        };
        // SAFETY: callback diagnostic output transfers a live owning allocation.
        let diagnostic = unsafe { (!diagnostic.is_null()).then(|| Box::from_raw(diagnostic)) };
        callback_status(status, diagnostic)?;
        let current = working.inner.access()?;
        current
            .require_closed("return from a circuit callback")
            .map_err(|error| Error::new(QDK_CALLBACK_ERROR, error.to_string()))?;
        let selected = scope.inner.access()?;
        if let Some(replacement) = selected.as_ref() {
            Ok(replacement
                .inner
                .access()?
                .closed_store("return replacement")
                .map_err(Error::from)?
                .clone())
        } else {
            Ok(current.store().clone())
        }
        // Scope, children and working owner drop in that order on every exit.
    }
}

impl CircuitPass for CircuitAdapter {
    fn name(&self) -> &str {
        &self.name
    }
    fn claim(&self) -> PassClaim {
        PassClaim::unchecked()
    }
    fn run(
        &self,
        circuit: CircuitStore,
        _context: &mut PassContext,
    ) -> Result<PassOutput<CircuitStore>, String> {
        self.call(circuit)
            .map(PassOutput::changed)
            .map_err(|error| self.shared.fail(error))
    }
}

struct GraphAdapter {
    pass: ForeignGraphPass,
    shared: Arc<Shared>,
}

impl GraphPass for GraphAdapter {
    fn name(&self) -> &str {
        &self.pass.name
    }
    fn claim(&self) -> PassClaim {
        PassClaim::unchecked()
    }
    fn run(&self, graph: Graph, _context: &mut PassContext) -> Result<PassOutput<Graph>, String> {
        self.pass
            .run(&graph)
            .map(|(graph, outcome, crossings)| {
                self.shared
                    .crossings
                    .fetch_add(crossings, Ordering::Relaxed);
                PassOutput {
                    program: graph,
                    changed: !matches!(outcome, PassOutcome::Unchanged),
                }
            })
            .map_err(|error| self.shared.fail(error))
    }
}

fn anchor(tag: u32) -> Result<PassAnchor, Error> {
    match tag {
        QDK_ANCHOR_CIRCUIT_BUILT => Ok(PassAnchor::CircuitBuilt),
        QDK_ANCHOR_GRAPH_LOWERED => Ok(PassAnchor::GraphLowered),
        QDK_ANCHOR_GRAPH_REWRITTEN => Ok(PassAnchor::GraphRewritten),
        QDK_ANCHOR_CIRCUIT_RAISED => Ok(PassAnchor::CircuitRaised),
        _ => Err(Error::invalid("unknown pass anchor")),
    }
}

fn entry(value: &PassEntry) -> QdkPassEntryInfo {
    QdkPassEntryInfo {
        anchor: match value.anchor {
            PassAnchor::CircuitBuilt => QDK_ANCHOR_CIRCUIT_BUILT,
            PassAnchor::GraphLowered => QDK_ANCHOR_GRAPH_LOWERED,
            PassAnchor::GraphRewritten => QDK_ANCHOR_GRAPH_REWRITTEN,
            PassAnchor::CircuitRaised => QDK_ANCHOR_CIRCUIT_RAISED,
        },
        built_in: u32::from(value.built_in),
        name: value.name.as_bytes().to_vec().into(),
    }
}

/// Offers an independent closed, unpoisoned, inactive and dependent-free
/// replacement. Acceptance moves ownership and immediately clears candidate.
/// Rejection leaves caller ownership unchanged. A second offer is `AlreadySet`.
/// # Safety
/// Scope is valid only during its callback. Candidate is a disjoint owning
/// slot containing a live circuit; error output is writable and disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_callback_offer(
    scope: *mut QdkCircuitCallbackScope,
    candidate: *mut *mut QdkCircuit,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: callback supplies a live scope and candidate owning slot.
    unsafe {
        boundary(error, None, || {
            let mut selected = required(scope, "callback scope")?.inner.access()?;
            if selected.is_some() {
                return Err(Error::new(
                    QDK_ALREADY_SET,
                    "a replacement has already been accepted",
                ));
            }
            let candidate = slot(candidate, "candidate slot")?;
            let value = required(*candidate, "candidate")?;
            value.inner.require_releasable()?;
            value
                .inner
                .access()?
                .require_closed("offer a callback replacement")
                .map_err(Error::from)?;
            let accepted = Box::from_raw(*candidate);
            *candidate = ptr::null_mut();
            *selected = Some(accepted);
            Ok(())
        })
    }
}

/// Creates an owning native pipeline with or without built-ins (one or zero),
/// using an explicit optimization-level tag.
/// # Safety
/// Writable output slots are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_new(
    built_ins: u32,
    optimization: u32,
    out: *mut *mut QdkPipeline,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller provides disjoint writable outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let mut native = match built_ins {
                0 => PassPipeline::empty(),
                1 => PassPipeline::with_built_ins(),
                _ => return Err(Error::invalid("built_ins must be zero or one")),
            };
            native.set_optimization_level(match optimization {
                QDK_OPTIMIZATION_NONE => OptimizationLevel::None,
                QDK_OPTIMIZATION_DEFAULT => OptimizationLevel::Default,
                QDK_OPTIMIZATION_AGGRESSIVE => OptimizationLevel::Aggressive,
                _ => return Err(Error::invalid("unknown optimization level")),
            });
            *out = Box::into_raw(Box::new(QdkPipeline {
                inner: Handle::new(Pipeline {
                    native,
                    shared: Arc::new(Shared::default()),
                }),
            }));
            Ok(())
        })
    }
}

/// Registers a circuit callback at a circuit anchor. Success transfers context
/// destructor responsibility; every registration failure leaves it caller-owned.
/// Calls and destruction are synchronous on the calling thread, never concurrent.
/// # Safety
/// Pipeline, string extents and function pointers are valid. Context is an
/// integer token whose target supports sequential use on any calling thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_add_circuit(
    pipeline: *mut QdkPipeline,
    anchor_tag: u32,
    name: QdkString,
    callback: QdkCircuitCallback,
    cookie: QdkContext,
    destructor: QdkContextDestructor,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies valid inputs, callbacks and disjoint error slot.
    unsafe {
        boundary(error, None, || {
            let mut pipeline = required(pipeline, "pipeline")?.inner.access()?;
            let anchor = anchor(anchor_tag)?;
            let name = name.read()?.to_owned();
            let callback =
                callback.ok_or_else(|| Error::invalid("circuit callback must be non-NULL"))?;
            let context = ForeignContext::new(cookie, destructor);
            let pass = CircuitAdapter {
                name,
                callback,
                context: Arc::clone(&context),
                shared: Arc::clone(&pipeline.shared),
            };
            pipeline
                .native
                .add_circuit_pass(anchor, Box::new(pass), false)
                .map_err(|error| Error::invalid(error.to_string()))?;
            context.accept();
            Ok(())
        })
    }
}

/// Registers a graph decision callback at a graph anchor, in registration order.
/// Context ownership transfers only on success.
/// # Safety
/// Inputs and callbacks are valid; context follows `qdk_graph_pass_new`'s contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_add_graph(
    pipeline: *mut QdkPipeline,
    anchor_tag: u32,
    options: *const QdkGraphPassOptions,
    callback: QdkGraphCallback,
    cookie: QdkContext,
    destructor: QdkContextDestructor,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies valid input extents and foreign callbacks.
    unsafe {
        boundary(error, None, || {
            let mut pipeline = required(pipeline, "pipeline")?.inner.access()?;
            let anchor = anchor(anchor_tag)?;
            let pass = ForeignGraphPass::read(options, callback, cookie, destructor)?;
            let context = Arc::clone(&pass.context);
            let pass = GraphAdapter {
                pass,
                shared: Arc::clone(&pipeline.shared),
            };
            pipeline
                .native
                .add_graph_pass(anchor, Box::new(pass), false)
                .map_err(|error| Error::invalid(error.to_string()))?;
            context.accept();
            Ok(())
        })
    }
}

/// Returns the native listing length, including enabled built-ins.
/// # Safety
/// Pipeline is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_count(
    pipeline: *const QdkPipeline,
    out: *mut usize,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live pipeline and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            *out = required(pipeline, "pipeline")?
                .inner
                .access()?
                .native
                .listing()
                .len();
            Ok(())
        })
    }
}

/// Copies a native listing entry, ordered by anchor then registration.
/// # Safety
/// Pipeline is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_entry(
    pipeline: *const QdkPipeline,
    index: usize,
    out: *mut QdkPassEntryInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live pipeline and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let pipeline = required(pipeline, "pipeline")?.inner.access()?;
            let listing = pipeline.native.listing();
            *out = entry(
                listing
                    .get(index)
                    .ok_or_else(|| Error::invalid("pipeline entry index is out of range"))?,
            );
            Ok(())
        })
    }
}

/// Executes the native pipeline without modifying the input circuit. Neither
/// input nor pipeline can be freed or reentered while callbacks are running.
/// # Safety
/// Handles are live; writable outputs are disjoint; callbacks must not unwind.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_run(
    pipeline: *const QdkPipeline,
    circuit: *const QdkCircuit,
    out: *mut *mut QdkPipelineResult,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies live handles and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let pipeline = required(pipeline, "pipeline")?.inner.access()?;
            let circuit = required(circuit, "circuit")?.inner.access()?;
            let store = circuit
                .closed_store("run a pipeline")
                .map_err(Error::from)?;
            pipeline.shared.take_failure()?;
            pipeline.shared.crossings.store(0, Ordering::Relaxed);
            let native = match pipeline.native.run(store) {
                Ok(run) => run,
                Err(error) => {
                    return Err(pipeline
                        .shared
                        .take_failure()?
                        .unwrap_or_else(|| Error::invalid(error.to_string())));
                }
            };
            *out = Box::into_raw(Box::new(QdkPipelineResult {
                inner: Handle::new(Run {
                    native,
                    crossings: pipeline.shared.crossings.load(Ordering::Relaxed),
                }),
            }));
            Ok(())
        })
    }
}

/// Copies native report sizes, analysis count and actual foreign callback count.
/// # Safety
/// Result is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_result_info(
    result: *const QdkPipelineResult,
    out: *mut QdkPipelineResultInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live result and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let result = required(result, "pipeline result")?.inner.access()?;
            *out = QdkPipelineResultInfo {
                executed_count: result.native.executed.len(),
                changed_count: result.native.changed.len(),
                diagnostic_count: result.native.diagnostics.len(),
                analyses_computed: result.native.analyses_computed,
                callback_count: result.crossings,
            };
            Ok(())
        })
    }
}

/// Copies the output circuit into an independent owner. Native pipeline
/// graph crossings do not restore named registers.
/// # Safety
/// Result is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_result_circuit(
    result: *const QdkPipelineResult,
    out: *mut *mut QdkCircuit,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live result and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let result = required(result, "pipeline result")?.inner.access()?;
            *out = Box::into_raw(Box::new(QdkCircuit {
                inner: Handle::new(Circuit::of(result.native.circuit.clone())),
            }));
            Ok(())
        })
    }
}

/// Copies the graph at the final graph anchor into an independent owner.
/// # Safety
/// Result is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_result_graph(
    result: *const QdkPipelineResult,
    out: *mut *mut QdkGraph,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live result and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let result = required(result, "pipeline result")?.inner.access()?;
            *out = QdkGraph::owned(result.native.graph.clone());
            Ok(())
        })
    }
}

/// Copies one executed pass entry in actual run order.
/// # Safety
/// Result is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_result_entry(
    result: *const QdkPipelineResult,
    index: usize,
    out: *mut QdkPassEntryInfo,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live result and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let result = required(result, "pipeline result")?.inner.access()?;
            *out = entry(
                result
                    .native
                    .executed
                    .get(index)
                    .ok_or_else(|| Error::invalid("executed entry index is out of range"))?,
            );
            Ok(())
        })
    }
}

/// Copies one name from the native changed-pass report.
/// # Safety
/// Result is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_result_changed(
    result: *const QdkPipelineResult,
    index: usize,
    out: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live result and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let out = output(out)?;
            let result = required(result, "pipeline result")?.inner.access()?;
            *out = result
                .native
                .changed
                .get(index)
                .ok_or_else(|| Error::invalid("changed pass index is out of range"))?
                .as_bytes()
                .to_vec()
                .into();
            Ok(())
        })
    }
}

/// Copies a native report diagnostic's pass name and message.
/// # Safety
/// Result is live; writable outputs are disjoint.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_result_diagnostic(
    result: *const QdkPipelineResult,
    index: usize,
    pass: *mut QdkBytes,
    message: *mut QdkBytes,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller supplies a live result and disjoint outputs.
    unsafe {
        boundary(error, None, || {
            let pass = output(pass)?;
            let message = output(message)?;
            let result = required(result, "pipeline result")?.inner.access()?;
            let diagnostic = result
                .native
                .diagnostics
                .get(index)
                .ok_or_else(|| Error::invalid("pipeline diagnostic index is out of range"))?;
            let name = diagnostic.pass.as_bytes().to_vec();
            let text = diagnostic.message.as_bytes().to_vec();
            *pass = name.into();
            *message = text.into();
            Ok(())
        })
    }
}

/// Releases a pipeline and its successfully registered callback contexts.
/// # Safety
/// Slot contains NULL or a live exclusively owned pipeline.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_free(
    value: *mut *mut QdkPipeline,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and its live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}

/// Releases an independent pipeline result.
/// # Safety
/// Slot contains NULL or a live exclusively owned result.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_pipeline_result_free(
    value: *mut *mut QdkPipelineResult,
    error: *mut *mut QdkDiagnostic,
) -> QdkStatus {
    // SAFETY: caller owns this slot and its live allocation.
    unsafe { boundary(error, None, || release(value, |value| &value.inner)) }
}
