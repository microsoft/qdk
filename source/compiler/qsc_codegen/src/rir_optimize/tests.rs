// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_rir::builder;
use qsc_rir::rir::{
    Block, BlockId, Callable, CallableId, CallableType, Instruction, Literal, Operand, Program,
};

use super::optimize;
use crate::pass_pipeline::OptimizationLevel;

/// A program applying the same self-inverse gate twice in a row.
fn cancellable_program() -> Program {
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
    let h = || {
        Instruction::Call(
            CallableId(0),
            vec![Operand::Literal(Literal::Qubit(0))],
            None,
            None,
        )
    };
    program
        .blocks
        .insert(BlockId(0), Block(vec![h(), h(), Instruction::Return(None)]));
    program.num_qubits = 1;
    program
}

fn calls(program: &Program) -> usize {
    program
        .get_block(BlockId(0))
        .0
        .iter()
        .filter(|instruction| matches!(instruction, Instruction::Call(..)))
        .count()
}

#[test]
fn an_adjacent_pair_of_the_same_self_inverse_gate_is_removed() {
    let mut program = cancellable_program();
    assert_eq!(calls(&program), 2);
    optimize(&mut program, OptimizationLevel::Default);
    assert_eq!(
        calls(&program),
        0,
        "both halves of the pair leave, not just one"
    );
}

#[test]
fn a_quarter_turn_pair_is_left_alone() {
    // `t` twice is `s`, not the identity. This is the difference between a
    // correct pass and a silent miscompile, so it is asserted rather than
    // inferred from the rule's name list.
    let mut program = cancellable_program();
    let mut t = builder::h_decl();
    t.name = "__quantum__qis__t__body".to_string();
    program.callables.insert(CallableId(0), t);

    let before = program.to_string();
    optimize(&mut program, OptimizationLevel::Default);
    assert_eq!(program.to_string(), before);
}

#[test]
fn a_program_this_stage_cannot_lower_is_returned_untouched() {
    // A back edge refuses in the producer. Declining is the ordinary case, and
    // the program has to survive it byte for byte.
    let mut program = builder::two_body_program_with_loop();
    let before = program.to_string();
    optimize(&mut program, OptimizationLevel::Default);
    assert_eq!(program.to_string(), before);
}

#[test]
fn a_program_with_nothing_to_cancel_is_returned_untouched() {
    let mut program = builder::bell_program();
    let before = program.to_string();
    optimize(&mut program, OptimizationLevel::Default);
    assert_eq!(program.to_string(), before);
}

#[test]
fn nothing_cancels_at_the_level_that_asks_for_none() {
    // The default everywhere in code generation. Cancelling is sound about the
    // unitary and wrong about the device: a chain of `x` gates is a noise
    // accumulator, so a program written to measure gate error must survive.
    let mut program = cancellable_program();
    let before = program.to_string();
    optimize(&mut program, OptimizationLevel::None);
    assert_eq!(program.to_string(), before);
}

#[test]
fn a_pair_separated_by_a_gate_on_another_qubit_still_cancels() {
    // Adjacency is on the qubit's own chain, not in the instruction listing.
    // A textual reading would miss this, and the golden enumeration was built
    // on the chain reading, so the two have to agree.
    let mut program = cancellable_program();
    program.callables.insert(CallableId(2), builder::x_decl());
    program.num_qubits = 2;
    let h = || {
        Instruction::Call(
            CallableId(0),
            vec![Operand::Literal(Literal::Qubit(0))],
            None,
            None,
        )
    };
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            h(),
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::Qubit(1))],
                None,
                None,
            ),
            h(),
            Instruction::Return(None),
        ]),
    );

    optimize(&mut program, OptimizationLevel::Default);
    assert_eq!(
        calls(&program),
        1,
        "the pair cancels through the untouched qubit, leaving only the x"
    );
}

