// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Rich, typed Python projections of the syntactic `OpenQASM` AST.
//!
//! The syntactic tree is exposed as a backbone of three classes -- [`Program`],
//! [`Stmt`], and [`Expr`] -- plus a kind-specific subclass for every
//! [`StmtKind`]/[`ExprKind`] variant. Subclasses carry no extra Rust state; they
//! exist so that Python `isinstance` checks and `visit_<NodeType>` dispatch work
//! for every node. The backbone classes expose `span`, `kind`, `children()`, and
//! `accept(visitor)`, plus a small set of commonly useful typed accessors
//! (`name`, `op`, `value`, `annotations`).
//!
//! `children()` returns every child `Stmt`/`Expr` node reachable through the
//! intermediate (non-projected) AST wrapper types, which is exactly what a
//! visitor needs to traverse the whole tree.

use crate::span::Span;
use pyo3::IntoPyObjectExt;
use pyo3::prelude::*;
use pyo3::types::PyComplex;
use qdk_openqasm_parser::parser::ast as syntax;

/// A tag mirroring [`syntax::StmtKind`], exposed to Python as an integer-valued
/// enum.
#[pyclass(
    eq,
    eq_int,
    frozen,
    from_py_object,
    module = "qdk_openqasm_parser._native"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StmtKind {
    Alias,
    Assign,
    AssignOp,
    Barrier,
    Box,
    Break,
    Block,
    Cal,
    CalibrationGrammar,
    ClassicalDecl,
    ConstDecl,
    Continue,
    Def,
    DefCal,
    Delay,
    End,
    ExprStmt,
    ExternDecl,
    For,
    If,
    GateCall,
    GPhase,
    Include,
    IODeclaration,
    Measure,
    Pragma,
    QuantumGateDefinition,
    QuantumDecl,
    Reset,
    Return,
    Switch,
    WhileLoop,
    Err,
}

/// A tag mirroring [`syntax::ExprKind`], exposed to Python as an integer-valued
/// enum.
#[pyclass(
    eq,
    eq_int,
    frozen,
    from_py_object,
    module = "qdk_openqasm_parser._native"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Err,
    Ident,
    UnaryOp,
    BinaryOp,
    Lit,
    FunctionCall,
    Cast,
    IndexExpr,
    Paren,
    DurationOf,
}

/// The root of a syntactic `OpenQASM` program.
#[pyclass(module = "qdk_openqasm_parser._native", frozen)]
pub struct Program {
    inner: syntax::Program,
    #[pyo3(get)]
    span: Span,
    statements: Vec<Py<PyAny>>,
    version: Option<(u32, Option<u32>)>,
}

#[pymethods]
impl Program {
    /// The `(major, minor)` `OpenQASM` version declared by the program, if any.
    #[getter]
    fn version(&self) -> Option<(u32, Option<u32>)> {
        self.version
    }

    /// The top-level statements of the program.
    #[getter]
    fn statements(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        clone_nodes(py, &self.statements)
    }

    /// The child nodes of the program (its statements).
    fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        clone_nodes(py, &self.statements)
    }

    /// Dispatches this node to `visitor.visit(self)`.
    fn accept(slf: Bound<'_, Self>, visitor: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        Ok(visitor.call_method1("visit", (slf,))?.unbind())
    }

    fn __repr__(&self) -> String {
        format!("Program(statements=[{} items])", self.statements.len())
    }
}

/// The base class for every syntactic statement node.
#[pyclass(subclass, module = "qdk_openqasm_parser._native", frozen)]
pub struct Stmt {
    #[pyo3(get)]
    span: Span,
    #[pyo3(get)]
    kind: StmtKind,
    children: Vec<Py<PyAny>>,
    #[pyo3(get)]
    name: Option<String>,
    #[pyo3(get)]
    annotations: Vec<String>,
}

#[pymethods]
impl Stmt {
    /// The child nodes (`Stmt`/`Expr`) directly contained by this statement.
    fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        clone_nodes(py, &self.children)
    }

    /// Dispatches this node to `visitor.visit(self)`.
    fn accept(slf: Bound<'_, Self>, visitor: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        Ok(visitor.call_method1("visit", (slf,))?.unbind())
    }

    fn __repr__(&self) -> String {
        format!("{:?}({:?})", self.kind, self.span)
    }
}

