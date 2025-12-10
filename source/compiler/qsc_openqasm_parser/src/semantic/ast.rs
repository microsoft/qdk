// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_bigint::BigInt;
use qsc_data_structures::span::Span;
use std::{
    collections::VecDeque,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use crate::{
    display_utils::{
        write_field, write_header, write_indented_list, write_list_field, write_opt_field,
        writeln_field, writeln_header, writeln_list_field, writeln_opt_field,
    },
    parser::ast::{List, PathKind},
    semantic::{
        symbols::SymbolId,
        types::{ArrayDimensions, Type},
    },
    stdlib::{angle::Angle, complex::Complex, duration::Duration},
};

use crate::parser::ast as syntax;

#[derive(Clone, Debug)]
pub struct Program {
    pub version: Option<Version>,
    pub pragmas: List<Pragma>,
    pub statements: List<Stmt>,
}

impl Display for Program {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Program:")?;
        writeln_opt_field(f, "version", self.version.as_ref())?;
        writeln_list_field(f, "pragmas", &self.pragmas)?;
        write_list_field(f, "statements", &self.statements)
    }
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub span: Span,
    pub annotations: List<Annotation>,
    pub kind: Box<StmtKind>,
}

impl Display for Stmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "Stmt", self.span)?;
        writeln_list_field(f, "annotations", &self.annotations)?;
        write_field(f, "kind", &self.kind)
    }
}

#[derive(Clone, Debug)]
pub struct Annotation {
    pub span: Span,
    pub identifier: PathKind,
    pub value: Option<Arc<str>>,
    pub value_span: Option<Span>,
}

impl Display for Annotation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let value = self.value.as_ref().map(|val| format!("\"{val}\""));
        writeln_header(f, "Annotation", self.span)?;
        writeln_field(f, "identifier", &self.identifier.as_string())?;
        writeln_opt_field(f, "value", value.as_ref())?;
        write_opt_field(f, "value_span", self.value_span.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct MeasureExpr {
    pub span: Span,
    pub measure_token_span: Span,
    pub operand: GateOperand,
}

impl Display for MeasureExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "MeasureExpr", self.span)?;
        writeln_field(f, "measure_token_span", &self.measure_token_span)?;
        write_field(f, "operand", &self.operand)
    }
}

#[derive(Clone, Debug, Default)]
pub struct GateOperand {
    pub span: Span,
    pub kind: GateOperandKind,
}

impl Display for GateOperand {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "GateOperand", self.span)?;
        write_field(f, "kind", &self.kind)
    }
}

#[derive(Clone, Debug, Default)]
pub enum GateOperandKind {
    /// `IndexedIdent` and `Ident` get lowered to an `Expr`.
    Expr(Box<Expr>),
    HardwareQubit(HardwareQubit),
    #[default]
    Err,
}

impl Display for GateOperandKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expr(expr) => write!(f, "{expr}"),
            Self::HardwareQubit(qubit) => write!(f, "{qubit}"),
            Self::Err => write!(f, "Err"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct HardwareQubit {
    pub span: Span,
    pub name: Arc<str>,
}

impl Display for HardwareQubit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "HardwareQubit {}: {}", self.span, self.name)
    }
}

/// A statement kind.
#[derive(Clone, Debug, Default)]
pub enum StmtKind {
    Alias(AliasDeclStmt),
    Assign(AssignStmt),
    Barrier(BarrierStmt),
    Box(BoxStmt),
    Block(Box<Block>),
    Break(BreakStmt),
    Calibration(CalibrationStmt),
    CalibrationGrammar(CalibrationGrammarStmt),
    ClassicalDecl(ClassicalDeclarationStmt),
    Continue(ContinueStmt),
    Def(DefStmt),
    DefCal(DefCalStmt),
    Delay(DelayStmt),
    End(EndStmt),
    ExprStmt(ExprStmt),
    ExternDecl(ExternDecl),
    For(ForStmt),
    GateCall(GateCall),
    If(IfStmt),
    Include(IncludeStmt),
    IndexedClassicalTypeAssign(IndexedClassicalTypeAssignStmt),
    InputDeclaration(InputDeclaration),
    OutputDeclaration(OutputDeclaration),
    MeasureArrow(MeasureArrowStmt),
    Pragma(Pragma),
    QuantumGateDefinition(QuantumGateDefinition),
    QubitDecl(QubitDeclaration),
    QubitArrayDecl(QubitArrayDeclaration),
    Reset(ResetStmt),
    Return(ReturnStmt),
    Switch(SwitchStmt),
    WhileLoop(WhileLoop),
    /// An invalid statement.
    #[default]
    Err,
}

