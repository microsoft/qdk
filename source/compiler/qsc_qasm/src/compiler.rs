// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod error;

use core::f64;
use std::{rc::Rc, str::FromStr, sync::Arc};

use error::CompilerErrorKind;
use num_bigint::BigInt;
use qsc_data_structures::{span::Span, target::Profile};
use qsc_frontend::{compile::SourceMap, error::WithSource};
use rustc_hash::FxHashMap;

use crate::{
    CompilerConfig, OperationSignature, OutputSemantics, ProgramType, QasmCompileUnit,
    QubitSemantics,
    ast_builder::{
        build_adj_plus_ctl_functor, build_angle_cast_call_by_name,
        build_angle_convert_call_with_two_params, build_arg_pat, build_argument_validation_stmts,
        build_array_reverse_expr, build_assignment_statement, build_attr, build_barrier_call,
        build_binary_expr, build_call_no_params, build_call_stmt_no_params, build_call_with_param,
        build_call_with_params, build_classical_decl, build_complex_from_expr,
        build_convert_call_expr, build_convert_cast_call_by_name, build_end_stmt,
        build_expr_array_expr, build_for_stmt, build_function_or_operation,
        build_gate_call_param_expr, build_gate_call_with_params_and_callee,
        build_if_expr_then_block, build_if_expr_then_block_else_block,
        build_if_expr_then_block_else_expr, build_if_expr_then_expr_else_expr,
        build_implicit_return_stmt, build_index_expr, build_lit_angle_expr, build_lit_bigint_expr,
        build_lit_bool_expr, build_lit_complex_expr, build_lit_double_expr, build_lit_int_expr,
        build_lit_result_array_expr, build_lit_result_expr, build_managed_qubit_alloc,
        build_math_call_from_exprs, build_math_call_no_params, build_measure_call,
        build_measureeachz_call, build_operation_with_stmts, build_path_ident_expr,
        build_path_ident_ty, build_qasm_convert_call_with_one_param, build_qasm_import_decl,
        build_qasm_import_items, build_qasmstd_convert_call_with_two_params, build_range_expr,
        build_reset_all_call, build_reset_call, build_return_expr, build_return_unit,
        build_stmt_semi_from_expr, build_stmt_semi_from_expr_with_span,
        build_top_level_ns_with_items, build_tuple_expr, build_unary_op_expr,
        build_unmanaged_qubit_alloc, build_unmanaged_qubit_alloc_array, build_while_stmt,
        build_wrapped_block_expr, managed_qubit_alloc_array, map_qsharp_type_to_ast_ty,
        wrap_expr_in_parens,
    },
    io::SourceResolver,
    parser::ast::{List, PathKind, list_from_iter},
    semantic::{
        QasmSemanticParseResult,
        ast::{
            Array, BinaryOpExpr, Cast, Expr, GateOperand, GateOperandKind, Index, IndexedExpr,
            LiteralKind, MeasureExpr, Set, TimeUnit, UnaryOpExpr,
        },
        symbols::{IOKind, Symbol, SymbolId, SymbolTable},
        types::{Type, promote_types},
    },
    stdlib::complex::Complex,
};

use crate::semantic::ast as semast;
use qsc_ast::ast::{self as qsast, NodeId, Package};

const QSHARP_QIR_INTRINSIC_ANNOTATION: &str = "SimulatableIntrinsic";
const QDK_QIR_INTRINSIC_ANNOTATION: &str = "qdk.qir.intrinsic";
const QSHARP_CONFIG_ANNOTATION: &str = "Config";
const QDK_CONFIG_ANNOTATION: &str = "qdk.qir.profile";

/// Helper to create an error expression. Used when we fail to
/// compile an expression. It is assumed that an error was
/// already reported.
fn err_expr(span: Span) -> qsast::Expr {
    qsast::Expr {
        span,
        ..Default::default()
    }
}

#[must_use]
pub fn parse_and_compile_to_qsharp_ast_with_config<
    R: SourceResolver,
    S: Into<Arc<str>>,
    P: Into<Arc<str>>,
>(
    source: S,
    path: P,
    resolver: Option<&mut R>,
    config: CompilerConfig,
) -> QasmCompileUnit {
    let res = if let Some(resolver) = resolver {
        crate::semantic::parse_source(source, path, resolver)
    } else {
        crate::semantic::parse(source, path)
    };
    compile_to_qsharp_ast_with_config(res, config)
}

#[must_use]
pub fn compile_to_qsharp_ast_with_config(
    res: QasmSemanticParseResult,
    config: CompilerConfig,
) -> QasmCompileUnit {
    let program = res.program;

    let compiler = crate::compiler::QasmCompiler {
        source_map: res.source_map,
        config,
        stmts: vec![],
        symbols: res.symbols,
        errors: res.errors,
        pragma_config: PragmaConfig::default(),
    };

    compiler.compile(&program)
}

#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
pub enum PragmaKind {
    QdkBoxOpen,
    QdkBoxClose,
    QdkQirProfile,
}

impl FromStr for PragmaKind {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "qdk.box.open" => Ok(PragmaKind::QdkBoxOpen),
            "qdk.box.close" => Ok(PragmaKind::QdkBoxClose),
            "qdk.qir.profile" => Ok(PragmaKind::QdkQirProfile),
            _ => Err(()),
        }
    }
}

#[derive(Eq, PartialEq, Default)]
pub struct PragmaConfig {
    pub pragmas: FxHashMap<PragmaKind, Arc<str>>,
}

impl PragmaConfig {
    pub fn is_supported<S: AsRef<str>>(&self, pragma: S) -> bool {
        PragmaKind::from_str(pragma.as_ref()).is_ok()
    }

    /// Inserts a pragma into the configuration.
    /// If the pragma already exists, it will be overwritten.
    pub fn insert<V: Into<Arc<str>>>(&mut self, pragma: PragmaKind, value: V) {
        self.pragmas.insert(pragma, value.into());
    }

    #[must_use]
    pub fn get(&self, key: PragmaKind) -> Option<&Arc<str>> {
        self.pragmas.get(&key)
    }
}

pub struct QasmCompiler {
    /// The source map of QASM sources for error reporting.
    pub source_map: SourceMap,
    /// The configuration for the compiler.
    /// This includes the qubit semantics to follow when compiling to Q# AST.
    /// The output semantics to follow when compiling to Q# AST.
    /// The program type to compile to.
    pub config: CompilerConfig,
    /// The compiled statements accumulated during compilation.
    pub stmts: Vec<qsast::Stmt>,
    pub symbols: SymbolTable,
    pub errors: Vec<WithSource<crate::Error>>,
    pub pragma_config: PragmaConfig,
}

impl QasmCompiler {
    /// The main entry into compilation. This function will compile the
    /// source file and build the appropriate package based on the
    /// configuration.
    #[must_use]
    pub fn compile(mut self, program: &crate::semantic::ast::Program) -> QasmCompileUnit {
        // in non-file mode we need the runtime imports in the body
        let program_ty = self.config.program_ty.clone();

        // If we are compiling for operation/fragments, we need to
        // prepend to the list of statements.
        // In file mode we need to add top level imports which are
        // handled in the `build_file` method.
        if !matches!(program_ty, ProgramType::File) {
            self.append_runtime_import_decls();
        }
        for pragma in &program.pragmas {
            self.compile_pragma_stmt(pragma);
        }

        self.compile_stmts(&program.statements);
        let (package, signature) = match program_ty {
            ProgramType::File => self.build_file(),
            ProgramType::Operation => self.build_operation(),
            ProgramType::Fragments => (self.build_fragments(), None),
        };

        let target_profile = self.get_profile();
        QasmCompileUnit::new(
            self.source_map,
            self.errors,
            package,
            signature,
            target_profile,
        )
    }

    /// Gets the profile for compilation from the first profile
    /// pragma if present, otherwise default to `Unrestricted`.
    fn get_profile(&self) -> Profile {
        self.pragma_config
            .pragmas
            .get(&PragmaKind::QdkQirProfile)
            .map_or(Profile::Unrestricted, |profile_str| {
                Profile::from_str(profile_str.as_ref()).expect(
                "Invalid profile pragma; only a valid profile should be store in pragma_config.",
            )
            })
    }

    /// Build a package with namespace and an operation
    /// containing the compiled statements.
    fn build_file(&mut self) -> (Package, Option<OperationSignature>) {
        let whole_span = self.whole_span();
        let operation_name = self.config.operation_name();
        let (operation, mut signature) = self.create_entry_operation(operation_name, whole_span);
        let ns = self.config.namespace();
        signature.ns = Some(ns.to_string());
        let mut items = build_qasm_import_items();
        items.push(operation);
        let top = build_top_level_ns_with_items(whole_span, ns, items);
        (
            Package {
                nodes: Box::new([top]),
                ..Default::default()
            },
            Some(signature),
        )
    }

    /// Creates an operation with the given name.
    fn build_operation(&mut self) -> (qsast::Package, Option<OperationSignature>) {
        let whole_span = self.whole_span();
        let operation_name = self.config.operation_name();
        let (operation, signature) = self.create_entry_operation(operation_name, whole_span);
        (
            Package {
                nodes: Box::new([qsast::TopLevelNode::Stmt(Box::new(qsast::Stmt {
                    kind: Box::new(qsast::StmtKind::Item(Box::new(operation))),
                    span: whole_span,
                    id: qsast::NodeId::default(),
                }))]),
                ..Default::default()
            },
            Some(signature),
        )
    }

    /// Turns the compiled statements into package of top level nodes
    fn build_fragments(&mut self) -> qsast::Package {
        let nodes = self
            .stmts
            .drain(..)
            .map(Box::new)
            .map(qsast::TopLevelNode::Stmt)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        qsast::Package {
            nodes,
            ..Default::default()
        }
    }

    /// Returns a span containing all the statements in the program.
    fn whole_span(&self) -> Span {
        let main_src = self
            .source_map
            .iter()
            .next()
            .expect("there is at least one source");

        #[allow(clippy::cast_possible_truncation)]
        Span {
            lo: main_src.offset,
            hi: main_src.offset + main_src.contents.len() as u32,
        }
    }

    fn create_entry_operation<S: AsRef<str>>(
        &mut self,
        name: S,
        whole_span: Span,
    ) -> (qsast::Item, OperationSignature) {
        let stmts = self.stmts.drain(..).collect::<Vec<_>>();
        let input = self.symbols.get_input();

        // Analyze input for `Angle` types which we can't support as it would require
        // passing a struct from Python. So we need to raise an error saying to use `float`
        // which will preserve the angle type semantics via implicit conversion to angle
        // in the qasm program.
        if let Some(inputs) = &input {
            for input in inputs {
                let qsharp_ty = self.map_semantic_type_to_qsharp_type(&input.ty, input.ty_span);
                if matches!(qsharp_ty, crate::types::Type::Angle(..)) {
                    let message =
                        "use `float` types for passing input, using `angle` types".to_string();
                    let kind = CompilerErrorKind::NotSupported(message, input.span);
                    self.push_compiler_error(kind);
                }
            }
        }

        let output = self.symbols.get_output();
        self.create_entry_item(
            name,
            stmts,
            input,
            output,
            whole_span,
            self.config.output_semantics,
        )
    }

