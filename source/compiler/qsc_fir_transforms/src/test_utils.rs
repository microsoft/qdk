// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Shared test helpers for the `qsc_fir_transforms` crate.
//!
//! Provides compilation and snapshot utilities used across transform test
//! modules. Gated behind `#[cfg(any(test, feature = "testutil"))]`.
//!
//! Items marked with `#[allow(dead_code)]` are intended for use in test modules and may be
//! are used in multiple test modules, but are not used in the main crate code.

use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_fir::fir::{
    self, CallableDecl, CallableImpl, ExprId, ExprKind, ItemKind, LocalVarId, Package,
    PackageLookup, PatId, PatKind, Res, SpecDecl, StmtId, StmtKind,
};
use qsc_frontend::compile::{self as frontend_compile, PackageStore as HirPackageStore};
use qsc_hir::hir::PackageId;
use qsc_passes::{PackageType, lower_hir_to_fir, run_core_passes, run_default_passes};
use rustc_hash::FxHashMap;
use std::cell::RefCell;

use qsc_lowerer::map_hir_package_to_fir;

pub(crate) use crate::PipelineStage;

fn format_errors<T: ToString>(errors: &[T]) -> String {
    errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn assert_no_compile_errors(context: &str, errors: &[frontend_compile::Error]) {
    let error_messages = errors
        .iter()
        .map(|error| format!("{error:?}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        errors.is_empty(),
        "{context} has Q# compilation errors:\n{error_messages}"
    );
}

pub fn assert_no_pipeline_errors(context: &str, errors: &[crate::PipelineError]) {
    let error_messages = format_errors(errors);
    assert!(
        errors.is_empty(),
        "{context} produced FIR transform pipeline errors:\n{error_messages}"
    );
}

/// Asserts that a pipeline result did not produce non-fatal warnings.
pub fn assert_no_pipeline_warnings(context: &str, warnings: &[crate::PipelineError]) {
    let warning_messages = format_errors(warnings);
    assert!(
        warnings.is_empty(),
        "{context} produced FIR transform pipeline warnings:\n{warning_messages}"
    );
}

#[must_use]
pub fn format_pipeline_errors(errors: &[crate::PipelineError]) -> String {
    if errors.is_empty() {
        "(no error)".to_string()
    } else {
        format_errors(errors)
    }
}

/// Metadata prepended to transform snapshot views so failures carry their
/// scenario, pipeline stage, rendered surface, and protected claim.
#[derive(Clone, Copy, Debug)]
pub struct TransformSnapshotCase {
    pub case: &'static str,
    pub stage: PipelineStage,
    pub view: &'static str,
    pub claim: &'static str,
}

#[must_use]
pub fn format_transform_snapshot(
    case: &TransformSnapshotCase,
    actual_view: impl std::fmt::Display,
) -> String {
    format!(
        "case={}\nstage={:?}\nview={}\nclaim={}\n\n{}",
        case.case, case.stage, case.view, case.claim, actual_view
    )
}

/// Asserts that a warning-aware pipeline result has no fatal errors.
pub fn assert_pipeline_succeeded(context: &str, result: &crate::PipelineResult) {
    assert_no_pipeline_errors(context, &result.errors);
}

pub fn assert_pipeline_stage_succeeds(
    context: &str,
    store: &mut fir::PackageStore,
    pkg_id: fir::PackageId,
    stage: PipelineStage,
) -> crate::PipelineResult {
    let result = crate::run_pipeline_to_with_diagnostics(store, pkg_id, stage, &[]);
    assert_no_pipeline_errors(context, &result.errors);
    result
}

pub fn assert_full_pipeline_succeeds(
    context: &str,
    store: &mut fir::PackageStore,
    pkg_id: fir::PackageId,
) -> crate::PipelineResult {
    let result = crate::run_pipeline_with_diagnostics(store, pkg_id);
    assert_no_pipeline_errors(context, &result.errors);
    result
}

thread_local! {
    static STDLIB_PACKAGE_STORES: RefCell<FxHashMap<TargetCapabilityFlags, HirPackageStore>> =
        RefCell::default();
}

/// Sets up an HIR package store containing core + std libraries with default
/// passes applied, using the given target capabilities.
#[must_use]
pub fn package_store_with_stdlib(capabilities: TargetCapabilityFlags) -> HirPackageStore {
    build_package_store_with_stdlib(capabilities)
}

fn build_package_store_with_stdlib(capabilities: TargetCapabilityFlags) -> HirPackageStore {
    let mut core_unit = frontend_compile::core();
    assert_no_compile_errors("core library", &core_unit.errors);
    let core_errors = run_core_passes(&mut core_unit);
    assert!(
        core_errors.is_empty(),
        "core library has compilation errors"
    );
    let mut store = HirPackageStore::new(core_unit);

    let mut std_unit = frontend_compile::std(&store, capabilities);
    assert_no_compile_errors("std library", &std_unit.errors);
    let std_errors = run_default_passes(store.core(), &mut std_unit, PackageType::Lib);
    assert!(std_errors.is_empty(), "std library has compilation errors");
    store.insert(std_unit);

    store
}

fn with_cached_stdlib_store<T>(
    capabilities: TargetCapabilityFlags,
    f: impl FnOnce(&HirPackageStore, PackageId) -> T,
) -> T {
    STDLIB_PACKAGE_STORES.with(|stores| {
        let missing = !stores.borrow().contains_key(&capabilities);
        if missing {
            let store = build_package_store_with_stdlib(capabilities);
            stores.borrow_mut().insert(capabilities, store);
        }

        let stores = stores.borrow();
        let store = stores
            .get(&capabilities)
            .expect("cached stdlib store should exist");
        f(store, PackageId::CORE.successor())
    })
}

fn lower_cached_stdlib_and_user_to_fir(
    store: &HirPackageStore,
    std_id: PackageId,
    user_unit: &frontend_compile::CompileUnit,
) -> (fir::PackageStore, fir::PackageId) {
    let user_hir_id = user_unit.package_id();
    let core_unit = store
        .get(PackageId::CORE)
        .expect("cached core package should exist");
    let std_unit = store.get(std_id).expect("cached std package should exist");

    let mut fir_store = fir::PackageStore::new();
    for (hir_id, unit) in [(PackageId::CORE, core_unit), (std_id, std_unit)] {
        let mut lowerer = qsc_lowerer::Lowerer::new();
        let package = lowerer.lower_package(&unit.package, &fir_store);
        fir_store.insert(map_hir_package_to_fir(hir_id), package);
    }

    let mut lowerer = qsc_lowerer::Lowerer::new();
    let user_package = lowerer.lower_package(&user_unit.package, &fir_store);
    let fir_pkg_id = map_hir_package_to_fir(user_hir_id);
    fir_store.insert(fir_pkg_id, user_package);

    (fir_store, fir_pkg_id)
}

fn compile_to_fir_with_cached_stdlib(
    source: &str,
    entry: Option<&str>,
    capabilities: TargetCapabilityFlags,
) -> (fir::PackageStore, fir::PackageId) {
    with_cached_stdlib_store(capabilities, |store, std_id| {
        let sources = SourceMap::new(
            vec![("test.qs".into(), source.into())],
            entry.map(Into::into),
        );
        let mut unit = frontend_compile::compile(
            store,
            &[(PackageId::CORE, None), (std_id, None)],
            sources,
            capabilities,
            LanguageFeatures::default(),
        );
        assert_no_compile_errors("user code", &unit.errors);
        let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Exe);
        assert!(pass_errors.is_empty(), "user code has compilation errors");
        lower_cached_stdlib_and_user_to_fir(store, std_id, &unit)
    })
}

/// Convenience wrapper around [`package_store_with_stdlib`] that passes
/// [`TargetCapabilityFlags::empty()`].
#[must_use]
pub fn package_store_with_stdlib_default() -> HirPackageStore {
    package_store_with_stdlib(TargetCapabilityFlags::empty())
}

/// Compiles Q# source through core+std → HIR passes → FIR lowering.
///
/// Returns a FIR store with no transforms applied. Uses default (empty)
/// target capabilities.
#[must_use]
pub fn compile_to_fir(source: &str) -> (fir::PackageStore, fir::PackageId) {
    compile_to_fir_with_capabilities(source, TargetCapabilityFlags::empty())
}

/// Compiles Q# source through core+std → HIR passes → FIR lowering using the
/// given target capabilities.
///
/// Returns a FIR store with no transforms applied.
#[must_use]
pub fn compile_to_fir_with_capabilities(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> (fir::PackageStore, fir::PackageId) {
    compile_to_fir_with_cached_stdlib(source, None, capabilities)
}

/// Compiles a library Q# source and user Q# source through
/// core+std+lib → HIR passes → FIR lowering.
///
/// Returns a FIR store with 4 packages (core, std, lib, user) and the
/// user package ID. Uses default (empty) target capabilities.
#[must_use]
pub fn compile_to_fir_with_library(
    lib_source: &str,
    user_source: &str,
) -> (fir::PackageStore, fir::PackageId) {
    compile_to_fir_with_library_and_capabilities(
        lib_source,
        user_source,
        TargetCapabilityFlags::empty(),
    )
}

/// Compiles a library Q# source and user Q# source through
/// core+std+lib → HIR passes → FIR lowering using the given target
/// capabilities.
///
/// Returns a FIR store with 4 packages (core, std, lib, user) and the
/// user package ID.
#[must_use]
pub fn compile_to_fir_with_library_and_capabilities(
    lib_source: &str,
    user_source: &str,
    capabilities: TargetCapabilityFlags,
) -> (fir::PackageStore, fir::PackageId) {
    let mut store = package_store_with_stdlib(capabilities);
    let std_id = PackageId::CORE.successor();

    // Compile library package
    let lib_sources = SourceMap::new(vec![("lib.qs".into(), lib_source.into())], None);
    let mut lib_unit = frontend_compile::compile(
        &store,
        &[(PackageId::CORE, None), (std_id, None)],
        lib_sources,
        capabilities,
        LanguageFeatures::default(),
    );
    assert_no_compile_errors("library code", &lib_unit.errors);
    let lib_pass_errors = run_default_passes(store.core(), &mut lib_unit, PackageType::Lib);
    assert!(
        lib_pass_errors.is_empty(),
        "library code has compilation errors"
    );
    let lib_id = store.insert(lib_unit);

    // Compile user package depending on core + std + lib
    let user_sources = SourceMap::new(vec![("test.qs".into(), user_source.into())], None);
    let mut user_unit = frontend_compile::compile(
        &store,
        &[(PackageId::CORE, None), (std_id, None), (lib_id, None)],
        user_sources,
        capabilities,
        LanguageFeatures::default(),
    );
    assert_no_compile_errors("user code", &user_unit.errors);
    let user_pass_errors = run_default_passes(store.core(), &mut user_unit, PackageType::Exe);
    assert!(
        user_pass_errors.is_empty(),
        "user code has compilation errors"
    );
    let user_hir_id = store.insert(user_unit);

    let (fir_store, fir_pkg_id, _) = lower_hir_to_fir(&store, user_hir_id);
    (fir_store, fir_pkg_id)
}

/// Compiles Q# source through core+std → HIR passes → FIR lowering →
/// monomorphization.
///
/// Returns a monomorphized FIR store ready for defunctionalization or later
/// pipeline stages. Uses default (empty) target capabilities.
#[must_use]
pub fn compile_to_monomorphized_fir(source: &str) -> (fir::PackageStore, fir::PackageId) {
    compile_to_monomorphized_fir_with_capabilities(source, TargetCapabilityFlags::empty())
}

/// Compiles Q# source through core+std → HIR passes → FIR lowering →
/// monomorphization using the given target capabilities.
///
/// Returns a monomorphized FIR store ready for defunctionalization or later
/// pipeline stages.
#[must_use]
pub fn compile_to_monomorphized_fir_with_capabilities(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> (fir::PackageStore, fir::PackageId) {
    let (mut store, pkg_id) = compile_to_fir_with_capabilities(source, capabilities);
    let mut assigner = qsc_fir::assigner::Assigner::from_package(store.get(pkg_id));
    crate::monomorphize::monomorphize(&mut store, pkg_id, &mut assigner);
    (store, pkg_id)
}

/// Compiles Q# source through core+std → HIR passes → FIR lowering using an
/// explicit executable entry expression.
///
/// Returns a FIR store with no transforms applied.
#[must_use]
pub fn compile_to_fir_with_entry(source: &str, entry: &str) -> (fir::PackageStore, fir::PackageId) {
    compile_to_fir_with_cached_stdlib(source, Some(entry), TargetCapabilityFlags::empty())
}

/// Compiles Q# source and runs the FIR optimization pipeline up to the given
/// stage.
///
/// # Panics
///
/// Panics if compilation fails, or if the requested stage reaches
/// defunctionalization and the shared pipeline runner returns any errors.
#[allow(dead_code)]
pub(crate) fn compile_and_run_pipeline_to_with_errors(
    source: &str,
    stage: PipelineStage,
) -> (fir::PackageStore, fir::PackageId, crate::PipelineResult) {
    let (mut store, pkg_id) = compile_to_fir(source);
    let result = crate::run_pipeline_to_with_diagnostics(&mut store, pkg_id, stage, &[]);
    (store, pkg_id, result)
}

/// Compiles Q# source and runs the FIR optimization pipeline up to the given
/// stage, asserting that defunctionalization diagnostics stay empty once the
/// schedule reaches or passes that stage.
#[allow(dead_code)]
pub(crate) fn compile_and_run_pipeline_to(
    source: &str,
    stage: PipelineStage,
) -> (fir::PackageStore, fir::PackageId) {
    let (store, pkg_id, result) = compile_and_run_pipeline_to_with_errors(source, stage);
    if matches!(
        stage,
        PipelineStage::Defunc
            | PipelineStage::UdtErase
            | PipelineStage::TupleCompLower
            | PipelineStage::Sroa
            | PipelineStage::ArgPromote
            | PipelineStage::Gc
            | PipelineStage::ItemDce
            | PipelineStage::ExecGraphRebuild
            | PipelineStage::Full
    ) {
        assert_no_pipeline_errors("compile_and_run_pipeline_to", &result.errors);
    }

    (store, pkg_id)
}

/// Compiles library + user Q# source and runs the FIR pipeline, returning errors.
#[allow(dead_code)]
pub(crate) fn compile_and_run_pipeline_to_with_library_and_errors(
    lib_source: &str,
    user_source: &str,
    stage: PipelineStage,
) -> (fir::PackageStore, fir::PackageId, crate::PipelineResult) {
    let (mut store, pkg_id) = compile_to_fir_with_library(lib_source, user_source);
    let result = crate::run_pipeline_to_with_diagnostics(&mut store, pkg_id, stage, &[]);
    (store, pkg_id, result)
}

/// Compiles library + user Q# source and runs the FIR optimization pipeline
/// up to the given stage.
///
/// # Panics
///
/// Panics if compilation fails, or if the requested stage reaches
/// defunctionalization and the shared pipeline runner returns any errors.
#[allow(dead_code)]
pub(crate) fn compile_and_run_pipeline_to_with_library(
    lib_source: &str,
    user_source: &str,
    stage: PipelineStage,
) -> (fir::PackageStore, fir::PackageId) {
    let (store, pkg_id, result) =
        compile_and_run_pipeline_to_with_library_and_errors(lib_source, user_source, stage);
    if matches!(
        stage,
        PipelineStage::Defunc
            | PipelineStage::UdtErase
            | PipelineStage::TupleCompLower
            | PipelineStage::Sroa
            | PipelineStage::ArgPromote
            | PipelineStage::Gc
            | PipelineStage::ItemDce
            | PipelineStage::ExecGraphRebuild
            | PipelineStage::Full
    ) {
        assert_no_pipeline_errors("compile_and_run_pipeline_to_with_library", &result.errors);
    }
    (store, pkg_id)
}

#[allow(dead_code)]
fn local_name(package: &Package, local_id: LocalVarId) -> Option<&str> {
    package.pats.values().find_map(|pat| match &pat.kind {
        PatKind::Bind(ident) if ident.id == local_id => Some(ident.name.as_ref()),
        PatKind::Bind(_) | PatKind::Tuple(_) | PatKind::Discard => None,
    })
}

#[allow(dead_code)]
fn callable_ref_short(package: &Package, pkg_id: fir::PackageId, expr_id: ExprId) -> String {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Var(Res::Item(item_id), _) if item_id.package == pkg_id => {
            match &package.get_item(item_id.item).kind {
                ItemKind::Callable(decl) => decl.name.name.to_string(),
                _ => format!("Item({item_id})"),
            }
        }
        ExprKind::Var(Res::Item(item_id), _) => format!("Item({item_id})"),
        ExprKind::Var(Res::Local(local_id), _) => match local_name(package, *local_id) {
            Some(name) => format!("Local({name})"),
            None => format!("Local({local_id})"),
        },
        ExprKind::UnOp(op, inner) => {
            format!("{op}({})", callable_ref_short(package, pkg_id, *inner))
        }
        _ => expr_kind_short(package, expr_id),
    }
}

#[allow(dead_code)]
fn expr_detail_short(package: &Package, pkg_id: fir::PackageId, expr_id: ExprId) -> String {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Call(callee, args) => {
            let args_expr = package.get_expr(*args);
            format!(
                "Call({}, arg_ty={})",
                callable_ref_short(package, pkg_id, *callee),
                args_expr.ty
            )
        }
        _ => expr_kind_short(package, expr_id),
    }
}

#[allow(dead_code)]
fn push_spec_decl_summary(
    package: &Package,
    pkg_id: fir::PackageId,
    label: &str,
    spec: &SpecDecl,
    lines: &mut Vec<String>,
) {
    let block = package.get_block(spec.block);
    lines.push(format!("  {label}: block_ty={}", block.ty));
    for (index, stmt_id) in block.stmts.iter().enumerate() {
        let stmt = package.get_stmt(*stmt_id);
        let line = match &stmt.kind {
            StmtKind::Expr(expr_id) => {
                let expr = package.get_expr(*expr_id);
                format!(
                    "    [{index}] Expr ty={} {}",
                    expr.ty,
                    expr_detail_short(package, pkg_id, *expr_id)
                )
            }
            StmtKind::Semi(expr_id) => {
                let expr = package.get_expr(*expr_id);
                format!(
                    "    [{index}] Semi ty={} {}",
                    expr.ty,
                    expr_detail_short(package, pkg_id, *expr_id)
                )
            }
            StmtKind::Local(_, pat_id, expr_id) => {
                let pat = package.get_pat(*pat_id);
                let expr = package.get_expr(*expr_id);
                format!(
                    "    [{index}] Local pat_ty={} init_ty={} {}",
                    pat.ty,
                    expr.ty,
                    expr_detail_short(package, pkg_id, *expr_id)
                )
            }
            StmtKind::Item(local_item_id) => format!("    [{index}] Item {local_item_id}"),
        };
        lines.push(line);
    }
}

/// Extracts a deterministic summary of reachable callable signatures and body
/// shapes for the given package.
///
/// Entries are sorted alphabetically before being joined so `expect_test`
/// snapshots remain stable across runs regardless of the iteration order of
/// the underlying reachable-set container.
#[allow(dead_code)]
pub(crate) fn extract_reachable_callable_details(
    store: &fir::PackageStore,
    pkg_id: fir::PackageId,
) -> String {
    let package = store.get(pkg_id);
    let reachable = crate::reachability::collect_reachable_from_entry(store, pkg_id);

    let mut entries = Vec::new();
    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            let pat = package.get_pat(decl.input);
            let mut lines = vec![format!(
                "callable {}: input_ty={}, output_ty={}",
                decl.name.name, pat.ty, decl.output
            )];

            match &decl.implementation {
                CallableImpl::Intrinsic => lines.push("  intrinsic".to_string()),
                CallableImpl::SimulatableIntrinsic(spec) => {
                    push_spec_decl_summary(package, pkg_id, "simulatable", spec, &mut lines);
                }
                CallableImpl::Spec(spec_impl) => {
                    push_spec_decl_summary(package, pkg_id, "body", &spec_impl.body, &mut lines);
                    for (label, spec) in [
                        ("adj", spec_impl.adj.as_ref()),
                        ("ctl", spec_impl.ctl.as_ref()),
                        ("ctl_adj", spec_impl.ctl_adj.as_ref()),
                    ] {
                        if let Some(spec) = spec {
                            push_spec_decl_summary(package, pkg_id, label, spec, &mut lines);
                        }
                    }
                }
            }

            entries.push(lines.join("\n"));
        }
    }
    entries.sort();
    entries.join("\n")
}

