// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for lowering a graph to the runtime IR.
//!
//! The load-bearing property is not that QIR comes out. It is that the QIR is
//! the same QIR the direct path produces for the same program. Two producers
//! that agree on a handful of hand-picked assertions and disagree on operand
//! order or qubit numbering would pass a weaker test and emit programs that run
//! and give different answers.

use std::collections::{BTreeMap, BTreeSet};

use qsc_circuit_store::{
    BinaryOp, CircuitStore, Clbit, ControlFlow, Expr, IfElse, Literal, PackedOperation, ParamValue,
    Qubit, StandardGate,
};
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};
use qsc_qhdg::{
    Direction, Edge, EdgeKind, GlobalPhasePolicy, Graph, GraphBuilder, GraphRewriteDriver,
    GraphRewriteOutcome, InterfaceBinding, InterfaceRole, InverseCancellation, OperationClass,
    OperationFacts, PortId, PortKind, RegionKind, ResourceId, RewriteContext, StableNodeId, Stage,
    Usage,
};
use qsc_semantic_contracts::Type;

use super::{Error, graph_to_qir, graph_to_rir};
use crate::circuit::circuit_to_qir;
use crate::circuit_qhdg::{ReadError, ReadItem, circuit_to_graph, graph_to_circuit, read_program};

/// The profiles the two paths are compared under.
const PROFILES: [Profile; 2] = [Profile::Base, Profile::AdaptiveRI];

fn gate(store: &mut CircuitStore, gate: StandardGate, qubits: &[Qubit]) {
    store
        .push_standard(gate, qubits)
        .expect("gate should be accepted");
}

fn angled(store: &mut CircuitStore, gate: StandardGate, qubits: &[Qubit], angles: &[f64]) {
    let params: Vec<ParamValue> = angles.iter().copied().map(ParamValue::Literal).collect();
    store
        .push_parameterized(gate, qubits, &params)
        .expect("gate should be accepted");
}

/// Programs wide enough that agreement means something.
fn program_set() -> Vec<(&'static str, CircuitStore)> {
    vec![
        ("empty", CircuitStore::new(0, 0)),
        ("bell with measurement", bell()),
        ("single-qubit gate set", single_qubit_gates()),
        ("multi-qubit gate set", multi_qubit_gates()),
        ("reversed operand order", reversed_operands()),
        ("rotations", rotations()),
        ("controlled rotations", controlled_rotations()),
        ("barrier, reset, and measurement", mixed()),
        ("crossed classical targets", crossed_targets()),
        ("global phase", global_phase()),
        (
            "measurement with no classical bit",
            unrecorded_measurement(),
        ),
    ]
}

fn measure(store: &mut CircuitStore, qubit: u32, clbit: u32) {
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(qubit)],
            &[Clbit(clbit)],
            None,
        )
        .expect("measurement should be accepted");
}

fn bell() -> CircuitStore {
    let mut store = CircuitStore::new(2, 2);
    gate(&mut store, StandardGate::H, &[Qubit(0)]);
    gate(&mut store, StandardGate::Cx, &[Qubit(0), Qubit(1)]);
    measure(&mut store, 0, 0);
    measure(&mut store, 1, 1);
    store
}

fn single_qubit_gates() -> CircuitStore {
    let mut store = CircuitStore::new(1, 0);
    for standard in [
        StandardGate::I,
        StandardGate::X,
        StandardGate::Y,
        StandardGate::Z,
        StandardGate::H,
        StandardGate::S,
        StandardGate::SDag,
        StandardGate::T,
        StandardGate::TDag,
        StandardGate::SqrtX,
        StandardGate::SqrtXDag,
        StandardGate::PrepareZ,
    ] {
        gate(&mut store, standard, &[Qubit(0)]);
    }
    store
}

