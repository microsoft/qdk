// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Rich, read-only Python projections of the semantic `OpenQASM` AST.
//!
//! Mirrors the structure of [`crate::syntax`] but for the semantic tree: a
//! backbone of [`SemProgram`], [`SemStmt`], and [`SemExpr`] with a kind-specific
//! subclass per variant. Because the semantic tree carries resolved type and
//! const-value information, the [`SemExpr`] base additionally exposes `ty`,
//! `const_value`, and `symbol_id`.
//!
//! The resolved [`SymbolTable`] is projected as an iterable collection of
//! read-only [`Symbol`] views, each exposing `name`, `ty`, and `span`. Symbol
//! references are not tracked yet; a `references` accessor is deferred to a
//! future run.

use crate::span::Span;
use pyo3::IntoPyObjectExt;
use pyo3::prelude::*;
use pyo3::types::{PyComplex, PyList};
use qdk_openqasm_parser::semantic::ast as sem;
use qdk_openqasm_parser::semantic::symbols::{SymbolId, SymbolTable as CoreSymbolTable};
use qdk_openqasm_parser::semantic::types::Type as CoreType;

/// A tag mirroring [`sem::StmtKind`], exposed to Python as an integer-valued
/// enum.
#[pyclass(
    eq,
    eq_int,
    frozen,
    from_py_object,
    module = "qdk_openqasm_parser._native"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemStmtKind {
    Alias,
    Assign,
    Barrier,
    Box,
    Block,
    Break,
    Calibration,
    CalibrationGrammar,
    ClassicalDecl,
    Continue,
    Def,
    DefCal,
    Delay,
    End,
    ExprStmt,
    ExternDecl,
    For,
    GateCall,
    If,
    Include,
    IndexedClassicalTypeAssign,
    InputDeclaration,
    OutputDeclaration,
    MeasureArrow,
    Pragma,
    QuantumGateDefinition,
    QubitDecl,
    QubitArrayDecl,
    Reset,
    Return,
    Switch,
    WhileLoop,
    Err,
}

/// A tag mirroring [`sem::ExprKind`], exposed to Python as an integer-valued
/// enum.
#[pyclass(
    eq,
    eq_int,
    frozen,
    from_py_object,
    module = "qdk_openqasm_parser._native"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemExprKind {
    Err,
    CapturedIdent,
    Ident,
    UnaryOp,
    BinaryOp,
    Lit,
    FunctionCall,
    BuiltinFunctionCall,
    Cast,
    IndexedExpr,
    Paren,
    Measure,
    SizeofCall,
    DurationofCall,
    Concat,
}

/// A read-only view of a resolved semantic [`CoreType`].
#[pyclass(module = "qdk_openqasm_parser._native", frozen)]
pub struct Type {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    is_const: bool,
}

impl Type {
    fn from_core(py: Python<'_>, ty: &CoreType) -> PyResult<Py<Type>> {
        Py::new(
            py,
            Type {
                name: ty.to_string(),
                is_const: ty.is_const(),
            },
        )
    }
}

#[pymethods]
impl Type {
    fn __str__(&self) -> String {
        self.name.clone()
    }

    fn __repr__(&self) -> String {
        format!("Type({:?}, is_const={})", self.name, self.is_const)
    }
}

/// A read-only view of a resolved [`Symbol`].
#[pyclass(module = "qdk_openqasm_parser._native", frozen)]
pub struct Symbol {
    #[pyo3(get)]
    id: u32,
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    span: Span,
    #[pyo3(get)]
    ty: Py<Type>,
}

#[pymethods]
impl Symbol {
    fn __repr__(&self) -> String {
        format!("Symbol(id={}, name={:?})", self.id, self.name)
    }
}

/// An iterable, read-only projection of the resolved symbol table.
#[pyclass(module = "qdk_openqasm_parser._native", frozen)]
pub struct SymbolTable {
    symbols: Vec<Py<Symbol>>,
}

#[pymethods]
impl SymbolTable {
    /// The number of symbols in the table.
    fn __len__(&self) -> usize {
        self.symbols.len()
    }

    /// Iterates over the [`Symbol`] views in the table.
    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let list = PyList::new(py, self.symbols.iter().map(|s| s.clone_ref(py)))?;
        Ok(list.as_any().try_iter()?.into_any().unbind())
    }

    /// Returns the symbol with the given id, or `None`.
    fn get(&self, py: Python<'_>, id: u32) -> Option<Py<Symbol>> {
        self.symbols
            .iter()
            .find(|s| s.borrow(py).id == id)
            .map(|s| s.clone_ref(py))
    }

    /// Returns the first symbol with the given name, or `None`.
    fn lookup(&self, py: Python<'_>, name: &str) -> Option<Py<Symbol>> {
        self.symbols
            .iter()
            .find(|s| s.borrow(py).name == name)
            .map(|s| s.clone_ref(py))
    }

    fn __repr__(&self) -> String {
        format!("SymbolTable([{} symbols])", self.symbols.len())
    }
}

