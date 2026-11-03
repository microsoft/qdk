// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::{BTreeMap, BTreeSet};

use qsc_qhdg::{
    Edge, EdgeKind, EffectMode, PortKind, RegionId, RegionKind, StableNodeId, verify, vocabulary,
};
use qsc_rir::builder;
use qsc_rir::rir::{
    Block, BlockId, Callable, CallableId, CallableType, ConditionCode, Instruction, Literal,
    Operand, Prim, Program, Ty, Variable, VariableId,
};

use super::return_leg::{emit, emitted_exactly_once};
use super::{
    Error, IdentityCensus, RetireError, block_order, emitted_names, graph_to_program,
    lower_with_census, program_order, program_to_graph, program_to_graph_with_sites, unrepresented,
};
use crate::circuit_qhdg::{ReadItem, qubit_operands, read_program};
use crate::graph_order::{region_dependencies, region_orders, region_orders_with};
use crate::pass_pipeline::{Cancellation, cancel_inverse_pairs};

/// A program that measures one qubit and acts on another only if it was set.
///
/// This is the shape the phase exists to accept: a straight-line prefix, a
/// branch whose predicate is a read result, two arms, and a join.
fn result_conditional_program() -> Program {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::h_decl());
    program.callables.insert(CallableId(1), builder::m_decl());
    program
        .callables
        .insert(CallableId(2), builder::read_result_decl());
    program.callables.insert(CallableId(3), builder::x_decl());
    program.callables.insert(
        CallableId(4),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(4);

    let condition = Variable {
        variable_id: VariableId(0),
        ty: Ty::Prim(Prim::Boolean),
    };
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(0),
                vec![Operand::Literal(Literal::Qubit(0))],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(1),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::Result(0))],
                Some(condition),
                None,
            ),
            Instruction::Branch(condition, BlockId(1), BlockId(2), None),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Call(
                CallableId(3),
                vec![Operand::Literal(Literal::Qubit(1))],
                None,
                None,
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program
        .blocks
        .insert(BlockId(2), Block(vec![Instruction::Jump(BlockId(3))]));
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Return(None)]));

    program.num_qubits = 2;
    program.num_results = 1;
    program
}

#[test]
fn the_rir_producer_adds_nothing_to_the_shared_vocabulary() {
    vocabulary::accept_producer(emitted_names())
        .expect("every name this producer emits is in the shared vocabulary");
}

#[test]
fn a_straight_line_program_lowers_to_a_graph_that_verifies() {
    let graph = program_to_graph(&builder::bell_program()).expect("the program should lower");
    assert!(
        verify(&graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&graph)
    );
}

#[test]
fn a_reused_qubit_becomes_a_chain_rather_than_a_repeated_name() {
    let graph = program_to_graph(&builder::bell_program()).expect("the program should lower");
    for port in graph.ports() {
        if port.kind != PortKind::Capability {
            continue;
        }
        let uses = graph
            .edges()
            .iter()
            .filter(|edge| edge.source == port.id && edge.kind == EdgeKind::AffineCapability)
            .count();
        assert!(
            uses <= 1,
            "a capability was consumed more than once: {port:?}"
        );
    }
}

#[test]
fn a_result_conditional_branch_produces_a_gamma_with_an_owner() {
    let graph = program_to_graph(&result_conditional_program()).expect("the program should lower");
    assert!(
        verify(&graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&graph)
    );

    let arms: Vec<_> = graph
        .regions()
        .filter(|region| region.kind == RegionKind::Gamma)
        .collect();
    assert_eq!(arms.len(), 2, "a two-armed branch produces two regions");
    for arm in &arms {
        assert!(
            arm.owner.is_some(),
            "an unowned branch region is refused, so each arm names its gamma"
        );
    }
    assert_eq!(
        arms[0].owner, arms[1].owner,
        "both arms are owned by the same gamma"
    );

    // Without this the test would pass on a lowering that flattened the arms
    // into the module region and produced two gammas that owned nothing.
    let guarded = graph
        .nodes()
        .find(|node| node.operation.0 == "quantum.x")
        .expect("the then-arm applies x");
    assert!(
        arms.iter().any(|arm| arm.id == guarded.region),
        "the guarded gate sits inside an arm rather than beside the branch"
    );
    let carried = graph
        .nodes()
        .filter(|node| node.operation.0 == vocabulary::CARRY)
        .count();
    assert_eq!(
        carried, 1,
        "the arm that never touches the carried qubit hands it through"
    );
}

#[test]
fn the_gamma_reads_its_predicate_from_the_measurement_that_produced_it() {
    let graph = program_to_graph(&result_conditional_program()).expect("the program should lower");
    let gamma = graph
        .nodes()
        .find(|node| node.operation.0 == vocabulary::GAMMA)
        .expect("the branch produces a gamma");
    let selector = graph
        .ports()
        .find(|port| port.node == gamma.id && port.kind == PortKind::Value)
        .expect("the gamma reads a selector");
    let feeding: Vec<_> = graph
        .edges()
        .iter()
        .filter(|edge| edge.target == selector.id)
        .collect();
    assert_eq!(feeding.len(), 1, "the selector has exactly one source");
    assert_eq!(feeding[0].kind, EdgeKind::CopyableValue);

    let source = graph
        .port(feeding[0].source)
        .expect("the selector's source is a port");
    let producer = graph
        .nodes()
        .find(|node| node.id == source.node)
        .expect("the source belongs to a node");
    assert_eq!(
        producer.operation.0, "quantum.measure_z",
        "the predicate comes from the measurement rather than from a bit index"
    );
}

#[test]
fn a_back_edge_is_refused_rather_than_followed() {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::h_decl());
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(1);
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(0),
                vec![Operand::Literal(Literal::Qubit(0))],
                None,
                None,
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program
        .blocks
        .insert(BlockId(1), Block(vec![Instruction::Jump(BlockId(0))]));
    program.num_qubits = 1;

    assert_eq!(program_to_graph(&program), Err(Error::Unstructured));
}

#[test]
fn a_variable_qubit_operand_is_refused_rather_than_guessed() {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::h_decl());
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(1);
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(0),
                vec![Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Qubit),
                })],
                None,
                None,
            ),
            Instruction::Return(None),
        ]),
    );
    program.num_qubits = 1;

    assert_eq!(program_to_graph(&program), Err(Error::DynamicQubit));
}

/// A program whose one instruction both measures a qubit and resets it.
fn mresetz_program() -> Program {
    let mut program = Program::default();
    program
        .callables
        .insert(CallableId(0), builder::mresetz_decl());
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(1);
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(0),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Return(None),
        ]),
    );
    program.num_qubits = 1;
    program.num_results = 1;
    program
}

#[test]
fn measure_and_reset_is_one_node_carrying_both_effects() {
    let graph = program_to_graph(&mresetz_program()).expect("the program should lower");
    assert!(
        verify(&graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&graph)
    );

    let measured: Vec<_> = graph
        .nodes()
        .filter(|node| node.operation.0 == "quantum.measure_reset_z")
        .collect();
    assert_eq!(
        measured.len(),
        1,
        "one instruction becomes one node, which is what the identity round trip rests on"
    );

    // The reset is the half that dispatching on call type alone would have
    // dropped, so both halves are asserted rather than just the count.
    let node = measured[0];
    assert!(
        node.effects
            .iter()
            .any(|effect| effect.observable && effect.mode == EffectMode::Irreversible),
        "the measurement half is observable: {:?}",
        node.effects
    );
    assert!(
        node.effects
            .iter()
            .any(|effect| !effect.observable && effect.mode == EffectMode::Irreversible),
        "the reset half is irreversible without being observable: {:?}",
        node.effects
    );
    assert_eq!(
        graph
            .nodes()
            .filter(|node| node.operation.0 == "quantum.measure_z")
            .count(),
        0,
        "it is not lowered as a plain measurement"
    );
}

#[test]
fn an_unmapped_measurement_is_refused_by_name() {
    let mut program = Program::default();
    let mut unmapped = builder::mresetz_decl();
    unmapped.name = "__quantum__qis__mresetx__body".to_string();
    program.callables.insert(CallableId(0), unmapped);
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(1);
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(0),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Return(None),
        ]),
    );
    program.num_qubits = 1;
    program.num_results = 1;

    // A measurement variant the vocabulary has no term for does not silently
    // become a plain measurement and lose whatever else it does.
    assert_eq!(
        program_to_graph(&program),
        Err(Error::UnsupportedCallable {
            name: "__quantum__qis__mresetx__body".to_string()
        })
    );
}

