// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_rir::rir::{self, ConditionCode, FcmpConditionCode};
use qsc_rir::utils::get_all_block_successors;
use std::fmt::Write;

/// A trait for converting a type into QIR of type `T`.
/// This can be used to generate QIR strings or other representations.
pub trait ToQir<T> {
    fn to_qir(&self, program: &rir::Program) -> T;
}

impl ToQir<String> for rir::Literal {
    fn to_qir(&self, _program: &rir::Program) -> String {
        match self {
            rir::Literal::Bool(b) => format!("i1 {b}"),
            rir::Literal::Double(d) => {
                if (d.floor() - d.ceil()).abs() < f64::EPSILON {
                    // The value is a whole number, which requires at least one decimal point
                    // to differentiate it from an integer value.
                    format!("double {d:.1}")
                } else {
                    format!("double {d}")
                }
            }
            rir::Literal::Integer(i) => format!("i64 {i}"),
            rir::Literal::Pointer => "ptr null".to_string(),
            rir::Literal::Qubit(q) => format!("ptr inttoptr (i64 {q} to ptr)"),
            rir::Literal::Result(r) => format!("ptr inttoptr (i64 {r} to ptr)"),
            rir::Literal::Tag(idx, _) => format!("ptr @{idx}"),
            rir::Literal::EmptyTag => "ptr @empty_tag".to_string(),
        }
    }
}

impl ToQir<String> for rir::Ty {
    fn to_qir(&self, _program: &rir::Program) -> String {
        match self {
            rir::Ty::Boolean => "i1".to_string(),
            rir::Ty::Double => "double".to_string(),
            rir::Ty::Integer => "i64".to_string(),
            rir::Ty::Pointer | rir::Ty::Qubit | rir::Ty::Result => "ptr".to_string(),
        }
    }
}

impl ToQir<String> for Option<rir::Ty> {
    fn to_qir(&self, program: &rir::Program) -> String {
        match self {
            Some(ty) => ToQir::<String>::to_qir(ty, program),
            None => "void".to_string(),
        }
    }
}

impl ToQir<String> for rir::VariableId {
    fn to_qir(&self, _program: &rir::Program) -> String {
        format!("%var_{}", self.0)
    }
}

impl ToQir<String> for rir::Variable {
    fn to_qir(&self, program: &rir::Program) -> String {
        format!(
            "{} {}",
            ToQir::<String>::to_qir(&self.ty, program),
            ToQir::<String>::to_qir(&self.variable_id, program)
        )
    }
}

impl ToQir<String> for rir::Operand {
    fn to_qir(&self, program: &rir::Program) -> String {
        match self {
            rir::Operand::Literal(lit) => ToQir::<String>::to_qir(lit, program),
            rir::Operand::Variable(var) => ToQir::<String>::to_qir(var, program),
        }
    }
}

impl ToQir<String> for rir::FcmpConditionCode {
    fn to_qir(&self, _program: &rir::Program) -> String {
        match self {
            rir::FcmpConditionCode::False => "false".to_string(),
            rir::FcmpConditionCode::OrderedAndEqual => "oeq".to_string(),
            rir::FcmpConditionCode::OrderedAndGreaterThan => "ogt".to_string(),
            rir::FcmpConditionCode::OrderedAndGreaterThanOrEqual => "oge".to_string(),
            rir::FcmpConditionCode::OrderedAndLessThan => "olt".to_string(),
            rir::FcmpConditionCode::OrderedAndLessThanOrEqual => "ole".to_string(),
            rir::FcmpConditionCode::OrderedAndNotEqual => "one".to_string(),
            rir::FcmpConditionCode::Ordered => "ord".to_string(),
            rir::FcmpConditionCode::UnorderedOrEqual => "ueq".to_string(),
            rir::FcmpConditionCode::UnorderedOrGreaterThan => "ugt".to_string(),
            rir::FcmpConditionCode::UnorderedOrGreaterThanOrEqual => "uge".to_string(),
            rir::FcmpConditionCode::UnorderedOrLessThan => "ult".to_string(),
            rir::FcmpConditionCode::UnorderedOrLessThanOrEqual => "ule".to_string(),
            rir::FcmpConditionCode::UnorderedOrNotEqual => "une".to_string(),
            rir::FcmpConditionCode::Unordered => "uno".to_string(),
            rir::FcmpConditionCode::True => "true".to_string(),
        }
    }
}

