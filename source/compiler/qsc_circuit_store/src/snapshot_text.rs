// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_snapshot::{
    Budget, Error,
    text::{self, Record, Value, Writer},
};

use crate::{
    AngleExpr, AngleId, AngleOp, BinaryOp, BlockId, BoxBody, ControlFlow, CustomOperation, Expr,
    ExprId, ForLoop, IfElse, Literal, LoopIndex, PackedOperation, ParamValue, Parameter,
    ParameterId, Snapshot, SnapshotInstruction, StandardGate, Switch, TimeUnit, UnitaryOperation,
    Var, VarId, VarType, WhileLoop,
};

fn atom(w: &mut Writer, v: impl std::fmt::Display) -> Result<(), Error> {
    w.atom(v);
    Ok(())
}
fn list<T>(
    w: &mut Writer,
    values: &[T],
    mut write: impl FnMut(&T, &mut Writer) -> Result<(), Error>,
) -> Result<(), Error> {
    w.open("list")?;
    for value in values {
        write(value, w)?;
    }
    w.close()
}
fn optional<T>(
    w: &mut Writer,
    v: Option<T>,
    write: impl FnOnce(T, &mut Writer) -> Result<(), Error>,
) -> Result<(), Error> {
    match v {
        None => w.field("none", |_| Ok(())),
        Some(v) => w.field("some", |w| write(v, w)),
    }
}
fn read_optional(v: &Value) -> Result<Option<&Value>, Error> {
    let (tag, values) = text::variant(v)?;
    match (tag, values.as_slice()) {
        ("none", []) => Ok(None),
        ("some", [v]) => Ok(Some(*v)),
        _ => Err(Error::malformed("invalid optional value")),
    }
}
fn read_list<T>(
    v: &Value,
    b: &mut Budget,
    mut read: impl FnMut(&Value, &mut Budget) -> Result<T, Error>,
) -> Result<Vec<T>, Error> {
    text::sequence(v, b)?
        .into_iter()
        .map(|v| read(v, b))
        .collect()
}
fn ids(v: &[u32], w: &mut Writer) -> Result<(), Error> {
    list(w, v, |v, w| atom(w, v))
}
fn read_ids(v: &Value, b: &mut Budget) -> Result<Vec<u32>, Error> {
    read_list(v, b, |v, _| text::u32(v))
}

pub fn write(value: &Snapshot, w: &mut Writer) -> Result<(), Error> {
    w.open("store")?;
    w.field("version", |w| atom(w, value.schema_version))?;
    w.field("qubitCount", |w| atom(w, value.qubit_count))?;
    w.field("clbitCount", |w| atom(w, value.clbit_count))?;
    w.field("globalPhase", |w| w.real(value.global_phase.to_bits()))?;
    w.field("parameters", |w| {
        list(w, &value.parameters, |v, w| w.string(&v.name))
    })?;
    w.field("variables", |w| {
        list(w, &value.vars, |v, w| {
            w.open("variable")?;
            w.field("name", |w| w.string(&v.name))?;
            w.field("type", |w| {
                atom(
                    w,
                    match v.ty {
                        VarType::Bool => "boolean",
                        VarType::Int => "integer",
                        VarType::Float => "real",
                    },
                )
            })?;
            w.close()
        })
    })?;
    w.field("expressions", |w| list(w, &value.exprs, write_expr))?;
    w.field("angles", |w| list(w, &value.angles, write_angle))?;
    w.field("blocks", |w| {
        list(w, &value.blocks, |v, w| {
            w.open("block")?;
            w.field("instructions", |w| list(w, v, write_instruction))?;
            w.close()
        })
    })?;
    w.field("instructions", |w| {
        list(w, &value.instructions, write_instruction)
    })?;
    w.close()
}