/// A loop whose body is one self-inverse gate.
///
/// The body's own back edge makes that gate adjacent to itself, which is the
/// shape the cancellation rule has to decline: a pair drawn from two different
/// iterations is a claim about the trip count, and a body of one node is not a
/// pair at all.
fn looping_program() -> Program {
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
    let condition = qsc_rir::rir::Variable {
        variable_id: qsc_rir::rir::VariableId(0),
        ty: qsc_rir::rir::Ty::Prim(qsc_rir::rir::Prim::Boolean),
    };
    let h = || {
        Instruction::Call(
            CallableId(0),
            vec![Operand::Literal(Literal::Qubit(0))],
            None,
            None,
        )
    };
    program
        .blocks
        .insert(BlockId(0), Block(vec![Instruction::Jump(BlockId(1))]));
    program.blocks.insert(
        BlockId(1),
        Block(vec![Instruction::Branch(
            condition,
            BlockId(2),
            BlockId(3),
            None,
        )]),
    );
    program
        .blocks
        .insert(BlockId(2), Block(vec![h(), Instruction::Jump(BlockId(1))]));
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Return(None)]));
    program.num_qubits = 1;
    program
}

#[test]
fn a_gate_adjacent_to_itself_across_a_back_edge_is_left_alone() {
    let mut program = looping_program();
    let report = optimize(&mut program, OptimizationLevel::Default);
    let body = program
        .get_block(BlockId(2))
        .0
        .iter()
        .filter(|instruction| matches!(instruction, Instruction::Call(..)))
        .count();
    assert_eq!(body, 1, "the loop body kept its gate: {report:?}");
    assert!(
        !report.changed,
        "the stage should decline rather than reason across iterations: {report:?}"
    );
}

/// A loop whose body holds an adjacent cancelling pair.
///
/// `bracketed` puts a gate on either side of the pair, which is what decides
/// whether the theta's carried interface survives the cancellation: the carried
/// binding names the body's first use and the back-edge exit names its last
/// production, so a pair at either end takes an interface port with it.
/// A loop whose body holds the given gates on one qubit.
///
/// Where a cancelling pair sits in the body is what decides whether the theta's
/// carried interface survives its removal: the carried binding names the body's
/// first use of the wire and the back-edge exit names its last production, so a
/// pair at either end takes an interface port with it.
fn cancellable_loop_program(body_gates: &[u32]) -> Program {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), builder::h_decl());
    program.callables.insert(CallableId(1), builder::x_decl());
    program.callables.insert(CallableId(2), builder::z_decl());
    program.callables.insert(
        CallableId(3),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.entry = CallableId(3);
    let condition = qsc_rir::rir::Variable {
        variable_id: qsc_rir::rir::VariableId(0),
        ty: qsc_rir::rir::Ty::Prim(qsc_rir::rir::Prim::Boolean),
    };
    let gate = |callable: u32| {
        Instruction::Call(
            CallableId(callable),
            vec![Operand::Literal(Literal::Qubit(0))],
            None,
            None,
        )
    };
    program
        .blocks
        .insert(BlockId(0), Block(vec![Instruction::Jump(BlockId(1))]));
    program.blocks.insert(
        BlockId(1),
        Block(vec![Instruction::Branch(
            condition,
            BlockId(2),
            BlockId(3),
            None,
        )]),
    );
    let mut body: Vec<Instruction> = body_gates.iter().map(|id| gate(*id)).collect();
    body.push(Instruction::Jump(BlockId(1)));
    program.blocks.insert(BlockId(2), Block(body));
    program
        .blocks
        .insert(BlockId(3), Block(vec![Instruction::Return(None)]));
    program.num_qubits = 1;
    program
}

/// The gates the loop body kept, in order.
fn loop_body(program: &Program) -> Vec<String> {
    program
        .get_block(BlockId(2))
        .0
        .iter()
        .filter_map(|instruction| match instruction {
            Instruction::Call(callable, ..) => Some(program.get_callable(*callable).name.clone()),
            _ => None,
        })
        .collect()
}

