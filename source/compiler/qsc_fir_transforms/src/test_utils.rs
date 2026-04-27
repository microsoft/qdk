// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Shared test helpers for the `qsc_fir_transforms` crate.
//!
//! Provides compilation and snapshot utilities used across transform test
//! modules. Gated behind `#[cfg(any(test, feature = "testutil"))]`.

use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_fir::fir::{
    self, CallableImpl, ExprId, ExprKind, ItemKind, LocalVarId, Package, PackageLookup, PatKind,
    Res, SpecDecl, StmtId, StmtKind,
};
use qsc_frontend::compile::{self as frontend_compile, PackageStore as HirPackageStore};
use qsc_hir::hir::PackageId;
use qsc_passes::{PackageType, lower_hir_to_fir, run_core_passes, run_default_passes};

#[cfg(test)]
use qsc_lowerer::map_hir_package_to_fir;

pub(crate) use crate::PipelineStage;

pub fn assert_no_pipeline_errors(context: &str, errors: &[crate::PipelineError]) {
    let error_messages = errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        errors.is_empty(),
        "{context} produced FIR transform pipeline errors:\n{error_messages}"
    );
}

/// Sets up an HIR package store containing core + std libraries with default
/// passes applied, using the given target capabilities.
#[must_use]
pub fn package_store_with_stdlib(capabilities: TargetCapabilityFlags) -> HirPackageStore {
    let mut core_unit = frontend_compile::core();
    let core_errors = run_core_passes(&mut core_unit);
    assert!(
        core_errors.is_empty(),
        "core library has compilation errors"
    );
    let mut store = HirPackageStore::new(core_unit);

    let mut std_unit = frontend_compile::std(&store, capabilities);
    let std_errors = run_default_passes(store.core(), &mut std_unit, PackageType::Lib);
    assert!(std_errors.is_empty(), "std library has compilation errors");
    store.insert(std_unit);

    store
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
    let mut store = package_store_with_stdlib(capabilities);
    let std_id = PackageId::CORE.successor();
    let sources = SourceMap::new(vec![("test.qs".into(), source.into())], None);
    let mut unit = frontend_compile::compile(
        &store,
        &[(PackageId::CORE, None), (std_id, None)],
        sources,
        capabilities,
        LanguageFeatures::default(),
    );
    let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Exe);
    assert!(pass_errors.is_empty(), "user code has compilation errors");
    let hir_package_id = store.insert(unit);
    let (fir_store, fir_pkg_id, _) = lower_hir_to_fir(&store, hir_package_id);
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
    let mut store = package_store_with_stdlib(TargetCapabilityFlags::empty());
    let std_id = PackageId::CORE.successor();
    let sources = SourceMap::new(vec![("test.qs".into(), source.into())], Some(entry.into()));
    let mut unit = frontend_compile::compile(
        &store,
        &[(PackageId::CORE, None), (std_id, None)],
        sources,
        TargetCapabilityFlags::empty(),
        LanguageFeatures::default(),
    );
    let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Exe);
    assert!(pass_errors.is_empty(), "user code has compilation errors");
    let hir_package_id = store.insert(unit);
    let (fir_store, fir_pkg_id, _) = lower_hir_to_fir(&store, hir_package_id);
    (fir_store, fir_pkg_id)
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
) -> (fir::PackageStore, fir::PackageId, Vec<crate::PipelineError>) {
    let (mut store, pkg_id) = compile_to_fir(source);
    let errors = crate::run_pipeline_to(&mut store, pkg_id, stage, &[]);
    (store, pkg_id, errors)
}

/// Compiles Q# source and runs the FIR optimization pipeline up to the given
/// stage, asserting that defunctionalization diagnostics stay empty once the
/// schedule reaches or passes that stage.
#[allow(dead_code)]
pub(crate) fn compile_and_run_pipeline_to(
    source: &str,
    stage: PipelineStage,
) -> (fir::PackageStore, fir::PackageId) {
    let (store, pkg_id, errors) = compile_and_run_pipeline_to_with_errors(source, stage);
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
        assert_no_pipeline_errors("compile_and_run_pipeline_to", &errors);
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

/// Evaluates the entry exec graph of the given FIR store with a fixed
/// simulator seed for determinism. Returns `Ok(value)` on success, or
/// `Err(error_string)` on evaluation failure.
#[cfg(test)]
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

/// Compiles Q# source to FIR using a single lowerer (matching the
/// `qsc_eval` test pattern), and evaluates the entry exec graph.
///
/// The FIR has no transforms applied — this captures the original program
/// semantics.
#[cfg(test)]
pub(crate) fn eval_qsharp_original(source: &str) -> Result<qsc_eval::val::Value, String> {
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

    let sources = SourceMap::new(vec![("test.qs".into(), source.into())], None);
    let mut unit = frontend_compile::compile(
        &hir_store,
        &[(PackageId::CORE, None), (std_id, None)],
        sources,
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
    fir_store.insert(map_hir_package_to_fir(user_hir_id), unit_fir);

    try_eval_fir_entry(&fir_store, map_hir_package_to_fir(user_hir_id))
}

/// Compiles Q# source, runs the full FIR transform pipeline, and evaluates
/// the entry exec graph.
#[cfg(test)]
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
        (Err(_), Err(_)) => {
            // Both failed — the transform preserves the error behavior.
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

        let (_store, _pkg_id, errors) =
            compile_and_run_pipeline_to_with_errors(source, PipelineStage::Full);

        assert!(
            !errors.is_empty(),
            "expected defunctionalization diagnostics to be returned"
        );
        let messages = errors
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