/// The base class for every syntactic expression node.
#[pyclass(subclass, module = "qdk_openqasm_parser._native", frozen)]
pub struct Expr {
    #[pyo3(get)]
    span: Span,
    #[pyo3(get)]
    kind: ExprKind,
    children: Vec<Py<PyAny>>,
    #[pyo3(get)]
    name: Option<String>,
    #[pyo3(get)]
    op: Option<String>,
    #[pyo3(get)]
    value: Option<Py<PyAny>>,
}

#[pymethods]
impl Expr {
    /// The child expression nodes directly contained by this expression.
    fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        clone_nodes(py, &self.children)
    }

    /// Dispatches this node to `visitor.visit(self)`.
    fn accept(slf: Bound<'_, Self>, visitor: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        Ok(visitor.call_method1("visit", (slf,))?.unbind())
    }

    fn __repr__(&self) -> String {
        format!("{:?}({:?})", self.kind, self.span)
    }
}

fn clone_nodes(py: Python<'_>, nodes: &[Py<PyAny>]) -> Vec<Py<PyAny>> {
    nodes.iter().map(|node| node.clone_ref(py)).collect()
}

// ----------------------------------------------------------------------------
// Per-variant subclasses
// ----------------------------------------------------------------------------

macro_rules! stmt_subclass {
    ($name:ident) => {
        #[pyclass(extends = Stmt, module = "qdk_openqasm_parser._native", frozen)]
        pub struct $name;
    };
}

macro_rules! expr_subclass {
    ($name:ident) => {
        #[pyclass(extends = Expr, module = "qdk_openqasm_parser._native", frozen)]
        pub struct $name;
    };
}

stmt_subclass!(AliasStmt);
stmt_subclass!(AssignStmt);
stmt_subclass!(AssignOpStmt);
stmt_subclass!(BarrierStmt);
stmt_subclass!(BoxStmt);
stmt_subclass!(BreakStmt);
stmt_subclass!(BlockStmt);
stmt_subclass!(CalStmt);
stmt_subclass!(CalibrationGrammarStmt);
stmt_subclass!(ClassicalDeclStmt);
stmt_subclass!(ConstDeclStmt);
stmt_subclass!(ContinueStmt);
stmt_subclass!(DefStmt);
stmt_subclass!(DefCalStmt);
stmt_subclass!(DelayStmt);
stmt_subclass!(EndStmt);
stmt_subclass!(ExprStmt);
stmt_subclass!(ExternDeclStmt);
stmt_subclass!(ForStmt);
stmt_subclass!(IfStmt);
stmt_subclass!(GateCallStmt);
stmt_subclass!(GPhaseStmt);
stmt_subclass!(IncludeStmt);
stmt_subclass!(IODeclarationStmt);
stmt_subclass!(MeasureStmt);
stmt_subclass!(PragmaStmt);
stmt_subclass!(QuantumGateDefinitionStmt);
stmt_subclass!(QuantumDeclStmt);
stmt_subclass!(ResetStmt);
stmt_subclass!(ReturnStmt);
stmt_subclass!(SwitchStmt);
stmt_subclass!(WhileLoopStmt);
stmt_subclass!(ErrStmt);

expr_subclass!(ErrExpr);
expr_subclass!(IdentExpr);
expr_subclass!(UnaryOpExpr);
expr_subclass!(BinaryOpExpr);
expr_subclass!(LitExpr);
expr_subclass!(FunctionCallExpr);
expr_subclass!(CastExpr);
expr_subclass!(IndexExpr);
expr_subclass!(ParenExpr);
expr_subclass!(DurationOfExpr);