/// A graph-side retirement inside a theta body carries back into the program
/// without removing the loop.
#[test]
fn a_pair_inside_a_loop_body_is_retired_and_the_loop_survives() {
    let mut program = cancellable_loop_program(&[0, 1, 1, 2]);
    let report = optimize(&mut program, OptimizationLevel::Default);

    assert!(report.changed, "expected the pair to go: {report:?}");
    assert_eq!(
        loop_body(&program),
        ["__quantum__qis__h__body", "__quantum__qis__z__body"],
        "the pair left and its neighbours did not"
    );
    assert_eq!(
        program
            .get_block(BlockId(2))
            .0
            .last()
            .map(ToString::to_string),
        Some("Jump(1)".to_string()),
        "the body still comes back, so the loop is still a loop"
    );
    assert_eq!(
        program.get_block(BlockId(1)).0.len(),
        1,
        "the condition block is untouched, so the trip count is unchanged"
    );

    // The last call to `x` was inside the loop body, which is the case the
    // reachability walk has to reach through a back edge rather than around it.
    assert!(
        program
            .callables
            .iter()
            .all(|(_, callable)| callable.name != "__quantum__qis__x__body"),
        "the declaration whose last caller was inside the loop goes with it"
    );
    assert_eq!(
        program.callables.iter().count(),
        3,
        "only that one declaration left"
    );
}

/// A pair at either end of the body holds a port the loop's contract names, so
/// splicing past it would leave the carried binding or the back-edge exit
/// pointing at nothing. Both ends are checked, because the two halves of the
/// contract are named in different places and one guard could cover only one.
#[test]
fn a_pair_at_the_edge_of_a_loop_body_is_declined_by_the_rule() {
    for gates in [vec![1, 1, 0], vec![0, 1, 1]] {
        let mut program = cancellable_loop_program(&gates);
        let before = program.to_string();
        let report = optimize(&mut program, OptimizationLevel::Default);

        assert_eq!(program.to_string(), before, "{gates:?} was not left alone");
        // The rule declining and the transaction rejecting are both safe and
        // are not the same outcome: a rejected transaction abandons the whole
        // graph, which is what the next test measures.
        assert_eq!(
            report.declined.as_deref(),
            Some("Unchanged"),
            "{gates:?} should decline in the rule rather than be refused after the fact"
        );
    }
}

/// What declining in the rule buys, stated as a result rather than as a reason.
///
/// This body holds three cancelling pairs, one interior and one at each end. A
/// patch refused after the fact abandons the graph it was building, so the
/// interior pair used to be lost along with the two that were never legal.
#[test]
fn a_legal_pair_still_cancels_beside_ones_the_loop_boundary_declines() {
    let mut program = cancellable_loop_program(&[0, 0, 1, 1, 2, 2]);
    let report = optimize(&mut program, OptimizationLevel::Default);

    assert!(
        report.changed,
        "expected the interior pair to go: {report:?}"
    );
    assert_eq!(
        loop_body(&program),
        [
            "__quantum__qis__h__body",
            "__quantum__qis__h__body",
            "__quantum__qis__z__body",
            "__quantum__qis__z__body"
        ],
        "the interior pair left and the two the boundary declined stayed"
    );
}

/// A retirement cannot empty a loop body, and not because anything forbids that
/// by name.
///
/// The only pair that could is the body's whole content, and a back edge makes
/// that pair adjacent to itself across iterations, which the rule already
/// declines on the trip-count argument. The carried interface is never asked to
/// survive its own disappearance.
#[test]
fn a_retirement_cannot_empty_a_loop_body() {
    let mut program = cancellable_loop_program(&[1, 1]);
    let before = program.to_string();
    let report = optimize(&mut program, OptimizationLevel::Default);

    assert_eq!(program.to_string(), before);
    assert!(!report.changed, "{report:?}");
}