/// The root of a semantic `OpenQASM` program.
#[pyclass(module = "qdk_openqasm_parser._native", frozen)]
pub struct SemProgram {
    statements: Vec<Py<PyAny>>,
    #[pyo3(get)]
    pragmas: Vec<String>,
    version: Option<(u32, Option<u32>)>,
}

#[pymethods]
impl SemProgram {
    #[getter]
    fn version(&self) -> Option<(u32, Option<u32>)> {
        self.version
    }

    #[getter]
    fn statements(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        clone_nodes(py, &self.statements)
    }

    fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        clone_nodes(py, &self.statements)
    }

    fn accept(slf: Bound<'_, Self>, visitor: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        Ok(visitor.call_method1("visit", (slf,))?.unbind())
    }

    fn __repr__(&self) -> String {
        format!("SemProgram(statements=[{} items])", self.statements.len())
    }
}

/// The base class for every semantic statement node.
#[pyclass(subclass, module = "qdk_openqasm_parser._native", frozen)]
pub struct SemStmt {
    #[pyo3(get)]
    span: Span,
    #[pyo3(get)]
    kind: SemStmtKind,
    children: Vec<Py<PyAny>>,
    #[pyo3(get)]
    name: Option<String>,
    #[pyo3(get)]
    annotations: Vec<String>,
}

#[pymethods]
impl SemStmt {
    fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        clone_nodes(py, &self.children)
    }

    fn accept(slf: Bound<'_, Self>, visitor: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        Ok(visitor.call_method1("visit", (slf,))?.unbind())
    }

    fn __repr__(&self) -> String {
        format!("{:?}({:?})", self.kind, self.span)
    }
}

/// The base class for every semantic expression node.
#[pyclass(subclass, module = "qdk_openqasm_parser._native", frozen)]
pub struct SemExpr {
    #[pyo3(get)]
    span: Span,
    #[pyo3(get)]
    kind: SemExprKind,
    children: Vec<Py<PyAny>>,
    #[pyo3(get)]
    name: Option<String>,
    #[pyo3(get)]
    op: Option<String>,
    #[pyo3(get)]
    value: Option<Py<PyAny>>,
    #[pyo3(get)]
    ty: Py<Type>,
    #[pyo3(get)]
    const_value: Option<Py<PyAny>>,
    #[pyo3(get)]
    symbol_id: Option<u32>,
}

