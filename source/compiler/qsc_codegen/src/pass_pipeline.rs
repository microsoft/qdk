// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running passes at the named anchors, in one declared order.
//!
//! The pipeline is what makes an anchor more than a name. It walks the real
//! progression once, from a circuit to the graph and back out again, and stops
//! at each anchor to run whatever is registered there.
//!
//! Passes are registered at anchor granularity and run in the order the
//! progression reaches the anchors, with registration order breaking ties
//! inside one anchor. There is no way to reorder, replace, or remove what is
//! already registered, which is what keeps the anchor a stable thing to write
//! against while the composition inside it stays free to change.
//!
//! A Rust pass and a bound pass are the same thing here. Both are registered
//! the same way, both appear in the same listing, and neither is given a borrow
//! of the program it is handed.

use std::collections::{BTreeMap, BTreeSet};

use qsc_circuit_store::CircuitStore;
use qsc_qhdg::vocabulary::SELF_INVERSE;
use qsc_qhdg::{
    AnalysisManager, DiagonalCollapse, DischargeContext, GlobalPhasePolicy, Graph,
    GraphRewriteDriver, GraphRewriteOutcome, InverseCancellation, InverseVocabulary, Observation,
    Operand, OperandWires, OperationClass, OperationFacts, ProofObligation, ResourceId,
    RewriteContext, StableNodeId, verify,
};
use qsc_semantic_contracts::{Registry, gate_identity};

use crate::circuit_qhdg::{
    ALLOCATE, QUANTUM, RELEASE, RESET, circuit_to_graph, gate_names, graph_to_circuit,
    qubit_operands,
};
use crate::pass_anchor::{PassAnchor, PassStage};

/// How hard the pipeline is asking its passes to work.
///
/// This is the first thing that could only enter a pass through its own
/// constructor, which is why two passes could not previously agree about it.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum OptimizationLevel {
    /// Structural work only. A pass that trades compile time for program
    /// quality declines to run.
    None,
    #[default]
    Default,
    Aggressive,
}

/// How hard a caller is asking each of the two compiler-side stages to work.
///
/// Independent levels let a caller attribute a change to the stage that made
/// it. Both stages remove the same pairs wherever both accept, so enabling
/// them together cannot establish whether the FIR stage acted or declined.
///
/// No `Default` derive, because `OptimizationLevel`'s default is not `None` and
/// a stage that is opt-in must not arrive on by accident.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OptimizationLevels {
    /// The FIR stage, run inside the FIR transform pipeline.
    pub fir: OptimizationLevel,
    /// The post-partial-evaluation stage, run on RIR.
    pub rir: OptimizationLevel,
}

impl OptimizationLevels {
    /// Neither stage runs. This is what delivered output is compiled at.
    pub const OFF: Self = Self {
        fir: OptimizationLevel::None,
        rir: OptimizationLevel::None,
    };

    /// Only the post-partial-evaluation stage.
    #[must_use]
    pub const fn at_rir(rir: OptimizationLevel) -> Self {
        Self {
            fir: OptimizationLevel::None,
            rir,
        }
    }

    /// Only the FIR stage.
    #[must_use]
    pub const fn at_fir(fir: OptimizationLevel) -> Self {
        Self {
            fir,
            rir: OptimizationLevel::None,
        }
    }
}

impl Default for OptimizationLevels {
    fn default() -> Self {
        Self::OFF
    }
}

/// Something a pass wants the caller to know that is not a failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassDiagnostic {
    pub pass: String,
    pub message: String,
}

/// What a pass is given besides the program.
///
/// It carries the analysis manager, the optimization level, and a diagnostics
/// sink, and deliberately not a handle to a physical target: no physical target
/// exists yet, so the field would have no producer.
#[derive(Debug, Default)]
pub struct PassContext {
    analyses: AnalysisManager,
    level: OptimizationLevel,
    diagnostics: Vec<PassDiagnostic>,
}

impl PassContext {
    #[must_use]
    pub fn new(level: OptimizationLevel) -> Self {
        Self {
            analyses: AnalysisManager::default(),
            level,
            diagnostics: Vec::new(),
        }
    }