pub fn read(mut v: Record<'_>, b: &mut Budget) -> Result<Snapshot, Error> {
    let schema_version = text::u32(v.field("version")?)?;
    let qubit_count = text::u32(v.field("qubitCount")?)?;
    let clbit_count = text::u32(v.field("clbitCount")?)?;
    let global_phase = f64::from_bits(text::real(v.field("globalPhase")?)?);
    let parameters = read_list(v.field("parameters")?, b, |v, _| {
        Ok(Parameter {
            name: text::string(v)?,
        })
    })?;
    let vars = read_list(v.field("variables")?, b, |v, _| {
        let mut v = Record::new(v, "variable")?;
        let name = text::string(v.field("name")?)?;
        let ty = match text::symbol(v.field("type")?)? {
            "boolean" => VarType::Bool,
            "integer" => VarType::Int,
            "real" => VarType::Float,
            _ => return Err(Error::malformed("unknown variable type")),
        };
        v.finish()?;
        Ok(Var { name, ty })
    })?;
    let exprs = read_list(v.field("expressions")?, b, |v, _| read_expr(v))?;
    let angles = read_list(v.field("angles")?, b, |v, _| read_angle(v))?;
    let blocks = read_list(v.field("blocks")?, b, |v, b| {
        let mut v = Record::new(v, "block")?;
        let instructions = read_list(v.field("instructions")?, b, read_instruction)?;
        v.finish()?;
        Ok(instructions)
    })?;
    let instructions = read_list(v.field("instructions")?, b, read_instruction)?;
    v.finish()?;
    Ok(Snapshot {
        schema_version,
        qubit_count,
        clbit_count,
        global_phase,
        parameters,
        vars,
        exprs,
        angles,
        blocks,
        instructions,
    })
}

fn write_instruction(v: &SnapshotInstruction, w: &mut Writer) -> Result<(), Error> {
    w.open("instruction")?;
    w.field("operation", |w| write_operation(&v.operation, w))?;
    w.field("qubits", |w| ids(&v.qubits, w))?;
    w.field("clbits", |w| ids(&v.clbits, w))?;
    w.field("parameters", |w| {
        optional(w, v.params.as_ref(), |v, w| {
            list(w, v, |v, w| match v {
                ParamValue::Literal(v) => w.field("literal", |w| w.real(v.to_bits())),
                ParamValue::Parameter(v) => w.field("parameter", |w| atom(w, v.0)),
                ParamValue::Expr(v) => w.field("expression", |w| atom(w, v.0)),
            })
        })
    })?;
    w.close()
}

fn read_instruction(v: &Value, b: &mut Budget) -> Result<SnapshotInstruction, Error> {
    let mut v = Record::new(v, "instruction")?;
    let operation = read_operation(v.field("operation")?, b)?;
    let qubits = read_ids(v.field("qubits")?, b)?;
    let clbits = read_ids(v.field("clbits")?, b)?;
    let params = read_optional(v.field("parameters")?)?
        .map(|v| {
            read_list(v, b, |v, _| {
                let (tag, args) = text::variant(v)?;
                let [v] = args.as_slice() else {
                    return Err(Error::malformed("invalid parameter value"));
                };
                match tag {
                    "literal" => Ok(ParamValue::Literal(f64::from_bits(text::real(v)?))),
                    "parameter" => Ok(ParamValue::Parameter(ParameterId(text::u32(v)?))),
                    "expression" => Ok(ParamValue::Expr(AngleId(text::u32(v)?))),
                    _ => Err(Error::malformed("unknown parameter value")),
                }
            })
        })
        .transpose()?;
    v.finish()?;
    Ok(SnapshotInstruction {
        operation,
        qubits,
        clbits,
        params,
    })
}

