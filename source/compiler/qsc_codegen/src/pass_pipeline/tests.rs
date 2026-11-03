// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the pass pipeline.
//!
//! Two things are pinned here. That every anchor is a place a pass can actually
//! run, shown by registering one at each and watching its change reach the
//! output, which is the expectation the anchor set could not claim for itself.
//! And that the order a run takes is the order the listing declares, because an
//! extension point whose order is not the published one is not one.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use qsc_circuit_store::{
    CircuitStore, Clbit, ControlFlow, Expr, IfElse, PackedOperation, ParamValue, Qubit,
    StandardGate,
};
use qsc_fir::fir::{ItemKind, StoreItemId};
use qsc_fir_transforms::run_pipeline_with_diagnostics_and_optimize;
use qsc_fir_transforms::test_utils::{assert_no_pipeline_errors, compile_to_fir};
use qsc_qhdg::{
    Adjacency, AnalysisContext, AnalysisScope, Demand, Graph, Observation, PortKind,
    ProofObligation, StableNodeId,
};

use super::{
    CancelInversePairs, CircuitPass, GraphPass, GroupOutcome, OptimizationLevel, PassClaim,
    PassContext, PassEntry, PassOutput, PassPipeline, PassTier, PipelineError,
    RetireDiagonalBeforeZCollapse,
};
use crate::circuit_qasm::circuit_to_qasm;
use crate::circuit_qhdg::{ReadItem, circuit_to_graph, read_program};
use crate::graph_pass::{PassAction, apply_patches, find_matches};
use crate::pass_anchor::PassAnchor;

fn circuit(gates: &[StandardGate]) -> CircuitStore {
    let mut store = CircuitStore::new(1, 0);
    for standard in gates {
        store
            .push_standard(*standard, &[Qubit(0)])
            .expect("gate should be accepted");
    }
    store
}

/// A circuit holding an adjacent inverse pair, so the built-in has something to
/// do, and one operation that survives it.
fn cancellable() -> CircuitStore {
    circuit(&[StandardGate::H, StandardGate::H, StandardGate::X])
}

fn operations(graph: &Graph) -> Vec<String> {
    read_program(graph)
        .expect("graph should read")
        .items
        .into_iter()
        .filter_map(|item| match item {
            ReadItem::Instruction(instruction) => Some(instruction.operation),
            ReadItem::Conditional(_) | ReadItem::Loop(_) => None,
        })
        .collect()
}

fn exported(store: &CircuitStore) -> String {
    circuit_to_qasm(store).expect("circuit should export")
}

fn names(entries: &[PassEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

/// A two-qubit circuit holding one gate twice over the operands given.
fn paired(standard: StandardGate, first: &[u32], second: &[u32]) -> CircuitStore {
    let mut store = CircuitStore::new(3, 0);
    for operands in [first, second] {
        let qubits: Vec<Qubit> = operands.iter().map(|index| Qubit(*index)).collect();
        store
            .push_standard(standard, &qubits)
            .expect("gate should be accepted");
    }
    store
}

/// Appends one gate, which is the smallest change visible in the output.
struct AppendGate {
    name: &'static str,
    standard: StandardGate,
}

impl CircuitPass for AppendGate {
    fn name(&self) -> &str {
        self.name
    }

    fn claim(&self) -> PassClaim {
        PassClaim::unchecked()
    }

    fn run(
        &self,
        mut circuit: CircuitStore,
        _context: &mut PassContext,
    ) -> Result<PassOutput<CircuitStore>, String> {
        circuit
            .push_standard(self.standard, &[Qubit(0)])
            .map_err(|error| format!("{error:?}"))?;
        Ok(PassOutput::changed(circuit))
    }
}

/// Renames every node carrying one operation, through the same contract the
/// binding gives a caller.
struct RenameOperation {
    name: &'static str,
    from: &'static str,
    to: &'static str,
}

impl GraphPass for RenameOperation {
    fn name(&self) -> &str {
        self.name
    }

    fn claim(&self) -> PassClaim {
        PassClaim::unchecked()
    }

    fn run(&self, graph: Graph, _context: &mut PassContext) -> Result<PassOutput<Graph>, String> {
        let decisions: BTreeMap<StableNodeId, PassAction> =
            find_matches(&graph, &[self.from.to_string()].into_iter().collect())
                .into_iter()
                .map(|matched| (matched.node, PassAction::Replace(self.to.to_string())))
                .collect();
        let changed = !decisions.is_empty();
        let budget = decisions.len().max(1);
        apply_patches(&graph, &decisions, budget)
            .map(|(rewritten, _)| PassOutput {
                program: rewritten,
                changed,
            })
            .map_err(|error| error.to_string())
    }
}

/// Records what it was handed and changes nothing.
struct Observer {
    seen: Arc<Mutex<Vec<String>>>,
}

impl GraphPass for Observer {
    fn name(&self) -> &'static str {
        "observe"
    }

    fn claim(&self) -> PassClaim {
        PassClaim::certified([ProofObligation::StructuralOnly])
    }

    fn run(&self, graph: Graph, _context: &mut PassContext) -> Result<PassOutput<Graph>, String> {
        self.seen
            .lock()
            .expect("no other thread holds this")
            .extend(operations(&graph));
        Ok(PassOutput::unchanged(graph))
    }
}

struct AlwaysFails;

impl GraphPass for AlwaysFails {
    fn name(&self) -> &'static str {
        "always-fails"
    }

    fn claim(&self) -> PassClaim {
        PassClaim::certified([ProofObligation::StructuralOnly])
    }

    fn run(&self, _: Graph, _context: &mut PassContext) -> Result<PassOutput<Graph>, String> {
        Err("nothing here works".to_string())
    }
}

/// Reads an analysis through the context and records what the manager had
/// computed by the time it ran, which is how cache reuse is observed from
/// outside the cache.
struct ReadsDemand {
    name: &'static str,
    seen: Arc<Mutex<Vec<(OptimizationLevel, usize)>>>,
}

impl GraphPass for ReadsDemand {
    fn name(&self) -> &str {
        self.name
    }

    fn claim(&self) -> PassClaim {
        PassClaim::certified([ProofObligation::StructuralOnly])
    }

