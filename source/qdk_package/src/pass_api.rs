// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The published pass-authoring surface.
//!
//! Two contracts, and the reason there are two is structural. A patch says what
//! changes about nodes that already exist, and the rewrite framework certifies
//! it. A fragment says what a node becomes, which may be several nodes, and no
//! patch can express that.
//!
//! A pass declares which operations it applies to and the traversal stays
//! native, so a pass is consulted once per match rather than once per node.
//! Nothing crossing into Python borrows the graph, which is why the graph's own
//! transaction type is absent from this surface rather than merely undocumented.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use qsc_circuit_store::CircuitStore;
use qsc_codegen::graph_pass::{
    PassAction, PassMatch as Matched, PassOutcome, apply_fragments, apply_patches, find_matches,
};
use qsc_codegen::pass_anchor::{PassAnchor as Anchor, PassStage as Stage};
use qsc_codegen::pass_pipeline::{
    CircuitPass, GraphPass, OptimizationLevel, PassClaim, PassContext, PassOutput,
    PassPipeline as Pipeline,
};
use qsc_qhdg::{Graph, StableNodeId};

use crate::circuit_api::QuantumCircuit;
use crate::graph_api::QuantumGraph;

/// What the program is at an anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum PassStage {
    /// A circuit, carrying registers, moments, and compile-time loop indices.
    Circuit,
    /// A port graph, carrying regions, ports, ownership, effects, and identity.
    Graph,
}

impl From<Stage> for PassStage {
    fn from(value: Stage) -> Self {
        match value {
            Stage::Circuit => Self::Circuit,
            Stage::Graph => Self::Graph,
        }
    }
}

/// Where a pass runs in the progression from a circuit to generated code.
#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int, from_py_object, module = "qdk.circuit")]
pub enum PassAnchor {
    /// On the circuit, after it is built or imported and before it is lowered.
    CircuitBuilt,
    /// On the graph, immediately after lowering and before any rewrite.
    GraphLowered,
    /// On the graph, after the built-in rewrites and before it is read.
    GraphRewritten,
    /// On the circuit raised back out of the graph, before it is exported.
    CircuitRaised,
}

impl From<PassAnchor> for Anchor {
    fn from(value: PassAnchor) -> Self {
        match value {
            PassAnchor::CircuitBuilt => Self::CircuitBuilt,
            PassAnchor::GraphLowered => Self::GraphLowered,
            PassAnchor::GraphRewritten => Self::GraphRewritten,
            PassAnchor::CircuitRaised => Self::CircuitRaised,
        }
    }
}

impl From<Anchor> for PassAnchor {
    fn from(value: Anchor) -> Self {
        match value {
            Anchor::CircuitBuilt => Self::CircuitBuilt,
            Anchor::GraphLowered => Self::GraphLowered,
            Anchor::GraphRewritten => Self::GraphRewritten,
            Anchor::CircuitRaised => Self::CircuitRaised,
        }
    }
}

#[pymethods]
impl PassAnchor {
    /// Every anchor, in the order the progression reaches them.
    #[staticmethod]
    fn all() -> Vec<Self> {
        Anchor::ALL.map(Into::into).to_vec()
    }

    /// The stable name of the anchor.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    #[getter]
    fn name(&self) -> &'static str {
        Anchor::from(*self).name()
    }

    /// What the program is when a pass at this anchor runs.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    #[getter]
    fn stage(&self) -> PassStage {
        Anchor::from(*self).stage().into()
    }

    /// Whether the anchor is reached going out of the graph rather than into it.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    #[getter]
    fn outbound(&self) -> bool {
        Anchor::from(*self).is_outbound()
    }
}