    #[must_use]
    pub const fn level(&self) -> OptimizationLevel {
        self.level
    }

    /// The analyses computed so far. One manager serves the whole run, so an
    /// analysis one pass computed is served to the next.
    pub const fn analyses(&mut self) -> &mut AnalysisManager {
        &mut self.analyses
    }

    /// Records something the caller should see. A pass that cannot continue
    /// returns an error instead.
    pub fn raise(&mut self, pass: &str, message: impl Into<String>) {
        self.diagnostics.push(PassDiagnostic {
            pass: pass.to_string(),
            message: message.into(),
        });
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[PassDiagnostic] {
        &self.diagnostics
    }
}

/// What a pass produced, and whether it changed anything.
///
/// A struct rather than a bare pair, because the useful next question is why a
/// pass changed nothing, and answering it should add a field here rather than
/// change every signature a second time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassOutput<T> {
    pub program: T,
    pub changed: bool,
}

impl<T> PassOutput<T> {
    pub const fn changed(program: T) -> Self {
        Self {
            program,
            changed: true,
        }
    }

    pub const fn unchanged(program: T) -> Self {
        Self {
            program,
            changed: false,
        }
    }
}

/// How a pass's preservation claim is backed.
///
/// This is the tiering the design review asks for, stated by the pass rather
/// than inferred from it. A certified pass has every edit discharged by the
/// driver, and names the obligations its rules declare, because two of those
/// obligations check nothing about the program and a tier that could not see
/// the difference would launder a deferral into a proof. A direct-mutation pass
/// edits the program itself, so nothing per rewrite can check it and the claim
/// has to be checked against programs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PassTier {
    /// Every edit commits through a certificate the driver discharges, against
    /// the obligations named here.
    Certified {
        obligations: BTreeSet<ProofObligation>,
    },
    /// The pass edits the program itself. A named corpus is the only thing
    /// standing behind its claim, and `None` means nothing is.
    DirectMutation { corpus: Option<String> },
}

/// What a pass claims about the programs it rewrites.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassClaim {
    /// The observations this pass leaves as it found them.
    pub preserves: BTreeSet<Observation>,
    pub tier: PassTier,
}

impl PassClaim {
    /// A pass whose every edit the driver discharges, preserving everything.
    ///
    /// The obligations are stated rather than implied, because "certified" is
    /// worth exactly what the driver checks and the claim check reads them to
    /// find out.
    #[must_use]
    pub fn certified(obligations: impl IntoIterator<Item = ProofObligation>) -> Self {
        Self {
            preserves: Observation::all(),
            tier: PassTier::Certified {
                obligations: obligations.into_iter().collect(),
            },
        }
    }

    /// A pass that edits the program itself, with nothing checking the claim.
    #[must_use]
    pub fn unchecked() -> Self {
        Self {
            preserves: Observation::all(),
            tier: PassTier::DirectMutation { corpus: None },
        }
    }
}

/// A pass that runs where the program is a circuit.
///
/// The circuit is moved in and moved out rather than borrowed, so a pass may be
/// written in another language without anything holding a borrow across the
/// call.
pub trait CircuitPass: Send + Sync {
    /// The name this pass is listed under.
    fn name(&self) -> &str;

    /// What this pass preserves, and what backs the claim.
    fn claim(&self) -> PassClaim;

    /// Runs the pass.
    ///
    /// # Errors
    ///
    /// Returns the reason the pass could not produce a circuit.
    fn run(
        &self,
        circuit: CircuitStore,
        context: &mut PassContext,
    ) -> Result<PassOutput<CircuitStore>, String>;
}

/// A pass that runs where the program is a graph.
pub trait GraphPass: Send + Sync {
    /// The name this pass is listed under.
    fn name(&self) -> &str;

    /// What this pass preserves, and what backs the claim.
    fn claim(&self) -> PassClaim;

    /// Runs the pass.
    ///
    /// # Errors
    ///
    /// Returns the reason the pass could not produce a graph.
    fn run(&self, graph: Graph, context: &mut PassContext) -> Result<PassOutput<Graph>, String>;
}