#[pymethods]
impl SemExpr {
    fn children(&self, py: Python<'_>) -> Vec<Py<PyAny>> {
        clone_nodes(py, &self.children)
    }

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

macro_rules! sem_stmt_subclass {
    ($name:ident) => {
        #[pyclass(extends = SemStmt, module = "qdk_openqasm_parser._native", frozen)]
        pub struct $name;
    };
}

macro_rules! sem_expr_subclass {
    ($name:ident) => {
        #[pyclass(extends = SemExpr, module = "qdk_openqasm_parser._native", frozen)]
        pub struct $name;
    };
}

sem_stmt_subclass!(SemAliasStmt);
sem_stmt_subclass!(SemAssignStmt);
sem_stmt_subclass!(SemBarrierStmt);
sem_stmt_subclass!(SemBoxStmt);
sem_stmt_subclass!(SemBlockStmt);
sem_stmt_subclass!(SemBreakStmt);
sem_stmt_subclass!(SemCalibrationStmt);
sem_stmt_subclass!(SemCalibrationGrammarStmt);
sem_stmt_subclass!(SemClassicalDeclStmt);
sem_stmt_subclass!(SemContinueStmt);
sem_stmt_subclass!(SemDefStmt);
sem_stmt_subclass!(SemDefCalStmt);
sem_stmt_subclass!(SemDelayStmt);
sem_stmt_subclass!(SemEndStmt);
sem_stmt_subclass!(SemExprStmt);
sem_stmt_subclass!(SemExternDeclStmt);
sem_stmt_subclass!(SemForStmt);
sem_stmt_subclass!(SemGateCallStmt);
sem_stmt_subclass!(SemIfStmt);
sem_stmt_subclass!(SemIncludeStmt);
sem_stmt_subclass!(SemIndexedClassicalTypeAssignStmt);
sem_stmt_subclass!(SemInputDeclarationStmt);
sem_stmt_subclass!(SemOutputDeclarationStmt);
sem_stmt_subclass!(SemMeasureArrowStmt);
sem_stmt_subclass!(SemPragmaStmt);
sem_stmt_subclass!(SemQuantumGateDefinitionStmt);
sem_stmt_subclass!(SemQubitDeclStmt);
sem_stmt_subclass!(SemQubitArrayDeclStmt);
sem_stmt_subclass!(SemResetStmt);
sem_stmt_subclass!(SemReturnStmt);
sem_stmt_subclass!(SemSwitchStmt);
sem_stmt_subclass!(SemWhileLoopStmt);
sem_stmt_subclass!(SemErrStmt);

sem_expr_subclass!(SemErrExpr);
sem_expr_subclass!(SemCapturedIdentExpr);
sem_expr_subclass!(SemIdentExpr);
sem_expr_subclass!(SemUnaryOpExpr);
sem_expr_subclass!(SemBinaryOpExpr);
sem_expr_subclass!(SemLitExpr);
sem_expr_subclass!(SemFunctionCallExpr);
sem_expr_subclass!(SemBuiltinFunctionCallExpr);
sem_expr_subclass!(SemCastExpr);
sem_expr_subclass!(SemIndexedExpr);
sem_expr_subclass!(SemParenExpr);
sem_expr_subclass!(SemMeasureExpr);
sem_expr_subclass!(SemSizeofCallExpr);
sem_expr_subclass!(SemDurationofCallExpr);
sem_expr_subclass!(SemConcatExpr);

/// Registers all semantic node classes with the native module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SemStmtKind>()?;
    m.add_class::<SemExprKind>()?;
    m.add_class::<Type>()?;
    m.add_class::<Symbol>()?;
    m.add_class::<SymbolTable>()?;
    m.add_class::<SemProgram>()?;
    m.add_class::<SemStmt>()?;
    m.add_class::<SemExpr>()?;
    m.add_class::<SemAliasStmt>()?;
    m.add_class::<SemAssignStmt>()?;
    m.add_class::<SemBarrierStmt>()?;
    m.add_class::<SemBoxStmt>()?;
    m.add_class::<SemBlockStmt>()?;
    m.add_class::<SemBreakStmt>()?;
    m.add_class::<SemCalibrationStmt>()?;
    m.add_class::<SemCalibrationGrammarStmt>()?;
    m.add_class::<SemClassicalDeclStmt>()?;
    m.add_class::<SemContinueStmt>()?;
    m.add_class::<SemDefStmt>()?;
    m.add_class::<SemDefCalStmt>()?;
    m.add_class::<SemDelayStmt>()?;
    m.add_class::<SemEndStmt>()?;
    m.add_class::<SemExprStmt>()?;
    m.add_class::<SemExternDeclStmt>()?;
    m.add_class::<SemForStmt>()?;
    m.add_class::<SemGateCallStmt>()?;
    m.add_class::<SemIfStmt>()?;
    m.add_class::<SemIncludeStmt>()?;
    m.add_class::<SemIndexedClassicalTypeAssignStmt>()?;
    m.add_class::<SemInputDeclarationStmt>()?;
    m.add_class::<SemOutputDeclarationStmt>()?;
    m.add_class::<SemMeasureArrowStmt>()?;
    m.add_class::<SemPragmaStmt>()?;
    m.add_class::<SemQuantumGateDefinitionStmt>()?;
    m.add_class::<SemQubitDeclStmt>()?;
    m.add_class::<SemQubitArrayDeclStmt>()?;
    m.add_class::<SemResetStmt>()?;
    m.add_class::<SemReturnStmt>()?;
    m.add_class::<SemSwitchStmt>()?;
    m.add_class::<SemWhileLoopStmt>()?;
    m.add_class::<SemErrStmt>()?;
    m.add_class::<SemErrExpr>()?;
    m.add_class::<SemCapturedIdentExpr>()?;
    m.add_class::<SemIdentExpr>()?;
    m.add_class::<SemUnaryOpExpr>()?;
    m.add_class::<SemBinaryOpExpr>()?;
    m.add_class::<SemLitExpr>()?;
    m.add_class::<SemFunctionCallExpr>()?;
    m.add_class::<SemBuiltinFunctionCallExpr>()?;
    m.add_class::<SemCastExpr>()?;
    m.add_class::<SemIndexedExpr>()?;
    m.add_class::<SemParenExpr>()?;
    m.add_class::<SemMeasureExpr>()?;
    m.add_class::<SemSizeofCallExpr>()?;
    m.add_class::<SemDurationofCallExpr>()?;
    m.add_class::<SemConcatExpr>()?;
    Ok(())
}

// ----------------------------------------------------------------------------
// Symbol table projection
// ----------------------------------------------------------------------------

/// Builds the iterable [`SymbolTable`] projection from the resolved table.
pub fn symbol_table_to_py(py: Python<'_>, symbols: &CoreSymbolTable) -> PyResult<Py<SymbolTable>> {
    let mut entries = Vec::new();
    for (id, symbol) in symbols.iter() {
        let symbol = Py::new(
            py,
            Symbol {
                id: id.into(),
                name: symbol.name.clone(),
                span: symbol.span.into(),
                ty: Type::from_core(py, &symbol.ty)?,
            },
        )?;
        entries.push(symbol);
    }
    Py::new(py, SymbolTable { symbols: entries })
}