    fn run(&self, graph: Graph, context: &mut PassContext) -> Result<PassOutput<Graph>, String> {
        let adjacency = Adjacency::new(&graph);
        let level = context.level();
        context
            .analyses()
            .get::<Demand>(
                &AnalysisContext::new(&graph, &adjacency),
                AnalysisScope::Graph,
            )
            .map_err(|error| error.to_string())?;
        let computed = context.analyses().computations();
        self.seen
            .lock()
            .expect("no other thread holds this")
            .push((level, computed));
        Ok(PassOutput::unchanged(graph))
    }
}

#[test]
fn two_passes_read_the_same_level_and_the_same_analysis_manager() {
    // The whole point of the context. Neither pass was constructed with a
    // level, and neither built its own cache, so agreeing about both is a
    // property of the pipeline rather than of how they were made.
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut pipeline = PassPipeline::empty();
    pipeline.set_optimization_level(OptimizationLevel::Aggressive);
    for (anchor, name) in [
        (PassAnchor::GraphLowered, "first"),
        (PassAnchor::GraphRewritten, "second"),
    ] {
        pipeline
            .add_graph_pass(
                anchor,
                Box::new(ReadsDemand {
                    name,
                    seen: Arc::clone(&seen),
                }),
                false,
            )
            .expect("a graph pass belongs at a graph anchor");
    }

    let run = pipeline
        .run(&cancellable())
        .expect("the run should complete");
    let observed = seen.lock().expect("no other thread holds this").clone();
    assert_eq!(
        observed,
        vec![
            (OptimizationLevel::Aggressive, 1),
            (OptimizationLevel::Aggressive, 1)
        ],
        "both passes read the level the pipeline was set to, and the second was served the analysis the first computed"
    );
    assert_eq!(
        run.analyses_computed, 1,
        "one manager serves the whole run, so an anchor boundary is not a cache boundary"
    );
    assert!(run.changed.is_empty(), "neither pass edited anything");
}

#[test]
fn a_pass_returning_a_fresh_equal_revision_graph_recomputes_demand() {
    struct ReplaceGraph;
    impl GraphPass for ReplaceGraph {
        fn name(&self) -> &'static str {
            "replace-graph"
        }
        fn claim(&self) -> PassClaim {
            PassClaim::unchecked()
        }
        fn run(
            &self,
            graph: Graph,
            context: &mut PassContext,
        ) -> Result<PassOutput<Graph>, String> {
            let adjacency = Adjacency::new(&graph);
            let original = context
                .analyses()
                .get::<Demand>(
                    &AnalysisContext::new(&graph, &adjacency),
                    AnalysisScope::Graph,
                )
                .map_err(|error| error.to_string())?;
            let replacement = circuit_to_graph(&circuit(&[StandardGate::X]))
                .map_err(|error| error.to_string())?;
            assert_eq!(graph.revision(), replacement.revision());
            let adjacency = Adjacency::new(&replacement);
            let uncached = <Demand as qsc_qhdg::Analysis>::compute(
                &AnalysisContext::new(&replacement, &adjacency),
                &AnalysisScope::Graph,
            )
            .map_err(|error| error.to_string())?;
            assert_ne!(
                original, uncached,
                "replacement has different demanded nodes"
            );
            let actual = context
                .analyses()
                .get::<Demand>(
                    &AnalysisContext::new(&replacement, &adjacency),
                    AnalysisScope::Graph,
                )
                .map_err(|error| error.to_string())?;
            assert_eq!(actual, uncached);
            assert_eq!(context.analyses().computations(), 2);
            Ok(PassOutput::changed(replacement))
        }
    }
    let mut pipeline = PassPipeline::empty();
    pipeline
        .add_graph_pass(PassAnchor::GraphLowered, Box::new(ReplaceGraph), true)
        .expect("explicit unchecked pass");
    let run = pipeline.run(&cancellable()).expect("replacement run");
    assert_eq!(run.analyses_computed, 2);
    assert_eq!(
        exported(&run.circuit),
        exported(&circuit(&[StandardGate::X]))
    );
}

#[test]
fn a_pass_that_changes_nothing_is_distinguishable_from_one_that_does() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut pipeline = PassPipeline::empty();
    pipeline
        .add_graph_pass(
            PassAnchor::GraphLowered,
            Box::new(Observer {
                seen: Arc::clone(&seen),
            }),
            false,
        )
        .expect("a graph pass belongs at a graph anchor");
    pipeline
        .add_graph_pass(
            PassAnchor::GraphRewritten,
            Box::new(RenameOperation {
                name: "rename",
                from: "quantum.x",
                to: "quantum.y",
            }),
            false,
        )
        .expect("a graph pass belongs at a graph anchor");

    let run = pipeline
        .run(&cancellable())
        .expect("the run should complete");
    assert_eq!(
        run.changed,
        vec!["rename".to_string()],
        "the report names the pass that edited, and the programs are never compared to find out"
    );
    assert_eq!(names(&run.executed), ["observe", "rename"]);
}

#[test]
fn a_pass_raises_a_diagnostic_that_reaches_the_caller() {
    // The built-in cancellation pass has nothing to do on a circuit holding no
    // self-inverse operation, and says so rather than reporting failure.
    let run = PassPipeline::with_built_ins()
        .run(&circuit(&[StandardGate::T, StandardGate::T]))
        .expect("the run should complete");
    assert_eq!(
        run.diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.pass.as_str(), diagnostic.message.as_str()))
            .collect::<Vec<_>>(),
        [(
            "cancel-inverse-pairs",
            "no operation in this graph is self-inverse"
        )]
    );
    assert!(run.changed.is_empty());
}

#[test]
fn the_optimization_level_reaches_a_built_in_pass() {
    let mut pipeline = PassPipeline::with_built_ins();
    pipeline.set_optimization_level(OptimizationLevel::None);
    let declined = pipeline
        .run(&cancellable())
        .expect("the run should complete");
    assert!(
        declined.changed.is_empty(),
        "at level none the pass declines rather than cancelling"
    );
    assert_eq!(operations(&declined.graph).len(), 3);

    let run = PassPipeline::with_built_ins()
        .run(&cancellable())
        .expect("the run should complete");
    assert_eq!(run.changed, vec!["cancel-inverse-pairs".to_string()]);
    assert_eq!(operations(&run.graph).len(), 1);
}