    #[allow(clippy::too_many_lines)]
    fn create_entry_item<S: AsRef<str>>(
        &mut self,
        name: S,
        stmts: Vec<qsast::Stmt>,
        input: Option<Vec<Rc<Symbol>>>,
        output: Option<Vec<Rc<Symbol>>>,
        whole_span: Span,
        output_semantics: OutputSemantics,
    ) -> (qsast::Item, OperationSignature) {
        let mut stmts = stmts;
        let is_qiskit = matches!(output_semantics, OutputSemantics::Qiskit);
        let mut signature = OperationSignature {
            input: vec![],
            output: String::new(),
            name: name.as_ref().to_string(),
            ns: None,
        };
        let output_ty = self.apply_output_semantics(
            output,
            whole_span,
            output_semantics,
            &mut stmts,
            is_qiskit,
        );

        if let Some(input) = &input {
            let args = input
                .iter()
                .map(|s| {
                    let qsharp_ty = self.map_semantic_type_to_qsharp_type(&s.ty, s.ty_span);
                    let ast_ty = map_qsharp_type_to_ast_ty(&qsharp_ty, s.ty_span);
                    (&s.name, ast_ty, s.span, &s.ty)
                })
                .collect::<Vec<_>>();
            let mut validation_stmts = Self::get_argument_validation_stmts(&args);
            validation_stmts.extend(stmts);
            stmts = validation_stmts;
        }

        let ast_ty = map_qsharp_type_to_ast_ty(&output_ty, whole_span);
        signature.output = format!("{output_ty}");
        // This can create a collision on multiple compiles when interactive
        // We also have issues with the new entry point inference logic.
        let input_desc = match input {
            Some(ref input) => input
                .iter()
                .map(|s| {
                    let qsharp_ty = self.map_semantic_type_to_qsharp_type(&s.ty, s.ty_span);
                    (s.name.to_string(), format!("{qsharp_ty}"))
                })
                .collect(),
            None => vec![],
        };

        signature.input = input_desc;
        let input_pats = match input {
            Some(input) => input
                .iter()
                .map(|s| {
                    let qsharp_ty = self.map_semantic_type_to_qsharp_type(&s.ty, s.ty_span);
                    build_arg_pat(
                        s.name.clone(),
                        s.span,
                        map_qsharp_type_to_ast_ty(&qsharp_ty, s.ty_span),
                    )
                })
                .collect(),
            None => vec![],
        };

        let add_entry_point_attr = matches!(self.config.program_ty, ProgramType::File);
        (
            build_operation_with_stmts(
                name,
                input_pats,
                ast_ty,
                stmts,
                whole_span,
                add_entry_point_attr,
            ),
            signature,
        )
    }

    fn apply_output_semantics(
        &mut self,
        output: Option<Vec<Rc<Symbol>>>,
        whole_span: Span,
        output_semantics: OutputSemantics,
        stmts: &mut Vec<qsast::Stmt>,
        is_qiskit: bool,
    ) -> crate::types::Type {
        if matches!(output_semantics, OutputSemantics::ResourceEstimation) {
            // we have no output, but need to set the entry point return type
            crate::types::Type::Tuple(vec![])
        } else if let Some(output) = output {
            let output_exprs = if is_qiskit {
                output
                    .iter()
                    .rev()
                    .filter(|symbol| {
                        matches!(symbol.ty, crate::semantic::types::Type::BitArray(..))
                    })
                    .map(|symbol| {
                        let ident =
                            build_path_ident_expr(symbol.name.as_str(), symbol.span, symbol.span);

                        build_array_reverse_expr(ident)
                    })
                    .collect::<Vec<_>>()
            } else {
                output
                    .iter()
                    .map(|symbol| {
                        let ident =
                            build_path_ident_expr(symbol.name.as_str(), symbol.span, symbol.span);
                        if matches!(symbol.ty, Type::Angle(..)) {
                            // we can't output a struct, so we need to convert it to a double
                            build_angle_cast_call_by_name(
                                "AngleAsDouble",
                                ident,
                                symbol.span,
                                symbol.span,
                            )
                        } else {
                            ident
                        }
                    })
                    .collect::<Vec<_>>()
            };
            // this is the output whether it is inferred or explicitly defined
            // map the output symbols into a return statement, add it to the nodes list,
            // and get the entry point return type
            let output_types = if is_qiskit {
                output
                    .iter()
                    .rev()
                    .filter(|symbol| {
                        matches!(symbol.ty, crate::semantic::types::Type::BitArray(..))
                    })
                    .map(|symbol| self.map_semantic_type_to_qsharp_type(&symbol.ty, symbol.ty_span))
                    .collect::<Vec<_>>()
            } else {
                output
                    .iter()
                    .map(|symbol| {
                        let qsharp_ty =
                            self.map_semantic_type_to_qsharp_type(&symbol.ty, symbol.ty_span);
                        if matches!(qsharp_ty, crate::types::Type::Angle(..)) {
                            crate::types::Type::Double(symbol.ty.is_const())
                        } else {
                            qsharp_ty
                        }
                    })
                    .collect::<Vec<_>>()
            };

            let (output_ty, output_expr) = if output_types.len() == 1 {
                (output_types[0].clone(), output_exprs[0].clone())
            } else {
                let output_ty = crate::types::Type::Tuple(output_types);
                let output_expr = build_tuple_expr(output_exprs);
                (output_ty, output_expr)
            };

            let return_stmt = build_implicit_return_stmt(output_expr);
            stmts.push(return_stmt);
            output_ty
        } else {
            if is_qiskit {
                let kind = CompilerErrorKind::QiskitEntryPointMissingOutput(whole_span);
                self.push_compiler_error(kind);
            }
            crate::types::Type::Tuple(vec![])
        }
    }

    /// Appends the runtime imports to the compiled statements.
    fn append_runtime_import_decls(&mut self) {
        for stmt in build_qasm_import_decl() {
            self.stmts.push(stmt);
        }
    }

    fn compile_stmts(&mut self, stmts: &[Box<crate::semantic::ast::Stmt>]) {
        for stmt in stmts {
            let compiled_stmt = self.compile_stmt(stmt.as_ref());
            if let Some(stmt) = compiled_stmt {
                self.stmts.push(stmt);
            }
        }
    }

    fn compile_stmt(&mut self, stmt: &crate::semantic::ast::Stmt) -> Option<qsast::Stmt> {
        if !stmt.annotations.is_empty()
            && !matches!(
                stmt.kind.as_ref(),
                semast::StmtKind::QuantumGateDefinition(..) | semast::StmtKind::Def(..)
            )
        {
            for annotation in &stmt.annotations {
                self.push_compiler_error(CompilerErrorKind::InvalidAnnotationTarget(
                    annotation.span,
                ));
            }
        }

        match stmt.kind.as_ref() {
            semast::StmtKind::Alias(stmt) => self.compile_alias_decl_stmt(stmt),
            semast::StmtKind::Assign(stmt) => self.compile_assign_stmt(stmt),
            semast::StmtKind::Barrier(stmt) => Self::compile_barrier_stmt(stmt),
            semast::StmtKind::Box(stmt) => self.compile_box_stmt(stmt),
            semast::StmtKind::Block(stmt) => self.compile_block_stmt(stmt),
            semast::StmtKind::Break(stmt) => self.compile_break_stmt(stmt),
            semast::StmtKind::Calibration(cal) => self.compile_calibration_stmt(cal),
            semast::StmtKind::CalibrationGrammar(stmt) => {
                self.compile_calibration_grammar_stmt(stmt)
            }
            semast::StmtKind::ClassicalDecl(stmt) => self.compile_classical_decl(stmt),
            semast::StmtKind::Continue(stmt) => self.compile_continue_stmt(stmt),
            semast::StmtKind::Def(def_stmt) => self.compile_def_stmt(def_stmt, &stmt.annotations),
            semast::StmtKind::DefCal(stmt) => self.compile_def_cal_stmt(stmt),
            semast::StmtKind::Delay(stmt) => self.compile_delay_stmt(stmt),
            semast::StmtKind::End(stmt) => Self::compile_end_stmt(stmt),
            semast::StmtKind::ExprStmt(stmt) => self.compile_expr_stmt(stmt),
            semast::StmtKind::ExternDecl(stmt) => self.compile_extern_stmt(stmt),
            semast::StmtKind::For(stmt) => self.compile_for_stmt(stmt),
            semast::StmtKind::If(stmt) => self.compile_if_stmt(stmt),
            semast::StmtKind::GateCall(stmt) => self.compile_gate_call_stmt(stmt),
            semast::StmtKind::Include(stmt) => self.compile_include_stmt(stmt),
            semast::StmtKind::IndexedClassicalTypeAssign(stmt) => {
                self.compile_indexed_classical_type_assign_stmt(stmt)
            }
            semast::StmtKind::InputDeclaration(stmt) => self.compile_input_decl_stmt(stmt),
            semast::StmtKind::OutputDeclaration(stmt) => self.compile_output_decl_stmt(stmt),
            semast::StmtKind::MeasureArrow(stmt) => self.compile_measure_stmt(stmt),
            semast::StmtKind::Pragma(_) => {
                unreachable!("pragma should have been removed in the lowerer")
            }
            semast::StmtKind::QuantumGateDefinition(gate_stmt) => {
                self.compile_gate_decl_stmt(gate_stmt, &stmt.annotations)
            }
            semast::StmtKind::QubitDecl(stmt) => self.compile_qubit_decl_stmt(stmt),
            semast::StmtKind::QubitArrayDecl(stmt) => self.compile_qubit_array_decl_stmt(stmt),
            semast::StmtKind::Reset(stmt) => self.compile_reset_stmt(stmt),
            semast::StmtKind::Return(stmt) => self.compile_return_stmt(stmt),
            semast::StmtKind::Switch(stmt) => self.compile_switch_stmt(stmt),
            semast::StmtKind::WhileLoop(stmt) => self.compile_while_stmt(stmt),
            semast::StmtKind::Err => {
                // todo: determine if we should push an error here
                // Are we going to allow trying to compile a program with semantic errors?
                None
            }
        }
    }

    /// Alias statements are compiled into the Q# ast as array concatenation for qubits
    /// and an compilation error for bit arrays.
    ///
    /// All of the heavy lifting is done in the lowerer, which transforms the
    /// semantic AST into a form that can be easily compiled into Q#.
    ///
    /// So here we compile each array expression and build up a binary op addition
    /// if there is more than one expression to concatenate.
    fn compile_alias_decl_stmt(&mut self, stmt: &semast::AliasDeclStmt) -> Option<qsast::Stmt> {
        let symbol = self.symbols[stmt.symbol_id].clone();
        if matches!(symbol.ty, Type::BitArray(..)) {
            self.push_unimplemented_error_message("bit register alias statements", stmt.span);
            return None;
        }
        let exprs = stmt
            .exprs
            .iter()
            .map(|expr| self.compile_expr(expr))
            .collect::<Vec<_>>();

        assert!(
            !stmt.exprs.is_empty(),
            "alias decl must have at least one expression"
        );

        let mut expr_iter = exprs.into_iter();
        let mut expr = expr_iter
            .next()
            .expect("alias decl must have at least one expression");

        for rhs in expr_iter {
            let span = Span {
                lo: expr.span.lo,
                hi: rhs.span.hi,
            };
            expr = build_binary_expr(false, qsast::BinOp::Add, expr, rhs, span);
        }

        let ty = self.map_semantic_type_to_qsharp_type(&symbol.ty, symbol.ty_span);
        let is_const = matches!(
            ty,
            crate::types::Type::Qubit | crate::types::Type::QubitArray(..)
        ) || symbol.ty.is_const();

        let decl = build_classical_decl(
            &symbol.name,
            is_const,
            symbol.ty_span,
            stmt.span,
            symbol.span,
            &ty,
            expr,
        );

        Some(decl)
    }