/// A node a pass matched.
///
/// Plain data rather than a view, because a pass runs while the graph is not
/// borrowed and a live view could not survive that.
#[pyclass(module = "qdk.circuit", frozen)]
pub struct PassMatch {
    /// The node's durable identity, which survives a rewrite that keeps it.
    #[pyo3(get)]
    node: u64,
    /// What the node is.
    #[pyo3(get)]
    operation: String,
    /// How many operands the node threads from input to output.
    #[pyo3(get)]
    operands: usize,
}

#[pymethods]
impl PassMatch {
    fn __repr__(&self) -> String {
        format!(
            "PassMatch(node={}, operation={:?})",
            self.node, self.operation
        )
    }
}

/// What a pass asks to happen at a matched node.
///
/// The vocabulary is deliberately narrow. Edges are not exposed, because an
/// edge in this graph names an operand rather than a node, and a caller that
/// rethreaded operands by hand would be able to produce a graph that no longer
/// says what its program does.
#[pyclass(module = "qdk.circuit", frozen, from_py_object)]
#[derive(Clone)]
pub struct PassPatch {
    action: PassAction,
}

#[pymethods]
impl PassPatch {
    /// Keeps the node and its operands, changes what it is.
    #[staticmethod]
    fn replace(operation: String) -> Self {
        Self {
            action: PassAction::Replace(operation),
        }
    }

    /// Retires the node and splices each operand thread past it.
    #[staticmethod]
    fn remove() -> Self {
        Self {
            action: PassAction::Remove,
        }
    }

    fn __repr__(&self) -> String {
        match &self.action {
            PassAction::Replace(operation) => format!("PassPatch.replace({operation:?})"),
            _ => "PassPatch.remove()".to_string(),
        }
    }
}

/// What a pass run produced.
#[pyclass(module = "qdk.circuit", frozen)]
pub struct PassResult {
    /// The graph the run produced, which is a new graph rather than the old one.
    #[pyo3(get)]
    graph: Py<QuantumGraph>,
    /// How the run ended: `unchanged`, `committed`, `budget_exhausted`, or
    /// `oscillation`.
    #[pyo3(get)]
    outcome: String,
    /// How many decisions were applied.
    #[pyo3(get)]
    applications: usize,
    /// How many times the pass was consulted, which is one per match.
    #[pyo3(get)]
    crossings: usize,
}

#[pymethods]
impl PassResult {
    fn __repr__(&self) -> String {
        format!(
            "PassResult(outcome={:?}, applications={}, crossings={})",
            self.outcome, self.applications, self.crossings
        )
    }
}

fn wrap(
    py: Python<'_>,
    graph: Graph,
    outcome: &PassOutcome,
    crossings: usize,
) -> PyResult<PassResult> {
    Ok(PassResult {
        graph: Py::new(py, QuantumGraph { graph })?,
        outcome: outcome.name().to_string(),
        applications: outcome.applications(),
        crossings,
    })
}

/// Consults the pass once for each matched node and collects its decisions.
///
/// The graph is not borrowed here, which is the whole point: matches arrive as
/// owned records and decisions leave as owned records.
fn decide<T>(
    matches: Vec<Matched>,
    mut ask: impl FnMut(&Matched) -> PyResult<Option<T>>,
    into: impl Fn(T) -> PassAction,
) -> PyResult<(BTreeMap<StableNodeId, PassAction>, usize)> {
    let crossings = matches.len();
    let mut decisions = BTreeMap::new();
    for matched in matches {
        if let Some(decision) = ask(&matched)? {
            decisions.insert(matched.node, into(decision));
        }
    }
    Ok((decisions, crossings))
}

pub(crate) fn run_patches(
    py: Python<'_>,
    graph: &Graph,
    operations: &BTreeSet<String>,
    pass: &Bound<'_, PyAny>,
    budget: Option<usize>,
) -> PyResult<PassResult> {
    let (rewritten, outcome, crossings) = patched(graph, operations, pass, budget)?;
    wrap(py, rewritten, &outcome, crossings)
}