#[test]
fn the_new_origin_reaches_the_cancellation_pass_with_facts_rather_than_with_none() {
    // `inverse_facts` returns an empty map for a graph it cannot read, and the
    // pass treats that as nothing to do and reports success. So a new origin
    // the reader mishandled would pass every other check in this file while
    // being silently invisible to every rewrite. This asserts the map is
    // populated, which is the only thing that distinguishes the two.
    let graph = crate::rir_qhdg::program_to_graph(&qsc_rir::builder::bell_program())
        .expect("the program should lower");
    let facts = super::inverse_facts(&graph);

    let named: BTreeSet<&str> = facts
        .keys()
        .filter_map(|id| graph.node_by_stable_id(*id))
        .map(|node| node.operation.0.as_str())
        .collect();
    assert_eq!(
        named,
        BTreeSet::from(["quantum.cx", "quantum.h"]),
        "the self-inverse gates this program holds are the ones with facts"
    );
    assert!(
        facts.values().all(|fact| !fact.resources.is_empty()),
        "operands resolved to resources rather than declining, so a rewrite can compare them"
    );
}

#[test]
fn the_fir_origin_reaches_the_cancellation_pass_with_facts_rather_than_with_none() {
    // The same hazard, asked of the third origin. An empty map is not this
    // pass's failure, so a FIR-origin graph the reader mishandled would pass
    // every other check in this file while being invisible to every rewrite.
    let source = r"
        @EntryPoint()
        operation Main() : Unit {
            use a = Qubit();
            use b = Qubit();
            H(a);
            CNOT(a, b);
            T(b);
        }
        ";
    let (mut store, package) = compile_to_fir(source);
    let mut graph = None;
    let result =
        run_pipeline_with_diagnostics_and_optimize(&mut store, package, |store, package| {
            let item = store
                .get(package)
                .items
                .iter()
                .find_map(|(id, item)| match &item.kind {
                    ItemKind::Callable(decl) if &*decl.name.name == "Main" => Some(id),
                    ItemKind::Callable(_) | ItemKind::Ty(..) => None,
                })
                .expect("the pipeline should still hold the entry point");
            graph = Some(
                crate::fir_qhdg::callable_to_graph(store, StoreItemId { package, item })
                    .expect("the body should lower"),
            );
        });
    assert_no_pipeline_errors("pass_pipeline::the_fir_origin", &result.errors);
    let graph = graph.expect("the optimization point should have been reached");
    let facts = super::inverse_facts(&graph);

    let named: BTreeSet<&str> = facts
        .keys()
        .filter_map(|id| graph.node_by_stable_id(*id))
        .map(|node| node.operation.0.as_str())
        .collect();
    assert_eq!(
        named,
        BTreeSet::from(["quantum.cx", "quantum.h"]),
        "the self-inverse gates this body holds are the ones with facts, and T is not one"
    );
    assert!(
        facts.values().all(|fact| !fact.resources.is_empty()),
        "operands resolved to resources rather than declining, so a rewrite can compare them"
    );
}

/// Renames one operation to another and reports whether it found any, so two of
/// these facing opposite ways either converge or fight forever depending on the
/// names they are given.
fn rename(name: &'static str, from: &'static str, to: &'static str) -> Box<dyn GraphPass> {
    Box::new(RenameOperation { name, from, to })
}

#[test]
fn a_declaration_that_is_not_a_claim_is_refused_at_registration() {
    // Refused where it is written rather than where it would have mattered.
    // Four ways to say nothing: preserving no observation is the absence of a
    // claim rather than a weaker one; naming an empty corpus is worse than
    // naming none, because it reads as checked and is not, and so is naming a
    // corpus nothing runs; and certification resting on an obligation the
    // driver does not check is a deferral wearing a proof's name.
    struct Declares(PassClaim);

    impl GraphPass for Declares {
        fn name(&self) -> &'static str {
            "declares"
        }

        fn claim(&self) -> PassClaim {
            self.0.clone()
        }

        fn run(
            &self,
            graph: Graph,
            _context: &mut PassContext,
        ) -> Result<PassOutput<Graph>, String> {
            Ok(PassOutput::unchanged(graph))
        }
    }

    let mut pipeline = PassPipeline::empty();
    let refuse = |pipeline: &mut PassPipeline, claim: PassClaim| {
        pipeline.add_graph_pass(PassAnchor::GraphRewritten, Box::new(Declares(claim)), false)
    };

    assert_eq!(
        refuse(
            &mut pipeline,
            PassClaim {
                preserves: BTreeSet::new(),
                tier: PassTier::Certified {
                    obligations: BTreeSet::from([ProofObligation::StructuralOnly])
                },
            }
        ),
        Err(PipelineError::EmptyClaim {
            name: "declares".to_string()
        })
    );
    assert_eq!(
        refuse(
            &mut pipeline,
            PassClaim {
                preserves: Observation::all(),
                tier: PassTier::DirectMutation {
                    corpus: Some("  ".to_string())
                },
            }
        ),
        Err(PipelineError::UnnamedCorpus {
            name: "declares".to_string()
        })
    );
    assert_eq!(
        refuse(
            &mut pipeline,
            PassClaim {
                preserves: Observation::all(),
                tier: PassTier::DirectMutation {
                    corpus: Some("corpus-nobody-runs".to_string())
                },
            }
        ),
        Err(PipelineError::UnregisteredCorpus {
            name: "declares".to_string(),
            corpus: "corpus-nobody-runs".to_string()
        }),
        "a corpus name reads as checked, so one nothing runs is refused where it is written"
    );
    assert_eq!(
        refuse(&mut pipeline, PassClaim::certified([])),
        Err(PipelineError::UnbackedCertification {
            name: "declares".to_string()
        }),
        "certification by no obligation at all is the hole the tier exists to close"
    );
    for deferred in [
        ProofObligation::Assumed,
        ProofObligation::DifferentialCorpus {
            corpus: "circuit-pipeline".to_string(),
        },
    ] {
        assert_eq!(
            refuse(
                &mut pipeline,
                PassClaim::certified([ProofObligation::InverseAnnihilation, deferred.clone()])
            ),
            Err(PipelineError::UncheckedCertification {
                name: "declares".to_string(),
                obligation: deferred.name().to_string()
            }),
            "an obligation the driver defers cannot back a certified claim, even beside one it discharges"
        );
    }
    assert!(pipeline.listing().is_empty());

    pipeline
        .add_graph_pass(
            PassAnchor::GraphRewritten,
            Box::new(Declares(PassClaim::unchecked())),
            false,
        )
        .expect("a pass that says what it preserves is registrable");
    assert_eq!(names(&pipeline.listing()), ["declares"]);
}