fn symbol_name(symbols: &CoreSymbolTable, id: SymbolId) -> Option<String> {
    symbols.get(id).map(|symbol| symbol.name.clone())
}

// ----------------------------------------------------------------------------
// Conversion: semantic AST -> Python node tree
// ----------------------------------------------------------------------------

/// Builds the rich Python [`SemProgram`] tree from a semantic program.
pub fn program_to_py(
    py: Python<'_>,
    program: &sem::Program,
    symbols: &CoreSymbolTable,
) -> PyResult<Py<SemProgram>> {
    let mut statements = Vec::with_capacity(program.statements.len());
    for stmt in &program.statements {
        statements.push(stmt_to_py(py, stmt, symbols)?);
    }
    let pragmas = program
        .pragmas
        .iter()
        .map(|p| {
            p.identifier.as_ref().map_or_else(
                String::new,
                qdk_openqasm_parser::parser::ast::PathKind::as_string,
            )
        })
        .collect();
    let version = program.version.map(|v| (v.major, v.minor));
    Py::new(
        py,
        SemProgram {
            statements,
            pragmas,
            version,
        },
    )
}

fn make_stmt(
    stmt: &sem::Stmt,
    kind: SemStmtKind,
    children: Vec<Py<PyAny>>,
    name: Option<String>,
) -> SemStmt {
    SemStmt {
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
fn stmt_to_py(py: Python<'_>, stmt: &sem::Stmt, symbols: &CoreSymbolTable) -> PyResult<Py<PyAny>> {
    use sem::StmtKind as K;
    let obj: Py<PyAny> = match stmt.kind.as_ref() {
        K::Alias(s) => {
            let mut c = Vec::new();
            push_expr_list(py, &mut c, &s.exprs, symbols)?;
            let base = make_stmt(
                stmt,
                SemStmtKind::Alias,
                c,
                symbol_name(symbols, s.symbol_id),
            );
            Py::new(py, (SemAliasStmt, base))?.into_any()
        }
        K::Assign(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.lhs, symbols)?;
            push_expr(py, &mut c, &s.rhs, symbols)?;
            let base = make_stmt(stmt, SemStmtKind::Assign, c, None);
            Py::new(py, (SemAssignStmt, base))?.into_any()
        }
        K::Barrier(s) => {
            let mut c = Vec::new();
            for op in &s.qubits {
                gate_operand_exprs(py, &mut c, op, symbols)?;
            }
            let base = make_stmt(stmt, SemStmtKind::Barrier, c, None);
            Py::new(py, (SemBarrierStmt, base))?.into_any()
        }
        K::Box(s) => {
            let mut c = Vec::new();
            push_opt_expr(py, &mut c, s.duration.as_ref(), symbols)?;
            push_stmt_list(py, &mut c, &s.body, symbols)?;
            let base = make_stmt(stmt, SemStmtKind::Box, c, None);
            Py::new(py, (SemBoxStmt, base))?.into_any()
        }
        K::Block(s) => {
            let mut c = Vec::new();
            push_stmt_list(py, &mut c, &s.stmts, symbols)?;
            let base = make_stmt(stmt, SemStmtKind::Block, c, None);
            Py::new(py, (SemBlockStmt, base))?.into_any()
        }
        K::Break(_) => {
            let base = make_stmt(stmt, SemStmtKind::Break, Vec::new(), None);
            Py::new(py, (SemBreakStmt, base))?.into_any()
        }
        K::Calibration(_) => {
            let base = make_stmt(stmt, SemStmtKind::Calibration, Vec::new(), None);
            Py::new(py, (SemCalibrationStmt, base))?.into_any()
        }
        K::CalibrationGrammar(s) => {
            let base = make_stmt(
                stmt,
                SemStmtKind::CalibrationGrammar,
                Vec::new(),
                Some(s.name.to_string()),
            );
            Py::new(py, (SemCalibrationGrammarStmt, base))?.into_any()
        }
        K::ClassicalDecl(s) => {
            let mut c = Vec::new();
            push_expr_list(py, &mut c, &s.ty_exprs, symbols)?;
            push_expr(py, &mut c, &s.init_expr, symbols)?;
            let base = make_stmt(
                stmt,
                SemStmtKind::ClassicalDecl,
                c,
                symbol_name(symbols, s.symbol_id),
            );
            Py::new(py, (SemClassicalDeclStmt, base))?.into_any()
        }
        K::Continue(_) => {
            let base = make_stmt(stmt, SemStmtKind::Continue, Vec::new(), None);
            Py::new(py, (SemContinueStmt, base))?.into_any()
        }
        K::Def(s) => {
            let mut c = Vec::new();
            for param in &s.params {
                push_expr_list(py, &mut c, &param.ty_exprs, symbols)?;
            }
            push_expr_list(py, &mut c, &s.return_ty_exprs, symbols)?;
            push_stmt_list(py, &mut c, &s.body.stmts, symbols)?;
            let base = make_stmt(stmt, SemStmtKind::Def, c, symbol_name(symbols, s.symbol_id));
            Py::new(py, (SemDefStmt, base))?.into_any()
        }
        K::DefCal(_) => {
            let base = make_stmt(stmt, SemStmtKind::DefCal, Vec::new(), None);
            Py::new(py, (SemDefCalStmt, base))?.into_any()
        }
        K::Delay(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.duration, symbols)?;
            for op in &s.qubits {
                gate_operand_exprs(py, &mut c, op, symbols)?;
            }
            let base = make_stmt(stmt, SemStmtKind::Delay, c, None);
            Py::new(py, (SemDelayStmt, base))?.into_any()
        }
        K::End(_) => {
            let base = make_stmt(stmt, SemStmtKind::End, Vec::new(), None);
            Py::new(py, (SemEndStmt, base))?.into_any()
        }
        K::ExprStmt(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.expr, symbols)?;
            let base = make_stmt(stmt, SemStmtKind::ExprStmt, c, None);
            Py::new(py, (SemExprStmt, base))?.into_any()
        }
        K::ExternDecl(s) => {
            let mut c = Vec::new();
            push_expr_list(py, &mut c, &s.ty_exprs, symbols)?;
            push_expr_list(py, &mut c, &s.return_ty_exprs, symbols)?;
            let base = make_stmt(
                stmt,
                SemStmtKind::ExternDecl,
                c,
                symbol_name(symbols, s.symbol_id),
            );
            Py::new(py, (SemExternDeclStmt, base))?.into_any()
        }
        K::For(s) => {
            let mut c = Vec::new();
            push_expr_list(py, &mut c, &s.ty_exprs, symbols)?;
            enumerable_set_exprs(py, &mut c, &s.set_declaration, symbols)?;
            push_stmt(py, &mut c, &s.body, symbols)?;
            let base = make_stmt(
                stmt,
                SemStmtKind::For,
                c,
                symbol_name(symbols, s.loop_variable),
            );
            Py::new(py, (SemForStmt, base))?.into_any()
        }
        K::GateCall(s) => {
            let mut c = Vec::new();
            for m in &s.modifiers {
                gate_modifier_exprs(py, &mut c, m, symbols)?;
            }
            push_expr_list(py, &mut c, &s.args, symbols)?;
            push_opt_expr(py, &mut c, s.duration.as_ref(), symbols)?;
            for op in &s.qubits {
                gate_operand_exprs(py, &mut c, op, symbols)?;
            }
            let base = make_stmt(
                stmt,
                SemStmtKind::GateCall,
                c,
                symbol_name(symbols, s.symbol_id),
            );
            Py::new(py, (SemGateCallStmt, base))?.into_any()
        }
        K::If(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.condition, symbols)?;
            push_stmt(py, &mut c, &s.if_body, symbols)?;
            if let Some(else_body) = &s.else_body {
                push_stmt(py, &mut c, else_body, symbols)?;
            }
            let base = make_stmt(stmt, SemStmtKind::If, c, None);
            Py::new(py, (SemIfStmt, base))?.into_any()
        }
        K::Include(s) => {
            let base = make_stmt(
                stmt,
                SemStmtKind::Include,
                Vec::new(),
                Some(s.filename.to_string()),
            );
            Py::new(py, (SemIncludeStmt, base))?.into_any()
        }
        K::IndexedClassicalTypeAssign(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.lhs, symbols)?;
            for index in &s.indices {
                index_exprs(py, &mut c, index, symbols)?;
            }
            push_expr(py, &mut c, &s.rhs, symbols)?;
            let base = make_stmt(stmt, SemStmtKind::IndexedClassicalTypeAssign, c, None);
            Py::new(py, (SemIndexedClassicalTypeAssignStmt, base))?.into_any()
        }
        K::InputDeclaration(s) => {
            let mut c = Vec::new();
            push_expr_list(py, &mut c, &s.ty_exprs, symbols)?;
            let base = make_stmt(
                stmt,
                SemStmtKind::InputDeclaration,
                c,
                symbol_name(symbols, s.symbol_id),
            );
            Py::new(py, (SemInputDeclarationStmt, base))?.into_any()
        }
        K::OutputDeclaration(s) => {
            let mut c = Vec::new();
            push_expr_list(py, &mut c, &s.ty_exprs, symbols)?;
            push_expr(py, &mut c, &s.init_expr, symbols)?;
            let base = make_stmt(
                stmt,
                SemStmtKind::OutputDeclaration,
                c,
                symbol_name(symbols, s.symbol_id),
            );
            Py::new(py, (SemOutputDeclarationStmt, base))?.into_any()
        }
        K::MeasureArrow(s) => {
            let mut c = Vec::new();
            gate_operand_exprs(py, &mut c, &s.measurement.operand, symbols)?;
            if let Some(target) = &s.target {
                push_expr(py, &mut c, target, symbols)?;
            }
            let base = make_stmt(stmt, SemStmtKind::MeasureArrow, c, None);
            Py::new(py, (SemMeasureArrowStmt, base))?.into_any()
        }
        K::Pragma(s) => {
            let name = s
                .identifier
                .as_ref()
                .map(qdk_openqasm_parser::parser::ast::PathKind::as_string);
            let base = make_stmt(stmt, SemStmtKind::Pragma, Vec::new(), name);
            Py::new(py, (SemPragmaStmt, base))?.into_any()
        }
        K::QuantumGateDefinition(s) => {
            let mut c = Vec::new();
            push_stmt_list(py, &mut c, &s.body.stmts, symbols)?;
            let base = make_stmt(
                stmt,
                SemStmtKind::QuantumGateDefinition,
                c,
                symbol_name(symbols, s.symbol_id),
            );
            Py::new(py, (SemQuantumGateDefinitionStmt, base))?.into_any()
        }
        K::QubitDecl(s) => {
            let base = make_stmt(
                stmt,
                SemStmtKind::QubitDecl,
                Vec::new(),
                symbol_name(symbols, s.symbol_id),
            );
            Py::new(py, (SemQubitDeclStmt, base))?.into_any()
        }
        K::QubitArrayDecl(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.size, symbols)?;
            let base = make_stmt(
                stmt,
                SemStmtKind::QubitArrayDecl,
                c,
                symbol_name(symbols, s.symbol_id),
            );
            Py::new(py, (SemQubitArrayDeclStmt, base))?.into_any()
        }
        K::Reset(s) => {
            let mut c = Vec::new();
            gate_operand_exprs(py, &mut c, &s.operand, symbols)?;
            let base = make_stmt(stmt, SemStmtKind::Reset, c, None);
            Py::new(py, (SemResetStmt, base))?.into_any()
        }
        K::Return(s) => {
            let mut c = Vec::new();
            if let Some(expr) = &s.expr {
                push_expr(py, &mut c, expr, symbols)?;
            }
            let base = make_stmt(stmt, SemStmtKind::Return, c, None);
            Py::new(py, (SemReturnStmt, base))?.into_any()
        }
        K::Switch(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.target, symbols)?;
            for case in &s.cases {
                push_expr_list(py, &mut c, &case.labels, symbols)?;
                push_stmt_list(py, &mut c, &case.block.stmts, symbols)?;
            }
            if let Some(default) = &s.default {
                push_stmt_list(py, &mut c, &default.stmts, symbols)?;
            }
            let base = make_stmt(stmt, SemStmtKind::Switch, c, None);
            Py::new(py, (SemSwitchStmt, base))?.into_any()
        }
        K::WhileLoop(s) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &s.condition, symbols)?;
            push_stmt(py, &mut c, &s.body, symbols)?;
            let base = make_stmt(stmt, SemStmtKind::WhileLoop, c, None);
            Py::new(py, (SemWhileLoopStmt, base))?.into_any()
        }
        K::Err => {
            let base = make_stmt(stmt, SemStmtKind::Err, Vec::new(), None);
            Py::new(py, (SemErrStmt, base))?.into_any()
        }
    };
    Ok(obj)
}