    fn compile_concat_expr(&mut self, expr: &semast::ConcatExpr) -> qsast::Expr {
        let exprs = expr
            .operands
            .iter()
            .map(|expr| self.compile_expr(expr))
            .collect::<Vec<_>>();

        assert!(
            exprs.len() >= 2,
            "the parser guarantees that a concat expression has at least two operands"
        );

        let mut expr_iter = exprs.into_iter();
        let mut expr = expr_iter
            .next()
            .expect("concat exprs must have at least one expression");

        for rhs in expr_iter {
            let span = Span {
                lo: expr.span.lo,
                hi: rhs.span.hi,
            };
            expr = build_binary_expr(false, qsast::BinOp::Add, expr, rhs, span);
        }

        expr
    }

    fn compile_assign_stmt(&mut self, stmt: &semast::AssignStmt) -> Option<qsast::Stmt> {
        let lhs = self.compile_expr(&stmt.lhs);
        let rhs = self.compile_expr(&stmt.rhs);
        Some(build_assignment_statement(lhs, rhs, stmt.span))
    }

    fn compile_indexed_classical_type_assign_stmt(
        &mut self,
        stmt: &semast::IndexedClassicalTypeAssignStmt,
    ) -> Option<qsast::Stmt> {
        // Invariant: The lowerer ensures that we only get here if the
        //            rhs can be assigned to the fully indexed rhs.

        // Compile the partially indexed lhs.
        let lhs = self.compile_expr(&stmt.lhs);

        // Compile the rhs, which already was casted to the type of the fully indexed lhs.
        let rhs = self.compile_expr(&stmt.rhs);
        let rhs_span = stmt.rhs.span;

        // Now we build a Block expr in which we will:
        //  1. Create a temp_var initialized to the partially indexed rhs casted to bitarray.
        //  2. Fully index the temp_var and assign the rhs to it.
        //  3. Return the modified temp_var casted back to the type of the partially indexed lhs.

        // 1. Create a temp_var initialized to the partially indexed lhs casted to bitarray.
        let width = stmt
            .lhs
            .ty
            .width()
            .expect("we only got here if ty is a sized int, uint, or angle");
        // 1.1 First we cast the partially indexed lhs to bitarray.
        let temp_var_stmt_init_expr = self.compile_expr(&semast::Expr {
            span: rhs_span,
            kind: Box::new(semast::ExprKind::Cast(semast::Cast {
                span: stmt.rhs.span,
                ty: Type::BitArray(width, false),
                expr: stmt.lhs.clone(),
                kind: semast::CastKind::Implicit,
                ty_exprs: list_from_iter([]),
            })),
            const_value: None,
            ty: Type::BitArray(width, false),
        });
        // 1.2 Then we build the temp_var.
        let temp_var_stmt = build_classical_decl(
            "bitarray",
            false,
            rhs_span,
            rhs_span,
            rhs_span,
            &crate::types::Type::ResultArray(crate::types::ArrayDimensions::One, false),
            temp_var_stmt_init_expr,
        );
        let temp_var_expr = build_path_ident_expr("bitarray", rhs_span, rhs_span);

        // 2. Fully index the temp_var and assign the rhs to it.
        // 2.1 Finish indexing the lhs with the classical indices.
        let mut update_stmt_lhs = temp_var_expr.clone();
        for index in &stmt.indices {
            let index = self.compile_index(index);
            update_stmt_lhs = build_index_expr(update_stmt_lhs, index, lhs.span);
        }

        // 2.2 Assign the rhs to the fully indexed temp_var.
        let update_stmt = build_assignment_statement(update_stmt_lhs, rhs, stmt.span);

        // 3. Return the modified temp_var casted back to the type of the partially indexed lhs.
        // 3.1 First we cast the temp_var back to the lhs type.
        let output_expr = Self::cast_bit_array_expr_to_ty(
            temp_var_expr,
            &Type::BitArray(width, false),
            &stmt.lhs.ty,
            width,
            rhs_span,
        );

        // 3.2 Then we build the implicit return.
        let implicit_return = build_implicit_return_stmt(output_expr);

        // Finally we build the Block expr.
        let block = qsast::Block {
            id: Default::default(),
            span: rhs_span,
            stmts: list_from_iter(vec![temp_var_stmt, update_stmt, implicit_return]),
        };

        let rhs = qsast::Expr {
            id: Default::default(),
            span: rhs_span,
            kind: Box::new(qsast::ExprKind::Block(Box::new(block))),
        };

        Some(build_assignment_statement(lhs, rhs, stmt.span))
    }

    fn compile_barrier_stmt(stmt: &semast::BarrierStmt) -> Option<qsast::Stmt> {
        Some(build_barrier_call(stmt.span))
    }

    fn compile_box_stmt(&mut self, stmt: &semast::BoxStmt) -> Option<qsast::Stmt> {
        // We don't support boxes with duration, so we report an error if it exists.
        if let Some(duration) = &stmt.duration {
            self.push_unsupported_error_message("box with duration", duration.span);
        }

        let open = self
            .pragma_config
            .get(PragmaKind::QdkBoxOpen)
            .map(|name| build_call_stmt_no_params(name, &[], Span::default(), Span::default()));
        let close = self
            .pragma_config
            .get(PragmaKind::QdkBoxClose)
            .map(|name| build_call_stmt_no_params(name, &[], Span::default(), Span::default()));

        let body = stmt
            .body
            .iter()
            .filter_map(|stmt| self.compile_stmt(stmt))
            .collect::<Vec<_>>();

        let mut stmts = vec![];
        if let Some(open) = open {
            stmts.push(open);
        }
        stmts.extend(body);
        if let Some(close) = close {
            stmts.push(close);
        }

        let block = qsast::Block {
            id: qsast::NodeId::default(),
            stmts: list_from_iter(stmts),
            span: stmt.span,
        };

        Some(build_stmt_semi_from_expr(build_wrapped_block_expr(block)))
    }

    fn compile_block(&mut self, block: &semast::Block) -> qsast::Block {
        let stmts = block
            .stmts
            .iter()
            .filter_map(|stmt| self.compile_stmt(stmt))
            .collect::<Vec<_>>();
        qsast::Block {
            id: qsast::NodeId::default(),
            stmts: list_from_iter(stmts),
            span: block.span,
        }
    }

    fn compile_block_stmt(&mut self, block: &semast::Block) -> Option<qsast::Stmt> {
        let block = self.compile_block(block);
        Some(build_stmt_semi_from_expr(build_wrapped_block_expr(block)))
    }

    fn compile_break_stmt(&mut self, stmt: &semast::BreakStmt) -> Option<qsast::Stmt> {
        self.push_unsupported_error_message("break stmt", stmt.span);
        None
    }

    fn compile_calibration_stmt(&mut self, stmt: &semast::CalibrationStmt) -> Option<qsast::Stmt> {
        // Calibration statements are not supported in the QDK
        self.push_unsupported_error_message("calibration statements", stmt.span);
        None
    }

    fn compile_calibration_grammar_stmt(
        &mut self,
        stmt: &semast::CalibrationGrammarStmt,
    ) -> Option<qsast::Stmt> {
        // Calibration grammar statements are not supported in the QDK
        self.push_unsupported_error_message("calibration grammar statements", stmt.span);
        None
    }

    fn compile_classical_decl(
        &mut self,
        decl: &semast::ClassicalDeclarationStmt,
    ) -> Option<qsast::Stmt> {
        let symbol = &self.symbols[decl.symbol_id].clone();
        let name = &symbol.name;
        let is_const = symbol.ty.is_const();
        let ty_span = decl.ty_span;
        let decl_span = decl.span;
        let name_span = symbol.span;
        let qsharp_ty = self.map_semantic_type_to_qsharp_type(&symbol.ty, symbol.ty_span);
        let expr = decl.init_expr.as_ref();

        let expr = self.compile_expr(expr);
        let stmt = build_classical_decl(
            name, is_const, ty_span, decl_span, name_span, &qsharp_ty, expr,
        );

        Some(stmt)
    }

    fn compile_continue_stmt(&mut self, stmt: &semast::ContinueStmt) -> Option<qsast::Stmt> {
        self.push_unsupported_error_message("continue stmt", stmt.span);
        None
    }

    fn compile_def_stmt(
        &mut self,
        stmt: &semast::DefStmt,
        annotations: &List<semast::Annotation>,
    ) -> Option<qsast::Stmt> {
        let symbol = self.symbols[stmt.symbol_id].clone();
        let return_type = match &symbol.ty {
            Type::Function(_, return_type) => return_type,
            _ => {
                // this can happen if the def statement shadows a non-def symbol
                // Since the symbol is not a function, we assume it returns an error type.
                // There is already an error reported for this case.
                &Arc::from(crate::semantic::types::Type::Err)
            }
        };

        let name = symbol.name.clone();

        let args: Vec<_> = stmt
            .params
            .iter()
            .map(|arg| {
                let symbol = self.symbols[arg.symbol_id].clone();
                let name = symbol.name.clone();
                let semantic_type = symbol.ty.clone();
                let qsharp_ty = self.map_semantic_type_to_qsharp_type(&symbol.ty, symbol.ty_span);
                let ast_type = map_qsharp_type_to_ast_ty(&qsharp_ty, symbol.ty_span);
                (
                    name.clone(),
                    ast_type.clone(),
                    build_arg_pat(name, symbol.span, ast_type),
                    semantic_type,
                )
            })
            .collect();

        let body = self.compile_block(&stmt.body);
        let body = Self::prepend_argument_validation_to_block(body, &args);
        let qsharp_ty = self.map_semantic_type_to_qsharp_type(return_type, stmt.return_type_span);
        let return_type = map_qsharp_type_to_ast_ty(&qsharp_ty, stmt.return_type_span);
        let kind = if stmt.has_qubit_params
            || annotations
                .iter()
                .any(|annotation| Self::is_simulatable_intrinsic(annotation))
        {
            qsast::CallableKind::Operation
        } else {
            qsast::CallableKind::Function
        };

        let attrs = annotations
            .iter()
            .filter_map(|annotation| self.compile_annotation(annotation));

        // We use the same primitives used for declaring gates, because def declarations
        // in QASM can take qubits as arguments and call quantum gates.
        Some(build_function_or_operation(
            name,
            args,
            vec![],
            body,
            symbol.span,
            stmt.body.span,
            stmt.span,
            return_type,
            kind,
            None,
            list_from_iter(attrs),
        ))
    }

    fn compile_def_cal_stmt(&mut self, stmt: &semast::DefCalStmt) -> Option<qsast::Stmt> {
        self.push_unsupported_error_message("def cal statements", stmt.span);
        None
    }

    fn compile_delay_stmt(&mut self, stmt: &semast::DelayStmt) -> Option<qsast::Stmt> {
        self.push_unsupported_error_message("delay statements", stmt.span);
        None
    }

    fn compile_end_stmt(stmt: &semast::EndStmt) -> Option<qsast::Stmt> {
        Some(build_end_stmt(stmt.span))
    }

    fn compile_expr_stmt(&mut self, stmt: &semast::ExprStmt) -> Option<qsast::Stmt> {
        let expr = self.compile_expr(&stmt.expr);
        Some(build_stmt_semi_from_expr_with_span(expr, stmt.span))
    }

    fn compile_extern_stmt(&mut self, stmt: &semast::ExternDecl) -> Option<qsast::Stmt> {
        self.push_unimplemented_error_message("extern statements", stmt.span);
        None
    }

