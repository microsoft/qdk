// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::compiler::{PragmaConfig, parse_and_compile_to_qsharp_ast_with_config};
use crate::{
    CompilerConfig, OutputSemantics, ProgramType, QasmCompileUnit, QubitSemantics,
    get_semantic_errors_from_lowering_result,
};
use expect_test::Expect;
use miette::Report;
use qsc::compile::compile_ast;
use qsc::compile::package_store_with_stdlib;
use qsc::interpret::Error;
use qsc::target::Profile;
use qsc::{
    SourceMap, Span,
    ast::{Package, Stmt, TopLevelNode, mut_visit::MutVisitor},
};
use qsc_hir::hir::PackageId;
use qsc_openqasm_parser::io::{InMemorySourceResolver, SourceResolver};
use qsc_openqasm_parser::semantic::{QasmSemanticParseResult, parse_source};
use qsc_passes::PackageType;
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub(crate) mod assignment;
pub(crate) mod compiler_errors;
pub(crate) mod declaration;
pub(crate) mod expression;
pub(crate) mod fuzz;
pub(crate) mod output;
pub(crate) mod sample_circuits;
pub(crate) mod scopes;
pub(crate) mod statement;
pub(crate) mod types;

pub(crate) fn fail_on_compilation_errors(unit: &QasmCompileUnit) {
    if unit.has_errors() {
        print_compilation_errors(unit);
        panic!("Errors found in compilation");
    }
}

pub(crate) fn print_compilation_errors(unit: &QasmCompileUnit) {
    if unit.has_errors() {
        for e in unit.errors() {
            println!("{:?}", Report::new(e.clone()));
        }
    }
}

pub(crate) fn gen_qsharp(package: &Package) -> String {
    qsc::codegen::qsharp::write_package_string(package)
}

/// Generates QIR from an AST package.
/// This function is used for testing purposes only.
/// The interactive environment uses a different mechanism to generate QIR.
/// As we need an entry expression to generate QIR in those cases.
///
/// This function assumes that the AST package was designed as an entry point.
pub(crate) fn generate_qir_from_ast(
    ast_package: Package,
    source_map: SourceMap,
    profile: Profile,
) -> Result<String, Vec<Error>> {
    let capabilities = profile.into();
    let (stdid, mut store) = package_store_with_stdlib(capabilities);
    let dependencies = vec![(PackageId::CORE, None), (stdid, None)];
    qsc::codegen::qir::get_qir_from_ast(
        &mut store,
        &dependencies,
        ast_package,
        source_map,
        capabilities,
    )
}

fn compile<S: Into<Arc<str>>>(source: S) -> miette::Result<QasmCompileUnit, Vec<Report>> {
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    compile_with_config(source, config)
}

fn compile_with_config<S: Into<Arc<str>>>(
    source: S,
    config: CompilerConfig,
) -> miette::Result<QasmCompileUnit, Vec<Report>> {
    let res = parse(source)?;
    if res.has_syntax_errors() {
        for e in res.sytax_errors() {
            println!("{:?}", Report::new(e.clone()));
        }
    }
    assert!(!res.has_syntax_errors());
    let errors = get_semantic_errors_from_lowering_result(&res);
    let program = res.program;

    let compiler = crate::compiler::QasmCompiler {
        source_map: res.source_map,
        config,
        stmts: vec![],
        symbols: res.symbols,
        qubits: vec![],
        errors,
        pragma_config: PragmaConfig::default(),
        functor_constraints: FxHashMap::default(),
    };

    let unit = compiler.compile(&program);
    Ok(unit)
}

#[allow(dead_code)]
pub fn compile_all<P: Into<Arc<str>>>(
    path: P,
    sources: impl IntoIterator<Item = (Arc<str>, Arc<str>)>,
) -> miette::Result<QasmCompileUnit, Vec<Report>> {
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::Qiskit,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    compile_all_with_config(path, sources, config)
}