impl Display for StmtKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            StmtKind::Alias(alias) => write!(f, "{alias}"),
            StmtKind::Assign(stmt) => write!(f, "{stmt}"),
            StmtKind::Barrier(barrier) => write!(f, "{barrier}"),
            StmtKind::Box(box_stmt) => write!(f, "{box_stmt}"),
            StmtKind::Block(block) => write!(f, "{block}"),
            StmtKind::Break(stmt) => write!(f, "{stmt}"),
            StmtKind::Calibration(cal) => write!(f, "{cal}"),
            StmtKind::CalibrationGrammar(grammar) => write!(f, "{grammar}"),
            StmtKind::ClassicalDecl(decl) => write!(f, "{decl}"),
            StmtKind::Continue(stmt) => write!(f, "{stmt}"),
            StmtKind::Def(def) => write!(f, "{def}"),
            StmtKind::DefCal(defcal) => write!(f, "{defcal}"),
            StmtKind::Delay(delay) => write!(f, "{delay}"),
            StmtKind::End(end_stmt) => write!(f, "{end_stmt}"),
            StmtKind::ExprStmt(expr) => write!(f, "{expr}"),
            StmtKind::ExternDecl(decl) => write!(f, "{decl}"),
            StmtKind::For(for_stmt) => write!(f, "{for_stmt}"),
            StmtKind::GateCall(gate_call) => write!(f, "{gate_call}"),
            StmtKind::If(if_stmt) => write!(f, "{if_stmt}"),
            StmtKind::IndexedClassicalTypeAssign(stmt) => write!(f, "{stmt}"),
            StmtKind::Include(include) => write!(f, "{include}"),
            StmtKind::InputDeclaration(io) => write!(f, "{io}"),
            StmtKind::OutputDeclaration(io) => write!(f, "{io}"),
            StmtKind::MeasureArrow(measure) => write!(f, "{measure}"),
            StmtKind::Pragma(pragma) => write!(f, "{pragma}"),
            StmtKind::QuantumGateDefinition(gate) => write!(f, "{gate}"),
            StmtKind::QubitDecl(decl) => write!(f, "{decl}"),
            StmtKind::QubitArrayDecl(decl) => write!(f, "{decl}"),
            StmtKind::Reset(reset_stmt) => write!(f, "{reset_stmt}"),
            StmtKind::Return(return_stmt) => write!(f, "{return_stmt}"),
            StmtKind::Switch(switch_stmt) => write!(f, "{switch_stmt}"),
            StmtKind::WhileLoop(while_loop) => write!(f, "{while_loop}"),
            StmtKind::Err => write!(f, "Err"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AliasDeclStmt {
    pub symbol_id: SymbolId,
    pub exprs: List<Expr>,
    pub span: Span,
}

impl Display for AliasDeclStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "AliasDeclStmt", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        write_list_field(f, "exprs", &self.exprs)
    }
}

#[derive(Clone, Debug)]
pub struct AssignStmt {
    pub span: Span,
    pub lhs: Expr,
    pub rhs: Expr,
}

impl Display for AssignStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "AssignStmt", self.span)?;
        writeln_field(f, "lhs", &self.lhs)?;
        write_field(f, "rhs", &self.rhs)
    }
}

#[derive(Clone, Debug)]
pub struct BarrierStmt {
    pub span: Span,
    pub qubits: List<GateOperand>,
}

impl Display for BarrierStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "BarrierStmt", self.span)?;
        write_list_field(f, "operands", &self.qubits)
    }
}

#[derive(Clone, Debug)]
pub struct BoxStmt {
    pub span: Span,
    pub duration: Option<Expr>,
    pub body: List<Stmt>,
}

impl Display for BoxStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "BoxStmt", self.span)?;
        writeln_opt_field(f, "duration", self.duration.as_ref())?;
        write_list_field(f, "body", &self.body)
    }
}

/// A sequenced block of statements.
#[derive(Clone, Debug, Default)]
pub struct Block {
    /// The span.
    pub span: Span,
    /// The statements in the block.
    pub stmts: List<Stmt>,
}

impl Display for Block {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write_header(f, "Block", self.span)?;
        write_indented_list(f, &self.stmts)
    }
}

#[derive(Clone, Debug, Default)]
pub struct BreakStmt {
    pub span: Span,
}

impl Display for BreakStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write_header(f, "BreakStmt", self.span)
    }
}

#[derive(Clone, Debug)]
pub struct CalibrationStmt {
    pub span: Span,
    pub content: Arc<str>,
}

impl Display for CalibrationStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "CalibrationStmt", self.span)?;
        write_field(f, "content", &self.content)
    }
}

#[derive(Clone, Debug)]
pub struct CalibrationGrammarStmt {
    pub span: Span,
    pub name: Arc<str>,
}

impl Display for CalibrationGrammarStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "CalibrationGrammarStmt", self.span)?;
        write_field(f, "name", &self.name)
    }
}

#[derive(Clone, Debug)]
pub struct ClassicalDeclarationStmt {
    pub span: Span,
    pub ty_span: Span,
    pub symbol_id: SymbolId,
    pub ty_exprs: List<Expr>,
    pub init_expr: Box<Expr>,
}

impl Display for ClassicalDeclarationStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ClassicalDeclarationStmt", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_field(f, "ty_span", &self.ty_span)?;
        writeln_list_field(f, "ty_exprs", &self.ty_exprs)?;
        write_field(f, "init_expr", self.init_expr.as_ref())
    }
}

#[derive(Clone, Debug, Default)]
pub struct ContinueStmt {
    pub span: Span,
}

impl Display for ContinueStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write_header(f, "ContinueStmt", self.span)
    }
}