fn write_literal(v: &Literal, w: &mut Writer) -> Result<(), Error> {
    match v {
        Literal::Bool(v) => w.field("boolean", |w| atom(w, if *v { "true" } else { "false" })),
        Literal::Int(v) => w.field("integer", |w| atom(w, v)),
        Literal::Float(v) => w.field("real", |w| w.real(v.to_bits())),
    }
}
fn read_literal(v: &Value) -> Result<Literal, Error> {
    let (tag, args) = text::variant(v)?;
    let [v] = args.as_slice() else {
        return Err(Error::malformed("invalid literal"));
    };
    match tag {
        "boolean" => Ok(Literal::Bool(text::boolean(v)?)),
        "integer" => Ok(Literal::Int(text::i64(v)?)),
        "real" => Ok(Literal::Float(f64::from_bits(text::real(v)?))),
        _ => Err(Error::malformed("unknown literal")),
    }
}
fn binary_name(v: BinaryOp) -> &'static str {
    match v {
        BinaryOp::Eq => "eq",
        BinaryOp::Ne => "ne",
        BinaryOp::Lt => "lt",
        BinaryOp::Le => "le",
        BinaryOp::Gt => "gt",
        BinaryOp::Ge => "ge",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::Add => "add",
        BinaryOp::Sub => "sub",
        BinaryOp::Mul => "mul",
    }
}
fn angle_name(v: AngleOp) -> &'static str {
    match v {
        AngleOp::Add => "add",
        AngleOp::Sub => "sub",
        AngleOp::Mul => "mul",
        AngleOp::Div => "div",
    }
}
fn binary_fields(w: &mut Writer, tag: &str, op: &str, l: u32, r: u32) -> Result<(), Error> {
    w.open(tag)?;
    w.field("op", |w| atom(w, op))?;
    w.field("left", |w| atom(w, l))?;
    w.field("right", |w| atom(w, r))?;
    w.close()
}
fn read_binary(v: &Value, tag: &str) -> Result<(String, u32, u32), Error> {
    let mut v = Record::new(v, tag)?;
    let op = text::symbol(v.field("op")?)?.to_owned();
    let l = text::u32(v.field("left")?)?;
    let r = text::u32(v.field("right")?)?;
    v.finish()?;
    Ok((op, l, r))
}
fn write_expr(v: &Expr, w: &mut Writer) -> Result<(), Error> {
    match v {
        Expr::Literal(v) => w.field("literal", |w| write_literal(v, w)),
        Expr::Var(v) => w.field("variable", |w| atom(w, v.0)),
        Expr::Clbit(v) => w.field("clbit", |w| atom(w, v)),
        Expr::Not(v) => w.field("not", |w| atom(w, v.0)),
        Expr::Binary(op, l, r) => w.field("binary", |w| {
            binary_fields(w, "binaryExpression", binary_name(*op), l.0, r.0)
        }),
    }
}
fn read_expr(v: &Value) -> Result<Expr, Error> {
    let (tag, args) = text::variant(v)?;
    let [v] = args.as_slice() else {
        return Err(Error::malformed("invalid expression"));
    };
    Ok(match tag {
        "literal" => Expr::Literal(read_literal(v)?),
        "variable" => Expr::Var(VarId(text::u32(v)?)),
        "clbit" => Expr::Clbit(text::u32(v)?),
        "not" => Expr::Not(ExprId(text::u32(v)?)),
        "binary" => {
            let (op, l, r) = read_binary(v, "binaryExpression")?;
            let op = match op.as_str() {
                "eq" => BinaryOp::Eq,
                "ne" => BinaryOp::Ne,
                "lt" => BinaryOp::Lt,
                "le" => BinaryOp::Le,
                "gt" => BinaryOp::Gt,
                "ge" => BinaryOp::Ge,
                "and" => BinaryOp::And,
                "or" => BinaryOp::Or,
                "add" => BinaryOp::Add,
                "sub" => BinaryOp::Sub,
                "mul" => BinaryOp::Mul,
                _ => return Err(Error::malformed("unknown binary operator")),
            };
            Expr::Binary(op, ExprId(l), ExprId(r))
        }
        _ => return Err(Error::malformed("unknown expression")),
    })
}
fn write_angle(v: &AngleExpr, w: &mut Writer) -> Result<(), Error> {
    match v {
        AngleExpr::Literal(v) => w.field("literal", |w| w.real(v.to_bits())),
        AngleExpr::Parameter(v) => w.field("parameter", |w| atom(w, v.0)),
        AngleExpr::Neg(v) => w.field("neg", |w| atom(w, v.0)),
        AngleExpr::Binary(op, l, r) => w.field("binary", |w| {
            binary_fields(w, "binaryCircuitAngle", angle_name(*op), l.0, r.0)
        }),
    }
}
fn read_angle(v: &Value) -> Result<AngleExpr, Error> {
    let (tag, args) = text::variant(v)?;
    let [v] = args.as_slice() else {
        return Err(Error::malformed("invalid angle"));
    };
    Ok(match tag {
        "literal" => AngleExpr::Literal(f64::from_bits(text::real(v)?)),
        "parameter" => AngleExpr::Parameter(ParameterId(text::u32(v)?)),
        "neg" => AngleExpr::Neg(AngleId(text::u32(v)?)),
        "binary" => {
            let (op, l, r) = read_binary(v, "binaryCircuitAngle")?;
            let op = match op.as_str() {
                "add" => AngleOp::Add,
                "sub" => AngleOp::Sub,
                "mul" => AngleOp::Mul,
                "div" => AngleOp::Div,
                _ => return Err(Error::malformed("unknown angle operator")),
            };
            AngleExpr::Binary(op, AngleId(l), AngleId(r))
        }
        _ => return Err(Error::malformed("unknown angle")),
    })
}