#[allow(dead_code)]
pub fn compile_all_fragments<P: Into<Arc<str>>>(
    path: P,
    sources: impl IntoIterator<Item = (Arc<str>, Arc<str>)>,
) -> miette::Result<QasmCompileUnit, Vec<Report>> {
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::Fragments,
        None,
        None,
    );
    compile_all_with_config(path, sources, config)
}

fn compile_fragments<S: Into<Arc<str>>>(source: S) -> miette::Result<QasmCompileUnit, Vec<Report>> {
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::Fragments,
        None,
        None,
    );
    compile_with_config(source, config)
}

pub fn compile_all_with_config<P: Into<Arc<str>>>(
    path: P,
    sources: impl IntoIterator<Item = (Arc<str>, Arc<str>)>,
    config: CompilerConfig,
) -> miette::Result<QasmCompileUnit, Vec<Report>> {
    let res = parse_all(path, sources)?;
    assert!(!res.has_syntax_errors());
    let errors = get_semantic_errors_from_lowering_result(&res);
    let program = res.program;

    let compiler = crate::compiler::QasmCompiler {
        source_map: res.source_map,
        config,
        stmts: vec![],
        symbols: res.symbols,
        qubits: vec![],
        errors,
        pragma_config: PragmaConfig::default(),
        functor_constraints: FxHashMap::default(),
    };

    let unit = compiler.compile(&program);
    Ok(unit)
}

fn compile_qasm_to_qir(source: &str) -> Result<String, Vec<Report>> {
    let unit = compile(source)?;
    fail_on_compilation_errors(&unit);
    let package = unit.package;
    let qir = generate_qir_from_ast(package, unit.source_map, unit.profile).map_err(|errors| {
        errors
            .iter()
            .map(|e| Report::new(e.clone()))
            .collect::<Vec<_>>()
    })?;
    Ok(qir)
}

/// used to do full compilation with best effort of the input.
/// This is useful for fuzz testing.
fn compile_qasm_best_effort(source: &str) {
    let mut resolver = InMemorySourceResolver::from_iter([]);
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::File,
        Some("Fuzz".into()),
        None,
    );

    let unit = parse_and_compile_to_qsharp_ast_with_config(
        source,
        "source.qasm",
        Some(&mut resolver),
        config,
    );
    let (sources, _, package, _, profile) = unit.into_tuple();

    let (stdid, store) = package_store_with_stdlib(profile.into());
    let dependencies = vec![(PackageId::CORE, None), (stdid, None)];

    let (mut _unit, _errors) = compile_ast(
        &store,
        &dependencies,
        package,
        sources,
        PackageType::Lib,
        profile.into(),
    );
}

pub(crate) fn gen_qsharp_stmt(stmt: &Stmt) -> String {
    qsc::codegen::qsharp::write_stmt_string(stmt)
}

#[allow(dead_code)]
pub(crate) fn compare_compilation_to_qsharp(unit: &QasmCompileUnit, expected: &str) {
    let package = &unit.package;
    let despanned_ast = AstDespanner.despan(package);
    let qsharp = gen_qsharp(&despanned_ast);
    difference::assert_diff!(&qsharp, expected, "\n", 0);
}

pub(crate) fn parse<S: Into<Arc<str>>>(
    source: S,
) -> miette::Result<QasmSemanticParseResult, Vec<Report>> {
    let source = source.into();
    let name: Arc<str> = "Test.qasm".into();
    let sources = [(name.clone(), source.clone())];
    let mut resolver = InMemorySourceResolver::from_iter(sources);
    let res = parse_source(source, name, &mut resolver);
    if res.source.has_errors() {
        let errors = res
            .errors()
            .into_iter()
            .map(|e| Report::new(e.clone()))
            .collect();
        return Err(errors);
    }
    Ok(res)
}