impl ToQir<String> for rir::ConditionCode {
    fn to_qir(&self, _program: &rir::Program) -> String {
        match self {
            rir::ConditionCode::Eq => "eq".to_string(),
            rir::ConditionCode::Ne => "ne".to_string(),
            rir::ConditionCode::Sgt => "sgt".to_string(),
            rir::ConditionCode::Sge => "sge".to_string(),
            rir::ConditionCode::Slt => "slt".to_string(),
            rir::ConditionCode::Sle => "sle".to_string(),
        }
    }
}

impl ToQir<String> for rir::Instruction {
    fn to_qir(&self, program: &rir::Program) -> String {
        match self {
            rir::Instruction::Add(lhs, rhs, variable) => {
                binop_to_qir("add", lhs, rhs, *variable, program)
            }
            rir::Instruction::Ashr(lhs, rhs, variable) => {
                binop_to_qir("ashr", lhs, rhs, *variable, program)
            }
            rir::Instruction::BitwiseAnd(lhs, rhs, variable) => {
                simple_bitwise_to_qir("and", lhs, rhs, *variable, program)
            }
            rir::Instruction::BitwiseNot(value, variable) => {
                bitwise_not_to_qir(value, *variable, program)
            }
            rir::Instruction::BitwiseOr(lhs, rhs, variable) => {
                simple_bitwise_to_qir("or", lhs, rhs, *variable, program)
            }
            rir::Instruction::BitwiseXor(lhs, rhs, variable) => {
                simple_bitwise_to_qir("xor", lhs, rhs, *variable, program)
            }
            rir::Instruction::Branch(cond, true_id, false_id) => {
                format!(
                    "  br {}, label %{}, label %{}",
                    ToQir::<String>::to_qir(cond, program),
                    ToQir::<String>::to_qir(true_id, program),
                    ToQir::<String>::to_qir(false_id, program)
                )
            }
            rir::Instruction::Call(call_id, args, output) => {
                call_to_qir(args, *call_id, *output, program)
            }
            rir::Instruction::Fadd(lhs, rhs, variable) => {
                fbinop_to_qir("fadd", lhs, rhs, *variable, program)
            }
            rir::Instruction::Fdiv(lhs, rhs, variable) => {
                fbinop_to_qir("fdiv", lhs, rhs, *variable, program)
            }
            rir::Instruction::Fmul(lhs, rhs, variable) => {
                fbinop_to_qir("fmul", lhs, rhs, *variable, program)
            }
            rir::Instruction::Fsub(lhs, rhs, variable) => {
                fbinop_to_qir("fsub", lhs, rhs, *variable, program)
            }
            rir::Instruction::LogicalAnd(lhs, rhs, variable) => {
                logical_binop_to_qir("and", lhs, rhs, *variable, program)
            }
            rir::Instruction::LogicalNot(value, variable) => {
                logical_not_to_qir(value, *variable, program)
            }
            rir::Instruction::LogicalOr(lhs, rhs, variable) => {
                logical_binop_to_qir("or", lhs, rhs, *variable, program)
            }
            rir::Instruction::Mul(lhs, rhs, variable) => {
                binop_to_qir("mul", lhs, rhs, *variable, program)
            }
            rir::Instruction::Fcmp(op, lhs, rhs, variable) => {
                fcmp_to_qir(*op, lhs, rhs, *variable, program)
            }
            rir::Instruction::Icmp(op, lhs, rhs, variable) => {
                icmp_to_qir(*op, lhs, rhs, *variable, program)
            }
            rir::Instruction::Jump(block_id) => {
                format!("  br label %{}", ToQir::<String>::to_qir(block_id, program))
            }
            rir::Instruction::Phi(args, variable) => phi_to_qir(args, *variable, program),
            rir::Instruction::Return => "  ret i64 0".to_string(),
            rir::Instruction::Sdiv(lhs, rhs, variable) => {
                binop_to_qir("sdiv", lhs, rhs, *variable, program)
            }
            rir::Instruction::Shl(lhs, rhs, variable) => {
                binop_to_qir("shl", lhs, rhs, *variable, program)
            }
            rir::Instruction::Srem(lhs, rhs, variable) => {
                binop_to_qir("srem", lhs, rhs, *variable, program)
            }
            rir::Instruction::Store(operand, variable) => {
                store_to_qir(*operand, *variable, program)
            }
            rir::Instruction::Sub(lhs, rhs, variable) => {
                binop_to_qir("sub", lhs, rhs, *variable, program)
            }
            rir::Instruction::Convert(operand, variable) => {
                convert_to_qir(operand, *variable, program)
            }
            rir::Instruction::Advanced(instr) => ToQir::<String>::to_qir(instr, program),
        }
    }
}

