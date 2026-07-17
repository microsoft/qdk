// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Checked canonical serialization for the syntactic `OpenQASM` AST.

use num_bigint::Sign;
use std::fmt::Write as _;
use thiserror::Error;

use crate::{
    parser::ast::{
        AccessControl, Annotation, ArrayBaseTypeKind, ArrayReferenceType, ArrayType, BinOp, Block,
        DefParameter, DefParameterType, EnumerableSet, Expr, ExprKind, ExternParameter, ForStmt,
        GateModifierKind, GateOperand, GateOperandKind, IdentOrIndexedIdent, IfStmt, Index,
        IndexList, IndexListItem, LiteralKind, MeasureExpr, PathKind, Pragma, Program,
        QuantumGateModifier, Range, ScalarType, ScalarTypeKind, Stmt, StmtKind, SwitchStmt,
        TimeUnit, TypeDef, UnaryOp, ValueExpr, WhileLoop,
    },
    span::Span,
};

/// An error encountered while serializing a syntactic `OpenQASM` program.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum UnparseError {
    /// The AST contains a node synthesized by parser recovery.
    #[error("cannot unparse recovered syntax at {span}")]
    RecoveredSyntax { span: Span },
    /// The AST contains a floating-point value with no valid source spelling.
    #[error("cannot unparse a non-finite floating-point literal at {span}")]
    NonFiniteFloat { span: Span },
    /// A string value cannot be represented by the grammar.
    #[error("invalid string literal value at {span}: {message}")]
    InvalidString { span: Span, message: String },
    /// The AST contains syntax that this format version cannot serialize.
    #[error("unsupported syntax variant {kind} at {span}")]
    UnsupportedSyntax { kind: &'static str, span: Span },
}

impl UnparseError {
    /// Returns the stable machine-readable error code.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::RecoveredSyntax { .. } => "recovered-syntax",
            Self::NonFiniteFloat { .. } => "non-finite-float",
            Self::InvalidString { .. } => "invalid-string",
            Self::UnsupportedSyntax { .. } => "unsupported-syntax",
        }
    }

    /// Returns the source span associated with the error.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::RecoveredSyntax { span }
            | Self::NonFiniteFloat { span }
            | Self::InvalidString { span, .. }
            | Self::UnsupportedSyntax { span, .. } => *span,
        }
    }
}

/// Emits canonical source for a syntactic `OpenQASM` program.
///
/// Output uses LF line endings, two spaces per indentation level, one
/// statement per line, and exactly one trailing newline. Comments and original
/// whitespace are intentionally not preserved.
pub fn unparse(program: &Program) -> Result<String, UnparseError> {
    Emitter {
        qasm2: program.version.is_some_and(|version| version.major == 2),
    }
    .program(program)
}

struct Emitter {
    qasm2: bool,
}

impl Emitter {
    fn program(&self, program: &Program) -> Result<String, UnparseError> {
        let mut output = String::new();
        if let Some(version) = program.version {
            output.push_str("OPENQASM ");
            output.push_str(&version.to_string());
            output.push_str(";\n");
        }
        for statement in &program.statements {
            self.statement(&mut output, statement, 0)?;
        }
        if !output.ends_with('\n') {
            output.push('\n');
        }
        Ok(output)
    }