fn multi_qubit_gates() -> CircuitStore {
    let mut store = CircuitStore::new(3, 0);
    gate(&mut store, StandardGate::Cx, &[Qubit(0), Qubit(1)]);
    gate(&mut store, StandardGate::Cy, &[Qubit(1), Qubit(2)]);
    gate(&mut store, StandardGate::Cz, &[Qubit(2), Qubit(0)]);
    gate(&mut store, StandardGate::Swap, &[Qubit(0), Qubit(2)]);
    gate(
        &mut store,
        StandardGate::Ccx,
        &[Qubit(0), Qubit(1), Qubit(2)],
    );
    gate(
        &mut store,
        StandardGate::Cswap,
        &[Qubit(2), Qubit(1), Qubit(0)],
    );
    store
}

/// cx(1, 0) is not cx(0, 1). A lowering that recovered qubits by sorted index
/// rather than by port order would pass everything else here.
fn reversed_operands() -> CircuitStore {
    let mut store = CircuitStore::new(3, 0);
    gate(&mut store, StandardGate::Cx, &[Qubit(1), Qubit(0)]);
    gate(
        &mut store,
        StandardGate::Ccx,
        &[Qubit(2), Qubit(1), Qubit(0)],
    );
    store
}

fn rotations() -> CircuitStore {
    let mut store = CircuitStore::new(2, 0);
    angled(&mut store, StandardGate::Rx, &[Qubit(0)], &[0.375]);
    angled(&mut store, StandardGate::Ry, &[Qubit(0)], &[-1.25]);
    angled(&mut store, StandardGate::Rz, &[Qubit(1)], &[3.5]);
    angled(
        &mut store,
        StandardGate::Rxx,
        &[Qubit(0), Qubit(1)],
        &[0.125],
    );
    angled(
        &mut store,
        StandardGate::Ryy,
        &[Qubit(1), Qubit(0)],
        &[0.25],
    );
    angled(
        &mut store,
        StandardGate::Rzz,
        &[Qubit(0), Qubit(1)],
        &[-0.5],
    );
    store
}

/// Gates that reach the runtime IR only through a decomposition.
fn controlled_rotations() -> CircuitStore {
    let mut store = CircuitStore::new(2, 0);
    angled(&mut store, StandardGate::Crx, &[Qubit(0), Qubit(1)], &[0.5]);
    angled(
        &mut store,
        StandardGate::Cry,
        &[Qubit(0), Qubit(1)],
        &[0.75],
    );
    angled(&mut store, StandardGate::Crz, &[Qubit(1), Qubit(0)], &[1.5]);
    store
}

fn mixed() -> CircuitStore {
    let mut store = CircuitStore::new(2, 1);
    gate(&mut store, StandardGate::H, &[Qubit(0)]);
    store
        .push(
            PackedOperation::Barrier { qubit_arity: 2 },
            &[Qubit(0), Qubit(1)],
            &[],
            None,
        )
        .expect("barrier should be accepted");
    store
        .push(PackedOperation::Reset, &[Qubit(1)], &[], None)
        .expect("reset should be accepted");
    measure(&mut store, 1, 0);
    store
}

/// Measuring the higher qubit into the lower bit is where a lowering that
/// numbered results by visit order rather than by the circuit's own naming
/// would diverge.
fn crossed_targets() -> CircuitStore {
    let mut store = CircuitStore::new(2, 2);
    gate(&mut store, StandardGate::X, &[Qubit(1)]);
    measure(&mut store, 1, 0);
    measure(&mut store, 0, 1);
    store
}

fn global_phase() -> CircuitStore {
    let mut store = CircuitStore::new(1, 0);
    store.set_global_phase(0.625);
    gate(&mut store, StandardGate::H, &[Qubit(0)]);
    store
}

/// A program both paths refuse, so the comparison covers what they refuse and
/// not only what they emit.
///
/// The store accepts a measurement that names no classical bit, and neither path
/// has anywhere to put the result. Each reaches that conclusion through its own
/// emitter, which is what makes the agreement worth asserting.
fn unrecorded_measurement() -> CircuitStore {
    let mut store = CircuitStore::new(1, 0);
    gate(&mut store, StandardGate::H, &[Qubit(0)]);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("the store accepts a measurement with no classical bit");
    store
}