/// Why a pipeline could not be built or run.
#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum PipelineError {
    #[error("'{anchor}' is a {expected} anchor, so a {given} pass cannot run there")]
    Stage {
        anchor: &'static str,
        expected: &'static str,
        given: &'static str,
    },
    #[error("the circuit could not be lowered: {0}")]
    Lower(String),
    #[error("the graph could not be raised: {0}")]
    Raise(String),
    #[error("iteration group '{name}' needs at least one pass and at least one iteration")]
    EmptyGroup { name: String },
    #[error("pass '{name}' declares it preserves nothing, which is not a claim")]
    EmptyClaim { name: String },
    #[error("pass '{name}' names a corpus that is not a name")]
    UnnamedCorpus { name: String },
    #[error("pass '{name}' names corpus '{corpus}', which nothing runs")]
    UnregisteredCorpus { name: String, corpus: String },
    #[error("pass '{name}' claims certification without naming an obligation to be certified by")]
    UnbackedCertification { name: String },
    #[error(
        "pass '{name}' claims certification over obligation '{obligation}', which checks nothing about the program"
    )]
    UncheckedCertification { name: String, obligation: String },
    #[error("pass '{name}' at '{anchor}' failed: {reason}")]
    Failed {
        name: String,
        anchor: &'static str,
        reason: String,
    },
}

/// One registered pass, as the listing reports it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PassEntry {
    pub anchor: PassAnchor,
    pub name: String,
    /// Whether the pass came with the pipeline rather than from a caller.
    pub built_in: bool,
}

/// How an iteration group stopped.
///
/// The two outcomes are separate for the same reason the rewrite driver keeps
/// `Committed` and `BudgetExhausted` apart: a group that ran out of iterations
/// with work still to do has not reached a fixed point, and a caller that
/// cannot tell the difference finds out later and somewhere else.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GroupOutcome {
    /// No member reported a change, so the group is at a fixed point.
    Converged { group: String, iterations: usize },
    /// The bound was reached with a member still reporting changes.
    BoundExhausted { group: String, iterations: usize },
}

/// What a pipeline run produced.
pub struct PipelineRun {
    /// The circuit that came back out, after the outbound anchor.
    pub circuit: CircuitStore,
    /// The graph as it stood when the last graph anchor finished with it.
    pub graph: Graph,
    /// The passes that ran, in the order they ran. A group member appears once
    /// for each iteration it ran in.
    pub executed: Vec<PassEntry>,
    /// What the passes reported that was not a failure.
    pub diagnostics: Vec<PassDiagnostic>,
    /// The passes that reported changing the program, in run order.
    pub changed: Vec<String>,
    /// How each iteration group stopped.
    pub groups: Vec<GroupOutcome>,
    /// How many analyses the run computed rather than served. One manager
    /// serves every anchor, so this is what reuse across anchors looks like.
    pub analyses_computed: usize,
}

/// Passes that repeat together until none of them changes the program.
///
/// Graph-stage only, and for the reason the phase excluded the canonical groups
/// themselves: every group the algorithms addendum specifies is made of graph
/// passes, so a circuit group would be a construct with nothing to put in it.
struct PassGroup {
    name: String,
    passes: Vec<Box<dyn GraphPass>>,
    max_iterations: usize,
}

enum Registered {
    Circuit(Box<dyn CircuitPass>),
    Graph(Box<dyn GraphPass>),
    Group(PassGroup),
}

struct Registration {
    anchor: PassAnchor,
    registered: Registered,
    built_in: bool,
}

impl Registration {
    /// What this registration contributes to a listing, in run order. A group
    /// contributes one entry per member, so a caller sees what will run.
    fn entries(&self) -> Vec<PassEntry> {
        let named = |name: String| PassEntry {
            anchor: self.anchor,
            name,
            built_in: self.built_in,
        };
        match &self.registered {
            Registered::Circuit(pass) => vec![named(pass.name().to_string())],
            Registered::Graph(pass) => vec![named(pass.name().to_string())],
            Registered::Group(group) => group
                .passes
                .iter()
                .map(|pass| named(format!("{}/{}", group.name, pass.name())))
                .collect(),
        }
    }
}