fn patched(
    graph: &Graph,
    operations: &BTreeSet<String>,
    pass: &Bound<'_, PyAny>,
    budget: Option<usize>,
) -> PyResult<(Graph, PassOutcome, usize)> {
    let matches = find_matches(graph, operations);
    let budget = budget.unwrap_or_else(|| matches.len().max(1));
    let (decisions, crossings) = decide(
        matches,
        |matched| {
            let decided = pass.call1((view(matched),))?;
            if decided.is_none() {
                return Ok(None);
            }
            Ok(Some(decided.extract::<PassPatch>()?.action))
        },
        |action| action,
    )?;
    let (rewritten, outcome) = apply_patches(graph, &decisions, budget)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    Ok((rewritten, outcome, crossings))
}

pub(crate) fn run_fragments(
    py: Python<'_>,
    graph: &Graph,
    operations: &BTreeSet<String>,
    pass: &Bound<'_, PyAny>,
) -> PyResult<PassResult> {
    let (expanded, outcome, crossings) = fragmented(graph, operations, pass)?;
    wrap(py, expanded, &outcome, crossings)
}

fn fragmented(
    graph: &Graph,
    operations: &BTreeSet<String>,
    pass: &Bound<'_, PyAny>,
) -> PyResult<(Graph, PassOutcome, usize)> {
    let (decisions, crossings) = decide(
        find_matches(graph, operations),
        |matched| {
            let decided = pass.call1((view(matched),))?;
            if decided.is_none() {
                return Ok(None);
            }
            Ok(Some(decided.extract::<Vec<String>>()?))
        },
        PassAction::Expand,
    )?;
    let (expanded, outcome) = apply_fragments(graph, &decisions)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    Ok((expanded, outcome, crossings))
}

fn view(matched: &Matched) -> PassMatch {
    PassMatch {
        node: matched.node.0,
        operation: matched.operation.clone(),
        operands: matched.operands,
    }
}

/// One registered pass, as the pipeline lists it.
#[pyclass(module = "qdk.circuit", frozen, skip_from_py_object)]
#[derive(Clone)]
pub struct PassListing {
    /// Where in the progression the pass runs.
    #[pyo3(get)]
    anchor: PassAnchor,
    /// The name the pass is listed under.
    #[pyo3(get)]
    name: String,
    /// Whether the pass came with the compiler rather than from a caller.
    #[pyo3(get)]
    built_in: bool,
}

#[pymethods]
impl PassListing {
    fn __repr__(&self) -> String {
        format!(
            "PassListing(anchor={:?}, name={:?}, built_in={})",
            Anchor::from(self.anchor).name(),
            self.name,
            self.built_in
        )
    }
}

impl From<qsc_codegen::pass_pipeline::PassEntry> for PassListing {
    fn from(value: qsc_codegen::pass_pipeline::PassEntry) -> Self {
        Self {
            anchor: value.anchor.into(),
            name: value.name,
            built_in: value.built_in,
        }
    }
}

/// What a pipeline run produced.
#[pyclass(module = "qdk.circuit", frozen)]
pub struct PipelineResult {
    /// The circuit that came back out, after the outbound anchor.
    #[pyo3(get)]
    circuit: Py<QuantumCircuit>,
    /// The graph as it stood when the last graph anchor finished with it.
    #[pyo3(get)]
    graph: Py<QuantumGraph>,
    /// The passes that ran, in the order they ran.
    #[pyo3(get)]
    executed: Vec<PassListing>,
    /// The passes that reported changing the program, in run order.
    #[pyo3(get)]
    changed: Vec<String>,
    /// What the passes reported that was not a failure, as `(pass, message)`.
    #[pyo3(get)]
    diagnostics: Vec<(String, String)>,
    /// How many analyses the run computed rather than served from the cache one
    /// manager keeps for the whole run.
    #[pyo3(get)]
    analyses_computed: usize,
    /// How many times a pass was consulted, totalled over the run.
    #[pyo3(get)]
    crossings: usize,
}