/// Finds a callable by name among reachable items from a non-root package
/// (typically a library package). Panics if the callable is not found.
#[allow(dead_code)]
pub(crate) fn find_library_callable(
    store: &fir::PackageStore,
    root_pkg_id: fir::PackageId,
    callable_name: &str,
) -> fir::StoreItemId {
    crate::reachability::collect_reachable_from_entry(store, root_pkg_id)
        .into_iter()
        .find(|store_item_id| {
            if store_item_id.package == root_pkg_id {
                return false;
            }
            let package = store.get(store_item_id.package);
            let item = package.get_item(store_item_id.item);
            matches!(
                &item.kind,
                fir::ItemKind::Callable(decl) if decl.name.name.as_ref() == callable_name
            )
        })
        .unwrap_or_else(|| {
            panic!("library callable '{callable_name}' not found among reachable items")
        })
}

/// Asserts that the named callable body ends in an expression whose type
/// matches the enclosing block type.
pub fn assert_callable_body_terminal_expr_matches_block_type(
    store: &fir::PackageStore,
    pkg_id: fir::PackageId,
    callable_name: &str,
) {
    let package = store.get(pkg_id);
    let item = package
        .items
        .values()
        .find(|item| match &item.kind {
            ItemKind::Callable(decl) => decl.name.name.as_ref() == callable_name,
            _ => false,
        })
        .expect("callable should exist");

    let ItemKind::Callable(decl) = &item.kind else {
        panic!("item should be callable");
    };
    let spec = match &decl.implementation {
        CallableImpl::Spec(spec_impl) => &spec_impl.body,
        CallableImpl::SimulatableIntrinsic(spec) => spec,
        CallableImpl::Intrinsic => panic!("callable '{callable_name}' should have a body"),
    };

    let block = package.get_block(spec.block);
    let last_stmt_id = *block
        .stmts
        .last()
        .expect("callable body should not be empty");
    let last_stmt = package.get_stmt(last_stmt_id);
    let StmtKind::Expr(expr_id) = last_stmt.kind else {
        panic!(
            "callable '{callable_name}' should end in an Expr stmt, got {:?}",
            last_stmt.kind
        );
    };
    let expr = package.get_expr(expr_id);
    assert_eq!(
        expr.ty, block.ty,
        "callable '{callable_name}' trailing expr type should match block type"
    );
}