/// Passes to run at the named anchors, in one declared order.
#[derive(Default)]
pub struct PassPipeline {
    registrations: Vec<Registration>,
    level: OptimizationLevel,
}

impl PassPipeline {
    /// A pipeline with nothing registered, which runs the progression alone.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Sets the level every pass in this pipeline reads.
    pub const fn set_optimization_level(&mut self, level: OptimizationLevel) {
        self.level = level;
    }

    #[must_use]
    pub const fn optimization_level(&self) -> OptimizationLevel {
        self.level
    }

    /// A pipeline carrying the passes that ship with the compiler.
    #[must_use]
    pub fn with_built_ins() -> Self {
        let mut pipeline = Self::empty();
        pipeline
            .add_graph_pass(
                PassAnchor::GraphRewritten,
                Box::new(CancelInversePairs),
                true,
            )
            .expect("a graph pass belongs at a graph anchor");
        pipeline
            .add_graph_pass(
                PassAnchor::GraphRewritten,
                Box::new(RetireDiagonalBeforeZCollapse),
                true,
            )
            .expect("a graph pass belongs at a graph anchor");
        pipeline
    }

    /// Registers a circuit pass at an anchor.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Stage`] when the anchor is one where the
    /// program is a graph, or an error when the pass's claim is not one.
    pub fn add_circuit_pass(
        &mut self,
        anchor: PassAnchor,
        pass: Box<dyn CircuitPass>,
        built_in: bool,
    ) -> Result<(), PipelineError> {
        Self::require(anchor, PassStage::Circuit)?;
        check_claim(pass.name(), &pass.claim())?;
        self.registrations.push(Registration {
            anchor,
            registered: Registered::Circuit(pass),
            built_in,
        });
        Ok(())
    }

    /// Registers a graph pass at an anchor.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Stage`] when the anchor is one where the
    /// program is a circuit, or an error when the pass's claim is not one.
    pub fn add_graph_pass(
        &mut self,
        anchor: PassAnchor,
        pass: Box<dyn GraphPass>,
        built_in: bool,
    ) -> Result<(), PipelineError> {
        Self::require(anchor, PassStage::Graph)?;
        check_claim(pass.name(), &pass.claim())?;
        self.registrations.push(Registration {
            anchor,
            registered: Registered::Graph(pass),
            built_in,
        });
        Ok(())
    }

    /// Registers a group of graph passes that repeat together until none of
    /// them reports a change or the bound is reached.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Stage`] when the anchor is one where the
    /// program is a circuit, and [`PipelineError::EmptyGroup`] when the group
    /// has no members or no iterations.
    pub fn add_graph_group(
        &mut self,
        anchor: PassAnchor,
        name: impl Into<String>,
        passes: Vec<Box<dyn GraphPass>>,
        max_iterations: usize,
    ) -> Result<(), PipelineError> {
        Self::require(anchor, PassStage::Graph)?;
        let name = name.into();
        if passes.is_empty() || max_iterations == 0 {
            return Err(PipelineError::EmptyGroup { name });
        }
        for pass in &passes {
            check_claim(pass.name(), &pass.claim())?;
        }
        self.registrations.push(Registration {
            anchor,
            registered: Registered::Group(PassGroup {
                name,
                passes,
                max_iterations,
            }),
            built_in: false,
        });
        Ok(())
    }

    fn require(anchor: PassAnchor, given: PassStage) -> Result<(), PipelineError> {
        if anchor.stage() == given {
            return Ok(());
        }
        Err(PipelineError::Stage {
            anchor: anchor.name(),
            expected: anchor.stage().name(),
            given: given.name(),
        })
    }

    /// Every registered pass, in the order a run reaches it.
    #[must_use]
    pub fn listing(&self) -> Vec<PassEntry> {
        self.ordered()
            .into_iter()
            .flat_map(Registration::entries)
            .collect()
    }

    /// Registrations in run order: by anchor, then by when they were added.
    fn ordered(&self) -> Vec<&Registration> {
        let mut ordered: Vec<&Registration> = self.registrations.iter().collect();
        ordered.sort_by_key(|registration| registration.anchor);
        ordered
    }

