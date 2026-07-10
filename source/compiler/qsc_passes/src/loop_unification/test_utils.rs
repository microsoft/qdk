// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::Expect;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_hir::{mut_visit::MutVisitor, validate::Validator, visit::Visitor};

use crate::loop_unification::{Error, LoopUni};

pub(super) fn desugar(file: &str) -> (compile::CompileUnit, PackageStore, Vec<Error>) {
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let mut loop_uni = LoopUni {
        core: store.core(),
        assigner: &mut unit.assigner,
        errors: Vec::new(),
    };
    loop_uni.visit_package(&mut unit.package);
    let errors = loop_uni.errors;
    Validator::default().visit_package(&unit.package);
    (unit, store, errors)
}

/// Runs the operand-lifting normalize pass before the desugar, matching the
/// order of the real pass pipeline, so operand-position `break`/`continue` is
/// hoisted to statement position and then desugared.
pub(super) fn desugar_normalized(file: &str) -> (compile::CompileUnit, PackageStore, Vec<Error>) {
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    crate::loop_normalize::LoopNormalize::new(&mut unit.assigner).visit_package(&mut unit.package);
    let mut loop_uni = LoopUni {
        core: store.core(),
        assigner: &mut unit.assigner,
        errors: Vec::new(),
    };
    loop_uni.visit_package(&mut unit.package);
    let errors = loop_uni.errors;
    Validator::default().visit_package(&unit.package);
    (unit, store, errors)
}

pub(super) fn check(file: &str, expect: &Expect) {
    let (unit, store, errors) = desugar(file);
    assert!(errors.is_empty(), "unexpected desugar errors: {errors:?}");
    expect.assert_eq(&crate::qsharp_gen::write_package_qsharp(
        &store,
        &unit.package,
    ));
}

pub(super) fn check_normalized(file: &str, expect: &Expect) {
    let (unit, store, errors) = desugar_normalized(file);
    assert!(errors.is_empty(), "unexpected desugar errors: {errors:?}");
    expect.assert_eq(&crate::qsharp_gen::write_package_qsharp(
        &store,
        &unit.package,
    ));
}