/// Returns a short human-readable label for an expression kind.
///
/// Used to annotate exec graph snapshot nodes for readability.
/// Includes sub-discriminant info for `BinOp`, `UnOp`, `AssignOp`, and `Lit`.
#[must_use]
pub fn expr_kind_short(package: &Package, expr_id: ExprId) -> String {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Array(items) => format!("Array(len={})", items.len()),
        ExprKind::ArrayLit(items) => format!("ArrayLit(len={})", items.len()),
        ExprKind::ArrayRepeat(_, _) => "ArrayRepeat".to_string(),
        ExprKind::Assign(_, _) => "Assign".to_string(),
        ExprKind::AssignField(_, _, _) => "AssignField".to_string(),
        ExprKind::AssignIndex(_, _, _) => "AssignIndex".to_string(),
        ExprKind::AssignOp(op, _, _) => format!("AssignOp({op:?})"),
        ExprKind::BinOp(op, _, _) => format!("BinOp({op:?})"),
        ExprKind::Block(_) => "Block".to_string(),
        ExprKind::Call(_, _) => "Call".to_string(),
        ExprKind::Closure(_, _) => "Closure".to_string(),
        ExprKind::Fail(_) => "Fail".to_string(),
        ExprKind::Field(_, _) => "Field".to_string(),
        ExprKind::Hole => "Hole".to_string(),
        ExprKind::If(_, _, _) => "If".to_string(),
        ExprKind::Index(_, _) => "Index".to_string(),
        ExprKind::Lit(lit) => format!("Lit({lit:?})"),
        ExprKind::Range(_, _, _) => "Range".to_string(),
        ExprKind::Return(_) => "Return".to_string(),
        ExprKind::String(parts) => format!("String(parts={})", parts.len()),
        ExprKind::Struct(_, _, _) => "Struct".to_string(),
        ExprKind::Tuple(es) => format!("Tuple(len={})", es.len()),
        ExprKind::UnOp(op, _) => format!("UnOp({op:?})"),
        ExprKind::UpdateField(_, _, _) => "UpdateField".to_string(),
        ExprKind::UpdateIndex(_, _, _) => "UpdateIndex".to_string(),
        ExprKind::Var(_, _) => "Var".to_string(),
        ExprKind::While(_, _) => "While".to_string(),
    }
}