#[pymethods]
impl PipelineResult {
    fn __repr__(&self) -> String {
        format!(
            "PipelineResult({} passes ran, {} changed, crossings={})",
            self.executed.len(),
            self.changed.len(),
            self.crossings
        )
    }
}

/// What the adapters report back out of a run.
///
/// The exception a pass raised is carried rather than flattened, so a caller
/// sees what their own code raised instead of a string the pipeline made up.
#[derive(Default)]
struct Shared {
    failure: Arc<Mutex<Option<PyErr>>>,
    crossings: Arc<AtomicUsize>,
}

impl Shared {
    fn take_failure(&self) -> Option<PyErr> {
        self.failure
            .lock()
            .expect("no other thread holds this")
            .take()
    }
}

fn stash(failure: &Mutex<Option<PyErr>>, error: PyErr) -> String {
    let reason = error.to_string();
    *failure.lock().expect("no other thread holds this") = Some(error);
    reason
}

/// A pass written in Python, running where the program is a circuit.
struct BoundCircuitPass {
    name: String,
    pass: Py<PyAny>,
    failure: Arc<Mutex<Option<PyErr>>>,
}

impl BoundCircuitPass {
    fn call(&self, py: Python<'_>, circuit: CircuitStore) -> PyResult<CircuitStore> {
        let handed = Py::new(py, QuantumCircuit::of(circuit))?;
        let returned = self.pass.bind(py).call1((handed.clone_ref(py),))?;
        if returned.is_none() {
            // The pass edited what it was handed rather than returning one.
            return Ok(handed.borrow(py).closed_store("run")?.clone());
        }
        let produced = returned.cast::<QuantumCircuit>().map_err(PyErr::from)?;
        let store = produced.borrow().closed_store("run")?.clone();
        Ok(store)
    }
}

impl CircuitPass for BoundCircuitPass {
    fn name(&self) -> &str {
        &self.name
    }

    fn claim(&self) -> PassClaim {
        // A Python pass is handed a circuit and hands one back. Nothing here
        // checks what it did, and the claim says so rather than implying a
        // check that does not exist.
        PassClaim::unchecked()
    }

    fn run(
        &self,
        circuit: CircuitStore,
        _context: &mut PassContext,
    ) -> Result<PassOutput<CircuitStore>, String> {
        Python::attach(|py| {
            self.call(py, circuit)
                // A bound circuit pass is handed a circuit and hands one back,
                // with no report of what it did, so the pipeline has to assume
                // it did something rather than compare the two programs.
                .map(PassOutput::changed)
                .map_err(|error| stash(&self.failure, error))
        })
    }
}

/// A pass written in Python, running where the program is a graph.
struct BoundGraphPass {
    name: String,
    operations: BTreeSet<String>,
    pass: Py<PyAny>,
    /// Whether the pass returns a fragment rather than a patch.
    expands: bool,
    budget: Option<usize>,
    failure: Arc<Mutex<Option<PyErr>>>,
    crossings: Arc<AtomicUsize>,
}

impl GraphPass for BoundGraphPass {
    fn name(&self) -> &str {
        &self.name
    }

    fn claim(&self) -> PassClaim {
        // The rewrites go through the certified driver, but the caller decides
        // each one and the obligation backing it is `Assumed`, so the claim is
        // no better than a direct edit's.
        PassClaim::unchecked()
    }

    fn run(&self, graph: Graph, _context: &mut PassContext) -> Result<PassOutput<Graph>, String> {
        Python::attach(|py| {
            let pass = self.pass.bind(py);
            let produced = if self.expands {
                fragmented(&graph, &self.operations, pass)
            } else {
                patched(&graph, &self.operations, pass, self.budget)
            };
            match produced {
                Ok((graph, outcome, crossings)) => {
                    self.crossings.fetch_add(crossings, Ordering::Relaxed);
                    Ok(PassOutput {
                        program: graph,
                        changed: !matches!(outcome, PassOutcome::Unchanged),
                    })
                }
                Err(error) => Err(stash(&self.failure, error)),
            }
        })
    }
}