pub(crate) fn parse_all<P: Into<Arc<str>>>(
    path: P,
    sources: impl IntoIterator<Item = (Arc<str>, Arc<str>)>,
) -> miette::Result<QasmSemanticParseResult, Vec<Report>> {
    let path = path.into();
    let mut resolver = InMemorySourceResolver::from_iter(sources);
    let source = resolver
        .resolve(&path, &path)
        .map_err(|e| vec![Report::new(e)])?
        .1;
    let res = parse_source(source, path, &mut resolver);
    if res.source.has_errors() {
        let errors = res
            .errors()
            .into_iter()
            .map(|e| Report::new(e.clone()))
            .collect();
        Err(errors)
    } else {
        Ok(res)
    }
}

pub fn compile_qasm_to_qsharp_file<S: Into<Arc<str>>>(
    source: S,
) -> miette::Result<String, Vec<Report>> {
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::File,
        Some("Test".into()),
        None,
    );
    let unit = compile_with_config(source, config)?;
    if unit.has_errors() {
        let errors = unit.errors.into_iter().map(Report::new).collect();
        return Err(errors);
    }
    let qsharp = gen_qsharp(&unit.package);
    Ok(qsharp)
}

pub fn compile_qasm_to_qsharp_operation<S: Into<Arc<str>>>(
    source: S,
) -> miette::Result<String, Vec<Report>> {
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::Operation,
        Some("Test".into()),
        None,
    );
    let unit = compile_with_config(source, config)?;
    if unit.has_errors() {
        let errors = unit.errors.into_iter().map(Report::new).collect();
        return Err(errors);
    }
    let qsharp = gen_qsharp(&unit.package);
    Ok(qsharp)
}

pub fn compile_qasm_to_qsharp<S: Into<Arc<str>>>(source: S) -> miette::Result<String, Vec<Report>> {
    compile_qasm_to_qsharp_with_semantics(source, QubitSemantics::Qiskit)
}

pub fn check_qasm_to_qsharp<S: Into<Arc<str>>>(source: S, expect: &Expect) {
    match compile_qasm_to_qsharp(source) {
        Ok(qsharp) => {
            expect.assert_eq(&qsharp);
        }
        Err(errors) => {
            let buffer = errors
                .iter()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>()
                .join("\n");
            expect.assert_eq(&buffer);
        }
    }
}

pub fn check_qasm_to_qir(source: &str, expect: &Expect) {
    match compile_qasm_to_qir(source) {
        Ok(qsharp) => {
            expect.assert_eq(&qsharp);
        }
        Err(errors) => {
            let buffer = errors
                .iter()
                .map(|e| format!("{e:?}"))
                .collect::<Vec<_>>()
                .join("\n");
            expect.assert_eq(&buffer);
        }
    }
}

pub fn compile_qasm_to_qsharp_with_semantics<S: Into<Arc<str>>>(
    source: S,
    qubit_semantics: QubitSemantics,
) -> miette::Result<String, Vec<Report>> {
    let config = CompilerConfig::new(
        qubit_semantics,
        OutputSemantics::Qiskit,
        ProgramType::Fragments,
        None,
        None,
    );
    let unit = compile_with_config(source, config)?;
    qsharp_from_qasm_compilation(unit)
}

pub fn qsharp_from_qasm_compilation(unit: QasmCompileUnit) -> miette::Result<String, Vec<Report>> {
    if unit.has_errors() {
        let errors = unit.errors.into_iter().map(Report::new).collect();
        return Err(errors);
    }
    let qsharp = gen_qsharp(&unit.package);
    Ok(qsharp)
}

pub fn compile_qasm_stmt_to_qsharp<S: Into<Arc<str>>>(
    source: S,
) -> miette::Result<String, Vec<Report>> {
    compile_qasm_stmt_to_qsharp_with_semantics(source, QubitSemantics::Qiskit)
}