/// Returns a short human-readable label for a statement kind.
///
/// Used to annotate exec graph snapshot nodes for readability.
#[allow(dead_code)]
pub(crate) fn stmt_kind_short(package: &Package, stmt_id: StmtId) -> String {
    let stmt = package.get_stmt(stmt_id);
    match &stmt.kind {
        StmtKind::Expr(_) => "Expr".to_string(),
        StmtKind::Item(_) => "Item".to_string(),
        StmtKind::Local(_, _, _) => "Local".to_string(),
        StmtKind::Semi(_) => "Semi".to_string(),
    }
}

/// Formats a pattern as a human-readable string showing binding names, types,
/// and tuple structure.
#[allow(dead_code)]
pub(crate) fn format_pat(package: &Package, pat_id: PatId) -> String {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => format!("Bind({}: {})", ident.name, pat.ty),
        PatKind::Tuple(sub_pats) => {
            let subs: Vec<String> = sub_pats.iter().map(|&id| format_pat(package, id)).collect();
            format!("Tuple({})", subs.join(", "))
        }
        PatKind::Discard => format!("Discard({})", pat.ty),
    }
}

/// Collects all pattern bindings in a package into a map from local variable
/// ID to its name.
#[allow(dead_code)]
pub(crate) fn local_names(package: &Package) -> FxHashMap<LocalVarId, String> {
    package
        .pats
        .values()
        .filter_map(|pat| match &pat.kind {
            PatKind::Bind(ident) => Some((ident.id, ident.name.to_string())),
            PatKind::Tuple(_) | PatKind::Discard => None,
        })
        .collect()
}