    fn compile_for_stmt(&mut self, stmt: &semast::ForStmt) -> Option<qsast::Stmt> {
        let loop_var = self.symbols[stmt.loop_variable].clone();
        let iterable = self.compile_enumerable_set(&stmt.set_declaration);
        let body = self.compile_block(&Self::stmt_as_block(&stmt.body));
        let qsharp_ty = self.map_semantic_type_to_qsharp_type(&loop_var.ty, loop_var.ty_span);

        Some(build_for_stmt(
            &loop_var.name,
            loop_var.span,
            &qsharp_ty,
            loop_var.ty_span,
            iterable,
            body,
            stmt.span,
        ))
    }

    fn compile_if_stmt(&mut self, stmt: &semast::IfStmt) -> Option<qsast::Stmt> {
        let condition = self.compile_expr(&stmt.condition);
        let then_block = self.compile_block(&Self::stmt_as_block(&stmt.if_body));
        let else_block = stmt
            .else_body
            .as_ref()
            .map(|stmt| self.compile_block(&Self::stmt_as_block(stmt)));

        let if_expr = if let Some(else_block) = else_block {
            build_if_expr_then_block_else_block(condition, then_block, else_block, stmt.span)
        } else {
            build_if_expr_then_block(condition, then_block, stmt.span)
        };

        Some(build_stmt_semi_from_expr(if_expr))
    }

    fn stmt_as_block(stmt: &semast::Stmt) -> semast::Block {
        match &*stmt.kind {
            semast::StmtKind::Block(block) => *block.to_owned(),
            _ => semast::Block {
                span: stmt.span,
                stmts: list_from_iter([stmt.clone()]),
            },
        }
    }

    fn compile_function_call_expr(&mut self, expr: &semast::FunctionCall) -> qsast::Expr {
        let symbol = self.symbols[expr.symbol_id].clone();
        let name = &symbol.name;
        let name_span = expr.fn_name_span;
        if expr.args.is_empty() {
            build_call_no_params(name, &[], expr.span, expr.fn_name_span)
        } else {
            let args: Vec<_> = expr
                .args
                .iter()
                .map(|expr| self.compile_expr(expr))
                .collect();

            if args.len() == 1 {
                let operand_span = expr.args[0].span;
                let operand = args.into_iter().next().expect("there is one argument");
                build_call_with_param(name, &[], operand, name_span, operand_span, expr.span)
            } else {
                build_call_with_params(name, &[], args, name_span, expr.span)
            }
        }
    }

    fn compile_sizeof_call_expr(&mut self, expr: &semast::SizeofCallExpr) -> qsast::Expr {
        let span = expr.span;
        let name_span = expr.fn_name_span;
        let array = self.compile_expr(&expr.array);
        let dim = self.compile_expr(&expr.dim);
        let operands = vec![array, dim];
        let array_dims = expr.array_dims;
        assert!(
            (1..=7).contains(&array_dims),
            "array dimension should be between 1 and 7"
        );
        let fn_name = format!("sizeof_{array_dims}");
        build_call_with_params(
            &fn_name,
            &["Std", "OpenQASM", "Builtin"],
            operands,
            name_span,
            span,
        )
    }

    fn compile_durationof_call_expr(&mut self, expr: &semast::DurationofCallExpr) -> qsast::Expr {
        self.push_unsupported_error_message("durationof call", expr.span);
        err_expr(expr.span)
    }

    fn compile_gate_call_stmt(&mut self, stmt: &semast::GateCall) -> Option<qsast::Stmt> {
        if let Some(duration) = &stmt.duration {
            self.push_unsupported_error_message("gate call duration", duration.span);
        }

        let symbol = self.symbols[stmt.symbol_id].clone();
        let mut qubits: Vec<_> = stmt
            .qubits
            .iter()
            .map(|q| self.compile_gate_operand(q))
            .collect();
        let args: Vec<_> = stmt.args.iter().map(|arg| self.compile_expr(arg)).collect();

        // Take the number of qubit args that the gates expects from the source qubits.
        let gate_qubits =
            qubits.split_off(qubits.len().saturating_sub(stmt.quantum_arity as usize));

        // Then merge the classical args with the qubit args. This will give
        // us the args for the call prior to wrapping in tuples for controls.
        let args: Vec<_> = args.into_iter().chain(gate_qubits).collect();
        let mut args = build_gate_call_param_expr(args, qubits.len());
        let mut callee = build_path_ident_expr(&symbol.name, stmt.gate_name_span, stmt.span);

        for modifier in &stmt.modifiers {
            match &modifier.kind {
                semast::GateModifierKind::Inv => {
                    callee = build_unary_op_expr(
                        qsast::UnOp::Functor(qsast::Functor::Adj),
                        callee,
                        modifier.modifier_keyword_span,
                    );
                }
                semast::GateModifierKind::Pow(expr) => {
                    let exponent_expr = self.compile_expr(expr);
                    args = build_tuple_expr(vec![exponent_expr, callee, args]);
                    callee =
                        build_path_ident_expr("ApplyOperationPowerA", modifier.span, stmt.span);
                }
                semast::GateModifierKind::Ctrl(num_ctrls) => {
                    let num_ctrls = num_ctrls.get_const_u32()?;

                    // remove the last n qubits from the qubit list
                    if qubits.len() < num_ctrls as usize {
                        let kind = CompilerErrorKind::InvalidNumberOfQubitArgs(
                            num_ctrls as usize,
                            qubits.len(),
                            modifier.span,
                        );
                        self.push_compiler_error(kind);
                        return None;
                    }
                    let ctrl = qubits.split_off(qubits.len().saturating_sub(num_ctrls as usize));
                    let ctrls = build_expr_array_expr(ctrl, modifier.span);
                    args = build_tuple_expr(vec![ctrls, args]);
                    callee = build_unary_op_expr(
                        qsast::UnOp::Functor(qsast::Functor::Ctl),
                        callee,
                        modifier.modifier_keyword_span,
                    );
                }
                semast::GateModifierKind::NegCtrl(num_ctrls) => {
                    let num_ctrls = num_ctrls.get_const_u32()?;

                    // remove the last n qubits from the qubit list
                    if qubits.len() < num_ctrls as usize {
                        let kind = CompilerErrorKind::InvalidNumberOfQubitArgs(
                            num_ctrls as usize,
                            qubits.len(),
                            modifier.span,
                        );
                        self.push_compiler_error(kind);
                        return None;
                    }
                    let ctrl = qubits.split_off(qubits.len().saturating_sub(num_ctrls as usize));
                    let ctrls = build_expr_array_expr(ctrl, modifier.span);
                    let lit_0 = build_lit_int_expr(0, Span::default());
                    args = build_tuple_expr(vec![lit_0, callee, ctrls, args]);
                    callee = build_path_ident_expr(
                        "ApplyControlledOnInt",
                        modifier.modifier_keyword_span,
                        stmt.span,
                    );
                }
            }
        }

        let expr = build_gate_call_with_params_and_callee(args, callee, stmt.span);
        Some(build_stmt_semi_from_expr(expr))
    }

    fn compile_include_stmt(&mut self, stmt: &semast::IncludeStmt) -> Option<qsast::Stmt> {
        self.push_unimplemented_error_message("include statements", stmt.span);
        None
    }

    #[allow(clippy::unused_self)]
    fn compile_input_decl_stmt(&mut self, _stmt: &semast::InputDeclaration) -> Option<qsast::Stmt> {
        None
    }

    fn compile_output_decl_stmt(
        &mut self,
        stmt: &semast::OutputDeclaration,
    ) -> Option<qsast::Stmt> {
        let symbol = &self.symbols[stmt.symbol_id];

        // input decls should have been pushed to symbol table,
        // but should not be in the stmts list.
        if symbol.io_kind != IOKind::Output {
            return None;
        }

        let symbol = symbol.clone();
        let name = &symbol.name;
        let is_const = symbol.ty.is_const();
        let ty_span = stmt.ty_span; // todo
        let decl_span = stmt.span;
        let name_span = symbol.span;
        let qsharp_ty = self.map_semantic_type_to_qsharp_type(&symbol.ty, symbol.ty_span);

        let expr = stmt.init_expr.as_ref();

        let expr = self.compile_expr(expr);
        let stmt = build_classical_decl(
            name, is_const, ty_span, decl_span, name_span, &qsharp_ty, expr,
        );

        Some(stmt)
    }

    fn compile_measure_stmt(&mut self, stmt: &semast::MeasureArrowStmt) -> Option<qsast::Stmt> {
        self.push_unimplemented_error_message("measure statements", stmt.span);
        None
    }

    fn compile_pragma_stmt(&mut self, stmt: &semast::Pragma) {
        fn is_parameterless_and_returns_void(args: &Arc<[Type]>, return_ty: &Arc<Type>) -> bool {
            args.is_empty() && matches!(&**return_ty, crate::semantic::types::Type::Void)
        }

        let name_str = stmt
            .identifier
            .as_ref()
            .map_or_else(String::new, PathKind::as_string);

        // Check if the pragma is supported by the compiler.
        // If not, we push an error message and return.
        if !self.pragma_config.is_supported(&name_str) {
            self.push_unsupported_error_message(format!("pragma statement: {name_str}"), stmt.span);
            return;
        }

        // The pragma is supported, so we get the pragma kind.
        let pragma = PragmaKind::from_str(&name_str).expect("valid pragma");

        match (pragma, stmt.value.as_ref()) {
            (PragmaKind::QdkBoxOpen, Some(value)) => {
                if let Ok(symbol) = self.symbols.get_symbol_by_name(value)
                    && let crate::semantic::types::Type::Function(args, return_ty) = &symbol.1.ty
                    && is_parameterless_and_returns_void(args, return_ty)
                {
                    self.pragma_config
                        .insert(PragmaKind::QdkBoxOpen, value.clone());
                    return;
                }
                self.push_compiler_error(CompilerErrorKind::InvalidBoxPragmaTarget(
                    value.to_string(),
                    stmt.value_span.unwrap_or(stmt.span),
                ));
            }
            (PragmaKind::QdkBoxClose, Some(value)) => {
                if let Ok(symbol) = self.symbols.get_symbol_by_name(value)
                    && let crate::semantic::types::Type::Function(args, return_ty) = &symbol.1.ty
                    && is_parameterless_and_returns_void(args, return_ty)
                {
                    self.pragma_config
                        .insert(PragmaKind::QdkBoxClose, value.clone());
                    return;
                }
                self.push_compiler_error(CompilerErrorKind::InvalidBoxPragmaTarget(
                    value.to_string(),
                    stmt.value_span.unwrap_or(stmt.span),
                ));
            }
            (PragmaKind::QdkBoxOpen | PragmaKind::QdkBoxClose, None) => {
                self.push_compiler_error(CompilerErrorKind::MissingBoxPragmaTarget(stmt.span));
            }
            (PragmaKind::QdkQirProfile, Some(profile)) => {
                // For this pragma, we only keep the first instance.
                if Profile::from_str(profile).is_ok() {
                    if !self
                        .pragma_config
                        .pragmas
                        .contains_key(&PragmaKind::QdkQirProfile)
                    {
                        self.pragma_config
                            .insert(PragmaKind::QdkQirProfile, profile.clone());
                    }
                    return;
                }
                self.push_compiler_error(CompilerErrorKind::InvalidProfilePragmaTarget(
                    profile.to_string(),
                    stmt.value_span.unwrap_or(stmt.span),
                ));
            }
            (PragmaKind::QdkQirProfile, None) => {
                self.push_compiler_error(CompilerErrorKind::InvalidProfilePragmaTarget(
                    String::new(),
                    stmt.span,
                ));
            }
        }
    }

