// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::VecDeque;
use std::fmt::Write;
use std::iter::once;
use std::ops::ShlAssign;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;
use std::vec;

use super::const_eval::ConstEvalError;
use super::symbols::ScopeKind;
use super::types::Type;
use super::types::binop_requires_asymmetric_angle_op;
use super::types::binop_requires_int_conversion_for_type;
use super::types::binop_requires_symmetric_uint_conversion;
use super::types::is_complex_binop_supported;
use super::types::promote_to_uint_ty;
use super::types::promote_width;
use super::types::requires_symmetric_conversion;
use super::types::try_promote_with_casting;
use super::types::types_equal_except_const;
use super::types::unary_op_can_be_applied_to_type;
use num_bigint::BigInt;
use num_traits::FromPrimitive;
use num_traits::Num;
use qsc_data_structures::span::Span;
use qsc_frontend::{compile::SourceMap, error::WithSource};
use rustc_hash::FxHashMap;

use super::symbols::{IOKind, Symbol, SymbolTable};

use crate::convert::safe_i64_to_f64;
use crate::parser::QasmSource;
use crate::parser::ast::List;
use crate::parser::ast::list_from_iter;
use crate::semantic::ast::Expr;
use crate::semantic::passes::DurationAccumulator;
use crate::semantic::symbols::SymbolResult;
use crate::semantic::types::base_types_equal;
use crate::semantic::types::binary_op_is_supported_for_types;
use crate::semantic::types::both_types_are_duration_subtypes;
use crate::semantic::types::can_cast_literal;
use crate::semantic::types::can_cast_literal_with_value_knowledge;
use crate::semantic::types::either_type_is_duration_subtype;
use crate::semantic::types::relax_constness;
use crate::semantic::types::type_is_duration_subtype;
use crate::stdlib::angle::Angle;
use crate::stdlib::builtin_functions;
use crate::stdlib::complex::Complex;
use crate::stdlib::duration::Duration;

use super::ast as semantic;
use crate::parser::ast as syntax;

use super::{
    SemanticErrorKind,
    ast::{Stmt, Version},
};

/// Macro to create an error expression. Used when we fail to
/// lower an expression. It is assumed that an error was
/// already reported.
macro_rules! err_expr {
    ($ty:expr) => {
        semantic::Expr::new(Span::default(), semantic::ExprKind::Err, $ty)
    };

    ($ty:expr, $span:expr) => {
        semantic::Expr::new($span, semantic::ExprKind::Err, $ty)
    };
}

const SWITCH_MINIMUM_SUPPORTED_VERSION: semantic::Version = semantic::Version {
    major: 3,
    minor: Some(1),
    span: Span { lo: 0, hi: 0 },
};

const QASM2_VERSION: semantic::Version = semantic::Version {
    major: 2,
    minor: Some(0),
    span: Span { lo: 0, hi: 0 },
};

const QASM3_STDGATES_INC: &str = "stdgates.inc";
const QASM2_STDGATES_INC: &str = "qelib1.inc";

const QASM3_STDGATES: &[&str] = &[
    "p", "x", "y", "z", "h", "ch", "s", "sdg", "t", "tdg", "sx", "rx", "ry", "rz", "crx", "cry",
    "crz", "cx", "cy", "cz", "cp", "swap", "cswap", "ccx", "cu", "CX", "phase", "cphase", "id",
    "u1", "u2", "u3",
];

const QASM2_STDGATES: &[&str] = &[
    "u3", "u2", "u1", "cx", "id", "x", "y", "z", "h", "s", "sdg", "t", "tdg", "rx", "ry", "rz",
    "cz", "cy", "ch", "ccx", "crz", "cu1", "cu3",
];

pub(crate) struct Lowerer {
    /// The root QASM source to compile.
    pub source: QasmSource,
    /// The source map of QASM sources for error reporting.
    pub source_map: SourceMap,
    pub errors: Vec<WithSource<crate::Error>>,
    /// The file stack is used to track the current file for error reporting.
    /// When we include a file, we push the file path to the stack and pop it
    /// when we are done with the file.
    /// This allows us to report errors with the correct file path.
    pub symbols: SymbolTable,
    /// The version of the QASM source. Used to determine the base gate set
    /// and other features.
    pub version: Option<Version>,
    pub stmts: Vec<Stmt>,
    pub pragmas: Vec<semantic::Pragma>,
}

impl Lowerer {
    pub fn new(source: QasmSource, source_map: SourceMap) -> Self {
        // do a quick check for the version to set up the symbol table
        // lowering and validation come later
        let version = source.program().version;
        let symbols = if let Some(version) = version {
            if version.major == 2 && version.minor == Some(0) {
                SymbolTable::new_qasm2()
            } else {
                SymbolTable::default()
            }
        } else {
            SymbolTable::default()
        };

        // we don't set the version here, as we need to check
        // for allowed version during lowering
        let version = None;
        let stmts = Vec::new();
        let errors = Vec::new();
        Self {
            source,
            source_map,
            errors,
            symbols,
            version,
            stmts,
            pragmas: Vec::new(),
        }
    }

    pub fn lower(mut self) -> crate::semantic::QasmSemanticParseResult {
        // Should we fail if we see a version in included files?
        let source = &self.source.clone();
        self.version = self.lower_version(source.program().version);

        self.lower_source(source);

        assert!(
            self.symbols.is_current_scope_global(),
            "scope stack was non popped correctly"
        );

        let program = semantic::Program {
            version: self.version,
            statements: syntax::list_from_iter(self.stmts),
            pragmas: syntax::list_from_iter(self.pragmas),
        };

        super::QasmSemanticParseResult {
            source: self.source,
            source_map: self.source_map,
            symbols: self.symbols,
            program,
            errors: self.errors,
        }
    }

    fn lower_version(&mut self, version: Option<syntax::Version>) -> Option<Version> {
        if let Some(version) = version {
            if version.major == 2 && version.minor == Some(0) {
                return Some(QASM2_VERSION);
            }
            if version.major != 3 {
                self.push_semantic_error(SemanticErrorKind::UnsupportedVersion(
                    format!("{version}"),
                    version.span,
                ));
            } else if let Some(minor) = version.minor {
                if minor != 0 && minor != 1 {
                    self.push_semantic_error(SemanticErrorKind::UnsupportedVersion(
                        format!("{version}"),
                        version.span,
                    ));
                }
            }
            return Some(crate::semantic::ast::Version {
                span: version.span,
                major: version.major,
                minor: version.minor,
            });
        }
        None
    }

    /// Root recursive function for lowering the source.
    fn lower_source(&mut self, source: &QasmSource) {
        // we keep an iterator of the includes so we can match them with the
        // source includes. The include statements only have the path, but
        // we have already loaded all of source files in the
        // `source.includes()`
        let mut includes = source.includes().iter();

        for stmt in &source.program().statements {
            match &*stmt.kind {
                syntax::StmtKind::Include(include) => {
                    // if we are not in the root  we should not be able to include
                    // as this is a limitation of the QASM3 language
                    if !self.symbols.is_current_scope_global() {
                        let kind = SemanticErrorKind::IncludeNotInGlobalScope(
                            include.filename.to_string(),
                            include.span,
                        );
                        self.push_semantic_error(kind);
                        continue;
                    }

                    // special case for stdgates.inc
                    // it won't be in the includes list
                    if include.filename.to_lowercase() == QASM3_STDGATES_INC {
                        if self.version == Some(QASM2_VERSION) {
                            self.push_semantic_error(
                                SemanticErrorKind::IncludeNotInLanguageVersion(
                                    include.filename.to_string(),
                                    "3.0".to_string(),
                                    include.span,
                                ),
                            );
                        }
                        self.define_stdgates(include.span);
                        continue;
                    } else if include.filename.to_lowercase() == QASM2_STDGATES_INC {
                        if self.version != Some(QASM2_VERSION) {
                            self.push_semantic_error(
                                SemanticErrorKind::IncludeNotInLanguageVersion(
                                    include.filename.to_string(),
                                    "2.0".to_string(),
                                    include.span,
                                ),
                            );
                        }
                        self.define_qelib1_gates(include.span);
                        continue;
                    } else if include.filename.to_lowercase() == "qdk.inc" {
                        self.define_mresetzchecked();
                        continue;
                    }

                    let include = includes.next().expect("missing include");
                    self.lower_source(include);
                }
                syntax::StmtKind::Pragma(stmt) => {
                    let pragma = Self::lower_pragma(stmt);
                    self.pragmas.push(pragma);
                }
                _ => {
                    let mut stmts = self.lower_stmt(stmt);
                    self.stmts.append(&mut stmts);
                }
            }
        }
    }

    fn lower_stmt(&mut self, stmt: &syntax::Stmt) -> Vec<semantic::Stmt> {
        let annotations = list_from_iter(Self::lower_annotations(&stmt.annotations));
        let kind = match &*stmt.kind {
            syntax::StmtKind::Alias(stmt) => self.lower_alias(stmt),
            syntax::StmtKind::Assign(stmt) => self.lower_assign_stmt(stmt),
            syntax::StmtKind::AssignOp(stmt) => self.lower_assign_op_stmt(stmt),
            syntax::StmtKind::Barrier(stmt) => self.lower_barrier_stmt(stmt),
            syntax::StmtKind::Box(stmt) => self.lower_box(stmt),
            syntax::StmtKind::Break(stmt) => self.lower_break(stmt),
            syntax::StmtKind::Block(stmt) => {
                semantic::StmtKind::Block(Box::new(self.lower_block(stmt)))
            }
            syntax::StmtKind::Cal(stmt) => Self::lower_calibration(stmt),
            syntax::StmtKind::CalibrationGrammar(stmt) => Self::lower_calibration_grammar(stmt),
            syntax::StmtKind::ClassicalDecl(stmt) => self.lower_classical_decl(stmt),
            syntax::StmtKind::ConstDecl(stmt) => self.lower_const_decl(stmt),
            syntax::StmtKind::Continue(stmt) => self.lower_continue_stmt(stmt),
            syntax::StmtKind::Def(stmt) => self.lower_def(stmt),
            syntax::StmtKind::DefCal(stmt) => Self::lower_def_cal(stmt),
            syntax::StmtKind::Delay(stmt) => self.lower_delay(stmt),
            syntax::StmtKind::End(stmt) => Self::lower_end_stmt(stmt),
            syntax::StmtKind::ExprStmt(stmt) => self.lower_expr_stmt(stmt),
            syntax::StmtKind::ExternDecl(extern_decl) => self.lower_extern(extern_decl),
            syntax::StmtKind::For(stmt) => self.lower_for_stmt(stmt),
            syntax::StmtKind::If(stmt) => self.lower_if_stmt(stmt),
            syntax::StmtKind::GateCall(stmt) => {
                return self.lower_gate_call_stmts(stmt, &annotations);
            }
            syntax::StmtKind::GPhase(stmt) => {
                return self.lower_gphase_stmts(stmt, &annotations);
            }
            syntax::StmtKind::Include(stmt) => self.lower_include(stmt),
            syntax::StmtKind::IODeclaration(stmt) => self.lower_io_decl(stmt),
            syntax::StmtKind::Measure(stmt) => self.lower_measure_arrow_stmt(stmt),
            syntax::StmtKind::Pragma(..) => {
                unreachable!("pragma should be handled in lower_source")
            }
            syntax::StmtKind::QuantumGateDefinition(stmt) => self.lower_gate_def(stmt),
            syntax::StmtKind::QuantumDecl(stmt) => self.lower_quantum_decl(stmt),
            syntax::StmtKind::Reset(stmt) => self.lower_reset(stmt),
            syntax::StmtKind::Return(stmt) => self.lower_return(stmt),
            syntax::StmtKind::Switch(stmt) => self.lower_switch(stmt),
            syntax::StmtKind::WhileLoop(stmt) => self.lower_while_stmt(stmt),
            syntax::StmtKind::Err => semantic::StmtKind::Err,
        };

        vec![semantic::Stmt {
            span: stmt.span,
            annotations,
            kind: Box::new(kind),
        }]
    }

    /// Define the `OpenQASM` 3.0 standard gates in the symbol table.
    fn define_stdgates(&mut self, span: Span) {
        fn gate_symbol(name: &str, cargs: u32, qargs: u32) -> Symbol {
            Symbol::new(
                name,
                Span::default(),
                Type::Gate(cargs, qargs),
                Default::default(),
                Default::default(),
            )
        }
        let gates = vec![
            gate_symbol("p", 1, 1),
            gate_symbol("x", 0, 1),
            gate_symbol("y", 0, 1),
            gate_symbol("z", 0, 1),
            gate_symbol("h", 0, 1),
            gate_symbol("ch", 0, 2),
            gate_symbol("s", 0, 1),
            gate_symbol("sdg", 0, 1),
            gate_symbol("t", 0, 1),
            gate_symbol("tdg", 0, 1),
            gate_symbol("sx", 0, 1),
            gate_symbol("rx", 1, 1),
            gate_symbol("ry", 1, 1),
            gate_symbol("rz", 1, 1),
            gate_symbol("crx", 1, 2),
            gate_symbol("cry", 1, 2),
            gate_symbol("crz", 1, 2),
            gate_symbol("cx", 0, 2),
            gate_symbol("cy", 0, 2),
            gate_symbol("cz", 0, 2),
            gate_symbol("cp", 1, 2),
            gate_symbol("swap", 0, 2),
            gate_symbol("cswap", 0, 3),
            gate_symbol("ccx", 0, 3),
            gate_symbol("cu", 4, 2),
            gate_symbol("CX", 0, 2),
            gate_symbol("phase", 1, 1),
            gate_symbol("cphase", 1, 2),
            gate_symbol("id", 0, 1),
            gate_symbol("u1", 1, 1),
            gate_symbol("u2", 2, 1),
            gate_symbol("u3", 3, 1),
        ];
        for gate in gates {
            let name = gate.name.clone();
            if self.symbols.insert_symbol(gate).is_err() {
                self.push_redefined_symbol_error(name.as_str(), span);
            }
        }
    }

    /// Define the `OpenQASM` 2.0 standard gates in the symbol table.
    fn define_qelib1_gates(&mut self, span: Span) {
        fn gate_symbol(name: &str, cargs: u32, qargs: u32) -> Symbol {
            Symbol::new(
                name,
                Span::default(),
                Type::Gate(cargs, qargs),
                Default::default(),
                Default::default(),
            )
        }

        let gates = vec![
            // --- QE Hardware primitives ---
            gate_symbol("u3", 3, 1),
            gate_symbol("u2", 2, 1),
            gate_symbol("u1", 1, 1),
            gate_symbol("cx", 0, 2),
            gate_symbol("id", 0, 1),
            // --- QE Standard Gates ---
            gate_symbol("x", 0, 1),
            gate_symbol("y", 0, 1),
            gate_symbol("z", 0, 1),
            gate_symbol("h", 0, 1),
            gate_symbol("s", 0, 1),
            gate_symbol("sdg", 0, 1),
            gate_symbol("t", 0, 1),
            gate_symbol("tdg", 0, 1),
            // --- Standard rotations ---
            gate_symbol("rx", 1, 1),
            gate_symbol("ry", 1, 1),
            gate_symbol("rz", 1, 1),
            // --- QE Standard User-Defined Gates  ---
            gate_symbol("cz", 0, 2),
            gate_symbol("cy", 0, 2),
            gate_symbol("ch", 0, 2),
            gate_symbol("ccx", 0, 3),
            gate_symbol("crz", 1, 2),
            gate_symbol("cu1", 1, 2),
            gate_symbol("cu3", 3, 2),
        ];
        for gate in gates {
            let name = gate.name.clone();
            if self.symbols.insert_symbol(gate).is_err() {
                self.push_redefined_symbol_error(name.as_str(), span);
            }
        }
    }

    fn define_mresetzchecked(&mut self) {
        let name = "mresetz_checked";
        let symbol = Symbol::new(
            name,
            Span::default(),
            Type::Function(vec![Type::Qubit].into(), Type::Int(None, false).into()),
            Span::default(),
            Default::default(),
        );
        if self.symbols.insert_symbol(symbol).is_err() {
            self.push_redefined_symbol_error(name, Span::default());
        }
    }

    /// Define the Qiskit standard gates in the symbol table.
    /// Qiskit emits QASM3 that can't compile because it omits
    /// definitions for many gates that aren't included in the
    /// standard gates include file. We define them here so that
    /// the symbol table is complete and we can lower the QASM3.
    /// We must also define the gates in the `Std.OpenQASM` module so
    /// that we can compile the QASM3 to Q#.
    fn define_qiskit_standard_gate_if_needed<S>(&mut self, name: S, span: Span)
    where
        S: AsRef<str>,
    {
        const QISKIT_STDGATES: [&str; 20] = [
            "rxx",
            "ryy",
            "rzz",
            "dcx",
            "ecr",
            "r",
            "rzx",
            "cs",
            "csdg",
            "sxdg",
            "csx",
            "cu1",
            "cu3",
            "rccx",
            "c3sqrtx",
            "c3x",
            "rc3x",
            "xx_minus_yy",
            "xx_plus_yy",
            "ccz",
        ];
        // only define the gate if it is not already defined
        // and it is in the list of Qiskit standard gates
        if self.symbols.get_symbol_by_name(&name).is_err()
            && QISKIT_STDGATES.contains(&name.as_ref())
        {
            self.define_qiskit_standard_gate(name, span);
        }
    }

    fn define_qiskit_standard_gate<S>(&mut self, name: S, span: Span)
    where
        S: AsRef<str>,
    {
        fn gate_symbol(name: &str, cargs: u32, qargs: u32) -> Symbol {
            Symbol::new(
                name,
                Span::default(),
                Type::Gate(cargs, qargs),
                Default::default(),
                Default::default(),
            )
        }
        // QIR intrinsics missing from qasm std library, that Qiskit won't emit qasm defs for
        // rxx, ryy, rzz;

        // Remaining gates that are not in the qasm std library, but are standard gates in Qiskit
        // that Qiskit wont emit correctly.
        // dcx, ecr, r, rzx, cs, csdg, sxdg, csx, cu1, cu3, rccx, c3sqrtx, c3x, rc3x, xx_minus_yy, xx_plus_yy, ccz;
        let gates = FxHashMap::from_iter([
            ("rxx", gate_symbol("rxx", 1, 2)),
            ("ryy", gate_symbol("ryy", 1, 2)),
            ("rzz", gate_symbol("rzz", 1, 2)),
            ("dcx", gate_symbol("dcx", 0, 2)),
            ("ecr", gate_symbol("ecr", 0, 2)),
            ("r", gate_symbol("r", 2, 1)),
            ("rzx", gate_symbol("rzx", 1, 2)),
            ("cs", gate_symbol("cs", 0, 2)),
            ("csdg", gate_symbol("csdg", 0, 2)),
            ("sxdg", gate_symbol("sxdg", 0, 1)),
            ("csx", gate_symbol("csx", 0, 2)),
            ("cu1", gate_symbol("cu1", 1, 2)),
            ("cu3", gate_symbol("cu3", 3, 2)),
            ("rccx", gate_symbol("rccx", 0, 3)),
            ("c3sqrtx", gate_symbol("c3sqrtx", 0, 4)),
            ("c3x", gate_symbol("c3x", 0, 4)),
            ("rc3x", gate_symbol("rc3x", 0, 4)),
            ("xx_minus_yy", gate_symbol("xx_minus_yy", 2, 2)),
            ("xx_plus_yy", gate_symbol("xx_plus_yy", 2, 2)),
            ("ccz", gate_symbol("ccz", 0, 3)),
        ]);
        let gate = gates.get(name.as_ref()).expect("missing gate symbol");
        if self.symbols.insert_symbol(gate.clone()).is_err() {
            self.push_redefined_symbol_error(name.as_ref(), span);
        }
    }

    fn try_insert_or_get_existing_symbol_id<S>(
        &mut self,
        name: S,
        symbol: Symbol,
    ) -> super::symbols::SymbolId
    where
        S: AsRef<str>,
    {
        let symbol_span = symbol.span;

        match self.symbols.try_insert_or_get_existing(symbol) {
            Ok(symbol_id) => symbol_id,
            Err(symbol_id) => {
                self.push_redefined_symbol_error(name.as_ref(), symbol_span);
                symbol_id
            }
        }
    }

    fn try_get_existing_or_insert_err_symbol<S>(
        &mut self,
        name: S,
        span: Span,
    ) -> (super::symbols::SymbolId, std::rc::Rc<Symbol>)
    where
        S: AsRef<str>,
    {
        let result = self
            .symbols
            .try_get_existing_or_insert_err_symbol(name.as_ref(), span);

        match result {
            SymbolResult::Ok(symbol_id, symbol) => (symbol_id, symbol),

            // The symbol was not found.
            SymbolResult::NotFound(symbol_id, symbol) => {
                self.push_missing_symbol_error(name, span);
                (symbol_id, symbol)
            }
            // The symbol was found, but it isn't visible, because it isn't const.
            SymbolResult::NotVisible(symbol_id, symbol) => {
                self.push_semantic_error(SemanticErrorKind::ExprMustBeConst(
                    "a captured variable".into(),
                    span,
                ));
                (symbol_id, symbol)
            }
        }
    }

    /// This helper method is meant to be used when failing to lower a declaration statement
    /// and returning `StmtKind::Err` before getting to build the symbol due to insufficient
    /// type information. This usually happens when there is a const evaluation error when
    /// trying to compute the size of a sized type.
    fn try_insert_err_symbol_or_push_redefined_symbol_error<S>(&mut self, name: S, span: Span)
    where
        S: AsRef<str>,
    {
        let err_symbol = Symbol::err(name.as_ref(), span);
        let _ = self.symbols.try_insert_or_get_existing(err_symbol);
    }

    /// The let keyword allows declared quantum bits and registers to be referred to by
    /// another name as long as the alias is in scope.
    /// The lhs type is always a register whose size is calculated based on the rhs expr.
    /// except where the rhs is a single (qu)bit.
    /// ```openqasm
    /// qubit[5] q;
    /// let myreg = q[1:4];
    /// ```
    /// Here `myreg[0]` refers to the qubit `q[1]` and so on. The type of `myreg` is
    /// `qubit[3]`. Aliases can also use array concatenation of registers.
    /// ```openqasm
    /// qubit[2] one;
    /// qubit[10] two;
    /// // Aliased register of twelve qubits
    /// let concatenated = one ++ two;
    /// // First qubit in aliased qubit array
    /// let first = concatenated[0];
    /// // Qubits zero, three and five
    /// let qubit_selection = two[{0, 3, 5}];
    /// ```
    fn lower_alias(&mut self, alias: &syntax::AliasDeclStmt) -> semantic::StmtKind {
        let name = get_identifier_name(&alias.ident);

        let rhs = alias
            .exprs
            .iter()
            .map(|expr| self.lower_expr(expr))
            .collect::<Vec<_>>();

        assert!(!rhs.is_empty(), "alias must have at least one expression");

        let ty = self.get_alias_ty_from_exprs(&rhs);

        let symbol = Symbol::new(
            &name,
            alias.ident.span(),
            ty,
            alias.ident.span(),
            IOKind::Default,
        );

        let symbol_id = self.try_insert_or_get_existing_symbol_id(name, symbol);

        semantic::StmtKind::Alias(semantic::AliasDeclStmt {
            span: alias.span,
            symbol_id,
            exprs: syntax::list_from_iter(rhs),
        })
    }