#[derive(Clone, Debug)]
pub struct DefParameter {
    pub span: Span,
    pub symbol_id: SymbolId,
    pub ty_exprs: List<Expr>,
}

impl Display for DefParameter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DefParameter", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        write_list_field(f, "ty_exprs", &self.ty_exprs)
    }
}

#[derive(Clone, Debug)]
pub struct DefStmt {
    pub span: Span,
    pub symbol_id: SymbolId,
    pub has_qubit_params: bool,
    pub params: List<DefParameter>,
    pub body: Block,
    pub return_type_span: Span,
    pub return_ty_exprs: List<Expr>,
}

impl Display for DefStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DefStmt", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_field(f, "has_qubit_params", &self.has_qubit_params)?;
        writeln_list_field(f, "parameters", &self.params)?;
        writeln_field(f, "return_type_span", &self.return_type_span)?;
        writeln_list_field(f, "return_ty_exprs", &self.return_ty_exprs)?;
        write_field(f, "body", &self.body)
    }
}

#[derive(Clone, Debug)]
pub struct DefCalStmt {
    pub span: Span,
    pub content: Arc<str>,
}

impl Display for DefCalStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DefCalStmt", self.span)?;
        write_field(f, "content", &self.content)
    }
}

#[derive(Clone, Debug)]
pub struct DelayStmt {
    pub span: Span,
    pub duration: Expr,
    pub qubits: List<GateOperand>,
}

impl Display for DelayStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DelayStmt", self.span)?;
        writeln_field(f, "duration", &self.duration)?;
        write_list_field(f, "qubits", &self.qubits)
    }
}

#[derive(Clone, Debug)]
pub struct EndStmt {
    pub span: Span,
}

impl Display for EndStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "End {}", self.span)
    }
}

#[derive(Clone, Debug)]
pub struct ExprStmt {
    pub span: Span,
    pub expr: Expr,
}

impl Display for ExprStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ExprStmt", self.span)?;
        write_field(f, "expr", &self.expr)
    }
}

#[derive(Clone, Debug)]
pub struct ExternDecl {
    pub span: Span,
    pub symbol_id: SymbolId,
    pub ty_exprs: List<Expr>,
    pub return_ty_exprs: List<Expr>,
}

impl Display for ExternDecl {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ExternDecl", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_list_field(f, "ty_exprs", &self.ty_exprs)?;
        write_list_field(f, "return_ty_exprs", &self.return_ty_exprs)
    }
}

#[derive(Clone, Debug)]
pub struct ForStmt {
    pub span: Span,
    pub loop_variable: SymbolId,
    pub ty_exprs: List<Expr>,
    pub set_declaration: Box<EnumerableSet>,
    pub body: Stmt,
}

impl Display for ForStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ForStmt", self.span)?;
        writeln_field(f, "loop_variable", &self.loop_variable)?;
        writeln_list_field(f, "ty_exprs", &self.ty_exprs)?;
        writeln_field(f, "iterable", &self.set_declaration)?;
        write_field(f, "body", &self.body)
    }
}

#[derive(Clone, Debug)]
pub struct GateCall {
    pub span: Span,
    pub modifiers: List<QuantumGateModifier>,
    pub symbol_id: SymbolId,
    pub gate_name_span: Span,
    pub args: List<Expr>,
    pub qubits: List<GateOperand>,
    pub duration: Option<Expr>,
    pub classical_arity: u32,
    pub quantum_arity: u32,
}

impl Display for GateCall {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "GateCall", self.span)?;
        writeln_list_field(f, "modifiers", &self.modifiers)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_field(f, "gate_name_span", &self.gate_name_span)?;
        writeln_list_field(f, "args", &self.args)?;
        writeln_list_field(f, "qubits", &self.qubits)?;
        writeln_opt_field(f, "duration", self.duration.as_ref())?;
        writeln_field(f, "classical_arity", &self.classical_arity)?;
        write_field(f, "quantum_arity", &self.quantum_arity)
    }
}

#[derive(Clone, Debug)]
pub struct IfStmt {
    pub span: Span,
    pub condition: Expr,
    pub if_body: Stmt,
    pub else_body: Option<Stmt>,
}

impl Display for IfStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IfStmt", self.span)?;
        writeln_field(f, "condition", &self.condition)?;
        writeln_field(f, "if_body", &self.if_body)?;
        write_opt_field(f, "else_body", self.else_body.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct IncludeStmt {
    pub span: Span,
    pub filename: Arc<str>,
}

impl Display for IncludeStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IncludeStmt", self.span)?;
        write_field(f, "filename", &self.filename)
    }
}

#[derive(Clone, Debug)]
pub struct IndexedClassicalTypeAssignStmt {
    pub span: Span,
    pub lhs: Expr,
    pub indices: VecDeque<Index>,
    pub rhs: Expr,
}

impl Display for IndexedClassicalTypeAssignStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IndexedClassicalTypeAssignStmt", self.span)?;
        writeln_field(f, "lhs", &self.lhs)?;
        writeln_field(f, "rhs", &self.rhs)?;
        write_list_field(f, "indices", &self.indices)
    }
}

