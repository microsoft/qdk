// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::model::Type;
use crate::model::{Instruction, Operand};

use super::spec::{QUBIT_TYPE_NAME, RESULT_TYPE_NAME};

/// Build a void call instruction: `call void @callee(args...)`.
#[must_use]
pub fn void_call(callee: &str, args: Vec<(Type, Operand)>) -> Instruction {
    Instruction::Call {
        callee: callee.to_string(),
        args,
        return_ty: None,
        result: None,
        attr_refs: Vec::new(),
    }
}

/// Build a qubit operand: `inttoptr (i64 <id> to %Qubit*)`.
#[must_use]
pub fn qubit_op(id: u32) -> (Type, Operand) {
    (
        Type::NamedPtr(QUBIT_TYPE_NAME.to_string()),
        Operand::int_to_named_ptr(i64::from(id), QUBIT_TYPE_NAME),
    )
}

/// Build a result operand: `inttoptr (i64 <id> to %Result*)`.
#[must_use]
pub fn result_op(id: u32) -> (Type, Operand) {
    (
        Type::NamedPtr(RESULT_TYPE_NAME.to_string()),
        Operand::int_to_named_ptr(i64::from(id), RESULT_TYPE_NAME),
    )
}

/// Build a `double` constant operand pair.
#[must_use]
pub fn double_op(val: f64) -> (Type, Operand) {
    (Type::Double, Operand::float_const(Type::Double, val))
}

/// Build an `i64` constant operand pair.
#[must_use]
pub fn i64_op(val: i64) -> (Type, Operand) {
    (Type::Integer(64), Operand::IntConst(Type::Integer(64), val))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Type;
    use crate::model::{Instruction, Operand};

    #[test]
    fn test_void_call() {
        let instr = void_call("__quantum__qis__h__body", vec![qubit_op(0)]);
        match &instr {
            Instruction::Call {
                callee,
                args,
                return_ty,
                result,
                attr_refs,
            } => {
                assert_eq!(callee, "__quantum__qis__h__body");
                assert_eq!(args.len(), 1);
                assert!(return_ty.is_none());
                assert!(result.is_none());
                assert!(attr_refs.is_empty());
            }
            _ => panic!("expected Instruction::Call"),
        }
    }

    #[test]
    fn test_qubit_op() {
        let (ty, op) = qubit_op(3);
        assert_eq!(ty, Type::NamedPtr("Qubit".to_string()));
        assert_eq!(
            op,
            Operand::IntToPtr(3, Type::NamedPtr("Qubit".to_string()))
        );
    }

    #[test]
    fn test_result_op() {
        let (ty, op) = result_op(5);
        assert_eq!(ty, Type::NamedPtr("Result".to_string()));
        assert_eq!(
            op,
            Operand::IntToPtr(5, Type::NamedPtr("Result".to_string()))
        );
    }

    #[test]
    fn test_double_op() {
        let (ty, op) = double_op(std::f64::consts::PI);
        assert_eq!(ty, Type::Double);
        assert_eq!(op, Operand::float_const(Type::Double, std::f64::consts::PI));
    }

    #[test]
    fn test_i64_op() {
        let (ty, op) = i64_op(42);
        assert_eq!(ty, Type::Integer(64));
        assert_eq!(op, Operand::IntConst(Type::Integer(64), 42));
    }
}