    fn get_alias_ty_from_exprs(&mut self, rhs: &Vec<Expr>) -> Type {
        fn get_expr_ty_names_as_str(exprs: &[Expr]) -> String {
            exprs
                .iter()
                .map(|expr| &expr.ty)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        }

        let first_expr = rhs.first().expect("missing rhs");
        let first_ty = first_expr.ty.clone();

        // validate that the types are of the correct subset
        if rhs.len() == 1 {
            if !matches!(
                first_ty,
                Type::Qubit | Type::QubitArray(_) | Type::BitArray(..)
            ) {
                let tys = get_expr_ty_names_as_str(rhs);
                let kind = SemanticErrorKind::InvalidTypeInAlias(tys, first_expr.span);
                self.push_semantic_error(kind);
                return Type::Err;
            }
            return first_ty;
        }

        // qubit is no longer an allowed type as you can't concatenate it with other types
        let mut has_invalid_types = false;
        for expr in rhs {
            if !matches!(expr.ty, Type::QubitArray(_) | Type::BitArray(..)) {
                let tys = expr.ty.to_string();
                let kind = SemanticErrorKind::InvalidTypeInAlias(tys, expr.span);
                self.push_semantic_error(kind);
                has_invalid_types = true;
            }
        }
        if has_invalid_types {
            return Type::Err;
        }

        // We know the types are valid at this point
        // but they may be mixed together which isn't allowed.
        let all_tys = rhs.iter().map(|expr| &expr.ty).collect::<Vec<_>>();
        let all_same = all_tys.iter().all(|ty| matches!(ty, Type::QubitArray(_)))
            || all_tys.iter().all(|ty| matches!(ty, Type::BitArray(..)));
        if !all_same {
            let lo = first_expr.span.lo;
            let hi = rhs.last().expect("missing last rhs").span.hi;
            let span = Span { lo, hi };
            let tys = get_expr_ty_names_as_str(rhs);
            let kind = SemanticErrorKind::InconsistentTypesInAlias(tys, span);
            self.push_semantic_error(kind);
            return Type::Err;
        }

        let mut width = 0;
        let mut binding_is_const = true;
        for expr in rhs {
            match expr.ty {
                Type::QubitArray(size) => {
                    width += size;
                }
                Type::BitArray(size, is_const) => {
                    // in the case we are dealing with a bit array, we need to track constness
                    // relaxing is as we go.
                    binding_is_const &= is_const;
                    width += size;
                }
                _ => {
                    unreachable!("rhs expressions must be qubit arrays");
                }
            }
        }
        if matches!(first_ty, Type::BitArray(..)) {
            // if the first expression is a bit array, we need to use the bit array type
            Type::BitArray(width, binding_is_const)
        } else {
            // otherwise we use the qubit array type
            Type::QubitArray(width)
        }
    }

    fn lower_assign_stmt(&mut self, stmt: &syntax::AssignStmt) -> semantic::StmtKind {
        match &*stmt.lhs {
            syntax::IdentOrIndexedIdent::Ident(ident) => {
                self.lower_simple_assign_stmt(ident, &stmt.rhs, stmt.span)
            }
            syntax::IdentOrIndexedIdent::IndexedIdent(indexed_ident) => {
                self.lower_indexed_assign_stmt(indexed_ident, &stmt.rhs, stmt.span)
            }
        }
    }

    fn lower_simple_assign_stmt(
        &mut self,
        ident: &syntax::Ident,
        rhs: &syntax::ValueExpr,
        span: Span,
    ) -> semantic::StmtKind {
        let lhs = self.lower_ident_expr(ident);
        let ty = lhs.ty.clone();
        let rhs = match rhs {
            syntax::ValueExpr::Expr(expr) => {
                let expr = self.lower_expr(expr);
                self.cast_expr_with_target_type_or_default(Some(expr), &ty, span)
            }
            syntax::ValueExpr::Measurement(measure_expr) => {
                let expr = self.lower_measure_expr(measure_expr);
                self.cast_expr_to_type(&ty, &expr)
            }
            syntax::ValueExpr::Concat(expr) => {
                let expr = self.lower_array_concat_expr(expr);
                self.cast_expr_with_target_type_or_default(Some(expr), &ty, span)
            }
        };

        if lhs.ty.is_const() {
            let kind =
                SemanticErrorKind::CannotUpdateConstVariable(ident.name.to_string(), ident.span);
            self.push_semantic_error(kind);
            return semantic::StmtKind::Err;
        }

        if lhs.ty.is_readonly_array_ref() {
            let kind =
                SemanticErrorKind::CannotUpdateReadonlyArrayRef(ident.name.to_string(), ident.span);
            self.push_semantic_error(kind);
            return semantic::StmtKind::Err;
        }

        semantic::StmtKind::Assign(semantic::AssignStmt { span, lhs, rhs })
    }

    fn lower_indexed_assign_stmt(
        &mut self,
        indexed_ident: &syntax::IndexedIdent,
        rhs: &syntax::ValueExpr,
        span: Span,
    ) -> semantic::StmtKind {
        assert!(!indexed_ident.indices.is_empty());

        let (lhs, classical_indices) = self.lower_indexed_ident_expr(indexed_ident);

        if lhs.ty.is_const() {
            let kind = SemanticErrorKind::CannotUpdateConstVariable(
                indexed_ident.ident.name.to_string(),
                indexed_ident.ident.span,
            );
            self.push_semantic_error(kind);
            return semantic::StmtKind::Err;
        }

        if !classical_indices.is_empty() {
            let rhs = match rhs {
                syntax::ValueExpr::Expr(expr) => self.lower_expr(expr),
                syntax::ValueExpr::Measurement(measure_expr) => {
                    self.lower_measure_expr(measure_expr)
                }
                syntax::ValueExpr::Concat(expr) => self.lower_array_concat_expr(expr),
            };
            return self.lower_indexed_classical_type_assign_stmt(
                lhs,
                &rhs,
                span,
                classical_indices,
            );
        }

        let indexed_ty = &lhs.ty;
        let rhs = match rhs {
            syntax::ValueExpr::Expr(expr) => {
                let expr = self.lower_expr(expr);
                self.cast_expr_with_target_type_or_default(Some(expr), indexed_ty, span)
            }
            syntax::ValueExpr::Measurement(measure_expr) => {
                let expr = self.lower_measure_expr(measure_expr);
                self.cast_expr_to_type(indexed_ty, &expr)
            }
            syntax::ValueExpr::Concat(expr) => {
                let expr = self.lower_array_concat_expr(expr);
                self.cast_expr_with_target_type_or_default(Some(expr), indexed_ty, span)
            }
        };

        semantic::StmtKind::Assign(semantic::AssignStmt { span, lhs, rhs })
    }

    fn lower_indexed_classical_type_assign_stmt(
        &mut self,
        lhs: semantic::Expr,
        rhs: &semantic::Expr,
        span: Span,
        indices: VecDeque<semantic::Index>,
    ) -> semantic::StmtKind {
        // We need to check that we can assign the rhs to the fully indexed lhs.
        let fully_indexed_lhs = self.lower_index_expr_rec(lhs.clone(), indices.clone());
        let indexed_ty = &fully_indexed_lhs.ty;
        let Some(rhs) = Self::try_cast_expr_to_type(indexed_ty, rhs) else {
            self.push_invalid_cast_error(indexed_ty, &rhs.ty, span);
            return semantic::StmtKind::Err;
        };

        // We return the rhs already casted to the type of the fully indexed lhs.
        // So, if return here, it is guaranteed that the assignment will succeed.
        semantic::StmtKind::IndexedClassicalTypeAssign(semantic::IndexedClassicalTypeAssignStmt {
            span,
            lhs,
            indices,
            rhs,
        })
    }

    fn lower_assign_op_stmt(&mut self, stmt: &syntax::AssignOpStmt) -> semantic::StmtKind {
        match &*stmt.lhs {
            syntax::IdentOrIndexedIdent::Ident(ident) => {
                self.lower_simple_assign_op_stmt(ident, stmt.op, &stmt.rhs, stmt.span)
            }
            syntax::IdentOrIndexedIdent::IndexedIdent(indexed_ident) => {
                self.lower_indexed_assign_op_stmt(indexed_ident, stmt.op, &stmt.rhs, stmt.span)
            }
        }
    }

    /// We lower a binary-assign statement like `a += 2;` as `a = a + 2;`.
    fn lower_simple_assign_op_stmt(
        &mut self,
        ident: &syntax::Ident,
        op: syntax::BinOp,
        rhs: &syntax::ValueExpr,
        span: Span,
    ) -> semantic::StmtKind {
        let lhs = self.lower_ident_expr(ident);

        // Check that lhs can be updated.
        let ty = lhs.ty.clone();
        if ty.is_const() {
            let kind =
                SemanticErrorKind::CannotUpdateConstVariable(ident.name.to_string(), ident.span);
            self.push_semantic_error(kind);
            return semantic::StmtKind::Err;
        }

        if lhs.ty.is_readonly_array_ref() {
            let kind =
                SemanticErrorKind::CannotUpdateReadonlyArrayRef(ident.name.to_string(), ident.span);
            self.push_semantic_error(kind);
            return semantic::StmtKind::Err;
        }

        // Construct the rhs binary expression.
        let rhs = match rhs {
            syntax::ValueExpr::Expr(expr) => self.lower_expr(expr),
            syntax::ValueExpr::Measurement(measure_expr) => self.lower_measure_expr(measure_expr),
            syntax::ValueExpr::Concat(expr) => {
                let kind = SemanticErrorKind::InvalidConcatenationPosition(
                    "the rhs of assignment operation statements".to_string(),
                    expr.span,
                );
                self.push_semantic_error(kind);
                return semantic::StmtKind::Err;
            }
        };
        let binary_expr = self.lower_binary_op_expr(op, lhs.clone(), rhs, span);

        // Cast the binary expression to the type of the lhs.
        let binary_expr = self.cast_expr_with_target_type_or_default(Some(binary_expr), &ty, span);

        semantic::StmtKind::Assign(semantic::AssignStmt {
            span,
            lhs,
            rhs: binary_expr,
        })
    }

    fn lower_indexed_assign_op_stmt(
        &mut self,
        indexed_ident: &syntax::IndexedIdent,
        op: syntax::BinOp,
        rhs: &syntax::ValueExpr,
        span: Span,
    ) -> semantic::StmtKind {
        assert!(!indexed_ident.indices.is_empty());

        let (lhs, classical_indices) = self.lower_indexed_ident_expr(indexed_ident);

        // Check that lhs can be updated.
        if lhs.ty.is_const() {
            let kind = SemanticErrorKind::CannotUpdateConstVariable(
                indexed_ident.ident.name.to_string(),
                indexed_ident.ident.span,
            );
            self.push_semantic_error(kind);
            return semantic::StmtKind::Err;
        }

        if !classical_indices.is_empty() {
            let binary_expr_lhs = self.lower_index_expr_rec(lhs.clone(), classical_indices.clone());
            let binary_expr_rhs = match rhs {
                syntax::ValueExpr::Expr(expr) => self.lower_expr(expr),
                syntax::ValueExpr::Measurement(measure_expr) => {
                    self.lower_measure_expr(measure_expr)
                }
                syntax::ValueExpr::Concat(expr) => self.lower_array_concat_expr(expr),
            };
            let rhs = self.lower_binary_op_expr(op, binary_expr_lhs, binary_expr_rhs, span);
            return self.lower_indexed_classical_type_assign_stmt(
                lhs,
                &rhs,
                span,
                classical_indices,
            );
        }

        // Construct the rhs binary expression.
        let rhs = match rhs {
            syntax::ValueExpr::Expr(expr) => self.lower_expr(expr),
            syntax::ValueExpr::Measurement(measure_expr) => self.lower_measure_expr(measure_expr),
            syntax::ValueExpr::Concat(expr) => self.lower_array_concat_expr(expr),
        };
        let binary_expr = self.lower_binary_op_expr(op, lhs.clone(), rhs, span);

        // Cast the binary expression to the type of the lhs.
        let indexed_ty = &lhs.ty;
        let binary_expr =
            self.cast_expr_with_target_type_or_default(Some(binary_expr), indexed_ty, span);

        semantic::StmtKind::Assign(semantic::AssignStmt {
            span,
            lhs,
            rhs: binary_expr,
        })
    }

    fn lower_expr(&mut self, expr: &syntax::Expr) -> semantic::Expr {
        match &*expr.kind {
            syntax::ExprKind::BinaryOp(bin_op_expr) => {
                let lhs = self.lower_expr(&bin_op_expr.lhs);
                let rhs = self.lower_expr(&bin_op_expr.rhs);
                self.lower_binary_op_expr(bin_op_expr.op, lhs, rhs, expr.span)
            }
            syntax::ExprKind::Cast(expr) => self.lower_cast_expr(expr),
            syntax::ExprKind::Err => err_expr!(Type::Err, expr.span),
            syntax::ExprKind::FunctionCall(expr) => self.lower_function_call_expr(expr),
            syntax::ExprKind::Ident(ident) => self.lower_ident_expr(ident),
            syntax::ExprKind::IndexExpr(expr) => self.lower_index_expr(expr),
            syntax::ExprKind::Lit(lit) => self.lower_lit_expr(lit, None),
            syntax::ExprKind::Paren(pexpr) => self.lower_paren_expr(pexpr, expr.span),
            syntax::ExprKind::UnaryOp(expr) => self.lower_unary_op_expr(expr),
            syntax::ExprKind::DurationOf(expr) => self.lower_duration_of_expr(expr),
        }
    }

    fn lower_decl_expr(
        &mut self,
        expr: &syntax::Expr,
        ty: &Type,
        lowering_array: bool,
    ) -> semantic::Expr {
        let expr = match &*expr.kind {
            syntax::ExprKind::Lit(lit) => self.lower_lit_expr(lit, Some(ty)),
            _ => self.lower_expr(expr),
        };

        // If we are not lowering an array OR are lowering one but haven't reached a leaf node
        // we return expr without casting.
        if !lowering_array || ty.is_array() {
            expr
        } else {
            // We take this branch if we are trying to lower an array but reached a leaf node.
            self.cast_expr_to_type(ty, &expr)
        }
    }

    fn get_concat_type_from_exprs(&mut self, rhs: &Vec<Expr>) -> Type {
        fn get_expr_ty_names_as_str(exprs: &[Expr]) -> String {
            exprs
                .iter()
                .map(|expr| &expr.ty)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        }

        let first_expr = rhs
            .first()
            .expect("the parser guarantees that a concat expression has at least two operands");
        let first_ty = first_expr.ty.clone();

        let mut has_invalid_types = false;
        for expr in rhs {
            if !matches!(
                expr.ty,
                Type::Array(..) | Type::StaticArrayRef(..) | Type::DynArrayRef(..)
            ) {
                let tys = expr.ty.to_string();
                let kind = SemanticErrorKind::InvalidTypeInArrayConcatenation(tys, expr.span);
                self.push_semantic_error(kind);
                has_invalid_types = true;
            }
        }

        if has_invalid_types {
            return Type::Err;
        }

        let dummy_index = semantic::Index::Expr(semantic::Expr::int(0, Span::default()));
        let indexed_ty = self.get_indexed_type(&first_ty, Span::default(), &dummy_index);

        // We know the types are valid at this point
        // but they may be mixed together which isn't allowed.
        let all_tys = rhs.iter().map(|expr| &expr.ty).collect::<Vec<_>>();
        let all_same = all_tys.iter().all(|ty| {
            let element_indexed_ty = self.get_indexed_type(ty, Span::default(), &dummy_index);
            let indexed_ty_is_the_same = types_equal_except_const(&indexed_ty, &element_indexed_ty);
            let same_kind_of_array = matches!(
                (&first_ty, ty),
                (Type::Array(..), Type::Array(..))
                    | (Type::StaticArrayRef(..), Type::StaticArrayRef(..))
                    | (Type::DynArrayRef(..), Type::DynArrayRef(..))
            );
            indexed_ty_is_the_same && same_kind_of_array
        });

        if !all_same {
            let lo = first_expr.span.lo;
            let hi = rhs.last().expect("missing last rhs").span.hi;
            let span = Span { lo, hi };
            let tys = get_expr_ty_names_as_str(rhs);
            let kind = SemanticErrorKind::InconsistentTypesInArrayConcatenation(tys, span);
            self.push_semantic_error(kind);
            return Type::Err;
        }

        if matches!(first_ty, Type::DynArrayRef(..)) {
            return first_ty.clone();
        }

        let mut size = 0;
        for expr in rhs {
            match &expr.ty {
                Type::Array(array_ty) => {
                    if let Some(operand_width) = array_ty.dims.indexed_dim_size() {
                        size += operand_width;
                    }
                }
                Type::StaticArrayRef(array_ty) => {
                    if let Some(operand_width) = array_ty.dims.indexed_dim_size() {
                        size += operand_width;
                    }
                }
                Type::DynArrayRef(_) => {
                    // Dynamic array references were handled above.
                    unreachable!("`Type::DynArrayRef` was handled above");
                }
                _ => {
                    unreachable!("rhs expressions must be arrays");
                }
            }
        }

        let base_ty = if let Type::Array(array_ty) = &first_ty {
            array_ty.base_ty.clone().into()
        } else if let Type::StaticArrayRef(array_ty) = &first_ty {
            array_ty.base_ty.clone().into()
        } else {
            unreachable!()
        };

        // We get the array dimension of the first operand.
        let mut concat_dims: Vec<_> = first_ty
            .array_dims()
            .expect("`Type::Array` has `ArrayDimensions`")
            .into_iter()
            .collect();

        // We replace the indexed dimension by the computed size.
        // When editing code the number of dimensions can temporarily be zero if the source code is
        //   `array[int,`
        // so, we access the indexed_dimension conditionally.
        if !concat_dims.is_empty() {
            concat_dims[0] = size;
        }

        if matches!(first_ty, Type::Array(..)) {
            Type::make_array_ty(&concat_dims, &base_ty)
        } else {
            Type::make_static_array_ref_ty(&concat_dims, &base_ty, false)
        }
    }

    fn lower_array_concat_expr(&mut self, expr: &syntax::ConcatExpr) -> semantic::Expr {
        // The parser guarantees that a concat expression has at least two operands.
        assert!(expr.operands.len() >= 2);
        let operands = expr
            .operands
            .iter()
            .map(|expr| self.lower_expr(expr))
            .collect::<Vec<_>>();
        let ty = self.get_concat_type_from_exprs(&operands);
        let kind = semantic::ExprKind::Concat(semantic::ConcatExpr {
            span: expr.span,
            operands: list_from_iter(operands),
        });
        Expr::new(expr.span, kind, ty)
    }

    fn lower_cast_expr(&mut self, cast: &syntax::Cast) -> semantic::Expr {
        let cast_span = cast.span;
        let expr = self.lower_expr(&cast.arg);
        let (ty, ty_exprs) = self.get_semantic_type_from_tydef(&cast.ty, expr.ty.is_const());
        let mut cast = self.cast_expr_to_type_with_span(&ty, &expr, cast_span);

        cast.span = cast_span;
        // If lowering the cast succeeded, mark it as explicit.
        if let semantic::ExprKind::Cast(cast_ref) = cast.kind.as_mut() {
            cast_ref.kind = semantic::CastKind::Explicit;
            cast_ref.ty_exprs = list_from_iter(ty_exprs);
        }

        cast
    }

    fn lower_ident_expr(&mut self, ident: &syntax::Ident) -> semantic::Expr {
        let name = ident.name.clone();

        let (symbol_id, symbol) = self.try_get_existing_or_insert_err_symbol(&name, ident.span);

        // Design Note: The end goal of this const evaluation is to be able to compile qasm
        //              annotations as Q# attributes like `@SimulatableIntrinsic()`.
        //
        //              QASM3 subroutines and gates can be recursive and capture const symbols
        //              outside their scope. In Q#, only lambdas can capture symbols, but only
        //              proper functions and operations can be recursive or have attributes on
        //              them. To get both, annotations & recursive gates/functions and the
        //              ability to capture const symbols outside the gate/function scope, we
        //              decided to compile the gates/functions as proper Q# operations/functions
        //              and evaluate at lowering-time all references to const symbols outside
        //              the current gate/function scope.

        // This is true if we are inside any gate or function scope.
        let is_symbol_inside_gate_or_function_scope =
            self.symbols.is_scope_rooted_in_gate_or_subroutine();

        // This is true if the symbol is outside the most inner gate or function scope.
        let is_symbol_declaration_outside_gate_or_function_scope = self
            .symbols
            .is_symbol_outside_most_inner_gate_or_function_scope(symbol_id);

        let need_to_capture_symbol = is_symbol_inside_gate_or_function_scope
            && is_symbol_declaration_outside_gate_or_function_scope;

        let kind = if need_to_capture_symbol && symbol.ty.is_const() {
            if symbol.get_const_value().is_some() {
                semantic::ExprKind::CapturedIdent(symbol_id)
            } else {
                // If the const evaluation fails, we return Err but don't push
                // any additional error. The error was already pushed in the
                // const_eval function.
                semantic::ExprKind::Ident(symbol_id)
            }
        } else {
            semantic::ExprKind::Ident(symbol_id)
        };

        semantic::Expr::new(ident.span, kind, symbol.ty.clone())
    }

    fn check_lit_array_size(&mut self, expr: &syntax::Lit, ty: &Type) {
        if let Some(dims) = ty.array_dims() {
            let expected_size = dims.into_iter().next();

            let Some(expected_size) = expected_size else {
                // If we don't have at least one dimension_size, it means the dimension was Err.
                // In that case we already issued an error before.
                return;
            };

            let syntax::LiteralKind::Array(array) = &expr.kind else {
                let kind = SemanticErrorKind::ArrayDeclarationTypeError(
                    format!(
                        "expected an array of size {} but found {}",
                        expected_size, &expr.kind
                    ),
                    expr.span,
                );
                self.push_semantic_error(kind);
                return;
            };

            let actual_size = array.len();

            if actual_size != expected_size as usize {
                let kind = SemanticErrorKind::ArrayDeclarationTypeError(
                    format!(
                        "expected an array of size {expected_size} but found one of size {actual_size}"
                    ),
                    expr.span,
                );
                self.push_semantic_error(kind);
            }
        }
    }