#[test]
fn every_built_in_pass_declares_what_it_preserves() {
    // The declaration is required rather than defaulted, so a pass cannot be
    // written without deciding what it claims, and cannot claim certification
    // without naming what certified it.
    let pipeline = PassPipeline::with_built_ins();
    assert_eq!(
        names(&pipeline.listing()),
        ["cancel-inverse-pairs", "retire-diagonal-before-z-collapse"]
    );
    assert_eq!(
        CancelInversePairs.claim(),
        PassClaim::certified([ProofObligation::InverseAnnihilation]),
        "the compiler's own pass has every edit discharged by the driver"
    );
    assert_eq!(CancelInversePairs.claim().preserves, Observation::all());
    assert_eq!(
        RetireDiagonalBeforeZCollapse.claim(),
        PassClaim::certified([ProofObligation::DiagonalBeforeZCollapse]),
        "and so does the one that reads a gate-semantics predicate"
    );
}

#[test]
fn a_circuit_operation_resolves_to_the_contract_carrying_its_semantics() {
    // Nothing in a circuit graph resolved to a contract before this: node
    // operations are `quantum.` and registry identities are `qdk.`.
    let context = super::circuit_discharge();
    let registry = context
        .contracts()
        .expect("the context production builds carries the contracts");

    let rz = context.identity_of("quantum.rz").expect("rz resolves");
    assert_eq!(rz, "qdk.quantum.rz");
    assert!(
        registry
            .get(rz)
            .expect("contract")
            .computational_basis_diagonal
    );
    let h = context.identity_of("quantum.h").expect("h resolves");
    assert!(
        !registry
            .get(h)
            .expect("contract")
            .computational_basis_diagonal
    );

    // Three measurements share one contract, which is why this is a table and
    // not a prefix rule.
    assert_eq!(
        context.identity_of("quantum.measure_z"),
        Some("qdk.quantum.measure")
    );
    assert_eq!(
        context.identity_of("quantum.measure_x"),
        Some("qdk.quantum.measure")
    );

    // An operation the table does not name resolves to nothing rather than to a
    // default, so nothing is declared about it and every claim built on the
    // lookup declines.
    assert_eq!(context.identity_of("quantum.prepare_z"), None);
    assert_eq!(context.identity_of("quantum.barrier"), None);
    assert_eq!(context.identity_of("vendor.mystery"), None);
}

#[test]
fn every_gate_the_vocabulary_emits_has_a_contract_or_is_deliberately_absent() {
    // The comparison neither side can make alone: the circuit vocabulary lives
    // in this crate and the contracts live in the registry, and a gate added to
    // one and not the other would otherwise just stop being declared anything.
    let identities = super::circuit_contract_identities();
    let unmapped: Vec<&str> = crate::circuit_qhdg::gate_names()
        .filter(|name| !identities.contains_key(*name))
        .collect();
    assert_eq!(
        unmapped,
        [
            "quantum.prepare_x",
            "quantum.prepare_y",
            "quantum.prepare_z"
        ],
        "preparation is the only part of the vocabulary the registry does not name"
    );
}

#[test]
fn the_declared_diagonal_operations_are_named_under_the_graphs_own_vocabulary() {
    // The rule reads these names, and the graph holds `quantum.` names, so a
    // set carrying `qdk.` identities would match nothing and decline silently.
    let diagonal = super::circuit_diagonal_operations();
    assert_eq!(
        diagonal.iter().map(String::as_str).collect::<Vec<_>>(),
        [
            "quantum.crz",
            "quantum.cz",
            "quantum.i",
            "quantum.rz",
            "quantum.rzz",
            "quantum.s",
            "quantum.s_adj",
            "quantum.t",
            "quantum.t_adj",
            "quantum.z",
        ]
    );
    assert!(
        !diagonal.contains("quantum.measure_z") && !diagonal.contains("quantum.reset"),
        "a sink is not a diagonal gate, and confusing the two is the whole hazard"
    );
}

#[test]
fn a_group_of_alternating_passes_reaches_a_fixed_point_and_stops() {
    // The first member has work on the first iteration and the second has work
    // after it. Neither has work on the second iteration, which is what stops
    // the group.
    let mut pipeline = PassPipeline::empty();
    pipeline
        .add_graph_group(
            PassAnchor::GraphRewritten,
            "normalize",
            vec![
                rename("x-to-y", "quantum.x", "quantum.y"),
                rename("y-to-z", "quantum.y", "quantum.z"),
            ],
            8,
        )
        .expect("a graph group belongs at a graph anchor");

    let run = pipeline
        .run(&cancellable())
        .expect("the run should complete");
    assert_eq!(
        run.groups,
        vec![GroupOutcome::Converged {
            group: "normalize".to_string(),
            iterations: 2
        }],
        "one iteration to do the work and one to find there is none left"
    );
    assert_eq!(
        run.changed,
        vec![
            "normalize/x-to-y".to_string(),
            "normalize/y-to-z".to_string()
        ],
        "each member reports for itself"
    );
    assert!(operations(&run.graph).contains(&"quantum.z".to_string()));
}

#[test]
fn a_group_that_oscillates_stops_at_its_bound_with_a_named_outcome() {
    // Two passes that undo each other. There is no fixed point, so the only
    // honest stopping condition is the bound, and saying so is the whole
    // difference between this and a hang.
    let mut pipeline = PassPipeline::empty();
    pipeline
        .add_graph_group(
            PassAnchor::GraphRewritten,
            "fight",
            vec![
                rename("there", "quantum.x", "quantum.y"),
                rename("back", "quantum.y", "quantum.x"),
            ],
            3,
        )
        .expect("a graph group belongs at a graph anchor");

    let run = pipeline
        .run(&cancellable())
        .expect("the run should complete");
    assert_eq!(
        run.groups,
        vec![GroupOutcome::BoundExhausted {
            group: "fight".to_string(),
            iterations: 3
        }],
        "a group that never settles is distinguishable from one that did"
    );
    assert_eq!(run.executed.len(), 6, "three iterations of two passes");
}

