// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeMap;

use super::{
    get_partial_evaluation_error, get_partial_evaluation_error_with_capabilities,
    get_rir_program_with_capabilities,
};
use crate::Error;
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};
use qsc_rir::rir::{
    CallableType, ConditionCode, Instruction, Literal, Operand, Prim, Program, Ty, VariableId,
};

#[derive(Clone)]
enum TraceValue {
    Scalar(i64),
    Array(Vec<i64>),
}

impl TraceValue {
    fn scalar(&self) -> i64 {
        let Self::Scalar(value) = self else {
            panic!("expected scalar");
        };
        *value
    }
}

fn value(
    operand: Operand,
    variables: &BTreeMap<VariableId, TraceValue>,
    program: &Program,
) -> TraceValue {
    match operand {
        Operand::Variable(var) => variables[&var.variable_id].clone(),
        Operand::Literal(Literal::Integer(value)) => TraceValue::Scalar(value),
        Operand::Literal(Literal::Bool(value)) => TraceValue::Scalar(i64::from(value)),
        Operand::Literal(Literal::Qubit(id)) => TraceValue::Scalar(id.into()),
        Operand::Literal(Literal::Array(id)) => TraceValue::Array(
            program.array_literals[id]
                .contents
                .iter()
                .map(|literal| value(Operand::Literal(*literal), variables, program).scalar())
                .collect(),
        ),
        Operand::Literal(_) => panic!("unexpected trace operand: {operand}"),
    }
}

/// Executes the classical label-binding instructions with scripted readouts, recording physical
/// qubit operands at Probe calls. This checks identities independently of quantum gate semantics.
fn trace(source: &str, profile: Profile, readouts: &[bool]) -> Vec<Vec<i64>> {
    let source = format!(
        "operation Probe(a : Qubit, b : Qubit, c : Qubit) : Unit {{ body intrinsic; }}\n{source}"
    );
    let program = get_rir_program_with_capabilities(&source, profile.into());
    qsc_rir::passes::check_and_transform(&mut program.clone());
    assert!(!program.use_dynamic_qubit_management);
    assert!(
        program
            .callables
            .iter()
            .all(|(id, callable)| { id == program.entry || callable.body.is_none() })
    );
    assert!(
        !program
            .callables
            .iter()
            .any(|(_, callable)| callable.name.contains("swap"))
    );

    trace_program(&program, readouts)
}

fn trace_program(program: &Program, readouts: &[bool]) -> Vec<Vec<i64>> {
    let mut variables = BTreeMap::new();
    let mut readouts = readouts.iter();
    let mut calls = Vec::new();
    let mut block = program
        .get_callable(program.entry)
        .body
        .expect("entry body");
    for _ in 0..1_000 {
        for instruction in &program.get_block(block).0 {
            let eval = |operand| value(operand, &variables, program);
            match instruction {
                Instruction::Store(operand, var) | Instruction::Convert(operand, var) => {
                    let value = eval(*operand);
                    variables.insert(var.variable_id, value);
                }
                Instruction::StoreArray(operands, var) => {
                    let values = operands
                        .iter()
                        .map(|operand| eval(*operand).scalar())
                        .collect();
                    variables.insert(var.variable_id, TraceValue::Array(values));
                }
                Instruction::Index(array, index, var) => {
                    let TraceValue::Array(values) = eval(*array) else {
                        panic!("expected array");
                    };
                    let index = usize::try_from(eval(*index).scalar()).expect("nonnegative index");
                    variables.insert(var.variable_id, TraceValue::Scalar(values[index]));
                }
                Instruction::Add(left, right, var) => {
                    let result = eval(*left).scalar() + eval(*right).scalar();
                    variables.insert(var.variable_id, TraceValue::Scalar(result));
                }
                Instruction::Icmp(condition, left, right, var) => {
                    let left = eval(*left).scalar();
                    let right = eval(*right).scalar();
                    let result = match condition {
                        ConditionCode::Eq => left == right,
                        ConditionCode::Ne => left != right,
                        ConditionCode::Slt => left < right,
                        ConditionCode::Sle => left <= right,
                        ConditionCode::Sgt => left > right,
                        ConditionCode::Sge => left >= right,
                    };
                    variables.insert(var.variable_id, TraceValue::Scalar(i64::from(result)));
                }
                Instruction::LogicalNot(operand, var) => {
                    let result = eval(*operand).scalar() == 0;
                    variables.insert(var.variable_id, TraceValue::Scalar(i64::from(result)));
                }
                Instruction::Call(id, args, output, _) => {
                    let callable = program.get_callable(*id);
                    if callable.name == "Probe" {
                        assert!(
                            args.iter()
                                .all(|arg| arg.get_type() == Ty::Prim(Prim::Qubit))
                        );
                        calls.push(args.iter().map(|arg| eval(*arg).scalar()).collect());
                    } else if callable.call_type == CallableType::Readout {
                        let var = output.expect("readout output");
                        let result = *readouts.next().expect("scripted readout");
                        variables.insert(var.variable_id, TraceValue::Scalar(i64::from(result)));
                    } else {
                        assert!(
                            output.is_none(),
                            "unexpected value-returning call: {callable}"
                        );
                    }
                }
                Instruction::Branch(condition, if_true, if_false, _) => {
                    block = if variables[&condition.variable_id].scalar() != 0 {
                        *if_true
                    } else {
                        *if_false
                    };
                }
                Instruction::Jump(target) => block = *target,
                Instruction::Return(_) => {
                    assert!(readouts.next().is_none(), "unused scripted readouts");
                    return calls;
                }
                _ => panic!("unexpected trace instruction: {instruction}"),
            }
        }
    }
    panic!("trace did not terminate");
}

