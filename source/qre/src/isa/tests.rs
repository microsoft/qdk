// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn test_fixed_arity_instruction() {
    let instr = Instruction::fixed_arity(1, Encoding::Physical, 2, 100, Some(10), Some(5), 0.01);

    assert_eq!(instr.id(), 1);
    assert_eq!(instr.encoding(), Encoding::Physical);
    assert_eq!(instr.arity(), Some(2));
    assert_eq!(instr.time(None), Some(100));
    assert_eq!(instr.space(None), Some(10));
    assert_eq!(instr.length(None), Some(5));
    assert_eq!(instr.error_rate(None), Some(0.01));
}

#[test]
fn test_variable_arity_instruction() {
    let time_fn = VariableArityFunction::linear(10);
    let space_fn = VariableArityFunction::constant(5);
    let error_rate_fn = VariableArityFunction::constant(0.001);

    let instr =
        Instruction::variable_arity(2, Encoding::Logical, time_fn, space_fn, None, error_rate_fn);

    assert_eq!(instr.id(), 2);
    assert_eq!(instr.encoding(), Encoding::Logical);
    assert_eq!(instr.arity(), None);

    // Check evaluation at specific arity
    assert_eq!(instr.time(Some(3)), Some(30)); // 3 * 10
    assert_eq!(instr.space(Some(3)), Some(5));
    assert_eq!(instr.length(Some(3)), Some(5)); // Defaulted to space_fn
    assert_eq!(instr.error_rate(Some(3)), Some(0.001));

    // Check None arity returns None for variable metrics
    assert_eq!(instr.time(None), None);
}

#[test]
fn test_isa_satisfies() {
    let mut isa = ISA::new();
    let instr1 = Instruction::fixed_arity(1, Encoding::Physical, 2, 100, None, None, 0.01);
    isa.add_instruction(instr1);

    let mut reqs = ISARequirements::new();

    // Test exact match
    reqs.add_constraint(InstructionConstraint::new(
        1,
        Encoding::Physical,
        Some(2),
        Some(ConstraintBound::less_than(0.02)),
    ));
    assert!(isa.satisfies(&reqs));

    // Test failing error rate
    let mut reqs_fail = ISARequirements::new();
    reqs_fail.add_constraint(InstructionConstraint::new(
        1,
        Encoding::Physical,
        Some(2),
        Some(ConstraintBound::less_than(0.005)),
    ));
    assert!(!isa.satisfies(&reqs_fail));

    // Test failing arity
    let mut reqs_fail_arity = ISARequirements::new();
    reqs_fail_arity.add_constraint(InstructionConstraint::new(
        1,
        Encoding::Physical,
        Some(3),
        Some(ConstraintBound::less_than(0.02)),
    ));
    assert!(!isa.satisfies(&reqs_fail_arity));

    // Test failing encoding
    let mut reqs_fail_enc = ISARequirements::new();
    reqs_fail_enc.add_constraint(InstructionConstraint::new(
        1,
        Encoding::Logical,
        Some(2),
        None,
    ));
    assert!(!isa.satisfies(&reqs_fail_enc));

    // Test missing instruction
    let mut reqs_missing = ISARequirements::new();
    reqs_missing.add_constraint(InstructionConstraint::new(
        99,
        Encoding::Physical,
        None,
        None,
    ));
    assert!(!isa.satisfies(&reqs_missing));
}

#[test]
fn test_variable_arity_satisfies() {
    let mut isa = ISA::new();
    let time_fn = VariableArityFunction::linear(10);
    let space_fn = VariableArityFunction::constant(5);
    let error_rate_fn = VariableArityFunction::linear(0.001); // 0.001 * arity

    let instr = Instruction::variable_arity(
        10,
        Encoding::Logical,
        time_fn,
        space_fn,
        None,
        error_rate_fn,
    );
    isa.add_instruction(instr);

    let mut reqs = ISARequirements::new();
    // Check for arity 5, error rate should be 0.005
    reqs.add_constraint(InstructionConstraint::new(
        10,
        Encoding::Logical,
        Some(5),
        Some(ConstraintBound::less_than(0.01)),
    ));
    assert!(isa.satisfies(&reqs)); // 0.005 < 0.01

    let mut reqs_fail = ISARequirements::new();
    // Check for arity 20, error rate should be 0.02
    reqs_fail.add_constraint(InstructionConstraint::new(
        10,
        Encoding::Logical,
        Some(20),
        Some(ConstraintBound::less_than(0.01)),
    ));
    assert!(!isa.satisfies(&reqs_fail)); // 0.02 not < 0.01
}

#[test]
fn test_variable_arity_function() {
    let linear_fn = VariableArityFunction::linear(10);
    assert_eq!(linear_fn.evaluate(3), 30);
    assert_eq!(linear_fn.evaluate(0), 0);

    let constant_fn = VariableArityFunction::constant(5);
    assert_eq!(constant_fn.evaluate(3), 5);
    assert_eq!(constant_fn.evaluate(0), 5);

    // Test with a custom function
    let custom_fn = VariableArityFunction::generic(|arity| arity * arity); // Quadratic
    assert_eq!(custom_fn.evaluate(3), 9);
    assert_eq!(custom_fn.evaluate(4), 16);
}

#[test]
fn test_instruction_display_known_id() {
    use crate::trace::instruction_ids::H;

    let instr = Instruction::fixed_arity(H, Encoding::Physical, 1, 100, None, None, 0.01);
    let display = format!("{instr}");

    assert!(display.contains('H'), "Expected 'H' in '{display}'");
    assert!(
        display.contains("arity: 1"),
        "Expected 'arity: 1' in '{display}'"
    );
}

#[test]
fn test_instruction_display_unknown_id() {
    let unknown_id = 0x9999;
    let instr = Instruction::fixed_arity(unknown_id, Encoding::Logical, 2, 50, None, None, 0.001);
    let display = format!("{instr}");

    assert!(
        display.contains("??"),
        "Expected '??' for unknown ID in '{display}'"
    );
}

#[test]
fn test_instruction_display_variable_arity() {
    use crate::trace::instruction_ids::MULTI_PAULI_MEAS;

    let time_fn = VariableArityFunction::linear(10);
    let space_fn = VariableArityFunction::constant(5);
    let error_rate_fn = VariableArityFunction::constant(0.001);

    let instr = Instruction::variable_arity(
        MULTI_PAULI_MEAS,
        Encoding::Logical,
        time_fn,
        space_fn,
        None,
        error_rate_fn,
    );
    let display = format!("{instr}");

    assert!(
        display.contains("MULTI_PAULI_MEAS"),
        "Expected 'MULTI_PAULI_MEAS' in '{display}'"
    );
    // Variable arity instructions don't show arity
    assert!(
        !display.contains("arity:"),
        "Variable arity should not show arity in '{display}'"
    );
}