#[allow(clippy::too_many_arguments)]
fn make_expr(
    py: Python<'_>,
    expr: &sem::Expr,
    kind: SemExprKind,
    children: Vec<Py<PyAny>>,
    name: Option<String>,
    op: Option<String>,
    value: Option<Py<PyAny>>,
    symbol_id: Option<u32>,
) -> PyResult<SemExpr> {
    Ok(SemExpr {
        span: expr.span.into(),
        kind,
        children,
        name,
        op,
        value,
        ty: Type::from_core(py, &expr.ty)?,
        const_value: literal_value(py, expr.const_value.as_ref())?,
        symbol_id,
    })
}

#[allow(clippy::too_many_lines)]
fn expr_to_py(py: Python<'_>, expr: &sem::Expr, symbols: &CoreSymbolTable) -> PyResult<Py<PyAny>> {
    use sem::ExprKind as K;
    let obj: Py<PyAny> = match expr.kind.as_ref() {
        K::Err => {
            let base = make_expr(
                py,
                expr,
                SemExprKind::Err,
                Vec::new(),
                None,
                None,
                None,
                None,
            )?;
            Py::new(py, (SemErrExpr, base))?.into_any()
        }
        K::CapturedIdent(id) => {
            let base = make_expr(
                py,
                expr,
                SemExprKind::CapturedIdent,
                Vec::new(),
                symbol_name(symbols, *id),
                None,
                None,
                Some((*id).into()),
            )?;
            Py::new(py, (SemCapturedIdentExpr, base))?.into_any()
        }
        K::Ident(id) => {
            let base = make_expr(
                py,
                expr,
                SemExprKind::Ident,
                Vec::new(),
                symbol_name(symbols, *id),
                None,
                None,
                Some((*id).into()),
            )?;
            Py::new(py, (SemIdentExpr, base))?.into_any()
        }
        K::UnaryOp(e) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &e.expr, symbols)?;
            let base = make_expr(
                py,
                expr,
                SemExprKind::UnaryOp,
                c,
                None,
                Some(e.op.to_string()),
                None,
                None,
            )?;
            Py::new(py, (SemUnaryOpExpr, base))?.into_any()
        }
        K::BinaryOp(e) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &e.lhs, symbols)?;
            push_expr(py, &mut c, &e.rhs, symbols)?;
            let base = make_expr(
                py,
                expr,
                SemExprKind::BinaryOp,
                c,
                None,
                Some(e.op.to_string()),
                None,
                None,
            )?;
            Py::new(py, (SemBinaryOpExpr, base))?.into_any()
        }
        K::Lit(lit) => {
            let mut c = Vec::new();
            let value = if let sem::LiteralKind::Array(array) = lit {
                for item in &array.data {
                    push_expr(py, &mut c, item, symbols)?;
                }
                None
            } else {
                literal_kind_value(py, lit)?
            };
            let base = make_expr(py, expr, SemExprKind::Lit, c, None, None, value, None)?;
            Py::new(py, (SemLitExpr, base))?.into_any()
        }
        K::FunctionCall(e) => {
            let mut c = Vec::new();
            push_expr_list(py, &mut c, &e.args, symbols)?;
            let base = make_expr(
                py,
                expr,
                SemExprKind::FunctionCall,
                c,
                symbol_name(symbols, e.symbol_id),
                None,
                None,
                Some(e.symbol_id.into()),
            )?;
            Py::new(py, (SemFunctionCallExpr, base))?.into_any()
        }
        K::BuiltinFunctionCall(e) => {
            let mut c = Vec::new();
            for arg in &e.args {
                push_expr(py, &mut c, arg, symbols)?;
            }
            let base = make_expr(
                py,
                expr,
                SemExprKind::BuiltinFunctionCall,
                c,
                Some(e.name.to_string()),
                None,
                None,
                None,
            )?;
            Py::new(py, (SemBuiltinFunctionCallExpr, base))?.into_any()
        }
        K::Cast(e) => {
            let mut c = Vec::new();
            push_expr_list(py, &mut c, &e.ty_exprs, symbols)?;
            push_expr(py, &mut c, &e.expr, symbols)?;
            let base = make_expr(py, expr, SemExprKind::Cast, c, None, None, None, None)?;
            Py::new(py, (SemCastExpr, base))?.into_any()
        }
        K::IndexedExpr(e) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &e.collection, symbols)?;
            index_exprs(py, &mut c, &e.index, symbols)?;
            let base = make_expr(
                py,
                expr,
                SemExprKind::IndexedExpr,
                c,
                None,
                None,
                None,
                None,
            )?;
            Py::new(py, (SemIndexedExpr, base))?.into_any()
        }
        K::Paren(inner) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, inner, symbols)?;
            let base = make_expr(py, expr, SemExprKind::Paren, c, None, None, None, None)?;
            Py::new(py, (SemParenExpr, base))?.into_any()
        }
        K::Measure(e) => {
            let mut c = Vec::new();
            gate_operand_exprs(py, &mut c, &e.operand, symbols)?;
            let base = make_expr(py, expr, SemExprKind::Measure, c, None, None, None, None)?;
            Py::new(py, (SemMeasureExpr, base))?.into_any()
        }
        K::SizeofCall(e) => {
            let mut c = Vec::new();
            push_expr(py, &mut c, &e.array, symbols)?;
            push_expr(py, &mut c, &e.dim, symbols)?;
            let base = make_expr(py, expr, SemExprKind::SizeofCall, c, None, None, None, None)?;
            Py::new(py, (SemSizeofCallExpr, base))?.into_any()
        }
        K::DurationofCall(e) => {
            let mut c = Vec::new();
            push_stmt_list(py, &mut c, &e.scope.stmts, symbols)?;
            let base = make_expr(
                py,
                expr,
                SemExprKind::DurationofCall,
                c,
                None,
                None,
                None,
                None,
            )?;
            Py::new(py, (SemDurationofCallExpr, base))?.into_any()
        }
        K::Concat(e) => {
            let mut c = Vec::new();
            push_expr_list(py, &mut c, &e.operands, symbols)?;
            let base = make_expr(py, expr, SemExprKind::Concat, c, None, None, None, None)?;
            Py::new(py, (SemConcatExpr, base))?.into_any()
        }
    };
    Ok(obj)
}