/// Registers all syntactic node classes with the native module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<StmtKind>()?;
    m.add_class::<ExprKind>()?;
    m.add_class::<Program>()?;
    m.add_class::<Stmt>()?;
    m.add_class::<Expr>()?;
    m.add_class::<AliasStmt>()?;
    m.add_class::<AssignStmt>()?;
    m.add_class::<AssignOpStmt>()?;
    m.add_class::<BarrierStmt>()?;
    m.add_class::<BoxStmt>()?;
    m.add_class::<BreakStmt>()?;
    m.add_class::<BlockStmt>()?;
    m.add_class::<CalStmt>()?;
    m.add_class::<CalibrationGrammarStmt>()?;
    m.add_class::<ClassicalDeclStmt>()?;
    m.add_class::<ConstDeclStmt>()?;
    m.add_class::<ContinueStmt>()?;
    m.add_class::<DefStmt>()?;
    m.add_class::<DefCalStmt>()?;
    m.add_class::<DelayStmt>()?;
    m.add_class::<EndStmt>()?;
    m.add_class::<ExprStmt>()?;
    m.add_class::<ExternDeclStmt>()?;
    m.add_class::<ForStmt>()?;
    m.add_class::<IfStmt>()?;
    m.add_class::<GateCallStmt>()?;
    m.add_class::<GPhaseStmt>()?;
    m.add_class::<IncludeStmt>()?;
    m.add_class::<IODeclarationStmt>()?;
    m.add_class::<MeasureStmt>()?;
    m.add_class::<PragmaStmt>()?;
    m.add_class::<QuantumGateDefinitionStmt>()?;
    m.add_class::<QuantumDeclStmt>()?;
    m.add_class::<ResetStmt>()?;
    m.add_class::<ReturnStmt>()?;
    m.add_class::<SwitchStmt>()?;
    m.add_class::<WhileLoopStmt>()?;
    m.add_class::<ErrStmt>()?;
    m.add_class::<ErrExpr>()?;
    m.add_class::<IdentExpr>()?;
    m.add_class::<UnaryOpExpr>()?;
    m.add_class::<BinaryOpExpr>()?;
    m.add_class::<LitExpr>()?;
    m.add_class::<FunctionCallExpr>()?;
    m.add_class::<CastExpr>()?;
    m.add_class::<IndexExpr>()?;
    m.add_class::<ParenExpr>()?;
    m.add_class::<DurationOfExpr>()?;
    Ok(())
}

// ----------------------------------------------------------------------------
// Conversion: Rust AST -> Python node tree
// ----------------------------------------------------------------------------

/// Builds the rich Python [`Program`] tree from a syntactic program.
pub fn program_to_py(py: Python<'_>, program: &syntax::Program) -> PyResult<Py<Program>> {
    let mut statements = Vec::with_capacity(program.statements.len());
    for stmt in &program.statements {
        statements.push(stmt_to_py(py, stmt)?);
    }
    let version = program.version.map(|v| (v.major, v.minor));
    Py::new(
        py,
        Program {
            inner: program.clone(),
            span: program.span.into(),
            statements,
            version,
        },
    )
}

fn make_stmt(
    _py: Python<'_>,
    stmt: &syntax::Stmt,
    kind: StmtKind,
    children: Vec<Py<PyAny>>,
    name: Option<String>,
) -> Stmt {
    Stmt {
        span: stmt.span.into(),
        kind,
        children,
        name,
        annotations: stmt
            .annotations
            .iter()
            .map(|a| a.identifier.as_string())
            .collect(),
    }
}

