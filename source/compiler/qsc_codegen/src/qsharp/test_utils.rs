// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use std::sync::Arc;

use expect_test::Expect;
use qsc_ast::{ast::Package, mut_visit::MutVisitor};
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, span::Span,
    target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_hir::hir::PackageId;
use qsc_passes::{PackageType, run_core_passes, run_default_passes};

use crate::qsharp::write_package_string;

pub(crate) fn check(program: &str, expr: Option<&str>, expect: &Expect) {
    let (qsharp, src_ast_str) = compile_program(expr, program);
    expect.assert_eq(&qsharp);
    // Run the output against the compiler to ensure that input
    // and output both generate the same qsharp.
    let (round_trip_qsharp, gen_ast_str) = compile_program(expr, &qsharp);
    expect.assert_eq(&round_trip_qsharp);
    // we've validated the output, now validate the ASTs
    // We may have generated the same Q#, but may have changed semantics
    difference::assert_diff!(&src_ast_str, &gen_ast_str, "\n", 0);
}

pub(crate) fn get_compilation(sources: Option<SourceMap>) -> (PackageId, PackageStore) {
    let mut core = compile::core();
    assert!(run_core_passes(&mut core).is_empty());
    let mut store = PackageStore::new(core);
    let mut std = compile::std(&store, TargetCapabilityFlags::empty());
    assert!(run_default_passes(store.core(), &mut std, PackageType::Lib).is_empty());
    let std = store.insert(std);

    let mut unit = compile(
        &store,
        &[(std, None)],
        sources.unwrap_or_default(),
        TargetCapabilityFlags::all(),
        LanguageFeatures::empty(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    assert!(run_default_passes(store.core(), &mut unit, PackageType::Lib,).is_empty());
    let package_id = store.insert(unit);
    (package_id, store)
}

pub(crate) fn compile_program(expr: Option<&str>, program: &str) -> (String, String) {
    let expr_as_arc: Option<Arc<str>> = expr.map(|s| Arc::from(s.to_string()));
    let sources = SourceMap::new([("test".into(), program.into())], expr_as_arc);

    let (package_id, store) = get_compilation(Some(sources));
    let package = &store.get(package_id).expect("package must exist");

    let despanned_ast = AstDespanner.despan(&package.ast.package);
    let qsharp = write_package_string(&despanned_ast);
    let ast = format!("{despanned_ast}");
    (qsharp, ast)
}

struct AstDespanner;
impl AstDespanner {
    fn despan(&mut self, package: &Package) -> Package {
        let mut p = package.clone();
        self.visit_package(&mut p);
        p
    }
}

impl qsc_ast::mut_visit::MutVisitor for AstDespanner {
    fn visit_span(&mut self, span: &mut Span) {
        span.hi = 0;
        span.lo = 0;
    }
}
