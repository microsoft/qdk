// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::display_utils::{
    write_field, write_header, write_indented_list, write_list_field, write_opt_field,
    write_opt_list_field, writeln_field, writeln_header, writeln_list_field, writeln_opt_field,
};

use num_bigint::BigInt;
use qsc_data_structures::span::{Span, WithSpan};
use std::{
    fmt::{self, Display, Formatter},
    hash::Hash,
    sync::Arc,
};

use super::prim::SeqItem;

/// An alternative to `Vec<T>` that uses less stack space.
pub(crate) type List<T> = Box<[Box<T>]>;

pub(crate) fn list_from_iter<T>(vals: impl IntoIterator<Item = T>) -> List<T> {
    vals.into_iter().map(Box::new).collect()
}

#[derive(Clone, Debug)]
pub struct Program {
    pub span: Span,
    pub statements: List<Stmt>,
    pub version: Option<Version>,
}

impl Display for Program {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "Program", self.span)?;
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

/// A path that may or may not have been successfully parsed.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum PathKind {
    /// A successfully parsed path.
    Ok(Box<Path>),
    /// An invalid path.
    Err(Option<Box<IncompletePath>>),
}

impl Default for PathKind {
    fn default() -> Self {
        PathKind::Err(None)
    }
}

impl Display for PathKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PathKind::Ok(path) => write!(f, "{path}"),
            PathKind::Err(Some(incomplete_path)) => {
                write!(f, "Err IncompletePath {}:", incomplete_path.span)?;
                write_list_field(f, "segments", &incomplete_path.segments)
            }
            PathKind::Err(None) => write!(f, "Err",),
        }
    }
}

impl PathKind {
    /// Returns the span of the path, if it exists.
    #[must_use]
    pub fn span(&self) -> Option<Span> {
        match self {
            PathKind::Ok(path) => Some(path.span),
            PathKind::Err(Some(incomplete_path)) => Some(incomplete_path.span),
            PathKind::Err(None) => None,
        }
    }

    #[must_use]
    pub fn segments(&self) -> Option<Vec<Ident>> {
        match self {
            PathKind::Ok(path) => path.segments.as_ref().map(|s| s.to_vec()),
            PathKind::Err(incomplete_path) => {
                incomplete_path.as_ref().map(|path| path.segments.to_vec())
            }
        }
    }

    pub fn offset(&mut self, offset: u32) {
        match self {
            PathKind::Ok(path) => path.offset(offset),
            PathKind::Err(Some(incomplete_path)) => incomplete_path.offset(offset),
            PathKind::Err(None) => {}
        }
    }

    #[must_use]
    pub fn as_string(&self) -> String {
        match self {
            PathKind::Ok(path) => match &path.segments {
                Some(segments) => {
                    if segments.is_empty() {
                        return path.name.name.to_string();
                    }
                    let mut value = String::new();
                    for segment in segments {
                        if !value.is_empty() {
                            value.push('.');
                        }
                        value.push_str(&segment.name);
                    }
                    value.push('.');
                    value.push_str(&path.name.name);
                    value
                }
                None => path.name.name.to_string(),
            },
            PathKind::Err(Some(path)) => {
                let mut value = String::new();
                for segment in &path.segments {
                    if !value.is_empty() {
                        value.push('.');
                    }
                    value.push_str(&segment.name);
                }
                value
            }
            PathKind::Err(None) => "Err".to_string(),
        }
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

impl IncompletePath {
    pub fn offset(&mut self, offset: u32) {
        self.span.lo += offset;
        self.span.hi += offset;
        for segment in &mut self.segments {
            segment.span.lo += offset;
            segment.span.hi += offset;
        }
    }
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

impl WithSpan for Path {
    fn with_span(self, span: Span) -> Self {
        Self { span, ..self }
    }
}

impl Path {
    pub fn offset(&mut self, offset: u32) {
        self.span.lo += offset;
        self.span.hi += offset;
        for segment in self.segments.iter_mut().flatten() {
            segment.span.lo += offset;
            segment.span.hi += offset;
        }
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
        write_field(f, "operand", &self.operand)
    }
}

/// This expression is not part of the expression tree
/// and is only used as rhs of alias, classical declaration,
/// and assignment statements.
/// Grammar: `expression (DOUBLE_PLUS expression)*`.
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

/// A binary operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

impl WithSpan for GateOperand {
    fn with_span(self, span: Span) -> Self {
        Self {
            span,
            kind: self.kind.with_span(span),
        }
    }
}

impl Display for GateOperand {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "GateOperand", self.span)?;
        write_field(f, "kind", &self.kind)
    }
}

#[derive(Clone, Debug, Default)]
pub enum GateOperandKind {
    IdentOrIndexedIdent(Box<IdentOrIndexedIdent>),
    HardwareQubit(Box<HardwareQubit>),
    #[default]
    Err,
}

impl Display for GateOperandKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::IdentOrIndexedIdent(ident) => write!(f, "{ident}"),
            Self::HardwareQubit(qubit) => write!(f, "{qubit}"),
            Self::Err => write!(f, "Error"),
        }
    }
}