    fn lower_lit_expr(&mut self, expr: &syntax::Lit, ty: Option<&Type>) -> semantic::Expr {
        if let Some(ty) = ty {
            self.check_lit_array_size(expr, ty);
        }

        let (kind, ty) = match &expr.kind {
            syntax::LiteralKind::BigInt(value) => {
                // this case is only valid when there is an integer literal
                // that requires more than 64 bits to represent. We should probably
                // introduce a new type for this as openqasm promotion rules don't
                // cover this case as far as I know.
                (
                    semantic::ExprKind::Lit(semantic::LiteralKind::BigInt(value.clone())),
                    Type::Err,
                )
            }
            syntax::LiteralKind::Bitstring(value, size) => (
                semantic::ExprKind::Lit(semantic::LiteralKind::Bitstring(value.clone(), *size)),
                Type::BitArray(*size, true),
            ),
            syntax::LiteralKind::Bool(value) => (
                semantic::ExprKind::Lit(semantic::LiteralKind::Bool(*value)),
                Type::Bool(true),
            ),
            syntax::LiteralKind::Int(value) => (
                semantic::ExprKind::Lit(semantic::LiteralKind::Int(*value)),
                Type::Int(None, true),
            ),
            syntax::LiteralKind::Float(value) => (
                semantic::ExprKind::Lit(semantic::LiteralKind::Float(*value)),
                Type::Float(None, true),
            ),
            syntax::LiteralKind::Imaginary(value) => (
                semantic::ExprKind::Lit(Complex::imag(*value).into()),
                Type::Complex(None, true),
            ),
            syntax::LiteralKind::String(_) => {
                self.push_unsupported_error_message("string literals", expr.span);
                (semantic::ExprKind::Err, Type::Err)
            }
            syntax::LiteralKind::Duration(value, time_unit) => (
                semantic::ExprKind::Lit(semantic::LiteralKind::Duration(Duration {
                    value: *value,
                    unit: (*time_unit).into(),
                })),
                Type::Duration(true),
            ),
            syntax::LiteralKind::Array(exprs) => {
                if let Some(ty) = ty {
                    let dummy_index = semantic::Expr::new(
                        Span::default(),
                        semantic::ExprKind::Lit(semantic::LiteralKind::Int(0)),
                        Type::Int(None, true),
                    );

                    let indexed_ty = self.get_indexed_type(
                        ty,
                        Span::default(),
                        &semantic::Index::Expr(dummy_index),
                    );

                    let texprs = exprs
                        .iter()
                        .map(|expr| self.lower_decl_expr(expr, &indexed_ty, true))
                        .collect::<Vec<_>>();

                    let Some(dims) = ty.array_dims() else {
                        return err_expr!(Type::Err, expr.span);
                    };
                    let array = semantic::Array { data: texprs, dims };
                    let kind = semantic::ExprKind::Lit(semantic::LiteralKind::Array(array));

                    (kind, ty.clone())
                } else {
                    // array literals are only valid in classical decals (const and mut)
                    // and we have to know the expected type of the array in order to lower it
                    // So we can't lower array literals in general.
                    self.push_semantic_error(SemanticErrorKind::ArrayLiteralInNonClassicalDecl(
                        expr.span,
                    ));
                    (semantic::ExprKind::Err, Type::Err)
                }
            }
        };
        semantic::Expr::new(expr.span, kind, ty)
    }

    fn lower_paren_expr(&mut self, expr: &syntax::Expr, span: Span) -> semantic::Expr {
        let expr = self.lower_expr(expr);
        let ty = expr.ty.clone();
        let kind = semantic::ExprKind::Paren(expr);
        semantic::Expr::new(span, kind, ty)
    }

    fn lower_unary_op_expr(&mut self, expr: &syntax::UnaryOpExpr) -> semantic::Expr {
        match expr.op {
            syntax::UnaryOp::Neg => {
                let op = expr.op;
                let expr = self.lower_expr(&expr.expr);
                let ty = expr.ty.clone();
                if !unary_op_can_be_applied_to_type(op, &ty) {
                    let kind = SemanticErrorKind::TypeDoesNotSupportedUnaryNegation(
                        expr.ty.to_string(),
                        expr.span,
                    );
                    self.push_semantic_error(kind);
                }
                let span = expr.span;
                let unary = semantic::UnaryOpExpr {
                    span,
                    op: semantic::UnaryOp::Neg,
                    expr,
                };
                semantic::Expr::new(span, semantic::ExprKind::UnaryOp(unary), ty)
            }
            syntax::UnaryOp::NotB => {
                let op = expr.op;
                let expr = self.lower_expr(&expr.expr);
                let ty = expr.ty.clone();
                if !unary_op_can_be_applied_to_type(op, &ty) {
                    let kind = SemanticErrorKind::TypeDoesNotSupportedUnaryNegation(
                        expr.ty.to_string(),
                        expr.span,
                    );
                    self.push_semantic_error(kind);
                }
                let span = expr.span;
                let unary = semantic::UnaryOpExpr {
                    span,
                    op: semantic::UnaryOp::NotB,
                    expr,
                };
                semantic::Expr::new(span, semantic::ExprKind::UnaryOp(unary), ty)
            }
            syntax::UnaryOp::NotL => {
                // this is the  only unary operator that tries to coerce the type
                // I can't find it in the spec, but when looking at existing code
                // it seems that the ! operator coerces to a bool if possible
                let expr = self.lower_expr(&expr.expr);
                let expr_span = expr.span;
                let target_ty = Type::Bool(expr.ty.is_const());

                let expr =
                    self.cast_expr_with_target_type_or_default(Some(expr), &target_ty, expr_span);

                let ty = expr.ty.clone();

                semantic::Expr::new(
                    expr.span,
                    semantic::ExprKind::UnaryOp(semantic::UnaryOpExpr {
                        span: expr.span,
                        op: semantic::UnaryOp::NotL,
                        expr,
                    }),
                    ty,
                )
            }
        }
    }

    fn lower_duration_of_expr(&mut self, expr: &syntax::DurationofCall) -> semantic::Expr {
        let scope = self.lower_block(&expr.scope);

        let duration = DurationAccumulator::get_duration(&scope);

        let ty = Type::Duration(true);
        let kind = semantic::ExprKind::DurationofCall(semantic::DurationofCallExpr {
            span: expr.span,
            fn_name_span: expr.name_span,
            duration,
            scope,
        });
        semantic::Expr::new(expr.span, kind, ty)
    }

    fn lower_annotations(annotations: &[Box<syntax::Annotation>]) -> Vec<semantic::Annotation> {
        annotations
            .iter()
            .map(|annotation| Self::lower_annotation(annotation))
            .collect::<Vec<_>>()
    }

    fn lower_annotation(stmt: &syntax::Annotation) -> semantic::Annotation {
        semantic::Annotation {
            span: stmt.span,
            identifier: stmt.identifier.clone(),
            value: stmt.value.clone(),
            value_span: stmt.value_span,
        }
    }

    fn lower_barrier_stmt(&mut self, stmt: &syntax::BarrierStmt) -> semantic::StmtKind {
        let qubits = stmt.qubits.iter().map(|q| self.lower_gate_operand(q));
        let qubits = list_from_iter(qubits);
        semantic::StmtKind::Barrier(semantic::BarrierStmt {
            span: stmt.span,
            qubits,
        })
    }

    /// The "boxable" stmts were taken from the reference parser at
    /// <https://github.com/openqasm/openqasm/blob/main/source/openqasm/openqasm3/ast.py>.
    /// Search for the definition of `Box` there, and then for all the classes
    /// inhereting from `QuantumStatement`.
    fn lower_box(&mut self, stmt: &syntax::BoxStmt) -> semantic::StmtKind {
        self.symbols.push_scope(ScopeKind::Block);
        let stmts = stmt
            .body
            .iter()
            .flat_map(|stmt| self.lower_stmt(stmt))
            .collect::<Vec<_>>();
        self.symbols.pop_scope();

        let mut _has_invalid_stmt_kinds = false;
        for stmt in &stmt.body {
            match &*stmt.kind {
                syntax::StmtKind::Barrier(_)
                | syntax::StmtKind::Delay(_)
                | syntax::StmtKind::Reset(_)
                | syntax::StmtKind::GateCall(_)
                | syntax::StmtKind::GPhase(_)
                | syntax::StmtKind::Box(_) => {
                    // valid statements
                }
                _ => {
                    self.push_semantic_error(SemanticErrorKind::ClassicalStmtInBox(stmt.span));
                    _has_invalid_stmt_kinds = true;
                }
            }
        }

        let duration = stmt
            .duration
            .as_ref()
            .map(|d| self.lower_duration_designator(d));

        semantic::StmtKind::Box(semantic::BoxStmt {
            span: stmt.span,
            duration,
            body: list_from_iter(stmts),
        })
    }

    fn lower_break(&mut self, stmt: &syntax::BreakStmt) -> semantic::StmtKind {
        if self.symbols.is_scope_rooted_in_loop_scope() {
            semantic::StmtKind::Break(semantic::BreakStmt { span: stmt.span })
        } else {
            self.push_semantic_error(SemanticErrorKind::InvalidScope(
                "break".into(),
                "loop".into(),
                stmt.span,
            ));
            semantic::StmtKind::Err
        }
    }

    fn lower_block(&mut self, block: &syntax::Block) -> semantic::Block {
        self.symbols.push_scope(ScopeKind::Block);
        let mut stmts = Vec::new();
        for stmt in &block.stmts {
            stmts.append(&mut self.lower_stmt(stmt));
        }
        let stmts = list_from_iter(stmts);
        self.symbols.pop_scope();

        semantic::Block {
            span: block.span,
            stmts,
        }
    }

    fn lower_calibration(stmt: &syntax::CalibrationStmt) -> semantic::StmtKind {
        semantic::StmtKind::Calibration(semantic::CalibrationStmt {
            span: stmt.span,
            content: stmt.content.clone(),
        })
    }

    fn lower_calibration_grammar(stmt: &syntax::CalibrationGrammarStmt) -> semantic::StmtKind {
        semantic::StmtKind::CalibrationGrammar(semantic::CalibrationGrammarStmt {
            span: stmt.span,
            name: stmt.name.clone(),
        })
    }

    fn lower_classical_decl(
        &mut self,
        stmt: &syntax::ClassicalDeclarationStmt,
    ) -> semantic::StmtKind {
        let is_const = false; // const decls are handled separately
        let (ty, ty_exprs) = self.get_semantic_type_from_tydef(&stmt.ty, is_const);

        // Arrays are only allowed in the global scope.
        // If we are not in the global scope, we push an error.
        if ty.is_array()
            && !matches!(ty, Type::BitArray(..))
            && !self.symbols.is_current_scope_global()
        {
            let kind = SemanticErrorKind::ArrayDeclarationInNonGlobalScope(stmt.span);
            self.push_semantic_error(kind);
        }

        let init_expr = stmt.init_expr.as_deref();
        let ty_span = stmt.ty.span();
        let stmt_span = stmt.span;
        let name = stmt.identifier.name.clone();
        let mut symbol = Symbol::new(
            &name,
            stmt.identifier.span,
            ty.clone(),
            ty_span,
            IOKind::Default,
        );

        // process the symbol and init_expr gathering any errors
        let mut init_expr = match init_expr {
            Some(expr) => match expr {
                syntax::ValueExpr::Expr(expr) => {
                    let expr = self.lower_decl_expr(expr, &ty, false);
                    self.cast_expr_with_target_type_or_default(Some(expr), &ty, stmt_span)
                }
                syntax::ValueExpr::Measurement(measure_expr) => {
                    let expr = self.lower_measure_expr(measure_expr);
                    self.cast_expr_to_type(&ty, &expr)
                }
                syntax::ValueExpr::Concat(expr) => {
                    let expr = self.lower_array_concat_expr(expr);
                    self.cast_expr_with_target_type_or_default(Some(expr), &ty, stmt_span)
                }
            },
            None => self.cast_expr_with_target_type_or_default(None, &ty, stmt_span),
        };

        // If the type is a stretch, we need to evaluate the init_expr
        // to get the const value and set it in the symbol.
        // This is because stretch can only be const.
        // ideally you wouldn't be able to define stretches without being a const
        // decl or making it a semantic error, but all existing code uses
        // stretches without the const qualifier.
        if matches!(ty, Type::Stretch(..)) {
            init_expr = init_expr.with_const_value(self);
            symbol = symbol.with_const_expr(Rc::new(init_expr.clone()));
        }

        let symbol_id = self.try_insert_or_get_existing_symbol_id(name, symbol);

        semantic::StmtKind::ClassicalDecl(semantic::ClassicalDeclarationStmt {
            span: stmt_span,
            ty_span,
            symbol_id,
            ty_exprs: list_from_iter(ty_exprs),
            init_expr: Box::new(init_expr),
        })
    }

    fn lower_const_decl(&mut self, stmt: &syntax::ConstantDeclStmt) -> semantic::StmtKind {
        let is_const = true;
        let (ty, ty_exprs) = self.get_semantic_type_from_tydef(&stmt.ty, is_const);
        let ty_span = stmt.ty.span();
        let name = stmt.identifier.name.clone();
        let mut init_expr = match &stmt.init_expr {
            syntax::ValueExpr::Expr(expr) => {
                let expr = self.lower_decl_expr(expr, &ty, false);
                self.cast_expr_with_target_type_or_default(Some(expr), &ty, stmt.span)
            }
            syntax::ValueExpr::Measurement(measure_expr) => self.lower_measure_expr(measure_expr),
            syntax::ValueExpr::Concat(expr) => {
                let expr = self.lower_array_concat_expr(expr);
                self.cast_expr_with_target_type_or_default(Some(expr), &ty, stmt.span)
            }
        };

        let mut symbol = Symbol::new(
            &name,
            stmt.identifier.span,
            ty.clone(),
            ty_span,
            IOKind::Default,
        );

        if init_expr.ty.is_const() {
            init_expr = init_expr.with_const_value(self);
            symbol = symbol.with_const_expr(Rc::new(init_expr.clone()));
        }

        let symbol_id = self.try_insert_or_get_existing_symbol_id(name, symbol);

        if !init_expr.ty.is_err() && !init_expr.ty.is_const() {
            self.push_semantic_error(SemanticErrorKind::ExprMustBeConst(
                "const decl init expr".to_string(),
                init_expr.span,
            ));
        }

        semantic::StmtKind::ClassicalDecl(semantic::ClassicalDeclarationStmt {
            span: stmt.span,
            ty_span,
            symbol_id,
            ty_exprs: list_from_iter(ty_exprs),
            init_expr: Box::new(init_expr),
        })
    }

    fn lower_continue_stmt(&mut self, stmt: &syntax::ContinueStmt) -> semantic::StmtKind {
        if self.symbols.is_scope_rooted_in_loop_scope() {
            semantic::StmtKind::Continue(semantic::ContinueStmt { span: stmt.span })
        } else {
            self.push_semantic_error(SemanticErrorKind::InvalidScope(
                "continue".into(),
                "loop".into(),
                stmt.span,
            ));
            semantic::StmtKind::Err
        }
    }

    fn lower_def(&mut self, stmt: &syntax::DefStmt) -> semantic::StmtKind {
        // 1. Check that we are in the global scope. QASM3 semantics
        //    only allow def declarations in the global scope.
        if !self.symbols.is_current_scope_global() {
            let kind = SemanticErrorKind::DefDeclarationInNonGlobalScope(stmt.span);
            self.push_semantic_error(kind);
        }

        // 2. Build the parameter's type.
        let mut param_types = Vec::with_capacity(stmt.params.len());
        let mut param_symbols = Vec::with_capacity(stmt.params.len());
        let mut param_ty_exprs = Vec::with_capacity(stmt.params.len());

        for param in &stmt.params {
            let (symbol, ty_exprs) = self.lower_typed_parameter(param);
            param_types.push(symbol.ty.clone());
            param_symbols.push(symbol);
            param_ty_exprs.push(ty_exprs);
        }

        // 3. Build the return type.
        let (return_ty, return_ty_exprs) = if let Some(ty) = &stmt.return_type {
            let tydef = syntax::TypeDef::Scalar(*ty.clone());
            let (ty, ty_exprs) = self.get_semantic_type_from_tydef(&tydef, false);
            (ty, ty_exprs)
        } else {
            (crate::semantic::types::Type::Void, Vec::new())
        };

        // 2. Push the function symbol to the symbol table.
        let name = stmt.name.name.clone();
        let name_span = stmt.name.span;
        let ty =
            crate::semantic::types::Type::Function(param_types.into(), return_ty.clone().into());

        let has_qubit_params = stmt
            .params
            .iter()
            .any(|arg| matches!(&*arg.ty, syntax::DefParameterType::Qubit(..)));

        // Check that the name isn't a builtin function.
        let symbol_id = if BuiltinFunction::from_str(&name).is_ok() {
            self.push_semantic_error(SemanticErrorKind::RedefinedBuiltinFunction(
                name.as_ref().to_string(),
                stmt.name.span,
            ));
            None
        } else {
            let symbol = Symbol::new(&name, name_span, ty, Span::default(), IOKind::Default);
            Some(self.try_insert_or_get_existing_symbol_id(name, symbol))
        };

        // If the name is a builtin function we still lower the body of the `def` to provide
        // the user with as much feedback as possible.

        // Push the scope where the def lives.
        self.symbols
            .push_scope(ScopeKind::Function(return_ty.into()));

        let params = param_symbols
            .into_iter()
            .zip(param_ty_exprs)
            .map(|(symbol, ty_exprs)| {
                let name = symbol.name.clone();
                let span = symbol.span;
                let symbol_id = self.try_insert_or_get_existing_symbol_id(name, symbol);
                let ty_exprs = list_from_iter(ty_exprs);
                semantic::DefParameter {
                    span,
                    symbol_id,
                    ty_exprs,
                }
            })
            .collect::<Vec<_>>();

        let mut stmts = Vec::new();
        for stmt in &stmt.body.stmts {
            stmts.append(&mut self.lower_stmt(stmt));
        }

        let body = semantic::Block {
            span: stmt.body.span,
            stmts: list_from_iter(stmts),
        };

        // Pop the scope where the def lives.
        self.symbols.pop_scope();

        let return_type_span = stmt
            .return_type
            .as_ref()
            .map_or_else(Span::default, |ty| ty.span);
        if let Some(return_ty) = &stmt.return_type {
            self.check_that_def_returns_in_all_code_paths(&body, return_ty.span);
        }

        // If the name was a builtin function we return `StmtKind::Err`.
        let Some(symbol_id) = symbol_id else {
            return semantic::StmtKind::Err;
        };

        semantic::StmtKind::Def(semantic::DefStmt {
            span: stmt.span,
            symbol_id,
            has_qubit_params,
            params: list_from_iter(params),
            body,
            return_type_span,
            return_ty_exprs: list_from_iter(return_ty_exprs),
        })
    }

    fn check_that_def_returns_in_all_code_paths(&mut self, body: &semantic::Block, span: Span) {
        if !Self::block_always_returns(&body.stmts) {
            self.push_semantic_error(SemanticErrorKind::NonVoidDefShouldAlwaysReturn(span));
        }
    }