#[test]
fn the_measurement_that_does_have_a_term_still_lowers() {
    let graph = program_to_graph(&builder::bell_program()).expect("the program should lower");
    let measured = graph
        .nodes()
        .filter(|node| node.operation.0 == "quantum.measure_z")
        .count();
    assert_eq!(
        measured, 2,
        "the refusal is specific to the unmapped name, not to measurement"
    );
}

#[test]
fn a_bodied_callable_that_returns_a_value_is_refused() {
    let program = builder::two_body_program_with_branch();
    assert!(
        matches!(
            program_to_graph(&program),
            Err(Error::UnsupportedCallable { .. })
        ),
        "a value returned from a body would have to be threaded back to whatever reads it"
    );
}

/// A program shaped the way the Adaptive profile emits one.
///
/// Every gate call there goes through an IR-function wrapper whose body names
/// the gate's qubit as a parameter, so the entry block passes a literal and the
/// body sees a variable. `calls` is how many times the entry calls the wrapper.
fn wrapped_gate_program(calls: usize) -> Program {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::h_decl());
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "H".to_string(),
            input_type: vec![Ty::Prim(Prim::Qubit)],
            input_vars: vec![VariableId(0)],
            output_type: None,
            body: Some(BlockId(1)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(2);
    program.blocks.insert(
        BlockId(0),
        Block(
            (0..calls)
                .map(|_| {
                    Instruction::Call(
                        CallableId(1),
                        vec![Operand::Literal(Literal::Qubit(0))],
                        None,
                        None,
                    )
                })
                .chain([Instruction::Return(None)])
                .collect(),
        ),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Call(
                CallableId(0),
                vec![Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Qubit),
                })],
                None,
                None,
            ),
            Instruction::Return(None),
        ]),
    );
    program.num_qubits = 1;
    program
}

#[test]
fn a_call_into_a_body_lowers_the_gate_the_body_names() {
    // The Adaptive profile wraps every operation like this, so refusing a
    // bodied callable refuses the profile rather than an unusual program.
    let graph = program_to_graph(&wrapped_gate_program(1)).expect("the wrapped call should lower");
    assert!(
        verify(&graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&graph)
    );
    assert_eq!(
        graph
            .nodes()
            .filter(|node| node.operation.0 == "quantum.h")
            .count(),
        1,
        "the gate inside the body reached the graph"
    );
}

#[test]
fn a_parameter_resolves_to_the_wire_the_call_site_passed() {
    let (lowered, census) = lower_with_census(&wrapped_gate_program(1));
    assert!(lowered.is_ok(), "the wrapped call should lower");
    assert_eq!(
        census,
        IdentityCensus {
            resource: 1,
            binding: 0,
            opaque: 0
        },
        "the body names a parameter, and the call site is what says which wire it holds"
    );
}

#[test]
fn two_calls_into_one_body_are_two_nodes_sited_at_their_own_call() {
    let lowered =
        program_to_graph_with_sites(&wrapped_gate_program(2)).expect("the program should lower");
    let gates: Vec<_> = lowered
        .graph
        .nodes()
        .filter(|node| node.operation.0 == "quantum.h")
        .filter_map(|node| lowered.graph.stable_id(node.id))
        .collect();
    assert_eq!(gates.len(), 2, "one node per call, not one per body");

    let sites: BTreeSet<_> = gates
        .iter()
        .filter_map(|id| lowered.sites.get(id))
        .collect();
    assert_eq!(
        sites.len(),
        2,
        "two calls sharing one site would make retiring either one delete both"
    );
    assert!(
        sites.iter().all(|site| site.block == BlockId(0)),
        "the site is the call the entry block holds, not the body instruction the two calls share"
    );
}

#[test]
fn retiring_a_gate_lowered_from_a_body_removes_the_call_and_its_wrapper() {
    let program = wrapped_gate_program(2);
    let mut lowered = program_to_graph_with_sites(&program).expect("the program should lower");
    retire_gate(&mut lowered.graph, "quantum.h");
    retire_gate(&mut lowered.graph, "quantum.h");

    let returned = graph_to_program(&program, &lowered.graph, &lowered)
        .expect("a retirement inside a body carries back");
    assert_eq!(
        returned.get_block(BlockId(0)).0.len(),
        1,
        "both calls left, and only the return remains"
    );
    assert!(
        returned.callables.get(CallableId(1)).is_none(),
        "the wrapper lost its last caller, so its declaration cannot stay"
    );
    assert!(
        returned.blocks.get(BlockId(1)).is_none(),
        "the wrapper's body is unreachable, which the program is not allowed to hold"
    );
}

#[test]
fn a_callable_that_reaches_itself_is_refused_rather_than_walked_forever() {
    let mut program = Program::default();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "recurse".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(1)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(1);
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(CallableId(0), Vec::new(), None, None),
            Instruction::Return(None),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Call(CallableId(0), Vec::new(), None, None),
            Instruction::Return(None),
        ]),
    );

    assert_eq!(
        program_to_graph(&program),
        Err(Error::RecursiveCallable {
            name: "recurse".to_string()
        })
    );
}

/// A program whose gate names a variable qubit, assigned `stores` times.
fn stored_qubit_program(stores: usize) -> Program {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::h_decl());
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(1);
    let held = Variable {
        variable_id: VariableId(0),
        ty: Ty::Prim(Prim::Qubit),
    };
    program.blocks.insert(
        BlockId(0),
        Block(
            (0..stores)
                .map(|_| Instruction::Store(Operand::Literal(Literal::Qubit(0)), held))
                .chain([
                    Instruction::Call(CallableId(0), vec![Operand::Variable(held)], None, None),
                    Instruction::Return(None),
                ])
                .collect(),
        ),
    );
    program.num_qubits = 1;
    program
}

#[test]
fn a_variable_the_program_assigns_once_is_a_binding() {
    let (lowered, census) = lower_with_census(&stored_qubit_program(1));
    assert_eq!(
        census,
        IdentityCensus {
            resource: 0,
            binding: 1,
            opaque: 0
        }
    );
    // A binding is honest and still does not resolve, so the operation is
    // refused rather than lowered against a claim the producer cannot make.
    assert_eq!(lowered.err(), Some(Error::DynamicQubit));
}

#[test]
fn a_variable_the_program_assigns_twice_is_opaque_rather_than_a_binding() {
    // The obligation on a binding is that it holds one wire over the whole span
    // a caller compares across. A second store is exactly what withdraws that,
    // and the insertion point is before the SSA transform, so it can happen.
    let (lowered, census) = lower_with_census(&stored_qubit_program(2));
    assert_eq!(
        census,
        IdentityCensus {
            resource: 0,
            binding: 0,
            opaque: 1
        }
    );
    assert_eq!(lowered.err(), Some(Error::DynamicQubit));
}

#[test]
fn a_straight_line_graph_from_this_producer_reads_back_without_an_origin_branch() {
    let graph = program_to_graph(&builder::bell_program()).expect("the program should lower");
    let read = read_program(&graph).expect("the reader accepts the new origin as it stands");

    assert_eq!(read.qubit_count, 2);
    assert_eq!(read.clbit_count, 2);
    let steps: Vec<(&str, &[u32], &[u32])> = read
        .items
        .iter()
        .map(|item| match item {
            ReadItem::Instruction(instruction) => (
                instruction.operation.as_str(),
                instruction.qubits.as_slice(),
                instruction.clbits.as_slice(),
            ),
            ReadItem::Conditional(_) => panic!("a straight-line program has no conditional"),
            ReadItem::Loop(_) => panic!("a straight-line program has no loop"),
        })
        .collect();
    assert_eq!(
        steps,
        vec![
            ("quantum.h", &[0u32][..], &[][..]),
            ("quantum.cx", &[0, 1][..], &[][..]),
            ("quantum.measure_z", &[0][..], &[0][..]),
            ("quantum.measure_z", &[1][..], &[1][..]),
        ],
        "the reader resolves the operands the producer wrote rather than renumbering them"
    );
}

#[test]
fn a_new_origin_conditional_reads_back_as_the_conditional_it_is() {
    let graph = program_to_graph(&result_conditional_program()).expect("the program should lower");
    let read = read_program(&graph).expect("the reader accepts the new origin as it stands");

    let [
        ReadItem::Instruction(h),
        ReadItem::Instruction(measure),
        ReadItem::Conditional(conditional),
    ] = read.items.as_slice()
    else {
        panic!("expected a gate, a measurement, and a conditional: {read:?}");
    };
    assert_eq!(h.operation, "quantum.h");
    assert_eq!(measure.operation, "quantum.measure_z");
    assert_eq!(measure.clbits, vec![0]);
    assert_eq!(
        conditional.clbit, 0,
        "the branch selects on the bit the measurement wrote, not on a fresh number"
    );

    // The carry the producer emits so both arms produce the same things is
    // interface bookkeeping, so the arm that only carries reads back empty
    // rather than as a step of the program.
    let [ReadItem::Instruction(guarded)] = conditional.then_items.as_slice() else {
        panic!("the then-arm holds one gate: {:?}", conditional.then_items);
    };
    assert_eq!(
        (guarded.operation.as_str(), guarded.qubits.as_slice()),
        ("quantum.x", &[1u32][..])
    );
    assert_eq!(
        conditional.else_items,
        Some(Vec::new()),
        "an arm that carries and nothing else is empty, which is not the same program as no arm"
    );
}