impl WithSpan for GateOperandKind {
    fn with_span(self, span: Span) -> Self {
        match self {
            Self::IdentOrIndexedIdent(ident) => Self::IdentOrIndexedIdent(ident.with_span(span)),
            Self::HardwareQubit(qubit) => Self::HardwareQubit(qubit.with_span(span)),
            Self::Err => Self::Err,
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

impl WithSpan for HardwareQubit {
    fn with_span(self, span: Span) -> Self {
        Self { span, ..self }
    }
}

#[derive(Clone, Debug)]
pub struct AliasDeclStmt {
    pub span: Span,
    pub ident: Box<IdentOrIndexedIdent>,
    pub exprs: List<Expr>,
}

impl Display for AliasDeclStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "AliasDeclStmt", self.span)?;
        writeln_field(f, "ident", &self.ident)?;
        write_list_field(f, "exprs", &self.exprs)
    }
}

/// A statement kind.
#[derive(Clone, Debug, Default)]
pub enum StmtKind {
    Alias(AliasDeclStmt),
    Assign(AssignStmt),
    AssignOp(AssignOpStmt),
    Barrier(BarrierStmt),
    Box(BoxStmt),
    Break(BreakStmt),
    Block(Block),
    Cal(CalibrationStmt),
    CalibrationGrammar(CalibrationGrammarStmt),
    ClassicalDecl(ClassicalDeclarationStmt),
    ConstDecl(ConstantDeclStmt),
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
    GPhase(GPhase),
    Include(IncludeStmt),
    IODeclaration(IODeclaration),
    Measure(MeasureArrowStmt),
    Pragma(Pragma),
    QuantumGateDefinition(QuantumGateDefinition),
    QuantumDecl(QubitDeclaration),
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
            StmtKind::Assign(stmt) => write!(f, "{stmt}"),
            StmtKind::AssignOp(stmt) => write!(f, "{stmt}"),
            StmtKind::Alias(alias) => write!(f, "{alias}"),
            StmtKind::Barrier(barrier) => write!(f, "{barrier}"),
            StmtKind::Box(box_stmt) => write!(f, "{box_stmt}"),
            StmtKind::Break(break_stmt) => write!(f, "{break_stmt}"),
            StmtKind::Block(block) => write!(f, "{block}"),
            StmtKind::Cal(cal) => write!(f, "{cal}"),
            StmtKind::CalibrationGrammar(grammar) => write!(f, "{grammar}"),
            StmtKind::ClassicalDecl(decl) => write!(f, "{decl}"),
            StmtKind::ConstDecl(decl) => write!(f, "{decl}"),
            StmtKind::Continue(continue_stmt) => write!(f, "{continue_stmt}"),
            StmtKind::Def(def) => write!(f, "{def}"),
            StmtKind::DefCal(defcal) => write!(f, "{defcal}"),
            StmtKind::Delay(delay) => write!(f, "{delay}"),
            StmtKind::End(end_stmt) => write!(f, "{end_stmt}"),
            StmtKind::ExprStmt(expr) => write!(f, "{expr}"),
            StmtKind::ExternDecl(decl) => write!(f, "{decl}"),
            StmtKind::For(for_stmt) => write!(f, "{for_stmt}"),
            StmtKind::GateCall(gate_call) => write!(f, "{gate_call}"),
            StmtKind::GPhase(gphase) => write!(f, "{gphase}"),
            StmtKind::If(if_stmt) => write!(f, "{if_stmt}"),
            StmtKind::Include(include) => write!(f, "{include}"),
            StmtKind::IODeclaration(io) => write!(f, "{io}"),
            StmtKind::Measure(measure) => write!(f, "{measure}"),
            StmtKind::Pragma(pragma) => write!(f, "{pragma}"),
            StmtKind::QuantumGateDefinition(gate) => write!(f, "{gate}"),
            StmtKind::QuantumDecl(decl) => write!(f, "{decl}"),
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
    pub name: Arc<str>,
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
    pub content: Arc<str>,
}