#[derive(Clone, Debug)]
pub struct InputDeclaration {
    pub span: Span,
    // We don't have a type span here, because input decls are in
    // the symbol table which tracks the ty span separately.
    pub symbol_id: SymbolId,
    pub ty_exprs: List<Expr>,
}

impl Display for InputDeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "InputDeclaration", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        write_list_field(f, "ty_exprs", &self.ty_exprs)
    }
}

#[derive(Clone, Debug)]
pub struct OutputDeclaration {
    pub span: Span,
    pub ty_span: Span,
    pub ty_exprs: List<Expr>,
    pub symbol_id: SymbolId,
    pub init_expr: Box<Expr>,
}

impl Display for OutputDeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "OutputDeclaration", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_list_field(f, "ty_exprs", &self.ty_exprs)?;
        writeln_field(f, "ty_span", &self.ty_span)?;
        write_field(f, "init_expr", &self.init_expr)
    }
}

#[derive(Clone, Debug)]
pub struct MeasureArrowStmt {
    pub span: Span,
    pub measurement: MeasureExpr,
    pub target: Option<Box<Expr>>,
}

impl Display for MeasureArrowStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "MeasureArrowStmt", self.span)?;
        writeln_field(f, "measurement", &self.measurement)?;
        write_opt_field(f, "target", self.target.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct Pragma {
    pub span: Span,
    pub identifier: Option<PathKind>,
    pub value: Option<Arc<str>>,
    pub value_span: Option<Span>,
}

impl Display for Pragma {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let value = self.value.as_ref().map(|val| format!("\"{val}\""));
        writeln_header(f, "Pragma", self.span)?;
        writeln_opt_field(
            f,
            "identifier",
            self.identifier.as_ref().map(PathKind::as_string).as_ref(),
        )?;
        writeln_opt_field(f, "value", value.as_ref())?;
        write_opt_field(f, "value_span", self.value_span.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct QuantumGateDefinition {
    pub span: Span,
    pub name_span: Span,
    pub symbol_id: SymbolId,
    pub params: Box<[SymbolId]>,
    pub qubits: Box<[SymbolId]>,
    pub body: Block,
}

impl Display for QuantumGateDefinition {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "Gate", self.span)?;
        writeln_field(f, "name_span", &self.name_span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_list_field(f, "parameters", &self.params)?;
        writeln_list_field(f, "qubits", &self.qubits)?;
        write_field(f, "body", &self.body)
    }
}

#[derive(Clone, Debug)]
pub struct QubitDeclaration {
    pub span: Span,
    pub symbol_id: SymbolId,
}

impl Display for QubitDeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "QubitDeclaration", self.span)?;
        write_field(f, "symbol_id", &self.symbol_id)
    }
}

#[derive(Clone, Debug)]
pub struct QubitArrayDeclaration {
    pub span: Span,
    pub symbol_id: SymbolId,
    /// This `Expr` is const, but we don't substitute by the `LiteralKind` yet
    /// to be able to provide Span and Type information to the Language Service.
    pub size: Expr,
    pub size_span: Span,
}

impl Display for QubitArrayDeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "QubitArrayDeclaration", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_field(f, "size", &self.size)?;
        write_field(f, "size_span", &self.size_span)
    }
}

#[derive(Clone, Debug)]
pub struct ResetStmt {
    pub span: Span,
    pub reset_token_span: Span,
    pub operand: Box<GateOperand>,
}

impl Display for ResetStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ResetStmt", self.span)?;
        writeln_field(f, "reset_token_span", &self.reset_token_span)?;
        write_field(f, "operand", &self.operand)
    }
}

#[derive(Clone, Debug)]
pub struct ReturnStmt {
    pub span: Span,
    pub expr: Option<Box<Expr>>,
}

impl Display for ReturnStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ReturnStmt", self.span)?;
        write_opt_field(f, "expr", self.expr.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct SwitchStmt {
    pub span: Span,
    pub target: Expr,
    pub cases: List<SwitchCase>,
    /// Note that `None` is quite different to `[]` in this case; the latter is
    /// an explicitly empty body, whereas the absence of a default might mean
    /// that the switch is inexhaustive, and a linter might want to complain.
    pub default: Option<Block>,
}

impl Display for SwitchStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "SwitchStmt", self.span)?;
        writeln_field(f, "target", &self.target)?;
        writeln_list_field(f, "cases", &self.cases)?;
        write_opt_field(f, "default_case", self.default.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct SwitchCase {
    pub span: Span,
    pub labels: List<Expr>,
    pub block: Block,
}

impl Display for SwitchCase {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "SwitchCase", self.span)?;
        writeln_list_field(f, "labels", &self.labels)?;
        write_field(f, "block", &self.block)
    }
}

#[derive(Clone, Debug)]
pub struct WhileLoop {
    pub span: Span,
    pub condition: Expr,
    pub body: Stmt,
}

impl Display for WhileLoop {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "WhileLoop", self.span)?;
        writeln_field(f, "condition", &self.condition)?;
        write_field(f, "body", &self.body)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Expr {
    pub span: Span,
    pub kind: Box<ExprKind>,
    pub const_value: Option<LiteralKind>,
    pub ty: Type,
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "Expr", self.span)?;
        writeln_field(f, "ty", &self.ty)?;
        if self.const_value.is_some() {
            writeln_opt_field(f, "const_value", self.const_value.as_ref())?;
        }
        write_field(f, "kind", &self.kind)
    }
}