#[test]
fn measure_and_reset_reads_back_as_one_instruction_holding_its_bit() {
    let graph = program_to_graph(&mresetz_program()).expect("the program should lower");
    let read = read_program(&graph).expect("the reader accepts the new origin as it stands");

    // The circuit producer never emits this combined term, so this checks
    // that the shared reader also accepts the RIR vocabulary.
    let [ReadItem::Instruction(instruction)] = read.items.as_slice() else {
        panic!("expected one instruction: {read:?}");
    };
    assert_eq!(instruction.operation, "quantum.measure_reset_z");
    assert_eq!(instruction.qubits, vec![0]);
    assert_eq!(instruction.clbits, vec![0]);
}

#[test]
fn the_reader_reports_the_same_operands_the_walk_resolved() {
    let graph = program_to_graph(&result_conditional_program()).expect("the program should lower");
    let operands = qubit_operands(&graph).expect("the reader accepts the new origin as it stands");

    // A pass reasoning about qubits reaches the graph through this rather than
    // through `read_program`, and a nested arm is where the two could diverge
    // without anything failing.
    let guarded = graph
        .nodes()
        .find(|node| node.operation.0 == "quantum.x")
        .expect("the then-arm applies x");
    let stable = graph
        .stable_id(guarded.id)
        .expect("the guarded node has a durable identity");
    assert_eq!(operands.get(&stable), Some(&vec![1]));
}

/// A program carrying an angle small enough that RIR's own `PartialEq` cannot
/// tell it from zero.
///
/// `Literal::Double` compares with an absolute `f64::EPSILON`, so `1e-20` and
/// `0.0` are equal under it. Any test that asserted round-trip fidelity through
/// `==` would therefore pass on a return leg that had zeroed the angle.
fn small_angle_program() -> Program {
    const TINY: f64 = 1e-20;
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::rx_decl());
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(1);
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(0),
                vec![
                    Operand::Literal(Literal::Double(TINY)),
                    Operand::Literal(Literal::Qubit(0)),
                ],
                None,
                None,
            ),
            Instruction::Return(None),
        ]),
    );
    program.num_qubits = 1;
    program
}

/// Every program this stage accepts, for the round trip to run over.
fn corpus() -> Vec<(&'static str, Program)> {
    vec![
        ("bell", builder::bell_program()),
        ("result-conditional", result_conditional_program()),
        ("measure-and-reset", mresetz_program()),
        ("small-angle", small_angle_program()),
        ("repeated-store", repeated_store_program()),
        ("read-then-write", read_then_write_program()),
        ("disjoint-variables", disjoint_variable_program()),
        ("wrapped-gate", wrapped_gate_program(1)),
        ("wrapped-gate-twice", wrapped_gate_program(2)),
    ]
}

#[test]
fn the_identity_round_trip_is_byte_identical_across_the_corpus() {
    // Compare rendered text before enabling any rule, not `PartialEq`, which compares
    // doubles with an absolute epsilon and would call a corrupted angle equal.
    for (name, program) in corpus() {
        let lowered = program_to_graph_with_sites(&program).expect("the program should lower");
        let returned = graph_to_program(&program, &lowered.graph, &lowered)
            .expect("an untouched graph carries back");
        assert_eq!(
            returned.to_string(),
            program.to_string(),
            "{name} did not return byte-identical"
        );
    }
}

/// Every loop shape this stage accepts, kept apart from `corpus` on purpose.
///
/// The reach of loop support is `while` and `repeat-until` only: a `for` loop
/// compiles to a rotated shape the producer refuses by name, so a corpus that
/// blurred the two would overstate what the round trip has been run over.
fn loop_corpus() -> Vec<(&'static str, Program)> {
    vec![
        ("while", looping_program(false)),
        ("branch-inside-a-loop", branching_loop_program()),
        ("loop-carried-value", loop_carried_value_program()),
        ("header-condition", header_condition_loop_program()),
        (
            "header-empty-bodied-call",
            header_empty_bodied_call_loop_program(),
        ),
    ]
}

/// An untouched loop-bearing graph returns the exact source program,
/// independently of rewrite results.
#[test]
fn the_identity_round_trip_is_byte_identical_over_a_loop_bearing_corpus() {
    for (name, program) in loop_corpus() {
        let lowered = program_to_graph_with_sites(&program).expect("the program should lower");
        let returned = graph_to_program(&program, &lowered.graph, &lowered)
            .expect("an untouched graph carries back");
        assert_eq!(
            returned.to_string(),
            program.to_string(),
            "{name} did not return byte-identical"
        );
    }
}

#[test]
fn the_round_trip_is_not_byte_identical_by_declining_to_do_anything() {
    // The identity result above would look the same if the return leg copied
    // its input and never consulted the graph. This is what distinguishes the
    // two: with one node retired the output differs, so the pass through the
    // graph is load-bearing.
    let program = builder::bell_program();
    let mut lowered = program_to_graph_with_sites(&program).expect("the program should lower");
    retire_gate(&mut lowered.graph, "quantum.h");

    let returned =
        graph_to_program(&program, &lowered.graph, &lowered).expect("a retirement carries back");
    assert_ne!(returned.to_string(), program.to_string());
}

#[test]
fn a_double_survives_the_round_trip_bit_exactly() {
    let program = small_angle_program();
    let lowered = program_to_graph_with_sites(&program).expect("the program should lower");
    let returned = graph_to_program(&program, &lowered.graph, &lowered)
        .expect("an untouched graph carries back");

    let angle = |program: &Program| -> u64 {
        let Instruction::Call(_, args, ..) = &program.get_block(BlockId(0)).0[0] else {
            panic!("the first instruction is the rotation");
        };
        let Some(Operand::Literal(Literal::Double(value))) = args.first() else {
            panic!("the rotation carries a double");
        };
        value.to_bits()
    };

    // Bit pattern rather than `==`, which is the comparison the epsilon would
    // have made meaningless at this magnitude.
    assert_eq!(angle(&returned), angle(&program));
    assert_ne!(
        angle(&program),
        0.0f64.to_bits(),
        "the fixture is only meaningful while the angle is not itself zero"
    );
}

#[test]
fn a_retired_gate_leaves_the_program_and_nothing_else_does() {
    let program = builder::bell_program();
    let mut lowered = program_to_graph_with_sites(&program).expect("the program should lower");
    retire_gate(&mut lowered.graph, "quantum.h");

    // The retirement re-threaded the capability, so the graph is still a graph
    // rather than one with a hole in it. A return leg fed a broken graph would
    // otherwise be tested against a state no rewrite produces.
    assert!(
        verify(&lowered.graph).is_empty(),
        "expected a clean graph after retirement: {:?}",
        verify(&lowered.graph)
    );

    let returned =
        graph_to_program(&program, &lowered.graph, &lowered).expect("a retirement carries back");

    let hadamard = program
        .callables
        .iter()
        .find(|(_, callable)| callable.name == "__quantum__qis__h__body")
        .expect("the program calls h")
        .0;
    let calls_hadamard = |instruction: &Instruction| matches!(instruction, Instruction::Call(callable, ..) if *callable == hadamard);

    let before = &program.get_block(BlockId(0)).0;
    let after = &returned.get_block(BlockId(0)).0;
    assert_eq!(
        before.iter().filter(|i| calls_hadamard(i)).count(),
        1,
        "the fixture holds exactly one h to retire"
    );
    assert_eq!(
        after.iter().filter(|i| calls_hadamard(i)).count(),
        0,
        "the retired gate is gone from the program"
    );

    // Every other instruction survived in order and unchanged, which is the
    // half that separates a retirement from a rewrite.
    let expected: Vec<String> = before
        .iter()
        .filter(|instruction| !calls_hadamard(instruction))
        .map(ToString::to_string)
        .collect();
    let actual: Vec<String> = after.iter().map(ToString::to_string).collect();
    assert_eq!(actual, expected);

    // Nothing but the block body moved, which is what retire-only means.
    assert_eq!(returned.num_qubits, program.num_qubits);
    assert_eq!(returned.num_results, program.num_results);
    assert!(
        returned
            .callables
            .iter()
            .all(|(_, callable)| callable.name != "__quantum__qis__h__body"),
        "the declaration whose last caller left goes with it, because RIR requires every callable to be reachable from the entry"
    );
    assert_eq!(
        returned.callables.iter().count(),
        program.callables.iter().count() - 1,
        "only that one declaration left"
    );
}