    fn block_always_returns<'a>(stmts: impl IntoIterator<Item = &'a Box<semantic::Stmt>>) -> bool {
        for stmt in stmts {
            if Self::stmt_always_returns(stmt) {
                return true;
            }
        }
        false
    }

    fn stmt_always_returns(stmt: &semantic::Stmt) -> bool {
        match &*stmt.kind {
            semantic::StmtKind::Block(block) => Self::block_always_returns(&block.stmts),
            semantic::StmtKind::Box(stmt) => Self::block_always_returns(&stmt.body),
            semantic::StmtKind::If(stmt) => {
                if let Some(else_body) = &stmt.else_body {
                    Self::stmt_always_returns(&stmt.if_body) && Self::stmt_always_returns(else_body)
                } else {
                    false
                }
            }
            // We don't know if the user's switch is exhaustive.
            // We take a best effort approach and check if all the branches always return.
            semantic::StmtKind::Switch(stmt) => {
                let mut all_cases_return = true;
                for case in &stmt.cases {
                    all_cases_return &= Self::block_always_returns(&case.block.stmts);
                }
                if let Some(default_case) = &stmt.default {
                    all_cases_return &= Self::block_always_returns(&default_case.stmts);
                }
                all_cases_return
            }
            // We don't know if the iterable of the loop is empty at compiletime.
            // We take a best effort approach and check if the body always returns.
            semantic::StmtKind::For(stmt) => Self::stmt_always_returns(&stmt.body),
            semantic::StmtKind::WhileLoop(stmt) => Self::stmt_always_returns(&stmt.body),
            semantic::StmtKind::Return(..) | semantic::StmtKind::End(..) => true,
            _ => false,
        }
    }

    fn lower_typed_parameter(&mut self, typed_param: &syntax::DefParameter) -> (Symbol, Vec<Expr>) {
        let (ty, ty_exprs) = match &*typed_param.ty {
            syntax::DefParameterType::ArrayReference(ty) => {
                let tydef = syntax::TypeDef::ArrayReference(ty.clone());
                self.get_semantic_type_from_tydef(&tydef, false)
            }
            syntax::DefParameterType::Qubit(ty) => self.lower_qubit_type(ty),
            syntax::DefParameterType::Scalar(ty) => {
                let tydef = syntax::TypeDef::Scalar(ty.clone());
                self.get_semantic_type_from_tydef(&tydef, false)
            }
        };

        (
            Symbol::new(
                &typed_param.ident.name,
                typed_param.ident.span,
                ty,
                typed_param.ty.span(),
                IOKind::Default,
            ),
            ty_exprs,
        )
    }

    fn lower_qubit_type(
        &mut self,
        typed_param: &syntax::QubitType,
    ) -> (crate::semantic::types::Type, Vec<Expr>) {
        if let Some(size) = &typed_param.size {
            let size = self.const_eval_array_size_designator_expr(size);
            if let Some(size_expr) = size {
                let size = size_expr
                    .get_const_u32()
                    .expect("const evaluation succeeded");
                (
                    crate::semantic::types::Type::QubitArray(size),
                    vec![size_expr],
                )
            } else {
                (crate::semantic::types::Type::Err, Vec::new())
            }
        } else {
            (crate::semantic::types::Type::Qubit, Vec::new())
        }
    }

    fn lower_def_cal(stmt: &syntax::DefCalStmt) -> semantic::StmtKind {
        semantic::StmtKind::DefCal(semantic::DefCalStmt {
            span: stmt.span,
            content: stmt.content.clone(),
        })
    }

    fn lower_delay(&mut self, stmt: &syntax::DelayStmt) -> semantic::StmtKind {
        let qubits = stmt.qubits.iter().map(|q| self.lower_gate_operand(q));
        let qubits = list_from_iter(qubits);
        let duration = self.lower_duration_designator(&stmt.duration);

        semantic::StmtKind::Delay(semantic::DelayStmt {
            span: stmt.span,
            duration,
            qubits,
        })
    }

    fn lower_duration_designator(&mut self, expr: &syntax::Expr) -> Expr {
        let duration = self.lower_expr(expr);
        if duration.ty.is_const() {
            let duration = duration.with_const_value(self);
            self.validate_duration_ty_and_domain(&duration);
            duration
        } else {
            duration
        }
    }

    fn validate_duration_ty_and_domain(&mut self, expr: &Expr) {
        if type_is_duration_subtype(&expr.ty) && expr.ty.is_const() {
            if let Some(duration) = &expr.get_const_duration() {
                if duration.value < 0.0 {
                    let kind = SemanticErrorKind::DesignatorMustBePositiveDuration(expr.span);
                    self.push_semantic_error(kind);
                }
            } else {
                let kind = SemanticErrorKind::DurationMustBeKnownAtCompileTime(expr.span);
                self.push_semantic_error(kind);
            }
        } else {
            self.push_semantic_error(SemanticErrorKind::ExprMustBeDuration(expr.span));
        }
    }

    fn lower_end_stmt(stmt: &syntax::EndStmt) -> semantic::StmtKind {
        semantic::StmtKind::End(semantic::EndStmt { span: stmt.span })
    }

    fn lower_expr_stmt(&mut self, stmt: &syntax::ExprStmt) -> semantic::StmtKind {
        let expr = self.lower_expr(&stmt.expr);
        match &*expr.kind {
            semantic::ExprKind::Err => semantic::StmtKind::Err,
            semantic::ExprKind::Ident(id) => {
                let symbol = &self.symbols[*id];
                match &symbol.ty {
                    Type::Gate(..) => {
                        // gate call is missing qubits but the parser doesn't know that
                        self.push_semantic_error(SemanticErrorKind::GateCallMissingParams(
                            symbol.ty.to_string(),
                            expr.span,
                        ));
                        semantic::StmtKind::Err
                    }
                    Type::Function(..) => {
                        // function call is missing args but the parser doesn't know that
                        self.push_semantic_error(SemanticErrorKind::FuncMissingParams(
                            symbol.ty.to_string(),
                            expr.span,
                        ));
                        semantic::StmtKind::Err
                    }
                    _ => semantic::StmtKind::ExprStmt(semantic::ExprStmt {
                        span: stmt.span,
                        expr,
                    }),
                }
            }
            _ => semantic::StmtKind::ExprStmt(semantic::ExprStmt {
                span: stmt.span,
                expr,
            }),
        }
    }

    fn lower_extern(&mut self, stmt: &syntax::ExternDecl) -> semantic::StmtKind {
        // 1. Check that we are in the global scope. QASM3 semantics
        //    only allow extern declarations in the global scope.
        if !self.symbols.is_current_scope_global() {
            let kind = SemanticErrorKind::ExternDeclarationInNonGlobalScope(stmt.span);
            self.push_semantic_error(kind);
        }

        // 2. Build the parameter's type.
        let mut params = Vec::with_capacity(stmt.params.len());

        for param in &stmt.params {
            let ty = self.lower_extern_param(param);
            params.push(ty);
        }

        // 2. Build the return type.
        let (return_ty, ty_span, return_ty_exprs) = if let Some(ty) = &stmt.return_type {
            let ty_span = ty.span;
            let tydef = syntax::TypeDef::Scalar(ty.clone());
            let (ty, ty_exprs) = self.get_semantic_type_from_tydef(&tydef, false);
            (ty, ty_span, ty_exprs)
        } else {
            (
                crate::semantic::types::Type::Void,
                Span::default(),
                Vec::new(),
            )
        };

        if matches!(return_ty, crate::semantic::types::Type::Stretch(..)) {
            // extern functions cannot return stretches, so we push an error.
            let kind = SemanticErrorKind::ExternDeclarationCannotReturnStretch(ty_span);
            self.push_semantic_error(kind);
        }

        // extern functions can take of any number of arguments whose types correspond to the classical types of OpenQASM.
        // However, they cannot take qubits as parameters. We don't check the param types as they
        // are parse errors.

        // we also don't check the return type as it is a parse error to have an invalid return type.

        // 3. Push the extern symbol to the symbol table.
        let name = stmt.ident.name.clone();
        let name_span = stmt.ident.span;
        let tys = params.iter().map(|f| f.0.clone()).collect::<Vec<_>>();
        let ty_exprs = params.into_iter().flat_map(|f| f.1).collect::<Vec<_>>();
        let ty = crate::semantic::types::Type::Function(tys.into(), return_ty.into());
        let symbol = Symbol::new(&name, name_span, ty, ty_span, IOKind::Default);
        let symbol_id = self.try_insert_or_get_existing_symbol_id(name, symbol);

        semantic::StmtKind::ExternDecl(semantic::ExternDecl {
            span: stmt.span,
            symbol_id,
            ty_exprs: list_from_iter(ty_exprs),
            return_ty_exprs: list_from_iter(return_ty_exprs),
        })
    }

    fn lower_extern_param(&mut self, param: &syntax::ExternParameter) -> (Type, Vec<Expr>) {
        let tydef = match param {
            syntax::ExternParameter::ArrayReference(array_reference_type, _) => {
                syntax::TypeDef::ArrayReference(array_reference_type.clone())
            }
            syntax::ExternParameter::Scalar(scalar_type, _) => {
                syntax::TypeDef::Scalar(scalar_type.clone())
            }
        };
        self.get_semantic_type_from_tydef(&tydef, false)
    }

    /// If the body is a block, lowers the block pushing a scope.
    /// If the body is a single statement, also creates a block and
    /// pushes a scope, this is needed to handle broadcasting gate calls.
    fn lower_stmt_or_block_body(&mut self, stmt: &syntax::Stmt) -> semantic::Stmt {
        if matches!(&*stmt.kind, syntax::StmtKind::Block(..)) {
            let stmts = self.lower_stmt(stmt);
            assert_eq!(stmts.len(), 1);
            stmts
                .into_iter()
                .next()
                .expect("there is exactly one element in the vector")
        } else {
            self.symbols.push_scope(ScopeKind::Block);
            let stmts = self.lower_stmt(stmt);
            self.symbols.pop_scope();

            semantic::Stmt {
                span: stmt.span,
                annotations: Box::default(),
                kind: Box::new(semantic::StmtKind::Block(Box::new(semantic::Block {
                    span: stmt.span,
                    stmts: list_from_iter(stmts),
                }))),
            }
        }
    }

    fn lower_for_stmt(&mut self, stmt: &syntax::ForStmt) -> semantic::StmtKind {
        let set_declaration = self.lower_enumerable_set(&stmt.set_declaration);

        // Push scope where the loop variable lives.
        self.symbols.push_scope(ScopeKind::Loop);

        let (ty, ty_exprs) = self.get_semantic_type_from_scalar_ty(&stmt.ty, false);
        let symbol = Symbol::new(
            &stmt.ident.name,
            stmt.ident.span,
            ty.clone(),
            stmt.ty.span,
            IOKind::Default,
        );

        // This is the first variable in this scope, so
        // we don't need to check for redefined symbols.
        let symbol_id = self
            .symbols
            .insert_symbol(symbol)
            .expect("this should be the first variable in this scope");

        // We lower the body after registering the loop variable symbol_id.
        // The body of the for loop could be a single statement redefining
        // the loop variable, in which case we need to push a redefined
        // symbol error.
        let body = self.lower_stmt_or_block_body(&stmt.body);

        // Pop the scope where the loop variable lives.
        self.symbols.pop_scope();

        semantic::StmtKind::For(semantic::ForStmt {
            span: stmt.span,
            loop_variable: symbol_id,
            ty_exprs: list_from_iter(ty_exprs),
            set_declaration: Box::new(set_declaration),
            body,
        })
    }

    fn lower_if_stmt(&mut self, stmt: &syntax::IfStmt) -> semantic::StmtKind {
        let condition = self.lower_expr(&stmt.condition);

        let if_body = self.lower_stmt_or_block_body(&stmt.if_body);

        let else_body = stmt
            .else_body
            .as_ref()
            .map(|else_body| self.lower_stmt_or_block_body(else_body));

        // The semantics of a if statement is that the condition must be
        // of type bool, so we try to cast it, inserting a cast if necessary.
        let cond_ty = Type::Bool(condition.ty.is_const());
        let condition = self.cast_expr_to_type(&cond_ty, &condition);

        semantic::StmtKind::If(semantic::IfStmt {
            span: stmt.span,
            condition,
            if_body,
            else_body,
        })
    }

    fn lower_builtin_function_call_expr(
        &mut self,
        expr: &syntax::FunctionCall,
        builtin_fn: BuiltinFunction,
    ) -> semantic::Expr {
        use BuiltinFunction::*;

        let name_span = expr.name.span;
        let call_span = expr.span;
        let inputs: Vec<_> = expr
            .args
            .iter()
            .map(|e| self.lower_expr(e).with_const_value(self))
            .collect();

        let output = match builtin_fn {
            Arccos => builtin_functions::arccos(&inputs, name_span, call_span, self),
            Arcsin => builtin_functions::arcsin(&inputs, name_span, call_span, self),
            Arctan => builtin_functions::arctan(&inputs, name_span, call_span, self),
            Ceiling => builtin_functions::ceiling(&inputs, name_span, call_span, self),
            Cos => builtin_functions::cos(&inputs, name_span, call_span, self),
            Exp => builtin_functions::exp(&inputs, name_span, call_span, self),
            Floor => builtin_functions::floor(&inputs, name_span, call_span, self),
            Log => builtin_functions::log(&inputs, name_span, call_span, self),
            Mod => builtin_functions::mod_(&inputs, name_span, call_span, self),
            Popcount => builtin_functions::popcount(&inputs, name_span, call_span, self),
            Pow => builtin_functions::pow(&inputs, name_span, call_span, self),
            Rotl => builtin_functions::rotl(&inputs, name_span, call_span, self),
            Rotr => builtin_functions::rotr(&inputs, name_span, call_span, self),
            Sin => builtin_functions::sin(&inputs, name_span, call_span, self),
            Sqrt => builtin_functions::sqrt(&inputs, name_span, call_span, self),
            Tan => builtin_functions::tan(&inputs, name_span, call_span, self),
        };

        output.unwrap_or_else(|| err_expr!(Type::Err, call_span))
    }

    fn lower_sizeof_call_expr(&mut self, expr: &syntax::FunctionCall) -> semantic::Expr {
        use super::types::{ArrayType, StaticArrayRefType};
        let inputs: Vec<_> = expr.args.iter().map(|e| self.lower_expr(e)).collect();

        // Check that we have 1 or 2 arguments for sizeof.
        if inputs.is_empty() || inputs.len() > 2 {
            self.push_const_eval_error(sizeof_invalid_args_error(expr.span, &inputs));
            return err_expr!(Type::Err, expr.span);
        }

        let mut inputs_iter = inputs.clone().into_iter();

        // Get the 1st argument, and default the 2nd argument to zero if it's missing.
        let first_arg = inputs_iter.next().expect("there is at least one argument");
        let second_arg = inputs_iter
            .next()
            .unwrap_or_else(|| Expr::uint(0, expr.span));

        // The behavior of `sizeof` changes depending on the type of the first argument, the array.
        match &first_arg.ty {
            // If the first argument is an array  or an static reference. We can compute the length
            // of the requested dimension at lowering time, and the ouput is of type `const uint`.
            Type::Array(ArrayType { base_ty: _, dims })
            | Type::StaticArrayRef(StaticArrayRefType {
                base_ty: _,
                dims,
                is_mutable: _,
            }) => {
                // Check that the 2nd argument is a const expr.
                let Some(second_arg) = second_arg.with_const_value(self).get_const_u32() else {
                    self.push_const_eval_error(sizeof_invalid_args_error(expr.span, &inputs));
                    return err_expr!(Type::Err, expr.span);
                };
                let second_arg = second_arg as usize;
                let dims_vec: Vec<_> = dims.clone().into_iter().collect();

                if second_arg >= dims_vec.len() {
                    self.push_const_eval_error(ConstEvalError::SizeofInvalidDimension(
                        second_arg,
                        dims_vec.len(),
                        expr.span,
                    ));
                    return err_expr!(Type::Err, expr.span);
                }

                Expr::uint(i64::from(dims_vec[second_arg]), expr.span)
            }
            // If the first argument is a dynamic reference. We can only compute the length
            // of the requested dimension at runtime, and the ouput is of type `uint`.
            Type::DynArrayRef(ref_ty) => {
                let array_dims = ref_ty.dims;
                let kind = semantic::ExprKind::SizeofCall(semantic::SizeofCallExpr {
                    span: expr.span,
                    fn_name_span: expr.name.span,
                    array: first_arg,
                    array_dims: array_dims.into(),
                    dim: second_arg,
                });

                Expr::new(expr.span, kind, Type::UInt(None, false))
            }
            _ => {
                self.push_const_eval_error(sizeof_invalid_args_error(expr.span, &inputs));
                err_expr!(Type::Err, expr.span)
            }
        }
    }

    fn lower_function_call_expr(&mut self, expr: &syntax::FunctionCall) -> semantic::Expr {
        // 1. If the name refers to a builtin function, we defer
        //    the lowering to `lower_builtin_function_call_expr`.
        if let Ok(builtin_fn) = BuiltinFunction::from_str(&expr.name.name) {
            return self.lower_builtin_function_call_expr(expr, builtin_fn);
        }

        if &*expr.name.name == "sizeof" {
            return self.lower_sizeof_call_expr(expr);
        }

        // 2. Check that the function name actually refers to a function
        //    in the symbol table and get its symbol_id & symbol.
        let name = expr.name.name.clone();
        let name_span = expr.name.span;
        let (symbol_id, symbol) = self.try_get_existing_or_insert_err_symbol(name, name_span);

        let (params_ty, return_ty) = match &symbol.ty {
            Type::Function(params_ty, return_ty) => {
                let arity = params_ty.len();

                // 3. Check that function classical arity matches the number of classical args.
                if arity != expr.args.len() {
                    self.push_semantic_error(SemanticErrorKind::InvalidNumberOfClassicalArgs(
                        arity,
                        expr.args.len(),
                        expr.span,
                    ));
                }

                (params_ty.clone(), (**return_ty).clone())
            }
            Type::Gate(..) => {
                // The parser thinks that the gate call is a function call due to mising qubits.
                // We provide a better error message for gates that are called like functions.
                self.push_semantic_error(SemanticErrorKind::GateCalledLikeFunc(
                    symbol.ty.to_string(),
                    expr.span,
                ));
                return err_expr!(Type::Err, expr.span);
            }
            _ => {
                self.push_semantic_error(SemanticErrorKind::CannotCallNonFunction(expr.span));
                return err_expr!(Type::Err, expr.span);
            }
        };

        // 4. Lower the args.
        let mut params_ty_iter = params_ty.iter();
        let args = expr.args.iter().map(|arg| {
            let arg = self.lower_expr(arg);
            if let Some(ty) = params_ty_iter.next() {
                self.cast_expr_to_type(ty, &arg)
            } else {
                arg
            }
        });
        let args = list_from_iter(args);

        let kind = semantic::ExprKind::FunctionCall(semantic::FunctionCall {
            span: expr.span,
            fn_name_span: expr.name.span,
            symbol_id,
            args,
        });

        semantic::Expr::new(expr.span, kind, return_ty)
    }

    /// A broadcasted gate call unfolds into multiple statements.
    /// If the original gate call had annotations, we apply them
    /// to all the generated statements.
    fn lower_gate_call_stmts(
        &mut self,
        stmt: &syntax::GateCall,
        annotations: &List<semantic::Annotation>,
    ) -> Vec<semantic::Stmt> {
        let mut stmts = Vec::new();
        for kind in self.lower_gate_call_stmt(stmt) {
            let stmt = semantic::Stmt {
                span: stmt.span,
                annotations: annotations.clone(),
                kind: Box::new(kind),
            };
            stmts.push(stmt);
        }
        stmts
    }

    #[allow(clippy::too_many_lines)]
    fn lower_gate_call_stmt(&mut self, stmt: &syntax::GateCall) -> Vec<semantic::StmtKind> {
        // 1. Lower all the fields:
        //   1.1. Lower the modifiers.
        let mut modifiers = stmt
            .modifiers
            .iter()
            .filter_map(|modifier| self.lower_modifier(modifier))
            .collect::<Vec<_>>();
        // If we couldn't compute the modifiers there is no way to compile the gates
        // correctly, since we can't check its arity. In this case we return an Err.
        if modifiers.len() != stmt.modifiers.len() {
            return vec![semantic::StmtKind::Err];
        }

        //   1.3. Lower the args.
        let carg_ty = crate::semantic::types::Type::Angle(None, false);
        let args = stmt.args.iter().map(|arg| {
            let arg = self.lower_expr(arg);
            match &arg.kind.as_ref() {
                semantic::ExprKind::Lit(kind) => {
                    if can_cast_literal(&carg_ty, &arg.ty)
                        || can_cast_literal_with_value_knowledge(&carg_ty, kind)
                    {
                        self.coerce_literal_expr_to_type(&carg_ty, &arg, kind)
                    } else {
                        self.cast_expr_to_type(&carg_ty, &arg)
                    }
                }
                _ => self.cast_expr_to_type(&carg_ty, &arg),
            }
        });
        let args = list_from_iter(args);
        //   1.4. Lower the qubits.
        let qubits = stmt.qubits.iter().map(|q| self.lower_gate_operand(q));
        let qubits = list_from_iter(qubits);
        //   1.5. Lower the duration.
        let duration = stmt
            .duration
            .as_ref()
            .map(|d| self.lower_duration_designator(d));

        let name = stmt.name.name.to_string();

        // need a workaround for qiskit generating gate calls without having declared the gate
        self.define_qiskit_standard_gate_if_needed(&name, stmt.name.span);

        // 3. Check that the gate_name actually refers to a gate in the symbol table
        //    and get its symbol_id & symbol. Make sure to use the name that could've
        //    been overriden by the Q# name and the span of the original name.
        if self.symbols.get_symbol_by_name(&name).is_err() {
            if let Some(include) = self.get_include_file_defining_standard_gate(&name) {
                // The symbol is not defined, but the name is a standard gate name
                // and it is being used like a gate call. Tell the user that that they likely
                // need the appropriate include.
                self.push_semantic_error(SemanticErrorKind::StdGateCalledButNotIncluded(
                    include.to_string(),
                    stmt.span,
                ));
                return vec![semantic::StmtKind::Err];
            }
        }
        let (symbol_id, symbol) = self.try_get_existing_or_insert_err_symbol(name, stmt.name.span);

        let (classical_arity, quantum_arity) = match &symbol.ty {
            Type::Gate(classical_arity, quantum_arity) => (*classical_arity, *quantum_arity),
            Type::Function(_, _) => {
                // Symbol table says this is a function, but the parser thinks it is a gate call
                // likely due to missing parentheses. Provide a better error message for functions that are called like gates
                self.push_semantic_error(SemanticErrorKind::FuncCalledLikeGate(
                    symbol.ty.to_string(),
                    symbol.span,
                ));
                return vec![semantic::StmtKind::Err];
            }
            _ => {
                // catch all remaining cases where the symbol is not a gate.
                self.push_semantic_error(SemanticErrorKind::CannotCallNonGate(symbol.span));
                return vec![semantic::StmtKind::Err];
            }
        };

        // 4. Check that gate_call classical arity matches the number of classical args.
        if classical_arity as usize != args.len() {
            self.push_semantic_error(SemanticErrorKind::InvalidNumberOfClassicalArgs(
                classical_arity as usize,
                args.len(),
                stmt.span,
            ));
        }

        // 5. Check that gate_call quantum arity with modifiers matches the
        //    number of qubit args.
        let mut quantum_arity_with_modifiers = quantum_arity;
        for modifier in &modifiers {
            match &modifier.kind {
                semantic::GateModifierKind::Inv | semantic::GateModifierKind::Pow(_) => (),
                semantic::GateModifierKind::Ctrl(n) | semantic::GateModifierKind::NegCtrl(n) => {
                    quantum_arity_with_modifiers +=
                        n.get_const_u32().expect("const evaluation succeeded");
                }
            }
        }

        if quantum_arity_with_modifiers as usize != qubits.len() {
            self.push_semantic_error(SemanticErrorKind::InvalidNumberOfQubitArgs(
                quantum_arity_with_modifiers as usize,
                qubits.len(),
                stmt.span,
            ));
        }

        // 5.1 Reverse the modifiers to match the order expected by the compiler.
        modifiers.reverse();
        let modifiers = list_from_iter(modifiers);

        // 6. Check if we need to do broadcasting.
        let mut register_type = None;

        for qubit in &qubits {
            if let semantic::GateOperandKind::Expr(expr) = &qubit.kind {
                if matches!(expr.ty, Type::QubitArray(..)) {
                    register_type = Some(&expr.ty);
                    break;
                }
            }
        }

        // If at least one of the quantum args was a register
        // we try to do broadcasting.
        if let Some(register_type) = register_type {
            // 6.1 Check that all the quantum registers are of the same type/size.
            let mut resgisters_sizes_disagree = false;
            for qubit in &qubits {
                if let semantic::GateOperandKind::Expr(expr) = &qubit.kind {
                    if !matches!(&expr.ty, Type::Qubit)
                        && !base_types_equal(register_type, &expr.ty)
                    {
                        resgisters_sizes_disagree = true;
                        self.push_semantic_error(
                            SemanticErrorKind::BroadcastCallQuantumArgsDisagreeInSize(
                                register_type.to_string(),
                                expr.ty.to_string(),
                                expr.span,
                            ),
                        );
                    }
                }
            }

            // If the register sizes disagree, we don't have sane information
            // to generate multiple statements based on the sizes. So, we
            // return ::Err in this case.
            if resgisters_sizes_disagree {
                return vec![semantic::StmtKind::Err];
            }

            // 6.2 Convert the broadcast call in a list of simple stmts.
            let mut stmts = Vec::new();

            let Type::QubitArray(indexed_dim_size) = register_type else {
                unreachable!("we set register_type iff we find a QubitArray");
            };

            for index in 0..(*indexed_dim_size) {
                let qubits = qubits
                    .iter()
                    .map(|qubit| Self::index_into_qubit_register((**qubit).clone(), index));

                let qubits = list_from_iter(qubits);

                stmts.push(semantic::StmtKind::GateCall(semantic::GateCall {
                    span: stmt.span,
                    modifiers: modifiers.clone(),
                    symbol_id,
                    gate_name_span: stmt.name.span,
                    args: args.clone(),
                    qubits,
                    duration: duration.clone(),
                    classical_arity,
                    quantum_arity,
                }));
            }

            return stmts;
        }

        // 7. This is the base case with no broadcasting. We return:
        //   7.1. Gate symbol_id.
        //   7.2. All controls made explicit.
        //   7.3. Classical args.
        //   7.4. Quantum args in the order expected by the compiler.
        vec![semantic::StmtKind::GateCall(semantic::GateCall {
            span: stmt.span,
            modifiers,
            symbol_id,
            gate_name_span: stmt.name.span,
            args,
            qubits,
            duration,
            classical_arity,
            quantum_arity,
        })]

        // The compiler will be left to do all things that need explicit Q# knowledge.
        // But it won't need to check arities, know about implicit modifiers, or do
        // any casting of classical args. There is still some inherit complexity to
        // building a Q# gate call with this information, but it won't be cluttered
        // by all the QASM semantic analysis.
    }

    fn index_into_qubit_register(op: semantic::GateOperand, index: u32) -> semantic::GateOperand {
        let index = semantic::Index::Expr(semantic::Expr::new(
            op.span,
            semantic::ExprKind::Lit(semantic::LiteralKind::Int(index.into())),
            Type::UInt(None, true),
        ));

        match op.kind {
            semantic::GateOperandKind::Expr(expr) => {
                // Single qubits are allowed in a broadcast call.
                match &expr.ty {
                    Type::Qubit => semantic::GateOperand {
                        span: op.span,
                        kind: semantic::GateOperandKind::Expr(expr),
                    },
                    Type::QubitArray(..) => semantic::GateOperand {
                        span: op.span,
                        kind: semantic::GateOperandKind::Expr(Box::new(semantic::Expr::new(
                            op.span,
                            semantic::ExprKind::IndexedExpr(semantic::IndexedExpr {
                                span: op.span,
                                collection: expr,
                                index: Box::new(index),
                            }),
                            Type::Qubit,
                        ))),
                    },
                    _ => unreachable!("we set register_type iff we find a QubitArray"),
                }
            }
            _ => unreachable!("by this point `op` is guaranteed to be a quantum register"),
        }
    }

    /// A broadcasted gphase unfolds into multiple statements.
    /// If the original gphase had annotations, we apply them
    /// to all the generated statements.
    fn lower_gphase_stmts(
        &mut self,
        stmt: &syntax::GPhase,
        annotations: &List<semantic::Annotation>,
    ) -> Vec<semantic::Stmt> {
        let mut stmts = Vec::new();
        for kind in self.lower_gphase_stmt(stmt) {
            let stmt = semantic::Stmt {
                span: stmt.span,
                annotations: annotations.clone(),
                kind: Box::new(kind),
            };
            stmts.push(stmt);
        }
        stmts
    }

    /// This is just syntax sugar around a gate call.
    fn lower_gphase_stmt(&mut self, stmt: &syntax::GPhase) -> Vec<semantic::StmtKind> {
        let name = syntax::Ident {
            span: stmt.gphase_token_span,
            name: "gphase".into(),
        };
        let gate_call_stmt = syntax::GateCall {
            span: stmt.span,
            modifiers: stmt.modifiers.clone(),
            name,
            args: stmt.args.clone(),
            qubits: stmt.qubits.clone(),
            duration: stmt.duration.clone(),
        };
        self.lower_gate_call_stmt(&gate_call_stmt)
    }

    fn lower_modifier(
        &mut self,
        modifier: &syntax::QuantumGateModifier,
    ) -> Option<semantic::QuantumGateModifier> {
        let kind = match &modifier.kind {
            syntax::GateModifierKind::Inv => semantic::GateModifierKind::Inv,
            syntax::GateModifierKind::Pow(expr) => {
                semantic::GateModifierKind::Pow(self.lower_expr(expr))
            }
            syntax::GateModifierKind::Ctrl(expr) => {
                let ctrl_args = self.lower_modifier_ctrl_args(expr.as_ref())?;
                semantic::GateModifierKind::Ctrl(ctrl_args)
            }
            syntax::GateModifierKind::NegCtrl(expr) => {
                let ctrl_args = self.lower_modifier_ctrl_args(expr.as_ref())?;
                semantic::GateModifierKind::NegCtrl(ctrl_args)
            }
        };

        Some(semantic::QuantumGateModifier {
            span: modifier.span,
            modifier_keyword_span: modifier.modifier_keyword_span,
            kind,
        })
    }

    fn lower_modifier_ctrl_args(&mut self, expr: Option<&syntax::Expr>) -> Option<semantic::Expr> {
        let Some(expr) = expr else {
            return Some(
                semantic::Expr::new(
                    Span::default(),
                    semantic::ExprKind::Lit(semantic::LiteralKind::Int(1)),
                    Type::Int(None, true),
                )
                .with_const_value(self),
            );
        };

        let expr = self.lower_expr(expr);

        let target_ty = &Type::UInt(None, true);
        let Some(expr) = Self::try_cast_expr_to_type(target_ty, &expr) else {
            self.push_invalid_cast_error(target_ty, &expr.ty, expr.span);
            return None;
        };

        let expr = expr.with_const_value(self);

        let Some(lit) = expr.get_const_value() else {
            // const_eval must have pushed an error unless the ty is Err
            // in which case there is already an error pushed for the ty.
            return None;
        };

        let semantic::LiteralKind::Int(n) = lit else {
            // A CannotCastLiteral error was already pushed.
            return None;
        };

        if u32::try_from(n).is_err() {
            self.push_semantic_error(SemanticErrorKind::ExprMustFitInU32(
                "ctrl modifier argument".into(),
                expr.span,
            ));
            return None;
        }

        Some(expr)
    }

    /// This function is always a indication of a error. Either the
    /// program is declaring include in a non-global scope or the
    /// include is not handled in `self.lower_source` properly.
    fn lower_include(&mut self, stmt: &syntax::IncludeStmt) -> semantic::StmtKind {
        // if we are not in the root we should not be able to include
        if !self.symbols.is_current_scope_global() {
            let name = stmt.filename.to_string();
            let kind = SemanticErrorKind::IncludeNotInGlobalScope(name, stmt.span);
            self.push_semantic_error(kind);
            return semantic::StmtKind::Err;
        }
        // if we are at the root and we have an include, we should have
        // already handled it and we are in an invalid state
        panic!("include should have been handled in lower_source")
    }

    fn lower_io_decl(&mut self, stmt: &syntax::IODeclaration) -> semantic::StmtKind {
        let is_const = false;
        let (ty, ty_exprs) = self.get_semantic_type_from_tydef(&stmt.ty, is_const);
        let io_kind = stmt.io_identifier.into();

        assert!(
            io_kind == IOKind::Input || io_kind == IOKind::Output,
            "IOKind should be Input or Output"
        );

        let ty_span = stmt.ty.span();
        let stmt_span = stmt.span;
        let name = stmt.ident.name.clone();
        let symbol = Symbol::new(&name, stmt.ident.span, ty.clone(), ty_span, io_kind);

        let symbol_id = self.try_insert_or_get_existing_symbol_id(name, symbol);

        if io_kind == IOKind::Input {
            return semantic::StmtKind::InputDeclaration(semantic::InputDeclaration {
                span: stmt_span,
                symbol_id,
                ty_exprs: list_from_iter(ty_exprs),
            });
        }

        // if we have output, we need to assign a default value to declare the variable
        // if we have input, we can keep return none as we would promote the variable
        // to a parameter in the function signature once we generate the function
        let init_expr = self.get_default_value(&ty, stmt_span);
        semantic::StmtKind::OutputDeclaration(semantic::OutputDeclaration {
            span: stmt_span,
            ty_span,
            ty_exprs: list_from_iter(ty_exprs),
            symbol_id,
            init_expr: Box::new(init_expr),
        })
    }

    /// `measure q -> c;` is syntax sugar for `c = measure q;`
    fn lower_measure_arrow_stmt(&mut self, stmt: &syntax::MeasureArrowStmt) -> semantic::StmtKind {
        if let Some(target) = &stmt.target {
            self.lower_assign_stmt(&syntax::AssignStmt {
                span: stmt.span,
                lhs: Box::new(*target.clone()),
                rhs: syntax::ValueExpr::Measurement(stmt.measurement.clone()),
            })
        } else {
            let measure = self.lower_measure_expr(&stmt.measurement);
            semantic::StmtKind::ExprStmt(semantic::ExprStmt {
                span: stmt.span,
                expr: measure,
            })
        }
    }

    fn lower_pragma(stmt: &syntax::Pragma) -> semantic::Pragma {
        semantic::Pragma {
            span: stmt.span,
            identifier: stmt.identifier.clone(),
            value: stmt.value.clone(),
            value_span: stmt.value_span,
        }
    }

    fn lower_gate_def(&mut self, stmt: &syntax::QuantumGateDefinition) -> semantic::StmtKind {
        // 1. Check that we are in the global scope. QASM3 semantics
        //    only allow gate definitions in the global scope.
        if !self.symbols.is_current_scope_global() {
            let kind = SemanticErrorKind::GateDeclarationInNonGlobalScope(stmt.span);
            self.push_semantic_error(kind);
        }

        // 2. Push the gate symbol to the symbol table.
        #[allow(clippy::cast_possible_truncation)]
        let classical_arity = stmt
            .params
            .iter()
            .filter_map(|seq_item| seq_item.item_as_ref())
            .count() as u32;
        #[allow(clippy::cast_possible_truncation)]
        let quantum_arity = stmt
            .qubits
            .iter()
            .filter_map(|seq_item| seq_item.item_as_ref())
            .count() as u32;
        let name = stmt.ident.name.clone();
        let ty = crate::semantic::types::Type::Gate(classical_arity, quantum_arity);
        let symbol = Symbol::new(&name, stmt.ident.span, ty, Span::default(), IOKind::Default);
        let symbol_id = self.try_insert_or_get_existing_symbol_id(name, symbol);

        // Push the scope where the gate definition lives.
        self.symbols.push_scope(ScopeKind::Gate);

        // Design Note: If a formal parameter is missing (i.e. there are two consecutive commas and we
        //              have a missing item in the formal parameters list), we have two options:
        //                 1. Treat the missing item as if it wasn't there, and just push a parser
        //                    error saying there is a missing item. This is what Rust does.
        //                 2. Treat the missing item as a Type::Err and make it part of the gate
        //                    signature, this is what Q# does.
        //              We decided to go with (1) because it avoids propagating the SeqItem enum
        //              to the compiler, which is simpler.
        let params = stmt
            .params
            .iter()
            .filter_map(|seq_item| seq_item.item_as_ref())
            .map(|arg| {
                let ty = crate::semantic::types::Type::Angle(None, false);
                let symbol = Symbol::new(&arg.name, arg.span, ty, Span::default(), IOKind::Default);
                self.try_insert_or_get_existing_symbol_id(&arg.name, symbol)
            })
            .collect::<Box<_>>();

        let qubits = stmt
            .qubits
            .iter()
            .filter_map(|seq_item| seq_item.item_as_ref())
            .map(|arg| {
                let ty = crate::semantic::types::Type::Qubit;
                let symbol = Symbol::new(&arg.name, arg.span, ty, Span::default(), IOKind::Default);
                self.try_insert_or_get_existing_symbol_id(&arg.name, symbol)
            })
            .collect::<Box<_>>();

        let mut stmts = Vec::new();
        for stmt in &stmt.body.stmts {
            stmts.append(&mut self.lower_stmt(stmt));
        }

        let body = semantic::Block {
            span: stmt.body.span,
            stmts: list_from_iter(stmts),
        };

        // Pop the scope where the gate definition lives.
        self.symbols.pop_scope();

        semantic::StmtKind::QuantumGateDefinition(semantic::QuantumGateDefinition {
            span: stmt.span,
            name_span: stmt.ident.span,
            symbol_id,
            params,
            qubits,
            body,
        })
    }

    fn lower_quantum_decl(&mut self, stmt: &syntax::QubitDeclaration) -> semantic::StmtKind {
        if !self.symbols.is_current_scope_global() {
            let kind = SemanticErrorKind::QubitDeclarationInNonGlobalScope(stmt.span);
            self.push_semantic_error(kind);
        }

        // If there wasn't an explicit size, infer the size to be 1.
        let (ty, size_and_span) = if let Some(size_expr) = &stmt.ty.size {
            let span = size_expr.span;
            let size_expr = self.const_eval_quantum_register_size_expr(size_expr);

            let Some(size_expr) = size_expr else {
                // We insert an err symbol if the symbol was not previously defined.
                self.try_insert_err_symbol_or_push_redefined_symbol_error(
                    &stmt.qubit.name,
                    stmt.qubit.span,
                );

                // Any errors would have already been pushed by `const_eval_quantum_register_size`.
                return semantic::StmtKind::Err;
            };

            let Some(size) = size_expr.get_const_u32() else {
                // We insert an err symbol if the symbol was not previously defined.
                self.try_insert_err_symbol_or_push_redefined_symbol_error(
                    &stmt.qubit.name,
                    stmt.qubit.span,
                );

                // Any errors would have already been pushed by `const_eval_quantum_register_size`.
                return semantic::StmtKind::Err;
            };

            (Type::QubitArray(size), Some((size_expr, span)))
        } else {
            (Type::Qubit, None)
        };

        let name = stmt.qubit.name.clone();

        let symbol = Symbol::new(
            &name,
            stmt.qubit.span,
            ty.clone(),
            stmt.ty.span,
            IOKind::Default,
        );

        let symbol_id = self.try_insert_or_get_existing_symbol_id(name, symbol);

        if let Some((size, size_span)) = size_and_span {
            semantic::StmtKind::QubitArrayDecl(semantic::QubitArrayDeclaration {
                span: stmt.span,
                symbol_id,
                size,
                size_span,
            })
        } else {
            semantic::StmtKind::QubitDecl(semantic::QubitDeclaration {
                span: stmt.span,
                symbol_id,
            })
        }
    }

    fn lower_reset(&mut self, stmt: &syntax::ResetStmt) -> semantic::StmtKind {
        let operand = self.lower_gate_operand(&stmt.operand);
        semantic::StmtKind::Reset(semantic::ResetStmt {
            span: stmt.span,
            reset_token_span: stmt.reset_token_span,
            operand: Box::new(operand),
        })
    }

    fn lower_return(&mut self, stmt: &syntax::ReturnStmt) -> semantic::StmtKind {
        let expr = if let Some(expr) = stmt.expr.as_ref() {
            match &**expr {
                syntax::ValueExpr::Expr(expr) => Some(self.lower_expr(expr)),
                syntax::ValueExpr::Measurement(expr) => Some(self.lower_measure_expr(expr)),
                syntax::ValueExpr::Concat(expr) => {
                    let kind = SemanticErrorKind::InvalidConcatenationPosition(
                        "return statements".to_string(),
                        expr.span,
                    );
                    self.push_semantic_error(kind);
                    return semantic::StmtKind::Err;
                }
            }
        } else {
            None
        };
        let mut expr = expr.map(Box::new);

        let return_ty = self.symbols.get_subroutine_return_ty();

        match (&mut expr, return_ty) {
            // If we don't have a return type then we are not rooted in a subroutine scope.
            (_, None) => {
                self.push_semantic_error(SemanticErrorKind::InvalidScope(
                    "return statements".into(),
                    "subroutine".into(),
                    stmt.span,
                ));
                return semantic::StmtKind::Err;
            }
            (None, Some(ty)) => {
                if !matches!(ty.as_ref(), Type::Void) {
                    self.push_semantic_error(
                        SemanticErrorKind::MissingTargetExpressionInReturnStmt(stmt.span),
                    );
                }
            }
            (Some(expr), Some(ty)) => {
                if matches!(ty.as_ref(), Type::Void) {
                    self.push_semantic_error(
                        SemanticErrorKind::ReturningExpressionFromVoidSubroutine(expr.span),
                    );
                    return semantic::StmtKind::Err;
                }
                *expr = Box::new(self.cast_expr_to_type(&ty, expr));
            }
        }

        semantic::StmtKind::Return(semantic::ReturnStmt {
            span: stmt.span,
            expr,
        })
    }

    fn lower_switch(&mut self, stmt: &syntax::SwitchStmt) -> semantic::StmtKind {
        // Semantics of switch case is that the outer block doesn't introduce
        // a new scope but each case rhs does.

        // Can we add a new scope anyway to hold a temporary variable?
        // if we do that, we can refer to a new variable instead of the control
        // expr this would allow us to avoid the need to resolve the control
        // expr multiple times in the case where we have to coerce the control
        // expr to the correct type. Introducing a new variable without a new
        // scope would effect output semantics.
        let cases = stmt
            .cases
            .iter()
            .map(|case| self.lower_switch_case(case))
            .collect::<Vec<_>>();
        let default = stmt.default.as_ref().map(|d| self.lower_block(d));
        let target = self.lower_expr(&stmt.target);

        // The condition for the switch statement must be an integer type
        // so we use `cast_expr_to_type`, forcing the type to be an integer
        // type with implicit casts if necessary.
        let target_ty = Type::Int(None, target.ty.is_const());
        let target = self.cast_expr_to_type(&target_ty, &target);

        // We push a semantic error on switch statements if version is less than 3.1,
        // as they were introduced in 3.1.
        if let Some(ref version) = self.version {
            if version < &SWITCH_MINIMUM_SUPPORTED_VERSION {
                self.push_unsuported_in_this_version_error_message(
                    "switch statements",
                    &SWITCH_MINIMUM_SUPPORTED_VERSION,
                    stmt.span,
                );
            }
        }

        semantic::StmtKind::Switch(semantic::SwitchStmt {
            span: stmt.span,
            target,
            cases: list_from_iter(cases),
            default,
        })
    }

    fn lower_switch_case(&mut self, switch_case: &syntax::SwitchCase) -> semantic::SwitchCase {
        let label_ty = Type::Int(None, true);
        let labels = switch_case
            .labels
            .iter()
            .map(|label| {
                // The labels for each switch case must be of integer type
                // so we use `cast_expr_to_type`, forcing the type to be an integer
                // type with implicit casts if necessary.
                let label = self.lower_expr(label);
                self.cast_expr_to_type(&label_ty, &label)
            })
            .collect::<Vec<_>>();

        let block = self.lower_block(&switch_case.block);

        semantic::SwitchCase {
            span: switch_case.span,
            labels: list_from_iter(labels),
            block,
        }
    }

    fn lower_while_stmt(&mut self, stmt: &syntax::WhileLoop) -> semantic::StmtKind {
        // Push scope where the while loop lives. The while loop needs its own scope
        // so that break and continue know if they are inside a valid scope.
        self.symbols.push_scope(ScopeKind::Loop);

        let condition = self.lower_expr(&stmt.while_condition);
        let body = self.lower_stmt_or_block_body(&stmt.body);

        // The semantics of a while statement is that the condition must be
        // of type bool, so we try to cast it, inserting a cast if necessary.
        let cond_ty = Type::Bool(condition.ty.is_const());
        let while_condition = self.cast_expr_to_type(&cond_ty, &condition);

        // Pop scope where the while loop lives.
        self.symbols.pop_scope();

        semantic::StmtKind::WhileLoop(semantic::WhileLoop {
            span: stmt.span,
            condition: while_condition,
            body,
        })
    }

    fn get_semantic_type_from_tydef(
        &mut self,
        ty: &syntax::TypeDef,
        is_const: bool,
    ) -> (crate::semantic::types::Type, Vec<Expr>) {
        match ty {
            syntax::TypeDef::Scalar(scalar_type) => {
                self.get_semantic_type_from_scalar_ty(scalar_type, is_const)
            }
            syntax::TypeDef::Array(array_type) => self.get_semantic_type_from_array_ty(array_type),
            syntax::TypeDef::ArrayReference(array_reference_type) => {
                self.get_semantic_type_from_array_reference_ty(array_reference_type)
            }
        }
    }

    /// Helper function for const evaluating array sizes, type widths, and durations.
    fn const_eval_designator(&mut self, expr: &syntax::Expr) -> Option<semantic::Expr> {
        let expr = self.lower_expr(expr);
        let expr_span = expr.span;
        let expr = self
            .cast_expr_with_target_type_or_default(Some(expr), &Type::UInt(None, true), expr_span)
            .with_const_value(self);

        // const_eval would have pushed an error unless the ty is Err
        // in which case there is already an error pushed for the ty
        // so there is no need to add more errors here.
        expr.const_value.as_ref()?;

        Some(expr)
    }

    fn const_eval_array_size_designator_expr(
        &mut self,
        expr: &syntax::Expr,
    ) -> Option<semantic::Expr> {
        let size = self.const_eval_designator(expr)?;

        let Some(semantic::LiteralKind::Int(val)) = size.get_const_value() else {
            let msg = "array size".to_string();
            self.push_semantic_error(SemanticErrorKind::ExprMustBeInt(msg, expr.span));
            return None;
        };

        if val < 0 {
            let msg = "array size".to_string();
            self.push_semantic_error(SemanticErrorKind::ExprMustBeNonNegativeInt(msg, expr.span));
            return None;
        }

        if u32::try_from(val).is_err() {
            self.push_semantic_error(SemanticErrorKind::DesignatorTooLarge(expr.span));
        }

        Some(size)
    }

    fn const_eval_type_width_designator_expr(
        &mut self,
        expr: &syntax::Expr,
    ) -> Option<semantic::Expr> {
        let width = self.const_eval_designator(expr)?;

        let Some(semantic::LiteralKind::Int(val)) = width.get_const_value() else {
            let msg = "type width".to_string();
            self.push_semantic_error(SemanticErrorKind::ExprMustBeInt(msg, expr.span));
            return None;
        };

        if val < 1 {
            let msg = "type width".to_string();
            self.push_semantic_error(SemanticErrorKind::ExprMustBePositiveInt(msg, expr.span));
            return None;
        }

        if u32::try_from(val).is_err() {
            self.push_semantic_error(SemanticErrorKind::DesignatorTooLarge(expr.span));
        }

        Some(width)
    }

    fn const_eval_quantum_register_size_expr(
        &mut self,
        expr: &syntax::Expr,
    ) -> Option<semantic::Expr> {
        let size = self.const_eval_designator(expr)?;

        let Some(semantic::LiteralKind::Int(val)) = size.get_const_value() else {
            let msg = "quantum register size".into();
            self.push_semantic_error(SemanticErrorKind::ExprMustBeInt(msg, size.span));
            return None;
        };

        if val < 1 {
            let msg = "quantum register size".into();
            self.push_semantic_error(SemanticErrorKind::ExprMustBePositiveInt(msg, expr.span));
            return None;
        }

        if u32::try_from(val).is_err() {
            self.push_semantic_error(SemanticErrorKind::DesignatorTooLarge(expr.span));
        }

        // We already verified that the expression as a non-negative int,
        // so, this const cast will succeed.
        Self::try_cast_expr_to_type(&Type::UInt(None, true), &size)
            .map(|expr| expr.with_const_value(self))
    }

    #[allow(clippy::too_many_lines)]
    fn get_semantic_type_from_scalar_ty(
        &mut self,
        scalar_ty: &syntax::ScalarType,
        is_const: bool,
    ) -> (crate::semantic::types::Type, Vec<Expr>) {
        match &scalar_ty.kind {
            syntax::ScalarTypeKind::Bit(bit_type) => match &bit_type.size {
                Some(size) => {
                    let Some(size_expr) = self.const_eval_array_size_designator_expr(size) else {
                        return (crate::semantic::types::Type::Err, Vec::new());
                    };
                    let Some(size) = size_expr.get_const_u32() else {
                        return (crate::semantic::types::Type::Err, Vec::new());
                    };
                    (
                        crate::semantic::types::Type::BitArray(size, is_const),
                        vec![size_expr],
                    )
                }
                None => (crate::semantic::types::Type::Bit(is_const), Vec::new()),
            },
            syntax::ScalarTypeKind::Int(int_type) => match &int_type.size {
                Some(size) => {
                    let Some(size_expr) = self.const_eval_type_width_designator_expr(size) else {
                        return (crate::semantic::types::Type::Err, Vec::new());
                    };
                    let Some(size) = size_expr.get_const_u32() else {
                        return (crate::semantic::types::Type::Err, Vec::new());
                    };
                    (
                        crate::semantic::types::Type::Int(Some(size), is_const),
                        vec![size_expr],
                    )
                }
                None => (
                    crate::semantic::types::Type::Int(None, is_const),
                    Vec::new(),
                ),
            },
            syntax::ScalarTypeKind::UInt(uint_type) => match &uint_type.size {
                Some(size) => {
                    let Some(size_expr) = self.const_eval_type_width_designator_expr(size) else {
                        return (crate::semantic::types::Type::Err, Vec::new());
                    };
                    let Some(size) = size_expr.get_const_u32() else {
                        return (crate::semantic::types::Type::Err, Vec::new());
                    };
                    (
                        crate::semantic::types::Type::UInt(Some(size), is_const),
                        vec![size_expr],
                    )
                }
                None => (
                    crate::semantic::types::Type::UInt(None, is_const),
                    Vec::new(),
                ),
            },
            syntax::ScalarTypeKind::Float(float_type) => match &float_type.size {
                Some(size) => {
                    let Some(size_expr) = self.const_eval_type_width_designator_expr(size) else {
                        return (crate::semantic::types::Type::Err, Vec::new());
                    };
                    let Some(size) = size_expr.get_const_u32() else {
                        return (crate::semantic::types::Type::Err, Vec::new());
                    };
                    if size > 64 {
                        self.push_semantic_error(SemanticErrorKind::TypeMaxWidthExceeded(
                            "float".to_string(),
                            64,
                            size as usize,
                            float_type.span,
                        ));
                        (crate::semantic::types::Type::Err, Vec::new())
                    } else {
                        (
                            crate::semantic::types::Type::Float(Some(size), is_const),
                            vec![size_expr],
                        )
                    }
                }
                None => (
                    crate::semantic::types::Type::Float(None, is_const),
                    Vec::new(),
                ),
            },
            syntax::ScalarTypeKind::Complex(complex_type) => match &complex_type.base_size {
                Some(float_type) => match &float_type.size {
                    Some(size) => {
                        let Some(size_expr) = self.const_eval_type_width_designator_expr(size)
                        else {
                            return (crate::semantic::types::Type::Err, Vec::new());
                        };
                        let Some(size) = size_expr.get_const_u32() else {
                            return (crate::semantic::types::Type::Err, Vec::new());
                        };
                        (
                            crate::semantic::types::Type::Complex(Some(size), is_const),
                            vec![size_expr],
                        )
                    }
                    None => (
                        crate::semantic::types::Type::Complex(None, is_const),
                        Vec::new(),
                    ),
                },
                None => (
                    crate::semantic::types::Type::Complex(None, is_const),
                    Vec::new(),
                ),
            },
            syntax::ScalarTypeKind::Angle(angle_type) => match &angle_type.size {
                Some(size) => {
                    let Some(size_expr) = self.const_eval_type_width_designator_expr(size) else {
                        return (crate::semantic::types::Type::Err, Vec::new());
                    };
                    let Some(size) = size_expr.get_const_u32() else {
                        return (crate::semantic::types::Type::Err, Vec::new());
                    };
                    if size > 64 {
                        self.push_semantic_error(SemanticErrorKind::TypeMaxWidthExceeded(
                            "angle".to_string(),
                            64,
                            size as usize,
                            angle_type.span,
                        ));
                        (crate::semantic::types::Type::Err, Vec::new())
                    } else {
                        (
                            crate::semantic::types::Type::Angle(Some(size), is_const),
                            vec![size_expr],
                        )
                    }
                }
                None => (
                    crate::semantic::types::Type::Angle(None, is_const),
                    Vec::new(),
                ),
            },
            syntax::ScalarTypeKind::BoolType => {
                (crate::semantic::types::Type::Bool(is_const), Vec::new())
            }
            syntax::ScalarTypeKind::Duration => {
                (crate::semantic::types::Type::Duration(is_const), Vec::new())
            }
            syntax::ScalarTypeKind::Stretch => {
                (crate::semantic::types::Type::Stretch(true), Vec::new())
            }
            syntax::ScalarTypeKind::Err => (crate::semantic::types::Type::Err, Vec::new()),
        }
    }

    fn get_semantic_type_from_array_base_ty(
        &mut self,
        array_base_ty: &syntax::ArrayBaseTypeKind,
        span: Span,
    ) -> (Type, Vec<Expr>) {
        let base_tydef = match array_base_ty {
            syntax::ArrayBaseTypeKind::Int(ty) => syntax::TypeDef::Scalar(syntax::ScalarType {
                span,
                kind: syntax::ScalarTypeKind::Int(ty.clone()),
            }),
            syntax::ArrayBaseTypeKind::UInt(ty) => syntax::TypeDef::Scalar(syntax::ScalarType {
                span,
                kind: syntax::ScalarTypeKind::UInt(ty.clone()),
            }),
            syntax::ArrayBaseTypeKind::Float(ty) => syntax::TypeDef::Scalar(syntax::ScalarType {
                span,
                kind: syntax::ScalarTypeKind::Float(ty.clone()),
            }),
            syntax::ArrayBaseTypeKind::Complex(ty) => syntax::TypeDef::Scalar(syntax::ScalarType {
                span,
                kind: syntax::ScalarTypeKind::Complex(ty.clone()),
            }),
            syntax::ArrayBaseTypeKind::Angle(ty) => syntax::TypeDef::Scalar(syntax::ScalarType {
                span,
                kind: syntax::ScalarTypeKind::Angle(ty.clone()),
            }),
            syntax::ArrayBaseTypeKind::BoolType => syntax::TypeDef::Scalar(syntax::ScalarType {
                span,
                kind: syntax::ScalarTypeKind::BoolType,
            }),
            syntax::ArrayBaseTypeKind::Duration => syntax::TypeDef::Scalar(syntax::ScalarType {
                span,
                kind: syntax::ScalarTypeKind::Duration,
            }),
        };

        self.get_semantic_type_from_tydef(&base_tydef, false)
    }

    fn lower_array_dims(&mut self, dims: &List<syntax::Expr>) -> Vec<(u32, Expr)> {
        dims.iter()
            .filter_map(|expr| {
                self.const_eval_array_size_designator_expr(expr)
                    .and_then(|expr| expr.get_const_u32().map(|size| (size, expr)))
            })
            .collect::<Vec<_>>()
    }

    fn get_semantic_type_from_array_ty(
        &mut self,
        array_ty: &syntax::ArrayType,
    ) -> (crate::semantic::types::Type, Vec<semantic::Expr>) {
        let base_ty = self.get_semantic_type_from_array_base_ty(&array_ty.base_type, array_ty.span);
        let dims = self.lower_array_dims(&array_ty.dimensions);

        if dims.len() != array_ty.dimensions.len() {
            return (Type::Err, Vec::new());
        }

        if dims.is_empty() {
            self.push_unsupported_error_message("arrays with 0 dimensions", array_ty.span);
            return (Type::Err, Vec::new());
        }

        if dims.len() > 7 {
            self.push_unsupported_error_message(
                "arrays with more than 7 dimensions",
                array_ty.span,
            );
            return (Type::Err, Vec::new());
        }
        let dim_vals = dims.iter().map(|f| f.0).collect::<Vec<_>>();
        let exprs = base_ty
            .1
            .into_iter()
            .chain(dims.into_iter().map(|f| f.1))
            .collect();
        (Type::make_array_ty(&dim_vals, &base_ty.0), exprs)
    }

    fn get_semantic_type_from_array_reference_ty(
        &mut self,
        array_ref_ty: &syntax::ArrayReferenceType,
    ) -> (crate::semantic::types::Type, Vec<Expr>) {
        match array_ref_ty {
            syntax::ArrayReferenceType::Static(ref_ty) => {
                let is_mutable = matches!(ref_ty.mutability, syntax::AccessControl::Mutable);
                let base_ty =
                    self.get_semantic_type_from_array_base_ty(&ref_ty.base_type, ref_ty.span);
                let dims = self.lower_array_dims(&ref_ty.dimensions);

                if dims.len() != ref_ty.dimensions.len() {
                    return (Type::Err, Vec::new());
                }

                if dims.is_empty() {
                    self.push_unsupported_error_message("arrays with 0 dimensions", ref_ty.span);
                    return (Type::Err, Vec::new());
                }

                if dims.len() > 7 {
                    self.push_unsupported_error_message(
                        "arrays with more than 7 dimensions",
                        ref_ty.span,
                    );
                    return (Type::Err, Vec::new());
                }

                let dim_vals = dims.iter().map(|f| f.0).collect::<Vec<_>>();
                let exprs = base_ty
                    .1
                    .into_iter()
                    .chain(dims.into_iter().map(|f| f.1))
                    .collect();

                (
                    Type::make_static_array_ref_ty(&dim_vals, &base_ty.0, is_mutable),
                    exprs,
                )
            }
            syntax::ArrayReferenceType::Dyn(ref_ty) => {
                let is_mutable = matches!(ref_ty.mutability, syntax::AccessControl::Mutable);
                let base_ty =
                    self.get_semantic_type_from_array_base_ty(&ref_ty.base_type, ref_ty.span);
                let Some(num_dims_expr) =
                    self.const_eval_array_size_designator_expr(&ref_ty.dimensions)
                else {
                    // `Self::const_eval_array_size_designator_expr` already pushed
                    // the relevant error message. So we just return `Type::Err` here.
                    return (Type::Err, Vec::new());
                };

                let num_dims = num_dims_expr
                    .get_const_u32()
                    .expect("we only get here if we have a valid const expr");

                if num_dims == 0 {
                    self.push_unsupported_error_message("arrays with 0 dimensions", ref_ty.span);
                    return (Type::Err, Vec::new());
                }

                if num_dims > 7 {
                    self.push_unsupported_error_message(
                        "arrays with more than 7 dimensions",
                        ref_ty.span,
                    );
                    return (Type::Err, Vec::new());
                }

                let exprs = base_ty.1.into_iter().chain(once(num_dims_expr)).collect();
                (
                    Type::make_dyn_array_ref_ty(num_dims.into(), &base_ty.0, is_mutable),
                    exprs,
                )
            }
        }
    }

    fn cast_expr_with_target_type_or_default(
        &mut self,
        expr: Option<semantic::Expr>,
        ty: &Type,
        span: Span,
    ) -> semantic::Expr {
        let Some(mut rhs) = expr else {
            // In OpenQASM, classical variables may be uninitialized, but in Q#,
            // they must be initialized. We will use the default value for the type
            // to initialize the variable.
            return self.get_default_value(ty, span);
        };

        let rhs_ty = rhs.ty.clone();

        // avoid the cast
        if *ty == rhs_ty {
            // if the types are the same, we can use the rhs as is
            return rhs;
        }

        // if we have an exact type match, we can use the rhs as is
        if types_equal_except_const(ty, &rhs_ty) {
            // Since one the two exprs is non-const, we need to relax
            // the constness in the returned expr.
            rhs.ty = rhs.ty.as_non_const();
            return rhs;
        }

        // if the rhs is a literal, we can try to cast it to the target type
        // if they share the same base type.
        if let semantic::ExprKind::Lit(lit) = &*rhs.kind {
            // if the rhs is a literal, we can try to coerce it to the lhs type
            // we can do better than just types given we have a literal value
            if can_cast_literal(ty, &rhs_ty) || can_cast_literal_with_value_knowledge(ty, lit) {
                return self.coerce_literal_expr_to_type(ty, &rhs, lit);
            }
            // if we can't cast the literal, we can't proceed
            // create a semantic error and return
            self.push_invalid_literal_cast_error(ty, &rhs.ty, span);
            return err_expr!(Type::Err, span);
        }
        // the lhs has a type, but the rhs may be of a different type with
        // implicit and explicit conversions. We need to cast the rhs to the
        // lhs type, but if that cast fails, we will have already pushed an error
        // and we can't proceed
        self.cast_expr_to_type(ty, &rhs)
    }

    fn lower_measure_expr(&mut self, expr: &syntax::MeasureExpr) -> semantic::Expr {
        let operand = self.lower_gate_operand(&expr.operand);
        let ty = get_measurement_ty_from_gate_operand(&operand);

        let measurement = semantic::MeasureExpr {
            span: expr.span,
            measure_token_span: expr.measure_token_span,
            operand: self.lower_gate_operand(&expr.operand),
        };

        semantic::Expr::new(expr.span, semantic::ExprKind::Measure(measurement), ty)
    }

    #[allow(clippy::too_many_lines)]
    fn get_default_value(&mut self, ty: &Type, span: Span) -> semantic::Expr {
        use semantic::Expr;
        use semantic::ExprKind;
        use semantic::LiteralKind;
        let from_lit_kind = |kind| -> Expr { Expr::new(span, ExprKind::Lit(kind), ty.as_const()) };
        let expr = match ty {
            Type::Angle(_, _) => Some(from_lit_kind(LiteralKind::Angle(Default::default()))),
            Type::Bit(_) => Some(from_lit_kind(LiteralKind::Bit(false))),
            Type::Int(width, _) | Type::UInt(width, _) => {
                if let Some(width) = width {
                    if *width > 64 {
                        Some(from_lit_kind(LiteralKind::BigInt(
                            BigInt::from_u32(0).expect("0 should be a valid BigInt"),
                        )))
                    } else {
                        Some(from_lit_kind(LiteralKind::Int(0)))
                    }
                } else {
                    Some(from_lit_kind(LiteralKind::Int(0)))
                }
            }
            Type::Bool(_) => Some(from_lit_kind(LiteralKind::Bool(false))),
            Type::Float(_, _) => Some(from_lit_kind(LiteralKind::Float(0.0))),
            Type::Complex(_, _) => Some(from_lit_kind(Complex::default().into())),
            Type::Qubit => {
                let message = "qubit default values";
                self.push_unsupported_error_message(message, span);
                None
            }
            Type::HardwareQubit => {
                let message = "hardware qubit default values";
                self.push_unsupported_error_message(message, span);
                None
            }
            Type::QubitArray(_) => {
                let message = "qubit array default values";
                self.push_unsupported_error_message(message, span);
                None
            }
            Type::BitArray(size, _) => Some(from_lit_kind(semantic::LiteralKind::Bitstring(
                BigInt::ZERO,
                *size,
            ))),
            Type::Duration(_) | Type::Stretch(_) => {
                Some(from_lit_kind(LiteralKind::Duration(Duration::default())))
            }
            Type::Array(array) => {
                let base_ty: Type = array.base_ty.clone().into();
                let default = || self.get_default_value(&base_ty, span);
                Some(from_lit_kind(LiteralKind::Array(
                    semantic::Array::from_default(array.dims.clone(), default, &base_ty),
                )))
            }
            Type::Gate(_, _)
            | Type::Function(..)
            | Type::Range
            | Type::Set
            | Type::Void
            | Type::StaticArrayRef(..)
            | Type::DynArrayRef(..) => {
                let message = format!("default values for {ty}");
                self.push_unsupported_error_message(message, span);
                None
            }
            Type::Err => None,
        };
        let Some(expr) = expr else {
            return err_expr!(ty.as_const());
        };
        expr
    }

    pub(crate) fn coerce_literal_expr_to_type(
        &mut self,
        ty: &Type,
        expr: &semantic::Expr,
        kind: &semantic::LiteralKind,
    ) -> semantic::Expr {
        match Self::try_coerce_literal_expr_to_type(ty, expr, kind) {
            Ok(expr) => expr,
            Err(Some(err)) => {
                self.push_semantic_error(err);
                expr.clone()
            }
            Err(None) => expr.clone(),
        }
    }

    /// Tries to coerce a literal expr to the given type.
    ///
    /// Returns `None` if the cast isn't allowed.
    ///
    /// Returns `Err` if the cast is allowed but there was a value error.
    /// E.g.: this can happen when casting from int[64] to float[64] if
    /// the integer value doesn't fit in 53 bits (the size of `f64::MANTISSA`).
    #[allow(clippy::too_many_lines)]
    pub(crate) fn try_coerce_literal_expr_to_type(
        ty: &Type,
        rhs: &semantic::Expr,
        kind: &semantic::LiteralKind,
    ) -> Result<semantic::Expr, Option<SemanticErrorKind>> {
        assert!(matches!(*rhs.kind, semantic::ExprKind::Lit(..)));
        assert!(rhs.ty.is_const(), "literals must have const types");

        if *ty == rhs.ty {
            // Base case, we shouldn't have gotten here
            // but if we did, we can just return the rhs
            return Ok(rhs.clone());
        }

        if types_equal_except_const(ty, &rhs.ty) {
            // lhs isn't const, but rhs is, this is allowed
            return Ok(rhs.clone());
        }
        assert!(can_cast_literal(ty, &rhs.ty) || can_cast_literal_with_value_knowledge(ty, kind));
        let lhs_ty = ty.clone();
        let rhs_ty = rhs.ty.clone();
        let span = rhs.span;

        if matches!(lhs_ty, Type::Bit(..)) {
            match kind {
                semantic::LiteralKind::Int(value) => {
                    // can_cast_literal_with_value_knowledge guarantees that value is 0 or 1
                    return Ok(semantic::Expr::new(
                        span,
                        semantic::ExprKind::Lit(semantic::LiteralKind::Bit(*value != 0)),
                        lhs_ty.as_const(),
                    ));
                }
                semantic::LiteralKind::Bool(value) => {
                    return Ok(semantic::Expr::new(
                        span,
                        semantic::ExprKind::Lit(semantic::LiteralKind::Bit(*value)),
                        lhs_ty.as_const(),
                    ));
                }
                &semantic::LiteralKind::Angle(value) => {
                    return Ok(semantic::Expr::new(
                        span,
                        semantic::ExprKind::Lit(semantic::LiteralKind::Bit(value.value != 0)),
                        lhs_ty.as_const(),
                    ));
                }
                _ => (),
            }
        }
        // if lhs_ty is 1 dim bitarray and rhs is int/uint, we can cast
        let (is_int_to_bit_array, size) = match &lhs_ty {
            Type::BitArray(size, _) => {
                if matches!(rhs.ty, Type::Int(..) | Type::UInt(..)) {
                    (true, *size)
                } else {
                    (false, 0)
                }
            }
            _ => (false, 0),
        };
        if is_int_to_bit_array {
            if let semantic::LiteralKind::Int(value) = kind {
                if *value < 0 || *value >= (1 << size) {
                    return Err(Self::invalid_literal_cast_error(ty, &rhs.ty, rhs.span));
                }

                let u_size = size as usize;
                let bitstring = format!("{value:0u_size$b}");
                let Ok(value) = BigInt::from_str_radix(&bitstring, 2) else {
                    return Err(Self::invalid_literal_cast_error(ty, &rhs.ty, rhs.span));
                };

                return Ok(semantic::Expr::new(
                    span,
                    semantic::ExprKind::Lit(semantic::LiteralKind::Bitstring(value, size)),
                    lhs_ty.as_const(),
                ));
            }
        }
        if matches!(lhs_ty, Type::UInt(..)) {
            if let semantic::LiteralKind::Int(value) = kind {
                // this should have been validated by can_cast_literal_with_value_knowledge
                return Ok(semantic::Expr::new(
                    span,
                    semantic::ExprKind::Lit(semantic::LiteralKind::Int(*value)),
                    lhs_ty.as_const(),
                ));
            }
        }
        let result = match (&lhs_ty, &rhs_ty) {
            (Type::Float(..), Type::Int(..) | Type::UInt(..)) => {
                if let semantic::LiteralKind::Int(value) = kind {
                    if let Some(value) = safe_i64_to_f64(*value) {
                        return Ok(semantic::Expr::new(
                            span,
                            semantic::ExprKind::Lit(semantic::LiteralKind::Float(value)),
                            lhs_ty.as_const(),
                        ));
                    }
                    return Err(Some(SemanticErrorKind::InvalidCastValueRange(
                        rhs_ty.to_string(),
                        lhs_ty.to_string(),
                        span,
                    )));
                }
                None
            }
            (Type::Angle(width, _), Type::Float(..)) => {
                if let semantic::LiteralKind::Float(value) = kind {
                    return Ok(semantic::Expr::new(
                        span,
                        semantic::ExprKind::Lit(semantic::LiteralKind::Angle(
                            Angle::from_f64_maybe_sized(*value, *width),
                        )),
                        lhs_ty.as_const(),
                    ));
                }
                None
            }
            (Type::Angle(width, _), Type::Int(..) | Type::UInt(..)) => {
                // compatibility case for existing code
                if let semantic::LiteralKind::Int(value) = kind {
                    if *value == 0 {
                        return Ok(semantic::Expr::new(
                            span,
                            semantic::ExprKind::Lit(semantic::LiteralKind::Angle(
                                Angle::from_u64_maybe_sized(0, *width),
                            )),
                            lhs_ty.as_const(),
                        ));
                    }
                }
                None
            }
            (Type::Float(..), Type::Float(..)) => {
                if let semantic::LiteralKind::Float(value) = kind {
                    return Ok(semantic::Expr::new(
                        span,
                        semantic::ExprKind::Lit(semantic::LiteralKind::Float(*value)),
                        lhs_ty.as_const(),
                    ));
                }
                None
            }
            (Type::Complex(..), Type::Complex(..)) => {
                if let semantic::LiteralKind::Complex(value) = kind {
                    return Ok(semantic::Expr::new(
                        span,
                        semantic::ExprKind::Lit(semantic::LiteralKind::Complex(*value)),
                        lhs_ty.as_const(),
                    ));
                }
                None
            }
            (Type::Complex(..), Type::Float(..)) => {
                if let semantic::LiteralKind::Float(value) = kind {
                    return Ok(semantic::Expr::new(
                        span,
                        semantic::ExprKind::Lit(Complex::real(*value).into()),
                        lhs_ty.as_const(),
                    ));
                }
                None
            }
            (Type::Complex(..), Type::Int(..) | Type::UInt(..)) => {
                // complex requires a double as input, so we need to
                // convert the int to a double, then create the complex
                if let semantic::LiteralKind::Int(value) = kind {
                    if let Some(value) = safe_i64_to_f64(*value) {
                        return Ok(semantic::Expr::new(
                            span,
                            semantic::ExprKind::Lit(Complex::real(value).into()),
                            lhs_ty.as_const(),
                        ));
                    }
                    return Err(Some(SemanticErrorKind::InvalidCastValueRange(
                        "int".to_string(),
                        "float".to_string(),
                        span,
                    )));
                }
                None
            }
            (Type::Bit(..), Type::Int(..) | Type::UInt(..)) => {
                // we've already checked that the value is 0 or 1
                if let semantic::LiteralKind::Int(value) = kind {
                    if *value == 0 || *value == 1 {
                        return Ok(semantic::Expr::new(
                            span,
                            semantic::ExprKind::Lit(semantic::LiteralKind::Int(*value)),
                            lhs_ty.as_const(),
                        ));
                    }
                    panic!("value must be 0 or 1");
                } else {
                    panic!("literal must be an Int");
                }
            }
            (Type::Int(width, _), Type::Int(_, _) | Type::UInt(_, _)) => {
                // we've already checked that this conversion can happen from a signed to unsigned int
                match kind {
                    semantic::LiteralKind::Int(value) => {
                        return Ok(semantic::Expr::new(
                            span,
                            semantic::ExprKind::Lit(semantic::LiteralKind::Int(*value)),
                            lhs_ty.as_const(),
                        ));
                    }
                    semantic::LiteralKind::BigInt(value) => {
                        if let Some(width) = width {
                            let mut cap = BigInt::from_i64(1).expect("1 is a valid i64");
                            BigInt::shl_assign(&mut cap, width);
                            if *value >= cap {
                                return Err(Some(SemanticErrorKind::InvalidCastValueRange(
                                    value.to_string(),
                                    format!("int[{width}]"),
                                    span,
                                )));
                            }
                        }
                        return Ok(semantic::Expr::new(
                            span,
                            semantic::ExprKind::Lit(semantic::LiteralKind::BigInt(value.clone())),
                            lhs_ty.as_const(),
                        ));
                    }
                    _ => panic!("literal must be an Int or BigInt"),
                }
            }
            _ => None,
        };
        if result.is_none() {
            // we assert that the type can be casted
            // but we didn't cast it, so this is a bug
            panic!("literal type cast failed lhs: {:?}, rhs: {:?}", ty, rhs.ty);
        } else {
            result.ok_or(Self::invalid_literal_cast_error(ty, &rhs.ty, rhs.span))
        }
    }

    fn cast_expr_to_type(&mut self, ty: &Type, expr: &semantic::Expr) -> semantic::Expr {
        self.cast_expr_to_type_with_span(ty, expr, expr.span)
    }

    fn cast_expr_to_type_with_span(
        &mut self,
        ty: &Type,
        expr: &semantic::Expr,
        span: Span,
    ) -> semantic::Expr {
        let Some(mut cast_expr) = Self::try_cast_expr_to_type(ty, expr) else {
            self.push_invalid_cast_error(ty, &expr.ty, span);
            return expr.clone();
        };
        // the cast infra doesn't care about the span, so we need to set it
        // here before returning the cast expression
        // We only do this when we generate a cast expression
        if let semantic::ExprKind::Cast(cast_ref) = cast_expr.kind.as_mut() {
            cast_ref.span = span;
        }

        cast_expr
    }

    fn try_cast_expr_to_type(ty: &Type, expr: &semantic::Expr) -> Option<semantic::Expr> {
        if *ty == expr.ty {
            // Base case, we shouldn't have gotten here
            // but if we did, we can just return the rhs
            return Some(expr.clone());
        }
        if types_equal_except_const(ty, &expr.ty) {
            if expr.ty.is_const() {
                // lhs isn't const, but rhs is, we can just return the rhs
                let mut expr = expr.clone();
                // relax constness
                expr.ty = expr.ty.as_non_const();
                return Some(expr);
            }
            // the lsh is supposed to be const but is being initialized
            // to a non-const value, we can't allow this
            return None;
        }
        if ty.is_err() || expr.ty.is_err() {
            // if either type is an error, we can't cast
            return None;
        }
        // if the target type is wider, we can try to relax the rhs type
        // We only do this for float and complex. Int types
        // require extra handling for BigInts
        match (ty, &expr.ty) {
            (Type::Angle(w1, _), Type::Angle(w2, _))
            | (Type::Float(w1, _), Type::Float(w2, _))
            | (Type::Complex(w1, _), Type::Complex(w2, _)) => {
                if w1.is_none() && w2.is_some() {
                    return Some(wrap_expr_in_cast_expr(ty.clone(), expr.clone()));
                }

                if *w1 >= *w2 {
                    return Some(wrap_expr_in_cast_expr(ty.clone(), expr.clone()));
                }
            }
            _ => {}
        }
        // Casting of literals is handled elsewhere. This is for casting expressions
        // which cannot be bypassed and must be handled by built-in Q# casts from
        // the standard library.
        match &expr.ty {
            Type::Angle(width, _) => Self::cast_angle_expr_to_type(ty, expr, *width),
            Type::Bit(..) => Self::cast_bit_expr_to_type(ty, expr),
            Type::Bool(..) => Self::cast_bool_expr_to_type(ty, expr),
            Type::Complex(..) => cast_complex_expr_to_type(ty, expr),
            Type::Float(..) => Self::cast_float_expr_to_type(ty, expr),
            Type::Int(width, _) | Type::UInt(width, _) => {
                Self::cast_int_expr_to_type(ty, expr, *width)
            }
            Type::BitArray(size, _) => Self::cast_bitarray_expr_to_type(*size, ty, expr),
            Type::Array(..) => Self::cast_array_expr_to_type(ty, expr),
            Type::StaticArrayRef(..) => Self::cast_static_array_ref_to_type(ty, expr),
            Type::DynArrayRef(..) => Self::cast_dyn_array_ref_to_type(ty, expr),
            Type::Duration(..) | Type::Stretch(..) => cast_duration_subtype_expr_to_type(ty, expr),
            _ => None,
        }
    }

    /// +----------------+----------------------------------------------------------------+
    /// | Allowed casts  | Casting To                                                     |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    /// | Casting From   | bool  | int | uint | float | angle | bit[n] | duration | qubit |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    /// | angle          | Yes   | No  | No   | No    | -     | Yes    | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    fn cast_angle_expr_to_type(
        ty: &Type,
        rhs: &semantic::Expr,
        width: Option<u32>,
    ) -> Option<semantic::Expr> {
        assert!(matches!(rhs.ty, Type::Angle(..)));
        match ty {
            Type::Angle(..) | Type::Bit(..) | Type::Bool(..) => {
                Some(wrap_expr_in_cast_expr(ty.clone(), rhs.clone()))
            }
            Type::BitArray(size, _) if Some(*size) == width => {
                Some(wrap_expr_in_cast_expr(ty.clone(), rhs.clone()))
            }
            _ => None,
        }
    }

    /// +----------------+----------------------------------------------------------------+
    /// | Allowed casts  | Casting To                                                     |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    /// | Casting From   | bool  | int | uint | float | angle | bit[n] | duration | qubit |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    /// | bit            | Yes   | Yes | Yes  | No    | Yes   | -      | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    fn cast_bit_expr_to_type(ty: &Type, rhs: &semantic::Expr) -> Option<semantic::Expr> {
        assert!(matches!(rhs.ty, Type::Bit(..)));
        // There is no operand, choosing the span of the node
        // but we could use the expr span as well.
        match ty {
            Type::Float(..)
            | Type::Bool(..)
            | Type::Int(..)
            | Type::UInt(..)
            | Type::BitArray(..) => Some(wrap_expr_in_cast_expr(ty.clone(), rhs.clone())),

            _ => None,
        }
    }

    /// +----------------+----------------------------------------------------------------+
    /// | Allowed casts  | Casting To                                                     |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    /// | Casting From   | bool  | int | uint | float | angle | bit[n] | duration | qubit |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    /// | float          | Yes   | Yes | Yes  | -     | Yes   | No     | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    ///
    /// Additional cast to complex
    fn cast_float_expr_to_type(ty: &Type, rhs: &semantic::Expr) -> Option<semantic::Expr> {
        assert!(matches!(rhs.ty, Type::Float(..)));
        match ty {
            Type::Angle(..)
            | Type::Int(..)
            | Type::UInt(..)
            | Type::Float(..)
            | Type::Bool(..)
            | Type::Bit(..) => Some(wrap_expr_in_cast_expr(ty.clone(), rhs.clone())),
            Type::Complex(..) => {
                // Even though the spec doesn't say it, we need to allow
                // casting from float to complex, else this kind of expression
                // would be invalid: 2.0 + sin(pi) + 1.0 im
                Some(wrap_expr_in_cast_expr(ty.clone(), rhs.clone()))
            }
            _ => None,
        }
    }

    /// +----------------+----------------------------------------------------------------+
    /// | Allowed casts  | Casting To                                                     |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    /// | Casting From   | bool  | int | uint | float | angle | bit[n] | duration | qubit |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    /// | bool           | -     | Yes | Yes  | Yes   | No    | Yes    | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    fn cast_bool_expr_to_type(ty: &Type, rhs: &semantic::Expr) -> Option<semantic::Expr> {
        assert!(matches!(rhs.ty, Type::Bool(..)));
        match ty {
            Type::Bit(..)
            | Type::BitArray(..)
            | Type::Float(..)
            | Type::Int(..)
            | Type::UInt(..) => Some(wrap_expr_in_cast_expr(ty.clone(), rhs.clone())),
            _ => None,
        }
    }

    /// +----------------+----------------------------------------------------------------+
    /// | Allowed casts  | Casting To                                                     |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    /// | Casting From   | bool  | int | uint | float | angle | bit[n] | duration | qubit |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    /// | int            | Yes   | -   | Yes  | Yes   | No    | Yes    | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    /// | uint           | Yes   | Yes | -    | Yes   | No    | Yes    | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+--------+----------+-------+
    ///
    /// Additional cast to ``BigInt``
    #[allow(clippy::too_many_lines)]
    fn cast_int_expr_to_type(
        ty: &Type,
        rhs: &semantic::Expr,
        width: Option<u32>,
    ) -> Option<semantic::Expr> {
        assert!(matches!(rhs.ty, Type::Int(..) | Type::UInt(..)));

        match ty {
            Type::Float(..)
            | Type::Int(..)
            | Type::UInt(..)
            | Type::Bool(..)
            | Type::Bit(..)
            // Even though the spec doesn't say it, we need to allow
            // casting from int to complex, else this kind of expression
            // would be invalid: 2 + 1i
            | Type::Complex(..) => Some(wrap_expr_in_cast_expr(ty.clone(), rhs.clone())),
            Type::BitArray(size, _) if Some(*size) == width => {
                Some(wrap_expr_in_cast_expr(ty.clone(), rhs.clone()))
            }
            _ => None,
        }
    }

    fn cast_bitarray_expr_to_type(
        array_width: u32,
        ty: &Type,
        rhs: &semantic::Expr,
    ) -> Option<semantic::Expr> {
        match ty {
            Type::Bool(..) | Type::Bit(..) | Type::Int(None, _) | Type::UInt(None, _) => {
                Some(wrap_expr_in_cast_expr(ty.clone(), rhs.clone()))
            }
            Type::Angle(Some(width), _)
            | Type::Int(Some(width), _)
            | Type::UInt(Some(width), _)
                if *width == array_width =>
            {
                Some(wrap_expr_in_cast_expr(ty.clone(), rhs.clone()))
            }
            _ => None,
        }
    }

    fn cast_array_expr_to_type(ty: &Type, expr: &semantic::Expr) -> Option<semantic::Expr> {
        assert!(matches!(expr.ty, Type::Array(..)));

        let Type::Array(array_ty) = &expr.ty else {
            unreachable!("`expr` should be an array");
        };

        match ty {
            Type::StaticArrayRef(ref_ty) if !ref_ty.is_mutable => {
                if !types_equal_except_const(
                    &array_ty.base_ty.clone().into(),
                    &ref_ty.base_ty.clone().into(),
                ) || array_ty.dims != ref_ty.dims
                {
                    return None;
                }

                Some(Expr {
                    span: expr.span,
                    kind: expr.kind.clone(),
                    const_value: expr.const_value.clone(),
                    ty: ty.clone(),
                })
            }
            Type::DynArrayRef(ref_ty) if !ref_ty.is_mutable => {
                if !types_equal_except_const(
                    &array_ty.base_ty.clone().into(),
                    &ref_ty.base_ty.clone().into(),
                ) || array_ty.dims.num_dims() != u32::from(ref_ty.dims)
                {
                    return None;
                }

                Some(Expr {
                    span: expr.span,
                    kind: expr.kind.clone(),
                    const_value: expr.const_value.clone(),
                    ty: ty.clone(),
                })
            }
            _ => None,
        }
    }

    /// The only purpose of this cast is relaxing the mutability of array references.
    fn cast_static_array_ref_to_type(ty: &Type, expr: &semantic::Expr) -> Option<semantic::Expr> {
        assert!(matches!(expr.ty, Type::StaticArrayRef(..)));

        if base_types_equal(ty, &expr.ty) {
            Some(Expr {
                span: expr.span,
                kind: expr.kind.clone(),
                const_value: expr.const_value.clone(),
                ty: ty.clone(),
            })
        } else {
            None
        }
    }

    /// The only purpose of this cast is relaxing the mutability of array references.
    fn cast_dyn_array_ref_to_type(ty: &Type, expr: &semantic::Expr) -> Option<semantic::Expr> {
        assert!(matches!(expr.ty, Type::DynArrayRef(..)));

        if base_types_equal(ty, &expr.ty) {
            Some(Expr {
                span: expr.span,
                kind: expr.kind.clone(),
                const_value: expr.const_value.clone(),
                ty: ty.clone(),
            })
        } else {
            None
        }
    }

    #[allow(clippy::too_many_lines)]
    fn lower_binary_op_expr(
        &mut self,
        op: syntax::BinOp,
        lhs: semantic::Expr,
        rhs: semantic::Expr,
        span: Span,
    ) -> semantic::Expr {
        if lhs.ty.is_quantum() {
            let kind = SemanticErrorKind::QuantumTypesInBinaryExpression(lhs.span);
            self.push_semantic_error(kind);
        }

        if rhs.ty.is_quantum() {
            let kind = SemanticErrorKind::QuantumTypesInBinaryExpression(rhs.span);
            self.push_semantic_error(kind);
        }

        if either_type_is_duration_subtype(&lhs.ty, &rhs.ty) {
            return self.lower_duration_stretch_binop(op, lhs, rhs, span);
        }

        // Bit shifts with int lhs are not allowed in the spec.
        // If we have an int literal in the lhs, we reinterpret it as a uint.
        let lhs = if matches!(op, syntax::BinOp::Shl | syntax::BinOp::Shr)
            && matches!(
                &*lhs.kind,
                semantic::ExprKind::Lit(semantic::LiteralKind::Int(val)) if *val >= 0
            ) {
            semantic::Expr::new(lhs.span, *lhs.kind, Type::UInt(lhs.ty.width(), true))
        } else {
            lhs
        };

        let left_type = lhs.ty.clone();
        let right_type = rhs.ty.clone();

        if Self::binop_requires_bitwise_conversion(op, &left_type) {
            // if the operator requires bitwise conversion, we need to determine
            // what size of UInt to promote to. If we can't promote to a UInt, we
            // push an error and return None.
            let (ty, lhs_uint_promotion, rhs_uint_promotion) =
                promote_to_uint_ty(&left_type, &right_type);
            let Some(ty) = ty else {
                let target_ty = Type::UInt(None, left_type.is_const() && right_type.is_const());
                if lhs_uint_promotion.is_none() {
                    let kind = SemanticErrorKind::CannotCast(
                        left_type.to_string(),
                        target_ty.to_string(),
                        lhs.span,
                    );
                    self.push_semantic_error(kind);
                }
                if rhs_uint_promotion.is_none() {
                    let kind = SemanticErrorKind::CannotCast(
                        right_type.to_string(),
                        target_ty.to_string(),
                        rhs.span,
                    );
                    self.push_semantic_error(kind);
                }
                let bin_expr = semantic::BinaryOpExpr {
                    op: op.into(),
                    lhs,
                    rhs,
                };
                let kind = semantic::ExprKind::BinaryOp(bin_expr);
                let expr = semantic::Expr::new(span, kind, target_ty);
                return expr;
            };
            // Now that we know the effective Uint type, we can cast the lhs and rhs
            // so that operations can be performed on them.
            let new_lhs = self.cast_expr_to_type(&ty, &lhs);
            // only cast the rhs if the operator requires symmetric conversion
            let new_rhs = if Self::binop_requires_bitwise_symmetric_conversion(op) {
                self.cast_expr_to_type(&ty, &rhs)
            } else {
                rhs
            };

            let bin_expr = semantic::BinaryOpExpr {
                lhs: new_lhs,
                rhs: new_rhs,
                op: op.into(),
            };
            let kind = semantic::ExprKind::BinaryOp(bin_expr);
            let expr = semantic::Expr::new(span, kind, ty);

            let final_expr = self.cast_expr_to_type(&left_type, &expr);
            return final_expr;
        }

        // for int, uint, float, and complex, the lesser of the two types is cast to
        // the greater of the two. Within each level of complex and float, types with
        // greater width are greater than types with lower width.
        // complex > float > int/uint
        // Q# has built-in functions: IntAsDouble, IntAsBigInt to handle two cases.
        // If the width of a float is greater than 64, we can't represent it as a double.

        let ty_constness = lhs.ty.is_const() && rhs.ty.is_const();

        let (lhs, rhs, ty) = if matches!(op, syntax::BinOp::AndL | syntax::BinOp::OrL) {
            let ty = Type::Bool(ty_constness);
            let new_lhs = self.cast_expr_to_type(&ty, &lhs);
            let new_rhs = self.cast_expr_to_type(&ty, &rhs);
            (new_lhs, new_rhs, ty)
        } else if binop_requires_asymmetric_angle_op(op, &left_type, &rhs.ty) {
            if matches!(op, syntax::BinOp::Div)
                && matches!(left_type, Type::Angle(..))
                && matches!(right_type, Type::Angle(..))
            {
                // result is uint, we need to promote both sides to match width.
                let angle_ty = Type::Angle(promote_width(&left_type, &right_type), ty_constness);
                let new_lhs = self.cast_expr_to_type(&angle_ty, &lhs);
                let new_rhs = self.cast_expr_to_type(&angle_ty, &rhs);
                let int_ty = Type::UInt(angle_ty.width(), ty_constness);
                (new_lhs, new_rhs, int_ty)
            }
            // Design Decision: This branch isn't Spec compliant. It is an exception
            //                  to support existing code in the Qiskit ecosystem which
            //                  compiles to qasm that adds and subtracts floats and angles.
            else if matches!(op, syntax::BinOp::Add | syntax::BinOp::Sub) {
                if matches!(left_type, Type::Float(..)) {
                    let ty = Type::Angle(rhs.ty.width(), ty_constness);
                    let new_lhs = self.cast_expr_to_type(&ty, &lhs);
                    (new_lhs, rhs, ty)
                } else {
                    let ty = Type::Angle(rhs.ty.width(), ty_constness);
                    let new_rhs = self.cast_expr_to_type(&ty, &rhs);
                    (lhs, new_rhs, ty)
                }
            } else if matches!(left_type, Type::Angle(..)) {
                let ty = Type::Angle(left_type.width(), ty_constness);
                let new_lhs = self.cast_expr_to_type(&ty, &lhs);
                let rhs_ty = Type::UInt(ty.width(), ty_constness);
                let new_rhs = self.cast_expr_to_type(&rhs_ty, &rhs);
                (new_lhs, new_rhs, ty)
            } else {
                let lhs_ty = Type::UInt(rhs.ty.width(), ty_constness);
                let new_lhs = self.cast_expr_to_type(&lhs_ty, &lhs);
                let ty = Type::Angle(rhs.ty.width(), ty_constness);
                let new_rhs = self.cast_expr_to_type(&ty, &rhs);
                (new_lhs, new_rhs, ty)
            }
        } else if binop_requires_int_conversion_for_type(op, &left_type, &rhs.ty) {
            let ty = Type::Int(None, ty_constness);
            let new_lhs = self.cast_expr_to_type(&ty, &lhs);
            let new_rhs = self.cast_expr_to_type(&ty, &rhs);
            (new_lhs, new_rhs, ty)
        } else if requires_symmetric_conversion(op) {
            let promoted_type = try_promote_with_casting(&left_type, &right_type);
            let new_left = if promoted_type == left_type {
                lhs
            } else {
                match &lhs.kind.as_ref() {
                    semantic::ExprKind::Lit(kind) => {
                        if can_cast_literal(&promoted_type, &left_type)
                            || can_cast_literal_with_value_knowledge(&promoted_type, kind)
                        {
                            self.coerce_literal_expr_to_type(&promoted_type, &lhs, kind)
                        } else {
                            self.cast_expr_to_type(&promoted_type, &lhs)
                        }
                    }
                    _ => self.cast_expr_to_type(&promoted_type, &lhs),
                }
            };
            let new_right = if promoted_type == right_type {
                rhs
            } else {
                match &rhs.kind.as_ref() {
                    semantic::ExprKind::Lit(kind) => {
                        if can_cast_literal(&promoted_type, &right_type)
                            || can_cast_literal_with_value_knowledge(&promoted_type, kind)
                        {
                            self.coerce_literal_expr_to_type(&promoted_type, &rhs, kind)
                        } else {
                            self.cast_expr_to_type(&promoted_type, &rhs)
                        }
                    }
                    _ => self.cast_expr_to_type(&promoted_type, &rhs),
                }
            };
            (new_left, new_right, promoted_type)
        } else if binop_requires_symmetric_uint_conversion(op) {
            let ty = Type::UInt(None, ty_constness);
            let new_rhs = self.cast_expr_to_type(&ty, &rhs);
            (lhs, new_rhs, left_type)
        } else {
            (lhs, rhs, left_type)
        };

        // now that we have the lhs and rhs expressions, we can create the binary expression
        // but we need to check if the chosen operator is supported by the types after
        // promotion and conversion.

        let expr = if matches!(ty, Type::Complex(..)) {
            if is_complex_binop_supported(op) {
                let bin_expr = semantic::BinaryOpExpr {
                    op: op.into(),
                    lhs,
                    rhs,
                };
                let kind = semantic::ExprKind::BinaryOp(bin_expr);
                semantic::Expr::new(span, kind, ty.clone())
            } else {
                let kind =
                    SemanticErrorKind::OperatorNotAllowedForComplexValues(format!("{op:?}"), span);
                self.push_semantic_error(kind);
                err_expr!(ty.clone())
            }
        } else {
            let bin_expr = semantic::BinaryOpExpr {
                op: op.into(),
                lhs,
                rhs,
            };
            let kind = semantic::ExprKind::BinaryOp(bin_expr);
            semantic::Expr::new(span, kind, ty.clone())
        };

        let ty = match op.into() {
            semantic::BinOp::AndL
            | semantic::BinOp::Eq
            | semantic::BinOp::Gt
            | semantic::BinOp::Gte
            | semantic::BinOp::Lt
            | semantic::BinOp::Lte
            | semantic::BinOp::Neq
            | semantic::BinOp::OrL => Type::Bool(ty_constness),
            _ => ty,
        };
        let mut expr = expr;
        expr.ty = ty;
        expr
    }

    fn binop_requires_bitwise_conversion(op: syntax::BinOp, left_type: &Type) -> bool {
        if (matches!(
            op,
            syntax::BinOp::AndB | syntax::BinOp::OrB | syntax::BinOp::XorB
        ) && matches!(
            left_type,
            Type::Bit(..) | Type::UInt(..) | Type::BitArray(_, _)
        )) {
            return true;
        }
        if (matches!(op, syntax::BinOp::Shl | syntax::BinOp::Shr)
            && matches!(
                left_type,
                Type::Bit(..) | Type::UInt(..) | Type::BitArray(_, _)
            ))
        {
            return true;
        }
        false
    }

    fn binop_requires_bitwise_symmetric_conversion(op: syntax::BinOp) -> bool {
        matches!(
            op,
            syntax::BinOp::AndB
                | syntax::BinOp::OrB
                | syntax::BinOp::XorB
                | syntax::BinOp::Shl
                | syntax::BinOp::Shr
        )
    }

    /// <https://openqasm.com/language/delays.html#operations-on-durations>
    /// We can add/subtract two durations to get a new duration.
    /// We can multiply or divide them by a constant, to get a new duration.
    /// We can divide them by another duration, to get a machine-precision float.
    /// Negative durations are allowed, however passing a negative duration to a gate[duration] or box[duration] expression will result in an error.
    /// All operations on durations happen at compile time since ultimately all durations, including stretches, will be resolved to constants.
    fn lower_duration_stretch_binop(
        &mut self,
        op: syntax::BinOp,
        lhs: Expr,
        rhs: Expr,
        span: Span,
    ) -> Expr {
        let mut unsupported_binop =
            |op: syntax::BinOp, left_type: Type, right_type: Type, span: Span| -> Expr {
                self.push_semantic_error(SemanticErrorKind::CannotApplyOperatorToTypes(
                    op.to_string(),
                    left_type.to_string(),
                    right_type.to_string(),
                    span,
                ));
                err_expr!(Type::Err, span)
            };

        let left_type = lhs.ty.clone();
        let right_type = rhs.ty.clone();

        if !binary_op_is_supported_for_types(op.into(), &left_type, &right_type) {
            return unsupported_binop(op, left_type, right_type, span);
        }

        let is_const = relax_constness(&left_type, &right_type);

        // <https://openqasm.com/language/types.html#converting-duration-to-other-types>
        // Division of two durations results in a machine-precision float
        // No other operations between durations are allowed.
        let ty = if both_types_are_duration_subtypes(&left_type, &right_type) {
            if op == syntax::BinOp::Div {
                // Division of two durations results in a machine-precision float
                Type::Float(None, is_const)
            } else {
                // Add and Sub are allowed, so we return a duration type.
                Type::Duration(is_const)
            }
        } else {
            Type::Duration(is_const)
        };

        let bin_expr = semantic::BinaryOpExpr {
            op: op.into(),
            lhs,
            rhs,
        };
        let kind = semantic::ExprKind::BinaryOp(bin_expr);
        let expr = semantic::Expr::new(span, kind, ty);
        if is_const {
            expr.with_const_value(self)
        } else {
            expr
        }
    }

    fn lower_index(&mut self, index: &syntax::Index) -> Option<Vec<semantic::Index>> {
        match index {
            syntax::Index::IndexSet(set) => {
                // According to the grammar: <https://openqasm.com/grammar/index.html>
                //   "`setExpression` is only valid when being used as a single index.
                //    Registers can support it for creating aliases, but arrays cannot."
                self.push_semantic_error(SemanticErrorKind::IndexSetOnlyAllowedInAliasStmt(
                    set.span,
                ));
                None
            }
            syntax::Index::IndexList(multidimensional_index) => {
                self.lower_index_list(multidimensional_index)
            }
        }
    }

    fn lower_enumerable_set(&mut self, set: &syntax::EnumerableSet) -> semantic::EnumerableSet {
        match set {
            syntax::EnumerableSet::Set(set) => semantic::EnumerableSet::Set(self.lower_set(set)),
            syntax::EnumerableSet::Range(range_definition) => {
                semantic::EnumerableSet::Range(self.lower_range(range_definition).into())
            }
            syntax::EnumerableSet::Expr(expr) => {
                semantic::EnumerableSet::Expr(self.lower_expr(expr).into())
            }
        }
    }

    fn lower_set(&mut self, set: &syntax::Set) -> semantic::Set {
        let items = set
            .values
            .iter()
            .map(|expr| self.lower_expr(expr))
            .collect::<Vec<_>>();

        semantic::Set {
            span: set.span,
            values: syntax::list_from_iter(items),
        }
    }

    fn lower_index_list(&mut self, list: &syntax::IndexList) -> Option<Vec<semantic::Index>> {
        let indices: Vec<_> = list
            .values
            .iter()
            .filter_map(|index| match &**index {
                syntax::IndexListItem::RangeDefinition(range) => self
                    .lower_const_range(range)
                    .map(|range| semantic::Index::Range(range.into())),
                syntax::IndexListItem::Expr(expr) => {
                    Some(semantic::Index::Expr(self.lower_expr(expr)))
                }
                syntax::IndexListItem::Err => Some(semantic::Index::Expr(err_expr!(Type::Err))),
            })
            .collect();

        if list.values.len() != indices.len() {
            return None;
        }

        Some(indices)
    }

    /// Ranges used for register and array slicing must be const evaluatable.
    fn lower_const_range(&mut self, range: &syntax::Range) -> Option<semantic::Range> {
        let mut lower_and_const_eval = |expr| {
            let lowered_expr = self.lower_expr(expr);
            let lit_expr = Self::try_cast_expr_to_type(&Type::Int(None, true), &lowered_expr);
            let Some(lowered_expr) = lit_expr else {
                self.push_invalid_cast_error(&Type::Int(None, true), &lowered_expr.ty, expr.span);
                return None;
            };
            // const_eval will push any needed errors
            let lowered_expr = lowered_expr.with_const_value(self);

            // fail the lowering if we couldn't evaluate the const value
            let _ = lowered_expr.get_const_value()?;

            Some(lowered_expr)
        };

        let start = range.start.as_ref().map(&mut lower_and_const_eval);
        let step = range.step.as_ref().map(&mut lower_and_const_eval);
        let end = range.end.as_ref().map(&mut lower_and_const_eval);

        // The spec says that the step cannot be zero, so we push an error in that case.
        // <https://openqasm.com/language/types.html#register-concatenation-and-slicing>
        if let Some(Some(step)) = &step {
            if let Some(semantic::LiteralKind::Int(val)) = step.get_const_value() {
                if val == 0 {
                    self.push_semantic_error(SemanticErrorKind::ZeroStepInRange(range.span));
                    return None;
                }
            }
        }

        macro_rules! shortcircuit_inner {
            ($nested_option:expr) => {
                match $nested_option {
                    Some(inner) => Some(inner?),
                    None => None,
                }
            };
        }

        Some(semantic::Range {
            span: range.span,
            start: shortcircuit_inner!(start),
            step: shortcircuit_inner!(step),
            end: shortcircuit_inner!(end),
        })
    }

    /// These ranges as iterators in for loops. The spec says
    /// that `start` and `stop` are mandatory in this case.
    ///
    /// Spec: <https://openqasm.com/language/classical.html#for-loops>
    fn lower_range(&mut self, range: &syntax::Range) -> semantic::Range {
        let start = range.start.as_ref().map(|e| self.lower_expr(e));
        let step = range.step.as_ref().map(|e| self.lower_expr(e));
        let end = range.end.as_ref().map(|e| self.lower_expr(e));

        if start.is_none() {
            self.push_semantic_error(SemanticErrorKind::RangeExpressionsMustHaveStart(range.span));
        }

        if end.is_none() {
            self.push_semantic_error(SemanticErrorKind::RangeExpressionsMustHaveStop(range.span));
        }

        semantic::Range {
            span: range.span,
            start,
            step,
            end,
        }
    }

    fn lower_index_expr(&mut self, expr: &syntax::IndexExpr) -> semantic::Expr {
        let collection = self.lower_expr(&expr.collection);
        let Some(indices) = self.lower_index(&expr.index) else {
            // Since we can't evaluate the indices, we can't know the indexed type.
            return err_expr!(Type::Err, expr.span);
        };

        if indices.is_empty() {
            let kind = SemanticErrorKind::EmptyIndexOperator(expr.index.span());
            self.push_semantic_error(kind);
            return err_expr!(collection.ty.clone(), expr.span);
        }

        // The spec says:
        // "One or more dimension(s) of an array can be zero,
        //  in which case the array has size zero. An array of
        //  size zero cannot be indexed, e.g. given
        //  `array[float[32], 0] myArray;`
        //  it is an error to access either myArray[0] or myArray[-1]."
        if collection.ty.has_zero_size() {
            let kind = SemanticErrorKind::ZeroSizeArrayAccess(expr.span);
            self.push_semantic_error(kind);
            return err_expr!(Type::Err, expr.span);
        }

        self.lower_index_expr_rec(collection, indices.into())
    }

    fn lower_index_expr_rec(
        &mut self,
        expr: semantic::Expr,
        mut indices: VecDeque<semantic::Index>,
    ) -> semantic::Expr {
        // Base case: if the there are no indices, we just return the expression.
        if indices.is_empty() {
            return expr;
        }

        let ty = expr.ty.clone();

        // Recursive case: if the expression is an array we handle the first index here
        //                 and handle the rest recursively.
        if expr.ty.is_array() {
            let index = indices.pop_front().expect("there is at least one index");
            let indexed_ty = expr.ty.get_indexed_type(self, &index, expr.span);
            let span = Span {
                lo: expr.span.lo,
                hi: index.span().hi,
            };
            let expr = semantic::Expr::new(
                span,
                semantic::ExprKind::IndexedExpr(semantic::IndexedExpr {
                    span,
                    collection: Box::new(expr),
                    index: Box::new(index),
                }),
                indexed_ty,
            );
            self.lower_index_expr_rec(expr, indices)
        } else {
            // Recursive case: if the expression is an indexable classical type we handle
            //                 we handle it as a bitarray, which allow us to reuse the same
            //                 codepath for normal arrays.
            match ty {
                Type::Angle(Some(width), constness)
                | Type::Int(Some(width), constness)
                | Type::UInt(Some(width), constness) => {
                    self.index_as_bitarray(&expr, width, constness, indices)
                }
                _ => {
                    let kind =
                        SemanticErrorKind::CannotIndexType(ty.to_string(), indices[0].span());
                    self.push_semantic_error(kind);
                    err_expr!(Type::Err, expr.span)
                }
            }
        }
    }

    /// Helper method to index into the int, uint, and angle classical types.
    /// This method only gets called if the expr is a sized int, uint, or angle.
    fn index_as_bitarray(
        &mut self,
        expr: &semantic::Expr,
        width: u32,
        constness: bool,
        indices: VecDeque<semantic::Index>,
    ) -> semantic::Expr {
        // We first cast the sized int, uint, or angle to a bitarray.
        let expr = self.cast_expr_to_type(&Type::BitArray(width, constness), expr);

        // OpenQASM classical types get indexed in little-endian order. But bitarrays
        // behave as static arrays of 0s and 1s, so, they get indexed in big-endian
        // order. Therefore, we need to change the endianness of our indices after we
        // make the cast to bitarray.
        let indices = indices
            .into_iter()
            .map(|idx| self.change_index_endianness(idx, width))
            .collect();

        // Then, we reuse the codepath for lowering indexed arrays.
        // The spec says: "The bit slicing operation always returns a bit array
        //                of size equal to the size of the index set."
        // So, we don't need to convert back to the original type.
        self.lower_index_expr_rec(expr, indices)
    }

    fn get_indexed_type(
        &mut self,
        ty: &Type,
        span: Span,
        index: &semantic::Index,
    ) -> super::types::Type {
        ty.get_indexed_type(self, index, span)
    }

    /// This method is used to lower the lhs of assignment expressions.
    /// It returns a pair:
    ///   1. The ident indexed with all the array indices, but not with the classical type indices, if any.
    ///   2. A `VecDeque` containing any classical-type indices. These will be used later to finish building
    ///      the assign stmt in [`Self::lower_indexed_classical_type_assign_stmt`].
    fn lower_indexed_ident_expr(
        &mut self,
        indexed_ident: &syntax::IndexedIdent,
    ) -> (semantic::Expr, VecDeque<semantic::Index>) {
        assert!(!indexed_ident.indices.is_empty());

        let collection = self.lower_ident_expr(&indexed_ident.ident);

        if collection.ty.is_readonly_array_ref() {
            let kind = SemanticErrorKind::CannotUpdateReadonlyArrayRef(
                indexed_ident.ident.name.to_string(),
                indexed_ident.ident.span,
            );
            self.push_semantic_error(kind);
            return (err_expr!(Type::Err, indexed_ident.span), Default::default());
        }

        // We flatten the multiple square brackets, converting
        // a[1, 2][5, 7][2, 4:8]
        //   to
        // a[1, 2, 5, 7, 2, 4:8]
        let mut indices = Vec::new();
        for qasm_index in &indexed_ident.indices {
            if let Some(mut lowered_indices) = self.lower_index(qasm_index) {
                indices.append(&mut lowered_indices);
            }
        }

        if indices.len()
            != indexed_ident
                .indices
                .iter()
                .map(|i| i.num_indices())
                .sum::<usize>()
        {
            // Since we can't evaluate all the indices, we can't know the indexed type.
            return (err_expr!(Type::Err, indexed_ident.span), Default::default());
        }

        // The spec says:
        // "One or more dimension(s) of an array can be zero,
        //  in which case the array has size zero. An array of
        //  size zero cannot be indexed, e.g. given
        //  `array[float[32], 0] myArray;`
        //  it is an error to access either myArray[0] or myArray[-1]."
        if collection.ty.has_zero_size() {
            let kind = SemanticErrorKind::ZeroSizeArrayAccess(indexed_ident.span);
            self.push_semantic_error(kind);
            return (err_expr!(Type::Err, indexed_ident.span), Default::default());
        }

        self.lower_indexed_ident_expr_rec(collection, indices.into())
    }

    fn change_index_endianness(&mut self, idx: semantic::Index, width: u32) -> semantic::Index {
        let idx_span = idx.span();
        let width = i64::from(width);

        let unwrap_range_component = |expr_opt: Option<&Expr>, default: i64| -> (i64, Span) {
            if let Some(expr) = expr_opt {
                let Some(semantic::LiteralKind::Int(val)) = expr.const_value else {
                    unreachable!("range components are guaranteed to be const exprs");
                };
                (val, expr.span)
            } else {
                (default, idx_span)
            }
        };

        // If we have a variable `var` of a 32-bit type, and we want to change
        // the endianness of the index when doing var[idx], we need to do var[32 - idx - 1].
        let width_minus_expr_minus_one = |val: i64| -> i64 {
            let val = width - val - 1;

            // OpenQASM indices are width-based, so, we wrap around the width.
            val.rem_euclid(width)
        };

        match idx {
            semantic::Index::Expr(expr) => {
                let new_expr = if expr.ty.is_const() {
                    let (val, span) = unwrap_range_component(Some(&expr.with_const_value(self)), 0);
                    Expr::int(width_minus_expr_minus_one(val), span)
                } else {
                    Expr::bin_op(
                        semantic::BinOp::Sub,
                        // Instead of building two nested bin_ops, we just substract 1 from width.
                        Expr::int(width - 1, expr.span),
                        expr,
                    )
                };
                semantic::Index::Expr(new_expr)
            }
            semantic::Index::Range(range) => {
                let start = {
                    let (val, span) = unwrap_range_component(range.start.as_ref(), 0);
                    Some(Expr::int(width_minus_expr_minus_one(val), span))
                };
                let end = {
                    let (val, span) = unwrap_range_component(range.end.as_ref(), width - 1);
                    Some(Expr::int(width_minus_expr_minus_one(val), span))
                };
                let step = {
                    let (val, span) = unwrap_range_component(range.step.as_ref(), 1);
                    Some(Expr::int(-val, span))
                };
                semantic::Index::Range(Box::new(semantic::Range {
                    span: range.span,
                    start,
                    end,
                    step,
                }))
            }
        }
    }

    /// This method is used to lower the lhs of assignment expressions.
    fn lower_indexed_ident_expr_rec(
        &mut self,
        expr: semantic::Expr,
        mut indices: VecDeque<semantic::Index>,
    ) -> (semantic::Expr, VecDeque<semantic::Index>) {
        // Base case: if the there are no indices, we just return the expression.
        if indices.is_empty() {
            return (expr, indices);
        }

        let ty = expr.ty.clone();

        // Recursive case: if the expression is an array we handle the first index here
        //                 and handle the rest recursively.
        if expr.ty.is_array() {
            let index = indices.pop_front().expect("there is at least one index");
            let indexed_ty = expr.ty.get_indexed_type(self, &index, expr.span);
            let span = Span {
                lo: expr.span.lo,
                hi: index.span().hi,
            };
            let expr = semantic::Expr::new(
                span,
                semantic::ExprKind::IndexedExpr(semantic::IndexedExpr {
                    span,
                    collection: Box::new(expr),
                    index: Box::new(index),
                }),
                indexed_ty,
            );
            self.lower_indexed_ident_expr_rec(expr, indices)
        } else {
            match ty {
                Type::Angle(Some(width), _)
                | Type::Int(Some(width), _)
                | Type::UInt(Some(width), _) => {
                    // We don't want to index into the classical type when lowering
                    // the lhs of an assignment expression. Indexing into the classical
                    // type is handled in compiler.rs.

                    // OpenQASM classical types get indexed in little-endian order. But bitarrays
                    // behave as static arrays of 0s and 1s, so, they get indexed in big-endian
                    // order. Therefore, we need to change the endianness of our indices since we
                    // will cast to bitarray.
                    let indices = indices
                        .into_iter()
                        .map(|idx| self.change_index_endianness(idx, width))
                        .collect();

                    (expr, indices)
                }
                _ => {
                    let kind =
                        SemanticErrorKind::CannotIndexType(ty.to_string(), indices[0].span());
                    self.push_semantic_error(kind);
                    (err_expr!(Type::Err, expr.span), Default::default())
                }
            }
        }
    }

    fn lower_gate_operand(&mut self, operand: &syntax::GateOperand) -> semantic::GateOperand {
        let kind = match &operand.kind {
            syntax::GateOperandKind::IdentOrIndexedIdent(ident_or_indexed_ident) => {
                let expr = match &**ident_or_indexed_ident {
                    syntax::IdentOrIndexedIdent::Ident(ident) => self.lower_ident_expr(ident),
                    syntax::IdentOrIndexedIdent::IndexedIdent(indexed_ident) => {
                        self.lower_indexed_ident_expr(indexed_ident).0
                    }
                };

                semantic::GateOperandKind::Expr(Box::new(expr))
            }
            syntax::GateOperandKind::HardwareQubit(hw) => {
                semantic::GateOperandKind::HardwareQubit(Self::lower_hardware_qubit(hw))
            }
            syntax::GateOperandKind::Err => semantic::GateOperandKind::Err,
        };
        semantic::GateOperand {
            span: operand.span,
            kind,
        }
    }

    fn lower_hardware_qubit(hw: &syntax::HardwareQubit) -> semantic::HardwareQubit {
        semantic::HardwareQubit {
            span: hw.span,
            name: hw.name.clone(),
        }
    }

    fn push_invalid_cast_error(&mut self, target_ty: &Type, expr_ty: &Type, span: Span) {
        if target_ty.is_err() || expr_ty.is_err() {
            // if either type is an error, we don't need to push an error
            return;
        }
        let rhs_ty_name = expr_ty.to_string();
        let lhs_ty_name = target_ty.to_string();
        let kind = SemanticErrorKind::CannotCast(rhs_ty_name, lhs_ty_name, span);
        self.push_semantic_error(kind);
    }

    pub(crate) fn invalid_literal_cast_error(
        target_ty: &Type,
        expr_ty: &Type,
        span: Span,
    ) -> Option<SemanticErrorKind> {
        if target_ty.is_err() || expr_ty.is_err() {
            // if either type is an error, we don't need to push an error
            return None;
        }

        let rhs_ty_name = expr_ty.to_string();
        let lhs_ty_name = target_ty.to_string();
        Some(SemanticErrorKind::CannotCastLiteral(
            rhs_ty_name,
            lhs_ty_name,
            span,
        ))
    }

    pub(crate) fn push_invalid_literal_cast_error(
        &mut self,
        target_ty: &Type,
        expr_ty: &Type,
        span: Span,
    ) {
        if let Some(kind) = Self::invalid_literal_cast_error(target_ty, expr_ty, span) {
            self.push_semantic_error(kind);
        }
    }

    /// Pushes a missing symbol error with the given name
    /// This is a convenience method for pushing a `SemanticErrorKind::UndefinedSymbol` error.
    pub fn push_missing_symbol_error<S: AsRef<str>>(&mut self, name: S, span: Span) {
        let kind = SemanticErrorKind::UndefinedSymbol(name.as_ref().to_string(), span);
        self.push_semantic_error(kind);
    }

    /// Pushes a redefined symbol error with the given name and span.
    /// This is a convenience method for pushing a `SemanticErrorKind::RedefinedSymbol` error.
    pub fn push_redefined_symbol_error<S: AsRef<str>>(&mut self, name: S, span: Span) {
        let kind = SemanticErrorKind::RedefinedSymbol(name.as_ref().to_string(), span);
        self.push_semantic_error(kind);
    }

    /// Pushes an unsupported error with the supplied message.
    pub fn push_unsupported_error_message<S: AsRef<str>>(&mut self, message: S, span: Span) {
        let kind = SemanticErrorKind::NotSupported(message.as_ref().to_string(), span);
        self.push_semantic_error(kind);
    }

    pub fn push_unsuported_in_this_version_error_message<S: AsRef<str>>(
        &mut self,
        message: S,
        minimum_supported_version: &Version,
        span: Span,
    ) {
        let message = message.as_ref().to_string();
        let msv = minimum_supported_version.to_string();
        let kind = SemanticErrorKind::NotSupportedInThisVersion(message, msv, span);
        self.push_semantic_error(kind);
    }

    /// Pushes a semantic error with the given kind.
    pub fn push_semantic_error(&mut self, kind: SemanticErrorKind) {
        let kind = crate::ErrorKind::Semantic(crate::semantic::Error(kind));
        let error = self.create_err(kind);
        self.errors.push(error);
    }

    /// Pushes a const eval error with the given kind.
    pub fn push_const_eval_error(&mut self, kind: ConstEvalError) {
        let kind = crate::ErrorKind::ConstEval(kind);
        let error = self.create_err(kind);
        self.errors.push(error);
    }

    /// Creates an error from the given kind with the current source mapping.
    fn create_err(&self, kind: crate::ErrorKind) -> WithSource<crate::Error> {
        let error = crate::Error(kind);
        WithSource::from_map(&self.source_map, error)
    }

    pub fn get_include_file_defining_standard_gate(&self, name: &str) -> Option<&'static str> {
        if self.version == Some(QASM2_VERSION) {
            if QASM2_STDGATES.contains(&name) {
                return Some(QASM2_STDGATES_INC);
            }
        } else if QASM3_STDGATES.contains(&name) {
            return Some(QASM3_STDGATES_INC);
        }
        None
    }
}