fn gate_name(v: StandardGate) -> &'static str {
    match v {
        StandardGate::MeasureX => "measureX",
        StandardGate::MeasureY => "measureY",
        StandardGate::MeasureZ => "measureZ",
        StandardGate::PrepareX => "prepareX",
        StandardGate::PrepareY => "prepareY",
        StandardGate::PrepareZ => "prepareZ",
        _ => v.name(),
    }
}
fn write_operation(v: &PackedOperation, w: &mut Writer) -> Result<(), Error> {
    match v {
        PackedOperation::Standard(v) => w.field("standard", |w| atom(w, gate_name(*v))),
        PackedOperation::Custom(v) => w.field("custom", |w| {
            w.open("customOperation")?;
            w.field("name", |w| w.string(&v.name))?;
            w.field("qubitArity", |w| atom(w, v.qubit_arity))?;
            w.field("parameterArity", |w| atom(w, v.param_arity))?;
            w.close()
        }),
        PackedOperation::Unitary(v) => w.field("unitary", |w| {
            w.open("unitaryOperation")?;
            w.field("qubitArity", |w| atom(w, v.qubit_arity))?;
            w.field("matrix", |w| {
                list(w, &v.matrix, |&(re, im), w| {
                    w.open("complex64")?;
                    w.field("realBits", |w| atom(w, re.to_bits()))?;
                    w.field("imaginaryBits", |w| atom(w, im.to_bits()))?;
                    w.close()
                })
            })?;
            w.close()
        }),
        PackedOperation::Barrier { qubit_arity } => w.field("barrier", |w| atom(w, qubit_arity)),
        PackedOperation::Reset => w.field("reset", |_| Ok(())),
        PackedOperation::Delay { duration, unit } => w.field("delay", |w| {
            w.open("delay")?;
            w.field("duration", |w| w.real(duration.to_bits()))?;
            w.field("unit", |w| atom(w, unit.name()))?;
            w.close()
        }),
        PackedOperation::ControlFlow(v) => w.field("control", |w| write_control(v, w)),
    }
}
fn read_operation(v: &Value, b: &mut Budget) -> Result<PackedOperation, Error> {
    let (tag, args) = text::variant(v)?;
    if tag == "reset" && args.is_empty() {
        return Ok(PackedOperation::Reset);
    }
    let [v] = args.as_slice() else {
        return Err(Error::malformed("invalid operation"));
    };
    Ok(match tag {
        "standard" => {
            let name = text::symbol(v)?;
            PackedOperation::Standard(
                StandardGate::all()
                    .into_iter()
                    .find(|v| gate_name(*v) == name)
                    .ok_or_else(|| Error::malformed("unknown standard gate"))?,
            )
        }
        "custom" => {
            let mut v = Record::new(v, "customOperation")?;
            let name = text::string(v.field("name")?)?;
            let qubit_arity = text::u32(v.field("qubitArity")?)?
                .try_into()
                .map_err(Error::malformed)?;
            let param_arity = text::u32(v.field("parameterArity")?)?
                .try_into()
                .map_err(Error::malformed)?;
            v.finish()?;
            PackedOperation::Custom(Box::new(CustomOperation {
                name,
                qubit_arity,
                param_arity,
            }))
        }
        "unitary" => {
            let mut v = Record::new(v, "unitaryOperation")?;
            let qubit_arity = text::u32(v.field("qubitArity")?)?
                .try_into()
                .map_err(Error::malformed)?;
            let matrix = read_list(v.field("matrix")?, b, |v, _| {
                let mut v = Record::new(v, "complex64")?;
                let re = text::u64(v.field("realBits")?)?;
                let im = text::u64(v.field("imaginaryBits")?)?;
                v.finish()?;
                Ok((f64::from_bits(re), f64::from_bits(im)))
            })?;
            v.finish()?;
            PackedOperation::Unitary(Box::new(UnitaryOperation {
                qubit_arity,
                matrix,
            }))
        }
        "barrier" => PackedOperation::Barrier {
            qubit_arity: text::u32(v)?.try_into().map_err(Error::malformed)?,
        },
        "delay" => {
            let mut v = Record::new(v, "delay")?;
            let duration = f64::from_bits(text::real(v.field("duration")?)?);
            let unit = text::symbol(v.field("unit")?)?
                .parse::<TimeUnit>()
                .map_err(|()| Error::malformed("unknown time unit"))?;
            v.finish()?;
            PackedOperation::Delay { duration, unit }
        }
        "control" => PackedOperation::ControlFlow(Box::new(read_control(v, b)?)),
        _ => return Err(Error::malformed("unknown operation")),
    })
}