/// Compares what the two paths produce, including when they refuse.
fn assert_paths_agree(name: &str, store: &CircuitStore, profile: Profile) {
    let capabilities: TargetCapabilityFlags = profile.into();
    let graph = circuit_to_graph(store).unwrap_or_else(|error| panic!("{name}: {error}"));
    let direct = circuit_to_qir(store, capabilities);
    let through_graph = graph_to_qir(&graph, capabilities);
    match (direct, through_graph) {
        (Ok(direct), Ok(through_graph)) => assert_eq!(
            direct, through_graph,
            "{name} under {profile:?}: the two paths produced different QIR"
        ),
        (Err(direct), Ok(_)) => {
            panic!(
                "{name} under {profile:?}: the direct path refused with {direct} and the graph path did not"
            );
        }
        (Ok(_), Err(through_graph)) => {
            panic!(
                "{name} under {profile:?}: the graph path refused with {through_graph} and the direct path did not"
            );
        }
        (Err(direct), Err(through_graph)) => assert_eq!(
            direct.to_string(),
            through_graph.to_string(),
            "{name} under {profile:?}: the two paths refused for different reasons"
        ),
    }
}

/// The runtime-IR program carries no `Debug`, so a refusal is unwrapped by hand.
fn refusal(result: Result<qsc_rir::rir::Program, Error>) -> Error {
    match result {
        Ok(_) => panic!("expected a refusal"),
        Err(error) => error,
    }
}

#[test]
fn the_graph_path_and_the_direct_path_produce_the_same_qir() {
    for (name, store) in program_set() {
        for profile in PROFILES {
            assert_paths_agree(name, &store, profile);
        }
    }
}

#[test]
fn both_paths_refuse_the_same_program_and_name_the_same_reason() {
    // Asserted on its own as well as through the program set, because a case
    // that stopped refusing would still pass there by taking the output arm.
    let store = unrecorded_measurement();
    let graph = circuit_to_graph(&store).expect("circuit should lower");
    for profile in PROFILES {
        let capabilities: TargetCapabilityFlags = profile.into();
        let direct =
            circuit_to_qir(&store, capabilities).expect_err("the direct path should refuse");
        let through_graph =
            graph_to_qir(&graph, capabilities).expect_err("the graph path should refuse");
        assert_eq!(
            direct.to_string(),
            through_graph.to_string(),
            "under {profile:?} the two paths worded the same refusal differently"
        );
        assert_eq!(
            through_graph,
            Error::UnsupportedOperation {
                operation: "measurement without a classical bit"
            }
        );
    }
}

#[test]
fn a_measurement_keeps_the_classical_bit_the_circuit_named() {
    // Asserted against the runtime IR rather than the text, so a failure names
    // the operand rather than a diff of two programs.
    let mut store = CircuitStore::new(2, 2);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(1)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");
    let graph = circuit_to_graph(&store).expect("circuit should lower");
    let program = graph_to_rir(&graph, Profile::AdaptiveRI.into()).expect("graph should lower");

    let text = format!("{program}");
    assert!(
        text.contains("Qubit(1)") && text.contains("Result(0)"),
        "the measurement should name qubit 1 and result 0: {text}"
    );
    assert_eq!(program.num_qubits, 2);
    assert_eq!(program.num_results, 2);
}

#[test]
fn an_operation_with_no_runtime_form_is_named() {
    let mut store = CircuitStore::new(1, 0);
    gate(&mut store, StandardGate::H, &[Qubit(0)]);
    let graph = rebuild_renaming(
        &circuit_to_graph(&store).expect("circuit should lower"),
        "quantum.h",
        "quantum.magic_state_distill",
    );
    assert_eq!(
        refusal(graph_to_rir(&graph, Profile::Base.into())),
        Error::Unsupported {
            operation: "quantum.magic_state_distill".to_string()
        }
    );
}

/// The graph carries symbolic angles so passes can run on a variational
/// program. A target cannot, so the boundary refuses by name rather than
/// emitting a zero or panicking.
#[test]
fn an_unbound_parameter_is_refused_at_the_runtime_boundary() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("theta");
    store
        .push_parameterized(
            StandardGate::Rx,
            &[Qubit(0)],
            &[qsc_circuit_store::ParamValue::Parameter(theta)],
        )
        .expect("gate should be accepted");
    let graph = circuit_to_graph(&store).expect("circuit should lower");
    assert_eq!(
        refusal(graph_to_rir(&graph, Profile::Base.into())),
        Error::UnboundParameter {
            parameter: "theta".to_string()
        }
    );
}