    fn compile_gate_decl_stmt(
        &mut self,
        stmt: &semast::QuantumGateDefinition,
        annotations: &List<semast::Annotation>,
    ) -> Option<qsast::Stmt> {
        let symbol = self.symbols[stmt.symbol_id].clone();
        let name = symbol.name.clone();
        // if the gate has the name of a qasm or qiskit built-in gate
        // it means that the stdgates libraries are not being used.
        // we let the user compile their own gates with the same name.

        let cargs: Vec<_> = stmt
            .params
            .iter()
            .map(|arg| {
                let symbol = self.symbols[*arg].clone();
                let name = symbol.name.clone();
                let semantic_type = symbol.ty.clone();
                let qsharp_ty = self.map_semantic_type_to_qsharp_type(&symbol.ty, symbol.ty_span);
                let ast_type = map_qsharp_type_to_ast_ty(&qsharp_ty, symbol.ty_span);
                (
                    name.clone(),
                    ast_type.clone(),
                    build_arg_pat(name, symbol.span, ast_type),
                    semantic_type,
                )
            })
            .collect();

        let qargs: Vec<_> = stmt
            .qubits
            .iter()
            .map(|arg| {
                let symbol = self.symbols[*arg].clone();
                let name = symbol.name.clone();
                let semantic_type = symbol.ty.clone();
                let qsharp_ty = self.map_semantic_type_to_qsharp_type(&symbol.ty, symbol.ty_span);
                let ast_type = map_qsharp_type_to_ast_ty(&qsharp_ty, symbol.ty_span);
                (
                    name.clone(),
                    ast_type.clone(),
                    build_arg_pat(name, symbol.span, ast_type),
                    semantic_type,
                )
            })
            .collect();

        let body = Some(self.compile_block(&stmt.body));

        let attrs = annotations
            .iter()
            .filter_map(|annotation| self.compile_annotation(annotation));

        // Do not compile functors if we have the simulatable intrinsic annotation.
        let functors = if annotations
            .iter()
            .any(|annotation| Self::is_simulatable_intrinsic(annotation))
        {
            None
        } else {
            Some(build_adj_plus_ctl_functor())
        };

        Some(build_function_or_operation(
            name,
            cargs,
            qargs,
            body,
            stmt.name_span,
            stmt.body.span,
            stmt.span,
            build_path_ident_ty("Unit"),
            qsast::CallableKind::Operation,
            functors,
            list_from_iter(attrs),
        ))
    }

    fn compile_annotation(&mut self, annotation: &semast::Annotation) -> Option<qsast::Attr> {
        let name = annotation.identifier.as_string();
        match name.as_str() {
            QSHARP_QIR_INTRINSIC_ANNOTATION | QSHARP_CONFIG_ANNOTATION => {
                Some(build_attr(name, annotation.value.clone(), annotation.span))
            }
            QDK_CONFIG_ANNOTATION => Some(build_attr(
                QSHARP_CONFIG_ANNOTATION,
                annotation.value.clone(),
                annotation.span,
            )),
            QDK_QIR_INTRINSIC_ANNOTATION => {
                // Map the QDK QIR intrinsic annotation to the simulatable intrinsic annotation
                // which is used by the Q# compiler
                Some(build_attr(
                    QSHARP_QIR_INTRINSIC_ANNOTATION,
                    annotation.value.clone(),
                    annotation.span,
                ))
            }
            _ => {
                self.push_compiler_error(CompilerErrorKind::UnknownAnnotation(
                    format!("@{name}"),
                    annotation.span,
                ));
                None
            }
        }
    }

    fn compile_qubit_decl_stmt(&mut self, stmt: &semast::QubitDeclaration) -> Option<qsast::Stmt> {
        let symbol = self.symbols[stmt.symbol_id].clone();
        let name = &symbol.name;
        let name_span = symbol.span;

        let stmt = match self.config.qubit_semantics {
            QubitSemantics::QSharp => build_managed_qubit_alloc(name, stmt.span, name_span),
            QubitSemantics::Qiskit => build_unmanaged_qubit_alloc(name, stmt.span, name_span),
        };
        Some(stmt)
    }

    fn compile_qubit_array_decl_stmt(
        &mut self,
        stmt: &semast::QubitArrayDeclaration,
    ) -> Option<qsast::Stmt> {
        let symbol = self.symbols[stmt.symbol_id].clone();
        let name = &symbol.name;
        let name_span = symbol.span;

        let stmt = match self.config.qubit_semantics {
            QubitSemantics::QSharp => {
                let size = stmt.size.get_const_u32()?;
                managed_qubit_alloc_array(name, size, stmt.span, name_span, stmt.size_span)
            }
            QubitSemantics::Qiskit => build_unmanaged_qubit_alloc_array(
                name,
                stmt.size.get_const_u32()?,
                stmt.span,
                name_span,
                stmt.size_span,
            ),
        };
        Some(stmt)
    }

    fn compile_reset_stmt(&mut self, stmt: &semast::ResetStmt) -> Option<qsast::Stmt> {
        let is_register = matches!(stmt.operand.kind, crate::semantic::ast::GateOperandKind::Expr(ref expr) if matches!(expr.ty, Type::QubitArray(..)));

        let operand = self.compile_gate_operand(&stmt.operand);
        let operand_span = operand.span;
        let expr = if is_register {
            build_reset_all_call(operand, stmt.reset_token_span, operand_span)
        } else {
            build_reset_call(operand, stmt.reset_token_span, operand_span)
        };
        Some(build_stmt_semi_from_expr(expr))
    }

    fn compile_return_stmt(&mut self, stmt: &semast::ReturnStmt) -> Option<qsast::Stmt> {
        let expr = stmt.expr.as_ref().map(|expr| self.compile_expr(expr));

        let expr = if let Some(expr) = expr {
            build_return_expr(expr, stmt.span)
        } else {
            build_return_unit(stmt.span)
        };

        Some(build_stmt_semi_from_expr(expr))
    }

    fn compile_switch_stmt(&mut self, stmt: &semast::SwitchStmt) -> Option<qsast::Stmt> {
        // For each case, convert the lhs into a sequence of equality checks
        // and then fold them into a single expression of logical ors for
        // the if expr
        let control = self.compile_expr(&stmt.target);
        let cases: Vec<(qsast::Expr, qsast::Block)> = stmt
            .cases
            .iter()
            .map(|case| {
                let block = self.compile_block(&case.block);

                let case = case
                    .labels
                    .iter()
                    .map(|label| {
                        let lhs = control.clone();
                        let rhs = self.compile_expr(label);
                        build_binary_expr(false, qsast::BinOp::Eq, lhs, rhs, label.span)
                    })
                    .fold(None, |acc, expr| match acc {
                        None => Some(expr),
                        Some(acc) => {
                            let qsop = qsast::BinOp::OrL;
                            let span = Span {
                                lo: acc.span.lo,
                                hi: expr.span.hi,
                            };
                            Some(build_binary_expr(false, qsop, acc, expr, span))
                        }
                    });
                // The type checker doesn't know that we have at least one case
                // so we have to unwrap here since the accumulation is guaranteed
                // to have Some(value)
                let case = case.expect("Case must have at least one expression");
                (case, block)
            })
            .collect();

        let default_block = stmt.default.as_ref().map(|block| self.compile_block(block));

        let default_expr = default_block.map(build_wrapped_block_expr);
        let if_expr = cases
            .into_iter()
            .rev()
            .fold(default_expr, |else_expr, (cond, block)| {
                let span = Span {
                    lo: cond.span.lo,
                    hi: block.span.hi,
                };
                Some(build_if_expr_then_block_else_expr(
                    cond, block, else_expr, span,
                ))
            });
        if_expr.map(build_stmt_semi_from_expr)
    }

    fn compile_while_stmt(&mut self, stmt: &semast::WhileLoop) -> Option<qsast::Stmt> {
        let condition = self.compile_expr(&stmt.condition);
        match &*stmt.body.kind {
            semast::StmtKind::Block(block) => {
                let block = self.compile_block(block);
                Some(build_while_stmt(condition, block, stmt.span))
            }
            semast::StmtKind::Err => Some(qsast::Stmt {
                id: NodeId::default(),
                span: stmt.body.span,
                kind: Box::new(qsast::StmtKind::Err),
            }),
            _ => {
                let block_stmt = self.compile_stmt(&stmt.body)?;
                let block = qsast::Block {
                    id: qsast::NodeId::default(),
                    stmts: list_from_iter([block_stmt]),
                    span: stmt.span,
                };
                Some(build_while_stmt(condition, block, stmt.span))
            }
        }
    }

    fn compile_expr(&mut self, expr: &semast::Expr) -> qsast::Expr {
        if expr.ty.is_const()
            && let Some(value) = expr.get_const_value()
        {
            return self.compile_literal_expr(&value, expr.span);
        }

        match expr.kind.as_ref() {
            semast::ExprKind::Err => qsast::Expr {
                span: expr.span,
                ..Default::default()
            },
            semast::ExprKind::CapturedIdent(symbol_id) => {
                self.compile_captured_ident_expr(*symbol_id, expr.span)
            }
            semast::ExprKind::Ident(symbol_id) => self.compile_ident_expr(*symbol_id, expr.span),
            semast::ExprKind::UnaryOp(unary_op_expr) => self.compile_unary_op_expr(unary_op_expr),
            semast::ExprKind::BinaryOp(binary_op_expr) => {
                self.compile_binary_op_expr(binary_op_expr)
            }
            semast::ExprKind::Lit(literal_kind) => {
                self.compile_literal_expr(literal_kind, expr.span)
            }
            semast::ExprKind::FunctionCall(function_call) => {
                self.compile_function_call_expr(function_call)
            }
            semast::ExprKind::BuiltinFunctionCall(_) => {
                let Some(value) = expr.get_const_value() else {
                    unreachable!("builtin function call exprs are only lowered if they succeed");
                };

                self.compile_literal_expr(&value, expr.span)
            }
            semast::ExprKind::Cast(cast) => self.compile_cast_expr(cast),
            semast::ExprKind::IndexedExpr(index_expr) => self.compile_indexed_expr(index_expr),
            semast::ExprKind::Paren(pexpr) => self.compile_paren_expr(pexpr, expr.span),
            semast::ExprKind::Measure(mexpr) => self.compile_measure_expr(mexpr, &expr.ty),
            semast::ExprKind::SizeofCall(sizeof_call) => self.compile_sizeof_call_expr(sizeof_call),
            semast::ExprKind::Concat(concat) => self.compile_concat_expr(concat),
            semast::ExprKind::DurationofCall(duration_call) => {
                self.compile_durationof_call_expr(duration_call)
            }
        }
    }

    fn compile_captured_ident_expr(&mut self, symbol_id: SymbolId, span: Span) -> qsast::Expr {
        let symbol = &self.symbols[symbol_id];
        // when closing over a constant value we will have a const value
        // associated with the symbol, but due to scoping rule differences
        // we have to "copy" the value into the usage.
        let Some(value) = symbol.get_const_value() else {
            unreachable!("captured ident exprs should always have a const value");
        };
        self.compile_literal_expr(&value, span)
    }

    fn compile_ident_expr(&mut self, symbol_id: SymbolId, span: Span) -> qsast::Expr {
        let symbol = &self.symbols[symbol_id];
        match symbol.name.as_str() {
            "euler" | "ℇ" => build_math_call_no_params("E", span),
            "pi" | "π" => build_math_call_no_params("PI", span),
            "tau" | "τ" => {
                let expr = build_math_call_no_params("PI", span);
                qsast::Expr {
                    kind: Box::new(qsast::ExprKind::BinOp(
                        qsast::BinOp::Mul,
                        Box::new(build_lit_double_expr(2.0, span)),
                        Box::new(expr),
                    )),
                    span,
                    id: qsast::NodeId::default(),
                }
            }
            _ => build_path_ident_expr(&symbol.name, span, span),
        }
    }