#[test]
fn a_node_the_lowering_did_not_produce_is_refused_rather_than_ignored() {
    let program = builder::bell_program();
    let mut lowered = program_to_graph_with_sites(&program).expect("the program should lower");
    let region = lowered
        .graph
        .regions()
        .find(|region| region.kind == RegionKind::ModuleOmega)
        .expect("the graph has a module region")
        .id;
    lowered
        .graph
        .add_node(region, "quantum.x", Vec::new())
        .expect("a node can be added");

    // The return leg only removes. A node a pass introduced has no instruction
    // to become, and carrying on would drop it silently.
    assert_eq!(
        graph_to_program(&program, &lowered.graph, &lowered).err(),
        Some(RetireError::Introduced)
    );
}

#[test]
fn every_instruction_that_produced_a_node_is_named_in_the_site_table() {
    // An instruction missing from this table can never be retired, and nothing
    // else in the round trip would report that: the identity case would still
    // be byte-identical and the retirement case tests a different instruction.
    let program = builder::bell_program();
    let lowered = program_to_graph_with_sites(&program).expect("the program should lower");

    let named: BTreeSet<usize> = lowered
        .sites
        .values()
        .filter(|site| site.block == BlockId(0))
        .map(|site| site.index)
        .collect();
    // Every instruction the graph represents has a site. The represented set is
    // every non-terminator instruction in a caller-visible block: a terminator
    // is split off before lowering and a block cannot lose one, so it is
    // carried by position instead.
    let expected: BTreeSet<usize> = program
        .get_block(BlockId(0))
        .0
        .iter()
        .enumerate()
        .filter(|(_, instruction)| {
            !matches!(
                instruction,
                Instruction::Jump(..) | Instruction::Branch(..) | Instruction::Return(..)
            )
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(named, expected, "every represented instruction has a site");
    assert!(!named.is_empty(), "the table is populated at all");

    // A measurement becomes a node plus a store, and both name the one
    // instruction rather than the store being orphaned.
    assert!(
        lowered.sites.len() > named.len(),
        "at least one instruction produced more than one node"
    );
}

#[test]
fn a_gate_parameter_does_not_reach_the_graph() {
    // Pinned rather than fixed. This producer threads qubits and drops every
    // other operand, so a rotation reaches the graph as a bare `quantum.rx`
    // with no angle. The round trip is unharmed, because the return leg copies
    // the instruction rather than rebuilding it from the node, which is the
    // same property that makes doubles exact. But a rewrite reading this graph
    // sees every rotation as identical, so no angle-aware rule can be enabled
    // until the parameter is carried. Recorded as a follow-up.
    let graph = program_to_graph(&small_angle_program()).expect("the program should lower");
    let read = read_program(&graph).expect("the graph reads back");
    let [ReadItem::Instruction(rotation)] = read.items.as_slice() else {
        panic!("expected one instruction: {read:?}");
    };
    assert_eq!(rotation.operation, "quantum.rx");
    assert!(
        rotation.params.is_empty(),
        "the angle is absent from the graph, which is the gap this pins: {:?}",
        rotation.params
    );
}

/// Retires the first node with this operation, re-threading its capability the
/// way a rewrite would.
fn retire_gate(graph: &mut qsc_qhdg::Graph, operation: &str) {
    let node = graph
        .nodes()
        .find(|node| node.operation.0 == operation)
        .expect("the program holds this gate")
        .id;
    let (inputs, outputs) = {
        let node = graph.nodes().find(|found| found.id == node).expect("live");
        (node.inputs.clone(), node.outputs.clone())
    };
    let capability = |graph: &qsc_qhdg::Graph, ports: &[qsc_qhdg::PortId]| {
        ports
            .iter()
            .copied()
            .find(|port| {
                graph
                    .port(*port)
                    .is_some_and(|port| port.kind == PortKind::Capability)
            })
            .expect("a gate holds a capability")
    };
    let input = capability(graph, &inputs);
    let output = capability(graph, &outputs);
    let source = graph
        .edges()
        .iter()
        .find(|edge| edge.target == input)
        .expect("the capability came from somewhere")
        .source;
    let target = graph
        .edges()
        .iter()
        .find(|edge| edge.source == output)
        .expect("the capability went somewhere")
        .target;

    graph.retire_node(node).expect("the node was live");
    graph
        .connect(Edge {
            source,
            target,
            kind: EdgeKind::AffineCapability,
        })
        .expect("the wire rejoins");
}

/// The block shape partial evaluation emits for a `while` at the Adaptive
/// profile: a preheader that jumps to the condition, a condition that branches
/// to the body or past it, and a body that jumps back to the condition.
///
/// `header_operation` puts a gate in the condition block, which is the case the
/// stage has to refuse: the condition runs once more than the body does, so
/// nothing this stage can build repeats it the right number of times.
fn looping_program(header_operation: bool) -> Program {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::h_decl());
    program.callables.insert(CallableId(1), builder::x_decl());
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(2);

    let condition = Variable {
        variable_id: VariableId(0),
        ty: Ty::Prim(Prim::Boolean),
    };
    let gate = |callable: CallableId| {
        Instruction::Call(
            callable,
            vec![Operand::Literal(Literal::Qubit(0))],
            None,
            None,
        )
    };
    program.blocks.insert(
        BlockId(0),
        Block(vec![gate(CallableId(0)), Instruction::Jump(BlockId(1))]),
    );
    let mut header = Vec::new();
    if header_operation {
        header.push(gate(CallableId(0)));
    }
    header.push(Instruction::Branch(condition, BlockId(2), BlockId(3), None));
    program.blocks.insert(BlockId(1), Block(header));
    program.blocks.insert(
        BlockId(2),
        Block(vec![gate(CallableId(1)), Instruction::Jump(BlockId(1))]),
    );
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Return(None)]));

    program.num_qubits = 1;
    program
}

/// The loop's back-edge declaration must verify in a producer-emitted graph,
/// not only in a hand-built fixture.
#[test]
fn a_loop_becomes_a_theta_that_verifies() {
    let graph = program_to_graph(&looping_program(false)).expect("a loop lowers");
    let theta = graph
        .regions()
        .find(|region| region.kind == RegionKind::Theta)
        .expect("the loop became a theta region");
    assert!(
        theta.owner.is_some(),
        "a body no node owns is a region nothing enters"
    );
    assert!(
        theta
            .interface
            .exits
            .iter()
            .any(|(kind, _)| *kind == qsc_qhdg::ExitKind::Backedge),
        "a body that declares no back edge says nothing about repeating"
    );
    assert!(
        theta
            .interface
            .bindings
            .iter()
            .any(|binding| binding.role == qsc_qhdg::InterfaceRole::Carried),
        "a loop over a qubit carries that qubit across its back edge"
    );
    assert!(
        verify(&graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&graph)
    );
}

/// The producer's output and the reader's acceptance are two halves of one
/// claim, and until now only the second half had ever been exercised.
#[test]
fn the_theta_a_producer_emits_reads_back_as_a_loop() {
    let graph = program_to_graph(&looping_program(false)).expect("a loop lowers");
    let read = read_program(&graph).expect("the reader accepts what the producer emitted");

    let [ReadItem::Instruction(before), ReadItem::Loop(repeating)] = read.items.as_slice() else {
        panic!("expected a gate then a loop, got {:?}", read.items);
    };
    assert_eq!(before.operation, "quantum.h");
    assert_eq!(
        repeating.carried,
        vec![0],
        "the wire the loop carries is the qubit the body acts on"
    );
    let body: Vec<&str> = repeating
        .body
        .iter()
        .map(|item| match item {
            ReadItem::Instruction(instruction) => instruction.operation.as_str(),
            other => panic!("a body of one gate read back as {other:?}"),
        })
        .collect();
    assert_eq!(body, ["quantum.x"]);
}

/// The `while` shape with a cancelling pair bracketed inside its body, which is
/// the one position a retirement can take without holding a port the loop's own
/// contract names.
fn loop_with_an_interior_pair() -> Program {
    let mut program = looping_program(false);
    program.callables.insert(CallableId(3), builder::z_decl());
    let gate = |callable: u32| {
        Instruction::Call(
            CallableId(callable),
            vec![Operand::Literal(Literal::Qubit(0))],
            None,
            None,
        )
    };
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            gate(0),
            gate(1),
            gate(1),
            gate(3),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program
}