pub fn compile_qasm_stmt_to_qsharp_with_semantics<S: Into<Arc<str>>>(
    source: S,
    qubit_semantics: QubitSemantics,
) -> miette::Result<String, Vec<Report>> {
    let config = CompilerConfig::new(
        qubit_semantics,
        OutputSemantics::Qiskit,
        ProgramType::Fragments,
        None,
        None,
    );
    let unit = compile_with_config(source, config)?;
    if unit.has_errors() {
        let errors = unit.errors.into_iter().map(Report::new).collect();
        return Err(errors);
    }
    let qsharp = get_last_statement_as_qsharp(&unit.package);
    Ok(qsharp)
}

fn get_last_statement_as_qsharp(package: &Package) -> String {
    match package.nodes.iter().last() {
        Some(i) => match i {
            TopLevelNode::Namespace(_) => panic!("Expected Stmt, got Namespace"),
            TopLevelNode::Stmt(stmt) => gen_qsharp_stmt(stmt.as_ref()),
        },
        None => panic!("Expected Stmt, got None"),
    }
}

pub struct AstDespanner;
impl AstDespanner {
    #[allow(dead_code)] // false positive lint
    pub fn despan(&mut self, package: &Package) -> Package {
        let mut p = package.clone();
        self.visit_package(&mut p);
        p
    }
}

impl MutVisitor for AstDespanner {
    fn visit_span(&mut self, span: &mut Span) {
        span.hi = 0;
        span.lo = 0;
    }
}

#[allow(dead_code)]
struct HirDespanner;
impl HirDespanner {
    #[allow(dead_code)]
    fn despan(&mut self, package: &qsc::hir::Package) -> qsc::hir::Package {
        let mut p = package.clone();
        qsc::hir::mut_visit::MutVisitor::visit_package(self, &mut p);
        p
    }
}

impl qsc::hir::mut_visit::MutVisitor for HirDespanner {
    fn visit_span(&mut self, span: &mut Span) {
        span.hi = 0;
        span.lo = 0;
    }
}

mod qsharp {
    use qsc_ast::ast::Package;
    use qsc_data_structures::{language_features::LanguageFeatures, source::SourceMap};
    use qsc_frontend::compile::parse_all;

    pub(super) fn parse_package(sources: Option<SourceMap>) -> Package {
        let (ast_package, _) = parse_all(&sources.unwrap_or_default(), LanguageFeatures::empty());
        ast_package
    }

    #[must_use]
    pub(super) fn parse_program(program: &str) -> Package {
        let sources = SourceMap::new([("test".into(), program.into())], None);
        parse_package(Some(sources))
    }
}

#[allow(dead_code)]
pub(crate) fn compare_qasm_and_qasharp_asts(source: &str) {
    // 1. Generate a despaned QASM package.
    let config = crate::CompilerConfig::new(
        crate::QubitSemantics::Qiskit,
        crate::OutputSemantics::Qiskit,
        crate::ProgramType::File,
        None,
        None,
    );
    let mut resolver = qsc_openqasm_parser::io::InMemorySourceResolver::from_iter([]);
    let unit = parse_and_compile_to_qsharp_ast_with_config(
        source,
        "source.qasm",
        Some(&mut resolver),
        config,
    );
    fail_on_compilation_errors(&unit);
    let despanned_qasm_ast = AstDespanner.despan(&unit.package);

    // 2. Generate Q# source from the QASM ast.
    let qsharp_src = gen_qsharp(&despanned_qasm_ast);

    // 3. Generate a despaned Q# package using the Q# compiler.
    let qsharp_package = qsharp::parse_program(&qsharp_src);
    let despanned_qsharp_ast = AstDespanner.despan(&qsharp_package);

    // 4. Compare diffs between the ASTs generated by QASM and by Q#.
    let despanned_qasm_ast = despanned_qasm_ast.to_string();
    let despanned_qsharp_ast = despanned_qsharp_ast.to_string();

    difference::assert_diff!(&despanned_qasm_ast, &despanned_qsharp_ast, "\n", 0);
}