    fn compile_unary_op_expr(&mut self, unary: &UnaryOpExpr) -> qsast::Expr {
        match unary.op {
            semast::UnaryOp::Neg => self.compile_neg_expr(&unary.expr, unary.span),
            semast::UnaryOp::NotB => self.compile_bitwise_not_expr(&unary.expr, unary.span),
            semast::UnaryOp::NotL => self.compile_logical_not_expr(&unary.expr, unary.span),
        }
    }
    fn compile_neg_expr(&mut self, expr: &Expr, span: Span) -> qsast::Expr {
        let compiled_expr = self.compile_expr(expr);

        if matches!(expr.ty, Type::Angle(..)) {
            build_angle_cast_call_by_name("NegAngle", compiled_expr, span, expr.span)
        } else {
            build_unary_op_expr(qsast::UnOp::Neg, compiled_expr, span)
        }
    }

    fn compile_bitwise_not_expr(&mut self, expr: &Expr, span: Span) -> qsast::Expr {
        let compiled_expr = self.compile_expr(expr);

        if matches!(expr.ty, Type::Angle(..)) {
            build_call_with_param(
                "AngleNotB",
                &["Std", "OpenQASM", "Angle"],
                compiled_expr,
                span,
                expr.span,
                span,
            )
        } else {
            build_unary_op_expr(qsast::UnOp::NotB, compiled_expr, span)
        }
    }

    fn compile_logical_not_expr(&mut self, expr: &Expr, span: Span) -> qsast::Expr {
        let expr = self.compile_expr(expr);
        build_unary_op_expr(qsast::UnOp::NotL, expr, span)
    }

    fn compile_binary_op_expr(&mut self, binary: &BinaryOpExpr) -> qsast::Expr {
        let op = Self::map_bin_op(binary.op);
        let lhs = self.compile_expr(&binary.lhs);
        let rhs = self.compile_expr(&binary.rhs);

        if matches!(&binary.lhs.ty, Type::Angle(..)) || matches!(&binary.rhs.ty, Type::Angle(..)) {
            return self.compile_angle_binary_op(op, lhs, rhs, &binary.lhs.ty, &binary.rhs.ty);
        }

        if matches!(&binary.lhs.ty, Type::Complex(..))
            || matches!(&binary.rhs.ty, Type::Complex(..))
        {
            return Self::compile_complex_binary_op(op, lhs, rhs);
        }

        let is_assignment = false;
        build_binary_expr(is_assignment, op, lhs, rhs, binary.span())
    }

    fn compile_angle_binary_op(
        &mut self,
        op: qsast::BinOp,
        lhs: qsast::Expr,
        rhs: qsast::Expr,
        lhs_ty: &crate::semantic::types::Type,
        rhs_ty: &crate::semantic::types::Type,
    ) -> qsast::Expr {
        let span = Span {
            lo: lhs.span.lo,
            hi: rhs.span.hi,
        };

        let mut operands = vec![lhs, rhs];

        let fn_name: &str = match op {
            // Bit shift
            qsast::BinOp::Shl => "__AngleShl__",
            qsast::BinOp::Shr => "AngleShr",

            // Bitwise
            qsast::BinOp::AndB => "AngleAndB",
            qsast::BinOp::OrB => "AngleOrB",
            qsast::BinOp::XorB => "AngleXorB",

            // Comparison
            qsast::BinOp::Eq => "AngleEq",
            qsast::BinOp::Neq => "AngleNeq",
            qsast::BinOp::Gt => "AngleGt",
            qsast::BinOp::Gte => "AngleGte",
            qsast::BinOp::Lt => "AngleLt",
            qsast::BinOp::Lte => "AngleLte",

            // Arithmetic
            qsast::BinOp::Add => "AddAngles",
            qsast::BinOp::Sub => "SubtractAngles",
            qsast::BinOp::Mul => {
                // if we are doing `int * angle` we need to
                // reverse the order of the args to MultiplyAngleByInt
                if matches!(lhs_ty, Type::Int(..) | Type::UInt(..)) {
                    operands.reverse();
                }
                "MultiplyAngleByInt"
            }
            qsast::BinOp::Div => {
                if matches!(lhs_ty, Type::Angle(..))
                    && matches!(rhs_ty, Type::Int(..) | Type::UInt(..))
                {
                    "DivideAngleByInt"
                } else {
                    "DivideAngleByAngle"
                }
            }

            _ => {
                self.push_unsupported_error_message("angle binary operation", span);
                return err_expr(span);
            }
        };

        build_call_with_params(fn_name, &["Std", "OpenQASM", "Angle"], operands, span, span)
    }

    fn compile_complex_binary_op(
        op: qsast::BinOp,
        lhs: qsast::Expr,
        rhs: qsast::Expr,
    ) -> qsast::Expr {
        let span = Span {
            lo: lhs.span.lo,
            hi: rhs.span.hi,
        };

        let fn_name: &str = match op {
            // Arithmetic
            qsast::BinOp::Add => "PlusC",
            qsast::BinOp::Sub => "MinusC",
            qsast::BinOp::Mul => "TimesC",
            qsast::BinOp::Div => "DividedByC",
            qsast::BinOp::Exp => "PowC",
            _ => {
                // we are already pushing a semantic error in the lowerer
                // if the operation is not supported. So, we just return
                // an Expr::Err here.
                return err_expr(span);
            }
        };

        build_math_call_from_exprs(fn_name, vec![lhs, rhs], span)
    }

    fn compile_literal_expr(&mut self, lit: &LiteralKind, span: Span) -> qsast::Expr {
        match lit {
            LiteralKind::Angle(value) => build_lit_angle_expr(*value, span),
            LiteralKind::Array(value) => self.compile_array_literal(value, span),
            LiteralKind::Bitstring(big_int, width) => {
                Self::compile_bitstring_literal(big_int, *width, span)
            }
            LiteralKind::Bit(value) => Self::compile_bit_literal(*value, span),
            LiteralKind::Bool(value) => Self::compile_bool_literal(*value, span),
            LiteralKind::Duration(duration) => {
                self.compile_duration_literal(duration.value, duration.unit, span)
            }
            LiteralKind::Float(value) => Self::compile_float_literal(*value, span),
            LiteralKind::Complex(value) => Self::compile_complex_literal(*value, span),
            LiteralKind::Int(value) => Self::compile_int_literal(*value, span),
            LiteralKind::BigInt(value) => Self::compile_bigint_literal(value, span),
        }
    }

    fn compile_cast_expr(&mut self, cast: &Cast) -> qsast::Expr {
        let expr = self.compile_expr(&cast.expr);
        let cast_expr = match cast.expr.ty {
            crate::semantic::types::Type::Bit(_) => {
                Self::cast_bit_expr_to_ty(expr, &cast.expr.ty, &cast.ty, cast.span)
            }
            crate::semantic::types::Type::Bool(_) => {
                Self::cast_bool_expr_to_ty(expr, &cast.expr.ty, &cast.ty, cast.span)
            }
            crate::semantic::types::Type::Duration(_) => {
                self.cast_duration_expr_to_ty(expr, &cast.expr.ty, &cast.ty, cast.span)
            }
            crate::semantic::types::Type::Angle(_, _) => {
                Self::cast_angle_expr_to_ty(expr, &cast.expr.ty, &cast.ty, cast.span)
            }
            crate::semantic::types::Type::Complex(_, _) => {
                self.cast_complex_expr_to_ty(expr, &cast.expr.ty, &cast.ty, cast.span)
            }
            crate::semantic::types::Type::Float(_, _) => {
                Self::cast_float_expr_to_ty(expr, &cast.expr.ty, &cast.ty, cast.span)
            }
            crate::semantic::types::Type::Int(_, _) | crate::semantic::types::Type::UInt(_, _) => {
                Self::cast_int_expr_to_ty(expr, &cast.expr.ty, &cast.ty, cast.span)
            }
            crate::semantic::types::Type::BitArray(size, _) => {
                Self::cast_bit_array_expr_to_ty(expr, &cast.expr.ty, &cast.ty, size, cast.span)
            }
            _ => err_expr(cast.span),
        };
        if matches!(*cast_expr.kind, qsast::ExprKind::Err) {
            self.push_unsupported_error_message(
                format!("casting {} to {} type", cast.expr.ty, cast.ty),
                cast.span,
            );
        }
        cast_expr
    }

    fn compile_indexed_expr(&mut self, index_expr: &IndexedExpr) -> qsast::Expr {
        let expr = self.compile_expr(&index_expr.collection);
        let index = self.compile_index(&index_expr.index);
        build_index_expr(expr, index, index_expr.span)
    }

    fn compile_paren_expr(&mut self, paren: &Expr, span: Span) -> qsast::Expr {
        let expr = self.compile_expr(paren);
        wrap_expr_in_parens(expr, span)
    }

    fn compile_measure_expr(
        &mut self,
        expr: &MeasureExpr,
        ty: &crate::semantic::types::Type,
    ) -> qsast::Expr {
        assert!(matches!(ty, Type::BitArray(..) | Type::Bit(..)));

        let call_span = expr.span;
        let name_span = expr.measure_token_span;
        let arg = self.compile_gate_operand(&expr.operand);
        let operand_span = expr.operand.span;
        if matches!(ty, Type::Bit(..)) {
            build_measure_call(arg, name_span, operand_span, call_span)
        } else {
            build_measureeachz_call(arg, name_span, operand_span, call_span)
        }
    }

    fn compile_gate_operand(&mut self, op: &GateOperand) -> qsast::Expr {
        match &op.kind {
            GateOperandKind::HardwareQubit(hw) => {
                // We don't support hardware qubits, so we need to push an error
                // but we can still create an identifier for the hardware qubit
                // and let the rest of the containing expression compile to
                // catch any other errors
                let message = "hardware qubit operands";
                self.push_unsupported_error_message(message, op.span);
                build_path_ident_expr(hw.name.clone(), hw.span, op.span)
            }
            GateOperandKind::Expr(expr) => self.compile_expr(expr),
            GateOperandKind::Err => err_expr(op.span),
        }
    }

    fn compile_index(&mut self, elem: &Index) -> qsast::Expr {
        match elem {
            Index::Expr(expr) => self.compile_expr(expr),
            Index::Range(range) => self.compile_range_expr(range),
        }
    }

    fn compile_set(&mut self, set: &Set) -> qsast::Expr {
        let expr_list: Vec<_> = set
            .values
            .iter()
            .map(|expr| self.compile_expr(expr))
            .collect();

        build_expr_array_expr(expr_list, set.span)
    }

    fn compile_enumerable_set(&mut self, set: &semast::EnumerableSet) -> qsast::Expr {
        match set {
            semast::EnumerableSet::Set(set) => self.compile_set(set),
            semast::EnumerableSet::Expr(expr) => self.compile_expr(expr),
            semast::EnumerableSet::Range(range) => self.compile_range_expr(range),
        }
    }

    fn compile_range_expr(&mut self, range: &semast::Range) -> qsast::Expr {
        let start = range.start.as_ref().map(|expr| self.compile_expr(expr));
        let step = range.step.as_ref().map(|expr| self.compile_expr(expr));
        let end = range.end.as_ref().map(|expr| self.compile_expr(expr));
        build_range_expr(start, step, end, range.span)
    }