impl ToQir<String> for rir::AdvancedInstr {
    fn to_qir(&self, program: &rir::Program) -> String {
        match self {
            rir::AdvancedInstr::Alloca(size, variable) => alloca_to_qir(*size, *variable, program),
            rir::AdvancedInstr::Load(var_from, var_to) => load_to_qir(*var_from, *var_to, program),
        }
    }
}

fn convert_to_qir(
    operand: &rir::Operand,
    variable: rir::Variable,
    program: &rir::Program,
) -> String {
    let operand_ty = get_value_ty(operand);
    let var_ty = get_variable_ty(variable);
    assert_ne!(
        operand_ty, var_ty,
        "input/output types ({operand_ty}, {var_ty}) should not match in convert"
    );

    let convert_instr = match (operand_ty, var_ty) {
        ("i64", "double") => "sitofp i64",
        ("double", "i64") => "fptosi double",
        _ => panic!("unsupported conversion from {operand_ty} to {var_ty} in convert instruction"),
    };

    format!(
        "  {} = {convert_instr} {} to {var_ty}",
        ToQir::<String>::to_qir(&variable.variable_id, program),
        get_value_as_str(operand, program),
    )
}

fn store_to_qir(operand: rir::Operand, variable: rir::Variable, program: &rir::Program) -> String {
    let op_ty = get_value_ty(&operand);
    format!(
        "  store {op_ty} {}, ptr {}",
        get_value_as_str(&operand, program),
        ToQir::<String>::to_qir(&variable.variable_id, program)
    )
}

fn load_to_qir(var_from: rir::Variable, var_to: rir::Variable, program: &rir::Program) -> String {
    let var_to_ty = get_variable_ty(var_to);
    format!(
        "  {} = load {var_to_ty}, ptr {}",
        ToQir::<String>::to_qir(&var_to.variable_id, program),
        ToQir::<String>::to_qir(&var_from.variable_id, program)
    )
}

fn alloca_to_qir(size: Option<u64>, variable: rir::Variable, program: &rir::Program) -> String {
    if let Some(_size) = size {
        // TODO(swernli): We would need to a way to ensure we get the inner type, since the variable will be something
        // like `[i64 x 10]` and we want to alloca `i64`.
        // let variable_ty = get_variable_inner_ty(variable);
        // format!(
        //     "  {} = alloca {variable_ty}, i64 {size}",
        //     ToQir::<String>::to_qir(&variable.variable_id, program),
        // )
        todo!("alloca with size")
    } else {
        let variable_ty = get_variable_ty(variable);
        format!(
            "  {} = alloca {variable_ty}",
            ToQir::<String>::to_qir(&variable.variable_id, program)
        )
    }
}

pub(crate) fn logical_not_to_qir(
    value: &rir::Operand,
    variable: rir::Variable,
    program: &rir::Program,
) -> String {
    let value_ty = get_value_ty(value);
    let var_ty = get_variable_ty(variable);
    assert_eq!(
        value_ty, var_ty,
        "mismatched input/output types ({value_ty}, {var_ty}) for not"
    );
    assert_eq!(var_ty, "i1", "unsupported type {var_ty} for not");

    format!(
        "  {} = xor i1 {}, true",
        ToQir::<String>::to_qir(&variable.variable_id, program),
        get_value_as_str(value, program)
    )
}

pub(crate) fn logical_binop_to_qir(
    op: &str,
    lhs: &rir::Operand,
    rhs: &rir::Operand,
    variable: rir::Variable,
    program: &rir::Program,
) -> String {
    let lhs_ty = get_value_ty(lhs);
    let rhs_ty = get_value_ty(rhs);
    let var_ty = get_variable_ty(variable);
    assert_eq!(
        lhs_ty, rhs_ty,
        "mismatched input types ({lhs_ty}, {rhs_ty}) for {op}"
    );
    assert_eq!(
        lhs_ty, var_ty,
        "mismatched input/output types ({lhs_ty}, {var_ty}) for {op}"
    );
    assert_eq!(var_ty, "i1", "unsupported type {var_ty} for {op}");

    format!(
        "  {} = {op} {var_ty} {}, {}",
        ToQir::<String>::to_qir(&variable.variable_id, program),
        get_value_as_str(lhs, program),
        get_value_as_str(rhs, program)
    )
}