#[allow(clippy::too_many_lines)]
fn stmt_to_py(py: Python<'_>, stmt: &syntax::Stmt) -> PyResult<Py<PyAny>> {
    use syntax::StmtKind as K;
    let obj: Py<PyAny> = match stmt.kind.as_ref() {
        K::Alias(s) => {
            let mut c = Vec::new();
            ident_or_indexed_exprs(py, &mut c, &s.ident)?;
            push_expr_list(py, &mut c, &s.exprs)?;
            let base = make_stmt(py, stmt, StmtKind::Alias, c, ident_name(&s.ident));
            Py::new(py, (AliasStmt, base))?.into_any()
        }
        K::Assign(s) => {
            let mut c = Vec::new();
            ident_or_indexed_exprs(py, &mut c, &s.lhs)?;
            value_expr_exprs(py, &mut c, &s.rhs)?;
            let base = make_stmt(py, stmt, StmtKind::Assign, c, ident_name(&s.lhs));
            Py::new(py, (AssignStmt, base))?.into_any()
        }
        K::AssignOp(s) => {
            let mut c = Vec::new();
            ident_or_indexed_exprs(py, &mut c, &s.lhs)?;
            value_expr_exprs(py, &mut c, &s.rhs)?;
            let base = make_stmt(py, stmt, StmtKind::AssignOp, c, ident_name(&s.lhs));
            Py::new(py, (AssignOpStmt, base))?.into_any()
        }
        K::Barrier(s) => {
            let mut c = Vec::new();
            for op in &s.qubits {
                gate_operand_exprs(py, &mut c, op)?;
            }
            let base = make_stmt(py, stmt, StmtKind::Barrier, c, None);
            Py::new(py, (BarrierStmt, base))?.into_any()
        }
        K::Box(s) => {
            let mut c = Vec::new();
            push_opt_expr(py, &mut c, s.duration.as_ref())?;
            push_stmt_list(py, &mut c, &s.body)?;
            let base = make_stmt(py, stmt, StmtKind::Box, c, None);
            Py::new(py, (BoxStmt, base))?.into_any()
        }
        K::Break(_) => {
            let base = make_stmt(py, stmt, StmtKind::Break, Vec::new(), None);
            Py::new(py, (BreakStmt, base))?.into_any()
        }
        K::Block(s) => {
            let mut c = Vec::new();
            push_stmt_list(py, &mut c, &s.stmts)?;
            let base = make_stmt(py, stmt, StmtKind::Block, c, None);
            Py::new(py, (BlockStmt, base))?.into_any()
        }
        K::Cal(_) => {
            let base = make_stmt(py, stmt, StmtKind::Cal, Vec::new(), None);
            Py::new(py, (CalStmt, base))?.into_any()
        }
        K::CalibrationGrammar(s) => {
            let base = make_stmt(
                py,
                stmt,
                StmtKind::CalibrationGrammar,
                Vec::new(),
                Some(s.name.to_string()),
            );
            Py::new(py, (CalibrationGrammarStmt, base))?.into_any()
        }
        K::ClassicalDecl(s) => {
            let mut c = Vec::new();
            typedef_exprs(py, &mut c, &s.ty)?;
            if let Some(init) = &s.init_expr {
                value_expr_exprs(py, &mut c, init)?;
            }
            let base = make_stmt(
                py,
                stmt,
                StmtKind::ClassicalDecl,
                c,
                Some(s.identifier.name.to_string()),
            );
            Py::new(py, (ClassicalDeclStmt, base))?.into_any()
        }
        K::ConstDecl(s) => {
            let mut c = Vec::new();
            typedef_exprs(py, &mut c, &s.ty)?;
            value_expr_exprs(py, &mut c, &s.init_expr)?;
            let base = make_stmt(
                py,
                stmt,
                StmtKind::ConstDecl,
                c,
                Some(s.identifier.name.to_string()),
            );
            Py::new(py, (ConstDeclStmt, base))?.into_any()
        }
        K::Continue(_) => {
            let base = make_stmt(py, stmt, StmtKind::Continue, Vec::new(), None);
            Py::new(py, (ContinueStmt, base))?.into_any()
        }
        K::Def(s) => {
            let mut c = Vec::new();
            for param in &s.params {
                def_param_exprs(py, &mut c, param)?;
            }
            push_stmt_list(py, &mut c, &s.body.stmts)?;
            let base = make_stmt(py, stmt, StmtKind::Def, c, Some(s.name.name.to_string()));
            Py::new(py, (DefStmt, base))?.into_any()
        }
        K::DefCal(_) => {
            let base = make_stmt(py, stmt, StmtKind::DefCal, Vec::new(), None);
            Py::new(py, (DefCalStmt, base))?.into_any()
        }
        K::Delay(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.duration)?;
            for op in &s.qubits {
                gate_operand_exprs(py, &mut c, op)?;
            }
            let base = make_stmt(py, stmt, StmtKind::Delay, c, None);
            Py::new(py, (DelayStmt, base))?.into_any()
        }
        K::End(_) => {
            let base = make_stmt(py, stmt, StmtKind::End, Vec::new(), None);
            Py::new(py, (EndStmt, base))?.into_any()
        }
        K::ExprStmt(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.expr)?;
            let base = make_stmt(py, stmt, StmtKind::ExprStmt, c, None);
            Py::new(py, (ExprStmt, base))?.into_any()
        }
        K::ExternDecl(s) => {
            let mut c = Vec::new();
            for param in &s.params {
                extern_param_exprs(py, &mut c, param)?;
            }
            let base = make_stmt(
                py,
                stmt,
                StmtKind::ExternDecl,
                c,
                Some(s.ident.name.to_string()),
            );
            Py::new(py, (ExternDeclStmt, base))?.into_any()
        }
        K::For(s) => {
            let mut c = Vec::new();
            enumerable_set_exprs(py, &mut c, &s.set_declaration)?;
            push_stmt(py, &mut c, &s.body)?;
            let base = make_stmt(py, stmt, StmtKind::For, c, Some(s.ident.name.to_string()));
            Py::new(py, (ForStmt, base))?.into_any()
        }
        K::If(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.condition)?;
            push_stmt(py, &mut c, &s.if_body)?;
            if let Some(else_body) = &s.else_body {
                push_stmt(py, &mut c, else_body)?;
            }
            let base = make_stmt(py, stmt, StmtKind::If, c, None);
            Py::new(py, (IfStmt, base))?.into_any()
        }
        K::GateCall(s) => {
            let mut c = Vec::new();
            for m in &s.modifiers {
                gate_modifier_exprs(py, &mut c, m)?;
            }
            push_expr_list(py, &mut c, &s.args)?;
            push_opt_expr(py, &mut c, s.duration.as_ref())?;
            for op in &s.qubits {
                gate_operand_exprs(py, &mut c, op)?;
            }
            let base = make_stmt(
                py,
                stmt,
                StmtKind::GateCall,
                c,
                Some(s.name.name.to_string()),
            );
            Py::new(py, (GateCallStmt, base))?.into_any()
        }
        K::GPhase(s) => {
            let mut c = Vec::new();
            for m in &s.modifiers {
                gate_modifier_exprs(py, &mut c, m)?;
            }
            push_expr_list(py, &mut c, &s.args)?;
            push_opt_expr(py, &mut c, s.duration.as_ref())?;
            for op in &s.qubits {
                gate_operand_exprs(py, &mut c, op)?;
            }
            let base = make_stmt(py, stmt, StmtKind::GPhase, c, None);
            Py::new(py, (GPhaseStmt, base))?.into_any()
        }
        K::Include(s) => {
            let base = make_stmt(
                py,
                stmt,
                StmtKind::Include,
                Vec::new(),
                Some(s.filename.to_string()),
            );
            Py::new(py, (IncludeStmt, base))?.into_any()
        }
        K::IODeclaration(s) => {
            let mut c = Vec::new();
            typedef_exprs(py, &mut c, &s.ty)?;
            let base = make_stmt(
                py,
                stmt,
                StmtKind::IODeclaration,
                c,
                Some(s.ident.name.to_string()),
            );
            Py::new(py, (IODeclarationStmt, base))?.into_any()
        }
        K::Measure(s) => {
            let mut c = Vec::new();
            gate_operand_exprs(py, &mut c, &s.measurement.operand)?;
            if let Some(target) = &s.target {
                ident_or_indexed_exprs(py, &mut c, target)?;
            }
            let base = make_stmt(py, stmt, StmtKind::Measure, c, None);
            Py::new(py, (MeasureStmt, base))?.into_any()
        }
        K::Pragma(s) => {
            let name = s.identifier.as_ref().map(syntax::PathKind::as_string);
            let base = make_stmt(py, stmt, StmtKind::Pragma, Vec::new(), name);
            Py::new(py, (PragmaStmt, base))?.into_any()
        }
        K::QuantumGateDefinition(s) => {
            let mut c = Vec::new();
            push_stmt_list(py, &mut c, &s.body.stmts)?;
            let base = make_stmt(
                py,
                stmt,
                StmtKind::QuantumGateDefinition,
                c,
                Some(s.ident.name.to_string()),
            );
            Py::new(py, (QuantumGateDefinitionStmt, base))?.into_any()
        }
        K::QuantumDecl(s) => {
            let mut c = Vec::new();
            push_opt_expr(py, &mut c, s.ty.size.as_ref())?;
            let base = make_stmt(
                py,
                stmt,
                StmtKind::QuantumDecl,
                c,
                Some(s.qubit.name.to_string()),
            );
            Py::new(py, (QuantumDeclStmt, base))?.into_any()
        }
        K::Reset(s) => {
            let mut c = Vec::new();
            gate_operand_exprs(py, &mut c, &s.operand)?;
            let base = make_stmt(py, stmt, StmtKind::Reset, c, None);
            Py::new(py, (ResetStmt, base))?.into_any()
        }
        K::Return(s) => {
            let mut c = Vec::new();
            if let Some(expr) = &s.expr {
                value_expr_exprs(py, &mut c, expr)?;
            }
            let base = make_stmt(py, stmt, StmtKind::Return, c, None);
            Py::new(py, (ReturnStmt, base))?.into_any()
        }
        K::Switch(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.target)?;
            for case in &s.cases {
                push_expr_list(py, &mut c, &case.labels)?;
                push_stmt_list(py, &mut c, &case.block.stmts)?;
            }
            if let Some(default) = &s.default {
                push_stmt_list(py, &mut c, &default.stmts)?;
            }
            let base = make_stmt(py, stmt, StmtKind::Switch, c, None);
            Py::new(py, (SwitchStmt, base))?.into_any()
        }
        K::WhileLoop(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.while_condition)?;
            push_stmt(py, &mut c, &s.body)?;
            let base = make_stmt(py, stmt, StmtKind::WhileLoop, c, None);
            Py::new(py, (WhileLoopStmt, base))?.into_any()
        }
        K::Err => {
            let base = make_stmt(py, stmt, StmtKind::Err, Vec::new(), None);
            Py::new(py, (ErrStmt, base))?.into_any()
        }
    };
    Ok(obj)
}