impl Expr {
    #[must_use]
    pub fn new(span: Span, kind: ExprKind, ty: super::types::Type) -> Self {
        Self {
            span,
            kind: kind.into(),
            ty,
            const_value: None,
        }
    }

    #[must_use]
    pub fn int(val: i64, span: Span) -> Self {
        let val = LiteralKind::Int(val);
        Expr {
            span,
            kind: Box::new(ExprKind::Lit(val.clone())),
            ty: Type::Int(None, true),
            const_value: Some(val),
        }
    }

    #[must_use]
    pub fn uint(val: i64, span: Span) -> Self {
        let val = LiteralKind::Int(val);
        Expr {
            span,
            kind: Box::new(ExprKind::Lit(val.clone())),
            ty: Type::UInt(None, true),
            const_value: Some(val),
        }
    }

    #[must_use]
    pub fn float(val: f64, span: Span) -> Self {
        let val = LiteralKind::Float(val);
        Expr {
            span,
            kind: Box::new(ExprKind::Lit(val.clone())),
            ty: Type::Float(None, true),
            const_value: Some(val),
        }
    }

    #[must_use]
    pub fn builtin_funcall(
        name: &str,
        span: Span,
        fn_name_span: Span,
        function_ty: Type,
        args: &[Expr],
        output: LiteralKind,
    ) -> Self {
        let Type::Function(_, ty) = &function_ty else {
            unreachable!("if we hit this there is a bug in the builtin functions implementation");
        };

        let ty = ty.as_ref().clone();

        Self {
            span,
            kind: Box::new(ExprKind::BuiltinFunctionCall(BuiltinFunctionCall {
                span,
                fn_name_span,
                name: name.into(),
                args: args.into(),
                function_ty,
            })),
            ty,
            const_value: Some(output),
        }
    }

    #[must_use]
    pub fn bin_op(op: BinOp, lhs: Self, rhs: Self) -> Self {
        let ty = lhs.ty.clone();
        let span = Span {
            lo: lhs.span.lo,
            hi: rhs.span.hi,
        };

        Self {
            span,
            kind: Box::new(ExprKind::BinaryOp(BinaryOpExpr { op, lhs, rhs })),
            const_value: None,
            ty,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum ExprKind {
    /// An expression with invalid syntax that can't be parsed.
    #[default]
    Err,
    CapturedIdent(SymbolId),
    Ident(SymbolId),
    UnaryOp(UnaryOpExpr),
    BinaryOp(BinaryOpExpr),
    Lit(LiteralKind),
    FunctionCall(FunctionCall),
    BuiltinFunctionCall(BuiltinFunctionCall),
    Cast(Cast),
    IndexedExpr(IndexedExpr),
    Paren(Expr),
    Measure(MeasureExpr),
    SizeofCall(SizeofCallExpr),
    DurationofCall(DurationofCallExpr),
    Concat(ConcatExpr),
}

impl Display for ExprKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ExprKind::Err => write!(f, "Err"),
            ExprKind::CapturedIdent(id) => write!(f, "CapturedSymbolId({id})"),
            ExprKind::Ident(id) => write!(f, "SymbolId({id})"),
            ExprKind::UnaryOp(expr) => write!(f, "{expr}"),
            ExprKind::BinaryOp(expr) => write!(f, "{expr}"),
            ExprKind::Lit(lit) => write!(f, "Lit: {lit}"),
            ExprKind::FunctionCall(call) => write!(f, "{call}"),
            ExprKind::BuiltinFunctionCall(call) => write!(f, "{call}"),
            ExprKind::Cast(expr) => write!(f, "{expr}"),
            ExprKind::IndexedExpr(expr) => write!(f, "{expr}"),
            ExprKind::Paren(expr) => write!(f, "Paren {expr}"),
            ExprKind::Measure(expr) => write!(f, "{expr}"),
            ExprKind::SizeofCall(call) => write!(f, "{call}"),
            ExprKind::DurationofCall(call) => write!(f, "{call}"),
            ExprKind::Concat(expr) => write!(f, "{expr}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Set {
    pub span: Span,
    pub values: List<Expr>,
}

impl Display for Set {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "Set", self.span)?;
        write_list_field(f, "values", &self.values)
    }
}

#[derive(Clone, Debug)]
pub struct Range {
    pub span: Span,
    pub start: Option<Expr>,
    pub end: Option<Expr>,
    pub step: Option<Expr>,
}

impl Display for Range {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "Range", self.span)?;
        writeln_opt_field(f, "start", self.start.as_ref())?;
        writeln_opt_field(f, "step", self.step.as_ref())?;
        write_opt_field(f, "end", self.end.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct QuantumGateModifier {
    pub span: Span,
    pub modifier_keyword_span: Span,
    pub kind: GateModifierKind,
}

impl Display for QuantumGateModifier {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "QuantumGateModifier", self.span)?;
        writeln_field(f, "modifier_keyword_span", &self.modifier_keyword_span)?;
        write_field(f, "kind", &self.kind)
    }
}