#[test]
fn a_group_with_nothing_to_do_runs_each_member_once() {
    let mut pipeline = PassPipeline::empty();
    pipeline
        .add_graph_group(
            PassAnchor::GraphRewritten,
            "idle",
            vec![
                rename("a", "quantum.absent", "quantum.other"),
                rename("b", "quantum.missing", "quantum.other"),
            ],
            8,
        )
        .expect("a graph group belongs at a graph anchor");

    let run = pipeline
        .run(&cancellable())
        .expect("the run should complete");
    assert_eq!(
        run.groups,
        vec![GroupOutcome::Converged {
            group: "idle".to_string(),
            iterations: 1
        }]
    );
    assert_eq!(names(&run.executed), ["idle/a", "idle/b"]);
    assert!(run.changed.is_empty());
}

#[test]
fn a_group_needs_members_and_iterations_to_be_a_group() {
    let mut pipeline = PassPipeline::empty();
    assert_eq!(
        pipeline.add_graph_group(PassAnchor::GraphRewritten, "empty", Vec::new(), 8),
        Err(PipelineError::EmptyGroup {
            name: "empty".to_string()
        })
    );
    assert_eq!(
        pipeline.add_graph_group(
            PassAnchor::GraphRewritten,
            "never",
            vec![rename("a", "quantum.x", "quantum.y")],
            0
        ),
        Err(PipelineError::EmptyGroup {
            name: "never".to_string()
        })
    );
    assert_eq!(
        pipeline.add_graph_group(
            PassAnchor::CircuitBuilt,
            "wrong-stage",
            vec![rename("a", "quantum.x", "quantum.y")],
            8
        ),
        Err(PipelineError::Stage {
            anchor: "circuit_built",
            expected: "circuit",
            given: "graph",
        })
    );
    assert!(pipeline.listing().is_empty());
}

#[test]
fn a_pass_can_only_be_registered_where_its_program_exists() {
    let mut pipeline = PassPipeline::empty();
    assert_eq!(
        pipeline.add_graph_pass(PassAnchor::CircuitBuilt, Box::new(AlwaysFails), false),
        Err(PipelineError::Stage {
            anchor: "circuit_built",
            expected: "circuit",
            given: "graph",
        })
    );
    assert!(
        pipeline
            .add_circuit_pass(
                PassAnchor::GraphLowered,
                Box::new(AppendGate {
                    name: "append",
                    standard: StandardGate::Z,
                }),
                false,
            )
            .is_err()
    );
    assert!(pipeline.listing().is_empty(), "nothing refused was kept");
}

#[test]
fn the_compilers_own_pass_is_listed_at_a_named_anchor() {
    let listing = PassPipeline::with_built_ins().listing();
    assert_eq!(
        names(&listing),
        [
            CancelInversePairs.name(),
            RetireDiagonalBeforeZCollapse.name()
        ]
    );
    assert!(
        listing
            .iter()
            .all(|entry| entry.anchor == PassAnchor::GraphRewritten && entry.built_in)
    );
}

#[test]
fn the_compilers_own_pass_changes_the_program() {
    let run = PassPipeline::with_built_ins()
        .run(&cancellable())
        .expect("pipeline should run");
    assert_eq!(operations(&run.graph), ["quantum.x"]);
    assert_eq!(
        names(&run.executed),
        ["cancel-inverse-pairs", "retire-diagonal-before-z-collapse"]
    );
    assert_eq!(
        run.changed,
        vec!["cancel-inverse-pairs".to_string()],
        "nothing here collapses a basis, so the second built-in declines"
    );

    let untouched = PassPipeline::empty()
        .run(&cancellable())
        .expect("pipeline should run");
    assert_eq!(
        operations(&untouched.graph),
        ["quantum.h", "quantum.h", "quantum.x"],
        "the pair survives when the pass is not registered"
    );
    assert!(untouched.executed.is_empty());
}

/// A one-qubit circuit measured into one classical bit, with whatever gates
/// come first.
fn measured(gates: &[StandardGate]) -> CircuitStore {
    let mut store = CircuitStore::new(1, 1);
    for standard in gates {
        store
            .push_standard(*standard, &[Qubit(0)])
            .expect("gate should be accepted");
    }
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");
    store
}

fn measured_inverse_pair() -> CircuitStore {
    let mut store = CircuitStore::new(2, 2);
    store
        .push_standard(StandardGate::X, &[Qubit(0)])
        .expect("prepare");
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("first cx");
    store
        .push_standard(StandardGate::Cx, &[Qubit(0), Qubit(1)])
        .expect("second cx");
    for index in 0..2 {
        store
            .push(
                PackedOperation::Standard(StandardGate::MeasureZ),
                &[Qubit(index)],
                &[Clbit(index)],
                None,
            )
            .expect("measure");
    }
    store
}