fn make_expr(
    expr: &syntax::Expr,
    kind: ExprKind,
    children: Vec<Py<PyAny>>,
    name: Option<String>,
    op: Option<String>,
    value: Option<Py<PyAny>>,
) -> Expr {
    Expr {
        span: expr.span.into(),
        kind,
        children,
        name,
        op,
        value,
    }
}

fn expr_to_py(py: Python<'_>, expr: &syntax::Expr) -> PyResult<Py<PyAny>> {
    use syntax::ExprKind as K;
    let obj: Py<PyAny> = match expr.kind.as_ref() {
        K::Err => {
            let base = make_expr(expr, ExprKind::Err, Vec::new(), None, None, None);
            Py::new(py, (ErrExpr, base))?.into_any()
        }
        K::Ident(id) => {
            let base = make_expr(
                expr,
                ExprKind::Ident,
                Vec::new(),
                Some(id.name.to_string()),
                None,
                None,
            );
            Py::new(py, (IdentExpr, base))?.into_any()
        }
        K::UnaryOp(e) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &e.expr)?;
            let base = make_expr(
                expr,
                ExprKind::UnaryOp,
                c,
                None,
                Some(e.op.to_string()),
                None,
            );
            Py::new(py, (UnaryOpExpr, base))?.into_any()
        }
        K::BinaryOp(e) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &e.lhs)?;
            push_expr(py, &mut c, &e.rhs)?;
            let base = make_expr(
                expr,
                ExprKind::BinaryOp,
                c,
                None,
                Some(e.op.to_string()),
                None,
            );
            Py::new(py, (BinaryOpExpr, base))?.into_any()
        }
        K::Lit(lit) => {
            let mut c = Vec::new();
            let value = if let syntax::LiteralKind::Array(items) = &lit.kind {
                push_expr_list(py, &mut c, items)?;
                None
            } else {
                literal_value(py, &lit.kind)?
            };
            let base = make_expr(expr, ExprKind::Lit, c, None, None, value);
            Py::new(py, (LitExpr, base))?.into_any()
        }
        K::FunctionCall(e) => {
            let mut c = Vec::new();
            push_expr_list(py, &mut c, &e.args)?;
            let base = make_expr(
                expr,
                ExprKind::FunctionCall,
                c,
                Some(e.name.name.to_string()),
                None,
                None,
            );
            Py::new(py, (FunctionCallExpr, base))?.into_any()
        }
        K::Cast(e) => {
            let mut c = Vec::new();
            typedef_exprs(py, &mut c, &e.ty)?;
            push_expr(py, &mut c, &e.arg)?;
            let base = make_expr(expr, ExprKind::Cast, c, None, None, None);
            Py::new(py, (CastExpr, base))?.into_any()
        }
        K::IndexExpr(e) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &e.collection)?;
            index_exprs(py, &mut c, &e.index)?;
            let base = make_expr(expr, ExprKind::IndexExpr, c, None, None, None);
            Py::new(py, (IndexExpr, base))?.into_any()
        }
        K::Paren(inner) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, inner)?;
            let base = make_expr(expr, ExprKind::Paren, c, None, None, None);
            Py::new(py, (ParenExpr, base))?.into_any()
        }
        K::DurationOf(e) => {
            let mut c = Vec::new();
            push_stmt_list(py, &mut c, &e.scope.stmts)?;
            let base = make_expr(expr, ExprKind::DurationOf, c, None, None, None);
            Py::new(py, (DurationOfExpr, base))?.into_any()
        }
    };
    Ok(obj)
}

