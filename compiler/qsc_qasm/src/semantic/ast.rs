// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_bigint::BigInt;
use qsc_data_structures::span::Span;
use std::{
    fmt::{self, Display, Formatter},
    hash::Hash,
    rc::Rc,
};

use crate::{
    display_utils::{
        write_field, write_header, write_indented_list, write_list_field, write_opt_field,
        write_opt_list_field, writeln_field, writeln_header, writeln_list_field, writeln_opt_field,
    },
    parser::ast::List,
    semantic::symbols::SymbolId,
    stdlib::angle::Angle,
};

use crate::parser::ast as syntax;

#[derive(Clone, Debug)]
pub struct Program {
    pub statements: List<Stmt>,
    pub version: Option<Version>,
}

impl Display for Program {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Program:")?;
        writeln_opt_field(f, "version", self.version.as_ref())?;
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
    pub identifier: Rc<str>,
    pub value: Option<Rc<str>>,
}

impl Display for Annotation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let identifier = format!("\"{}\"", self.identifier);
        let value = self.value.as_ref().map(|val| format!("\"{val}\""));
        writeln_header(f, "Annotation", self.span)?;
        writeln_field(f, "identifier", &identifier)?;
        write_opt_field(f, "value", value.as_ref())
    }
}

/// A path that was successfully parsed up to a certain `.`,
/// but is missing its final identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IncompletePath {
    /// The whole span of the incomplete path,
    /// including the final `.` and any whitespace or keyword
    /// that follows it.
    pub span: Span,
    /// Any segments that were successfully parsed before the final `.`.
    pub segments: Box<[Ident]>,
    /// Whether a keyword exists after the final `.`.
    /// This keyword can be presumed to be a partially typed identifier.
    pub keyword: bool,
}

/// A path to a declaration or a field access expression,
/// to be disambiguated during name resolution.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Path {
    /// The span.
    pub span: Span,
    /// The segments that make up the front of the path before the final `.`.
    pub segments: Option<Box<[Ident]>>,
    /// The declaration or field name.
    pub name: Box<Ident>,
}

impl Display for Path {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln_header(f, "Path", self.span)?;
        writeln_field(f, "name", &self.name)?;
        write_opt_list_field(f, "segments", self.segments.as_ref())
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

/// A binary operator.
#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Copy, Debug)]
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
    pub name: Rc<str>,
}

impl Display for HardwareQubit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "HardwareQubit {}: {}", self.span, self.name)
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

/// A statement kind.
#[derive(Clone, Debug, Default)]
pub enum StmtKind {
    Alias(AliasDeclStmt),
    Assign(AssignStmt),
    IndexedAssign(IndexedAssignStmt),
    AssignOp(AssignOpStmt),
    Barrier(BarrierStmt),
    Box(BoxStmt),
    Block(Box<Block>),
    Break(BreakStmt),
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
    If(IfStmt),
    GateCall(GateCall),
    Include(IncludeStmt),
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
            StmtKind::AssignOp(stmt) => write!(f, "{stmt}"),
            StmtKind::Barrier(barrier) => write!(f, "{barrier}"),
            StmtKind::Box(box_stmt) => write!(f, "{box_stmt}"),
            StmtKind::Block(block) => write!(f, "{block}"),
            StmtKind::Break(stmt) => write!(f, "{stmt}"),
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
            StmtKind::Include(include) => write!(f, "{include}"),
            StmtKind::IndexedAssign(assign) => write!(f, "{assign}"),
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
pub struct CalibrationGrammarStmt {
    pub span: Span,
    pub name: String,
}

impl Display for CalibrationGrammarStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "CalibrationGrammarStmt", self.span)?;
        write_field(f, "name", &self.name)
    }
}

#[derive(Clone, Debug)]
pub struct DefCalStmt {
    pub span: Span,
}

impl Display for DefCalStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "DefCalStmt {}", self.span)
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

#[derive(Clone, Debug, Default)]
pub struct ContinueStmt {
    pub span: Span,
}