/// Wraps the given expression in a cast expression with the specified type.
/// We mark the cast as implicit as it is almost always the case. In the case of
/// explicit casts, we update the field accordingly afterwards.
fn wrap_expr_in_cast_expr(ty: Type, rhs: semantic::Expr) -> semantic::Expr {
    semantic::Expr::new(
        rhs.span,
        semantic::ExprKind::Cast(semantic::Cast {
            span: Span::default(),
            expr: rhs,
            ty: ty.clone(),
            kind: semantic::CastKind::Implicit,
            ty_exprs: list_from_iter(vec![]),
        }),
        ty,
    )
}

fn get_measurement_ty_from_gate_operand(operand: &semantic::GateOperand) -> Type {
    if let semantic::GateOperandKind::Expr(expr) = &operand.kind {
        if let Type::QubitArray(size) = expr.ty {
            return Type::BitArray(size, false);
        }
    }

    Type::Bit(false)
}

/// +----------------+-------------------------------------------------------------+
/// | Allowed casts  | Casting To                                                  |
/// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
/// | Casting From   | bool  | int | uint | float | angle | bit | duration | qubit |
/// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
/// | complex        | ??    | ??  | ??   | ??    | No    | ??  | No       | No    |
/// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
fn cast_complex_expr_to_type(ty: &Type, rhs: &semantic::Expr) -> Option<semantic::Expr> {
    assert!(matches!(rhs.ty, Type::Complex(..)));

    if matches!((ty, &rhs.ty), (Type::Complex(..), Type::Complex(..))) {
        // complex can only cast to complex. We do the same as for floats
        // when handling the widths, that is, ignoring them, since complex
        // numbers are a pair of floats.
        return Some(rhs.clone());
    }
    None
}