#[derive(Clone, Debug)]
pub enum GateModifierKind {
    Inv,
    Pow(Expr),
    /// This `Expr` is const, but we don't substitute by the `LiteralKind` yet
    /// to be able to provide Span and Type information to the Language Service.
    Ctrl(Expr),
    /// This `Expr` is const, but we don't substitute by the `LiteralKind` yet
    /// to be able to provide Span and Type information to the Language Service.
    NegCtrl(Expr),
}

impl Display for GateModifierKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            GateModifierKind::Inv => write!(f, "Inv"),
            GateModifierKind::Pow(expr) => write!(f, "Pow {expr}"),
            GateModifierKind::Ctrl(ctrls) => write!(f, "Ctrl {ctrls:?}"),
            GateModifierKind::NegCtrl(ctrls) => write!(f, "NegCtrl {ctrls:?}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct IntType {
    pub span: Span,
    pub size: Option<Expr>,
}

impl Display for IntType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IntType", self.span)?;
        write_opt_field(f, "size", self.size.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct UIntType {
    pub span: Span,
    pub size: Option<Expr>,
}

impl Display for UIntType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "UIntType", self.span)?;
        write_opt_field(f, "size", self.size.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct FloatType {
    pub span: Span,
    pub size: Option<Expr>,
}

impl Display for FloatType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "FloatType", self.span)?;
        write_opt_field(f, "size", self.size.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct ComplexType {
    pub span: Span,
    pub base_size: Option<FloatType>,
}

impl Display for ComplexType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ComplexType", self.span)?;
        write_opt_field(f, "base_size", self.base_size.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct AngleType {
    pub span: Span,
    pub size: Option<Expr>,
}

impl Display for AngleType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "AngleType", self.span)?;
        write_opt_field(f, "size", self.size.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct BitType {
    pub span: Span,
    pub size: Option<Expr>,
}

impl Display for BitType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "BitType", self.span)?;
        write_opt_field(f, "size", self.size.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct QuantumArgument {
    pub span: Span,
    pub expr: Option<Expr>,
}

impl Display for QuantumArgument {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "QuantumArgument", self.span)?;
        write_opt_field(f, "expr", self.expr.as_ref())
    }
}

#[derive(Clone, Debug)]
pub enum EnumerableSet {
    Set(Set),
    Range(Box<Range>),
    Expr(Box<Expr>),
}

impl Display for EnumerableSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            EnumerableSet::Set(set) => write!(f, "{set}"),
            EnumerableSet::Range(range) => write!(f, "{range}"),
            EnumerableSet::Expr(expr) => write!(f, "{expr}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct UnaryOpExpr {
    pub span: Span,
    pub op: UnaryOp,
    pub expr: Expr,
}

impl Display for UnaryOpExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "UnaryOpExpr", self.span)?;
        writeln_field(f, "op", &self.op)?;
        write_field(f, "expr", &self.expr)
    }
}

#[derive(Clone, Debug)]
pub struct BinaryOpExpr {
    pub op: BinOp,
    pub lhs: Expr,
    pub rhs: Expr,
}

impl BinaryOpExpr {
    #[must_use]
    pub fn span(&self) -> Span {
        Span {
            lo: self.lhs.span.lo,
            hi: self.rhs.span.hi,
        }
    }
}

impl Display for BinaryOpExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "BinaryOpExpr:")?;
        writeln_field(f, "op", &self.op)?;
        writeln_field(f, "lhs", &self.lhs)?;
        write_field(f, "rhs", &self.rhs)
    }
}

#[derive(Clone, Debug)]
pub struct FunctionCall {
    pub span: Span,
    pub fn_name_span: Span,
    pub symbol_id: SymbolId,
    pub args: List<Expr>,
}

impl Display for FunctionCall {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "FunctionCall", self.span)?;
        writeln_field(f, "fn_name_span", &self.fn_name_span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        write_list_field(f, "args", &self.args)
    }
}

#[derive(Clone, Debug)]

pub struct SizeofCallExpr {
    pub span: Span,
    pub fn_name_span: Span,
    pub array: Expr,
    pub array_dims: u32,
    pub dim: Expr,
}

impl Display for SizeofCallExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "SizeofCallExpr", self.span)?;
        writeln_field(f, "fn_name_span", &self.fn_name_span)?;
        writeln_field(f, "array", &self.array)?;
        writeln_field(f, "array_dims", &self.array_dims)?;
        write_field(f, "dim", &self.dim)
    }
}

#[derive(Clone, Debug)]

pub struct DurationofCallExpr {
    pub span: Span,
    pub fn_name_span: Span,
    pub duration: Duration,
    pub scope: Block,
}

impl Display for DurationofCallExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DurationofCallExpr", self.span)?;
        writeln_field(f, "fn_name_span", &self.fn_name_span)?;
        writeln_field(f, "duration", &self.duration)?;
        write_field(f, "scope", &self.scope)
    }
}

/// The information in this struct is aimed to be consumed
/// by the language service. The result of the computation
/// is already stored in the [`Expr::const_value`] field by
/// the time the `Expr` is created.
#[derive(Clone, Debug)]
pub struct BuiltinFunctionCall {
    pub span: Span,
    pub fn_name_span: Span,
    pub name: Arc<str>,
    pub function_ty: Type,
    pub args: Box<[Expr]>,
}