impl Display for ContinueStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write_header(f, "ContinueStmt", self.span)
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Ident {
    pub span: Span,
    pub name: Rc<str>,
}

impl Default for Ident {
    fn default() -> Self {
        Ident {
            span: Span::default(),
            name: "".into(),
        }
    }
}

impl Display for Ident {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Ident {} \"{}\"", self.span, self.name)
    }
}

#[derive(Clone, Debug)]
pub struct IndexedIdent {
    pub span: Span,
    pub name_span: Span,
    pub index_span: Span,
    pub symbol_id: SymbolId,
    pub indices: List<IndexElement>,
}

impl Display for IndexedIdent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IndexedIdent", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_field(f, "name_span", &self.name_span)?;
        writeln_field(f, "index_span", &self.index_span)?;
        write_list_field(f, "indices", &self.indices)
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

#[derive(Clone, Debug, Default)]
pub struct Expr {
    pub span: Span,
    pub kind: Box<ExprKind>,
    pub ty: super::types::Type,
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "Expr", self.span)?;
        writeln_field(f, "ty", &self.ty)?;
        write_field(f, "kind", &self.kind)
    }
}

#[derive(Clone, Debug)]
pub struct DiscreteSet {
    pub span: Span,
    pub values: List<Expr>,
}

impl Display for DiscreteSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DiscreteSet", self.span)?;
        write_list_field(f, "values", &self.values)
    }
}

#[derive(Clone, Debug)]
pub struct IndexSet {
    pub span: Span,
    pub values: List<IndexSetItem>,
}

impl Display for IndexSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IndexSet", self.span)?;
        write_list_field(f, "values", &self.values)
    }
}

#[derive(Clone, Debug)]
pub struct RangeDefinition {
    pub span: Span,
    pub start: Option<Expr>,
    pub end: Option<Expr>,
    pub step: Option<Expr>,
}

impl Display for RangeDefinition {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "RangeDefinition", self.span)?;
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
    Ctrl(u32),
    NegCtrl(u32),
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
pub struct Pragma {
    pub span: Span,
    pub identifier: Rc<str>,
    pub value: Option<Rc<str>>,
}

impl Display for Pragma {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let identifier = format!("\"{}\"", self.identifier);
        let value = self.value.as_ref().map(|val| format!("\"{val}\""));
        writeln_header(f, "Pragma", self.span)?;
        writeln_field(f, "identifier", &identifier)?;
        write_opt_field(f, "value", value.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct IncludeStmt {
    pub span: Span,
    pub filename: String,
}

impl Display for IncludeStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IncludeStmt", self.span)?;
        write_field(f, "filename", &self.filename)
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
    pub size: u32,
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
pub struct ExternDecl {
    pub span: Span,
    pub symbol_id: SymbolId,
    pub params: Box<[crate::types::Type]>,
    pub return_type: crate::types::Type,
}

impl Display for ExternDecl {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ExternDecl", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_list_field(f, "parameters", &self.params)?;
        write_field(f, "return_type", &self.return_type)
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

#[derive(Clone, Debug)]
pub struct MeasureArrowStmt {
    pub span: Span,
    pub measurement: MeasureExpr,
    pub target: Option<Box<IndexedIdent>>,
}

impl Display for MeasureArrowStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "MeasureArrowStmt", self.span)?;
        writeln_field(f, "measurement", &self.measurement)?;
        write_opt_field(f, "target", self.target.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct ClassicalDeclarationStmt {
    pub span: Span,
    pub ty_span: Span,
    pub symbol_id: SymbolId,
    pub init_expr: Box<Expr>,
}

impl Display for ClassicalDeclarationStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ClassicalDeclarationStmt", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_field(f, "ty_span", &self.ty_span)?;
        write_field(f, "init_expr", self.init_expr.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct InputDeclaration {
    pub span: Span,
    pub symbol_id: SymbolId,
}

impl Display for InputDeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "InputDeclaration", self.span)?;
        write_field(f, "symbol_id", &self.symbol_id)
    }
}

#[derive(Clone, Debug)]
pub struct OutputDeclaration {
    pub span: Span,
    pub ty_span: Span,
    pub symbol_id: SymbolId,
    pub init_expr: Box<Expr>,
}

impl Display for OutputDeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "OutputDeclaration", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_field(f, "ty_span", &self.ty_span)?;
        write_field(f, "init_expr", &self.init_expr)
    }
}