impl Display for DefCalStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DefCalStmt", self.span)?;
        write_field(f, "content", &self.content)
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

#[derive(Clone, Debug)]
pub struct DurationofCall {
    pub span: Span,
    pub name_span: Span,
    pub scope: Block,
}

impl Display for DurationofCall {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DurationofCall", self.span)?;
        writeln_field(f, "name_span", &self.name_span)?;
        write_field(f, "scope", &self.scope)
    }
}

#[derive(Clone, Debug)]
pub enum IdentOrIndexedIdent {
    Ident(Ident),
    IndexedIdent(IndexedIdent),
}

impl IdentOrIndexedIdent {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            IdentOrIndexedIdent::Ident(ident) => ident.span,
            IdentOrIndexedIdent::IndexedIdent(ident) => ident.span,
        }
    }
}

impl Display for IdentOrIndexedIdent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            IdentOrIndexedIdent::Ident(ident) => write!(f, "{ident}"),
            IdentOrIndexedIdent::IndexedIdent(ident) => write!(f, "{ident}"),
        }
    }
}

impl WithSpan for IdentOrIndexedIdent {
    fn with_span(self, span: Span) -> Self {
        match self {
            Self::Ident(ident) => Self::Ident(ident.with_span(span)),
            Self::IndexedIdent(ident) => Self::IndexedIdent(ident.with_span(span)),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Ident {
    pub span: Span,
    pub name: Arc<str>,
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

impl WithSpan for Ident {
    fn with_span(self, span: Span) -> Self {
        Self { span, ..self }
    }
}

#[derive(Clone, Debug)]
pub struct IndexedIdent {
    pub span: Span,
    pub index_span: Span,
    pub ident: Ident,
    pub indices: List<Index>,
}

impl Display for IndexedIdent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IndexedIdent", self.span)?;
        writeln_field(f, "ident", &self.ident)?;
        writeln_field(f, "index_span", &self.index_span)?;
        write_list_field(f, "indices", &self.indices)
    }
}

impl WithSpan for IndexedIdent {
    fn with_span(self, span: Span) -> Self {
        Self { span, ..self }
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
}

impl WithSpan for Expr {
    fn with_span(self, span: Span) -> Self {
        Self { span, ..self }
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Expr {}: {}", self.span, self.kind)
    }
}

#[derive(Clone, Debug)]
pub enum Index {
    /// Only allowed in registers, and only in alias statements.
    /// `alias b = a[{1, 5, 8}];`
    IndexSet(Set),
    /// A list of indices, used for multidimensional indexing.
    /// `int b = a[2, 3:7];`
    IndexList(IndexList),
}

impl Display for Index {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Index::IndexSet(set) => write!(f, "{set}"),
            Index::IndexList(list) => write!(f, "{list}"),
        }
    }
}

impl Index {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Index::IndexSet(set) => set.span,
            Index::IndexList(set) => set.span,
        }
    }

    #[must_use]
    pub fn num_indices(&self) -> usize {
        match self {
            // According to the spec Index sets count as a single index.
            Index::IndexSet(_) => 1,
            Index::IndexList(index_list) => index_list.values.len(),
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
pub struct IndexList {
    pub span: Span,
    pub values: List<IndexListItem>,
}

impl Display for IndexList {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IndexList", self.span)?;
        write_list_field(f, "values", &self.values)
    }
}

#[derive(Clone, Debug, Default)]
pub enum IndexListItem {
    RangeDefinition(Range),
    Expr(Expr),
    #[default]
    Err,
}

/// This is needed to able to use `IndexSetItem` in the `seq` combinator.
impl WithSpan for IndexListItem {
    fn with_span(self, span: Span) -> Self {
        match self {
            IndexListItem::RangeDefinition(range) => {
                IndexListItem::RangeDefinition(range.with_span(span))
            }
            IndexListItem::Expr(expr) => IndexListItem::Expr(expr.with_span(span)),
            IndexListItem::Err => IndexListItem::Err,
        }
    }
}

impl Display for IndexListItem {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            IndexListItem::RangeDefinition(range) => write!(f, "{range}"),
            IndexListItem::Expr(expr) => write!(f, "{expr}"),
            IndexListItem::Err => write!(f, "Err"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Range {
    pub span: Span,
    pub start: Option<Expr>,
    pub end: Option<Expr>,
    pub step: Option<Expr>,
}

impl WithSpan for Range {
    fn with_span(self, span: Span) -> Self {
        Self { span, ..self }
    }
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
    Ctrl(Option<Expr>),
    NegCtrl(Option<Expr>),
}

impl Display for GateModifierKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            GateModifierKind::Inv => write!(f, "Inv"),
            GateModifierKind::Pow(expr) => write!(f, "Pow {expr}"),
            GateModifierKind::Ctrl(expr) => write!(f, "Ctrl {expr:?}"),
            GateModifierKind::NegCtrl(expr) => write!(f, "NegCtrl {expr:?}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClassicalArgument {
    pub span: Span,
    pub ty: ScalarType,
    pub name: Box<IdentOrIndexedIdent>,
    pub access: Option<AccessControl>,
}

impl Display for ClassicalArgument {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(access) = &self.access {
            write!(
                f,
                "ClassicalArgument {}: {}, {}, {}",
                self.span, self.ty, self.name, access
            )
        } else {
            write!(
                f,
                "ClassicalArgument {}: {}, {}",
                self.span, self.ty, self.name
            )
        }
    }
}

#[derive(Clone, Debug)]
pub enum ExternParameter {
    ArrayReference(ArrayReferenceType, Span),
    Scalar(ScalarType, Span),
}

impl ExternParameter {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            ExternParameter::ArrayReference(_, span) | ExternParameter::Scalar(_, span) => *span,
        }
    }
}