impl Display for BuiltinFunctionCall {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "BuiltinFunctionCall", self.span)?;
        writeln_field(f, "fn_name_span", &self.fn_name_span)?;
        writeln_field(f, "name", &self.name)?;
        writeln_field(f, "function_ty", &self.function_ty)?;
        write_list_field(f, "args", self.args.iter())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastKind {
    Explicit,
    Implicit,
}

impl Display for CastKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CastKind::Explicit => write!(f, "Explicit"),
            CastKind::Implicit => write!(f, "Implicit"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Cast {
    pub span: Span,
    pub ty: Type,
    pub ty_exprs: List<Expr>,
    pub expr: Expr,
    pub kind: CastKind,
}

impl Display for Cast {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "Cast", self.span)?;
        writeln_field(f, "ty", &self.ty)?;
        writeln_list_field(f, "ty_exprs", &self.ty_exprs)?;
        writeln_field(f, "expr", &self.expr)?;
        write_field(f, "kind", &self.kind)
    }
}

#[derive(Clone, Debug)]
pub struct ConcatExpr {
    pub span: Span,
    pub operands: List<Expr>,
}

impl Display for ConcatExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ConcatExpr", self.span)?;
        write_list_field(f, "operands", &self.operands)
    }
}

#[derive(Clone, Debug)]
pub struct IndexedExpr {
    pub span: Span,
    pub collection: Box<Expr>,
    pub index: Box<Index>,
}

impl Display for IndexedExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IndexedExpr", self.span)?;
        writeln_field(f, "collection", &self.collection)?;
        write_field(f, "index", &self.index)
    }
}

#[derive(Clone, Debug)]
pub enum LiteralKind {
    Angle(Angle),
    Array(Array),
    Bitstring(BigInt, u32),
    Bool(bool),
    Duration(Duration),
    Float(f64),
    Complex(Complex),
    Int(i64),
    BigInt(BigInt),
    Bit(bool),
}

impl Display for LiteralKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LiteralKind::Array(array) => write!(f, "{array}"),
            LiteralKind::Bitstring(value, width) => {
                let width = *width as usize;
                write!(f, "Bitstring(\"{:0>width$}\")", value.to_str_radix(2))
            }
            LiteralKind::Angle(a) => write!(f, "Angle({a})"),
            LiteralKind::Bit(b) => write!(f, "Bit({:?})", u8::from(*b)),
            LiteralKind::Bool(b) => write!(f, "Bool({b:?})"),
            LiteralKind::Complex(value) => write!(f, "Complex({value})"),
            LiteralKind::Duration(value) => {
                write!(f, "Duration({value})")
            }
            LiteralKind::Float(value) => write!(f, "Float({value:?})"),
            LiteralKind::Int(i) => write!(f, "Int({i:?})"),
            LiteralKind::BigInt(i) => write!(f, "BigInt({i:?})"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Array {
    pub data: Vec<Expr>,
    pub dims: ArrayDimensions,
}

impl Display for Array {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write_list_field(f, "array", &self.data)
    }
}

impl Array {
    pub fn from_default(
        dims: ArrayDimensions,
        default: impl FnOnce() -> Expr,
        base_ty: &Type,
    ) -> Self {
        let dims_vec: Vec<_> = dims.clone().into_iter().collect();
        let data = Self::from_default_recursive(&dims_vec, default, base_ty);
        Self { data, dims }
    }

