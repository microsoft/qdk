// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_snapshot::{Budget, Error, length, snapshot_capnp as wire, string};

use crate::{
    AngleExpr, AngleId, AngleOp, BinaryOp, BlockId, BoxBody, ControlFlow, CustomOperation, Expr,
    ExprId, ForLoop, IfElse, Literal, LoopIndex, PackedOperation, ParamValue, Parameter,
    ParameterId, Snapshot, SnapshotInstruction, StandardGate, Switch, TimeUnit, UnitaryOperation,
    Var, VarId, VarType, WhileLoop,
};

macro_rules! enum_codec {
    ($encode:ident, $decode:ident, $native:ident, $wire:ident, {$($a:ident => $b:ident),+ $(,)?}) => {
        fn $encode(value: $native) -> wire::$wire {
            match value { $($native::$a => wire::$wire::$b),+ }
        }
        fn $decode(value: wire::$wire) -> Result<$native, Error> {
            match value {
                $(wire::$wire::$b => Ok($native::$a)),+,
                wire::$wire::Invalid => Err(Error::malformed(concat!("invalid ", stringify!($wire)))),
            }
        }
    };
}

enum_codec!(write_gate, read_gate, StandardGate, StandardGate, {
    I=>I, X=>X, Y=>Y, Z=>Z, H=>H, S=>S, SDag=>Sdg, SqrtX=>Sx, SqrtXDag=>Sxdg,
    T=>T, TDag=>Tdg, Cx=>Cx, Cy=>Cy, Cz=>Cz, Swap=>Swap, Ccx=>Ccx, Cswap=>Cswap,
    Rx=>Rx, Ry=>Ry, Rz=>Rz, Rxx=>Rxx, Ryy=>Ryy, Rzz=>Rzz, Crx=>Crx, Cry=>Cry,
    Crz=>Crz, MeasureX=>MeasureX, MeasureY=>MeasureY, MeasureZ=>MeasureZ,
    PrepareX=>PrepareX, PrepareY=>PrepareY, PrepareZ=>PrepareZ,
});
enum_codec!(write_time, read_time, TimeUnit, TimeUnit, {Dt=>Dt,Ns=>Ns,Us=>Us,Ms=>Ms,S=>S});
enum_codec!(write_var_type, read_var_type, VarType, VariableType, {Bool=>Boolean,Int=>Integer,Float=>Real});
enum_codec!(write_angle_op, read_angle_op, AngleOp, AngleOp, {Add=>Add,Sub=>Sub,Mul=>Mul,Div=>Div});
enum_codec!(write_binary_op, read_binary_op, BinaryOp, BinaryOp, {
    Eq=>Eq,Ne=>Ne,Lt=>Lt,Le=>Le,Gt=>Gt,Ge=>Ge,And=>And,Or=>Or,Add=>Add,Sub=>Sub,Mul=>Mul,
});

pub fn write(snapshot: &Snapshot, mut out: wire::circuit_store::Builder<'_>) -> Result<(), Error> {
    out.set_version(snapshot.schema_version);
    out.set_qubit_count(snapshot.qubit_count);
    out.set_clbit_count(snapshot.clbit_count);
    out.reborrow()
        .init_global_phase()
        .set_bits(snapshot.global_phase.to_bits());
    let mut parameters = out
        .reborrow()
        .init_parameters(length(snapshot.parameters.len())?);
    for (i, parameter) in snapshot.parameters.iter().enumerate() {
        parameters.set(length(i)?, &parameter.name);
    }
    let mut variables = out.reborrow().init_variables(length(snapshot.vars.len())?);
    for (i, var) in snapshot.vars.iter().enumerate() {
        let mut value = variables.reborrow().get(length(i)?);
        value.set_name(&var.name);
        value.set_type(write_var_type(var.ty));
    }
    let mut expressions = out
        .reborrow()
        .init_expressions(length(snapshot.exprs.len())?);
    for (i, expr) in snapshot.exprs.iter().enumerate() {
        write_expr(*expr, expressions.reborrow().get(length(i)?));
    }
    let mut angles = out.reborrow().init_angles(length(snapshot.angles.len())?);
    for (i, angle) in snapshot.angles.iter().enumerate() {
        write_angle(*angle, angles.reborrow().get(length(i)?));
    }
    let mut blocks = out.reborrow().init_blocks(length(snapshot.blocks.len())?);
    for (i, block) in snapshot.blocks.iter().enumerate() {
        let mut list = blocks
            .reborrow()
            .get(length(i)?)
            .init_instructions(length(block.len())?);
        for (index, instruction) in block.iter().enumerate() {
            write_instruction(instruction, list.reborrow().get(length(index)?))?;
        }
    }
    let mut list = out.init_instructions(length(snapshot.instructions.len())?);
    for (i, instruction) in snapshot.instructions.iter().enumerate() {
        write_instruction(instruction, list.reborrow().get(length(i)?))?;
    }
    Ok(())
}