#[test]
fn checked_inverse_bypass_preserves_measured_ten_and_refuses_crossed_zero_one() {
    use qsc_qhdg::{GraphRewriteRule, InverseCancellation, PatchEdge, apply_certified_patch};
    let store = measured_inverse_pair();
    let mut graph = circuit_to_graph(&store).expect("lower");
    let adjacency = Adjacency::new(&graph);
    let facts = qsc_qhdg::RewriteContext {
        operation_facts: super::inverse_facts(&graph),
        ..qsc_qhdg::RewriteContext::default()
    };
    let seed = graph
        .nodes()
        .find(|node| node.operation.0 == "quantum.cx")
        .and_then(|node| graph.stable_id(node.id))
        .expect("cx");
    let matched = InverseCancellation
        .find(&graph, &adjacency, &facts, seed)
        .expect("find")
        .expect("pair");
    let certificate = InverseCancellation
        .prove(&graph, &adjacency, &facts, &matched)
        .expect("prove");
    let patch = InverseCancellation
        .build(&graph, &adjacency, &matched, &certificate)
        .expect("build");
    let mut crossed = patch.clone();
    let edges: Vec<_> = crossed.connect.iter().copied().collect();
    assert_eq!(edges.len(), 2);
    crossed.connect = BTreeSet::from([
        PatchEdge {
            target: edges[1].target,
            ..edges[0]
        },
        PatchEdge {
            target: edges[0].target,
            ..edges[1]
        },
    ]);
    let original = graph.clone();
    apply_certified_patch(
        &mut graph,
        &adjacency,
        &crossed,
        &certificate,
        &mut qsc_qhdg::DischargeContext::new(super::circuit_inverses()),
    )
    .expect_err("a genuine inverse pair cannot certify a swapped result");
    assert_eq!(graph, original);
    apply_certified_patch(
        &mut graph,
        &adjacency,
        &patch,
        &certificate,
        &mut qsc_qhdg::DischargeContext::new(super::circuit_inverses()),
    )
    .expect("positional bypass");
    let optimized = crate::circuit_qhdg::graph_to_circuit(&graph).expect("raise");
    assert!(
        !operations(&graph)
            .iter()
            .any(|operation| operation == "quantum.cx")
    );
    #[cfg(unix)]
    {
        let caps = qsc_data_structures::target::Profile::AdaptiveRI.into();
        crate::circuit::tests::assert_native_bits(
            &crate::circuit::circuit_to_qir(&store, caps).expect("original QIR"),
            "10",
        );
        crate::circuit::tests::assert_native_bits(
            &crate::circuit::circuit_to_qir(&optimized, caps).expect("optimized QIR"),
            "10",
        );
        let mut unchecked = original.clone();
        let mut asserted = certificate;
        asserted.obligation = ProofObligation::Assumed;
        asserted.evidence = "asserted by the caller".to_string();
        apply_certified_patch(
            &mut unchecked,
            &adjacency,
            &crossed,
            &asserted,
            &mut qsc_qhdg::DischargeContext::default().allowing_assumed(),
        )
        .expect("explicitly unchecked counterexample");
        let wrong = crate::circuit_qhdg::graph_to_circuit(&unchecked).expect("raise crossed graph");
        crate::circuit::tests::assert_native_bits(
            &crate::circuit::circuit_to_qir(&wrong, caps).expect("crossed QIR"),
            "01",
        );
    }
    #[cfg(not(unix))]
    let _ = optimized;
}

#[test]
fn the_collapse_pass_retires_a_diagonal_gate_that_runs_into_a_measurement() {
    let run = PassPipeline::with_built_ins()
        .run(&measured(&[StandardGate::H, StandardGate::Z]))
        .expect("pipeline should run");
    assert_eq!(
        operations(&run.graph),
        ["quantum.h", "quantum.measure_z"],
        "the z is gone and nothing else moved"
    );
    assert_eq!(
        run.changed,
        vec!["retire-diagonal-before-z-collapse".to_string()],
        "and the pass that did it is the one that reports it"
    );

    // The same circuit with the pass declining, so the reduction above is
    // attributable to it rather than to the progression.
    let mut declined = PassPipeline::with_built_ins();
    declined.set_optimization_level(OptimizationLevel::None);
    let untouched = declined
        .run(&measured(&[StandardGate::H, StandardGate::Z]))
        .expect("pipeline should run");
    assert_eq!(
        operations(&untouched.graph),
        ["quantum.h", "quantum.z", "quantum.measure_z"]
    );
}

#[test]
fn the_collapse_pass_leaves_a_gate_it_cannot_justify_retiring() {
    // A non-diagonal gate in the same position, so what the pass reads is the
    // declaration rather than the shape.
    let run = PassPipeline::with_built_ins()
        .run(&measured(&[StandardGate::H, StandardGate::SqrtX]))
        .expect("pipeline should run");
    assert_eq!(
        operations(&run.graph),
        ["quantum.h", "quantum.sx", "quantum.measure_z"]
    );
    assert!(run.changed.is_empty());
}

/// A measured qubit carrying a `z` afterwards, which is the prefix both halves
/// of the fan-out test share. The measurement comes first so a conditional has
/// a result to read.
fn measured_then_diagonal() -> CircuitStore {
    let mut store = CircuitStore::new(1, 2);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");
    store
        .push_standard(StandardGate::Z, &[Qubit(0)])
        .expect("gate should be accepted");
    store
}

/// Whether one node orders another, over every edge kind.
fn orders(graph: &Graph, source: StableNodeId, target: StableNodeId) -> bool {
    let adjacency = Adjacency::new(graph);
    let mut pending = vec![source];
    let mut seen = BTreeSet::from([source]);
    while let Some(current) = pending.pop() {
        if current == target {
            return true;
        }
        for next in adjacency.successors(current) {
            if seen.insert(next) {
                pending.push(next);
            }
        }
    }
    false
}

/// How many consumers the circuit's one declared-diagonal gate hands its
/// capability to, read through the same adjacency the rule's guard reads.
fn capability_consumers(graph: &Graph) -> usize {
    let node = graph
        .nodes()
        .find(|node| node.operation.0 == "quantum.z")
        .expect("the diagonal gate survives lowering");
    let outputs: Vec<_> = node
        .outputs
        .iter()
        .copied()
        .filter(|port| {
            graph
                .port(*port)
                .is_some_and(|held| held.kind == PortKind::Capability)
        })
        .collect();
    let [capability] = outputs.as_slice() else {
        panic!("a one-operand gate holds one capability output");
    };
    Adjacency::new(graph).consumers_of(*capability).len()
}