fn cast_duration_subtype_expr_to_type(ty: &Type, rhs: &semantic::Expr) -> Option<semantic::Expr> {
    assert!(matches!(rhs.ty, Type::Duration(..) | Type::Stretch(..)));

    // we know the rhs is a duration or a stretch
    // if the target type is a duration subtype, we can 'cast' it
    if type_is_duration_subtype(ty) {
        // duration can only 'cast' to a duration subtype.
        // We aren't going to actually cast, we just
        // change the type of the expression.
        let mut expr = rhs.clone();
        expr.ty = ty.clone();
        return Some(expr);
    }
    None
}

fn get_identifier_name(identifier: &syntax::IdentOrIndexedIdent) -> Arc<str> {
    match identifier {
        syntax::IdentOrIndexedIdent::Ident(ident) => ident.name.clone(),
        syntax::IdentOrIndexedIdent::IndexedIdent(ident) => ident.ident.name.clone(),
    }
}

#[derive(Debug, Clone, Copy)]
enum BuiltinFunction {
    Arccos,
    Arcsin,
    Arctan,
    Ceiling,
    Cos,
    Exp,
    Floor,
    Log,
    Mod,
    Popcount,
    Pow,
    Rotl,
    Rotr,
    Sin,
    Sqrt,
    Tan,
}