pub fn read(
    input: wire::circuit_store::Reader<'_>,
    budget: &mut Budget,
) -> Result<Snapshot, Error> {
    if input.get_version() != qsc_snapshot::SCHEMA_VERSION {
        return Err(Error::malformed(format!(
            "unsupported circuit schema version {}",
            input.get_version()
        )));
    }
    let parameters = input.get_parameters()?;
    budget.charge(parameters.len() as usize)?;
    let parameters = parameters
        .iter()
        .map(|v| Ok(Parameter { name: string(v?)? }))
        .collect::<Result<_, Error>>()?;
    let variables = input.get_variables()?;
    budget.charge(variables.len() as usize)?;
    let vars = variables
        .iter()
        .map(|v| {
            Ok(Var {
                name: string(v.get_name()?)?,
                ty: read_var_type(v.get_type()?)?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let expressions = input.get_expressions()?;
    budget.charge(expressions.len() as usize)?;
    let exprs = expressions
        .iter()
        .map(read_expr)
        .collect::<Result<_, _>>()?;
    let angles = input.get_angles()?;
    budget.charge(angles.len() as usize)?;
    let angles = angles.iter().map(read_angle).collect::<Result<_, _>>()?;
    let blocks = input.get_blocks()?;
    budget.charge(blocks.len() as usize)?;
    let blocks = blocks
        .iter()
        .map(|v| {
            let list = v.get_instructions()?;
            budget.charge(list.len() as usize)?;
            list.iter()
                .map(|v| read_instruction(v, budget))
                .collect::<Result<Vec<_>, Error>>()
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let list = input.get_instructions()?;
    budget.charge(list.len() as usize)?;
    let instructions = list
        .iter()
        .map(|v| read_instruction(v, budget))
        .collect::<Result<_, _>>()?;
    Ok(Snapshot {
        schema_version: input.get_version(),
        qubit_count: input.get_qubit_count(),
        clbit_count: input.get_clbit_count(),
        global_phase: f64::from_bits(input.get_global_phase()?.get_bits()),
        parameters,
        vars,
        exprs,
        angles,
        blocks,
        instructions,
    })
}

fn write_instruction(
    value: &SnapshotInstruction,
    mut out: wire::instruction::Builder<'_>,
) -> Result<(), Error> {
    write_operation(&value.operation, out.reborrow().init_operation())?;
    let mut qubits = out.reborrow().init_qubits(length(value.qubits.len())?);
    for (i, v) in value.qubits.iter().enumerate() {
        qubits.set(length(i)?, *v);
    }
    let mut clbits = out.reborrow().init_clbits(length(value.clbits.len())?);
    for (i, v) in value.clbits.iter().enumerate() {
        clbits.set(length(i)?, *v);
    }
    let mut params = out.init_parameters();
    match &value.params {
        None => params.set_none(()),
        Some(values) => {
            let mut list = params.init_some(length(values.len())?);
            for (i, v) in values.iter().enumerate() {
                let mut out = list.reborrow().get(length(i)?);
                match v {
                    ParamValue::Literal(v) => out.init_literal().set_bits(v.to_bits()),
                    ParamValue::Parameter(v) => out.set_parameter(v.0),
                    ParamValue::Expr(v) => out.set_expression(v.0),
                }
            }
        }
    }
    Ok(())
}

fn read_instruction(
    value: wire::instruction::Reader<'_>,
    budget: &mut Budget,
) -> Result<SnapshotInstruction, Error> {
    let q = value.get_qubits()?;
    let c = value.get_clbits()?;
    budget.charge(q.len() as usize)?;
    budget.charge(c.len() as usize)?;
    let params = match value.get_parameters()?.which()? {
        wire::optional_parameters::None(()) => None,
        wire::optional_parameters::Some(list) => {
            let list = list?;
            budget.charge(list.len() as usize)?;
            Some(
                list.iter()
                    .map(|v| {
                        Ok(match v.which()? {
                            wire::parameter_value::Literal(v) => {
                                ParamValue::Literal(f64::from_bits(v?.get_bits()))
                            }
                            wire::parameter_value::Parameter(v) => {
                                ParamValue::Parameter(ParameterId(v))
                            }
                            wire::parameter_value::Expression(v) => ParamValue::Expr(AngleId(v)),
                            wire::parameter_value::Invalid(()) => {
                                return Err(Error::malformed("invalid parameter value"));
                            }
                        })
                    })
                    .collect::<Result<_, Error>>()?,
            )
        }
    };
    Ok(SnapshotInstruction {
        operation: read_operation(value.get_operation()?, budget)?,
        qubits: q.iter().collect(),
        clbits: c.iter().collect(),
        params,
    })
}

fn write_literal(value: Literal, mut out: wire::literal::Builder<'_>) {
    match value {
        Literal::Bool(v) => out.set_boolean(v),
        Literal::Int(v) => out.set_integer(v),
        Literal::Float(v) => out.init_real().set_bits(v.to_bits()),
    }
}

fn read_literal(value: wire::literal::Reader<'_>) -> Result<Literal, Error> {
    Ok(match value.which()? {
        wire::literal::Boolean(v) => Literal::Bool(v),
        wire::literal::Integer(v) => Literal::Int(v),
        wire::literal::Real(v) => Literal::Float(f64::from_bits(v?.get_bits())),
        wire::literal::Invalid(()) => return Err(Error::malformed("invalid literal")),
    })
}

fn write_expr(value: Expr, mut out: wire::expression::Builder<'_>) {
    match value {
        Expr::Literal(v) => write_literal(v, out.init_literal()),
        Expr::Var(v) => out.set_variable(v.0),
        Expr::Clbit(v) => out.set_clbit(v),
        Expr::Not(v) => out.set_not(v.0),
        Expr::Binary(op, l, r) => {
            let mut out = out.init_binary();
            out.set_op(write_binary_op(op));
            out.set_left(l.0);
            out.set_right(r.0);
        }
    }
}

fn read_expr(value: wire::expression::Reader<'_>) -> Result<Expr, Error> {
    Ok(match value.which()? {
        wire::expression::Literal(v) => Expr::Literal(read_literal(v?)?),
        wire::expression::Variable(v) => Expr::Var(VarId(v)),
        wire::expression::Clbit(v) => Expr::Clbit(v),
        wire::expression::Not(v) => Expr::Not(ExprId(v)),
        wire::expression::Binary(v) => {
            let v = v?;
            Expr::Binary(
                read_binary_op(v.get_op()?)?,
                ExprId(v.get_left()),
                ExprId(v.get_right()),
            )
        }
        wire::expression::Invalid(()) => return Err(Error::malformed("invalid expression")),
    })
}

fn write_angle(value: AngleExpr, mut out: wire::circuit_angle::Builder<'_>) {
    match value {
        AngleExpr::Literal(v) => out.init_literal().set_bits(v.to_bits()),
        AngleExpr::Parameter(v) => out.set_parameter(v.0),
        AngleExpr::Neg(v) => out.set_neg(v.0),
        AngleExpr::Binary(op, l, r) => {
            let mut out = out.init_binary();
            out.set_op(write_angle_op(op));
            out.set_left(l.0);
            out.set_right(r.0);
        }
    }
}

fn read_angle(value: wire::circuit_angle::Reader<'_>) -> Result<AngleExpr, Error> {
    Ok(match value.which()? {
        wire::circuit_angle::Literal(v) => AngleExpr::Literal(f64::from_bits(v?.get_bits())),
        wire::circuit_angle::Parameter(v) => AngleExpr::Parameter(ParameterId(v)),
        wire::circuit_angle::Neg(v) => AngleExpr::Neg(AngleId(v)),
        wire::circuit_angle::Binary(v) => {
            let v = v?;
            AngleExpr::Binary(
                read_angle_op(v.get_op()?)?,
                AngleId(v.get_left()),
                AngleId(v.get_right()),
            )
        }
        wire::circuit_angle::Invalid(()) => return Err(Error::malformed("invalid angle")),
    })
}

fn write_option(value: Option<u32>, mut out: wire::optional_id32::Builder<'_>) {
    match value {
        None => out.set_none(()),
        Some(v) => out.set_some(v),
    }
}

fn read_option(value: wire::optional_id32::Reader<'_>) -> Result<Option<u32>, Error> {
    Ok(match value.which()? {
        wire::optional_id32::None(()) => None,
        wire::optional_id32::Some(v) => Some(v),
    })
}

fn write_operation(
    value: &PackedOperation,
    mut out: wire::operation::Builder<'_>,
) -> Result<(), Error> {
    match value {
        PackedOperation::Standard(v) => out.set_standard(write_gate(*v)),
        PackedOperation::Custom(v) => {
            let mut out = out.init_custom();
            out.set_name(&v.name);
            out.set_qubit_arity(v.qubit_arity);
            out.set_parameter_arity(v.param_arity);
        }
        PackedOperation::Unitary(v) => {
            let mut out = out.init_unitary();
            out.set_qubit_arity(v.qubit_arity);
            let mut list = out.init_matrix(length(v.matrix.len())?);
            for (i, (re, im)) in v.matrix.iter().enumerate() {
                let mut out = list.reborrow().get(length(i)?);
                out.set_real_bits(re.to_bits());
                out.set_imaginary_bits(im.to_bits());
            }
        }
        PackedOperation::Barrier { qubit_arity } => out.set_barrier(*qubit_arity),
        PackedOperation::Reset => out.set_reset(()),
        PackedOperation::Delay { duration, unit } => {
            let mut out = out.init_delay();
            out.reborrow().init_duration().set_bits(duration.to_bits());
            out.set_unit(write_time(*unit));
        }
        PackedOperation::ControlFlow(v) => write_control(v, out.init_control())?,
    }
    Ok(())
}

fn read_operation(
    value: wire::operation::Reader<'_>,
    budget: &mut Budget,
) -> Result<PackedOperation, Error> {
    Ok(match value.which()? {
        wire::operation::Standard(v) => PackedOperation::Standard(read_gate(v?)?),
        wire::operation::Custom(v) => {
            let v = v?;
            PackedOperation::Custom(Box::new(CustomOperation {
                name: string(v.get_name()?)?,
                qubit_arity: v.get_qubit_arity(),
                param_arity: v.get_parameter_arity(),
            }))
        }
        wire::operation::Unitary(v) => {
            let v = v?;
            let matrix = v.get_matrix()?;
            budget.charge(matrix.len() as usize)?;
            PackedOperation::Unitary(Box::new(UnitaryOperation {
                qubit_arity: v.get_qubit_arity(),
                matrix: matrix
                    .iter()
                    .map(|v| {
                        (
                            f64::from_bits(v.get_real_bits()),
                            f64::from_bits(v.get_imaginary_bits()),
                        )
                    })
                    .collect(),
            }))
        }
        wire::operation::Barrier(v) => PackedOperation::Barrier { qubit_arity: v },
        wire::operation::Reset(()) => PackedOperation::Reset,
        wire::operation::Delay(v) => {
            let v = v?;
            PackedOperation::Delay {
                duration: f64::from_bits(v.get_duration()?.get_bits()),
                unit: read_time(v.get_unit()?)?,
            }
        }
        wire::operation::Control(v) => {
            PackedOperation::ControlFlow(Box::new(read_control(v?, budget)?))
        }
        wire::operation::Invalid(()) => return Err(Error::malformed("invalid operation")),
    })
}

fn write_control(
    value: &ControlFlow,
    mut out: wire::control_flow::Builder<'_>,
) -> Result<(), Error> {
    match value {
        ControlFlow::IfElse(v) => {
            let mut out = out.init_if_else();
            out.set_condition(v.condition.0);
            out.set_then_block(v.then_block.0);
            write_option(v.else_block.map(|v| v.0), out.init_else_block());
        }
        ControlFlow::While(v) => {
            let mut out = out.init_while();
            out.set_condition(v.condition.0);
            out.set_body(v.body.0);
        }
        ControlFlow::For(v) => {
            let mut out = out.init_for();
            out.set_body(v.body.0);
            let mut index = out.reborrow().init_index();
            match v.index {
                None => index.set_none(()),
                Some(LoopIndex::Parameter(v)) => index.set_parameter(v.0),
                Some(LoopIndex::Var(v)) => index.set_variable(v.0),
            }
            let mut values = out.init_values(length(v.values.len())?);
            for (i, v) in v.values.iter().enumerate() {
                write_literal(*v, values.reborrow().get(length(i)?));
            }
        }
        ControlFlow::Switch(v) => {
            let mut out = out.init_switch();
            out.set_selector(v.selector.0);
            write_option(v.default.map(|v| v.0), out.reborrow().init_default());
            let mut cases = out.init_cases(length(v.cases.len())?);
            for (i, (values, body)) in v.cases.iter().enumerate() {
                let mut case = cases.reborrow().get(length(i)?);
                case.set_body(body.0);
                let mut list = case.init_values(length(values.len())?);
                for (j, v) in values.iter().enumerate() {
                    write_literal(*v, list.reborrow().get(length(j)?));
                }
            }
        }
        ControlFlow::Box(v) => {
            let mut out = out.init_box();
            out.set_body(v.body.0);
            let mut duration = out.reborrow().init_duration();
            match v.duration {
                None => duration.set_none(()),
                Some(v) => duration.init_some().set_bits(v.to_bits()),
            }
            let mut label = out.init_label();
            match &v.label {
                None => label.set_none(()),
                Some(v) => label.set_some(v),
            }
        }
        ControlFlow::Moment(v) => out.set_moment(v.0),
        ControlFlow::Break => out.set_break(()),
        ControlFlow::Continue => out.set_continue(()),
    }
    Ok(())
}

fn read_control(
    value: wire::control_flow::Reader<'_>,
    budget: &mut Budget,
) -> Result<ControlFlow, Error> {
    Ok(match value.which()? {
        wire::control_flow::IfElse(v) => {
            let v = v?;
            ControlFlow::IfElse(IfElse {
                condition: ExprId(v.get_condition()),
                then_block: BlockId(v.get_then_block()),
                else_block: read_option(v.get_else_block()?)?.map(BlockId),
            })
        }
        wire::control_flow::While(v) => {
            let v = v?;
            ControlFlow::While(WhileLoop {
                condition: ExprId(v.get_condition()),
                body: BlockId(v.get_body()),
            })
        }
        wire::control_flow::For(v) => {
            let v = v?;
            let index = match v.get_index()?.which()? {
                wire::loop_index::None(()) => None,
                wire::loop_index::Parameter(v) => Some(LoopIndex::Parameter(ParameterId(v))),
                wire::loop_index::Variable(v) => Some(LoopIndex::Var(VarId(v))),
            };
            let values = v.get_values()?;
            budget.charge(values.len() as usize)?;
            ControlFlow::For(ForLoop {
                index,
                values: values.iter().map(read_literal).collect::<Result<_, _>>()?,
                body: BlockId(v.get_body()),
            })
        }
        wire::control_flow::Switch(v) => {
            let v = v?;
            let cases = v.get_cases()?;
            budget.charge(cases.len() as usize)?;
            let cases = cases
                .iter()
                .map(|v| {
                    let values = v.get_values()?;
                    budget.charge(values.len() as usize)?;
                    Ok((
                        values.iter().map(read_literal).collect::<Result<_, _>>()?,
                        BlockId(v.get_body()),
                    ))
                })
                .collect::<Result<_, Error>>()?;
            ControlFlow::Switch(Switch {
                selector: ExprId(v.get_selector()),
                cases,
                default: read_option(v.get_default()?)?.map(BlockId),
            })
        }
        wire::control_flow::Box(v) => {
            let v = v?;
            let duration = match v.get_duration()?.which()? {
                wire::optional_real::None(()) => None,
                wire::optional_real::Some(v) => Some(f64::from_bits(v?.get_bits())),
            };
            let label = match v.get_label()?.which()? {
                wire::optional_text::None(()) => None,
                wire::optional_text::Some(v) => Some(string(v?)?),
            };
            ControlFlow::Box(BoxBody {
                body: BlockId(v.get_body()),
                duration,
                label,
            })
        }
        wire::control_flow::Moment(v) => ControlFlow::Moment(BlockId(v)),
        wire::control_flow::Break(()) => ControlFlow::Break,
        wire::control_flow::Continue(()) => ControlFlow::Continue,
        wire::control_flow::Invalid(()) => return Err(Error::malformed("invalid control flow")),
    })
}