impl Display for ExternParameter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ExternParameter::Scalar(ty, span) => {
                write!(f, "{span}: {ty}")
            }
            ExternParameter::ArrayReference(ty, span) => {
                write!(f, "{span}: {ty}")
            }
        }
    }
}

impl Default for ExternParameter {
    fn default() -> Self {
        ExternParameter::Scalar(ScalarType::default(), Span::default())
    }
}

impl WithSpan for ExternParameter {
    fn with_span(self, span: Span) -> Self {
        match self {
            ExternParameter::Scalar(ty, _) => ExternParameter::Scalar(ty, span),
            ExternParameter::ArrayReference(ty, _) => ExternParameter::ArrayReference(ty, span),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScalarType {
    pub span: Span,
    pub kind: ScalarTypeKind,
}

impl Display for ScalarType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ScalarType {}: {}", self.span, self.kind)
    }
}

#[derive(Clone, Debug, Default)]
pub enum ScalarTypeKind {
    Bit(BitType),
    Int(IntType),
    UInt(UIntType),
    Float(FloatType),
    Complex(ComplexType),
    Angle(AngleType),
    BoolType,
    Duration,
    Stretch,
    // Any usage of Err should have pushed a parse error
    #[default]
    Err,
}

impl Display for ScalarTypeKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ScalarTypeKind::Int(int) => write!(f, "{int}"),
            ScalarTypeKind::UInt(uint) => write!(f, "{uint}"),
            ScalarTypeKind::Float(float) => write!(f, "{float}"),
            ScalarTypeKind::Complex(complex) => write!(f, "{complex}"),
            ScalarTypeKind::Angle(angle) => write!(f, "{angle}"),
            ScalarTypeKind::Bit(bit) => write!(f, "{bit}"),
            ScalarTypeKind::BoolType => write!(f, "BoolType"),
            ScalarTypeKind::Duration => write!(f, "Duration"),
            ScalarTypeKind::Stretch => write!(f, "Stretch"),
            ScalarTypeKind::Err => write!(f, "Err"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ArrayBaseTypeKind {
    Int(IntType),
    UInt(UIntType),
    Float(FloatType),
    Complex(ComplexType),
    Angle(AngleType),
    BoolType,
    Duration,
}

impl Display for ArrayBaseTypeKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ArrayBaseTypeKind::Int(int) => write!(f, "ArrayBaseTypeKind {int}"),
            ArrayBaseTypeKind::UInt(uint) => write!(f, "ArrayBaseTypeKind {uint}"),
            ArrayBaseTypeKind::Float(float) => write!(f, "ArrayBaseTypeKind {float}"),
            ArrayBaseTypeKind::Complex(complex) => write!(f, "ArrayBaseTypeKind {complex}"),
            ArrayBaseTypeKind::Angle(angle) => write!(f, "ArrayBaseTypeKind {angle}"),
            ArrayBaseTypeKind::Duration => write!(f, "ArrayBaseTypeKind DurationType"),
            ArrayBaseTypeKind::BoolType => write!(f, "ArrayBaseTypeKind BoolType"),
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
pub enum TypeDef {
    Scalar(ScalarType),
    Array(ArrayType),
    ArrayReference(ArrayReferenceType),
}

impl TypeDef {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            TypeDef::Scalar(ident) => ident.span,
            TypeDef::Array(array) => array.span,
            TypeDef::ArrayReference(array) => array.span(),
        }
    }
}