/// What a rebind is for. The same graph, rebound, compiles to exactly the QIR
/// the direct path emits for the circuit that carried the value all along, so
/// the value update is a substitution rather than a second lowering.
#[test]
fn a_rebound_graph_compiles_to_the_same_qir_as_the_bound_circuit() {
    let mut store = CircuitStore::new(1, 0);
    let theta = store.declare_parameter("theta");
    store
        .push_parameterized(
            StandardGate::Rx,
            &[Qubit(0)],
            &[ParamValue::Parameter(theta)],
        )
        .expect("gate should be accepted");
    let mut graph = circuit_to_graph(&store).expect("circuit should lower");
    graph
        .rebind_parameters(&BTreeMap::from([("theta".to_string(), 0.375)]))
        .expect("rebind should verify");

    let mut bound = CircuitStore::new(1, 0);
    angled(&mut bound, StandardGate::Rx, &[Qubit(0)], &[0.375]);
    assert_eq!(
        graph_to_qir(&graph, Profile::Base.into()).expect("the rebound graph should compile"),
        circuit_to_qir(&bound, Profile::Base.into()).expect("the bound circuit should compile")
    );
}

#[test]
fn a_graph_from_another_producer_is_refused() {
    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let node = builder
        .add_node(region, "call", Vec::new())
        .expect("region exists");
    builder
        .add_port(
            node,
            Direction::Output,
            PortKind::ControlExit,
            Type::Unit,
            Stage::Residual,
            Usage::Affine,
        )
        .expect("node exists");
    let graph = builder.finish();
    assert_eq!(
        refusal(graph_to_rir(&graph, Profile::Base.into())),
        Error::Read(ReadError::Vocabulary {
            operation: "call".to_string()
        })
    );
}

/// A branch region only means something as the body of a node, because siblings
/// under one parent are what a gamma's branches are and nesting alone cannot say
/// which node they belong to or what selects between them.
#[test]
fn a_branch_region_no_node_owns_is_refused_on_both_sides() {
    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);
    builder.add_region(Some(module), RegionKind::Gamma);
    let graph = builder.finish();
    assert_eq!(
        refusal(graph_to_rir(&graph, Profile::Base.into())),
        Error::Read(ReadError::Malformed {
            reason: "a branch region is not the body of any node".to_string()
        })
    );
    assert!(
        graph_to_circuit(&graph).is_err(),
        "the raise refuses the same graph, so the two agree about what a region means"
    );
}

/// A module region holding a child region with a whole one-qubit program in it.
///
/// The body is self-contained, so the graph verifies clean and the refusal
/// under test is the reader's rather than the verifier's. That is the point:
/// the design review removed the reader's guard and measured what happened, and
/// a populated region of this shape read back as unconditional code with zero
/// diagnostics to say so.
fn populated_region(kind: RegionKind) -> Graph {
    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);
    let inner = builder.add_region(Some(module), kind);

    let mut carried = None;
    for operation in ["quantum.allocate", "quantum.x", "quantum.release"] {
        let node = builder
            .add_node(inner, operation, Vec::new())
            .expect("region exists");
        if let Some(source) = carried {
            let target = capability(&mut builder, node, Direction::Input);
            builder
                .connect(Edge {
                    source,
                    target,
                    kind: EdgeKind::AffineCapability,
                })
                .expect("capability edge");
        }
        carried = (operation != "quantum.release")
            .then(|| capability(&mut builder, node, Direction::Output));
    }

    let graph = builder.finish();
    assert!(
        qsc_qhdg::verify(&graph).is_empty(),
        "the fixture must be refused by the reader rather than by the verifier: {:?}",
        qsc_qhdg::verify(&graph)
    );
    graph
}