// ----------------------------------------------------------------------------
// Child-collection helpers
// ----------------------------------------------------------------------------

fn push_expr(py: Python<'_>, out: &mut Vec<Py<PyAny>>, expr: &syntax::Expr) -> PyResult<()> {
    out.push(expr_to_py(py, expr)?);
    Ok(())
}

fn push_opt_expr(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    expr: Option<&syntax::Expr>,
) -> PyResult<()> {
    if let Some(expr) = expr {
        out.push(expr_to_py(py, expr)?);
    }
    Ok(())
}

fn push_expr_list(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    exprs: &syntax::List<syntax::Expr>,
) -> PyResult<()> {
    for expr in exprs {
        out.push(expr_to_py(py, expr)?);
    }
    Ok(())
}

fn push_stmt(py: Python<'_>, out: &mut Vec<Py<PyAny>>, stmt: &syntax::Stmt) -> PyResult<()> {
    out.push(stmt_to_py(py, stmt)?);
    Ok(())
}

fn push_stmt_list(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    stmts: &syntax::List<syntax::Stmt>,
) -> PyResult<()> {
    for stmt in stmts {
        out.push(stmt_to_py(py, stmt)?);
    }
    Ok(())
}

fn ident_name(ident: &syntax::IdentOrIndexedIdent) -> Option<String> {
    match ident {
        syntax::IdentOrIndexedIdent::Ident(id) => Some(id.name.to_string()),
        syntax::IdentOrIndexedIdent::IndexedIdent(id) => Some(id.ident.name.to_string()),
    }
}

