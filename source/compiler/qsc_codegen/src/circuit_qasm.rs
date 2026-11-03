// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Converting a circuit to and from OpenQASM 3.
//!
//! Import reuses the existing OpenQASM front end rather than adding a second
//! parser. The front end hands back a semantic tree with symbols resolved, so
//! what is left is mapping registers onto the circuit's dense qubit and
//! classical bit indices.
//!
//! Import evaluates literal conversions through the front end's typed constant
//! evaluator, including modulo-one-turn angle conversion. Register comparisons
//! preserve every declared bit and offset. Runtime conversions and register
//! uses that the circuit cannot represent return named unsupported-import
//! diagnostics instead of discarding casts or truncating registers.
//!
//! The set of gates OpenQASM can express is not the same as the set QIR can.
//! Controlled rotations, the adjoint of the square root of X, and the
//! controlled swap all have OpenQASM spellings while having no QIR intrinsic,
//! so a circuit can export here and still need decomposition before it
//! compiles.

#[cfg(test)]
mod tests;

use std::fmt::Write;

use qdk_openqasm::io::InMemorySourceResolver;
use qdk_openqasm::semantic::ast as semast;
use qdk_openqasm::semantic::symbols::{SymbolId, SymbolTable};
use qdk_openqasm::semantic::types::Type;
use qsc_circuit_store::{
    AngleExpr, AngleId, AngleOp, BinaryOp, BlockId, BoxBody, CircuitStore, Clbit, ControlFlow,
    Expr, ExprId, ForLoop, IfElse, Literal, LoopIndex, PackedOperation, ParamValue, Qubit,
    StandardGate, Switch, TimeUnit, VarId, VarType, WhileLoop,
};
use rustc_hash::FxHashMap;

/// The qubit register name used by export.
const QUBIT_REGISTER: &str = "q";
/// The classical register name used by export.
const CLBIT_REGISTER: &str = "c";