/// Finds a callable declaration by name in the given package. Panics if not
/// found.
#[allow(dead_code)]
pub(crate) fn find_callable<'a>(package: &'a Package, callable_name: &str) -> &'a CallableDecl {
    package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == callable_name => {
                Some(decl.as_ref())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("callable '{callable_name}' not found"))
}

fn callable_body_spec<'a>(decl: &'a CallableDecl, callable_name: &str) -> &'a SpecDecl {
    match &decl.implementation {
        CallableImpl::Spec(spec_impl) => &spec_impl.body,
        CallableImpl::SimulatableIntrinsic(spec) => spec,
        CallableImpl::Intrinsic => panic!("callable '{callable_name}' should have a body"),
    }
}

#[must_use]
pub fn format_reachable_callable_summary(
    store: &fir::PackageStore,
    pkg_id: fir::PackageId,
) -> String {
    let package = store.get(pkg_id);
    let reachable = crate::reachability::collect_reachable_from_entry(store, pkg_id);

    let mut lines = Vec::new();
    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            let pat = package.get_pat(decl.input);
            lines.push(format!(
                "{}: input_ty={}, output_ty={}",
                decl.name.name, pat.ty, decl.output
            ));
        }
    }
    lines.sort();
    lines.join("\n")
}

#[must_use]
pub fn format_callable_body_summary(
    store: &fir::PackageStore,
    pkg_id: fir::PackageId,
    callable_name: &str,
) -> String {
    let package = store.get(pkg_id);
    let decl = find_callable(package, callable_name);
    let spec = callable_body_spec(decl, callable_name);
    let block = package.get_block(spec.block);

    let mut lines = vec![format!("block_ty={}", block.ty)];
    for (index, stmt_id) in block.stmts.iter().enumerate() {
        let stmt = package.get_stmt(*stmt_id);
        let line = match &stmt.kind {
            StmtKind::Expr(expr_id) => {
                let expr = package.get_expr(*expr_id);
                format!(
                    "[{index}] Expr ty={} {}",
                    expr.ty,
                    expr_kind_short(package, *expr_id)
                )
            }
            StmtKind::Semi(expr_id) => {
                let expr = package.get_expr(*expr_id);
                format!(
                    "[{index}] Semi ty={} {}",
                    expr.ty,
                    expr_kind_short(package, *expr_id)
                )
            }
            StmtKind::Local(_, pat_id, expr_id) => {
                let pat = package.get_pat(*pat_id);
                let expr = package.get_expr(*expr_id);
                format!(
                    "[{index}] Local pat_ty={} init_ty={} {}",
                    pat.ty,
                    expr.ty,
                    expr_kind_short(package, *expr_id)
                )
            }
            StmtKind::Item(local_item_id) => format!("[{index}] Item {local_item_id}"),
        };
        lines.push(line);
    }

    lines.join("\n")
}