impl FromStr for BuiltinFunction {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "arccos" => Ok(Self::Arccos),
            "arcsin" => Ok(Self::Arcsin),
            "arctan" => Ok(Self::Arctan),
            "ceiling" => Ok(Self::Ceiling),
            "cos" => Ok(Self::Cos),
            "exp" => Ok(Self::Exp),
            "floor" => Ok(Self::Floor),
            "log" => Ok(Self::Log),
            "mod" => Ok(Self::Mod),
            "popcount" => Ok(Self::Popcount),
            "pow" => Ok(Self::Pow),
            "rotl" => Ok(Self::Rotl),
            "rotr" => Ok(Self::Rotr),
            "sin" => Ok(Self::Sin),
            "sqrt" => Ok(Self::Sqrt),
            "tan" => Ok(Self::Tan),
            _ => Err(()),
        }
    }
}

fn sizeof_invalid_args_error(call_span: Span, inputs: &[Expr]) -> ConstEvalError {
    let mut error_msg = String::new();
    write!(
        error_msg,
        "There is no valid overload of `sizeof` for inputs: "
    )
    .expect("write should succeed");

    let inputs_str = inputs
        .iter()
        .map(|expr| expr.ty.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    write!(error_msg, "({inputs_str})").expect("write should succeed");
    write!(error_msg, "\nOverloads available are:").expect("write should succeed");
    write!(
        error_msg,
        "\n    fn sizeof(array[_, ...], const uint) -> const uint"
    )
    .expect("write should succeed");
    write!(
        error_msg,
        "\n    fn sizeof(array[_, #dim = _], uint) -> uint"
    )
    .expect("write should succeed");

    ConstEvalError::NoValidOverloadForBuiltinFunction(error_msg, call_span)
}