fn write_control(v: &ControlFlow, w: &mut Writer) -> Result<(), Error> {
    match v {
        ControlFlow::IfElse(v) => w.field("ifElse", |w| {
            w.open("ifElse")?;
            w.field("condition", |w| atom(w, v.condition.0))?;
            w.field("thenBlock", |w| atom(w, v.then_block.0))?;
            w.field("elseBlock", |w| {
                optional(w, v.else_block, |v, w| atom(w, v.0))
            })?;
            w.close()
        }),
        ControlFlow::While(v) => w.field("while", |w| {
            w.open("while")?;
            w.field("condition", |w| atom(w, v.condition.0))?;
            w.field("body", |w| atom(w, v.body.0))?;
            w.close()
        }),
        ControlFlow::For(v) => w.field("for", |w| {
            w.open("for")?;
            w.field("index", |w| match v.index {
                None => w.field("none", |_| Ok(())),
                Some(LoopIndex::Parameter(v)) => w.field("parameter", |w| atom(w, v.0)),
                Some(LoopIndex::Var(v)) => w.field("variable", |w| atom(w, v.0)),
            })?;
            w.field("values", |w| list(w, &v.values, write_literal))?;
            w.field("body", |w| atom(w, v.body.0))?;
            w.close()
        }),
        ControlFlow::Switch(v) => w.field("switch", |w| {
            w.open("switch")?;
            w.field("selector", |w| atom(w, v.selector.0))?;
            w.field("cases", |w| {
                list(w, &v.cases, |(values, body), w| {
                    w.open("case")?;
                    w.field("values", |w| list(w, values, write_literal))?;
                    w.field("body", |w| atom(w, body.0))?;
                    w.close()
                })
            })?;
            w.field("default", |w| optional(w, v.default, |v, w| atom(w, v.0)))?;
            w.close()
        }),
        ControlFlow::Box(v) => w.field("box", |w| {
            w.open("box")?;
            w.field("body", |w| atom(w, v.body.0))?;
            w.field("duration", |w| {
                optional(w, v.duration, |v, w| w.real(v.to_bits()))
            })?;
            w.field("label", |w| {
                optional(w, v.label.as_deref(), |v, w| w.string(v))
            })?;
            w.close()
        }),
        ControlFlow::Moment(v) => w.field("moment", |w| atom(w, v.0)),
        ControlFlow::Break => w.field("break", |_| Ok(())),
        ControlFlow::Continue => w.field("continue", |_| Ok(())),
    }
}
fn read_control(v: &Value, b: &mut Budget) -> Result<ControlFlow, Error> {
    let (tag, args) = text::variant(v)?;
    match (tag, args.as_slice()) {
        ("break", []) => return Ok(ControlFlow::Break),
        ("continue", []) => return Ok(ControlFlow::Continue),
        _ => {}
    }
    let [v] = args.as_slice() else {
        return Err(Error::malformed("invalid control flow"));
    };
    if tag == "moment" {
        return Ok(ControlFlow::Moment(BlockId(text::u32(v)?)));
    }
    let mut v = Record::new(v, tag)?;
    let result = match tag {
        "ifElse" => ControlFlow::IfElse(IfElse {
            condition: ExprId(text::u32(v.field("condition")?)?),
            then_block: BlockId(text::u32(v.field("thenBlock")?)?),
            else_block: read_optional(v.field("elseBlock")?)?
                .map(|v| text::u32(v).map(BlockId))
                .transpose()?,
        }),
        "while" => ControlFlow::While(WhileLoop {
            condition: ExprId(text::u32(v.field("condition")?)?),
            body: BlockId(text::u32(v.field("body")?)?),
        }),
        "for" => {
            let (tag, args) = text::variant(v.field("index")?)?;
            let index = match (tag, args.as_slice()) {
                ("none", []) => None,
                ("parameter", [v]) => Some(LoopIndex::Parameter(ParameterId(text::u32(v)?))),
                ("variable", [v]) => Some(LoopIndex::Var(VarId(text::u32(v)?))),
                _ => return Err(Error::malformed("invalid loop index")),
            };
            ControlFlow::For(ForLoop {
                index,
                values: read_list(v.field("values")?, b, |v, _| read_literal(v))?,
                body: BlockId(text::u32(v.field("body")?)?),
            })
        }
        "switch" => {
            let selector = ExprId(text::u32(v.field("selector")?)?);
            let cases = read_list(v.field("cases")?, b, |v, b| {
                let mut v = Record::new(v, "case")?;
                let values = read_list(v.field("values")?, b, |v, _| read_literal(v))?;
                let body = BlockId(text::u32(v.field("body")?)?);
                v.finish()?;
                Ok((values, body))
            })?;
            let default = read_optional(v.field("default")?)?
                .map(|v| text::u32(v).map(BlockId))
                .transpose()?;
            ControlFlow::Switch(Switch {
                selector,
                cases,
                default,
            })
        }
        "box" => ControlFlow::Box(BoxBody {
            body: BlockId(text::u32(v.field("body")?)?),
            duration: read_optional(v.field("duration")?)?
                .map(|v| text::real(v).map(f64::from_bits))
                .transpose()?,
            label: read_optional(v.field("label")?)?
                .map(text::string)
                .transpose()?,
        }),
        _ => return Err(Error::malformed("unknown control flow")),
    };
    v.finish()?;
    Ok(result)
}