pub(crate) fn bitwise_not_to_qir(
    value: &rir::Operand,
    variable: rir::Variable,
    program: &rir::Program,
) -> String {
    let value_ty = get_value_ty(value);
    let var_ty = get_variable_ty(variable);
    assert_eq!(
        value_ty, var_ty,
        "mismatched input/output types ({value_ty}, {var_ty}) for not"
    );
    assert_eq!(var_ty, "i64", "unsupported type {var_ty} for not");

    format!(
        "  {} = xor {var_ty} {}, -1",
        ToQir::<String>::to_qir(&variable.variable_id, program),
        get_value_as_str(value, program)
    )
}

pub(crate) fn call_to_qir(
    args: &[rir::Operand],
    call_id: rir::CallableId,
    output: Option<rir::Variable>,
    program: &rir::Program,
) -> String {
    let args = args
        .iter()
        .map(|arg| ToQir::<String>::to_qir(arg, program))
        .collect::<Vec<_>>()
        .join(", ");
    let callable = program.get_callable(call_id);
    if let Some(output) = output {
        format!(
            "  {} = call {} @{}({args})",
            ToQir::<String>::to_qir(&output.variable_id, program),
            ToQir::<String>::to_qir(&callable.output_type, program),
            callable.name
        )
    } else {
        format!(
            "  call {} @{}({args})",
            ToQir::<String>::to_qir(&callable.output_type, program),
            callable.name
        )
    }
}

pub(crate) fn fcmp_to_qir(
    op: FcmpConditionCode,
    lhs: &rir::Operand,
    rhs: &rir::Operand,
    variable: rir::Variable,
    program: &rir::Program,
) -> String {
    let lhs_ty = get_value_ty(lhs);
    let rhs_ty = get_value_ty(rhs);
    let var_ty = get_variable_ty(variable);
    assert_eq!(
        lhs_ty, rhs_ty,
        "mismatched input types ({lhs_ty}, {rhs_ty}) for fcmp {op}"
    );

    assert_eq!(var_ty, "i1", "unsupported output type {var_ty} for fcmp");
    format!(
        "  {} = fcmp {} {lhs_ty} {}, {}",
        ToQir::<String>::to_qir(&variable.variable_id, program),
        ToQir::<String>::to_qir(&op, program),
        get_value_as_str(lhs, program),
        get_value_as_str(rhs, program)
    )
}

pub(crate) fn icmp_to_qir(
    op: ConditionCode,
    lhs: &rir::Operand,
    rhs: &rir::Operand,
    variable: rir::Variable,
    program: &rir::Program,
) -> String {
    let lhs_ty = get_value_ty(lhs);
    let rhs_ty = get_value_ty(rhs);
    let var_ty = get_variable_ty(variable);
    assert_eq!(
        lhs_ty, rhs_ty,
        "mismatched input types ({lhs_ty}, {rhs_ty}) for icmp {op}"
    );

    assert_eq!(var_ty, "i1", "unsupported output type {var_ty} for icmp");
    format!(
        "  {} = icmp {} {lhs_ty} {}, {}",
        ToQir::<String>::to_qir(&variable.variable_id, program),
        ToQir::<String>::to_qir(&op, program),
        get_value_as_str(lhs, program),
        get_value_as_str(rhs, program)
    )
}

pub(crate) fn binop_to_qir(
    op: &str,
    lhs: &rir::Operand,
    rhs: &rir::Operand,
    variable: rir::Variable,
    program: &rir::Program,
) -> String {
    let lhs_ty = get_value_ty(lhs);
    let rhs_ty = get_value_ty(rhs);
    let var_ty = get_variable_ty(variable);
    assert_eq!(
        lhs_ty, rhs_ty,
        "mismatched input types ({lhs_ty}, {rhs_ty}) for {op}"
    );
    assert_eq!(
        lhs_ty, var_ty,
        "mismatched input/output types ({lhs_ty}, {var_ty}) for {op}"
    );
    assert_eq!(var_ty, "i64", "unsupported type {var_ty} for {op}");

    format!(
        "  {} = {op} {var_ty} {}, {}",
        ToQir::<String>::to_qir(&variable.variable_id, program),
        get_value_as_str(lhs, program),
        get_value_as_str(rhs, program)
    )
}