    /// Walks a circuit through the whole progression, running each anchor.
    ///
    /// # Errors
    ///
    /// Returns the reason the circuit could not be lowered, the graph could not
    /// be raised, or a pass could not produce a program its stage accepts.
    pub fn run(&self, circuit: &CircuitStore) -> Result<PipelineRun, PipelineError> {
        let mut report = RunReport::default();
        // One context for the whole run. Every pass reads the same level and
        // the same analysis manager, and an analysis one anchor computed is
        // still there at the next.
        let mut context = PassContext::new(self.level);
        let mut store = self.run_circuit_anchor(
            PassAnchor::CircuitBuilt,
            circuit.clone(),
            &mut context,
            &mut report,
        )?;

        let mut graph =
            circuit_to_graph(&store).map_err(|error| PipelineError::Lower(error.to_string()))?;
        for anchor in [PassAnchor::GraphLowered, PassAnchor::GraphRewritten] {
            graph = self.run_graph_anchor(anchor, graph, &mut context, &mut report)?;
        }

        store =
            graph_to_circuit(&graph).map_err(|error| PipelineError::Raise(error.to_string()))?;
        store =
            self.run_circuit_anchor(PassAnchor::CircuitRaised, store, &mut context, &mut report)?;

        let analyses_computed = context.analyses().computations();
        Ok(PipelineRun {
            circuit: store,
            graph,
            executed: report.executed,
            diagnostics: context.diagnostics,
            changed: report.changed,
            groups: report.groups,
            analyses_computed,
        })
    }

    fn run_circuit_anchor(
        &self,
        anchor: PassAnchor,
        mut store: CircuitStore,
        context: &mut PassContext,
        report: &mut RunReport,
    ) -> Result<CircuitStore, PipelineError> {
        for registration in self.ordered() {
            let Registered::Circuit(pass) = &registration.registered else {
                continue;
            };
            if registration.anchor != anchor {
                continue;
            }
            let produced = pass
                .run(store, context)
                .map_err(|reason| PipelineError::Failed {
                    name: pass.name().to_string(),
                    anchor: anchor.name(),
                    reason,
                })?;
            store = produced.program;
            if produced.changed {
                report.changed.push(pass.name().to_string());
            }
            report.executed.extend(registration.entries());
        }
        Ok(store)
    }

    fn run_graph_anchor(
        &self,
        anchor: PassAnchor,
        mut graph: Graph,
        context: &mut PassContext,
        report: &mut RunReport,
    ) -> Result<Graph, PipelineError> {
        for registration in self.ordered() {
            if registration.anchor != anchor {
                continue;
            }
            match &registration.registered {
                Registered::Graph(pass) => {
                    let entry = PassEntry {
                        anchor,
                        name: pass.name().to_string(),
                        built_in: registration.built_in,
                    };
                    graph = run_graph_pass(pass.as_ref(), graph, context, anchor, &entry, report)?;
                    report.executed.push(entry);
                }
                Registered::Group(group) => {
                    graph = run_group(group, graph, context, anchor, registration, report)?;
                }
                Registered::Circuit(_) => {}
            }
        }
        Ok(graph)
    }
}

fn run_group(
    group: &PassGroup,
    mut graph: Graph,
    context: &mut PassContext,
    anchor: PassAnchor,
    registration: &Registration,
    report: &mut RunReport,
) -> Result<Graph, PipelineError> {
    let mut iterations = 0;
    let mut settled = false;
    while iterations < group.max_iterations {
        iterations += 1;
        let mut moved = false;
        for pass in &group.passes {
            let entry = PassEntry {
                anchor,
                name: format!("{}/{}", group.name, pass.name()),
                built_in: registration.built_in,
            };
            let before = report.changed.len();
            graph = run_graph_pass(pass.as_ref(), graph, context, anchor, &entry, report)?;
            moved |= report.changed.len() > before;
            report.executed.push(entry);
        }
        if !moved {
            settled = true;
            break;
        }
    }
    report.groups.push(if settled {
        GroupOutcome::Converged {
            group: group.name.clone(),
            iterations,
        }
    } else {
        GroupOutcome::BoundExhausted {
            group: group.name.clone(),
            iterations,
        }
    });
    Ok(graph)
}