#[derive(Clone, Debug)]
pub struct DefStmt {
    pub span: Span,
    pub symbol_id: SymbolId,
    pub has_qubit_params: bool,
    pub params: Box<[SymbolId]>,
    pub body: Block,
    pub return_type: crate::types::Type,
}

impl Display for DefStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DefStmt", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_field(f, "has_qubit_params", &self.has_qubit_params)?;
        writeln_list_field(f, "parameters", &self.params)?;
        writeln_field(f, "return_type", &self.return_type)?;
        write_field(f, "body", &self.body)
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

#[derive(Clone, Debug)]
pub struct ForStmt {
    pub span: Span,
    pub loop_variable: SymbolId,
    pub set_declaration: Box<EnumerableSet>,
    pub body: Stmt,
}

impl Display for ForStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ForStmt", self.span)?;
        writeln_field(f, "loop_variable", &self.loop_variable)?;
        writeln_field(f, "iterable", &self.set_declaration)?;
        write_field(f, "body", &self.body)
    }
}

#[derive(Clone, Debug)]
pub enum EnumerableSet {
    DiscreteSet(DiscreteSet),
    RangeDefinition(RangeDefinition),
    Expr(Expr),
}

impl Display for EnumerableSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            EnumerableSet::DiscreteSet(set) => write!(f, "{set}"),
            EnumerableSet::RangeDefinition(range) => write!(f, "{range}"),
            EnumerableSet::Expr(expr) => write!(f, "{expr}"),
        }
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

#[derive(Clone, Debug, Default)]
pub enum ExprKind {
    /// An expression with invalid syntax that can't be parsed.
    #[default]
    Err,
    Ident(SymbolId),
    IndexedIdentifier(IndexedIdent),
    UnaryOp(UnaryOpExpr),
    BinaryOp(BinaryOpExpr),
    Lit(LiteralKind),
    FunctionCall(FunctionCall),
    Cast(Cast),
    IndexExpr(IndexExpr),
    Paren(Expr),
    Measure(MeasureExpr),
}

impl Display for ExprKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ExprKind::Err => write!(f, "Err"),
            ExprKind::Ident(id) => write!(f, "SymbolId({id})"),
            ExprKind::IndexedIdentifier(id) => write!(f, "{id}"),
            ExprKind::UnaryOp(expr) => write!(f, "{expr}"),
            ExprKind::BinaryOp(expr) => write!(f, "{expr}"),
            ExprKind::Lit(lit) => write!(f, "Lit: {lit}"),
            ExprKind::FunctionCall(call) => write!(f, "{call}"),
            ExprKind::Cast(expr) => write!(f, "{expr}"),
            ExprKind::IndexExpr(expr) => write!(f, "{expr}"),
            ExprKind::Paren(expr) => write!(f, "Paren {expr}"),
            ExprKind::Measure(expr) => write!(f, "{expr}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AssignStmt {
    pub span: Span,
    pub symbol_id: SymbolId,
    pub lhs_span: Span,
    pub rhs: Expr,
}

impl Display for AssignStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "AssignStmt", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_field(f, "lhs_span", &self.lhs_span)?;
        write_field(f, "rhs", &self.rhs)
    }
}

#[derive(Clone, Debug)]
pub struct IndexedAssignStmt {
    pub span: Span,
    pub symbol_id: SymbolId,
    pub name_span: Span,
    pub indices: List<IndexElement>,
    pub rhs: Expr,
}

impl Display for IndexedAssignStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "AssignStmt", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_field(f, "name_span", &self.name_span)?;
        writeln_list_field(f, "indices", &self.indices)?;
        write_field(f, "rhs", &self.rhs)
    }
}