pub(crate) fn fbinop_to_qir(
    op: &str,
    lhs: &rir::Operand,
    rhs: &rir::Operand,
    variable: rir::Variable,
    program: &rir::Program,
) -> String {
    let lhs_ty = get_value_ty(lhs);
    let rhs_ty = get_value_ty(rhs);
    let var_ty = get_variable_ty(variable);
    assert_eq!(
        lhs_ty, rhs_ty,
        "mismatched input types ({lhs_ty}, {rhs_ty}) for {op}"
    );
    assert_eq!(
        lhs_ty, var_ty,
        "mismatched input/output types ({lhs_ty}, {var_ty}) for {op}"
    );
    assert_eq!(var_ty, "double", "unsupported type {var_ty} for {op}");

    format!(
        "  {} = {op} {var_ty} {}, {}",
        ToQir::<String>::to_qir(&variable.variable_id, program),
        get_value_as_str(lhs, program),
        get_value_as_str(rhs, program)
    )
}

pub(crate) fn simple_bitwise_to_qir(
    op: &str,
    lhs: &rir::Operand,
    rhs: &rir::Operand,
    variable: rir::Variable,
    program: &rir::Program,
) -> String {
    let lhs_ty = get_value_ty(lhs);
    let rhs_ty = get_value_ty(rhs);
    let var_ty = get_variable_ty(variable);
    assert_eq!(
        lhs_ty, rhs_ty,
        "mismatched input types ({lhs_ty}, {rhs_ty}) for {op}"
    );
    assert_eq!(
        lhs_ty, var_ty,
        "mismatched input/output types ({lhs_ty}, {var_ty}) for {op}"
    );
    assert_eq!(var_ty, "i64", "unsupported type {var_ty} for {op}");

    format!(
        "  {} = {op} {var_ty} {}, {}",
        ToQir::<String>::to_qir(&variable.variable_id, program),
        get_value_as_str(lhs, program),
        get_value_as_str(rhs, program)
    )
}

pub(crate) fn phi_to_qir(
    args: &[(rir::Operand, rir::BlockId)],
    variable: rir::Variable,
    program: &rir::Program,
) -> String {
    assert!(
        !args.is_empty(),
        "phi instruction should have at least one argument"
    );
    let var_ty = get_variable_ty(variable);
    let args = args
        .iter()
        .map(|(arg, block_id)| {
            let arg_ty = get_value_ty(arg);
            assert_eq!(
                arg_ty, var_ty,
                "mismatched types ({var_ty} [... {arg_ty}]) for phi"
            );
            format!(
                "[{}, %{}]",
                get_value_as_str(arg, program),
                ToQir::<String>::to_qir(block_id, program)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "  {} = phi {var_ty} {args}",
        ToQir::<String>::to_qir(&variable.variable_id, program)
    )
}

pub(crate) fn get_value_as_str(value: &rir::Operand, program: &rir::Program) -> String {
    match value {
        rir::Operand::Literal(lit) => match lit {
            rir::Literal::Bool(b) => format!("{b}"),
            rir::Literal::Double(d) => {
                if (d.floor() - d.ceil()).abs() < f64::EPSILON {
                    // The value is a whole number, which requires at least one decimal point
                    // to differentiate it from an integer value.
                    format!("{d:.1}")
                } else {
                    format!("{d}")
                }
            }
            rir::Literal::Integer(i) => format!("{i}"),
            rir::Literal::Pointer => "null".to_string(),
            rir::Literal::Qubit(q) => format!("{q}"),
            rir::Literal::Result(r) => format!("{r}"),
            rir::Literal::Tag(..) | rir::Literal::EmptyTag => panic!(
                "tag literals should not be used as string values outside of output recording"
            ),
        },
        rir::Operand::Variable(var) => ToQir::<String>::to_qir(&var.variable_id, program),
    }
}

pub(crate) fn get_value_ty(lhs: &rir::Operand) -> &str {
    match lhs {
        rir::Operand::Literal(lit) => match lit {
            rir::Literal::Integer(_) => "i64",
            rir::Literal::Bool(_) => "i1",
            rir::Literal::Double(_) => get_f64_ty(),
            rir::Literal::Qubit(_)
            | rir::Literal::Result(_)
            | rir::Literal::Pointer
            | rir::Literal::Tag(..)
            | rir::Literal::EmptyTag => "ptr",
        },
        rir::Operand::Variable(var) => get_variable_ty(*var),
    }
}

pub(crate) fn get_variable_ty(variable: rir::Variable) -> &'static str {
    match variable.ty {
        rir::Ty::Integer => "i64",
        rir::Ty::Boolean => "i1",
        rir::Ty::Double => get_f64_ty(),
        rir::Ty::Qubit | rir::Ty::Result | rir::Ty::Pointer => "ptr",
    }
}

/// phi only supports "Floating-Point Types" which are defined as:
/// - `half` (`f16`)
/// - `bfloat`
/// - `float` (`f32`)
/// - `double` (`f64`)
/// - `fp128`
///
/// We only support `f64`, so we break the pattern used for integers
/// and have to use `double` here.
///
/// This conflicts with the QIR spec which says f64. Need to follow up on this.
pub(crate) fn get_f64_ty() -> &'static str {
    "double"
}