    fn compile_array_literal(&mut self, array: &Array, span: Span) -> qsast::Expr {
        let exprs = array
            .data
            .iter()
            .map(|expr| self.compile_expr(expr))
            .collect();

        build_expr_array_expr(exprs, span)
    }

    fn compile_bit_literal(value: bool, span: Span) -> qsast::Expr {
        build_lit_result_expr(value.into(), span)
    }

    fn compile_bool_literal(value: bool, span: Span) -> qsast::Expr {
        build_lit_bool_expr(value, span)
    }

    fn compile_duration_literal(
        &mut self,
        _value: f64,
        _unit: TimeUnit,
        span: Span,
    ) -> qsast::Expr {
        self.push_unsupported_error_message("timing literals", span);
        err_expr(span)
    }

    fn compile_bitstring_literal(value: &BigInt, width: u32, span: Span) -> qsast::Expr {
        let width = width as usize;
        // Handle the special case where the value is zero and width is zero
        if value == &BigInt::ZERO && width == 0 {
            return build_lit_result_array_expr(vec![], span);
        }

        let binary = value.to_str_radix(2).into_bytes().into_iter().map(|b| {
            // the string bytes are ASCII bytes, so we check their value offset from b'0'
            if (b - b'0') == 0 {
                qsast::Result::Zero
            } else {
                qsast::Result::One
            }
        });
        // Pad the binary representation with leading zeros to match the width
        let values = if binary.len() < width {
            let mut padded = vec![qsast::Result::Zero; width - binary.len()];
            padded.extend(binary);
            padded
        } else {
            binary.collect()
        };

        build_lit_result_array_expr(values, span)
    }

    fn compile_complex_literal(value: Complex, span: Span) -> qsast::Expr {
        build_lit_complex_expr(crate::types::Complex::new(value.real, value.imag), span)
    }

    fn compile_float_literal(value: f64, span: Span) -> qsast::Expr {
        build_lit_double_expr(value, span)
    }

    fn compile_int_literal(value: i64, span: Span) -> qsast::Expr {
        build_lit_int_expr(value, span)
    }

    fn compile_bigint_literal(value: &BigInt, span: Span) -> qsast::Expr {
        build_lit_bigint_expr(value.clone(), span)
    }

    /// Pushes an unsupported error with the supplied message.
    pub fn push_unsupported_error_message<S: AsRef<str>>(&mut self, message: S, span: Span) {
        let kind = CompilerErrorKind::NotSupported(message.as_ref().to_string(), span);
        self.push_compiler_error(kind);
    }

    /// Pushes an unimplemented error with the supplied message.
    pub fn push_unimplemented_error_message<S: AsRef<str>>(&mut self, message: S, span: Span) {
        let kind = CompilerErrorKind::Unimplemented(message.as_ref().to_string(), span);
        self.push_compiler_error(kind);
    }

    /// Pushes a semantic error with the given kind.
    pub fn push_compiler_error(&mut self, kind: CompilerErrorKind) {
        let kind = crate::ErrorKind::Compiler(error::Error(kind));
        let error = crate::Error(kind);
        let error = WithSource::from_map(&self.source_map, error);
        self.errors.push(error);
    }