#[derive(Clone, Debug)]
pub struct AssignOpStmt {
    pub span: Span,
    pub symbol_id: SymbolId,
    pub indices: List<IndexElement>,
    pub op: BinOp,
    pub lhs: Expr,
    pub rhs: Expr,
}

impl Display for AssignOpStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "AssignOpStmt", self.span)?;
        writeln_field(f, "symbol_id", &self.symbol_id)?;
        writeln_list_field(f, "indices", &self.indices)?;
        writeln_field(f, "op", &self.op)?;
        writeln_field(f, "lhs", &self.rhs)?;
        write_field(f, "rhs", &self.rhs)
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
pub struct Cast {
    pub span: Span,
    pub ty: crate::semantic::types::Type,
    pub expr: Expr,
}

impl Display for Cast {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "Cast", self.span)?;
        writeln_field(f, "ty", &self.ty)?;
        write_field(f, "expr", &self.expr)
    }
}

#[derive(Clone, Debug)]
pub struct IndexExpr {
    pub span: Span,
    pub collection: Expr,
    pub index: IndexElement,
}

impl Display for IndexExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IndexExpr", self.span)?;
        writeln_field(f, "collection", &self.collection)?;
        write_field(f, "index", &self.index)
    }
}

#[derive(Clone, Debug)]
pub enum LiteralKind {
    Angle(Angle),
    Array(List<Expr>),
    Bitstring(BigInt, u32),
    Bool(bool),
    Duration(f64, TimeUnit),
    Float(f64),
    Complex(f64, f64),
    Int(i64),
    BigInt(BigInt),
    String(Rc<str>),
    Bit(bool),
}

impl Display for LiteralKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LiteralKind::Array(exprs) => write_list_field(f, "Array", exprs),
            LiteralKind::Bitstring(value, width) => {
                let width = *width as usize;
                write!(f, "Bitstring(\"{:0>width$}\")", value.to_str_radix(2))
            }
            LiteralKind::Angle(a) => write!(f, "Angle({a})"),
            LiteralKind::Bit(b) => write!(f, "Bit({:?})", u8::from(*b)),
            LiteralKind::Bool(b) => write!(f, "Bool({b:?})"),
            LiteralKind::Complex(real, imag) => write!(f, "Complex({real:?}, {imag:?})"),
            LiteralKind::Duration(value, unit) => {
                write!(f, "Duration({value:?}, {unit:?})")
            }
            LiteralKind::Float(value) => write!(f, "Float({value:?})"),
            LiteralKind::Int(i) => write!(f, "Int({i:?})"),
            LiteralKind::BigInt(i) => write!(f, "BigInt({i:?})"),
            LiteralKind::String(s) => write!(f, "String({s:?})"),
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
pub enum IndexElement {
    DiscreteSet(DiscreteSet),
    IndexSet(IndexSet),
}

impl IndexElement {
    pub fn span(&self) -> Span {
        match self {
            IndexElement::DiscreteSet(discrete_set) => discrete_set.span,
            IndexElement::IndexSet(index_set) => index_set.span,
        }
    }
}

impl Display for IndexElement {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            IndexElement::DiscreteSet(set) => write!(f, "{set}"),
            IndexElement::IndexSet(set) => write!(f, "{set}"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum IndexSetItem {
    RangeDefinition(RangeDefinition),
    Expr(Expr),
    Err,
}

impl Display for IndexSetItem {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            IndexSetItem::RangeDefinition(range) => write!(f, "{range}"),
            IndexSetItem::Expr(expr) => write!(f, "{expr}"),
            IndexSetItem::Err => write!(f, "Err"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum TimeUnit {
    Dt,
    /// Nanoseconds.
    Ns,
    /// Microseconds.
    Us,
    /// Milliseconds.
    Ms,
    /// Seconds.
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

#[derive(Clone, Debug)]
pub struct EndStmt {
    pub span: Span,
}

impl Display for EndStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "End {}", self.span)
    }
}