/// Why a circuit could not be exported to or imported from OpenQASM.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("'{construct}' has no OpenQASM equivalent")]
    Unsupported { construct: &'static str },
    #[error("OpenQASM source could not be parsed: {0}")]
    Parse(String),
    #[error("'{construct}' is not imported into a circuit")]
    UnsupportedImport { construct: String },
    #[error("{what} is not a compile-time constant")]
    NotConstant { what: &'static str },
    #[error("register '{name}' has no known size")]
    UnsizedRegister { name: String },
    #[error("'{name}' cannot be written as an OpenQASM name")]
    UnwritableName { name: String },
}

/// Checks that a name can be written into OpenQASM as itself.
///
/// Names reach a circuit from its builder and from saved circuits, so they are
/// arbitrary text. Writing one straight into generated source would let it carry
/// punctuation and turn into other statements, so anything that is not a plain
/// identifier is refused rather than escaped, since OpenQASM has no escape form.
fn writable_name(name: &str) -> Result<&str, Error> {
    let mut characters = name.chars();
    let starts_well = characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_');
    let rest_is_well = characters.all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !starts_well || !rest_is_well {
        return Err(Error::UnwritableName {
            name: name.to_string(),
        });
    }
    // The registers this export declares would otherwise be shadowed.
    if name == QUBIT_REGISTER || name == CLBIT_REGISTER {
        return Err(Error::UnwritableName {
            name: name.to_string(),
        });
    }
    Ok(name)
}

/// How a gate is written in OpenQASM.
enum Spelling {
    /// A gate call under this name.
    Gate(&'static str),
    /// Has its own statement form rather than being a gate call.
    Statement,
    /// Has no OpenQASM equivalent.
    None,
}

/// The OpenQASM spelling of a gate.
const fn qasm_spelling(gate: StandardGate) -> Spelling {
    match gate {
        StandardGate::I => Spelling::Gate("id"),
        StandardGate::X => Spelling::Gate("x"),
        StandardGate::Y => Spelling::Gate("y"),
        StandardGate::Z => Spelling::Gate("z"),
        StandardGate::H => Spelling::Gate("h"),
        StandardGate::S => Spelling::Gate("s"),
        StandardGate::SDag => Spelling::Gate("sdg"),
        StandardGate::T => Spelling::Gate("t"),
        StandardGate::TDag => Spelling::Gate("tdg"),
        StandardGate::SqrtX => Spelling::Gate("sx"),
        // Spelled by the Qiskit extension set the front end also recognises.
        StandardGate::SqrtXDag => Spelling::Gate("sxdg"),
        StandardGate::Cx => Spelling::Gate("cx"),
        StandardGate::Cy => Spelling::Gate("cy"),
        StandardGate::Cz => Spelling::Gate("cz"),
        StandardGate::Swap => Spelling::Gate("swap"),
        StandardGate::Ccx => Spelling::Gate("ccx"),
        StandardGate::Cswap => Spelling::Gate("cswap"),
        StandardGate::Rx => Spelling::Gate("rx"),
        StandardGate::Ry => Spelling::Gate("ry"),
        StandardGate::Rz => Spelling::Gate("rz"),
        StandardGate::Rxx => Spelling::Gate("rxx"),
        StandardGate::Ryy => Spelling::Gate("ryy"),
        StandardGate::Rzz => Spelling::Gate("rzz"),
        StandardGate::Crx => Spelling::Gate("crx"),
        StandardGate::Cry => Spelling::Gate("cry"),
        StandardGate::Crz => Spelling::Gate("crz"),
        StandardGate::MeasureZ | StandardGate::PrepareZ => Spelling::Statement,
        // OpenQASM measures and prepares in the Z basis only.
        StandardGate::MeasureX
        | StandardGate::MeasureY
        | StandardGate::PrepareX
        | StandardGate::PrepareY => Spelling::None,
    }
}

/// The gate a name refers to, for import.
fn gate_from_qasm_name(name: &str) -> Option<StandardGate> {
    StandardGate::all()
        .into_iter()
        .find(|gate| matches!(qasm_spelling(*gate), Spelling::Gate(spelling) if spelling == name))
}

/// Writes a circuit as OpenQASM 3.
///
/// # Errors
///
/// Returns an error naming any construct with no OpenQASM equivalent.
pub fn circuit_to_qasm(store: &CircuitStore) -> Result<String, Error> {
    let mut out = String::new();
    // A switch arrived in OpenQASM 3.1, and the front end refuses one under a
    // 3.0 header, so the version is raised only for a program that needs it.
    let version = if store
        .instructions()
        .any(|instruction| uses_switch(store, instruction))
    {
        "3.1"
    } else {
        "3.0"
    };
    let _ = writeln!(out, "OPENQASM {version};");
    out.push_str("include \"stdgates.inc\";\n");
    // A parameter is a value supplied before the program runs, which is what an
    // input declaration is for. Without one the name would be undefined.
    for id in store.referenced_parameters() {
        let name = writable_name(&store.parameters()[id.0 as usize].name)?;
        let _ = writeln!(out, "input float {name};");
    }
    if store.qubit_count() > 0 {
        let _ = writeln!(out, "qubit[{}] {QUBIT_REGISTER};", store.qubit_count());
    }
    if store.clbit_count() > 0 {
        let _ = writeln!(out, "bit[{}] {CLBIT_REGISTER};", store.clbit_count());
    }
    for instruction in store.instructions() {
        write_instruction(&mut out, store, instruction, 0)?;
    }
    Ok(out)
}

/// Whether an instruction is a switch, or contains one at any depth.
fn uses_switch(store: &CircuitStore, instruction: &qsc_circuit_store::Instruction) -> bool {
    let PackedOperation::ControlFlow(control) = &instruction.operation else {
        return false;
    };
    if matches!(**control, ControlFlow::Switch(_)) {
        return true;
    }
    control.blocks().iter().any(|block| {
        store.block(*block).is_some_and(|body| {
            body.instructions
                .iter()
                .any(|nested| uses_switch(store, nested))
        })
    })
}

fn write_instruction(
    out: &mut String,
    store: &CircuitStore,
    instruction: &qsc_circuit_store::Instruction,
    indent: usize,
) -> Result<(), Error> {
    let pad = "  ".repeat(indent);
    let qubits: Vec<String> = store
        .qubits_of(instruction)
        .unwrap_or(&[])
        .iter()
        .map(|qubit| format!("{QUBIT_REGISTER}[{}]", qubit.0))
        .collect();
    let clbits: Vec<String> = store
        .clbits_of(instruction)
        .unwrap_or(&[])
        .iter()
        .map(|clbit| format!("{CLBIT_REGISTER}[{}]", clbit.0))
        .collect();

    match &instruction.operation {
        PackedOperation::Standard(StandardGate::MeasureZ) => {
            let target = clbits.first().ok_or(Error::Unsupported {
                construct: "measurement without a classical bit",
            })?;
            let _ = writeln!(out, "{pad}{target} = measure {};", qubits.join(", "));
        }
        PackedOperation::Standard(StandardGate::PrepareZ) | PackedOperation::Reset => {
            let _ = writeln!(out, "{pad}reset {};", qubits.join(", "));
        }
        PackedOperation::Standard(gate) => {
            let name = match qasm_spelling(*gate) {
                Spelling::Gate(name) => name,
                // OpenQASM cannot name this gate, but it can express what the
                // gate is made of, so the rewrite is written instead.
                Spelling::None => {
                    let indices: Vec<u32> = store
                        .qubits_of(instruction)
                        .unwrap_or(&[])
                        .iter()
                        .map(|qubit| qubit.0)
                        .collect();
                    let steps = crate::circuit::decompose::decompose(*gate, &indices, &[]).ok_or(
                        Error::Unsupported {
                            construct: gate.name(),
                        },
                    )?;
                    for step in steps {
                        write_step(out, &step, &clbits, indent)?;
                    }
                    return Ok(());
                }
                Spelling::Statement => {
                    return Err(Error::Unsupported {
                        construct: gate.name(),
                    });
                }
            };
            let angles = angle_text(store, instruction)?;
            let _ = writeln!(out, "{pad}{name}{angles} {};", qubits.join(", "));
        }
        PackedOperation::Barrier { .. } => {
            let _ = writeln!(out, "{pad}barrier {};", qubits.join(", "));
        }
        PackedOperation::ControlFlow(control) => {
            write_control_flow(out, store, control, indent)?;
        }
        PackedOperation::Delay { duration, unit } => {
            if !duration.is_finite() {
                return Err(Error::Unsupported {
                    construct: "delay of a duration that is not a number",
                });
            }
            let _ = writeln!(out, "{pad}delay[{duration}{unit}] {};", qubits.join(", "));
        }
        PackedOperation::Custom(_) => {
            return Err(Error::Unsupported {
                construct: "custom operation",
            });
        }
        PackedOperation::Unitary(_) => {
            return Err(Error::Unsupported {
                construct: "explicit unitary",
            });
        }
    }
    Ok(())
}

/// Writes one gate of a rewrite.
fn write_step(
    out: &mut String,
    step: &crate::circuit::decompose::Step,
    clbits: &[String],
    indent: usize,
) -> Result<(), Error> {
    let pad = "  ".repeat(indent);
    let qubits: Vec<String> = step
        .qubits
        .iter()
        .map(|qubit| format!("{QUBIT_REGISTER}[{qubit}]"))
        .collect();
    match qasm_spelling(step.gate) {
        Spelling::Gate(name) => {
            let angles = if step.angles.is_empty() {
                String::new()
            } else {
                let rendered: Vec<String> = step.angles.iter().map(|a| format!("{a}")).collect();
                format!("({})", rendered.join(", "))
            };
            let _ = writeln!(out, "{pad}{name}{angles} {};", qubits.join(", "));
        }
        Spelling::Statement if step.gate == StandardGate::MeasureZ => {
            let target = clbits.first().ok_or(Error::Unsupported {
                construct: "measurement without a classical bit",
            })?;
            let _ = writeln!(out, "{pad}{target} = measure {};", qubits.join(", "));
        }
        Spelling::Statement => {
            let _ = writeln!(out, "{pad}reset {};", qubits.join(", "));
        }
        Spelling::None => {
            return Err(Error::Unsupported {
                construct: step.gate.name(),
            });
        }
    }
    Ok(())
}

fn write_control_flow(
    out: &mut String,
    store: &CircuitStore,
    control: &ControlFlow,
    indent: usize,
) -> Result<(), Error> {
    let pad = "  ".repeat(indent);
    match control {
        ControlFlow::IfElse(if_else) => {
            let condition = condition_text(store, if_else.condition)?;
            let _ = writeln!(out, "{pad}if ({condition}) {{");
            write_block(out, store, if_else.then_block, indent + 1)?;
            if let Some(else_block) = if_else.else_block {
                let _ = writeln!(out, "{pad}}} else {{");
                write_block(out, store, else_block, indent + 1)?;
            }
            let _ = writeln!(out, "{pad}}}");
        }
        // OpenQASM has no moment. A box is the nearest thing it can say, since
        // it groups instructions as one scheduled unit, so the boundary
        // survives even though the claim of simultaneity does not.
        ControlFlow::Moment(body) => {
            let _ = writeln!(out, "{pad}box {{");
            write_block(out, store, *body, indent + 1)?;
            let _ = writeln!(out, "{pad}}}");
        }
        ControlFlow::Box(boxed) => {
            if boxed.duration.is_some() {
                return Err(Error::Unsupported {
                    construct: "box with duration",
                });
            }
            let _ = writeln!(out, "{pad}box {{");
            write_block(out, store, boxed.body, indent + 1)?;
            let _ = writeln!(out, "{pad}}}");
        }
        ControlFlow::While(loop_) => {
            let condition = condition_text(store, loop_.condition)?;
            let _ = writeln!(out, "{pad}while ({condition}) {{");
            write_block(out, store, loop_.body, indent + 1)?;
            let _ = writeln!(out, "{pad}}}");
        }
        ControlFlow::For(loop_) => {
            let name = loop_index_name(store, loop_)?;
            let ty = loop_value_type(&loop_.values);
            let values: Vec<String> = loop_.values.iter().map(literal_text).collect();
            let _ = writeln!(out, "{pad}for {ty} {name} in {{{}}} {{", values.join(", "));
            write_block(out, store, loop_.body, indent + 1)?;
            let _ = writeln!(out, "{pad}}}");
        }
        ControlFlow::Switch(switch) => {
            let target = operand_text(store, switch.selector)?;
            let _ = writeln!(out, "{pad}switch ({target}) {{");
            for (labels, block) in &switch.cases {
                let labels: Vec<String> = labels.iter().map(literal_text).collect();
                let _ = writeln!(out, "{pad}  case {} {{", labels.join(", "));
                write_block(out, store, *block, indent + 2)?;
                let _ = writeln!(out, "{pad}  }}");
            }
            if let Some(default) = switch.default {
                let _ = writeln!(out, "{pad}  default {{");
                write_block(out, store, default, indent + 2)?;
                let _ = writeln!(out, "{pad}  }}");
            }
            let _ = writeln!(out, "{pad}}}");
        }
        ControlFlow::Break => {
            let _ = writeln!(out, "{pad}break;");
        }
        ControlFlow::Continue => {
            let _ = writeln!(out, "{pad}continue;");
        }
    }
    Ok(())
}

fn write_block(
    out: &mut String,
    store: &CircuitStore,
    block: qsc_circuit_store::BlockId,
    indent: usize,
) -> Result<(), Error> {
    let body = store.block(block).ok_or(Error::Unsupported {
        construct: "body that does not exist",
    })?;
    for instruction in &body.instructions {
        write_instruction(out, store, instruction, indent)?;
    }
    Ok(())
}

/// The name a for loop's index is written under.
///
/// A loop that binds nothing still needs a name, because the syntax has no
/// form without one, so one is chosen that no declaration has taken.
fn loop_index_name(store: &CircuitStore, loop_: &ForLoop) -> Result<String, Error> {
    match loop_.index {
        Some(LoopIndex::Parameter(id)) => {
            writable_name(&store.parameters()[id.0 as usize].name).map(str::to_string)
        }
        Some(LoopIndex::Var(id)) => {
            writable_name(&store.vars()[id.0 as usize].name).map(str::to_string)
        }
        None => {
            let taken = |name: &str| {
                store.parameters().iter().any(|p| p.name == name)
                    || store.vars().iter().any(|v| v.name == name)
            };
            // One more candidate than there are declarations, so at least one
            // of them is free whatever the declarations are called.
            let limit = store.parameters().len() + store.vars().len();
            Ok((0..=limit)
                .map(|n| format!("_loop_i_{n}"))
                .find(|name| !taken(name))
                .expect("more candidates than declarations"))
        }
    }
}

/// The declared type of a for loop's index.
///
/// Widened to whatever holds every value, so a set mixing whole numbers with
/// fractions is not narrowed to integers and silently truncated.
fn loop_value_type(values: &[qsc_circuit_store::Literal]) -> &'static str {
    use qsc_circuit_store::Literal;
    if values
        .iter()
        .any(|value| matches!(value, Literal::Float(_)))
    {
        "float"
    } else if !values.is_empty() && values.iter().all(|v| matches!(v, Literal::Bool(_))) {
        "bool"
    } else {
        "int"
    }
}

fn literal_text(value: &qsc_circuit_store::Literal) -> String {
    use qsc_circuit_store::Literal;
    match value {
        Literal::Bool(value) => u8::from(*value).to_string(),
        Literal::Int(value) => value.to_string(),
        Literal::Float(value) => format!("{value}"),
    }
}

/// Renders an instruction's angles as a parenthesised argument list.
fn angle_text(
    store: &CircuitStore,
    instruction: &qsc_circuit_store::Instruction,
) -> Result<String, Error> {
    let params = instruction.params.as_deref().unwrap_or(&[]);
    if params.is_empty() {
        return Ok(String::new());
    }
    let rendered: Vec<String> = params
        .iter()
        .map(|param| match param {
            ParamValue::Literal(value) => Ok(format!("{value}")),
            ParamValue::Parameter(id) => {
                writable_name(&store.parameters()[id.0 as usize].name).map(str::to_string)
            }
            ParamValue::Expr(id) => angle_expr_text(store, *id),
        })
        .collect::<Result<_, Error>>()?;
    Ok(format!("({})", rendered.join(", ")))
}

/// Renders an angle expression.
///
/// Built in handle order rather than by descending the tree, so a restored
/// expression of any depth costs no stack. Every operation is parenthesised,
/// which makes the output independent of how the reader resolves precedence.
fn angle_expr_text(store: &CircuitStore, root: AngleId) -> Result<String, Error> {
    let arena = store.angles();
    if arena.get(root).is_none() {
        return Err(Error::Unsupported {
            construct: "angle expression that does not exist",
        });
    }
    let mut text: Vec<String> = Vec::with_capacity(arena.len());
    for node in arena.nodes() {
        let rendered = match *node {
            AngleExpr::Literal(value) => format!("{value}"),
            AngleExpr::Parameter(id) => {
                writable_name(&store.parameters()[id.0 as usize].name)?.to_string()
            }
            AngleExpr::Neg(inner) => format!("-({})", text[inner.0 as usize]),
            AngleExpr::Binary(op, left, right) => {
                let op = match op {
                    AngleOp::Add => "+",
                    AngleOp::Sub => "-",
                    AngleOp::Mul => "*",
                    AngleOp::Div => "/",
                };
                format!(
                    "({} {op} {})",
                    text[left.0 as usize], text[right.0 as usize]
                )
            }
        };
        text.push(rendered);
    }
    Ok(text
        .into_iter()
        .nth(root.0 as usize)
        .expect("root was checked to exist"))
}

/// Renders a condition, which the builder always shapes as a bit comparison.
fn condition_text(
    store: &CircuitStore,
    condition: qsc_circuit_store::ExprId,
) -> Result<String, Error> {
    let node = store.exprs().get(condition).ok_or(Error::Unsupported {
        construct: "condition that does not exist",
    })?;
    match node {
        Expr::Clbit(clbit) => Ok(format!("{CLBIT_REGISTER}[{clbit}] == 1")),
        Expr::Binary(op, left, right) => {
            let left = operand_text(store, *left)?;
            let right = operand_text(store, *right)?;
            let op = match op {
                qsc_circuit_store::BinaryOp::Eq => "==",
                qsc_circuit_store::BinaryOp::Ne => "!=",
                qsc_circuit_store::BinaryOp::Lt => "<",
                qsc_circuit_store::BinaryOp::Le => "<=",
                qsc_circuit_store::BinaryOp::Gt => ">",
                qsc_circuit_store::BinaryOp::Ge => ">=",
                _ => {
                    return Err(Error::Unsupported {
                        construct: "condition operator",
                    });
                }
            };
            Ok(format!("{left} {op} {right}"))
        }
        _ => Err(Error::Unsupported {
            construct: "condition shape",
        }),
    }
}

fn operand_text(store: &CircuitStore, expr: qsc_circuit_store::ExprId) -> Result<String, Error> {
    let node = store.exprs().get(expr).ok_or(Error::Unsupported {
        construct: "condition that does not exist",
    })?;
    match node {
        Expr::Clbit(clbit) => Ok(format!("{CLBIT_REGISTER}[{clbit}]")),
        Expr::Literal(qsc_circuit_store::Literal::Int(value)) => Ok(format!("{value}")),
        Expr::Literal(qsc_circuit_store::Literal::Bool(value)) => Ok(if *value {
            "1".to_string()
        } else {
            "0".to_string()
        }),
        Expr::Var(id) => writable_name(&store.vars()[id.0 as usize].name).map(str::to_string),
        _ => Err(Error::Unsupported {
            construct: "condition operand",
        }),
    }
}

/// Builds a circuit from OpenQASM 3 source.
///
/// Register comparisons preserve their complete bit patterns and offsets.
/// Literal casts use the front end's typed constant evaluator, including its
/// fixed-point, modulo-one-turn angle representation. Other runtime conversions
/// are refused rather than erased. Numeric register casts must preserve the
/// source bits within the destination width; widening retains the source sign.
///
/// # Errors
///
/// Returns an error when the source does not parse, or when it uses a construct
/// that has no place in a circuit.
pub fn circuit_from_qasm(source: &str) -> Result<CircuitStore, Error> {
    let mut result = qdk_openqasm::analyze_and_resolve(
        source,
        "circuit.qasm",
        None::<&mut InMemorySourceResolver>,
    );
    if !result.has_errors() {
        result.evaluate_literal_expressions();
    }
    if result.has_errors() {
        let messages: Vec<String> = result
            .all_errors()
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        return Err(Error::Parse(messages.join("; ")));
    }

    let mut layout = Layout::new(&result.symbols);
    layout.collect(&result.program.statements)?;
    let mut store = CircuitStore::new(layout.qubits, layout.clbits);
    layout.build(&result.program.statements, &mut store)?;
    Ok(store)
}

/// Maps OpenQASM registers onto the circuit's dense indices.
///
/// The front end resolves names to symbols but keeps registers separate, so the
/// flat numbering a circuit needs is assigned here, in declaration order.
struct Layout<'a> {
    symbols: &'a SymbolTable,
    qubit_base: FxHashMap<u32, u32>,
    clbit_base: FxHashMap<u32, u32>,
    /// Loop indices, which are declared while their loop is being read.
    vars: FxHashMap<u32, VarId>,
    qubits: u32,
    clbits: u32,
}