/// Compiles Q# source through the full FIR pipeline, then generates QIR via
/// partial evaluation and codegen. Uses Adaptive + `IntegerComputations`
/// capabilities so that Result-comparison programs can be lowered.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn generate_qir(source: &str) -> String {
    use qsc_codegen::qir::fir_to_qir;
    use qsc_data_structures::target::TargetCapabilityFlags;
    use qsc_partial_eval::ProgramEntry;

    let capabilities = TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations;
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    let package = store.get(pkg_id);
    let entry = ProgramEntry {
        exec_graph: package.entry_exec_graph.clone(),
        expr: (
            pkg_id,
            package
                .entry
                .expect("package must have an entry expression"),
        )
            .into(),
    };
    let compute_properties = qsc_rca::Analyzer::init(&store, capabilities).analyze_all();
    fir_to_qir(&store, capabilities, &compute_properties, &entry).expect("QIR generation failed")
}

/// Evaluates the entry exec graph of the given FIR store with a fixed
/// simulator seed for determinism. Returns `Ok(value)` on success, or
/// `Err(error_string)` on evaluation failure.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn try_eval_fir_entry(
    store: &fir::PackageStore,
    pkg_id: fir::PackageId,
) -> Result<qsc_eval::val::Value, String> {
    use qsc_eval::backend::{SparseSim, TracingBackend};
    use qsc_eval::output::GenericReceiver;
    use qsc_fir::fir::ExecGraphConfig;

    let package = store.get(pkg_id);
    let entry_graph = package.entry_exec_graph.clone();
    let mut env = qsc_eval::Env::default();
    let mut sim = SparseSim::new();
    let mut out = Vec::<u8>::new();
    let mut receiver = GenericReceiver::new(&mut out);
    qsc_eval::eval(
        pkg_id,
        Some(42),
        entry_graph,
        ExecGraphConfig::NoDebug,
        store,
        &mut env,
        &mut TracingBackend::no_tracer(&mut sim),
        &mut receiver,
    )
    .map_err(|(err, _frames)| format!("{err:?}"))
}