// ----------------------------------------------------------------------------
// Child-collection helpers
// ----------------------------------------------------------------------------

fn push_expr(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    expr: &sem::Expr,
    symbols: &CoreSymbolTable,
) -> PyResult<()> {
    out.push(expr_to_py(py, expr, symbols)?);
    Ok(())
}

fn push_opt_expr(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    expr: Option<&sem::Expr>,
    symbols: &CoreSymbolTable,
) -> PyResult<()> {
    if let Some(expr) = expr {
        out.push(expr_to_py(py, expr, symbols)?);
    }
    Ok(())
}

fn push_expr_list(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    exprs: &[Box<sem::Expr>],
    symbols: &CoreSymbolTable,
) -> PyResult<()> {
    for expr in exprs {
        out.push(expr_to_py(py, expr, symbols)?);
    }
    Ok(())
}

fn push_stmt(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    stmt: &sem::Stmt,
    symbols: &CoreSymbolTable,
) -> PyResult<()> {
    out.push(stmt_to_py(py, stmt, symbols)?);
    Ok(())
}

fn push_stmt_list(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    stmts: &[Box<sem::Stmt>],
    symbols: &CoreSymbolTable,
) -> PyResult<()> {
    for stmt in stmts {
        out.push(stmt_to_py(py, stmt, symbols)?);
    }
    Ok(())
}