impl Display for TypeDef {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            TypeDef::Scalar(scalar) => write!(f, "{scalar}"),
            TypeDef::Array(array) => write!(f, "{array}"),
            TypeDef::ArrayReference(array) => write!(f, "{array}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ArrayType {
    pub span: Span,
    pub base_type: ArrayBaseTypeKind,
    pub dimensions: List<Expr>,
}

impl Display for ArrayType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ArrayType", self.span)?;
        writeln_field(f, "base_type", &self.base_type)?;
        write_list_field(f, "dimensions", &self.dimensions)
    }
}

#[derive(Clone, Debug)]
pub enum ArrayReferenceType {
    Static(StaticArrayReferenceType),
    Dyn(DynArrayReferenceType),
}

impl ArrayReferenceType {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            ArrayReferenceType::Static(ty) => ty.span,
            ArrayReferenceType::Dyn(ty) => ty.span,
        }
    }
}

impl Display for ArrayReferenceType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ArrayReferenceType::Static(ty) => write!(f, "{ty}"),
            ArrayReferenceType::Dyn(ty) => write!(f, "{ty}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StaticArrayReferenceType {
    pub span: Span,
    pub mutability: AccessControl,
    pub base_type: ArrayBaseTypeKind,
    pub dimensions: List<Expr>,
}

impl Display for StaticArrayReferenceType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "StaticArrayReferenceType", self.span)?;
        writeln_field(f, "mutability", &self.mutability)?;
        writeln_field(f, "base_type", &self.base_type)?;
        write_list_field(f, "dimensions", &self.dimensions)
    }
}

#[derive(Clone, Debug)]
pub struct DynArrayReferenceType {
    pub span: Span,
    pub mutability: AccessControl,
    pub base_type: ArrayBaseTypeKind,
    pub dimensions: Expr,
}

impl Display for DynArrayReferenceType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DynArrayReferenceType", self.span)?;
        writeln_field(f, "mutability", &self.mutability)?;
        writeln_field(f, "base_type", &self.base_type)?;
        write_field(f, "dimensions", &self.dimensions)
    }
}

#[derive(Clone, Debug)]
pub enum AccessControl {
    ReadOnly,
    Mutable,
}

impl Display for AccessControl {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            AccessControl::ReadOnly => write!(f, "ReadOnly"),
            AccessControl::Mutable => write!(f, "Mutable"),
        }
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
pub struct QubitDeclaration {
    pub span: Span,
    pub ty: QubitType,
    pub qubit: Ident,
}

impl Display for QubitDeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "QubitDeclaration", self.span)?;
        writeln_field(f, "ty", &self.ty)?;
        write_field(f, "ident", &self.qubit)
    }
}

#[derive(Clone, Debug)]
pub struct QuantumGateDefinition {
    pub span: Span,
    pub ident: Box<Ident>,
    pub params: List<SeqItem<Ident>>,
    pub qubits: List<SeqItem<Ident>>,
    pub body: Box<Block>,
}