/// A retirement must preserve the loop interface before the return leg can
/// carry the edited body back into a program.
#[test]
fn a_retirement_inside_a_loop_body_leaves_the_carried_interface_intact() {
    let program = loop_with_an_interior_pair();
    let mut lowered = program_to_graph_with_sites(&program).expect("the program should lower");
    assert_eq!(
        cancel_inverse_pairs(&mut lowered.graph),
        Ok(Cancellation::Changed),
        "the interior pair should cancel"
    );
    assert!(
        verify(&lowered.graph).is_empty(),
        "expected a clean graph after retirement: {:?}",
        verify(&lowered.graph)
    );

    let theta = lowered
        .graph
        .regions()
        .find(|region| region.kind == RegionKind::Theta)
        .expect("the loop is still a theta");
    let named: Vec<qsc_qhdg::PortId> = theta
        .interface
        .exits
        .iter()
        .map(|(_, port)| *port)
        .chain(theta.interface.bindings.iter().map(|binding| binding.inner))
        .collect();
    assert!(
        theta
            .interface
            .exits
            .iter()
            .any(|(kind, _)| *kind == qsc_qhdg::ExitKind::Backedge),
        "the back edge is still declared"
    );
    assert!(
        theta
            .interface
            .bindings
            .iter()
            .any(|binding| binding.role == qsc_qhdg::InterfaceRole::Carried),
        "the carried wire is still declared"
    );
    // The half that would have failed silently. A binding whose port no node
    // holds is still a binding, so counting roles proves nothing on its own.
    for port in named {
        assert!(
            lowered.graph.port(port).is_some(),
            "the interface names port {port:?}, which the retirement took"
        );
    }

    let returned = graph_to_program(&program, &lowered.graph, &lowered)
        .expect("a retirement inside a loop carries back");
    assert_ne!(returned.to_string(), program.to_string());
}

/// The boundary of the accepted class, stated as a test rather than as a note.
#[test]
fn a_loop_whose_condition_holds_an_operation_is_refused() {
    assert_eq!(
        program_to_graph(&looping_program(true)),
        Err(Error::UnsupportedLoop {
            reason: "its condition holds an operation this stage would have to place".to_string(),
        })
    );
}

/// The header block of a program holding exactly one loop.
///
/// The header is the block whose branch an arm comes back to, which is the same
/// question `loop_shape` asks the producer's own way.
fn loop_header(program: &Program) -> BlockId {
    let mut found = None;
    for (id, block) in program.blocks.iter() {
        let Some(Instruction::Branch(_, then_block, else_block, _)) = block.0.last() else {
            continue;
        };
        let comes_back = [*then_block, *else_block].into_iter().any(|arm| {
            matches!(program.get_block(arm).0.last(), Some(Instruction::Jump(back)) if *back == id)
        });
        if comes_back {
            assert!(found.is_none(), "this helper assumes one loop");
            found = Some(id);
        }
    }
    found.expect("the fixture holds a loop")
}

/// The nodes one block's instructions produced, with their operations.
fn nodes_sited_in(lowered: &super::Lowered, block: BlockId) -> Vec<(StableNodeId, String)> {
    lowered
        .graph
        .nodes()
        .filter_map(|node| {
            let id = lowered.graph.stable_id(node.id)?;
            let site = lowered.sites.get(&id)?;
            (site.block == block).then(|| (id, node.operation.0.clone()))
        })
        .collect()
}

/// The first clause of the loop-header carrier exemption's precondition.
///
/// The exemption in `lower_chain` lets a `while` lower even though its header
/// nodes sit outside the region that repeats, and it is sound only while
/// nothing interprets a carrier. The guard there counts non-carrier nodes and
/// never asks what the carriers are. This asks, on producer output, because the
/// hazard is what the producer actually puts in a header.
#[test]
fn every_node_the_loop_header_exemption_admits_is_an_uninterpreted_carrier() {
    let mut rules = qsc_qhdg::graph_rewrite_rules();
    rules.push(Box::new(qsc_qhdg::DiagonalCollapse::new([
        "quantum.z",
        "quantum.cz",
    ])));
    for (name, program) in [
        ("header-condition", header_condition_loop_program()),
        (
            "header-empty-bodied-call",
            header_empty_bodied_call_loop_program(),
        ),
    ] {
        let header = loop_header(&program);
        let lowered = program_to_graph_with_sites(&program).expect("the fixture should lower");
        let admitted = nodes_sited_in(&lowered, header);
        assert!(
            !admitted.is_empty(),
            "{name} put no node in its loop header, so this asserts nothing"
        );

        let adjacency = qsc_qhdg::Adjacency::new(&lowered.graph);
        let context = qsc_qhdg::RewriteContext::default();
        for (id, operation) in admitted {
            assert_eq!(
                operation,
                vocabulary::OPAQUE,
                "{name} built '{operation}' in its loop header, which the exemption does not cover"
            );
            for rule in &rules {
                assert!(
                    rule.find(&lowered.graph, &adjacency, &context, id)
                        .expect("find runs")
                        .is_none(),
                    "'{}' matched a carrier the loop-header exemption admitted in {name}",
                    rule.name()
                );
            }
        }
    }
}

/// Where this stage's loop model departs from the RVSDG theta it is named for,
/// pinned so a change to either half is loud.
///
/// This does not endorse the shape it asserts. A theta in the specification is
/// defined by its predicate: the region produces a result that says whether to
/// repeat. This stage produces neither that result nor a region that contains
/// the condition computing it, so a header's carriers sit in the enclosing
/// region and continuation is not represented at all.
///
/// Both halves are tolerated only because nothing interprets a carrier and
/// nothing derives an iteration count from region membership, which is the
/// second clause of the loop-header exemption's precondition and the one no
/// test can hold on its own. Whoever implements the loop rotation is expected
/// to delete this test; that deletion is the decision, not an accident.
#[test]
fn the_loop_model_deviates_from_the_specification_in_two_pinned_ways() {
    let program = header_condition_loop_program();
    let header = loop_header(&program);
    let lowered = program_to_graph_with_sites(&program).expect("the fixture should lower");
    let theta = lowered
        .graph
        .regions()
        .find(|region| region.kind == RegionKind::Theta)
        .expect("the loop became a theta region");
    let enclosing = theta
        .parent
        .expect("a theta body sits inside the region its node is in");

    let admitted = nodes_sited_in(&lowered, header);
    assert!(
        !admitted.is_empty(),
        "the fixture put no node in its loop header, so the placement half asserts nothing"
    );
    for (id, _) in &admitted {
        let node = lowered
            .graph
            .nodes()
            .find(|node| lowered.graph.stable_id(node.id) == Some(*id))
            .expect("a sited node is live");
        assert_eq!(
            node.region, enclosing,
            "a loop header's carriers moved. If they now sit inside the theta, \
             the exemption in `lower_chain` no longer has anything to excuse \
             and should go with this test"
        );
    }

    // Guards the absence below against reading a region that declares nothing.
    assert!(
        !theta.interface.bindings.is_empty() && !theta.interface.exits.is_empty(),
        "a theta that declares no binding and no exit would pass the two absences vacuously"
    );
    // Not the absence of `InterfaceRole::Result`, which this producer already
    // uses for a measurement result relayed out of a loop body. A continuation
    // predicate is a boolean the region produces, and no port this theta's
    // contract names carries one.
    let named = theta.interface.exits.iter().map(|(_, port)| *port).chain(
        theta
            .interface
            .bindings
            .iter()
            .flat_map(|binding| [binding.outer, binding.inner]),
    );
    for port in named {
        let ty = &lowered
            .graph
            .port(port)
            .expect("the interface names a live port")
            .ty;
        assert_ne!(
            *ty,
            qsc_qhdg::Type::Bool,
            "the theta's contract now names a boolean. If that is the continuation \
             predicate, the loop model has become the specification's and this test \
             has served its purpose"
        );
    }
    assert!(
        !theta
            .interface
            .exits
            .iter()
            .any(|(kind, _)| *kind == qsc_qhdg::ExitKind::Branch),
        "the theta declares a branch exit, which is the other shape a continuation \
         decision could take. Re-read the loop-header exemption before keeping it"
    );
}