/// What a run has accumulated so far, so an anchor runner takes one parameter
/// for it rather than four.
#[derive(Default)]
struct RunReport {
    executed: Vec<PassEntry>,
    changed: Vec<String>,
    groups: Vec<GroupOutcome>,
}

/// Refuses a claim that is not one, at registration rather than at run time.
///
/// Three ways to say nothing. Preserving no observation is not a weaker claim,
/// it is the absence of one. Naming an empty corpus is worse than naming none,
/// because it reads as checked and is not, and so is naming a corpus nothing
/// runs. And certification is only worth what the driver checks: two
/// obligations defer the check elsewhere, so a pass resting on either has not
/// been certified here and cannot say it was.
fn check_claim(name: &str, claim: &PassClaim) -> Result<(), PipelineError> {
    if claim.preserves.is_empty() {
        return Err(PipelineError::EmptyClaim {
            name: name.to_string(),
        });
    }
    match &claim.tier {
        PassTier::Certified { obligations } => {
            if obligations.is_empty() {
                return Err(PipelineError::UnbackedCertification {
                    name: name.to_string(),
                });
            }
            if let Some(deferred) = obligations
                .iter()
                .find(|obligation| !obligation.checks_the_program())
            {
                return Err(PipelineError::UncheckedCertification {
                    name: name.to_string(),
                    obligation: deferred.name().to_string(),
                });
            }
        }
        PassTier::DirectMutation {
            corpus: Some(corpus),
        } => {
            if corpus.trim().is_empty() {
                return Err(PipelineError::UnnamedCorpus {
                    name: name.to_string(),
                });
            }
            if !DIFFERENTIAL_CORPORA.contains(&corpus.as_str()) {
                return Err(PipelineError::UnregisteredCorpus {
                    name: name.to_string(),
                    corpus: corpus.clone(),
                });
            }
        }
        PassTier::DirectMutation { corpus: None } => {}
    }
    Ok(())
}