#[test]
fn a_lowered_conditional_hands_a_diagonal_gates_capability_to_two_consumers() {
    // The arrangement the multiplicity guards decline, on a lowered program
    // rather than a hand-built graph. Three things make it what it is: the
    // conditional needs an else block, or the capability reaches one consumer;
    // a branch body has to touch the qubit, or it is not carried at all; and it
    // is left flat, because nesting would put the second consumer a region
    // further down and give the guards a different arrangement to read.
    let mut store = measured_then_diagonal();
    let then_block = store.new_block();
    store
        .push_into_block(
            then_block,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    let else_block = store.new_block();
    store
        .push_into_block(
            else_block,
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("gate should be accepted");
    let condition = store.push_expr(Expr::Clbit(0));
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: Some(else_block),
        }))
        .expect("conditional should be accepted");

    let branched = circuit_to_graph(&store).expect("the circuit should lower");
    assert_eq!(
        capability_consumers(&branched),
        2,
        "each branch connects the one outer capability the conditional handed it"
    );

    // The same prefix with a measurement where the conditional was. One
    // consumer, so what the count above reads is the conditional rather than
    // anything the two circuits share.
    let mut collapsed = measured_then_diagonal();
    collapsed
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(1)],
            None,
        )
        .expect("measurement should be accepted");
    let straight = circuit_to_graph(&collapsed).expect("the circuit should lower");
    assert_eq!(
        capability_consumers(&straight),
        1,
        "without the conditional the capability runs into the measurement alone"
    );

    // Corroborating rather than attributing. A branch consumer is not in the
    // collapsing set either, so this gate would survive with the multiplicity
    // guard deleted. What pins that guard is the pair of graph-level tests in
    // the qsc_qhdg rewrite suite.
    let survives = PassPipeline::with_built_ins()
        .run(&store)
        .expect("pipeline should run");
    assert!(
        survives
            .graph
            .nodes()
            .any(|node| node.operation.0 == "quantum.z"),
        "the pass leaves the gate before the conditional alone"
    );
    let retired = PassPipeline::with_built_ins()
        .run(&collapsed)
        .expect("pipeline should run");
    assert!(
        !retired
            .graph
            .nodes()
            .any(|node| node.operation.0 == "quantum.z"),
        "and retires the same gate when it runs into the measurement instead"
    );
}

#[test]
fn an_observable_inside_a_conditional_branch_lowers() {
    // Refused until the lowering stopped building a second, per-qubit effect
    // chain: an observable node inside a branch chained back to the parent
    // region across a boundary that binds interfaces only for capability ports.
    // The capability chain that remains crosses it through a declared binding.
    let lower = |body: PackedOperation| {
        let mut store = measured_then_diagonal();
        let then_block = store.new_block();
        store
            .push_into_block(then_block, body, &[Qubit(0)], &[], None)
            .expect("block operation should be accepted");
        let else_block = store.new_block();
        store
            .push_into_block(
                else_block,
                PackedOperation::Standard(StandardGate::H),
                &[Qubit(0)],
                &[],
                None,
            )
            .expect("block operation should be accepted");
        let condition = store.push_expr(Expr::Clbit(0));
        store
            .push_control_flow(ControlFlow::IfElse(IfElse {
                condition,
                then_block,
                else_block: Some(else_block),
            }))
            .expect("conditional should be accepted");
        circuit_to_graph(&store).map_err(|error| format!("{error:?}"))
    };

    let refusal = lower(PackedOperation::Reset);
    assert!(
        refusal.is_ok(),
        "a reset inside one branch lowers, got {refusal:?}"
    );
    assert!(
        lower(PackedOperation::Standard(StandardGate::X)).is_ok(),
        "and so does the same conditional with a unitary body"
    );

    // Ordering survives the region crossing. Nothing covered this before,
    // because the shape did not lower.
    let branched = lower(PackedOperation::Standard(StandardGate::MeasureZ))
        .expect("a measurement inside one branch lowers");
    let measure = |index: usize| {
        let node = branched
            .nodes()
            .filter(|node| node.operation.0 == "quantum.measure_z")
            .nth(index)
            .expect("measurement");
        branched.stable_id(node.id).expect("stable id")
    };
    assert!(
        orders(&branched, measure(0), measure(1)),
        "the branch measurement comes after the one outside the conditional"
    );
    assert!(
        !orders(&branched, measure(1), measure(0)),
        "and not the reverse, so this is an order rather than connectivity"
    );

    // Placement is no longer what decides it. The same measurement after the
    // conditional lowers as well.
    let mut after = measured_then_diagonal();
    let then_block = after.new_block();
    after
        .push_into_block(
            then_block,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("block operation should be accepted");
    let condition = after.push_expr(Expr::Clbit(0));
    after
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: None,
        }))
        .expect("conditional should be accepted");
    after
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(1)],
            None,
        )
        .expect("trailing measurement should be accepted");
    assert!(
        circuit_to_graph(&after).is_ok(),
        "a measurement after the conditional lowers too"
    );
}

#[test]
fn a_branch_that_ignores_a_carried_qubit_still_threads_it() {
    // The carried set is the union across branches, so a qubit only one branch
    // touches is carried through the other as well. The branch that ignores it
    // gets a `quantum.carry` node, which is what keeps the capability threaded
    // on both sides of the merge instead of stranding it.
    let mut store = CircuitStore::new(2, 2);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");
    let then_block = store.new_block();
    store
        .push_into_block(
            then_block,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("block operation should be accepted");
    let else_block = store.new_block();
    store
        .push_into_block(
            else_block,
            PackedOperation::Standard(StandardGate::H),
            &[Qubit(1)],
            &[],
            None,
        )
        .expect("block operation should be accepted");
    let condition = store.push_expr(Expr::Clbit(0));
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: Some(else_block),
        }))
        .expect("conditional should be accepted");

    let graph = circuit_to_graph(&store).expect("the circuit should lower");
    let carried = graph
        .nodes()
        .filter(|node| node.operation.0 == "quantum.carry")
        .count();
    assert_eq!(
        carried, 2,
        "each branch hands through the one qubit the other touched"
    );
}

#[test]
fn retiring_an_angle_bearing_gate_leaves_its_constant_producer_live() {
    // The node-count delta is the retired gate alone, not the two a reader
    // might expect for a parameterised gate. Verification accepts the dangling
    // constant, because the produced-and-never-consumed rule covers
    // qubit-carrying and affine-capability ports and a `Double` output is
    // neither.
    let mut store = CircuitStore::new(1, 1);
    store
        .push(
            PackedOperation::Standard(StandardGate::Rz),
            &[Qubit(0)],
            &[],
            Some(Box::new([ParamValue::Literal(0.5)])),
        )
        .expect("rz should be accepted");
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");

    let before = PassPipeline::empty()
        .run(&store)
        .expect("pipeline should run");
    let after = PassPipeline::with_built_ins()
        .run(&store)
        .expect("pipeline should run");
    assert_eq!(
        before.graph.nodes().count() - after.graph.nodes().count(),
        1,
        "one node per retirement, counting the gate and not its angle"
    );
    assert!(
        after
            .graph
            .nodes()
            .any(|node| node.operation.0.starts_with("constant:")),
        "the angle's producer is still there, live and unread"
    );
}