impl<'a> Layout<'a> {
    fn new(symbols: &'a SymbolTable) -> Self {
        Self {
            symbols,
            qubit_base: FxHashMap::default(),
            clbit_base: FxHashMap::default(),
            vars: FxHashMap::default(),
            qubits: 0,
            clbits: 0,
        }
    }

    /// Assigns every declared register a base index.
    fn collect(&mut self, statements: &[semast::Stmt]) -> Result<(), Error> {
        for statement in statements {
            match &*statement.kind {
                semast::StmtKind::QubitDecl(decl) => {
                    self.qubit_base.insert(decl.symbol_id.0, self.qubits);
                    self.qubits += 1;
                }
                semast::StmtKind::QubitArrayDecl(decl) => {
                    let size = const_index(&decl.size).ok_or(Error::NotConstant {
                        what: "register size",
                    })?;
                    self.qubit_base.insert(decl.symbol_id.0, self.qubits);
                    self.qubits += size;
                }
                semast::StmtKind::ClassicalDecl(decl) => {
                    let symbol = &self.symbols[decl.symbol_id];
                    match symbol.ty {
                        Type::Bit(_) => {
                            self.clbit_base.insert(decl.symbol_id.0, self.clbits);
                            self.clbits += 1;
                        }
                        Type::BitArray(size, _) => {
                            self.clbit_base.insert(decl.symbol_id.0, self.clbits);
                            self.clbits += size;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn build(
        &mut self,
        statements: &[semast::Stmt],
        store: &mut CircuitStore,
    ) -> Result<(), Error> {
        for statement in statements {
            self.build_statement(statement, None, store)?;
        }
        Ok(())
    }

    /// Appends an operation to the block being read, or to the circuit itself.
    fn emit(
        target: Option<BlockId>,
        store: &mut CircuitStore,
        operation: PackedOperation,
        qubits: &[Qubit],
        clbits: &[Clbit],
        params: Option<Box<[ParamValue]>>,
    ) -> Result<(), Error> {
        match target {
            Some(block) => store.push_into_block(block, operation, qubits, clbits, params),
            None => store.push(operation, qubits, clbits, params),
        }
        .map_err(|error| Error::UnsupportedImport {
            construct: error.to_string(),
        })
    }

    /// Appends a control-flow construct to the block being read.
    fn emit_control(
        target: Option<BlockId>,
        store: &mut CircuitStore,
        control: ControlFlow,
    ) -> Result<(), Error> {
        match target {
            Some(block) => store.push_control_flow_into_block(block, control),
            None => store.push_control_flow(control),
        }
        .map_err(|error| Error::UnsupportedImport {
            construct: error.to_string(),
        })
    }

    /// Reads a construct's body into a new block.
    ///
    /// A body is a single statement in the grammar, which is a block whenever
    /// it was written with braces and the statement itself otherwise.
    fn build_body(
        &mut self,
        body: &semast::Stmt,
        store: &mut CircuitStore,
    ) -> Result<BlockId, Error> {
        let block = store.new_block();
        match &*body.kind {
            semast::StmtKind::Block(inner) => {
                for statement in &inner.stmts {
                    self.build_statement(statement, Some(block), store)?;
                }
            }
            _ => self.build_statement(body, Some(block), store)?,
        }
        Ok(block)
    }

    fn build_block(
        &mut self,
        body: &semast::Block,
        store: &mut CircuitStore,
    ) -> Result<BlockId, Error> {
        let block = store.new_block();
        for statement in &body.stmts {
            self.build_statement(statement, Some(block), store)?;
        }
        Ok(block)
    }

    fn build_statement(
        &mut self,
        statement: &semast::Stmt,
        target: Option<BlockId>,
        store: &mut CircuitStore,
    ) -> Result<(), Error> {
        match &*statement.kind {
            // Declarations were handled while assigning the layout.
            semast::StmtKind::QubitDecl(_) | semast::StmtKind::QubitArrayDecl(_) => Ok(()),
            semast::StmtKind::GateCall(call) => self.build_gate_call(call, target, store),
            semast::StmtKind::Reset(reset) => {
                let qubits = self.qubits_of(std::slice::from_ref(&*reset.operand))?;
                for qubit in qubits {
                    Self::emit(target, store, PackedOperation::Reset, &[qubit], &[], None)?;
                }
                Ok(())
            }
            semast::StmtKind::Barrier(barrier) => {
                let qubits = self.qubits_of(&barrier.qubits)?;
                let arity = u8::try_from(qubits.len()).map_err(|_| Error::UnsupportedImport {
                    construct: "barrier over more qubits than a circuit records".to_string(),
                })?;
                Self::emit(
                    target,
                    store,
                    PackedOperation::Barrier { qubit_arity: arity },
                    &qubits,
                    &[],
                    None,
                )
            }
            semast::StmtKind::Delay(delay) => {
                let (duration, unit) = duration_value(&delay.duration)?;
                let qubits = self.qubits_of(&delay.qubits)?;
                for qubit in qubits {
                    Self::emit(
                        target,
                        store,
                        PackedOperation::Delay { duration, unit },
                        &[qubit],
                        &[],
                        None,
                    )?;
                }
                Ok(())
            }
            semast::StmtKind::ClassicalDecl(decl) => {
                // A declaration initialised by a measurement is a measurement.
                if let semast::ExprKind::Measure(measure) = &*decl.init_expr.kind {
                    return self.build_measure(measure, Some(decl.symbol_id), 0, target, store);
                }
                Ok(())
            }
            semast::StmtKind::Assign(assign) => {
                if let semast::ExprKind::Measure(measure) = &*assign.rhs.kind {
                    let (symbol, index) = classical_target(&assign.lhs)?;
                    return self.build_measure(measure, Some(symbol), index, target, store);
                }
                Ok(())
            }
            semast::StmtKind::ExprStmt(expr) => {
                if let semast::ExprKind::Measure(measure) = &*expr.expr.kind {
                    return self.build_measure(measure, None, 0, target, store);
                }
                Ok(())
            }
            _ => self.build_control_flow(statement, target, store),
        }
    }

    fn build_control_flow(
        &mut self,
        statement: &semast::Stmt,
        target: Option<BlockId>,
        store: &mut CircuitStore,
    ) -> Result<(), Error> {
        match &*statement.kind {
            semast::StmtKind::If(conditional) => {
                let condition = self.condition_of(&conditional.condition, store)?;
                let then_block = self.build_body(&conditional.if_body, store)?;
                let else_block = match &conditional.else_body {
                    Some(body) => Some(self.build_body(body, store)?),
                    None => None,
                };
                Self::emit_control(
                    target,
                    store,
                    ControlFlow::IfElse(IfElse {
                        condition,
                        then_block,
                        else_block,
                    }),
                )
            }
            semast::StmtKind::WhileLoop(loop_) => {
                let condition = self.condition_of(&loop_.condition, store)?;
                let body = self.build_body(&loop_.body, store)?;
                Self::emit_control(
                    target,
                    store,
                    ControlFlow::While(WhileLoop { condition, body }),
                )
            }
            semast::StmtKind::For(loop_) => self.build_for(loop_, target, store),
            semast::StmtKind::Switch(switch) => self.build_switch(switch, target, store),
            semast::StmtKind::Box(boxed) => {
                if boxed.duration.is_some() {
                    return Err(Error::UnsupportedImport {
                        construct: "box with duration".to_string(),
                    });
                }
                let block = store.new_block();
                for statement in &boxed.body {
                    self.build_statement(statement, Some(block), store)?;
                }
                Self::emit_control(
                    target,
                    store,
                    ControlFlow::Box(BoxBody {
                        body: block,
                        duration: None,
                        label: None,
                    }),
                )
            }
            // A bare block scopes names, which a circuit does not model, so its
            // contents join the block that contains it.
            semast::StmtKind::Block(inner) => {
                for statement in &inner.stmts {
                    self.build_statement(statement, target, store)?;
                }
                Ok(())
            }
            semast::StmtKind::Break(_) => Self::emit_control(target, store, ControlFlow::Break),
            semast::StmtKind::Continue(_) => {
                Self::emit_control(target, store, ControlFlow::Continue)
            }
            other => Err(Error::UnsupportedImport {
                construct: statement_name(other).to_string(),
            }),
        }
    }

    fn build_for(
        &mut self,
        loop_: &semast::ForStmt,
        target: Option<BlockId>,
        store: &mut CircuitStore,
    ) -> Result<(), Error> {
        let semast::EnumerableSet::Set(set) = &*loop_.set_declaration else {
            return Err(Error::UnsupportedImport {
                construct: "loop over anything but a listed set".to_string(),
            });
        };
        let values: Vec<Literal> = set
            .values
            .iter()
            .map(literal_value)
            .collect::<Result<_, Error>>()?;
        // OpenQASM has one kind of loop variable and it is read while the
        // program runs, so an index that a circuit held as a compile-time
        // parameter comes back as a runtime variable.
        let name = self.symbols[loop_.loop_variable].name.clone();
        let id = store.declare_var(name, loop_value_var_type(&values));
        self.vars.insert(loop_.loop_variable.0, id);
        let body = self.build_body(&loop_.body, store)?;
        Self::emit_control(
            target,
            store,
            ControlFlow::For(ForLoop {
                index: Some(LoopIndex::Var(id)),
                values,
                body,
            }),
        )
    }

    fn build_switch(
        &mut self,
        switch: &semast::SwitchStmt,
        target: Option<BlockId>,
        store: &mut CircuitStore,
    ) -> Result<(), Error> {
        let selector = self.classical_operand(&switch.target, store)?;
        let mut cases = Vec::with_capacity(switch.cases.len());
        // Cases keep the order they were written in. Sorting them would change
        // which body a value reaches whenever two cases name the same value.
        for case in &switch.cases {
            let labels: Vec<Literal> = case
                .labels
                .iter()
                .map(literal_value)
                .collect::<Result<_, Error>>()?;
            let block = self.build_block(&case.block, store)?;
            cases.push((labels, block));
        }
        let default = match &switch.default {
            Some(body) => Some(self.build_block(body, store)?),
            None => None,
        };
        Self::emit_control(
            target,
            store,
            ControlFlow::Switch(Switch {
                selector,
                cases,
                default,
            }),
        )
    }

    /// Reads a condition back into the circuit's own expression form.
    fn condition_of(&self, expr: &semast::Expr, store: &mut CircuitStore) -> Result<ExprId, Error> {
        if expr.get_const_value().is_some() {
            return literal_value(expr).map(|value| store.push_expr(Expr::Literal(value)));
        }
        match &*expr.kind {
            semast::ExprKind::Paren(inner) => self.condition_of(inner, store),
            semast::ExprKind::Cast(cast)
                if matches!((&cast.ty, &cast.expr.ty), (Type::Bool(_), Type::Bool(_))) =>
            {
                self.condition_of(&cast.expr, store)
            }
            semast::ExprKind::BinaryOp(binary) => {
                let op = binary_op_value(binary.op)?;
                if matches!(
                    op,
                    BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                ) && let Some(condition) = self.bit_comparison(binary, op, store)?
                {
                    return Ok(condition);
                }
                let left = self.condition_operand(&binary.lhs, store)?;
                let right = self.condition_operand(&binary.rhs, store)?;
                Ok(store.push_expr(Expr::Binary(op, left, right)))
            }
            _ => self.classical_operand(expr, store),
        }
    }

    fn bit_comparison(
        &self,
        binary: &semast::BinaryOpExpr,
        op: BinaryOp,
        store: &mut CircuitStore,
    ) -> Result<Option<ExprId>, Error> {
        let left = self.bit_value(&binary.lhs, store)?;
        let right = self.bit_value(&binary.rhs, store)?;
        if left.is_none() && right.is_none() {
            return Ok(None);
        }
        let width = left
            .as_ref()
            .or(right.as_ref())
            .expect("one bit operand")
            .0
            .len();
        let constant_bits = |expr: &semast::Expr,
                             store: &mut CircuitStore|
         -> Result<Vec<ExprId>, Error> {
            let value =
                if let Some(semast::LiteralKind::Bitstring(value, size)) = expr.get_const_value() {
                    if size as usize != width {
                        return Err(Error::UnsupportedImport {
                            construct: "register literal with an incompatible width".to_string(),
                        });
                    }
                    value
                } else {
                    match literal_value(expr)? {
                        Literal::Int(value) => num_bigint::BigInt::from(value),
                        Literal::Bool(value) => num_bigint::BigInt::from(u8::from(value)),
                        Literal::Float(_) => {
                            return Err(Error::UnsupportedImport {
                                construct: "non-integral register comparison".to_string(),
                            });
                        }
                    }
                };
            Ok((0..width)
                .map(|bit| store.push_expr(Expr::Literal(Literal::Bool(value.bit(bit as u64)))))
                .collect())
        };
        let signed = left.as_ref().or(right.as_ref()).expect("one bit operand").1;
        let left = match left {
            Some((bits, _)) => bits,
            None => constant_bits(&binary.lhs, store)?,
        };
        let right = match right {
            Some((bits, _)) => bits,
            None => constant_bits(&binary.rhs, store)?,
        };
        if left.len() != right.len() || width == 0 {
            return Err(Error::UnsupportedImport {
                construct: "register comparison with incompatible widths".to_string(),
            });
        }
        let mut equal = store.push_expr(Expr::Literal(Literal::Bool(true)));
        let mut less = store.push_expr(Expr::Literal(Literal::Bool(false)));
        for (index, (mut left, mut right)) in left.into_iter().zip(right).enumerate() {
            if signed && index + 1 == width {
                left = store.push_expr(Expr::Not(left));
                right = store.push_expr(Expr::Not(right));
            }
            let same = store.push_expr(Expr::Binary(BinaryOp::Eq, left, right));
            let lower = store.push_expr(Expr::Binary(BinaryOp::Lt, left, right));
            let tie = store.push_expr(Expr::Binary(BinaryOp::And, same, less));
            less = store.push_expr(Expr::Binary(BinaryOp::Or, lower, tie));
            equal = store.push_expr(Expr::Binary(BinaryOp::And, equal, same));
        }
        Ok(Some(match op {
            BinaryOp::Eq => equal,
            BinaryOp::Ne => store.push_expr(Expr::Not(equal)),
            BinaryOp::Lt => less,
            BinaryOp::Le => store.push_expr(Expr::Binary(BinaryOp::Or, less, equal)),
            BinaryOp::Gt => {
                let le = store.push_expr(Expr::Binary(BinaryOp::Or, less, equal));
                store.push_expr(Expr::Not(le))
            }
            BinaryOp::Ge => store.push_expr(Expr::Not(less)),
            _ => unreachable!("comparison"),
        }))
    }

    /// Reads a bit pattern without discarding widths or numeric cast semantics.
    fn bit_value(
        &self,
        expr: &semast::Expr,
        store: &mut CircuitStore,
    ) -> Result<Option<(Vec<ExprId>, bool)>, Error> {
        if expr.get_const_value().is_some() {
            return Ok(None);
        }
        match &*expr.kind {
            semast::ExprKind::Paren(inner) => self.bit_value(inner, store),
            semast::ExprKind::Cast(cast) if matches!(cast.ty, Type::Int(..) | Type::UInt(..)) => {
                let Some((mut bits, signed)) = self.bit_value(&cast.expr, store)? else {
                    return Ok(None);
                };
                let width = cast.ty.width().unwrap_or(i64::BITS) as usize;
                if bits.len() > width {
                    return Err(Error::UnsupportedImport {
                        construct: "register cast that cannot preserve its width".to_string(),
                    });
                }
                let extension = if signed {
                    *bits.last().ok_or_else(|| Error::UnsupportedImport {
                        construct: "zero-width signed register cast".to_string(),
                    })?
                } else {
                    store.push_expr(Expr::Literal(Literal::Bool(false)))
                };
                bits.resize(width, extension);
                Ok(Some((bits, matches!(cast.ty, Type::Int(..)))))
            }
            semast::ExprKind::ResolvedIdent(symbol) if self.clbit_base.contains_key(&symbol.0) => {
                let base = self.clbit_base[&symbol.0];
                let Type::BitArray(size, _) = self.symbols[*symbol].ty else {
                    return Ok(None);
                };
                Ok(Some((
                    (0..size)
                        .map(|offset| store.push_expr(Expr::Clbit(base + offset)))
                        .collect(),
                    false,
                )))
            }
            _ => Ok(None),
        }
    }

    /// Reads one side of a condition, which may itself be a comparison.
    fn condition_operand(
        &self,
        expr: &semast::Expr,
        store: &mut CircuitStore,
    ) -> Result<ExprId, Error> {
        match &*expr.kind {
            semast::ExprKind::BinaryOp(_) => self.condition_of(expr, store),
            _ => self.classical_operand(expr, store),
        }
    }

    /// Reads a classical value: a bit, a variable, or a literal.
    fn classical_operand(
        &self,
        expr: &semast::Expr,
        store: &mut CircuitStore,
    ) -> Result<ExprId, Error> {
        if expr.get_const_value().is_some() {
            return literal_value(expr).map(|value| store.push_expr(Expr::Literal(value)));
        }
        match &*expr.kind {
            semast::ExprKind::Paren(inner) => self.classical_operand(inner, store),
            semast::ExprKind::Cast(cast)
                if matches!(
                    (&cast.ty, &cast.expr.ty),
                    (
                        Type::Bool(_) | Type::Bit(_) | Type::Int(..) | Type::UInt(..),
                        Type::Bit(_) | Type::Bool(_)
                    )
                ) =>
            {
                self.classical_operand(&cast.expr, store)
            }
            semast::ExprKind::Cast(_) => Err(Error::UnsupportedImport {
                construct: "runtime cast in a condition".to_string(),
            }),
            semast::ExprKind::ResolvedIdent(symbol) => {
                if let Some(base) = self.clbit_base.get(&symbol.0) {
                    if matches!(self.symbols[*symbol].ty, Type::BitArray(size, _) if size != 1) {
                        return Err(Error::UnsupportedImport {
                            construct: "whole register outside a comparison".to_string(),
                        });
                    }
                    return Ok(store.push_expr(Expr::Clbit(*base)));
                }
                if let Some(id) = self.vars.get(&symbol.0) {
                    return Ok(store.push_expr(Expr::Var(*id)));
                }
                Err(Error::UnsupportedImport {
                    construct: format!("read of '{}'", self.symbols[*symbol].name),
                })
            }
            semast::ExprKind::IndexedExpr(indexed) => {
                let semast::ExprKind::ResolvedIdent(symbol) = &*indexed.collection.kind else {
                    return Err(Error::UnsupportedImport {
                        construct: "condition operand".to_string(),
                    });
                };
                let base =
                    self.clbit_base
                        .get(&symbol.0)
                        .ok_or_else(|| Error::UnsupportedImport {
                            construct: "read of a bit that was never declared".to_string(),
                        })?;
                let semast::Index::Expr(index) = &*indexed.index else {
                    return Err(Error::UnsupportedImport {
                        construct: "range over bits".to_string(),
                    });
                };
                let index = const_index(index).ok_or(Error::NotConstant { what: "bit index" })?;
                Ok(store.push_expr(Expr::Clbit(base + index)))
            }
            _ => literal_value(expr).map(|value| store.push_expr(Expr::Literal(value))),
        }
    }

    fn build_gate_call(
        &self,
        call: &semast::GateCall,
        target: Option<BlockId>,
        store: &mut CircuitStore,
    ) -> Result<(), Error> {
        if !call.modifiers.is_empty() {
            return Err(Error::UnsupportedImport {
                construct: "gate modifier".to_string(),
            });
        }
        let name = self.symbols[call.symbol_id].name.clone();
        let gate = gate_from_qasm_name(&name).ok_or_else(|| Error::UnsupportedImport {
            construct: format!("gate '{name}'"),
        })?;
        let qubits = self.qubits_of(&call.qubits)?;
        let angles: Vec<ParamValue> = call
            .args
            .iter()
            .map(|arg| constant_angle(arg).map(ParamValue::Literal))
            .collect::<Result<_, Error>>()?;
        let params = if angles.is_empty() {
            None
        } else {
            Some(angles.into_boxed_slice())
        };
        Self::emit(
            target,
            store,
            PackedOperation::Standard(gate),
            &qubits,
            &[],
            params,
        )
    }

    fn build_measure(
        &self,
        measure: &semast::MeasureExpr,
        target_bit: Option<SymbolId>,
        index: u32,
        target: Option<BlockId>,
        store: &mut CircuitStore,
    ) -> Result<(), Error> {
        let qubits = self.qubits_of(std::slice::from_ref(&measure.operand))?;
        for (offset, qubit) in qubits.into_iter().enumerate() {
            let clbits = match target_bit {
                Some(symbol) => {
                    let base =
                        self.clbit_base
                            .get(&symbol.0)
                            .ok_or_else(|| Error::UnsupportedImport {
                                construct: "measurement into an unknown bit".to_string(),
                            })?;
                    let offset = u32::try_from(offset).expect("operand count fits in u32");
                    vec![Clbit(base + index + offset)]
                }
                None => Vec::new(),
            };
            if clbits.is_empty() {
                return Err(Error::UnsupportedImport {
                    construct: "measurement with no target bit".to_string(),
                });
            }
            Self::emit(
                target,
                store,
                PackedOperation::Standard(StandardGate::MeasureZ),
                &[qubit],
                &clbits,
                None,
            )?;
        }
        Ok(())
    }

    /// Resolves gate operands to circuit qubit indices, expanding whole registers.
    fn qubits_of(&self, operands: &[semast::GateOperand]) -> Result<Vec<Qubit>, Error> {
        let mut resolved = Vec::with_capacity(operands.len());
        for operand in operands {
            let semast::GateOperandKind::Expr(expr) = &operand.kind else {
                return Err(Error::UnsupportedImport {
                    construct: "hardware qubit".to_string(),
                });
            };
            match &*expr.kind {
                semast::ExprKind::ResolvedIdent(symbol) => {
                    let base =
                        self.qubit_base
                            .get(&symbol.0)
                            .ok_or_else(|| Error::UnsupportedImport {
                                construct: "qubit that was never declared".to_string(),
                            })?;
                    match self.symbols[*symbol].ty {
                        Type::QubitArray(size) => {
                            resolved.extend((0..size).map(|offset| Qubit(base + offset)));
                        }
                        _ => resolved.push(Qubit(*base)),
                    }
                }
                semast::ExprKind::IndexedExpr(indexed) => {
                    let semast::ExprKind::ResolvedIdent(symbol) = &*indexed.collection.kind else {
                        return Err(Error::UnsupportedImport {
                            construct: "qubit reference".to_string(),
                        });
                    };
                    let base =
                        self.qubit_base
                            .get(&symbol.0)
                            .ok_or_else(|| Error::UnsupportedImport {
                                construct: "qubit that was never declared".to_string(),
                            })?;
                    let semast::Index::Expr(index) = &*indexed.index else {
                        return Err(Error::UnsupportedImport {
                            construct: "range over qubits".to_string(),
                        });
                    };
                    let index = const_index(index).ok_or(Error::NotConstant {
                        what: "qubit index",
                    })?;
                    resolved.push(Qubit(base + index));
                }
                _ => {
                    return Err(Error::UnsupportedImport {
                        construct: "qubit reference".to_string(),
                    });
                }
            }
        }
        Ok(resolved)
    }
}

/// Resolves the left side of a measurement assignment to a bit.
fn classical_target(expr: &semast::Expr) -> Result<(SymbolId, u32), Error> {
    match &*expr.kind {
        semast::ExprKind::ResolvedIdent(symbol) => Ok((*symbol, 0)),
        semast::ExprKind::IndexedExpr(indexed) => {
            let semast::ExprKind::ResolvedIdent(symbol) = &*indexed.collection.kind else {
                return Err(Error::UnsupportedImport {
                    construct: "measurement target".to_string(),
                });
            };
            let semast::Index::Expr(index) = &*indexed.index else {
                return Err(Error::UnsupportedImport {
                    construct: "range over bits".to_string(),
                });
            };
            let index = const_index(index).ok_or(Error::NotConstant { what: "bit index" })?;
            Ok((*symbol, index))
        }
        _ => Err(Error::UnsupportedImport {
            construct: "measurement target".to_string(),
        }),
    }
}

/// Reads an expression as a constant index.
///
/// Const evaluation fills in a value for some expressions but not for a plain
/// index literal, so the literal is read directly when it does.
fn const_index(expr: &semast::Expr) -> Option<u32> {
    if let Some(value) = expr.get_const_u32() {
        return Some(value);
    }
    match &*expr.kind {
        semast::ExprKind::Lit(semast::LiteralKind::Int(value)) => u32::try_from(*value).ok(),
        _ => None,
    }
}

/// Reads a gate argument as a fixed angle.
///
/// OpenQASM represents an angle as a fixed-point fraction of a turn rather than
/// as a float, and the front end folds a numeric argument into that form during
/// analysis. The value recovered here is therefore the nearest angle OpenQASM
/// can hold, not necessarily the value that was written.
fn constant_angle(expr: &semast::Expr) -> Result<f64, Error> {
    angle_value(expr).ok_or(Error::NotConstant { what: "gate angle" })
}

/// Reads an expression as a classical literal.
fn literal_value(expr: &semast::Expr) -> Result<Literal, Error> {
    if expr.get_const_value().is_none() {
        match &*expr.kind {
            semast::ExprKind::Paren(inner) => return literal_value(inner),
            semast::ExprKind::Cast(_) => {
                return Err(Error::UnsupportedImport {
                    construct: "cast without a representable constant value".to_string(),
                });
            }
            semast::ExprKind::UnaryOp(unary) if matches!(unary.op, semast::UnaryOp::Neg) => {
                return match literal_value(&unary.expr)? {
                    Literal::Int(value) => Ok(Literal::Int(-value)),
                    Literal::Float(value) => Ok(Literal::Float(-value)),
                    Literal::Bool(_) => Err(Error::UnsupportedImport {
                        construct: "negated boolean".to_string(),
                    }),
                };
            }
            _ => {}
        }
    }
    let literal = expr
        .get_const_value()
        .or_else(|| match &*expr.kind {
            semast::ExprKind::Lit(literal) => Some(literal.clone()),
            _ => None,
        })
        .ok_or(Error::NotConstant { what: "value" })?;
    match literal {
        semast::LiteralKind::Int(value) => Ok(Literal::Int(value)),
        semast::LiteralKind::Float(value) => Ok(Literal::Float(value)),
        semast::LiteralKind::Bool(value) | semast::LiteralKind::Bit(value) => {
            Ok(Literal::Bool(value))
        }
        _ => Err(Error::UnsupportedImport {
            construct: "value of a type a circuit does not hold".to_string(),
        }),
    }
}

/// Reads a delay's duration and the unit it is measured in.
fn duration_value(expr: &semast::Expr) -> Result<(f64, TimeUnit), Error> {
    let literal = expr
        .get_const_value()
        .or_else(|| match &*expr.kind {
            semast::ExprKind::Lit(literal) => Some(literal.clone()),
            _ => None,
        })
        .ok_or(Error::NotConstant {
            what: "delay duration",
        })?;
    let semast::LiteralKind::Duration(duration) = literal else {
        return Err(Error::UnsupportedImport {
            construct: "delay of something other than a duration".to_string(),
        });
    };
    let unit = match duration.unit {
        semast::TimeUnit::Dt => TimeUnit::Dt,
        semast::TimeUnit::Ns => TimeUnit::Ns,
        semast::TimeUnit::Us => TimeUnit::Us,
        semast::TimeUnit::Ms => TimeUnit::Ms,
        semast::TimeUnit::S => TimeUnit::S,
    };
    Ok((duration.value, unit))
}

/// The circuit operator an OpenQASM operator stands for.
fn binary_op_value(op: semast::BinOp) -> Result<BinaryOp, Error> {
    Ok(match op {
        semast::BinOp::Eq => BinaryOp::Eq,
        semast::BinOp::Neq => BinaryOp::Ne,
        semast::BinOp::Lt => BinaryOp::Lt,
        semast::BinOp::Lte => BinaryOp::Le,
        semast::BinOp::Gt => BinaryOp::Gt,
        semast::BinOp::Gte => BinaryOp::Ge,
        semast::BinOp::AndL | semast::BinOp::AndB => BinaryOp::And,
        semast::BinOp::OrL | semast::BinOp::OrB => BinaryOp::Or,
        semast::BinOp::Add => BinaryOp::Add,
        semast::BinOp::Sub => BinaryOp::Sub,
        semast::BinOp::Mul => BinaryOp::Mul,
        _ => {
            return Err(Error::UnsupportedImport {
                construct: "condition operator".to_string(),
            });
        }
    })
}

/// The declared type of an imported loop index.
fn loop_value_var_type(values: &[Literal]) -> VarType {
    if values
        .iter()
        .any(|value| matches!(value, Literal::Float(_)))
    {
        VarType::Float
    } else if !values.is_empty() && values.iter().all(|v| matches!(v, Literal::Bool(_))) {
        VarType::Bool
    } else {
        VarType::Int
    }
}

#[allow(clippy::cast_precision_loss)]
fn angle_value(expr: &semast::Expr) -> Option<f64> {
    if expr.get_const_value().is_none() {
        match &*expr.kind {
            semast::ExprKind::Cast(_) => return None,
            semast::ExprKind::Paren(inner) => return angle_value(inner),
            semast::ExprKind::UnaryOp(unary) if matches!(unary.op, semast::UnaryOp::Neg) => {
                return angle_value(&unary.expr).map(std::ops::Neg::neg);
            }
            _ => {}
        }
    }
    // Const evaluation fills in a value for some expressions but not for a plain
    // literal, so the literal is read directly when it does not.
    let literal = expr.get_const_value().or_else(|| match &*expr.kind {
        semast::ExprKind::Lit(literal) => Some(literal.clone()),
        _ => None,
    })?;
    match literal {
        semast::LiteralKind::Float(value) => Some(value),
        semast::LiteralKind::Int(value) => Some(value as f64),
        semast::LiteralKind::Angle(angle) => angle.try_into().ok(),
        _ => None,
    }
}

/// Names a statement a circuit has no place for.
fn statement_name(kind: &semast::StmtKind) -> &'static str {
    match kind {
        semast::StmtKind::Alias(_) => "alias",
        semast::StmtKind::Def(_) => "subroutine definition",
        semast::StmtKind::End(_) => "end",
        semast::StmtKind::QuantumGateDefinition(_) => "gate definition",
        _ => "statement",
    }
}