/// Compiles Q# source to FIR with cached core/std HIR setup and evaluates the
/// entry exec graph.
///
/// The FIR has no transforms applied — this captures the original program
/// semantics.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn eval_qsharp_original(source: &str) -> Result<qsc_eval::val::Value, String> {
    let (fir_store, pkg_id) =
        compile_to_fir_with_cached_stdlib(source, None, TargetCapabilityFlags::empty());
    try_eval_fir_entry(&fir_store, pkg_id)
}

/// Compiles library + user Q# source to FIR using a single lowerer (no
/// transforms) and evaluates the entry exec graph.
///
/// The FIR has no transforms applied — this captures the original program
/// semantics with a cross-package library dependency.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn eval_qsharp_original_with_library(
    lib_source: &str,
    user_source: &str,
) -> Result<qsc_eval::val::Value, String> {
    let mut lowerer = qsc_lowerer::Lowerer::new();
    let mut core = frontend_compile::core();
    run_core_passes(&mut core);
    let fir_store = fir::PackageStore::new();
    let core_fir = lowerer.lower_package(&core.package, &fir_store);
    let mut hir_store = HirPackageStore::new(core);

    let mut std = frontend_compile::std(&hir_store, TargetCapabilityFlags::empty());
    assert!(std.errors.is_empty());
    assert!(run_default_passes(hir_store.core(), &mut std, PackageType::Lib).is_empty());
    let std_fir = lowerer.lower_package(&std.package, &fir_store);
    let std_id = hir_store.insert(std);

    // Library package
    let lib_sources = SourceMap::new(vec![("lib.qs".into(), lib_source.into())], None);
    let mut lib_unit = frontend_compile::compile(
        &hir_store,
        &[(PackageId::CORE, None), (std_id, None)],
        lib_sources,
        TargetCapabilityFlags::empty(),
        LanguageFeatures::default(),
    );
    assert!(lib_unit.errors.is_empty(), "{:?}", lib_unit.errors);
    let lib_pass_errors = run_default_passes(hir_store.core(), &mut lib_unit, PackageType::Lib);
    assert!(lib_pass_errors.is_empty(), "{lib_pass_errors:?}");
    let lib_fir = lowerer.lower_package(&lib_unit.package, &fir_store);
    let lib_id = hir_store.insert(lib_unit);

    // User package
    let user_sources = SourceMap::new(vec![("test.qs".into(), user_source.into())], None);
    let mut unit = frontend_compile::compile(
        &hir_store,
        &[(PackageId::CORE, None), (std_id, None), (lib_id, None)],
        user_sources,
        TargetCapabilityFlags::empty(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let pass_errors = run_default_passes(hir_store.core(), &mut unit, PackageType::Exe);
    assert!(pass_errors.is_empty(), "{pass_errors:?}");
    let unit_fir = lowerer.lower_package(&unit.package, &fir_store);
    let user_hir_id = hir_store.insert(unit);

    let mut fir_store = fir::PackageStore::new();
    fir_store.insert(map_hir_package_to_fir(PackageId::CORE), core_fir);
    fir_store.insert(map_hir_package_to_fir(std_id), std_fir);
    fir_store.insert(map_hir_package_to_fir(lib_id), lib_fir);
    fir_store.insert(map_hir_package_to_fir(user_hir_id), unit_fir);

    try_eval_fir_entry(&fir_store, map_hir_package_to_fir(user_hir_id))
}

/// Compiles Q# source, runs the full FIR transform pipeline, and evaluates
/// the entry exec graph.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn eval_qsharp_transformed(source: &str) -> Result<qsc_eval::val::Value, String> {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    try_eval_fir_entry(&store, pkg_id)
}

/// Asserts semantic equivalence of a Q# program before and after the
/// full FIR transform pipeline.
///
/// 1. Compiles the original Q# source (no transforms) and evaluates it to
///    get the expected return value.
/// 2. Compiles and runs the full FIR pipeline, then evaluates to get the
///    actual return value.
/// 3. Asserts the two results match (both succeed with equal values, or
///    both fail).
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn check_semantic_equivalence(source: &str) {
    let expected = eval_qsharp_original(source);
    let actual = eval_qsharp_transformed(source);

    match (&expected, &actual) {
        (Ok(exp_val), Ok(act_val)) => {
            assert_eq!(
                exp_val, act_val,
                "semantic equivalence violated: original returned {exp_val}, \
                 transformed returned {act_val}"
            );
        }
        (Err(exp_err), Err(act_err)) => {
            assert_eq!(
                exp_err, act_err,
                "semantic equivalence violated: original failed with {exp_err}, transformed failed with {act_err}"
            );
        }
        (Ok(exp_val), Err(err)) => {
            panic!("original succeeded with {exp_val} but transformed failed: {err}");
        }
        (Err(err), Ok(act_val)) => {
            panic!("original failed with {err} but transformed succeeded with {act_val}");
        }
    }
}

/// Asserts semantic equivalence of a cross-package Q# program before and
/// after the full FIR transform pipeline.
///
/// 1. Compiles library + user Q# source (no transforms) and evaluates to
///    get the expected return value.
/// 2. Compiles and runs the full FIR pipeline, then evaluates to get the
///    actual return value.
/// 3. Asserts the two results match.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn check_semantic_equivalence_with_library(lib_source: &str, user_source: &str) {
    let expected = eval_qsharp_original_with_library(lib_source, user_source);
    let actual = {
        let (store, pkg_id) =
            compile_and_run_pipeline_to_with_library(lib_source, user_source, PipelineStage::Full);
        try_eval_fir_entry(&store, pkg_id)
    };

    match (&expected, &actual) {
        (Ok(exp_val), Ok(act_val)) => {
            assert_eq!(
                exp_val, act_val,
                "semantic equivalence violated: original returned {exp_val}, \
                 transformed returned {act_val}"
            );
        }
        (Err(exp_err), Err(act_err)) => {
            assert_eq!(
                exp_err, act_err,
                "semantic equivalence violated: original failed with {exp_err}, \
                 transformed failed with {act_err}"
            );
        }
        (Ok(exp_val), Err(err)) => {
            panic!("original succeeded with {exp_val} but transformed failed: {err}");
        }
        (Err(err), Ok(act_val)) => {
            panic!("original failed with {err} but transformed succeeded with {act_val}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::*;

    fn panic_message(panic: Box<dyn Any + Send>) -> String {
        match panic.downcast::<String>() {
            Ok(message) => *message,
            Err(panic) => match panic.downcast::<&str>() {
                Ok(message) => (*message).to_string(),
                Err(_) => "(non-string panic payload)".to_string(),
            },
        }
    }

    #[test]
    fn staged_runner_with_errors_returns_defunctionalization_diagnostics() {
        let source = r#"
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
            operation Main() : Unit {
                use q = Qubit();
                mutable op = H;
                mutable n = 3;
                while n > 0 {
                    op = X;
                    n -= 1;
                }
                ApplyOp(op, q);
            }
        "#;

        let (_store, _pkg_id, result) =
            compile_and_run_pipeline_to_with_errors(source, PipelineStage::Full);

        assert!(
            !result.errors.is_empty(),
            "expected defunctionalization diagnostics to be returned"
        );
        let messages = result
            .errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            messages.contains("callable argument could not be resolved statically"),
            "unexpected diagnostics: {messages}"
        );
    }

    #[test]
    fn checked_staged_runner_panics_on_unexpected_defunctionalization_diagnostics() {
        let source = r#"
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
            operation Main() : Unit {
                use q = Qubit();
                mutable op = H;
                mutable n = 3;
                while n > 0 {
                    op = X;
                    n -= 1;
                }
                ApplyOp(op, q);
            }
        "#;

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = compile_and_run_pipeline_to(source, PipelineStage::Full);
        }))
        .expect_err("checked staged runner should panic on unexpected diagnostics");
        let message = panic_message(panic);
        assert!(
            message.contains("compile_and_run_pipeline_to produced FIR transform pipeline errors"),
            "unexpected panic: {message}"
        );
        assert!(
            message.contains("callable argument could not be resolved statically"),
            "unexpected panic: {message}"
        );
    }

    #[test]
    fn reachable_callable_details_report_body_shape() {
        let source = r#"
            namespace Test {
                function Helper(x : Int) : Int { x + 1 }

                @EntryPoint()
                function Main() : Int {
                    Helper(2)
                }
            }
        "#;

        let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
        let summary = extract_reachable_callable_details(&store, pkg_id);

        assert!(
            summary.contains("callable Helper: input_ty=Int, output_ty=Int"),
            "unexpected summary: {summary}"
        );
        assert!(
            summary.contains("callable Main: input_ty=Unit, output_ty=Int"),
            "unexpected summary: {summary}"
        );
        assert!(
            summary.contains("body: block_ty=Int"),
            "unexpected summary: {summary}"
        );

        assert_callable_body_terminal_expr_matches_block_type(&store, pkg_id, "Helper");
        assert_callable_body_terminal_expr_matches_block_type(&store, pkg_id, "Main");
    }
}