#[test]
fn branch_permutations_preserve_cycles() {
    let source = r#"
        operation Main() : Unit {
            use qs = Qubit[3];
            if M(qs[0]) == One {
                Relabel(qs, [qs[1], qs[2], qs[0]]);
            } else {
                Relabel(qs, [qs[2], qs[0], qs[1]]);
            }
            Probe(qs[0], qs[1], qs[2]);
        }
    "#;
    for profile in [Profile::Adaptive, Profile::AdaptiveRIF] {
        assert_eq!(trace(source, profile, &[true]), [[1, 2, 0]]);
        assert_eq!(trace(source, profile, &[false]), [[2, 0, 1]]);
    }
}

#[test]
fn retry_loop_updates_labels_each_iteration() {
    let source = r#"
        operation Main() : Unit {
            use qs = Qubit[3];
            mutable retry = true;
            while retry {
                Probe(qs[0], qs[1], qs[2]);
                Relabel([qs[0], qs[1]], [qs[1], qs[0]]);
                set retry = MResetZ(qs[0]) == One;
            }
            Probe(qs[0], qs[1], qs[2]);
        }
    "#;
    assert_eq!(
        trace(source, Profile::Adaptive, &[true, true, false]),
        [[0, 1, 2], [1, 0, 2], [0, 1, 2], [1, 0, 2]],
    );
}

#[test]
fn zero_iteration_loop_keeps_initial_labels() {
    let source = r#"
        operation Main() : Unit {
            use qs = Qubit[3];
            while MResetZ(qs[2]) == One {
                Relabel([qs[0], qs[1]], [qs[1], qs[0]]);
            }
            Probe(qs[0], qs[1], qs[2]);
        }
    "#;
    assert_eq!(trace(source, Profile::Adaptive, &[false]), [[0, 1, 2]]);
    assert_eq!(
        trace(source, Profile::Adaptive, &[true, false]),
        [[1, 0, 2]]
    );
}

#[test]
fn empty_and_identity_permutations_need_no_bindings() {
    let source = r#"
        operation Main() : Unit {
            use qs = Qubit[3];
            if M(qs[0]) == One {
                Relabel([], []);
                Relabel(qs, qs);
            }
            Probe(qs[0], qs[1], qs[2]);
        }
    "#;
    for profile in [Profile::Adaptive, Profile::AdaptiveRIF] {
        assert_eq!(trace(source, profile, &[true]), [[0, 1, 2]]);
        assert_eq!(trace(source, profile, &[false]), [[0, 1, 2]]);
    }
}

#[test]
fn aliases_and_runtime_indices_observe_later_relabeling() {
    let source = r#"
        operation Main() : Unit {
            use qs = Qubit[4];
            let first = qs[0];
            let old = [qs[0], qs[2]];
            mutable index = 0;
            if M(qs[3]) == One { set index = 2; }
            let selected = qs[index];
            let saved = [selected];
            if M(qs[3]) == One {
                Relabel(qs, [qs[1], qs[2], qs[3], qs[0]]);
            }
            Probe(first, selected, saved[0]);
            for q in old { Probe(q, q, q); }
        }
    "#;
    assert_eq!(
        trace(source, Profile::Adaptive, &[false, true]),
        [[1, 1, 1], [1, 1, 1], [3, 3, 3]],
    );
    assert_eq!(
        trace(source, Profile::Adaptive, &[true, true]),
        [[1, 3, 3], [1, 1, 1], [3, 3, 3]],
    );
    assert_eq!(
        trace(source, Profile::Adaptive, &[true, false]),
        [[0, 2, 2], [0, 0, 0], [2, 2, 2]],
    );
}