/// Every ordering of `0..count`, so a pair can be tried in each arrangement.
fn permutations(count: u32) -> Vec<Vec<u32>> {
    if count == 0 {
        return vec![Vec::new()];
    }
    let mut all = Vec::new();
    for head in 0..count {
        for mut rest in permutations(count - 1) {
            for operand in &mut rest {
                if *operand >= head {
                    *operand += 1;
                }
            }
            rest.insert(0, head);
            all.push(rest);
        }
    }
    all
}

#[test]
fn a_pair_cancels_in_exactly_one_arrangement_of_its_operands() {
    // The semantics gate against the rule. Every operation the default pipeline
    // names, in every ordering of its operands, so this covers a class rather
    // than the cases someone thought to enumerate.
    //
    // Two of these gates, cz and swap, are symmetric, so their permuted pair is
    // also the identity and cancelling it would be sound. The rule declines
    // anyway, because it decides from operand position rather than from what an
    // operation means. That is deliberate and asserted here so it is not later
    // mistaken for a defect.
    for name in super::SELF_INVERSE {
        let gate = crate::circuit_qhdg::gate_from_name(name)
            .unwrap_or_else(|| panic!("'{name}' should be a standard gate"));
        let arity = u32::from(gate.qubit_arity());
        let aligned: Vec<u32> = (0..arity).collect();

        for ordering in permutations(arity) {
            let store = paired(gate, &aligned, &ordering);
            let run = PassPipeline::with_built_ins()
                .run(&store)
                .expect("pipeline should run");
            let survived = operations(&run.graph).len();

            if ordering == aligned {
                assert_eq!(
                    survived, 0,
                    "{name} over {aligned:?} twice is the identity and should cancel"
                );
            } else {
                assert_eq!(
                    survived, 2,
                    "{name} over {aligned:?} then {ordering:?} must not cancel"
                );
            }
        }
    }
}

#[test]
fn every_anchor_is_exercised_and_its_change_reaches_the_output() {
    let mut pipeline = PassPipeline::with_built_ins();
    pipeline
        .add_circuit_pass(
            PassAnchor::CircuitBuilt,
            Box::new(AppendGate {
                name: "append-y-on-the-way-in",
                standard: StandardGate::Y,
            }),
            false,
        )
        .expect("a circuit pass belongs at a circuit anchor");
    pipeline
        .add_graph_pass(
            PassAnchor::GraphLowered,
            Box::new(RenameOperation {
                name: "x-becomes-s",
                from: "quantum.x",
                to: "quantum.s",
            }),
            false,
        )
        .expect("a graph pass belongs at a graph anchor");
    pipeline
        .add_graph_pass(
            PassAnchor::GraphRewritten,
            Box::new(RenameOperation {
                name: "s-becomes-t",
                from: "quantum.s",
                to: "quantum.t",
            }),
            false,
        )
        .expect("a graph pass belongs at a graph anchor");
    pipeline
        .add_circuit_pass(
            PassAnchor::CircuitRaised,
            Box::new(AppendGate {
                name: "append-z-on-the-way-out",
                standard: StandardGate::Z,
            }),
            false,
        )
        .expect("a circuit pass belongs at a circuit anchor");

    let run = pipeline.run(&cancellable()).expect("pipeline should run");

    assert_eq!(
        run.executed
            .iter()
            .map(|entry| (entry.anchor, entry.name.as_str()))
            .collect::<Vec<_>>(),
        [
            (PassAnchor::CircuitBuilt, "append-y-on-the-way-in"),
            (PassAnchor::GraphLowered, "x-becomes-s"),
            (PassAnchor::GraphRewritten, "cancel-inverse-pairs"),
            (
                PassAnchor::GraphRewritten,
                "retire-diagonal-before-z-collapse"
            ),
            (PassAnchor::GraphRewritten, "s-becomes-t"),
            (PassAnchor::CircuitRaised, "append-z-on-the-way-out"),
        ],
        "a run reaches the anchors in the order the progression does"
    );
    assert_eq!(run.executed, pipeline.listing(), "and in the listed order");

    // The pair is gone because the built-in ran, x became t through two anchors
    // in turn, and the two appended gates bracket what is left.
    assert_eq!(
        exported(&run.circuit),
        exported(&circuit(&[
            StandardGate::T,
            StandardGate::Y,
            StandardGate::Z,
        ])),
        "every anchor's change is present in the exported program"
    );
}

#[test]
fn a_caller_pass_sees_what_the_built_in_left() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut pipeline = PassPipeline::with_built_ins();
    pipeline
        .add_graph_pass(
            PassAnchor::GraphRewritten,
            Box::new(Observer {
                seen: Arc::clone(&seen),
            }),
            false,
        )
        .expect("a graph pass belongs at a graph anchor");
    pipeline.run(&cancellable()).expect("pipeline should run");

    assert_eq!(
        seen.lock().expect("no other thread holds this").as_slice(),
        ["quantum.x"],
        "registration order puts the built-in first, so the pair is already gone"
    );
}

#[test]
fn a_failing_pass_is_reported_by_name_and_anchor() {
    let mut pipeline = PassPipeline::empty();
    pipeline
        .add_graph_pass(PassAnchor::GraphLowered, Box::new(AlwaysFails), false)
        .expect("a graph pass belongs at a graph anchor");
    assert_eq!(
        pipeline.run(&cancellable()).err(),
        Some(PipelineError::Failed {
            name: "always-fails".to_string(),
            anchor: "graph_lowered",
            reason: "nothing here works".to_string(),
        })
    );
}

#[test]
fn the_built_in_declines_a_graph_it_cannot_read() {
    // A pass registered earlier may produce a program outside the circuit
    // vocabulary. That is not the built-in's failure, so it leaves the graph
    // alone and the refusal comes from the raise, which is where it belongs.
    let mut pipeline = PassPipeline::with_built_ins();
    pipeline
        .add_graph_pass(
            PassAnchor::GraphLowered,
            Box::new(RenameOperation {
                name: "leave-the-vocabulary",
                from: "quantum.h",
                to: "vendor.mystery",
            }),
            false,
        )
        .expect("a graph pass belongs at a graph anchor");
    let refused = pipeline.run(&cancellable()).err();
    assert!(
        matches!(refused, Some(PipelineError::Raise(_))),
        "the raise is what refuses it, not the built-in: {refused:?}"
    );
}