/// Two back edges from one branch is not a loop with two bodies. There is no
/// single region for it to be, so it refuses rather than picking one.
#[test]
fn a_branch_both_of_whose_arms_come_back_is_refused() {
    let mut program = looping_program(false);
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::Call(
                CallableId(0),
                vec![Operand::Literal(Literal::Qubit(0))],
                None,
                None,
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    assert_eq!(
        program_to_graph(&program),
        Err(Error::UnsupportedLoop {
            reason: "both of its arms come back to the condition".to_string(),
        })
    );
}

/// A loop whose body branches on a measurement it takes and rejoins before the
/// jump back to the condition.
fn branching_loop_program() -> Program {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::h_decl());
    program.callables.insert(CallableId(1), builder::m_decl());
    program
        .callables
        .insert(CallableId(2), builder::read_result_decl());
    program.callables.insert(CallableId(3), builder::x_decl());
    program.callables.insert(
        CallableId(4),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(4);

    let repeating = Variable {
        variable_id: VariableId(0),
        ty: Ty::Prim(Prim::Boolean),
    };
    let measured = Variable {
        variable_id: VariableId(1),
        ty: Ty::Prim(Prim::Boolean),
    };
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(0),
                vec![Operand::Literal(Literal::Qubit(0))],
                None,
                None,
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![Instruction::Branch(
            repeating,
            BlockId(2),
            BlockId(6),
            None,
        )]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::Result(0))],
                Some(measured),
                None,
            ),
            Instruction::Branch(measured, BlockId(3), BlockId(4), None),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::Call(
                CallableId(3),
                vec![Operand::Literal(Literal::Qubit(1))],
                None,
                None,
            ),
            Instruction::Jump(BlockId(5)),
        ]),
    );
    program
        .blocks
        .insert(BlockId(4), Block(vec![Instruction::Jump(BlockId(5))]));
    program
        .blocks
        .insert(BlockId(5), Block(vec![Instruction::Jump(BlockId(1))]));
    program
        .blocks
        .insert(BlockId(6), Block(vec![Instruction::Return(None)]));
    program.num_qubits = 2;
    program.num_results = 1;
    program
}

/// The join search assumed a DAG, and a back edge is what stops the block graph
/// from being one.
///
/// Without the back edge removed from the search, the then-arm of this branch
/// is reachable from the else-arm the long way round, through the header, so
/// the search answers with the arm itself. The assumption was not weakened to
/// cope; the edge that broke it is taken out of the question, which leaves one
/// iteration a DAG again.
#[test]
fn a_branch_inside_a_loop_body_finds_its_own_join() {
    let program = branching_loop_program();

    let graph = program_to_graph(&program).expect("a branch inside a loop lowers");
    assert!(
        verify(&graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&graph)
    );

    let read = read_program(&graph).expect("the reader accepts the nesting");
    let [ReadItem::Instruction(_), ReadItem::Loop(repeating)] = read.items.as_slice() else {
        panic!("expected a gate then a loop, got {:?}", read.items);
    };
    let [
        ReadItem::Instruction(measurement),
        ReadItem::Conditional(branch),
    ] = repeating.body.as_slice()
    else {
        panic!(
            "expected a measurement then a branch, got {:?}",
            repeating.body
        );
    };
    assert_eq!(measurement.operation, "quantum.measure_z");
    let then_arm: Vec<&str> = branch
        .then_items
        .iter()
        .map(|item| match item {
            ReadItem::Instruction(instruction) => instruction.operation.as_str(),
            other => panic!("an arm of one gate read back as {other:?}"),
        })
        .collect();
    assert_eq!(
        then_arm,
        ["quantum.x"],
        "the arm holds the arm's gate rather than the rest of the loop"
    );
    assert!(
        branch.else_items.as_ref().is_none_or(Vec::is_empty),
        "the empty arm stayed empty: {:?}",
        branch.else_items
    );
}

/// Every node of each region in durable-identity order.
///
/// Identity is allocated as the lowering walks the program and is never
/// reissued, so identity order is source order. That is the thing the sort is
/// asserted to reproduce.
fn identity_order(graph: &qsc_qhdg::Graph) -> BTreeMap<RegionId, Vec<StableNodeId>> {
    let mut order: BTreeMap<RegionId, BTreeSet<StableNodeId>> = graph
        .regions()
        .map(|region| (region.id, BTreeSet::new()))
        .collect();
    for node in graph.nodes() {
        let id = graph
            .stable_id(node.id)
            .expect("a lowered node carries a durable identity");
        order.entry(node.region).or_default().insert(id);
    }
    order
        .into_iter()
        .map(|(region, ids)| (region, ids.into_iter().collect()))
        .collect()
}

/// The property the whole emission design rests on.
///
/// A minimum-identity-first topological sort over a graph whose identities were
/// allocated in source order reproduces that order, so taking output order from
/// the graph returns an untouched program unchanged. Until now that was an
/// argument about code rather than something the suite would notice breaking.
///
/// Asserted over every node of each region rather than over sited nodes only.
/// The unsited allocate and clbit nodes are the ones dependencies leave
/// mutually unordered, so they are the only pairs the tie-break decides, and an
/// assertion that skipped them would pass under any tie-break at all.
///
/// The graph now covers the represented set, so the claim extends past regions
/// to the program: derived order is source order, instruction for instruction,
/// in every block.
#[test]
fn the_derived_order_reproduces_source_order_across_every_corpus() {
    for (name, program) in corpus().into_iter().chain(loop_corpus()) {
        let lowered = program_to_graph_with_sites(&program).expect("the program should lower");
        let derived =
            region_orders(&lowered.graph).expect("a lowered graph linearizes into program order");
        assert_eq!(
            derived,
            identity_order(&lowered.graph),
            "{name} did not sort back into source order"
        );

        let mut placed: BTreeMap<BlockId, Vec<usize>> = BTreeMap::new();
        for (block, ids) in program_order(&lowered).expect("a lowered graph has a program order") {
            let indices = placed.entry(block).or_default();
            for id in ids {
                let index = lowered.sites[&id].index;
                // An instruction with several nodes is placed once, where its
                // first node lands, which is what emission does with it.
                if !indices.contains(&index) {
                    indices.push(index);
                }
            }
        }
        let mut expected: BTreeMap<BlockId, Vec<usize>> = BTreeMap::new();
        for site in &lowered.represented {
            expected.entry(site.block).or_default().push(site.index);
        }
        assert_eq!(
            placed, expected,
            "{name} did not place its instructions where the source had them"
        );
    }
}

#[test]
fn permuting_the_derived_order_permutes_the_emitted_program() {
    // What distinguishes emission from subtraction. The decline-to-act guard
    // does not: it passes under subtraction too, because it only shows that
    // removing a node removes an instruction. This shows the output follows the
    // order the graph yields, which subtraction could not do at all.
    let mut permuted = Vec::new();
    for (name, program) in corpus().into_iter().chain(loop_corpus()) {
        let lowered = program_to_graph_with_sites(&program).expect("the program should lower");
        let ordered = graph_to_program(&program, &lowered.graph, &lowered)
            .expect("an untouched graph carries back");
        assert_eq!(ordered.to_string(), program.to_string());

        let other = block_order(&lowered.graph, &lowered.sites, BTreeSet::pop_last)
            .expect("a lowered graph linearizes under either tie-break");
        let emitted =
            emit(&program, &lowered.graph, &lowered, &other).expect("the other order emits");
        if emitted.to_string() != ordered.to_string() {
            permuted.push(name);
        }
    }
    // Named rather than counted. Half the corpus holds two instructions in one
    // block that dependencies leave unordered, and those are the fixtures whose
    // emitted program moves when the order does. A change here means either the
    // corpus changed or emission stopped following the order it was handed.
    assert_eq!(
        permuted,
        [
            "bell",
            "result-conditional",
            "disjoint-variables",
            "branch-inside-a-loop",
            "loop-carried-value",
            "header-condition",
            "header-empty-bodied-call"
        ]
    );
}

#[test]
fn emission_accounts_for_every_represented_instruction_exactly_once() {
    // Omission and duplication are both silent on any program the corpus does
    // not hold, and subtraction could do neither. Both directions are asserted,
    // since a check catching only omission would leave duplication silent.
    let program = builder::bell_program();
    let lowered = program_to_graph_with_sites(&program).expect("the program should lower");
    let present: Vec<_> = lowered.represented.iter().copied().collect();
    let nothing = BTreeSet::new();
    assert_eq!(
        emitted_exactly_once(&lowered.represented, &nothing, &present),
        Ok(())
    );

    let dropped = present[0];
    assert_eq!(
        emitted_exactly_once(&lowered.represented, &nothing, &present[1..]),
        Err(RetireError::NotEmittedOnce {
            block: dropped.block.0,
            index: dropped.index,
            count: 0
        })
    );

    let mut twice = present.clone();
    twice.push(dropped);
    assert_eq!(
        emitted_exactly_once(&lowered.represented, &nothing, &twice),
        Err(RetireError::NotEmittedOnce {
            block: dropped.block.0,
            index: dropped.index,
            count: 2
        })
    );

    // A retired instruction is accounted for by being retired rather than by
    // being emitted, so it is not an omission.
    let retired = BTreeSet::from([dropped]);
    assert_eq!(
        emitted_exactly_once(&lowered.represented, &retired, &present[1..]),
        Ok(())
    );
}