/// A well-formed conditional: a gamma node with a predicate, two branch regions
/// it owns, one qubit fed into both, and one result they agree on.
///
/// It verifies clean, which is the fact that makes the reader's guard
/// load-bearing. The design review built a graph of this shape, removed the
/// guard, and watched it read back as unconditional code with nothing to say so.
fn populated_gamma() -> Graph {
    let mut builder = GraphBuilder::new();
    let module = builder.add_region(None, RegionKind::ModuleOmega);

    let declared = builder
        .add_node(module, "classical.allocate", Vec::new())
        .expect("region exists");
    result_value(&mut builder, declared, Direction::Output);

    let allocate = builder
        .add_node(module, "quantum.allocate", Vec::new())
        .expect("region exists");
    let allocated = capability(&mut builder, allocate, Direction::Output);

    let measure = builder
        .add_node(module, "quantum.measure_z", Vec::new())
        .expect("region exists");
    let measured = capability(&mut builder, measure, Direction::Input);
    builder
        .connect(Edge {
            source: allocated,
            target: measured,
            kind: EdgeKind::AffineCapability,
        })
        .expect("measurement edge");
    let live = capability(&mut builder, measure, Direction::Output);
    let predicate = result_value(&mut builder, measure, Direction::Output);

    let store = builder
        .add_node(module, "classical.store:0", Vec::new())
        .expect("region exists");
    builder
        .declare_payload(
            store,
            qsc_qhdg::NodePayload {
                value: Some(qsc_qhdg::NodeValue::ClassicalBit(0)),
                origin: None,
            },
        )
        .expect("node exists");
    let stored = result_value(&mut builder, store, Direction::Input);
    builder
        .connect(Edge {
            source: predicate,
            target: stored,
            kind: EdgeKind::CopyableValue,
        })
        .expect("store edge");

    let gamma = builder
        .add_node(module, "quantum.gamma", Vec::new())
        .expect("region exists");
    let selector = result_value(&mut builder, gamma, Direction::Input);
    builder
        .connect(Edge {
            source: predicate,
            target: selector,
            kind: EdgeKind::CopyableValue,
        })
        .expect("predicate edge");
    let merged = capability(&mut builder, gamma, Direction::Output);

    for operation in ["quantum.x", "quantum.y"] {
        let branch = builder.add_region(Some(module), RegionKind::Gamma);
        builder
            .declare_region_owner(branch, gamma)
            .expect("gamma exists");
        let node = builder
            .add_node(branch, operation, Vec::new())
            .expect("region exists");
        let takes = capability(&mut builder, node, Direction::Input);
        let gives = capability(&mut builder, node, Direction::Output);
        // The binding has to exist before the edge, because a cross-region edge
        // is legal only where an interface declares the crossing.
        for (outer, inner, role) in [
            (live, takes, InterfaceRole::Argument),
            (merged, gives, InterfaceRole::Result),
        ] {
            builder
                .bind_interface(branch, InterfaceBinding { outer, inner, role })
                .expect("binding");
        }
        builder
            .connect(Edge {
                source: live,
                target: takes,
                kind: EdgeKind::AffineCapability,
            })
            .expect("argument edge");
    }

    let release = builder
        .add_node(module, "quantum.release", Vec::new())
        .expect("region exists");
    let released = capability(&mut builder, release, Direction::Input);
    builder
        .connect(Edge {
            source: merged,
            target: released,
            kind: EdgeKind::AffineCapability,
        })
        .expect("release edge");

    let graph = builder.finish();
    assert!(
        qsc_qhdg::verify(&graph).is_empty(),
        "the fixture must be refused by the reader rather than by the verifier: {:?}",
        qsc_qhdg::verify(&graph)
    );
    graph
}

fn capability(builder: &mut GraphBuilder, node: qsc_qhdg::NodeId, direction: Direction) -> PortId {
    builder
        .add_port(
            node,
            direction,
            PortKind::Capability,
            Type::Qubit,
            Stage::Residual,
            Usage::Linear,
        )
        .expect("node exists")
}

fn result_value(
    builder: &mut GraphBuilder,
    node: qsc_qhdg::NodeId,
    direction: Direction,
) -> PortId {
    builder
        .add_port(
            node,
            direction,
            PortKind::Value,
            Type::Result,
            Stage::Residual,
            Usage::Copyable,
        )
        .expect("node exists")
}