impl ToQir<String> for rir::BlockId {
    fn to_qir(&self, _program: &rir::Program) -> String {
        format!("block_{}", self.0)
    }
}

impl ToQir<String> for rir::Block {
    fn to_qir(&self, program: &rir::Program) -> String {
        self.0
            .iter()
            .map(|instr| ToQir::<String>::to_qir(instr, program))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl ToQir<String> for rir::Callable {
    fn to_qir(&self, program: &rir::Program) -> String {
        let input_type = self
            .input_type
            .iter()
            .map(|t| ToQir::<String>::to_qir(t, program))
            .collect::<Vec<_>>()
            .join(", ");
        let output_type = ToQir::<String>::to_qir(&self.output_type, program);
        let Some(entry_id) = self.body else {
            return format!(
                "declare {output_type} @{}({input_type}){}",
                self.name,
                if matches!(
                    self.call_type,
                    rir::CallableType::Measurement | rir::CallableType::Reset
                ) {
                    // These callables are a special case that need the irreversible attribute.
                    " #1"
                } else {
                    ""
                }
            );
        };
        let mut body = String::new();
        let mut all_blocks = vec![entry_id];
        all_blocks.extend(get_all_block_successors(entry_id, program));
        for block_id in all_blocks {
            let block = program.get_block(block_id);
            write!(
                body,
                "{}:\n{}\n",
                ToQir::<String>::to_qir(&block_id, program),
                ToQir::<String>::to_qir(block, program)
            )
            .expect("writing to string should succeed");
        }
        assert!(
            input_type.is_empty(),
            "entry point should not have an input"
        );
        format!("define {output_type} @ENTRYPOINT__main() #0 {{\n{body}}}",)
    }
}

impl ToQir<String> for rir::Program {
    fn to_qir(&self, _program: &rir::Program) -> String {
        let callables = self
            .callables
            .iter()
            .map(|(_, callable)| ToQir::<String>::to_qir(callable, self))
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut constants = String::default();
        for (idx, tag) in self.tags.iter().enumerate() {
            // We need to add the tag as a global constant.
            writeln!(
                constants,
                "@{idx} = internal constant [{} x i8] c\"{tag}\\00\"",
                tag.len() + 1
            )
            .expect("writing to string should succeed");
        }
        let body = format!(
            include_str!("./v2/template.ll"),
            constants, callables, "advanced_profile", self.num_qubits, self.num_results
        );
        let flags = get_module_metadata(self);
        body + "\n" + &flags
    }
}

/// Create the module metadata for the given program.
/// creating the `llvm.module.flags` and its associated values.
pub(crate) fn get_module_metadata(_program: &rir::Program) -> String {
    let mut flags = String::new();

    // push the default attrs, we don't have any config values
    // for now that would change any of them.
    flags.push_str(
        r#"
!0 = !{i32 1, !"qir_major_version", i32 2}
!1 = !{i32 7, !"qir_minor_version", i32 0}
!2 = !{i32 1, !"dynamic_qubit_management", i1 false}
!3 = !{i32 1, !"dynamic_result_management", i1 false}
!4 = !{i32 5, !"int_computations", !{!"i64"}}
!5 = !{i32 5, !"float_computations", !{!"double"}}
!6 = !{i32 1, !"backwards_branching", i2 3}
"#,
    );

    let index = 7;
    let mut metadata_def = String::new();
    metadata_def.push_str("!llvm.module.flags = !{");
    for i in 0..index - 1 {
        write!(metadata_def, "!{i}, ").expect("writing to string should succeed");
    }
    writeln!(metadata_def, "!{}}}", index - 1).expect("writing to string should succeed");
    metadata_def + &flags
}