enum Registration {
    Circuit(Py<PyAny>),
    Graph {
        operations: BTreeSet<String>,
        pass: Py<PyAny>,
        expands: bool,
        budget: Option<usize>,
    },
}

struct Registered {
    anchor: Anchor,
    name: String,
    registration: Registration,
}

/// Passes to run at the named anchors, in one declared order.
///
/// Adding a pass to an anchor is the only edit. Nothing here reorders,
/// replaces, or removes what is already registered, which is what lets the
/// anchor be a stable thing to write against while what runs inside it stays
/// free to change.
#[pyclass(module = "qdk.circuit")]
pub struct PassPipeline {
    /// Whether the passes that ship with the compiler are included.
    built_ins: bool,
    /// How hard the passes are asked to work. This is the configuration that
    /// could previously only enter a pass through its own constructor.
    level: OptimizationLevel,
    registered: Vec<Registered>,
}

impl PassPipeline {
    fn compile(&self, py: Python<'_>, shared: &Shared) -> PyResult<Pipeline> {
        let mut pipeline = if self.built_ins {
            Pipeline::with_built_ins()
        } else {
            Pipeline::empty()
        };
        pipeline.set_optimization_level(self.level);
        for entry in &self.registered {
            let added = match &entry.registration {
                Registration::Circuit(pass) => pipeline.add_circuit_pass(
                    entry.anchor,
                    Box::new(BoundCircuitPass {
                        name: entry.name.clone(),
                        pass: pass.clone_ref(py),
                        failure: Arc::clone(&shared.failure),
                    }),
                    false,
                ),
                Registration::Graph {
                    operations,
                    pass,
                    expands,
                    budget,
                } => pipeline.add_graph_pass(
                    entry.anchor,
                    Box::new(BoundGraphPass {
                        name: entry.name.clone(),
                        operations: operations.clone(),
                        pass: pass.clone_ref(py),
                        expands: *expands,
                        budget: *budget,
                        failure: Arc::clone(&shared.failure),
                        crossings: Arc::clone(&shared.crossings),
                    }),
                    false,
                ),
            };
            added.map_err(|error| PyValueError::new_err(error.to_string()))?;
        }
        Ok(pipeline)
    }

    fn register(
        &mut self,
        anchor: PassAnchor,
        stage: Stage,
        name: String,
        registration: Registration,
    ) -> PyResult<()> {
        let anchor = Anchor::from(anchor);
        if anchor.stage() != stage {
            return Err(PyValueError::new_err(format!(
                "'{}' is a {} anchor, so a {} pass cannot run there",
                anchor.name(),
                anchor.stage().name(),
                stage.name()
            )));
        }
        self.registered.push(Registered {
            anchor,
            name,
            registration,
        });
        Ok(())
    }
}

#[pymethods]
impl PassPipeline {
    /// Builds a pipeline, carrying the compiler's own passes unless declined.
    ///
    /// `optimization_level` is one of `"none"`, `"default"`, or
    /// `"aggressive"`, and every pass in the pipeline reads the same one.
    #[new]
    #[pyo3(signature = (*, built_ins = true, optimization_level = "default"))]
    fn new(built_ins: bool, optimization_level: &str) -> PyResult<Self> {
        let level = match optimization_level {
            "none" => OptimizationLevel::None,
            "default" => OptimizationLevel::Default,
            "aggressive" => OptimizationLevel::Aggressive,
            other => {
                return Err(PyValueError::new_err(format!(
                    "unknown optimization level '{other}'; expected 'none', 'default', or 'aggressive'"
                )));
            }
        };
        Ok(Self {
            built_ins,
            level,
            registered: Vec::new(),
        })
    }