fn ident_or_indexed_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    ident: &syntax::IdentOrIndexedIdent,
) -> PyResult<()> {
    if let syntax::IdentOrIndexedIdent::IndexedIdent(indexed) = ident {
        for index in &indexed.indices {
            index_exprs(py, out, index)?;
        }
    }
    Ok(())
}

fn index_exprs(py: Python<'_>, out: &mut Vec<Py<PyAny>>, index: &syntax::Index) -> PyResult<()> {
    match index {
        syntax::Index::IndexSet(set) => push_expr_list(py, out, &set.values),
        syntax::Index::IndexList(list) => {
            for item in &list.values {
                match item.as_ref() {
                    syntax::IndexListItem::RangeDefinition(range) => range_exprs(py, out, range)?,
                    syntax::IndexListItem::Expr(expr) => push_expr(py, out, expr)?,
                    syntax::IndexListItem::Err => {}
                }
            }
            Ok(())
        }
    }
}

fn range_exprs(py: Python<'_>, out: &mut Vec<Py<PyAny>>, range: &syntax::Range) -> PyResult<()> {
    push_opt_expr(py, out, range.start.as_ref())?;
    push_opt_expr(py, out, range.step.as_ref())?;
    push_opt_expr(py, out, range.end.as_ref())
}

fn gate_operand_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    operand: &syntax::GateOperand,
) -> PyResult<()> {
    if let syntax::GateOperandKind::IdentOrIndexedIdent(ident) = &operand.kind {
        ident_or_indexed_exprs(py, out, ident)?;
    }
    Ok(())
}

fn value_expr_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    value: &syntax::ValueExpr,
) -> PyResult<()> {
    match value {
        syntax::ValueExpr::Concat(concat) => push_expr_list(py, out, &concat.operands),
        syntax::ValueExpr::Expr(expr) => push_expr(py, out, expr),
        syntax::ValueExpr::Measurement(measure) => gate_operand_exprs(py, out, &measure.operand),
    }
}

fn enumerable_set_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    set: &syntax::EnumerableSet,
) -> PyResult<()> {
    match set {
        syntax::EnumerableSet::Set(set) => push_expr_list(py, out, &set.values),
        syntax::EnumerableSet::Range(range) => range_exprs(py, out, range),
        syntax::EnumerableSet::Expr(expr) => push_expr(py, out, expr),
    }
}

fn gate_modifier_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    modifier: &syntax::QuantumGateModifier,
) -> PyResult<()> {
    match &modifier.kind {
        syntax::GateModifierKind::Inv => Ok(()),
        syntax::GateModifierKind::Pow(expr) => push_expr(py, out, expr),
        syntax::GateModifierKind::Ctrl(expr) | syntax::GateModifierKind::NegCtrl(expr) => {
            push_opt_expr(py, out, expr.as_ref())
        }
    }
}