/// Populated branch regions must retain their conditional execution rather
/// than being flattened into unconditional instructions.
#[test]
fn a_populated_gamma_reads_back_as_a_conditional_rather_than_unconditional_code() {
    let graph = populated_gamma();
    assert_eq!(
        graph
            .regions()
            .filter(|region| region.kind == RegionKind::Gamma)
            .count(),
        2,
        "the regions under test are populated branches"
    );
    let program = read_program(&graph).expect("the graph should read");
    let [_, ReadItem::Conditional(conditional)] = program.items.as_slice() else {
        panic!(
            "expected a measurement then a conditional, got {:?}",
            program.items
        );
    };
    assert_eq!(conditional.clbit, 0);
    assert_eq!(branch_operations(&conditional.then_items), ["quantum.x"]);
    assert_eq!(
        conditional
            .else_items
            .as_deref()
            .map(branch_operations)
            .expect("the second branch is the alternative"),
        ["quantum.y"],
        "the second branch is read as the alternative rather than merged into the first"
    );
}

fn branch_operations(items: &[ReadItem]) -> Vec<&str> {
    items
        .iter()
        .map(|item| match item {
            ReadItem::Instruction(instruction) => instruction.operation.as_str(),
            ReadItem::Conditional(_) => "conditional",
            ReadItem::Loop(_) => "loop",
        })
        .collect()
}

/// The guard for narrowing the refusal to an allow-list. Supporting two region
/// kinds must not let the other six fall through to a traversal that emits
/// their contents unconditionally, so this asserts a named refusal before the
/// narrowing and after it.
///
/// Read against theta until theta gained a traversal of its own. A kind with no
/// traversal is what this test is about, so it moved to one rather than being
/// weakened into asserting the acceptance it was written to guard against.
#[test]
fn a_populated_region_of_an_unsupported_kind_is_refused_by_name() {
    let graph = populated_region(RegionKind::Lambda);
    assert_eq!(
        graph.nodes().count(),
        3,
        "the region under test is populated"
    );
    assert_eq!(
        refusal(graph_to_rir(&graph, Profile::Base.into())),
        Error::Read(ReadError::Region {
            kind: "Lambda".to_string()
        })
    );
    assert!(graph_to_circuit(&graph).is_err());
}

/// A circuit holding a conditional, which is the first construct the graph
/// carries as a region rather than refusing.
fn conditional_circuit(with_else: bool) -> CircuitStore {
    let mut store = CircuitStore::new(2, 1);
    gate(&mut store, StandardGate::H, &[Qubit(0)]);
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
            &[Qubit(1)],
            &[],
            None,
        )
        .expect("branch instruction should be accepted");
    let else_block = with_else.then(|| {
        let block = store.new_block();
        store
            .push_into_block(
                block,
                PackedOperation::Standard(StandardGate::Z),
                &[Qubit(1)],
                &[],
                None,
            )
            .expect("branch instruction should be accepted");
        block
    });
    let condition = store.push_expr(Expr::Clbit(0));
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block,
        }))
        .expect("conditional should be accepted");
    store
}

/// A conditional reaches the same QIR through the graph and direct paths.
/// Under a profile without branching both
/// paths have to refuse it, and refuse it the same way, because a graph that
/// compiles what a circuit refuses is the divergence this comparison is for.
#[test]
fn a_conditional_reaches_qir_the_way_the_direct_path_does() {
    for with_else in [false, true] {
        let store = conditional_circuit(with_else);
        let graph = circuit_to_graph(&store).expect("the conditional should lower");
        for profile in PROFILES {
            let capabilities: TargetCapabilityFlags = profile.into();
            match (
                graph_to_qir(&graph, capabilities),
                circuit_to_qir(&store, capabilities),
            ) {
                (Ok(from_graph), Ok(from_circuit)) => assert_eq!(
                    from_graph, from_circuit,
                    "under {profile:?} the two paths should emit one program"
                ),
                (Err(from_graph), Err(from_circuit)) => assert_eq!(
                    from_graph.to_string(),
                    from_circuit.to_string(),
                    "under {profile:?} the two paths should refuse for one reason"
                ),
                (from_graph, from_circuit) => panic!(
                    "under {profile:?} the paths disagreed: {from_graph:?} against {from_circuit:?}"
                ),
            }
        }
    }
}