#[test]
fn static_relabeling_wrappers_update_caller_aliases() {
    let source = r#"
        operation SwapLabels(a : Qubit, b : Qubit) : Unit {
            Relabel([a, b], [b, a]);
        }
        operation Apply(action : Unit => Unit) : Unit { action(); }
        operation Main() : Unit {
            use qs = Qubit[3];
            let alias = qs[0];
            SwapLabels(qs[0], qs[1]);
            Probe(alias, qs[1], qs[2]);
            Apply(() => SwapLabels(qs[0], qs[2]));
            Probe(alias, qs[1], qs[2]);
        }
    "#;
    for profile in [Profile::Adaptive, Profile::AdaptiveRIF] {
        assert_eq!(trace(source, profile, &[]), [[1, 0, 2], [2, 0, 1]]);
    }
}

#[test]
fn relabel_through_callback_updates_caller_aliases() {
    let source = r#"
        operation ApplyIf(condition : Bool, action : (Unit => Unit)) : Unit {
            if condition { action(); }
        }
        operation SwapLabels(a : Qubit, b : Qubit) : Unit {
            Relabel([a, b], [b, a]);
        }
        operation Main() : Unit {
            use qs = Qubit[3];
            let alias = qs[0];
            ApplyIf(M(qs[2]) == One, () => SwapLabels(qs[0], qs[1]));
            Probe(alias, qs[1], qs[2]);
        }
    "#;
    for profile in [Profile::Adaptive, Profile::AdaptiveRIF] {
        assert_eq!(trace(source, profile, &[true]), [[1, 0, 2]]);
        assert_eq!(trace(source, profile, &[false]), [[0, 1, 2]]);
    }
}

#[test]
fn static_dynamic_and_adjoint_permutations_compose() {
    let source = r#"
        operation Main() : Unit {
            use qs = Qubit[4];
            Relabel(qs, [qs[1], qs[2], qs[0], qs[3]]);
            if M(qs[3]) == One {
                Adjoint Relabel(qs, [qs[1], qs[2], qs[0], qs[3]]);
            }
            Relabel(qs, [qs[1], qs[0], qs[3], qs[2]]);
            Probe(qs[0], qs[1], qs[2]);
            Probe(qs[3], qs[0], qs[1]);
        }
    "#;
    for profile in [Profile::Adaptive, Profile::AdaptiveRIF] {
        assert_eq!(trace(source, profile, &[true]), [[1, 0, 3], [2, 1, 0]]);
        assert_eq!(trace(source, profile, &[false]), [[2, 1, 3], [0, 2, 1]]);
    }
}

#[test]
fn released_labels_can_be_reused_after_dynamic_relabeling() {
    let source = r#"
        operation Main() : Unit {
            use keep = Qubit();
            {
                use temporary = Qubit();
                if M(keep) == One {
                    Relabel([keep, temporary], [temporary, keep]);
                }
                Reset(temporary);
            }
            use next = Qubit();
            Probe(keep, next, keep);
        }
    "#;
    for profile in [Profile::Adaptive, Profile::AdaptiveRIF] {
        assert_eq!(trace(source, profile, &[true]), [[1, 0, 1]]);
        assert_eq!(trace(source, profile, &[false]), [[0, 1, 0]]);
    }
}

#[test]
fn invalid_dynamic_permutations_still_fail_validation() {
    for permutation in ["[qs[0], qs[0]]", "[qs[0], qs[2]]"] {
        let source = format!(
            "operation Main() : Unit {{
                use qs = Qubit[3];
                if M(qs[0]) == One {{
                    Relabel([qs[0], qs[1]], {permutation});
                }}
            }}"
        );
        assert!(matches!(
            get_partial_evaluation_error(&source),
            Error::EvaluationFailed(..)
        ));
    }
}

#[test]
fn runtime_selected_permutations_report_an_error() {
    let source = r#"
        operation Main() : Unit {
            use qs = Qubit[3];
            let index = MResetZ(qs[2]) == One ? 1 | 0;
            let selected = qs[index];
            Relabel([selected], [selected]);
        }
    "#;
    assert!(matches!(
        get_partial_evaluation_error_with_capabilities(source, Profile::Adaptive.into()),
        Error::Unimplemented(message, _) if message == "relabeling runtime-selected qubits"
    ));
}

#[test]
fn dynamically_allocated_permutations_report_an_error() {
    let source = r#"
        operation Main() : Unit {
            use qs = Qubit[2];
            if M(qs[0]) == One { Relabel(qs, [qs[1], qs[0]]); }
        }
    "#;
    let capabilities = TargetCapabilityFlags::from(Profile::Adaptive)
        | TargetCapabilityFlags::DynamicQubitAllocation;
    assert!(matches!(
        get_partial_evaluation_error_with_capabilities(source, capabilities),
        Error::Unimplemented(message, _) if message == "relabeling runtime-selected qubits"
    ));
}