impl Display for QuantumGateDefinition {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "Gate", self.span)?;
        writeln_field(f, "ident", &self.ident)?;
        writeln_list_field(f, "parameters", &self.params)?;
        writeln_list_field(f, "qubits", &self.qubits)?;
        write_field(f, "body", &self.body)
    }
}

#[derive(Clone, Debug)]
pub struct ExternDecl {
    pub span: Span,
    pub ident: Box<Ident>,
    pub params: List<ExternParameter>,
    pub return_type: Option<ScalarType>,
}

impl Display for ExternDecl {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ExternDecl", self.span)?;
        writeln_field(f, "ident", &self.ident)?;
        writeln_list_field(f, "parameters", &self.params)?;
        write_opt_field(f, "return_type", self.return_type.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct GateCall {
    pub span: Span,
    pub modifiers: List<QuantumGateModifier>,
    pub name: Ident,
    pub args: List<Expr>,
    pub qubits: List<GateOperand>,
    pub duration: Option<Expr>,
}

impl Display for GateCall {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "GateCall", self.span)?;
        writeln_list_field(f, "modifiers", &self.modifiers)?;
        writeln_field(f, "name", &self.name)?;
        writeln_list_field(f, "args", &self.args)?;
        writeln_opt_field(f, "duration", self.duration.as_ref())?;
        write_list_field(f, "qubits", &self.qubits)
    }
}

#[derive(Clone, Debug)]
pub struct GPhase {
    pub span: Span,
    pub gphase_token_span: Span,
    pub modifiers: List<QuantumGateModifier>,
    pub args: List<Expr>,
    pub qubits: List<GateOperand>,
    pub duration: Option<Expr>,
}

impl Display for GPhase {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "GPhase", self.span)?;
        writeln_field(f, "gphase_token_span", &self.gphase_token_span)?;
        writeln_list_field(f, "modifiers", &self.modifiers)?;
        writeln_list_field(f, "args", &self.args)?;
        writeln_opt_field(f, "duration", self.duration.as_ref())?;
        write_list_field(f, "qubits", &self.qubits)
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
    pub target: Option<Box<IdentOrIndexedIdent>>,
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
    pub ty: Box<TypeDef>,
    pub identifier: Ident,
    pub init_expr: Option<Box<ValueExpr>>,
}

impl Display for ClassicalDeclarationStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ClassicalDeclarationStmt", self.span)?;
        writeln_field(f, "type", &self.ty)?;
        writeln_field(f, "ident", &self.identifier)?;
        write_opt_field(f, "init_expr", self.init_expr.as_ref())
    }
}

/// A special kind of Expr that allows measurement and concatenation expressions.
/// It is used as the rhs of alias, classical declaration, and assign statements.
#[derive(Clone, Debug)]
pub enum ValueExpr {
    Concat(ConcatExpr),
    Expr(Expr),
    Measurement(MeasureExpr),
}

impl Display for ValueExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Concat(expr) => write!(f, "{expr}"),
            Self::Expr(expr) => write!(f, "{expr}"),
            Self::Measurement(measure) => write!(f, "{measure}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct IODeclaration {
    pub span: Span,
    pub io_identifier: IOKeyword,
    pub ty: TypeDef,
    pub ident: Box<Ident>,
}

impl Display for IODeclaration {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IODeclaration", self.span)?;
        writeln_field(f, "io_keyword", &self.io_identifier)?;
        writeln_field(f, "type", &self.ty)?;
        write_field(f, "ident", &self.ident)
    }
}

#[derive(Clone, Debug)]
pub struct ConstantDeclStmt {
    pub span: Span,
    pub ty: TypeDef,
    pub identifier: Box<Ident>,
    pub init_expr: ValueExpr,
}

impl Display for ConstantDeclStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ConstantDeclStmt", self.span)?;
        writeln_field(f, "type", &self.ty)?;
        writeln_field(f, "ident", &self.identifier)?;
        write_field(f, "init_expr", &self.init_expr)
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

#[derive(Clone, Debug, Default)]
pub struct DefParameter {
    pub span: Span,
    pub ident: Ident,
    pub ty: Box<DefParameterType>,
}

impl Display for DefParameter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DefParameter", self.span)?;
        writeln_field(f, "ident", &self.ident)?;
        write_field(f, "type", &self.ty)
    }
}