/// The round trip through the region, which is what says the graph carries the
/// conditional rather than a flattening of it.
#[test]
fn a_conditional_raises_back_into_the_circuit_it_came_from() {
    for with_else in [false, true] {
        let store = conditional_circuit(with_else);
        let graph = circuit_to_graph(&store).expect("the conditional should lower");
        let raised = graph_to_circuit(&graph).expect("the conditional should raise");
        assert_eq!(
            circuit_to_qir(&raised, Profile::AdaptiveRI.into())
                .expect("the raised circuit should lower"),
            circuit_to_qir(&store, Profile::AdaptiveRI.into())
                .expect("the original circuit should lower"),
            "the raised conditional should run the program the original ran"
        );
    }
}

/// The published surface writes a conditional as an equality against one, which
/// the direct path folds away because the comparison carries nothing the result
/// does not. The graph path has to read through it as well, or the producer
/// this phase adds would have no driver a caller can reach.
#[test]
fn a_condition_written_as_an_equality_reaches_qir_the_way_a_bare_read_does() {
    let mut store = CircuitStore::new(2, 1);
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
            &[Qubit(1)],
            &[],
            None,
        )
        .expect("branch instruction should be accepted");
    let bit = store.push_expr(Expr::Clbit(0));
    let one = store.push_expr(Expr::Literal(Literal::Int(1)));
    let condition = store.push_expr(Expr::Binary(BinaryOp::Eq, bit, one));
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block,
            else_block: None,
        }))
        .expect("conditional should be accepted");

    let graph = circuit_to_graph(&store).expect("the conditional should lower");
    let capabilities: TargetCapabilityFlags = Profile::AdaptiveRI.into();
    assert_eq!(
        graph_to_qir(&graph, capabilities).expect("the graph should lower"),
        circuit_to_qir(&store, capabilities).expect("the circuit should lower"),
        "the two paths should emit one program for the condition the surface writes"
    );
}

/// A conditional inside a branch reaches the same program on both paths.
///
/// This shape used to be refused by name, on the reasoning that a branch's
/// argument binds to the port that feeds it and a binding crosses exactly one
/// region boundary, so a conditional inside a branch had no port at the
/// intervening level to bind through. The premise held; the conclusion did not,
/// because the lowering can create that port rather than having to find one.
/// It is also the first shape to exercise the emitter's recursion over nested
/// conditionals differentially.
#[test]
fn a_conditional_inside_a_branch_agrees_with_the_direct_path() {
    let mut store = CircuitStore::new(1, 1);
    store
        .push(
            PackedOperation::Standard(StandardGate::MeasureZ),
            &[Qubit(0)],
            &[Clbit(0)],
            None,
        )
        .expect("measurement should be accepted");
    let inner_body = store.new_block();
    store
        .push_into_block(
            inner_body,
            PackedOperation::Standard(StandardGate::X),
            &[Qubit(0)],
            &[],
            None,
        )
        .expect("branch instruction should be accepted");
    let outer_body = store.new_block();
    let condition = store.push_expr(Expr::Clbit(0));
    store
        .push_control_flow_into_block(
            outer_body,
            ControlFlow::IfElse(IfElse {
                condition,
                then_block: inner_body,
                else_block: None,
            }),
        )
        .expect("nested conditional should be accepted by the store");
    store
        .push_control_flow(ControlFlow::IfElse(IfElse {
            condition,
            then_block: outer_body,
            else_block: None,
        }))
        .expect("conditional should be accepted");

    let graph = circuit_to_graph(&store).expect("the nested conditional should lower");
    let capabilities: TargetCapabilityFlags = Profile::AdaptiveRI.into();
    assert_eq!(
        graph_to_qir(&graph, capabilities).expect("the graph should lower"),
        circuit_to_qir(&store, capabilities).expect("the circuit should lower"),
        "the two paths should emit one program for a conditional inside a branch"
    );
}