fn gate_operand_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    operand: &sem::GateOperand,
    symbols: &CoreSymbolTable,
) -> PyResult<()> {
    if let sem::GateOperandKind::Expr(expr) = &operand.kind {
        push_expr(py, out, expr, symbols)?;
    }
    Ok(())
}

fn index_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    index: &sem::Index,
    symbols: &CoreSymbolTable,
) -> PyResult<()> {
    match index {
        sem::Index::Expr(expr) => push_expr(py, out, expr, symbols),
        sem::Index::Range(range) => range_exprs(py, out, range, symbols),
    }
}

fn range_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    range: &sem::Range,
    symbols: &CoreSymbolTable,
) -> PyResult<()> {
    push_opt_expr(py, out, range.start.as_ref(), symbols)?;
    push_opt_expr(py, out, range.step.as_ref(), symbols)?;
    push_opt_expr(py, out, range.end.as_ref(), symbols)
}

fn enumerable_set_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    set: &sem::EnumerableSet,
    symbols: &CoreSymbolTable,
) -> PyResult<()> {
    match set {
        sem::EnumerableSet::Set(set) => push_expr_list(py, out, &set.values, symbols),
        sem::EnumerableSet::Range(range) => range_exprs(py, out, range, symbols),
        sem::EnumerableSet::Expr(expr) => push_expr(py, out, expr, symbols),
    }
}