impl WithSpan for DefParameter {
    fn with_span(self, span: Span) -> Self {
        Self {
            span,
            ident: self.ident,
            ty: self.ty,
        }
    }
}

#[derive(Clone, Debug)]
pub enum DefParameterType {
    ArrayReference(ArrayReferenceType),
    Qubit(QubitType),
    Scalar(ScalarType),
}

impl DefParameterType {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            DefParameterType::ArrayReference(ty) => ty.span(),
            DefParameterType::Qubit(ty) => ty.span,
            DefParameterType::Scalar(ty) => ty.span,
        }
    }
}

impl Display for DefParameterType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DefParameterType::ArrayReference(ty) => write!(f, "{ty}"),
            DefParameterType::Qubit(ty) => write!(f, "{ty}"),
            DefParameterType::Scalar(ty) => write!(f, "{ty}"),
        }
    }
}

impl Default for DefParameterType {
    fn default() -> Self {
        Self::Scalar(Default::default())
    }
}

#[derive(Debug, Clone)]
pub struct QubitType {
    pub span: Span,
    pub size: Option<Expr>,
}

impl Display for QubitType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "QubitType", self.span)?;
        write_opt_field(f, "size", self.size.as_ref())
    }
}

#[derive(Clone, Debug)]
pub struct DefStmt {
    pub span: Span,
    pub name: Box<Ident>,
    pub params: List<DefParameter>,
    pub body: Block,
    pub return_type: Option<Box<ScalarType>>,
}

impl Display for DefStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "DefStmt", self.span)?;
        writeln_field(f, "ident", &self.name)?;
        writeln_list_field(f, "params", &self.params)?;
        writeln_opt_field(f, "return_type", self.return_type.as_ref())?;
        write_field(f, "body", &self.body)
    }
}

#[derive(Clone, Debug)]
pub struct ReturnStmt {
    pub span: Span,
    pub expr: Option<Box<ValueExpr>>,
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
    pub while_condition: Expr,
    pub body: Stmt,
}

impl Display for WhileLoop {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "WhileLoop", self.span)?;
        writeln_field(f, "condition", &self.while_condition)?;
        write_field(f, "body", &self.body)
    }
}

#[derive(Clone, Debug)]
pub struct ForStmt {
    pub span: Span,
    pub ty: ScalarType,
    pub ident: Ident,
    pub set_declaration: Box<EnumerableSet>,
    pub body: Stmt,
}

impl Display for ForStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "ForStmt", self.span)?;
        writeln_field(f, "variable_type", &self.ty)?;
        writeln_field(f, "variable_name", &self.ident)?;
        writeln_field(f, "iterable", &self.set_declaration)?;
        write_field(f, "body", &self.body)
    }
}

#[derive(Clone, Debug)]
pub enum EnumerableSet {
    Set(Set),
    Range(Range),
    Expr(Expr),
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
    Ident(Ident),
    UnaryOp(UnaryOpExpr),
    BinaryOp(BinaryOpExpr),
    Lit(Lit),
    FunctionCall(FunctionCall),
    Cast(Cast),
    IndexExpr(IndexExpr),
    Paren(Expr),
    DurationOf(DurationofCall),
}

impl Display for ExprKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ExprKind::Err => write!(f, "Err"),
            ExprKind::Ident(id) => write!(f, "{id}"),
            ExprKind::UnaryOp(expr) => write!(f, "{expr}"),
            ExprKind::BinaryOp(expr) => write!(f, "{expr}"),
            ExprKind::Lit(lit) => write!(f, "{lit}"),
            ExprKind::FunctionCall(call) => write!(f, "{call}"),
            ExprKind::Cast(expr) => write!(f, "{expr}"),
            ExprKind::IndexExpr(expr) => write!(f, "{expr}"),
            ExprKind::Paren(expr) => write!(f, "Paren {expr}"),
            ExprKind::DurationOf(duration) => write!(f, "{duration}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AssignStmt {
    pub span: Span,
    pub lhs: Box<IdentOrIndexedIdent>,
    pub rhs: ValueExpr,
}

impl Display for AssignStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "AssignStmt", self.span)?;
        writeln_field(f, "lhs", &self.lhs)?;
        write_field(f, "rhs", &self.rhs)
    }
}