#[test]
fn the_two_paths_still_agree_after_a_rewrite() {
    // The pass changes the graph independently of the original circuit.
    // Comparing against the reduced circuit is
    // what makes this more than a round trip.
    let mut store = CircuitStore::new(1, 0);
    for standard in [StandardGate::H, StandardGate::H, StandardGate::X] {
        gate(&mut store, standard, &[Qubit(0)]);
    }
    let mut graph = circuit_to_graph(&store).expect("circuit should lower");
    cancel_adjacent_hadamards(&mut graph);

    let mut reduced = CircuitStore::new(1, 0);
    gate(&mut reduced, StandardGate::X, &[Qubit(0)]);

    for profile in PROFILES {
        let capabilities: TargetCapabilityFlags = profile.into();
        assert_eq!(
            graph_to_qir(&graph, capabilities).expect("the rewritten graph should lower"),
            circuit_to_qir(&reduced, capabilities).expect("the reduced circuit should lower"),
            "under {profile:?} the rewritten graph should produce the reduced program"
        );
    }
}

/// Runs the rewrite driver until the adjacent inverse pair cancels.
fn cancel_adjacent_hadamards(graph: &mut Graph) {
    let pair: Vec<StableNodeId> = graph
        .nodes()
        .filter(|node| node.operation.0 == "quantum.h")
        .filter_map(|node| graph.stable_id(node.id))
        .collect();
    assert_eq!(pair.len(), 2);
    let facts = OperationFacts {
        class: OperationClass::Unitary,
        resources: BTreeSet::from([ResourceId(0)]),
        inverse_of: Some("quantum.h".to_string()),
        global_phase: GlobalPhasePolicy::Exact,
        ..OperationFacts::default()
    };
    let context = RewriteContext {
        operation_facts: pair.iter().map(|node| (*node, facts.clone())).collect(),
        ..RewriteContext::default()
    };
    let outcome = GraphRewriteDriver::new(vec![Box::new(InverseCancellation)], 4)
        .run(
            graph,
            &context,
            &mut qsc_qhdg::DischargeContext::new(crate::pass_pipeline::circuit_inverses()),
        )
        .expect("driver runs");
    assert!(
        matches!(
            outcome,
            GraphRewriteOutcome::Committed {
                applications: 1,
                ..
            }
        ),
        "expected the adjacent pair to cancel: {outcome:?}"
    );
}

/// Rebuilds a graph with one operation renamed, standing in for a pass that
/// produced a construct no target form exists for.
fn rebuild_renaming(graph: &Graph, from: &str, to: &str) -> Graph {
    use std::collections::BTreeMap;

    let mut builder = GraphBuilder::new();
    let region = builder.add_region(None, RegionKind::ModuleOmega);
    let mut ports = BTreeMap::new();
    let mut rebuilt_nodes = Vec::new();
    for node in graph.nodes() {
        let name = if node.operation.0 == from {
            to.to_string()
        } else {
            node.operation.0.clone()
        };
        let rebuilt = builder
            .add_node(region, name, node.effects.clone())
            .expect("region exists");
        // A pass keeps what a node carries and does not claim its provenance.
        builder
            .declare_payload(
                rebuilt,
                qsc_qhdg::NodePayload {
                    value: node.payload.value.clone(),
                    origin: None,
                },
            )
            .expect("node exists");
        rebuilt_nodes.push((node.id, rebuilt));
        for old in node.inputs.iter().chain(node.outputs.iter()) {
            let port = graph.ports().find(|p| p.id == *old).expect("port exists");
            let new = builder
                .add_port(
                    rebuilt,
                    port.direction,
                    port.kind,
                    port.ty.clone(),
                    port.stage,
                    port.usage,
                )
                .expect("node exists");
            ports.insert(*old, new);
        }
    }
    for edge in graph.edges() {
        builder.connect_unchecked(qsc_qhdg::Edge {
            source: ports[&edge.source],
            target: ports[&edge.target],
            kind: edge.kind,
        });
    }
    for (old, new) in rebuilt_nodes {
        let node = graph.nodes().find(|n| n.id == old).expect("node exists");
        if node.effects.iter().any(|effect| effect.observable) {
            builder.demand(new);
        }
    }
    builder.finish()
}