fn gate_modifier_exprs(
    py: Python<'_>,
    out: &mut Vec<Py<PyAny>>,
    modifier: &sem::QuantumGateModifier,
    symbols: &CoreSymbolTable,
) -> PyResult<()> {
    match &modifier.kind {
        sem::GateModifierKind::Inv => Ok(()),
        sem::GateModifierKind::Pow(expr)
        | sem::GateModifierKind::Ctrl(expr)
        | sem::GateModifierKind::NegCtrl(expr) => push_expr(py, out, expr, symbols),
    }
}

fn literal_value(py: Python<'_>, lit: Option<&sem::LiteralKind>) -> PyResult<Option<Py<PyAny>>> {
    match lit {
        Some(lit) => literal_kind_value(py, lit),
        None => Ok(None),
    }
}

fn literal_kind_value(py: Python<'_>, lit: &sem::LiteralKind) -> PyResult<Option<Py<PyAny>>> {
    let value: Py<PyAny> = match lit {
        sem::LiteralKind::Array(_) => return Ok(None),
        sem::LiteralKind::Bool(b) | sem::LiteralKind::Bit(b) => b.into_py_any(py)?,
        sem::LiteralKind::Int(i) => i.into_py_any(py)?,
        sem::LiteralKind::Float(f) => f.into_py_any(py)?,
        sem::LiteralKind::Complex(value) => PyComplex::from_doubles(py, value.real, value.imag)
            .into_any()
            .unbind(),
        sem::LiteralKind::Angle(angle) => angle.to_string().into_py_any(py)?,
        sem::LiteralKind::Duration(duration) => duration.to_string().into_py_any(py)?,
        sem::LiteralKind::Bitstring(value, width) => {
            let width = *width as usize;
            format!("{:0>width$}", value.to_str_radix(2)).into_py_any(py)?
        }
        sem::LiteralKind::BigInt(value) => bigint_to_py(py, &value.to_str_radix(10))?,
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