    fn from_default_recursive(
        dims: &[u32],
        default: impl FnOnce() -> Expr,
        base_ty: &Type,
    ) -> Vec<Expr> {
        if dims.is_empty() {
            vec![]
        } else if dims.len() == 1 {
            let size = dims[0] as usize;
            if size >= 1 {
                let default_value = default();
                vec![default_value; size]
            } else {
                vec![]
            }
        } else {
            let data = Self::from_default_recursive(&dims[1..], default, base_ty);
            let array = Self {
                data,
                dims: (&dims[1..]).into(),
            };
            let expr = Expr::new(
                Default::default(),
                ExprKind::Lit(LiteralKind::Array(array)),
                Type::make_array_ty(&dims[1..], base_ty),
            );
            let dim_size = dims[0] as usize;
            vec![expr; dim_size]
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Version {
    pub major: u32,
    pub minor: Option<u32>,
    pub span: Span,
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        // If the minor versions are missing
        // we assume them to be 0.
        let self_minor = self.minor.unwrap_or_default();
        let other_minor = other.minor.unwrap_or_default();

        // Then we check if the major and minor version are equal.
        self.major == other.major && self_minor == other_minor
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // If the minor versions are missing
        // we assume them to be 0.
        let self_minor = self.minor.unwrap_or_default();
        let other_minor = other.minor.unwrap_or_default();

        // We compare the major versions.
        match self.major.partial_cmp(&other.major) {
            // If they are equal, we disambiguate
            // using the minor versions.
            Some(core::cmp::Ordering::Equal) => self_minor.partial_cmp(&other_minor),
            // Else, we return their ordering.
            ord => ord,
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.minor {
            Some(minor) => write!(f, "{}.{}", self.major, minor),
            None => write!(f, "{}", self.major),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Index {
    Expr(Expr),
    Range(Box<Range>),
}

impl Index {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Index::Expr(expr) => expr.span,
            Index::Range(range) => range.span,
        }
    }
}

impl Display for Index {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Index::Expr(expr) => write!(f, "{expr}"),
            Index::Range(range) => write!(f, "{range}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeUnit {
    Dt,
    /// Nanoseconds.
    Ns,
    /// Microseconds.
    Us,
    /// Milliseconds.
    Ms,
    /// Seconds.
    #[default]
    S,
}

impl Display for TimeUnit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TimeUnit::Dt => write!(f, "dt"),
            TimeUnit::Ns => write!(f, "ns"),
            TimeUnit::Us => write!(f, "us"),
            TimeUnit::Ms => write!(f, "ms"),
            TimeUnit::S => write!(f, "s"),
        }
    }
}

impl From<crate::parser::ast::TimeUnit> for TimeUnit {
    fn from(value: crate::parser::ast::TimeUnit) -> Self {
        match value {
            syntax::TimeUnit::Dt => Self::Dt,
            syntax::TimeUnit::Ns => Self::Ns,
            syntax::TimeUnit::Us => Self::Us,
            syntax::TimeUnit::Ms => Self::Ms,
            syntax::TimeUnit::S => Self::S,
        }
    }
}

/// A binary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    /// Addition: `+`.
    Add,
    /// Bitwise AND: `&`.
    AndB,
    /// Logical AND: `&&`.
    AndL,
    /// Division: `/`.
    Div,
    /// Equality: `==`.
    Eq,
    /// Exponentiation: `**`.
    Exp,
    /// Greater than: `>`.
    Gt,
    /// Greater than or equal: `>=`.
    Gte,
    /// Less than: `<`.
    Lt,
    /// Less than or equal: `<=`.
    Lte,
    /// Modulus: `%`.
    Mod,
    /// Multiplication: `*`.
    Mul,
    /// Inequality: `!=`.
    Neq,
    /// Bitwise OR: `|`.
    OrB,
    /// Logical OR: `||`.
    OrL,
    /// Shift left: `<<`.
    Shl,
    /// Shift right: `>>`.
    Shr,
    /// Subtraction: `-`.
    Sub,
    /// Bitwise XOR: `^`.
    XorB,
}

impl Display for BinOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            BinOp::Add => write!(f, "Add"),
            BinOp::AndB => write!(f, "AndB"),
            BinOp::AndL => write!(f, "AndL"),
            BinOp::Div => write!(f, "Div"),
            BinOp::Eq => write!(f, "Eq"),
            BinOp::Exp => write!(f, "Exp"),
            BinOp::Gt => write!(f, "Gt"),
            BinOp::Gte => write!(f, "Gte"),
            BinOp::Lt => write!(f, "Lt"),
            BinOp::Lte => write!(f, "Lte"),
            BinOp::Mod => write!(f, "Mod"),
            BinOp::Mul => write!(f, "Mul"),
            BinOp::Neq => write!(f, "Neq"),
            BinOp::OrB => write!(f, "OrB"),
            BinOp::OrL => write!(f, "OrL"),
            BinOp::Shl => write!(f, "Shl"),
            BinOp::Shr => write!(f, "Shr"),
            BinOp::Sub => write!(f, "Sub"),
            BinOp::XorB => write!(f, "XorB"),
        }
    }
}

impl From<syntax::BinOp> for BinOp {
    fn from(value: syntax::BinOp) -> Self {
        match value {
            syntax::BinOp::Add => BinOp::Add,
            syntax::BinOp::AndB => BinOp::AndB,
            syntax::BinOp::AndL => BinOp::AndL,
            syntax::BinOp::Div => BinOp::Div,
            syntax::BinOp::Eq => BinOp::Eq,
            syntax::BinOp::Exp => BinOp::Exp,
            syntax::BinOp::Gt => BinOp::Gt,
            syntax::BinOp::Gte => BinOp::Gte,
            syntax::BinOp::Lt => BinOp::Lt,
            syntax::BinOp::Lte => BinOp::Lte,
            syntax::BinOp::Mod => BinOp::Mod,
            syntax::BinOp::Mul => BinOp::Mul,
            syntax::BinOp::Neq => BinOp::Neq,
            syntax::BinOp::OrB => BinOp::OrB,
            syntax::BinOp::OrL => BinOp::OrL,
            syntax::BinOp::Shl => BinOp::Shl,
            syntax::BinOp::Shr => BinOp::Shr,
            syntax::BinOp::Sub => BinOp::Sub,
            syntax::BinOp::XorB => BinOp::XorB,
        }
    }
}

/// A unary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    /// Negation: `-`.
    Neg,
    /// Bitwise NOT: `~`.
    NotB,
    /// Logical NOT: `!`.
    NotL,
}

impl Display for UnaryOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOp::Neg => write!(f, "Neg"),
            UnaryOp::NotB => write!(f, "NotB"),
            UnaryOp::NotL => write!(f, "NotL"),
        }
    }
}