#[test]
fn an_order_that_leaves_an_instruction_out_is_refused_by_emission() {
    let program = builder::bell_program();
    let lowered = program_to_graph_with_sites(&program).expect("the program should lower");
    let mut order = block_order(&lowered.graph, &lowered.sites, BTreeSet::pop_first)
        .expect("a lowered graph has a program order");
    let block = *order.keys().next().expect("a program has a block");
    let dropped = order
        .get_mut(&block)
        .expect("the block was just named")
        .remove(0);

    let site = lowered.sites[&dropped];
    assert_eq!(
        emit(&program, &lowered.graph, &lowered, &order).err(),
        Some(RetireError::NotEmittedOnce {
            block: site.block.0,
            index: site.index,
            count: 0
        })
    );
}

#[test]
fn a_different_tie_break_stops_reproducing_source_order() {
    let program = builder::bell_program();
    let lowered = program_to_graph_with_sites(&program).expect("the program should lower");
    let perturbed = region_orders_with(&lowered.graph, BTreeSet::pop_last)
        .expect("a lowered graph linearizes under either tie-break");
    assert_ne!(
        perturbed,
        identity_order(&lowered.graph),
        "the fixpoint would hold under any tie-break, so the test above asserts nothing"
    );

    // Why it is sensitive, stated rather than left to the reader: bell's two
    // allocate nodes and two clbit nodes wait on nothing, so dependencies leave
    // their order open and only the smallest-ready-identity rule settles it.
    let dependencies = region_dependencies(&lowered.graph);
    let module = lowered
        .graph
        .regions()
        .find(|region| region.kind == RegionKind::ModuleOmega)
        .expect("a lowered graph has a module region")
        .id;
    let unblocked = lowered
        .graph
        .nodes()
        .filter(|node| node.region == module)
        .filter(|node| {
            let id = lowered
                .graph
                .stable_id(node.id)
                .expect("a lowered node carries a durable identity");
            match dependencies.get(&module).and_then(|needs| needs.get(&id)) {
                Some(needs) => needs.is_empty(),
                None => true,
            }
        })
        .filter(|node| {
            node.operation.0 == vocabulary::ALLOCATE || node.operation.0 == vocabulary::CLBIT
        })
        .count();
    assert!(
        unblocked >= 2,
        "the tie-break has fewer than two nodes to decide between, so perturbing it proves nothing"
    );
}

#[test]
fn a_carrier_does_not_stop_the_stage_finding_facts() {
    // The quiet failure this guards. The stage asks the circuit reader which
    // qubits an instruction touches, and the reader refuses any name outside
    // its vocabulary. That refusal is swallowed into an empty fact map, which
    // the pipeline reads as nothing to do, so the stage would decline on every
    // program and the byte-identity tests would not notice, because they call
    // the return leg directly and never run the stage.
    let program = builder::bell_program();
    let lowered = program_to_graph_with_sites(&program).expect("the program should lower");
    let before = qubit_operands(&lowered.graph).expect("a lowered graph reads");

    let mut graph = lowered.graph;
    let region = graph
        .regions()
        .find(|region| region.kind == RegionKind::ModuleOmega)
        .expect("a lowered graph has a module region")
        .id;
    graph
        .add_node(region, vocabulary::OPAQUE, Vec::new())
        .expect("the module region accepts a carrier");

    let after = qubit_operands(&graph).expect("a carrier does not make a graph unreadable");
    assert_eq!(
        after, before,
        "a carrier is skipped rather than read as an instruction"
    );
}

fn integer(value: i64) -> Operand {
    Operand::Literal(Literal::Integer(value))
}

fn counter(id: u32) -> Variable {
    Variable {
        variable_id: VariableId(id),
        ty: Ty::Prim(Prim::Integer),
    }
}

/// A program with `body` as its entry block, followed by a return.
fn classical_program(body: Vec<Instruction>) -> Program {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::h_decl());
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(1);
    program.blocks.insert(
        BlockId(0),
        Block(
            body.into_iter()
                .chain([
                    Instruction::Call(
                        CallableId(0),
                        vec![Operand::Literal(Literal::Qubit(0))],
                        None,
                        None,
                    ),
                    Instruction::Return(None),
                ])
                .collect(),
        ),
    );
    program.num_qubits = 1;
    program
}

/// The same variable written twice, which SSA has not yet turned into two.
fn repeated_store_program() -> Program {
    classical_program(vec![
        Instruction::Store(integer(1), counter(0)),
        Instruction::Store(integer(2), counter(0)),
    ])
}

/// A read of one variable followed by a later write to it.
fn read_then_write_program() -> Program {
    classical_program(vec![
        Instruction::Store(integer(1), counter(0)),
        Instruction::Store(Operand::Variable(counter(0)), counter(1)),
        Instruction::Store(integer(2), counter(0)),
    ])
}

/// Two variables that never name each other, so nothing may order them.
fn disjoint_variable_program() -> Program {
    classical_program(vec![
        Instruction::Store(integer(1), counter(0)),
        Instruction::Store(integer(2), counter(1)),
        Instruction::Store(integer(3), counter(0)),
    ])
}

/// A loop whose body updates a classical variable it took from outside.
///
/// The header holds nothing but its branch, because the condition runs once
/// more than the body does and this stage refuses a header that would have to
/// place an operation. The condition is defined here rather than left dangling:
/// both existing loop fixtures branch on a variable no instruction defines, so
/// neither can exercise a classical value crossing a back edge.
fn loop_carried_value_program() -> Program {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::x_decl());
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(1);
    let condition = Variable {
        variable_id: VariableId(1),
        ty: Ty::Prim(Prim::Boolean),
    };
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Store(integer(0), counter(0)),
            Instruction::Store(Operand::Literal(Literal::Bool(true)), condition),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![Instruction::Branch(
            condition,
            BlockId(2),
            BlockId(3),
            None,
        )]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Add(Operand::Variable(counter(0)), integer(1), counter(0)),
            Instruction::Call(
                CallableId(0),
                vec![Operand::Literal(Literal::Qubit(0))],
                None,
                None,
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Return(None)]));
    program.num_qubits = 1;
    program
}

/// A loop whose header computes its own condition.
///
/// The other three loop fixtures hold nothing but a `Branch` in the header, so
/// none of them ever reaches the carrier exemption's accepting path. A real
/// `while` header holds the comparison the branch reads, which lowers to a
/// carrier, which is the node the exemption is about.
fn header_condition_loop_program() -> Program {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::x_decl());
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(1);
    let condition = Variable {
        variable_id: VariableId(1),
        ty: Ty::Prim(Prim::Boolean),
    };
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Store(integer(0), counter(0)),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Icmp(
                ConditionCode::Slt,
                Operand::Variable(counter(0)),
                integer(3),
                condition,
            ),
            Instruction::Branch(condition, BlockId(2), BlockId(3), None),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Add(Operand::Variable(counter(0)), integer(1), counter(0)),
            Instruction::Call(
                CallableId(0),
                vec![Operand::Literal(Literal::Qubit(0))],
                None,
                None,
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Return(None)]));
    program.num_qubits = 1;
    program
}

/// A loop whose header calls a bodied callable whose body is only a terminator.
///
/// The call gets a carrier, because a caller-visible instruction the body left
/// no node for is carried instead. That makes it the second way a loop header
/// can hold a carrier, and the only fixture that reaches it.
fn header_empty_bodied_call_loop_program() -> Program {
    let mut program = header_condition_loop_program();
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "noop".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(4)),
            call_type: CallableType::Regular,
        },
    );
    program
        .blocks
        .insert(BlockId(4), Block(vec![Instruction::Return(None)]));
    let header = program.get_block(BlockId(1)).0.clone();
    program.blocks.insert(
        BlockId(1),
        Block(
            std::iter::once(Instruction::Call(CallableId(2), Vec::new(), None, None))
                .chain(header)
                .collect(),
        ),
    );
    program
}

/// A program that writes a classical variable inside a branch arm.
fn branch_arm_write_program() -> Program {
    let mut program = result_conditional_program();
    let arm = program.get_block(BlockId(1)).0.clone();
    program.blocks.insert(
        BlockId(1),
        Block(
            std::iter::once(Instruction::Store(integer(1), counter(1)))
                .chain(arm)
                .collect(),
        ),
    );
    program
}