    #[allow(clippy::too_many_lines)]
    fn statement(
        &self,
        output: &mut String,
        statement: &Stmt,
        indent: usize,
    ) -> Result<(), UnparseError> {
        for annotation in &statement.annotations {
            self.annotation(output, annotation, indent)?;
        }

        match statement.kind.as_ref() {
            StmtKind::Alias(alias) => self.line(
                output,
                indent,
                &format!(
                    "let {} = {};",
                    self.ident_or_indexed(&alias.ident)?,
                    self.expressions(&alias.exprs, " ++ ", indent)?
                ),
            ),
            StmtKind::Assign(assign) => self.line(
                output,
                indent,
                &format!(
                    "{} = {};",
                    self.ident_or_indexed(&assign.lhs)?,
                    self.value_expression(&assign.rhs, indent)?
                ),
            ),
            StmtKind::AssignOp(assign) => {
                let operator =
                    assignment_operator(assign.op).ok_or(UnparseError::UnsupportedSyntax {
                        kind: "compound-assignment operator",
                        span: assign.span,
                    })?;
                self.line(
                    output,
                    indent,
                    &format!(
                        "{} {operator}= {};",
                        self.ident_or_indexed(&assign.lhs)?,
                        self.value_expression(&assign.rhs, indent)?
                    ),
                );
            }
            StmtKind::Barrier(barrier) => {
                let operands = self.gate_operands(&barrier.qubits, indent)?;
                let rendered = if operands.is_empty() {
                    "barrier;".to_string()
                } else {
                    format!("barrier {operands};")
                };
                self.line(output, indent, &rendered);
            }
            StmtKind::Box(box_statement) => {
                self.pad(output, indent);
                output.push_str("box");
                if let Some(duration) = &box_statement.duration {
                    output.push('[');
                    output.push_str(&self.expression(duration, indent)?);
                    output.push(']');
                }
                output.push_str(" {\n");
                for body_statement in &box_statement.body {
                    self.statement(output, body_statement, indent + 1)?;
                }
                self.pad(output, indent);
                output.push_str("}\n");
            }
            StmtKind::Break(_) => self.line(output, indent, "break;"),
            StmtKind::Block(block) => {
                self.pad(output, indent);
                self.block(output, block, indent)?;
                output.push('\n');
            }
            StmtKind::Cal(calibration) => {
                self.raw_statement(output, indent, &calibration.content);
            }
            StmtKind::CalibrationGrammar(grammar) => {
                let name = string_literal(&grammar.name, grammar.span)?;
                self.line(output, indent, &format!("defcalgrammar {name};"));
            }
            StmtKind::ClassicalDecl(declaration) => {
                if self.qasm2
                    && declaration.init_expr.is_none()
                    && let TypeDef::Scalar(scalar) = declaration.ty.as_ref()
                    && let ScalarTypeKind::Bit(bit) = &scalar.kind
                {
                    let size = self.size_designator(bit.size.as_ref(), indent)?;
                    self.line(
                        output,
                        indent,
                        &format!("creg {}{size};", declaration.identifier.name),
                    );
                } else {
                    let initializer = declaration
                        .init_expr
                        .as_ref()
                        .map(|value| {
                            self.value_expression(value, indent)
                                .map(|value| format!(" = {value}"))
                        })
                        .transpose()?
                        .unwrap_or_default();
                    self.line(
                        output,
                        indent,
                        &format!(
                            "{} {}{initializer};",
                            self.type_definition(&declaration.ty, indent)?,
                            declaration.identifier.name
                        ),
                    );
                }
            }
            StmtKind::ConstDecl(declaration) => self.line(
                output,
                indent,
                &format!(
                    "const {} {} = {};",
                    self.type_definition(&declaration.ty, indent)?,
                    declaration.identifier.name,
                    self.value_expression(&declaration.init_expr, indent)?
                ),
            ),
            StmtKind::Continue(_) => self.line(output, indent, "continue;"),
            StmtKind::Def(definition) => {
                let parameters = definition
                    .params
                    .iter()
                    .map(|parameter| self.def_parameter(parameter, indent))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ");
                let return_type = definition
                    .return_type
                    .as_ref()
                    .map(|return_type| {
                        self.scalar_type(return_type, indent)
                            .map(|return_type| format!(" -> {return_type}"))
                    })
                    .transpose()?
                    .unwrap_or_default();
                self.pad(output, indent);
                write!(
                    output,
                    "def {}({parameters}){return_type} ",
                    definition.name.name
                )
                .expect("writing to String should succeed");
                self.block(output, &definition.body, indent)?;
                output.push('\n');
            }
            StmtKind::DefCal(calibration) => {
                self.raw_statement(output, indent, &calibration.content);
            }
            StmtKind::Delay(delay) => self.line(
                output,
                indent,
                &format!(
                    "delay[{}]{};",
                    self.expression(&delay.duration, indent)?,
                    self.space_gate_operands(&delay.qubits, indent)?
                ),
            ),
            StmtKind::End(_) => self.line(output, indent, "end;"),
            StmtKind::ExprStmt(expression) => self.line(
                output,
                indent,
                &format!("{};", self.expression(&expression.expr, indent)?),
            ),
            StmtKind::ExternDecl(declaration) => {
                let parameters = declaration
                    .params
                    .iter()
                    .map(|parameter| self.extern_parameter(parameter, indent))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ");
                let return_type = declaration
                    .return_type
                    .as_ref()
                    .map(|return_type| {
                        self.scalar_type(return_type, indent)
                            .map(|return_type| format!(" -> {return_type}"))
                    })
                    .transpose()?
                    .unwrap_or_default();
                self.line(
                    output,
                    indent,
                    &format!(
                        "extern {}({parameters}){return_type};",
                        declaration.ident.name
                    ),
                );
            }
            StmtKind::For(for_statement) => self.for_statement(output, for_statement, indent)?,
            StmtKind::If(if_statement) => self.if_statement(output, if_statement, indent)?,
            StmtKind::GateCall(call) => {
                let arguments = if call.args.is_empty() {
                    String::new()
                } else {
                    format!("({})", self.expressions(&call.args, ", ", indent)?)
                };
                let duration = call
                    .duration
                    .as_ref()
                    .map(|duration| {
                        self.expression(duration, indent)
                            .map(|duration| format!("[{duration}]"))
                    })
                    .transpose()?
                    .unwrap_or_default();
                self.line(
                    output,
                    indent,
                    &format!(
                        "{}{}{arguments}{duration}{};",
                        self.modifiers(&call.modifiers, indent)?,
                        call.name.name,
                        self.space_gate_operands(&call.qubits, indent)?
                    ),
                );
            }
            StmtKind::GPhase(phase) => {
                let arguments = if phase.args.is_empty() {
                    String::new()
                } else {
                    format!("({})", self.expressions(&phase.args, ", ", indent)?)
                };
                let duration = phase
                    .duration
                    .as_ref()
                    .map(|duration| {
                        self.expression(duration, indent)
                            .map(|duration| format!("[{duration}]"))
                    })
                    .transpose()?
                    .unwrap_or_default();
                self.line(
                    output,
                    indent,
                    &format!(
                        "{}gphase{arguments}{duration}{};",
                        self.modifiers(&phase.modifiers, indent)?,
                        self.space_gate_operands(&phase.qubits, indent)?
                    ),
                );
            }
            StmtKind::Include(include) => {
                let filename = string_literal(&include.filename, include.span)?;
                self.line(output, indent, &format!("include {filename};"));
            }
            StmtKind::IODeclaration(declaration) => self.line(
                output,
                indent,
                &format!(
                    "{} {} {};",
                    declaration.io_identifier,
                    self.type_definition(&declaration.ty, indent)?,
                    declaration.ident.name
                ),
            ),
            StmtKind::Measure(measurement) => {
                let target = measurement
                    .target
                    .as_ref()
                    .map(|target| {
                        self.ident_or_indexed(target)
                            .map(|target| format!(" -> {target}"))
                    })
                    .transpose()?
                    .unwrap_or_default();
                self.line(
                    output,
                    indent,
                    &format!(
                        "{}{target};",
                        self.measure_expression(&measurement.measurement, indent)?
                    ),
                );
            }
            StmtKind::Pragma(pragma) => self.pragma(output, pragma, indent)?,
            StmtKind::QuantumGateDefinition(definition) => {
                let parameters = definition
                    .params
                    .iter()
                    .map(|parameter| {
                        parameter
                            .item_as_ref()
                            .map(|parameter| parameter.name.as_ref())
                            .ok_or(UnparseError::RecoveredSyntax {
                                span: definition.span,
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ");
                let parameters = if parameters.is_empty() {
                    String::new()
                } else {
                    format!("({parameters})")
                };
                let qubits = definition
                    .qubits
                    .iter()
                    .map(|qubit| {
                        qubit.item_as_ref().map(|qubit| qubit.name.as_ref()).ok_or(
                            UnparseError::RecoveredSyntax {
                                span: definition.span,
                            },
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ");
                self.pad(output, indent);
                write!(
                    output,
                    "gate {}{parameters} {qubits} ",
                    definition.ident.name
                )
                .expect("writing to String should succeed");
                self.block(output, &definition.body, indent)?;
                output.push('\n');
            }
            StmtKind::QuantumDecl(declaration) => {
                let size = self.size_designator(declaration.ty.size.as_ref(), indent)?;
                if self.qasm2 {
                    self.line(
                        output,
                        indent,
                        &format!("qreg {}{size};", declaration.qubit.name),
                    );
                } else {
                    self.line(
                        output,
                        indent,
                        &format!("qubit{size} {};", declaration.qubit.name),
                    );
                }
            }
            StmtKind::Reset(reset) => self.line(
                output,
                indent,
                &format!("reset {};", self.gate_operand(&reset.operand, indent)?),
            ),
            StmtKind::Return(return_statement) => {
                if let Some(value) = &return_statement.expr {
                    let value = self.value_expression(value, indent)?;
                    self.line(output, indent, &format!("return {value};"));
                } else {
                    self.line(output, indent, "return;");
                }
            }
            StmtKind::Switch(switch) => self.switch_statement(output, switch, indent)?,
            StmtKind::WhileLoop(while_loop) => {
                self.while_statement(output, while_loop, indent)?;
            }
            StmtKind::Err => {
                return Err(UnparseError::RecoveredSyntax {
                    span: statement.span,
                });
            }
        }
        Ok(())
    }

    fn annotation(
        &self,
        output: &mut String,
        annotation: &Annotation,
        indent: usize,
    ) -> Result<(), UnparseError> {
        let mut rendered = format!(
            "@{}",
            complete_path(&annotation.identifier, annotation.span)?
        );
        if let Some(value) = &annotation.value {
            rendered.push(' ');
            rendered.push_str(value);
        }
        self.line(output, indent, &rendered);
        Ok(())
    }

    fn pragma(
        &self,
        output: &mut String,
        pragma: &Pragma,
        indent: usize,
    ) -> Result<(), UnparseError> {
        let mut rendered = String::from("pragma");
        if let Some(identifier) = &pragma.identifier {
            rendered.push(' ');
            rendered.push_str(&complete_path(identifier, pragma.span)?);
        }
        if let Some(value) = &pragma.value {
            rendered.push(' ');
            rendered.push_str(value);
        }
        self.line(output, indent, &rendered);
        Ok(())
    }

    fn for_statement(
        &self,
        output: &mut String,
        statement: &ForStmt,
        indent: usize,
    ) -> Result<(), UnparseError> {
        self.pad(output, indent);
        write!(
            output,
            "for {} {} in {} ",
            self.scalar_type(&statement.ty, indent)?,
            statement.ident.name,
            self.enumerable_set(&statement.set_declaration, indent)?
        )
        .expect("writing to String should succeed");
        self.body(output, &statement.body, indent)?;
        output.push('\n');
        Ok(())
    }

    fn if_statement(
        &self,
        output: &mut String,
        statement: &IfStmt,
        indent: usize,
    ) -> Result<(), UnparseError> {
        self.pad(output, indent);
        write!(
            output,
            "if ({}) ",
            self.expression(&statement.condition, indent)?
        )
        .expect("writing to String should succeed");
        self.body(output, &statement.if_body, indent)?;
        if let Some(else_body) = &statement.else_body {
            output.push_str(" else ");
            self.body(output, else_body, indent)?;
        }
        output.push('\n');
        Ok(())
    }

    fn while_statement(
        &self,
        output: &mut String,
        statement: &WhileLoop,
        indent: usize,
    ) -> Result<(), UnparseError> {
        self.pad(output, indent);
        write!(
            output,
            "while ({}) ",
            self.expression(&statement.while_condition, indent)?
        )
        .expect("writing to String should succeed");
        self.body(output, &statement.body, indent)?;
        output.push('\n');
        Ok(())
    }

    fn switch_statement(
        &self,
        output: &mut String,
        statement: &SwitchStmt,
        indent: usize,
    ) -> Result<(), UnparseError> {
        self.pad(output, indent);
        writeln!(
            output,
            "switch ({}) {{",
            self.expression(&statement.target, indent)?
        )
        .expect("writing to String should succeed");
        for case in &statement.cases {
            self.pad(output, indent + 1);
            write!(
                output,
                "case {} ",
                self.expressions(&case.labels, ", ", indent + 1)?
            )
            .expect("writing to String should succeed");
            self.block(output, &case.block, indent + 1)?;
            output.push('\n');
        }
        if let Some(default) = &statement.default {
            self.pad(output, indent + 1);
            output.push_str("default ");
            self.block(output, default, indent + 1)?;
            output.push('\n');
        }
        self.pad(output, indent);
        output.push_str("}\n");
        Ok(())
    }

    fn block(&self, output: &mut String, block: &Block, indent: usize) -> Result<(), UnparseError> {
        output.push_str("{\n");
        for statement in &block.stmts {
            self.statement(output, statement, indent + 1)?;
        }
        self.pad(output, indent);
        output.push('}');
        Ok(())
    }

    fn body(
        &self,
        output: &mut String,
        statement: &Stmt,
        indent: usize,
    ) -> Result<(), UnparseError> {
        if let StmtKind::Block(block) = statement.kind.as_ref() {
            self.block(output, block, indent)
        } else {
            output.push_str("{\n");
            self.statement(output, statement, indent + 1)?;
            self.pad(output, indent);
            output.push('}');
            Ok(())
        }
    }

    fn modifiers(
        &self,
        modifiers: &[QuantumGateModifier],
        indent: usize,
    ) -> Result<String, UnparseError> {
        let mut rendered = String::new();
        for modifier in modifiers {
            let value = match &modifier.kind {
                GateModifierKind::Inv => "inv @".to_string(),
                GateModifierKind::Pow(exponent) => {
                    format!("pow({}) @", self.expression(exponent, indent)?)
                }
                GateModifierKind::Ctrl(None) => "ctrl @".to_string(),
                GateModifierKind::Ctrl(Some(count)) => {
                    format!("ctrl({}) @", self.expression(count, indent)?)
                }
                GateModifierKind::NegCtrl(None) => "negctrl @".to_string(),
                GateModifierKind::NegCtrl(Some(count)) => {
                    format!("negctrl({}) @", self.expression(count, indent)?)
                }
            };
            rendered.push_str(&value);
            rendered.push(' ');
        }
        Ok(rendered)
    }

    fn measure_expression(
        &self,
        measurement: &MeasureExpr,
        indent: usize,
    ) -> Result<String, UnparseError> {
        Ok(format!(
            "measure {}",
            self.gate_operand(&measurement.operand, indent)?
        ))
    }

    fn gate_operand(&self, operand: &GateOperand, _indent: usize) -> Result<String, UnparseError> {
        match &operand.kind {
            GateOperandKind::IdentOrIndexedIdent(identifier) => self.ident_or_indexed(identifier),
            GateOperandKind::HardwareQubit(qubit) => Ok(format!("${}", qubit.name)),
            GateOperandKind::Err => Err(UnparseError::RecoveredSyntax { span: operand.span }),
        }
    }

    fn gate_operands(
        &self,
        operands: &[GateOperand],
        indent: usize,
    ) -> Result<String, UnparseError> {
        operands
            .iter()
            .map(|operand| self.gate_operand(operand, indent))
            .collect::<Result<Vec<_>, _>>()
            .map(|operands| operands.join(", "))
    }

    fn space_gate_operands(
        &self,
        operands: &[GateOperand],
        indent: usize,
    ) -> Result<String, UnparseError> {
        let operands = self.gate_operands(operands, indent)?;
        Ok(if operands.is_empty() {
            String::new()
        } else {
            format!(" {operands}")
        })
    }

    fn ident_or_indexed(&self, identifier: &IdentOrIndexedIdent) -> Result<String, UnparseError> {
        match identifier {
            IdentOrIndexedIdent::Ident(identifier) => Ok(identifier.name.to_string()),
            IdentOrIndexedIdent::IndexedIdent(identifier) => {
                let mut rendered = identifier.ident.name.to_string();
                for index in &identifier.indices {
                    rendered.push_str(&self.index(index, 0)?);
                }
                Ok(rendered)
            }
        }
    }

    fn value_expression(&self, value: &ValueExpr, indent: usize) -> Result<String, UnparseError> {
        match value {
            ValueExpr::Concat(concatenation) => {
                self.expressions(&concatenation.operands, " ++ ", indent)
            }
            ValueExpr::Expr(expression) => self.expression(expression, indent),
            ValueExpr::Measurement(measurement) => self.measure_expression(measurement, indent),
        }
    }

    fn expression(&self, expression: &Expr, indent: usize) -> Result<String, UnparseError> {
        match expression.kind.as_ref() {
            ExprKind::Err => Err(UnparseError::RecoveredSyntax {
                span: expression.span,
            }),
            ExprKind::Ident(identifier) => Ok(identifier.name.to_string()),
            ExprKind::UnaryOp(unary) => {
                let operand_precedence = expression_precedence(&unary.expr);
                let needs_parentheses = operand_precedence < UNARY_PRECEDENCE
                    || matches!(unary.expr.kind.as_ref(), ExprKind::UnaryOp(_));
                let operand =
                    parenthesize(self.expression(&unary.expr, indent)?, needs_parentheses);
                Ok(format!("{}{operand}", unary_operator(unary.op)))
            }
            ExprKind::BinaryOp(binary) => {
                let precedence = binary_operator_precedence(binary.op);
                let right_associative = matches!(binary.op, BinOp::Exp);
                let lhs_precedence = expression_precedence(&binary.lhs);
                let rhs_precedence = expression_precedence(&binary.rhs);
                let lhs = parenthesize(
                    self.expression(&binary.lhs, indent)?,
                    lhs_precedence < precedence
                        || (lhs_precedence == precedence && right_associative),
                );
                let rhs = parenthesize(
                    self.expression(&binary.rhs, indent)?,
                    rhs_precedence < precedence
                        || (rhs_precedence == precedence && !right_associative),
                );
                Ok(format!("{lhs} {} {rhs}", binary_operator(binary.op)))
            }
            ExprKind::Lit(literal) => self.literal(&literal.kind, literal.span, indent),
            ExprKind::FunctionCall(call) => Ok(format!(
                "{}({})",
                call.name.name,
                self.expressions(&call.args, ", ", indent)?
            )),
            ExprKind::Cast(cast) => Ok(format!(
                "{}({})",
                self.type_definition(&cast.ty, indent)?,
                self.expression(&cast.arg, indent)?
            )),
            ExprKind::IndexExpr(index) => Ok(format!(
                "{}{}",
                parenthesize(
                    self.expression(&index.collection, indent)?,
                    expression_precedence(&index.collection) < ATOM_PRECEDENCE
                ),
                self.index(&index.index, indent)?
            )),
            ExprKind::Paren(inner) => self.expression(inner, indent),
            ExprKind::DurationOf(duration) => {
                let mut rendered = String::from("durationof(");
                self.block(&mut rendered, &duration.scope, indent)?;
                rendered.push(')');
                Ok(rendered)
            }
        }
    }

    fn expressions(
        &self,
        expressions: &[Expr],
        separator: &str,
        indent: usize,
    ) -> Result<String, UnparseError> {
        expressions
            .iter()
            .map(|expression| self.expression(expression, indent))
            .collect::<Result<Vec<_>, _>>()
            .map(|expressions| expressions.join(separator))
    }

    fn literal(
        &self,
        literal: &LiteralKind,
        span: Span,
        indent: usize,
    ) -> Result<String, UnparseError> {
        match literal {
            LiteralKind::Array(expressions) => Ok(format!(
                "{{{}}}",
                self.expressions(expressions, ", ", indent)?
            )),
            LiteralKind::Bitstring(value, width) => {
                if *width == 0 || value.sign() == Sign::Minus {
                    return Err(UnparseError::UnsupportedSyntax {
                        kind: "bitstring literal",
                        span,
                    });
                }
                let digits = value.to_str_radix(2);
                let width =
                    usize::try_from(*width).map_err(|_| UnparseError::UnsupportedSyntax {
                        kind: "bitstring width",
                        span,
                    })?;
                if digits.len() > width {
                    return Err(UnparseError::UnsupportedSyntax {
                        kind: "bitstring width",
                        span,
                    });
                }
                Ok(format!("\"{digits:0>width$}\""))
            }
            LiteralKind::Bool(value) => Ok(value.to_string()),
            LiteralKind::Duration(value, unit) => Ok(format!(
                "{}{}",
                finite_float(*value, span)?,
                time_unit(*unit)
            )),
            LiteralKind::Float(value) => finite_float(*value, span),
            LiteralKind::Imaginary(value) => Ok(format!("{}im", finite_float(*value, span)?)),
            LiteralKind::Int(value) if *value == i64::MIN => Ok(value.unsigned_abs().to_string()),
            LiteralKind::Int(value) => Ok(value.to_string()),
            LiteralKind::BigInt(value) => Ok(value.to_string()),
            LiteralKind::String(value) => string_literal(value, span),
        }
    }

    fn index(&self, index: &Index, indent: usize) -> Result<String, UnparseError> {
        match index {
            Index::DiscreteSet(set) => Ok(format!(
                "[{{{}}}]",
                self.expressions(&set.values, ", ", indent)?
            )),
            Index::IndexList(list) => Ok(format!("[{}]", self.index_list(list, indent)?)),
        }
    }

    fn index_list(&self, list: &IndexList, indent: usize) -> Result<String, UnparseError> {
        list.values
            .iter()
            .map(|item| match item {
                IndexListItem::RangeDefinition(range) => self.range(range, indent),
                IndexListItem::Expr(expression) => self.expression(expression, indent),
                IndexListItem::Err => Err(UnparseError::RecoveredSyntax { span: list.span }),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|items| items.join(", "))
    }

    fn range(&self, range: &Range, indent: usize) -> Result<String, UnparseError> {
        let start = range
            .start
            .as_ref()
            .map(|start| self.expression(start, indent))
            .transpose()?
            .unwrap_or_default();
        let end = range
            .end
            .as_ref()
            .map(|end| self.expression(end, indent))
            .transpose()?
            .unwrap_or_default();
        if let Some(step) = &range.step {
            Ok(format!("{start}:{}:{end}", self.expression(step, indent)?))
        } else {
            Ok(format!("{start}:{end}"))
        }
    }

    fn enumerable_set(&self, set: &EnumerableSet, indent: usize) -> Result<String, UnparseError> {
        match set {
            EnumerableSet::DiscreteSet(set) => Ok(format!(
                "{{{}}}",
                self.expressions(&set.values, ", ", indent)?
            )),
            EnumerableSet::Range(range) => Ok(format!("[{}]", self.range(range, indent)?)),
            EnumerableSet::Expr(expression) => self.expression(expression, indent),
        }
    }

    fn size_designator(&self, size: Option<&Expr>, indent: usize) -> Result<String, UnparseError> {
        size.map(|size| {
            self.expression(size, indent)
                .map(|size| format!("[{size}]"))
        })
        .transpose()
        .map(Option::unwrap_or_default)
    }

    fn type_definition(&self, definition: &TypeDef, indent: usize) -> Result<String, UnparseError> {
        match definition {
            TypeDef::Scalar(scalar) => self.scalar_type(scalar, indent),
            TypeDef::Array(array) => self.array_type(array, indent),
            TypeDef::ArrayReference(reference) => self.array_reference_type(reference, indent),
        }
    }

    fn scalar_type(&self, scalar: &ScalarType, indent: usize) -> Result<String, UnparseError> {
        match &scalar.kind {
            ScalarTypeKind::Bit(bit) => Ok(format!(
                "bit{}",
                self.size_designator(bit.size.as_ref(), indent)?
            )),
            ScalarTypeKind::Int(integer) => Ok(format!(
                "int{}",
                self.size_designator(integer.size.as_ref(), indent)?
            )),
            ScalarTypeKind::Uint(integer) => Ok(format!(
                "uint{}",
                self.size_designator(integer.size.as_ref(), indent)?
            )),
            ScalarTypeKind::Float(float) => Ok(format!(
                "float{}",
                self.size_designator(float.size.as_ref(), indent)?
            )),
            ScalarTypeKind::Complex(complex) => {
                if let Some(base) = &complex.base_size {
                    Ok(format!(
                        "complex[float{}]",
                        self.size_designator(base.size.as_ref(), indent)?
                    ))
                } else {
                    Ok("complex".to_string())
                }
            }
            ScalarTypeKind::Angle(angle) => Ok(format!(
                "angle{}",
                self.size_designator(angle.size.as_ref(), indent)?
            )),
            ScalarTypeKind::BoolType => Ok("bool".to_string()),
            ScalarTypeKind::Duration => Ok("duration".to_string()),
            ScalarTypeKind::Stretch => Ok("stretch".to_string()),
            ScalarTypeKind::Err => Err(UnparseError::RecoveredSyntax { span: scalar.span }),
        }
    }

    fn array_base_type(
        &self,
        base: &ArrayBaseTypeKind,
        indent: usize,
    ) -> Result<String, UnparseError> {
        match base {
            ArrayBaseTypeKind::Int(integer) => Ok(format!(
                "int{}",
                self.size_designator(integer.size.as_ref(), indent)?
            )),
            ArrayBaseTypeKind::Uint(integer) => Ok(format!(
                "uint{}",
                self.size_designator(integer.size.as_ref(), indent)?
            )),
            ArrayBaseTypeKind::Float(float) => Ok(format!(
                "float{}",
                self.size_designator(float.size.as_ref(), indent)?
            )),
            ArrayBaseTypeKind::Complex(complex) => {
                if let Some(base) = &complex.base_size {
                    Ok(format!(
                        "complex[float{}]",
                        self.size_designator(base.size.as_ref(), indent)?
                    ))
                } else {
                    Ok("complex".to_string())
                }
            }
            ArrayBaseTypeKind::Angle(angle) => Ok(format!(
                "angle{}",
                self.size_designator(angle.size.as_ref(), indent)?
            )),
            ArrayBaseTypeKind::BoolType => Ok("bool".to_string()),
            ArrayBaseTypeKind::Duration => Ok("duration".to_string()),
        }
    }

    fn array_type(&self, array: &ArrayType, indent: usize) -> Result<String, UnparseError> {
        Ok(format!(
            "array[{}, {}]",
            self.array_base_type(&array.base_type, indent)?,
            self.expressions(&array.dimensions, ", ", indent)?
        ))
    }

    fn array_reference_type(
        &self,
        reference: &ArrayReferenceType,
        indent: usize,
    ) -> Result<String, UnparseError> {
        match reference {
            ArrayReferenceType::Static(reference) => Ok(format!(
                "{} array[{}, {}]",
                access_control(&reference.mutability),
                self.array_base_type(&reference.base_type, indent)?,
                self.expressions(&reference.dimensions, ", ", indent)?
            )),
            ArrayReferenceType::Dyn(reference) => Ok(format!(
                "{} array[{}, #dim = {}]",
                access_control(&reference.mutability),
                self.array_base_type(&reference.base_type, indent)?,
                self.expression(&reference.dimensions, indent)?
            )),
        }
    }

    fn def_parameter(
        &self,
        parameter: &DefParameter,
        indent: usize,
    ) -> Result<String, UnparseError> {
        let type_definition = match parameter.ty.as_ref() {
            DefParameterType::ArrayReference(reference) => {
                self.array_reference_type(reference, indent)?
            }
            DefParameterType::Qubit(qubit) => format!(
                "qubit{}",
                self.size_designator(qubit.size.as_ref(), indent)?
            ),
            DefParameterType::Scalar(scalar) => self.scalar_type(scalar, indent)?,
        };
        Ok(format!("{type_definition} {}", parameter.ident.name))
    }

    fn extern_parameter(
        &self,
        parameter: &ExternParameter,
        indent: usize,
    ) -> Result<String, UnparseError> {
        match parameter {
            ExternParameter::ArrayReference(reference, _) => {
                self.array_reference_type(reference, indent)
            }
            ExternParameter::Scalar(scalar, _) => self.scalar_type(scalar, indent),
        }
    }

    fn line(&self, output: &mut String, indent: usize, value: &str) {
        self.pad(output, indent);
        output.push_str(value);
        output.push('\n');
    }

    fn raw_statement(&self, output: &mut String, indent: usize, value: &str) {
        self.pad(output, indent);
        output.push_str(&normalize_line_endings(value));
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }

    #[allow(clippy::unused_self)]
    fn pad(&self, output: &mut String, indent: usize) {
        output.push_str(&"  ".repeat(indent));
    }
}

const UNARY_PRECEDENCE: u8 = 11;
const ATOM_PRECEDENCE: u8 = 15;

fn expression_precedence(expression: &Expr) -> u8 {
    match expression.kind.as_ref() {
        ExprKind::BinaryOp(binary) => binary_operator_precedence(binary.op),
        ExprKind::UnaryOp(_) => UNARY_PRECEDENCE,
        ExprKind::Paren(inner) => expression_precedence(inner),
        ExprKind::Err
        | ExprKind::Ident(_)
        | ExprKind::Lit(_)
        | ExprKind::FunctionCall(_)
        | ExprKind::Cast(_)
        | ExprKind::IndexExpr(_)
        | ExprKind::DurationOf(_) => ATOM_PRECEDENCE,
    }
}

fn binary_operator_precedence(operator: BinOp) -> u8 {
    match operator {
        BinOp::Exp => 12,
        BinOp::Mul | BinOp::Div | BinOp::Mod => 10,
        BinOp::Add | BinOp::Sub => 9,
        BinOp::Shl | BinOp::Shr => 8,
        BinOp::Gt | BinOp::Gte | BinOp::Lt | BinOp::Lte => 7,
        BinOp::Eq | BinOp::Neq => 6,
        BinOp::AndB => 5,
        BinOp::OrB => 4,
        BinOp::XorB => 3,
        BinOp::AndL => 2,
        BinOp::OrL => 1,
    }
}

fn binary_operator(operator: BinOp) -> &'static str {
    match operator {
        BinOp::Add => "+",
        BinOp::AndB => "&",
        BinOp::AndL => "&&",
        BinOp::Div => "/",
        BinOp::Eq => "==",
        BinOp::Exp => "**",
        BinOp::Gt => ">",
        BinOp::Gte => ">=",
        BinOp::Lt => "<",
        BinOp::Lte => "<=",
        BinOp::Mod => "%",
        BinOp::Mul => "*",
        BinOp::Neq => "!=",
        BinOp::OrB => "|",
        BinOp::OrL => "||",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Sub => "-",
        BinOp::XorB => "^",
    }
}

fn assignment_operator(operator: BinOp) -> Option<&'static str> {
    match operator {
        BinOp::Add
        | BinOp::AndB
        | BinOp::AndL
        | BinOp::Div
        | BinOp::Exp
        | BinOp::Mod
        | BinOp::Mul
        | BinOp::OrB
        | BinOp::OrL
        | BinOp::Shl
        | BinOp::Shr
        | BinOp::Sub
        | BinOp::XorB => Some(binary_operator(operator)),
        BinOp::Eq | BinOp::Gt | BinOp::Gte | BinOp::Lt | BinOp::Lte | BinOp::Neq => None,
    }
}

fn unary_operator(operator: UnaryOp) -> &'static str {
    match operator {
        UnaryOp::Neg => "-",
        UnaryOp::NotB => "~",
        UnaryOp::NotL => "!",
    }
}

fn parenthesize(value: String, needs_parentheses: bool) -> String {
    if needs_parentheses {
        format!("({value})")
    } else {
        value
    }
}

fn finite_float(value: f64, span: Span) -> Result<String, UnparseError> {
    if !value.is_finite() {
        return Err(UnparseError::NonFiniteFloat { span });
    }
    let rendered = value.to_string();
    if rendered.contains(['.', 'e', 'E']) {
        Ok(rendered)
    } else {
        Ok(format!("{rendered}.0"))
    }
}

fn string_literal(value: &str, span: Span) -> Result<String, UnparseError> {
    let mut rendered = String::with_capacity(value.len() + 2);
    let quote = if value
        .chars()
        .all(|character| matches!(character, '0' | '1' | '_'))
        && !value.is_empty()
    {
        '\''
    } else {
        '"'
    };
    rendered.push(quote);
    for character in value.chars() {
        match character {
            '\\' => rendered.push_str("\\\\"),
            '"' if quote == '"' => rendered.push_str("\\\""),
            '\'' if quote == '\'' => rendered.push_str("\\'"),
            '\n' => rendered.push_str("\\n"),
            '\r' => rendered.push_str("\\r"),
            '\t' => rendered.push_str("\\t"),
            character if character.is_control() => {
                return Err(UnparseError::InvalidString {
                    span,
                    message: format!(
                        "control character U+{:04X} has no OpenQASM escape",
                        character as u32
                    ),
                });
            }
            character => rendered.push(character),
        }
    }
    rendered.push(quote);
    Ok(rendered)
}

fn complete_path(path: &PathKind, span: Span) -> Result<String, UnparseError> {
    match path {
        PathKind::Ok(path) => Ok(path
            .segments
            .iter()
            .flatten()
            .map(|segment| segment.name.as_ref())
            .chain(std::iter::once(path.name.name.as_ref()))
            .collect::<Vec<_>>()
            .join(".")),
        PathKind::Err(_) => Err(UnparseError::RecoveredSyntax { span }),
    }
}

fn access_control(access: &AccessControl) -> &'static str {
    match access {
        AccessControl::ReadOnly => "readonly",
        AccessControl::Mutable => "mutable",
    }
}

fn time_unit(unit: TimeUnit) -> &'static str {
    match unit {
        TimeUnit::Dt => "dt",
        TimeUnit::Ns => "ns",
        TimeUnit::Us => "us",
        TimeUnit::Ms => "ms",
        TimeUnit::S => "s",
    }
}

fn normalize_line_endings(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
#[path = "unparse/tests.rs"]
mod tests;