fn run_graph_pass(
    pass: &dyn GraphPass,
    graph: Graph,
    context: &mut PassContext,
    anchor: PassAnchor,
    entry: &PassEntry,
    report: &mut RunReport,
) -> Result<Graph, PipelineError> {
    let failed = |reason: String| PipelineError::Failed {
        name: entry.name.clone(),
        anchor: anchor.name(),
        reason,
    };
    let produced = pass.run(graph, context).map_err(failed)?;
    if produced.changed {
        report.changed.push(entry.name.clone());
    }
    // Every pass leaves a graph the graph's own rules accept, so a later pass
    // never has to guess whether what it was handed is well formed.
    let diagnostics = verify(&produced.program);
    if !diagnostics.is_empty() {
        return Err(failed(
            diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code.as_str(), diagnostic.message))
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    Ok(produced.program)
}

/// The inverse relation this compiler is willing to act on.
///
/// The list itself is in the shared vocabulary, beside the gate names it makes
/// a claim about, so that the claim and the names it refers to cannot drift
/// apart. What is here is the decision to act on it.
#[must_use]
pub fn circuit_inverses() -> InverseVocabulary {
    InverseVocabulary::self_inverse(SELF_INVERSE)
}

/// The contract each circuit operation resolves to, so the driver can read what
/// an operation means without the matching rule telling it.
///
/// Derived from the two sides rather than written as a third table: the names
/// come from the circuit vocabulary and the identities are kept only when the
/// registry has a contract under them. A gate the registry does not name is
/// left out, resolves to nothing, and is therefore not declared anything, which
/// is the safe direction for every claim built on this.
///
/// The five class contracts are named here because they are the ones that keep
/// a shared identity rather than gaining a per-gate sibling. Preparation is
/// absent: it is not unitary and the registry has no contract for it.
#[must_use]
pub fn circuit_contract_identities() -> BTreeMap<String, String> {
    let registry = Registry::builtins();
    let mut identities: BTreeMap<String, String> = gate_names()
        .filter_map(|name| {
            let identity = gate_identity(name.strip_prefix(QUANTUM)?);
            registry.get(&identity)?;
            Some((name.to_string(), identity))
        })
        .collect();
    for (operation, identity) in [
        (ALLOCATE, "qdk.quantum.allocate"),
        (RESET, "qdk.quantum.reset"),
        (RELEASE, "qdk.quantum.release"),
        ("quantum.measure_x", "qdk.quantum.measure"),
        ("quantum.measure_y", "qdk.quantum.measure"),
        ("quantum.measure_z", "qdk.quantum.measure"),
    ] {
        identities.insert(operation.to_string(), identity.to_string());
    }
    identities
}

/// The operations this compiler declares diagonal in the computational basis.
///
/// Resolved from the registry here rather than listed, so the declaration is
/// made once where operation meaning lives. The rewrite rule needs the answer
/// under the graph's own names because a rule never sees a discharge context;
/// the discharge repeats the lookup for itself, and the two disagreeing is
/// exactly what the discharge exists to catch.
#[must_use]
pub fn circuit_diagonal_operations() -> BTreeSet<String> {
    let registry = Registry::builtins();
    circuit_contract_identities()
        .into_iter()
        .filter(|(_, identity)| {
            registry
                .get(identity)
                .is_some_and(|contract| contract.computational_basis_diagonal)
        })
        .map(|(operation, _)| operation)
        .collect()
}

/// The corpora a pass may declare it is checked against.
///
/// Two names, each paired with the corpus of the same name in the Python
/// semantics gate at `source/qdk_package/tests/test_pass_semantics.py`. The two
/// sides agree by a shared literal and nothing checks the agreement
/// automatically, because the oracle is Python and this side is Rust.
///
/// They are separate names rather than one because they compare different
/// things. `circuit-pipeline` is an exact, global-phase-sensitive comparison of
/// a whole-program unitary, and it reaches no program that measures.
/// `circuit-pipeline-measured` compares the whole post-run state, dephased in
/// the computational basis wherever the program measured, and cannot see a
/// dropped global phase. A pass declaring one has not been given the other.
pub const DIFFERENTIAL_CORPORA: [&str; 2] = ["circuit-pipeline", "circuit-pipeline-measured"];

/// The context this compiler's own passes are discharged against.
///
/// One constructor rather than one per pass, so what the driver can check does
/// not depend on which pass happened to build the context.
fn circuit_discharge() -> DischargeContext {
    DischargeContext::new(circuit_inverses())
        .with_corpora(DIFFERENTIAL_CORPORA)
        .with_contracts(Registry::builtins())
        .with_operation_identities(circuit_contract_identities())
}

/// Cancels adjacent pairs of the same self-inverse operation.
///
/// This is the compiler's own pass, written against the same registration the
/// binding uses. It is not expressible through the bound contract, which
/// matches on an operation name alone: whether a pair cancels depends on what
/// sits between the two nodes and on which qubits each one holds.
pub struct CancelInversePairs;

impl GraphPass for CancelInversePairs {
    fn name(&self) -> &'static str {
        "cancel-inverse-pairs"
    }

    fn claim(&self) -> PassClaim {
        PassClaim::certified([ProofObligation::InverseAnnihilation])
    }

    fn run(&self, graph: Graph, context: &mut PassContext) -> Result<PassOutput<Graph>, String> {
        if context.level() == OptimizationLevel::None {
            return Ok(PassOutput::unchanged(graph));
        }
        let mut working = graph;
        let outcome = cancel_inverse_pairs(&mut working)?;
        if outcome == Cancellation::NoFacts {
            context.raise(self.name(), "no operation in this graph is self-inverse");
        }
        Ok(PassOutput {
            program: working,
            changed: outcome == Cancellation::Changed,
        })
    }
}

/// What cancelling adjacent inverse pairs did to a graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cancellation {
    /// Nothing in the graph is self-inverse, or the graph could not be read.
    NoFacts,
    Unchanged,
    Changed,
}