    /// +----------------+-------------------------------------------------------------+
    /// | Allowed casts  | Casting To                                                  |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    /// | Casting From   | bool  | int | uint | float | angle | bit | duration | qubit |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    /// | angle          | Yes   | No  | No   | No    | -     | Yes | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    fn cast_angle_expr_to_ty(
        expr: qsast::Expr,
        expr_ty: &crate::semantic::types::Type,
        ty: &crate::semantic::types::Type,
        span: Span,
    ) -> qsast::Expr {
        assert!(matches!(expr_ty, Type::Angle(..)));
        // https://openqasm.com/language/types.html#casting-from-angle
        match ty {
            Type::Angle(..) => {
                // we know they are both angles, here we promote the width.
                let promoted_ty = promote_types(expr_ty, ty);
                if promoted_ty.width().is_some() && promoted_ty.width() != expr_ty.width() {
                    // we need to convert the angle to a different width
                    let width = promoted_ty.width().expect("width should be set");
                    build_angle_convert_call_with_two_params(
                        "AdjustAngleSizeNoTruncation",
                        expr,
                        build_lit_int_expr(width.into(), span),
                        span,
                        span,
                    )
                } else {
                    expr
                }
            }
            Type::Bit(..) => build_angle_cast_call_by_name("AngleAsResult", expr, span, span),
            Type::BitArray(..) => {
                build_angle_cast_call_by_name("AngleAsResultArrayBE", expr, span, span)
            }
            Type::Bool(..) => build_angle_cast_call_by_name("AngleAsBool", expr, span, span),
            _ => err_expr(span),
        }
    }

    /// +----------------+-------------------------------------------------------------+
    /// | Allowed casts  | Casting To                                                  |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    /// | Casting From   | bool  | int | uint | float | angle | bit | duration | qubit |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    /// | bit            | Yes   | Yes | Yes  | No    | Yes   | -   | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    fn cast_bit_expr_to_ty(
        expr: qsast::Expr,
        expr_ty: &crate::semantic::types::Type,
        ty: &crate::semantic::types::Type,
        span: Span,
    ) -> qsast::Expr {
        assert!(matches!(expr_ty, Type::Bit(..)));
        // There is no operand, choosing the span of the node
        // but we could use the expr span as well.
        let operand_span = expr.span;
        let name_span = span;
        match ty {
            Type::Angle(..) => {
                build_angle_cast_call_by_name("ResultAsAngle", expr, name_span, operand_span)
            }
            Type::Bool(..) => {
                build_convert_cast_call_by_name("ResultAsBool", expr, name_span, operand_span)
            }
            Type::Float(..) => {
                build_convert_cast_call_by_name("ResultAsDouble", expr, name_span, operand_span)
            }
            Type::Int(w, _) | Type::UInt(w, _) => {
                let function = if let Some(width) = w {
                    if *width > 64 {
                        "ResultAsBigInt"
                    } else {
                        "ResultAsInt"
                    }
                } else {
                    "ResultAsInt"
                };

                build_convert_cast_call_by_name(function, expr, name_span, operand_span)
            }
            Type::BitArray(size, _) => {
                let size_expr = build_lit_int_expr(i64::from(*size), Span::default());
                build_qasmstd_convert_call_with_two_params(
                    "ResultAsResultArrayBE",
                    expr,
                    size_expr,
                    name_span,
                    operand_span,
                )
            }
            _ => err_expr(span),
        }
    }

    fn cast_bit_array_expr_to_ty(
        expr: qsast::Expr,
        expr_ty: &crate::semantic::types::Type,
        ty: &crate::semantic::types::Type,
        size: u32,
        span: Span,
    ) -> qsast::Expr {
        assert!(matches!(expr_ty, Type::BitArray(_, _)));
        // There is no operand, choosing the span of the node
        // but we could use the expr span as well.
        let operand_span = expr.span;
        let name_span = span;

        match ty {
            Type::Bit(..) => build_convert_cast_call_by_name(
                "ResultArrayAsResultBE",
                expr,
                name_span,
                operand_span,
            ),
            Type::Bool(..) => {
                build_convert_cast_call_by_name("ResultArrayAsBool", expr, name_span, operand_span)
            }
            Type::Angle(Some(width), _) if *width == size => {
                build_angle_cast_call_by_name("ResultArrayAsAngleBE", expr, name_span, operand_span)
            }
            Type::Int(Some(width), _) | Type::UInt(Some(width), _) if *width == size => {
                build_convert_cast_call_by_name("ResultArrayAsIntBE", expr, name_span, operand_span)
            }
            Type::Int(None, _) | Type::UInt(None, _) => {
                build_convert_cast_call_by_name("ResultArrayAsIntBE", expr, name_span, operand_span)
            }
            _ => err_expr(span),
        }
    }

    /// +----------------+-------------------------------------------------------------+
    /// | Allowed casts  | Casting To                                                  |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    /// | Casting From   | bool  | int | uint | float | angle | bit | duration | qubit |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    /// | bool           | -     | Yes | Yes  | Yes   | No    | Yes | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    fn cast_bool_expr_to_ty(
        expr: qsast::Expr,
        expr_ty: &crate::semantic::types::Type,
        ty: &crate::semantic::types::Type,
        span: Span,
    ) -> qsast::Expr {
        assert!(matches!(expr_ty, Type::Bool(..)));
        let name_span = expr.span;
        let operand_span = span;
        match ty {
            Type::Bit(..) => {
                build_convert_cast_call_by_name("BoolAsResult", expr, name_span, operand_span)
            }
            Type::Float(..) => {
                build_convert_cast_call_by_name("BoolAsDouble", expr, name_span, operand_span)
            }
            Type::Int(w, _) | Type::UInt(w, _) => {
                let function = if let Some(width) = w {
                    if *width > 64 {
                        "BoolAsBigInt"
                    } else {
                        "BoolAsInt"
                    }
                } else {
                    "BoolAsInt"
                };
                build_convert_cast_call_by_name(function, expr, name_span, operand_span)
            }
            Type::BitArray(size, _) => {
                let size_expr = build_lit_int_expr(i64::from(*size), Span::default());
                build_qasmstd_convert_call_with_two_params(
                    "BoolAsResultArrayBE",
                    expr,
                    size_expr,
                    name_span,
                    operand_span,
                )
            }
            _ => err_expr(span),
        }
    }

    fn cast_complex_expr_to_ty(
        &mut self,
        _expr: qsast::Expr,
        _expr_ty: &crate::semantic::types::Type,
        _ty: &crate::semantic::types::Type,
        span: Span,
    ) -> qsast::Expr {
        self.push_unimplemented_error_message("cast complex expressions", span);
        err_expr(span)
    }

    fn cast_duration_expr_to_ty(
        &mut self,
        _expr: qsast::Expr,
        _expr_ty: &crate::semantic::types::Type,
        _ty: &crate::semantic::types::Type,
        span: Span,
    ) -> qsast::Expr {
        self.push_unimplemented_error_message("cast duration expressions", span);
        err_expr(span)
    }

    /// +----------------+-------------------------------------------------------------+
    /// | Allowed casts  | Casting To                                                  |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    /// | Casting From   | bool  | int | uint | float | angle | bit | duration | qubit |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    /// | float          | Yes   | Yes | Yes  | -     | Yes   | No  | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    ///
    /// Additional cast to complex
    fn cast_float_expr_to_ty(
        expr: qsast::Expr,
        expr_ty: &crate::semantic::types::Type,
        ty: &crate::semantic::types::Type,
        span: Span,
    ) -> qsast::Expr {
        assert!(matches!(expr_ty, Type::Float(..)));
        let name_span = expr.span;
        let operand_span = span;

        match ty {
            &Type::Complex(..) => build_complex_from_expr(expr),
            &Type::Angle(width, _) => {
                let expr_span = expr.span;
                let width =
                    build_lit_int_expr(width.unwrap_or(f64::MANTISSA_DIGITS).into(), expr_span);
                build_call_with_params(
                    "DoubleAsAngle",
                    &["Std", "OpenQASM", "Angle"],
                    vec![expr, width],
                    expr_span,
                    expr_span,
                )
            }
            &Type::Int(w, _) | &Type::UInt(w, _) => {
                let expr = build_math_call_from_exprs("Truncate", vec![expr], span);
                if let Some(w) = w {
                    if w > 64 {
                        build_convert_call_expr(expr, "IntAsBigInt")
                    } else {
                        expr
                    }
                } else {
                    expr
                }
            }
            // This is a width promotion, but it is a no-op in Q#.
            &Type::Float(..) => expr,
            &Type::Bool(..) => {
                let span = expr.span;
                let expr = build_math_call_from_exprs("Truncate", vec![expr], span);
                let const_int_zero_expr = build_lit_int_expr(0, span);
                let qsop = qsast::BinOp::Eq;
                let cond = build_binary_expr(false, qsop, expr, const_int_zero_expr, span);
                build_if_expr_then_expr_else_expr(
                    cond,
                    build_lit_bool_expr(false, span),
                    build_lit_bool_expr(true, span),
                    span,
                )
            }
            &Type::Bit(..) => {
                build_convert_cast_call_by_name("DoubleAsResult", expr, name_span, operand_span)
            }
            _ => err_expr(span),
        }
    }

    /// +----------------+-------------------------------------------------------------+
    /// | Allowed casts  | Casting To                                                  |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    /// | Casting From   | bool  | int | uint | float | angle | bit | duration | qubit |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    /// | int            | Yes   | -   | Yes  | Yes   | No    | Yes | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    /// | uint           | Yes   | Yes | -    | Yes   | No    | Yes | No       | No    |
    /// +----------------+-------+-----+------+-------+-------+-----+----------+-------+
    ///
    /// Additional cast to ``BigInt``
    /// With the exception of casting to ``BigInt``, there is no checking for overflow,
    /// widths, truncation, etc. Qiskit doesn't do these kinds of casts. For general
    /// `OpenQASM` support this will need to be fleshed out.
    #[allow(clippy::too_many_lines)]
    fn cast_int_expr_to_ty(
        expr: qsast::Expr,
        expr_ty: &crate::semantic::types::Type,
        ty: &crate::semantic::types::Type,
        span: Span,
    ) -> qsast::Expr {
        assert!(matches!(expr_ty, Type::Int(..) | Type::UInt(..)));
        let name_span = expr.span;
        let operand_span = span;
        match ty {
            Type::BitArray(size, _) => {
                let size = i64::from(*size);

                let size_expr = build_lit_int_expr(size, Span::default());
                build_qasmstd_convert_call_with_two_params(
                    "IntAsResultArrayBE",
                    expr,
                    size_expr,
                    name_span,
                    operand_span,
                )
            }
            Type::Float(..) => build_convert_call_expr(expr, "IntAsDouble"),
            Type::Int(tw, _) | Type::UInt(tw, _) => {
                // uint to int, or int/uint to BigInt
                if let Some(tw) = tw {
                    if *tw > 64 {
                        build_convert_call_expr(expr, "IntAsBigInt")
                    } else {
                        expr
                    }
                } else {
                    expr
                }
            }
            Type::Bool(..) => {
                let expr_span = expr.span;
                let const_int_zero_expr = build_lit_int_expr(0, expr.span);
                let qsop = qsast::BinOp::Eq;
                let cond = build_binary_expr(false, qsop, expr, const_int_zero_expr, expr_span);
                build_if_expr_then_expr_else_expr(
                    cond,
                    build_lit_bool_expr(false, expr_span),
                    build_lit_bool_expr(true, expr_span),
                    expr_span,
                )
            }
            Type::Bit(..) => {
                let operand_span = expr.span;
                build_qasm_convert_call_with_one_param("IntAsResult", expr, span, operand_span)
            }
            Type::Complex(..) => {
                let expr = build_convert_call_expr(expr, "IntAsDouble");
                build_complex_from_expr(expr)
            }
            _ => err_expr(span),
        }
    }

    fn map_bin_op(op: semast::BinOp) -> qsast::BinOp {
        match op {
            semast::BinOp::Add => qsast::BinOp::Add,
            semast::BinOp::AndB => qsast::BinOp::AndB,
            semast::BinOp::AndL => qsast::BinOp::AndL,
            semast::BinOp::Div => qsast::BinOp::Div,
            semast::BinOp::Eq => qsast::BinOp::Eq,
            semast::BinOp::Exp => qsast::BinOp::Exp,
            semast::BinOp::Gt => qsast::BinOp::Gt,
            semast::BinOp::Gte => qsast::BinOp::Gte,
            semast::BinOp::Lt => qsast::BinOp::Lt,
            semast::BinOp::Lte => qsast::BinOp::Lte,
            semast::BinOp::Mod => qsast::BinOp::Mod,
            semast::BinOp::Mul => qsast::BinOp::Mul,
            semast::BinOp::Neq => qsast::BinOp::Neq,
            semast::BinOp::OrB => qsast::BinOp::OrB,
            semast::BinOp::OrL => qsast::BinOp::OrL,
            semast::BinOp::Shl => qsast::BinOp::Shl,
            semast::BinOp::Shr => qsast::BinOp::Shr,
            semast::BinOp::Sub => qsast::BinOp::Sub,
            semast::BinOp::XorB => qsast::BinOp::XorB,
        }
    }

    fn is_simulatable_intrinsic(annotation: &semast::Annotation) -> bool {
        matches!(
            annotation.identifier.as_string().as_str(),
            QDK_QIR_INTRINSIC_ANNOTATION | QSHARP_QIR_INTRINSIC_ANNOTATION
        )
    }

    fn map_semantic_type_to_qsharp_type(
        &mut self,
        ty: &crate::semantic::types::Type,
        span: Span,
    ) -> crate::types::Type {
        use crate::semantic::types::Type;
        if ty.is_array()
            && matches!(
                ty.array_dims(),
                Some(crate::semantic::types::ArrayDimensions::Err)
            )
        {
            self.push_unsupported_error_message("arrays with more than 7 dimensions", span);
            return crate::types::Type::Err;
        }

        let is_const = ty.is_const();
        match ty {
            Type::Bit(_) => crate::types::Type::Result(is_const),
            Type::Qubit => crate::types::Type::Qubit,
            Type::HardwareQubit => {
                self.push_unsupported_error_message("hardware qubits", span);
                crate::types::Type::Err
            }
            Type::QubitArray(_) => {
                crate::types::Type::QubitArray(crate::types::ArrayDimensions::One)
            }
            Type::Int(width, _) | Type::UInt(width, _) => {
                if let Some(width) = width {
                    if *width > 64 {
                        crate::types::Type::BigInt(is_const)
                    } else {
                        crate::types::Type::Int(is_const)
                    }
                } else {
                    crate::types::Type::Int(is_const)
                }
            }
            Type::Float(_, _) => crate::types::Type::Double(is_const),
            Type::Angle(_, _) => crate::types::Type::Angle(is_const),
            Type::Complex(_, _) => crate::types::Type::Complex(is_const),
            Type::Bool(_) => crate::types::Type::Bool(is_const),
            Type::Duration(_) => {
                self.push_unsupported_error_message("duration type values", span);
                crate::types::Type::Err
            }
            Type::Stretch(_) => {
                self.push_unsupported_error_message("stretch type values", span);
                crate::types::Type::Err
            }
            Type::BitArray(_, _) => {
                crate::types::Type::ResultArray(crate::types::ArrayDimensions::One, is_const)
            }
            Type::Array(array)
                if !matches!(
                    array.base_ty,
                    crate::semantic::types::ArrayBaseType::Duration
                ) =>
            {
                let dims = (&array.dims).into();
                Self::make_qsharp_array_ty(&array.base_ty, dims)
            }
            Type::StaticArrayRef(array_ref) if !array_ref.is_mutable => {
                let dims = (&array_ref.dims).into();
                Self::make_qsharp_array_ty(&array_ref.base_ty, dims)
            }
            Type::DynArrayRef(array_ref) if !array_ref.is_mutable => {
                let dims = (array_ref.dims).into();
                Self::make_qsharp_array_ty(&array_ref.base_ty, dims)
            }
            Type::StaticArrayRef(array_ref) if array_ref.is_mutable => {
                let msg = format!("mutable array references `{ty}`");
                self.push_unsupported_error_message(msg, span);
                crate::types::Type::Err
            }
            Type::DynArrayRef(array_ref) if array_ref.is_mutable => {
                let msg = format!("mutable array references `{ty}`");
                self.push_unsupported_error_message(msg, span);
                crate::types::Type::Err
            }
            Type::Gate(cargs, qargs) => crate::types::Type::Gate(*cargs, *qargs),
            Type::Range => crate::types::Type::Range,
            Type::Void => crate::types::Type::Tuple(vec![]),
            Type::Function(args, return_ty) => {
                // This is a raw conversion of the semantic type to a Q# type.
                // Any extra promotion to Operation based on attributes/pragmas
                // will be done later in the compiler.

                let kind = if args.iter().any(|arg| {
                    matches!(arg, Type::Qubit | Type::HardwareQubit | Type::QubitArray(_))
                }) {
                    crate::types::CallableKind::Operation
                } else {
                    crate::types::CallableKind::Function
                };
                let args = args
                    .iter()
                    .map(|arg| self.map_semantic_type_to_qsharp_type(arg, Span::default()))
                    .collect::<Vec<_>>();
                let return_ty = self.map_semantic_type_to_qsharp_type(return_ty, Span::default());
                crate::types::Type::Callable(kind, args.into(), return_ty.into())
            }
            Type::Err => crate::types::Type::Err,
            _ => {
                let msg = format!("converting `{ty}` to Q# type");
                self.push_unimplemented_error_message(msg, span);
                crate::types::Type::Err
            }
        }
    }

    fn make_qsharp_array_ty(
        base_ty: &crate::semantic::types::ArrayBaseType,
        dims: crate::types::ArrayDimensions,
    ) -> crate::types::Type {
        match base_ty {
            crate::semantic::types::ArrayBaseType::Duration => unreachable!(),
            crate::semantic::types::ArrayBaseType::Bool => {
                crate::types::Type::BoolArray(dims, false)
            }
            crate::semantic::types::ArrayBaseType::Angle(_) => {
                crate::types::Type::AngleArray(dims, false)
            }
            crate::semantic::types::ArrayBaseType::Complex(_) => {
                crate::types::Type::ComplexArray(dims, false)
            }
            crate::semantic::types::ArrayBaseType::Float(_) => {
                crate::types::Type::DoubleArray(dims)
            }
            crate::semantic::types::ArrayBaseType::Int(width)
            | crate::semantic::types::ArrayBaseType::UInt(width) => {
                if let Some(width) = width {
                    if *width > 64 {
                        crate::types::Type::BigIntArray(dims, false)
                    } else {
                        crate::types::Type::IntArray(dims, false)
                    }
                } else {
                    crate::types::Type::IntArray(dims, false)
                }
            }
        }
    }

    fn get_argument_validation_stmts(
        args: &[(&String, qsast::Ty, Span, &Type)],
    ) -> Vec<qsast::Stmt> {
        args.iter()
            .filter_map(|(name, _, span, ty)| {
                if ty.is_array() && !matches!(ty, Type::DynArrayRef(..)) {
                    Some(build_argument_validation_stmts(name, ty, *span))
                } else {
                    None
                }
            })
            .flatten()
            .collect::<Vec<_>>()
    }

    fn prepend_argument_validation_to_block(
        mut body: qsast::Block,
        args: &[(String, qsast::Ty, qsast::Pat, Type)],
    ) -> Option<qsast::Block> {
        let args = args
            .iter()
            .map(|(name, ast_ty, pat, sym_type)| (name, ast_ty.clone(), pat.span, sym_type))
            .collect::<Vec<_>>();
        let stmts = Self::get_argument_validation_stmts(&args);
        let stmts = list_from_iter(stmts);
        body.stmts = stmts.into_iter().chain(body.stmts).collect();

        Some(body)
    }
}