#[derive(Clone, Debug)]
pub struct AssignOpStmt {
    pub span: Span,
    pub op: BinOp,
    pub lhs: Box<IdentOrIndexedIdent>,
    pub rhs: ValueExpr,
}

impl Display for AssignOpStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "AssignOpStmt", self.span)?;
        writeln_field(f, "op", &self.op)?;
        writeln_field(f, "lhs", &self.lhs)?;
        write_field(f, "rhs", &self.rhs)
    }
}

#[derive(Clone, Debug)]
pub struct UnaryOpExpr {
    pub op: UnaryOp,
    pub expr: Expr,
}

impl Display for UnaryOpExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "UnaryOpExpr:")?;
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
    pub name: Ident,
    pub args: List<Expr>,
}

impl Display for FunctionCall {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "FunctionCall", self.span)?;
        writeln_field(f, "name", &self.name)?;
        write_list_field(f, "args", &self.args)
    }
}

#[derive(Clone, Debug)]
pub struct Cast {
    pub span: Span,
    pub ty: TypeDef,
    pub arg: Expr,
}

impl Display for Cast {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "Cast", self.span)?;
        writeln_field(f, "type", &self.ty)?;
        write_field(f, "arg", &self.arg)
    }
}

#[derive(Clone, Debug)]
pub struct IndexExpr {
    pub span: Span,
    pub collection: Expr,
    pub index: Index,
}

impl Display for IndexExpr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header(f, "IndexExpr", self.span)?;
        writeln_field(f, "collection", &self.collection)?;
        write_field(f, "index", &self.index)
    }
}

#[derive(Clone, Debug)]
pub struct Lit {
    pub span: Span,
    pub kind: LiteralKind,
}

impl Display for Lit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "Lit: {}", self.kind)
    }
}

#[derive(Clone, Debug)]
pub enum LiteralKind {
    Array(List<Expr>),
    Bitstring(BigInt, u32),
    Bool(bool),
    Duration(f64, TimeUnit),
    Float(f64),
    Imaginary(f64),
    Int(i64),
    BigInt(BigInt),
    String(Arc<str>),
}

impl Display for LiteralKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LiteralKind::Array(exprs) => write_list_field(f, "Array", exprs),
            LiteralKind::Bitstring(value, width) => {
                let width = *width as usize;
                write!(f, "Bitstring(\"{:0>width$}\")", value.to_str_radix(2))
            }
            LiteralKind::Bool(b) => write!(f, "Bool({b:?})"),
            LiteralKind::Duration(value, unit) => {
                write!(f, "Duration({value:?}, {unit:?})")
            }
            LiteralKind::Float(value) => write!(f, "Float({value:?})"),
            LiteralKind::Imaginary(value) => write!(f, "Imaginary({value:?})"),
            LiteralKind::Int(i) => write!(f, "Int({i:?})"),
            LiteralKind::BigInt(i) => write!(f, "BigInt({i:?})"),
            LiteralKind::String(s) => write!(f, "String({s:?})"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Version {
    pub major: u32,
    pub minor: Option<u32>,
    pub span: Span,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.minor {
            Some(minor) => write!(f, "{}.{}", self.major, minor),
            None => write!(f, "{}", self.major),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IOKeyword {
    Input,
    Output,
}

impl Display for IOKeyword {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            IOKeyword::Input => write!(f, "input"),
            IOKeyword::Output => write!(f, "output"),
        }
    }
}

impl From<IOKeyword> for crate::semantic::symbols::IOKind {
    fn from(value: IOKeyword) -> Self {
        match value {
            IOKeyword::Input => crate::semantic::symbols::IOKind::Input,
            IOKeyword::Output => crate::semantic::symbols::IOKind::Output,
        }
    }
}

#[derive(Clone, Debug, Copy)]
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

#[derive(Clone, Debug)]
pub struct BreakStmt {
    pub span: Span,
}

impl Display for BreakStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "BreakStmt {}", self.span)
    }
}

#[derive(Clone, Debug)]
pub struct ContinueStmt {
    pub span: Span,
}

impl Display for ContinueStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "ContinueStmt {}", self.span)
    }
}

#[derive(Clone, Debug)]
pub struct EndStmt {
    pub span: Span,
}

impl Display for EndStmt {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "EndStmt {}", self.span)
    }
}