/// Cancels adjacent pairs of the same self-inverse operation, in place.
///
/// Shared rather than duplicated: the RIR stage and the circuit pass reach the
/// rule through this, so they cannot disagree about what cancels or about which
/// facts the driver was given.
///
/// # Errors
///
/// Returns the driver's own message when a patch is refused.
pub(crate) fn cancel_inverse_pairs(graph: &mut Graph) -> Result<Cancellation, String> {
    let facts = inverse_facts(graph);
    if facts.is_empty() {
        return Ok(Cancellation::NoFacts);
    }
    let rewrite = RewriteContext {
        operation_facts: facts,
        ..RewriteContext::default()
    };
    let budget = graph.nodes().count().max(1);
    let outcome = GraphRewriteDriver::new(vec![Box::new(InverseCancellation)], budget)
        .run(graph, &rewrite, &mut circuit_discharge())
        .map_err(|error| error.to_string())?;
    Ok(if matches!(outcome, GraphRewriteOutcome::Unchanged) {
        Cancellation::Unchanged
    } else {
        Cancellation::Changed
    })
}

/// Retires a gate that is diagonal in the computational basis when every
/// operand it holds runs straight into a Z-basis measurement or a reset.
///
/// The first pass in this compiler that reads a gate-semantics predicate. The
/// predicate is bound here, from the contract registry, because a rewrite rule
/// is never handed a discharge context. The driver looks the same declaration
/// up for itself when it discharges, so the two disagreeing produces a refused
/// patch rather than a wrong program.
pub struct RetireDiagonalBeforeZCollapse;

impl GraphPass for RetireDiagonalBeforeZCollapse {
    fn name(&self) -> &'static str {
        "retire-diagonal-before-z-collapse"
    }

    fn claim(&self) -> PassClaim {
        PassClaim::certified([ProofObligation::DiagonalBeforeZCollapse])
    }

    fn run(&self, graph: Graph, context: &mut PassContext) -> Result<PassOutput<Graph>, String> {
        if context.level() == OptimizationLevel::None {
            return Ok(PassOutput::unchanged(graph));
        }
        // No diagnostic where the cancellation pass raises one. A graph holding
        // nothing that collapses the computational basis is the ordinary case,
        // not something a caller wants reported once per run.
        let budget = graph.nodes().count().max(1);
        let mut working = graph;
        let rule = DiagonalCollapse::new(circuit_diagonal_operations());
        let outcome = GraphRewriteDriver::new(vec![Box::new(rule)], budget)
            .run(
                &mut working,
                &RewriteContext::default(),
                &mut circuit_discharge(),
            )
            .map_err(|error| error.to_string())?;
        Ok(PassOutput {
            program: working,
            changed: !matches!(outcome, GraphRewriteOutcome::Unchanged),
        })
    }
}

/// What the cancellation rule needs to know about each node it may match.
///
/// A graph this cannot read is left with no facts rather than refused, because
/// a pass registered earlier is free to produce a program outside the circuit
/// vocabulary and that is not this pass's failure.
///
/// Operands go through the shared identity surface rather than straight into a
/// resource set. A lowered circuit resolves all of them, so nothing about this
/// path changes; what the detour buys is that the next producer, whose operands
/// are bindings rather than indices, reaches the driver through the same
/// surface and declines where it cannot prove identity instead of inventing a
/// resource number.
fn inverse_facts(graph: &Graph) -> BTreeMap<StableNodeId, OperationFacts> {
    let Ok(operands) = qubit_operands(graph) else {
        return BTreeMap::new();
    };
    operands
        .into_iter()
        .filter_map(|(id, qubits)| {
            let operation = &graph.node_by_stable_id(id)?.operation.0;
            if !SELF_INVERSE.contains(&operation.as_str()) {
                return None;
            }
            let wires = OperandWires::new(
                qubits
                    .into_iter()
                    .map(|qubit| Operand::Resource(ResourceId(qubit.into()))),
            );
            Some((
                id,
                OperationFacts {
                    class: OperationClass::Unitary,
                    resources: wires.resolved()?,
                    inverse_of: Some(operation.clone()),
                    global_phase: GlobalPhasePolicy::Exact,
                    ..OperationFacts::default()
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests;