fn typedef_exprs(py: Python<'_>, out: &mut Vec<Py<PyAny>>, ty: &syntax::TypeDef) -> PyResult<()> {
    match ty {
        syntax::TypeDef::Scalar(scalar) => scalar_type_exprs(py, out, scalar),
        syntax::TypeDef::Array(array) => push_expr_list(py, out, &array.dimensions),
        syntax::TypeDef::ArrayReference(reference) => array_reference_exprs(py, out, reference),
    }
}

fn array_reference_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    reference: &syntax::ArrayReferenceType,
) -> PyResult<()> {
    match reference {
        syntax::ArrayReferenceType::Static(ty) => push_expr_list(py, out, &ty.dimensions),
        syntax::ArrayReferenceType::Dyn(ty) => push_expr(py, out, &ty.dimensions),
    }
}

fn scalar_type_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    scalar: &syntax::ScalarType,
) -> PyResult<()> {
    match &scalar.kind {
        syntax::ScalarTypeKind::Bit(ty) => push_opt_expr(py, out, ty.size.as_ref()),
        syntax::ScalarTypeKind::Int(ty) => push_opt_expr(py, out, ty.size.as_ref()),
        syntax::ScalarTypeKind::UInt(ty) => push_opt_expr(py, out, ty.size.as_ref()),
        syntax::ScalarTypeKind::Float(ty) => push_opt_expr(py, out, ty.size.as_ref()),
        syntax::ScalarTypeKind::Angle(ty) => push_opt_expr(py, out, ty.size.as_ref()),
        syntax::ScalarTypeKind::Complex(ty) => {
            if let Some(base) = &ty.base_size {
                push_opt_expr(py, out, base.size.as_ref())?;
            }
            Ok(())
        }
        syntax::ScalarTypeKind::BoolType
        | syntax::ScalarTypeKind::Duration
        | syntax::ScalarTypeKind::Stretch
        | syntax::ScalarTypeKind::Err => Ok(()),
    }
}

fn extern_param_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    param: &syntax::ExternParameter,
) -> PyResult<()> {
    match param {
        syntax::ExternParameter::ArrayReference(reference, _) => {
            array_reference_exprs(py, out, reference)
        }
        syntax::ExternParameter::Scalar(scalar, _) => scalar_type_exprs(py, out, scalar),
    }
}

fn def_param_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    param: &syntax::DefParameter,
) -> PyResult<()> {
    match param.ty.as_ref() {
        syntax::DefParameterType::ArrayReference(reference) => {
            array_reference_exprs(py, out, reference)
        }
        syntax::DefParameterType::Qubit(qubit) => push_opt_expr(py, out, qubit.size.as_ref()),
        syntax::DefParameterType::Scalar(scalar) => scalar_type_exprs(py, out, scalar),
    }
}

fn literal_value(py: Python<'_>, lit: &syntax::LiteralKind) -> PyResult<Option<Py<PyAny>>> {
    let value: Py<PyAny> = match lit {
        syntax::LiteralKind::Array(_) => return Ok(None),
        syntax::LiteralKind::Bool(b) => b.into_py_any(py)?,
        syntax::LiteralKind::Int(i) => i.into_py_any(py)?,
        syntax::LiteralKind::Float(f) => f.into_py_any(py)?,
        syntax::LiteralKind::Imaginary(f) => {
            PyComplex::from_doubles(py, 0.0, *f).into_any().unbind()
        }
        syntax::LiteralKind::String(s) => s.as_ref().into_py_any(py)?,
        syntax::LiteralKind::Duration(value, unit) => (*value, unit.to_string()).into_py_any(py)?,
        syntax::LiteralKind::Bitstring(value, width) => {
            let width = *width as usize;
            format!("{:0>width$}", value.to_str_radix(2)).into_py_any(py)?
        }
        syntax::LiteralKind::BigInt(value) => bigint_to_py(py, &value.to_str_radix(10))?,
    };
    Ok(Some(value))
}

fn bigint_to_py(py: Python<'_>, decimal: &str) -> PyResult<Py<PyAny>> {
    Ok(py
        .import("builtins")?
        .getattr("int")?
        .call1((decimal,))?
        .unbind())
}

/// Emits canonical `OpenQASM 3` source for a syntactic [`Program`].
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
pub fn unparse(program: PyRef<'_, Program>) -> String {
    qdk_openqasm_parser::unparse(&program.inner)
}