    /// Registers a pass at an anchor where the program is a circuit.
    ///
    /// The pass is called with a `QuantumCircuit` and may edit it in place and
    /// return `None`, or return a circuit of its own.
    ///
    /// Raises `ValueError` when the anchor is one where the program is a graph.
    fn add_circuit_pass(
        &mut self,
        anchor: PassAnchor,
        name: String,
        pass_: Py<PyAny>,
    ) -> PyResult<()> {
        self.register(anchor, Stage::Circuit, name, Registration::Circuit(pass_))
    }

    /// Registers a patch-returning pass at an anchor where the program is a
    /// graph.
    ///
    /// The pass sees the same [`PassMatch`] and returns the same [`PassPatch`]
    /// it would through `QuantumGraph.rewrite`.
    ///
    /// Raises `ValueError` when the anchor is one where the program is a
    /// circuit.
    #[pyo3(signature = (anchor, name, operations, pass_, budget = None))]
    fn add_rewrite_pass(
        &mut self,
        anchor: PassAnchor,
        name: String,
        operations: Vec<String>,
        pass_: Py<PyAny>,
        budget: Option<usize>,
    ) -> PyResult<()> {
        self.register(
            anchor,
            Stage::Graph,
            name,
            Registration::Graph {
                operations: operations.into_iter().collect(),
                pass: pass_,
                expands: false,
                budget,
            },
        )
    }

    /// Registers a fragment-returning pass at an anchor where the program is a
    /// graph.
    ///
    /// The pass returns the sequence of operations that stands in a matched
    /// node's place, as it would through `QuantumGraph.expand`.
    ///
    /// Raises `ValueError` when the anchor is one where the program is a
    /// circuit.
    fn add_expand_pass(
        &mut self,
        anchor: PassAnchor,
        name: String,
        operations: Vec<String>,
        pass_: Py<PyAny>,
    ) -> PyResult<()> {
        self.register(
            anchor,
            Stage::Graph,
            name,
            Registration::Graph {
                operations: operations.into_iter().collect(),
                pass: pass_,
                expands: true,
                budget: None,
            },
        )
    }

    /// Every registered pass, in the order a run reaches it.
    fn listing(&self, py: Python<'_>) -> PyResult<Vec<PassListing>> {
        Ok(self
            .compile(py, &Shared::default())?
            .listing()
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// Walks a circuit through the whole progression, running each anchor.
    ///
    /// The circuit handed in is not changed. Raises whatever a pass raised, and
    /// `ValueError` when the circuit cannot be lowered, the graph cannot be
    /// raised, or a pass produced something its stage does not accept.
    fn run(&self, py: Python<'_>, circuit: &Bound<'_, QuantumCircuit>) -> PyResult<PipelineResult> {
        let store = circuit
            .borrow()
            .closed_store("run a pipeline over")?
            .clone();
        let shared = Shared::default();
        let pipeline = self.compile(py, &shared)?;
        let run = pipeline.run(&store).map_err(|error| {
            shared
                .take_failure()
                .unwrap_or_else(|| PyValueError::new_err(error.to_string()))
        })?;
        Ok(PipelineResult {
            circuit: Py::new(py, QuantumCircuit::of(run.circuit))?,
            graph: Py::new(py, QuantumGraph { graph: run.graph })?,
            executed: run.executed.into_iter().map(Into::into).collect(),
            changed: run.changed,
            diagnostics: run
                .diagnostics
                .into_iter()
                .map(|diagnostic| (diagnostic.pass, diagnostic.message))
                .collect(),
            analyses_computed: run.analyses_computed,
            crossings: shared.crossings.load(Ordering::Relaxed),
        })
    }

    fn __repr__(&self) -> String {
        format!(
            "PassPipeline({} registered, built_ins={})",
            self.registered.len(),
            self.built_ins
        )
    }
}