/// The classical fixtures whose ordering the producer must preserve.
///
/// A view over the canonical corpus rather than a second fixture set, so the
/// byte-identity round trip covers these too and cannot drift from them.
fn classical_corpus() -> Vec<(&'static str, Program)> {
    corpus()
        .into_iter()
        .chain(loop_corpus())
        .filter(|(name, _)| {
            matches!(
                *name,
                "repeated-store"
                    | "read-then-write"
                    | "disjoint-variables"
                    | "loop-carried-value"
                    | "wrapped-gate"
                    | "wrapped-gate-twice"
            )
        })
        .collect()
}

/// The carriers of one graph in durable-identity order, which is source order.
fn carriers(graph: &qsc_qhdg::Graph) -> Vec<StableNodeId> {
    let mut found: Vec<StableNodeId> = graph
        .nodes()
        .filter(|node| node.operation.0 == vocabulary::OPAQUE)
        .map(|node| graph.stable_id(node.id).expect("a lowered node is live"))
        .collect();
    found.sort_unstable();
    found
}

#[test]
fn every_classical_fixture_lowers_to_a_graph_that_verifies() {
    // Cross-region value edges are the reason this is not a formality. A value
    // read inside a branch or a loop body needs a declared interface binding,
    // and verification refuses the edge without one.
    for (name, program) in corpus().into_iter().chain(loop_corpus()) {
        let graph = program_to_graph(&program).unwrap_or_else(|error| {
            panic!("{name} should lower: {error}");
        });
        assert!(
            verify(&graph).is_empty(),
            "{name} did not verify: {:?}",
            verify(&graph)
        );
    }
}

#[test]
fn no_carrier_port_is_linear() {
    // A port carrying a qubit is linear at any depth, so a second read of one
    // would trip the fan-out rule. Asserted on the port types rather than
    // inferred from the absence of a diagnostic, which only says no fixture
    // happened to read one twice.
    for (name, program) in corpus().into_iter().chain(loop_corpus()) {
        let graph = program_to_graph(&program).expect("the fixture should lower");
        let carried = carriers(&graph);
        for port in graph.ports() {
            let node = graph.stable_id(port.node).expect("a lowered node is live");
            if !carried.contains(&node) {
                continue;
            }
            assert_eq!(
                port.kind,
                PortKind::Value,
                "{name} gave a carrier a non-value port"
            );
            assert!(
                !port.ty.carries_qubit(),
                "{name} gave a carrier a linear port of type {:?}",
                port.ty
            );
        }
    }
}

#[test]
fn a_variable_accessed_twice_is_chained_in_source_order() {
    for (name, program) in [
        ("repeated-store", repeated_store_program()),
        ("read-then-write", read_then_write_program()),
    ] {
        let graph = program_to_graph(&program).expect("the fixture should lower");
        let carried = carriers(&graph);
        let module = graph
            .regions()
            .find(|region| region.kind == RegionKind::ModuleOmega)
            .expect("a lowered graph has a module region")
            .id;
        let dependencies = region_dependencies(&graph);
        let within = dependencies
            .get(&module)
            .expect("the module region has dependencies");
        for pair in carried.windows(2) {
            assert!(
                within
                    .get(&pair[1])
                    .is_some_and(|needs| needs.contains(&pair[0])),
                "{name} left two accesses to one variable unordered"
            );
        }
    }
}

#[test]
fn accesses_to_distinct_variables_are_not_ordered_against_each_other() {
    // The standing decision that disjoint operations gain no edge applies to
    // variables as much as to qubits. A chain per variable is the point; a
    // chain across variables would be a total order in disguise.
    let graph = program_to_graph(&disjoint_variable_program()).expect("the fixture should lower");
    let carried = carriers(&graph);
    let module = graph
        .regions()
        .find(|region| region.kind == RegionKind::ModuleOmega)
        .expect("a lowered graph has a module region")
        .id;
    let dependencies = region_dependencies(&graph);
    let within = dependencies
        .get(&module)
        .expect("the module region has dependencies");

    // Written as first, second, third: the first and third name one variable
    // and the second names another.
    let [first, second, third] = carried.as_slice() else {
        panic!("expected three carriers, got {}", carried.len());
    };
    assert!(within.get(third).is_some_and(|needs| needs.contains(first)));
    assert!(
        within
            .get(third)
            .is_none_or(|needs| !needs.contains(second)),
        "the chain reached across two variables that never name each other"
    );
    assert!(within.get(second).is_none_or(BTreeSet::is_empty));
}

#[test]
fn a_loop_carried_value_crosses_a_back_edge_it_declares() {
    // Both halves or neither. The verifier exempts the cycle only when the
    // region names the producing port as its back-edge exit and the consuming
    // port as carried, so a partial declaration refuses as an undeclared cycle
    // rather than passing.
    let graph = program_to_graph(&loop_carried_value_program()).expect("the loop lowers");
    let theta = graph
        .regions()
        .find(|region| region.kind == RegionKind::Theta)
        .expect("the loop became a theta region");
    let value_exits = theta
        .interface
        .exits
        .iter()
        .filter(|(kind, port)| {
            *kind == qsc_qhdg::ExitKind::Backedge
                && graph
                    .port(*port)
                    .is_some_and(|port| port.kind == PortKind::Value)
        })
        .count();
    assert!(
        value_exits >= 1,
        "the classical update crosses no declared back edge"
    );
    assert!(
        verify(&graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&graph)
    );
}

#[test]
fn a_write_inside_a_branch_arm_is_refused_and_names_the_variable() {
    // Both arms would write it and nothing joins them: `Phi` is refused and SSA
    // has not run, so there is no merge for anything after the join to read.
    assert_eq!(
        program_to_graph(&branch_arm_write_program()).err(),
        Some(Error::BranchWrite { variable: 1 })
    );
}

#[test]
fn a_kind_that_cannot_reach_this_stage_is_refused_rather_than_ordered() {
    // Unreachable by pass order, since the passes that create these run after
    // this stage's hook. Refused anyway, so a change to that order surfaces as
    // a refusal instead of as a silently reordered program.
    for (kind, instruction) in [
        ("Alloca", Instruction::Alloca(counter(0))),
        ("Load", Instruction::Load(counter(0), counter(1))),
        (
            "Phi",
            Instruction::Phi(vec![(integer(1), BlockId(0))], counter(0)),
        ),
    ] {
        assert_eq!(
            program_to_graph(&classical_program(vec![instruction])).err(),
            Some(Error::UnreachableKind { kind }),
            "{kind} was not refused"
        );
    }
}

#[test]
fn every_represented_instruction_gains_a_node_and_a_seeded_omission_does_not() {
    // A completeness check that has never failed is indistinguishable from one
    // that cannot fail, which is the reasoning the decline-to-act guard already
    // embodies for the round trip. The first half asserts the real lowering
    // passes; the second drops one instruction's nodes, which is exactly what a
    // producer path returning without building one would leave behind.
    for (name, program) in corpus().into_iter().chain(loop_corpus()) {
        let lowered = program_to_graph_with_sites(&program).expect("the program should lower");
        assert!(
            !lowered.represented.is_empty(),
            "{name} represents nothing, so the check would pass vacuously"
        );
        assert_eq!(
            unrepresented(&lowered.represented, lowered.sites.values().copied()),
            None,
            "{name} left an instruction unrepresented"
        );

        let dropped = *lowered
            .represented
            .iter()
            .next()
            .expect("the represented set is not empty");
        let seeded = lowered
            .sites
            .values()
            .copied()
            .filter(|site| *site != dropped);
        assert_eq!(
            unrepresented(&lowered.represented, seeded),
            Some(dropped),
            "{name} did not notice a seeded omission"
        );
    }
}

#[test]
fn no_delivered_rule_matches_a_carrier_this_producer_emitted() {
    // The same claim `qsc_qhdg` asserts on a hand-built graph, made here on one
    // the producer emitted, because a hand-built graph can be shaped to pass.
    let mut rules = qsc_qhdg::graph_rewrite_rules();
    rules.push(Box::new(qsc_qhdg::DiagonalCollapse::new([
        "quantum.z",
        "quantum.cz",
    ])));
    for (name, program) in classical_corpus() {
        let graph = program_to_graph(&program).expect("the fixture should lower");
        let adjacency = qsc_qhdg::Adjacency::new(&graph);
        let context = qsc_qhdg::RewriteContext::default();
        for seed in carriers(&graph) {
            for rule in &rules {
                assert!(
                    rule.find(&graph, &adjacency, &context, seed)
                        .expect("find runs")
                        .is_none(),
                    "'{}' matched a carrier in {name}",
                    rule.name()
                );
            }
        }
    }
}
