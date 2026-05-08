// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::needless_raw_string_hashes)]

//! Tests for the return unification pass.

mod contracts_and_errors;
mod flag_strategy;
mod idempotency;
mod qubit_release;
mod regressions;
mod semantic;
mod structured_strategy;
mod type_preservation;

use expect_test::{Expect, expect};
use rustc_hash::FxHashSet;

use crate::reachability::collect_reachable_from_entry;
use crate::test_utils::{
    PipelineStage, compile_and_run_pipeline_to, compile_and_run_pipeline_to_with_errors,
    compile_to_fir,
};
use crate::walk_utils::{for_each_expr, for_each_expr_in_callable_impl};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::{
    BinOp, BlockId, CallableImpl, Expr, ExprId, ExprKind, ItemKind, Lit, LocalVarId, Package,
    PackageId, PackageLookup, PackageStore, Pat, PatKind, Res, StmtId, StmtKind, StoreItemId, UnOp,
};
use qsc_fir::ty::{Prim, Ty};

pub(crate) type ReleaseCallableSet = FxHashSet<StoreItemId>;

/// Collects the set of callables that release qubit allocations.
pub(crate) fn collect_release_callables(store: &PackageStore) -> ReleaseCallableSet {
    let mut release_callables = FxHashSet::default();
    for (package_id, package) in store {
        for (item_id, item) in &package.items {
            let ItemKind::Callable(decl) = &item.kind else {
                continue;
            };
            if matches!(
                decl.name.name.as_ref(),
                "__quantum__rt__qubit_release" | "ReleaseQubitArray"
            ) {
                release_callables.insert(StoreItemId {
                    package: package_id,
                    item: item_id,
                });
            }
        }
    }
    release_callables
}

/// Test-only reimplementation of the removed `is_release_call` helper.
fn is_release_call_test(
    package: &Package,
    stmt_id: StmtId,
    release_set: &ReleaseCallableSet,
) -> bool {
    let stmt = package.get_stmt(stmt_id);
    let StmtKind::Semi(expr_id) = &stmt.kind else {
        return false;
    };
    let expr = package.get_expr(*expr_id);
    let ExprKind::Call(callee_id, _) = &expr.kind else {
        return false;
    };
    let callee = package.get_expr(*callee_id);
    let ExprKind::Var(Res::Item(item_id), _) = &callee.kind else {
        return false;
    };
    release_set.contains(&StoreItemId {
        package: item_id.package,
        item: item_id.item,
    })
}

struct NoHoistReturnUnifyResult {
    store: PackageStore,
    pkg_id: PackageId,
    before: String,
    after: String,
}

impl NoHoistReturnUnifyResult {
    fn before_after(&self) -> String {
        format!(
            "// before direct no-hoist return_unify\n{}\n// post direct no-hoist return_unify\n{}",
            self.before, self.after
        )
    }
}

pub(crate) fn assert_no_reachable_returns(store: &PackageStore, pkg_id: PackageId) {
    let package = store.get(pkg_id);
    let reachable = collect_reachable_from_entry(store, pkg_id);

    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            for_each_expr_in_callable_impl(package, &decl.implementation, &mut |_id, expr| {
                assert!(
                    !matches!(expr.kind, ExprKind::Return(_)),
                    "Return node found in callable '{}' after direct no-hoist return unification",
                    decl.name.name
                );
            });
        }
    }
}

fn compile_no_hoist_return_unified(source: &str) -> NoHoistReturnUnifyResult {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
    let before = crate::pretty::write_package_qsharp(&store, pkg_id);

    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let errors = super::unify_returns(&mut store, pkg_id, &mut assigner);
    assert!(
        errors.is_empty(),
        "direct no-hoist return_unify produced errors: {errors:?}\n// before direct no-hoist return_unify\n{before}"
    );
    assert_no_reachable_returns(&store, pkg_id);

    let after = crate::pretty::write_package_qsharp(&store, pkg_id);
    NoHoistReturnUnifyResult {
        store,
        pkg_id,
        before,
        after,
    }
}

fn release_store_id(package: &Package, expr: &Expr) -> Option<StoreItemId> {
    let ExprKind::Call(callee_id, _) = &expr.kind else {
        return None;
    };
    let callee = package.get_expr(*callee_id);
    let ExprKind::Var(Res::Item(item_id), _) = &callee.kind else {
        return None;
    };
    Some(StoreItemId {
        package: item_id.package,
        item: item_id.item,
    })
}

fn expr_contains_release_call(
    package: &Package,
    expr_id: ExprId,
    release_set: &ReleaseCallableSet,
) -> bool {
    let mut has_release = false;
    for_each_expr(package, expr_id, &mut |_id, expr| {
        has_release |= release_store_id(package, expr).is_some_and(|id| release_set.contains(&id));
    });
    has_release
}

fn stmt_contains_path_local_release_value(
    package: &Package,
    stmt_id: StmtId,
    release_set: &ReleaseCallableSet,
) -> bool {
    let stmt = package.get_stmt(stmt_id);
    match stmt.kind {
        StmtKind::Local(_, _, init_expr_id) | StmtKind::Expr(init_expr_id) => {
            expr_contains_release_call(package, init_expr_id, release_set)
        }
        StmtKind::Semi(expr_id) => {
            release_store_id(package, package.get_expr(expr_id)).is_none()
                && expr_contains_release_call(package, expr_id, release_set)
        }
        StmtKind::Item(_) => false,
    }
}

fn assert_path_local_releases_without_unconditional_suffix(
    result: &NoHoistReturnUnifyResult,
    callable_name: &str,
) {
    let package = result.store.get(result.pkg_id);
    let release_set = collect_release_callables(&result.store);
    let body_block_id = find_body_block_id(package, callable_name);
    let body_block = package.get_block(body_block_id);

    let Some(path_local_release_index) = body_block.stmts.iter().position(|&stmt_id| {
        stmt_contains_path_local_release_value(package, stmt_id, &release_set)
    }) else {
        panic!(
            "{callable_name} should preserve at least one path-local release after direct no-hoist return_unify\n{}",
            result.before_after()
        );
    };

    let release_suffix_after_path_local = body_block.stmts[path_local_release_index + 1..]
        .iter()
        .any(|&stmt_id| is_release_call_test(package, stmt_id, &release_set));

    assert!(
        !release_suffix_after_path_local,
        "{callable_name} should not run an unconditional release suffix after a value path that already contains path-local releases\n{}",
        result.before_after()
    );
}

fn expr_contains_guarded_release_call(
    package: &Package,
    expr_id: ExprId,
    release_set: &ReleaseCallableSet,
    has_returned_var_id: LocalVarId,
) -> bool {
    let mut found_guarded_release = false;
    for_each_expr(package, expr_id, &mut |_id, expr| {
        let ExprKind::If(cond_expr_id, then_expr_id, None) = &expr.kind else {
            return;
        };

        found_guarded_release |= is_not_flag_expr(package, *cond_expr_id, has_returned_var_id)
            && expr_contains_release_call(package, *then_expr_id, release_set);
    });
    found_guarded_release
}

fn assert_guarded_release_continuation(result: &NoHoistReturnUnifyResult, callable_name: &str) {
    let package = result.store.get(result.pkg_id);
    let release_set = collect_release_callables(&result.store);
    let (flag_pat, _) = find_local_init(package, callable_name, "__has_returned");
    let has_returned_var_id = local_var_id_from_named_pat(flag_pat, "__has_returned");
    let decl = find_callable_decl(package, callable_name);

    let mut found_guarded_release = false;
    for_each_expr_in_callable_impl(package, &decl.implementation, &mut |expr_id, _expr| {
        found_guarded_release |=
            expr_contains_guarded_release_call(package, expr_id, &release_set, has_returned_var_id);
    });

    assert!(
        found_guarded_release,
        "{callable_name} should guard release continuations with not __has_returned after direct no-hoist return_unify\n{}",
        result.before_after()
    );
}

fn eval_qsharp_no_hoist_return_unified(source: &str) -> Result<qsc_eval::val::Value, String> {
    let NoHoistReturnUnifyResult {
        mut store, pkg_id, ..
    } = compile_no_hoist_return_unified(source);
    crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, pkg_id, &[]);
    try_eval_fir_entry(&store, pkg_id)
}

fn check_no_hoist_semantic_equivalence(source: &str) {
    let expected = eval_qsharp_original(source);
    let actual = eval_qsharp_no_hoist_return_unified(source);

    match (&expected, &actual) {
        (Ok(exp_val), Ok(act_val)) => {
            assert_eq!(
                exp_val, act_val,
                "direct no-hoist return_unify semantic equivalence violated: original returned {exp_val}, transformed returned {act_val}"
            );
        }
        (Err(exp_err), Err(act_err)) => {
            assert_eq!(
                exp_err, act_err,
                "direct no-hoist return_unify semantic equivalence violated: original failed with {exp_err}, transformed failed with {act_err}"
            );
        }
        (Ok(exp_val), Err(err)) => {
            panic!(
                "original succeeded with {exp_val} but direct no-hoist return_unify failed: {err}"
            );
        }
        (Err(err), Ok(act_val)) => {
            panic!(
                "original failed with {err} but direct no-hoist return_unify succeeded with {act_val}"
            );
        }
    }
}

/// Compiles source through mono + `return_unify` and asserts no Return nodes
/// remain in any reachable callable. Returns a summary string of the body
/// structure for snapshot testing.
pub(crate) fn compile_return_unified(
    source: &str,
) -> (qsc_fir::fir::PackageStore, qsc_fir::fir::PackageId) {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ReturnUnify);
    assert_no_reachable_returns(&store, pkg_id);

    (store, pkg_id)
}

fn describe_pat(package: &Package, pat_id: qsc_fir::fir::PatId) -> String {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => format!("{}: {}", ident.name, pat.ty),
        PatKind::Tuple(items) => format!(
            "({})",
            items
                .iter()
                .map(|&item| describe_pat(package, item))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        PatKind::Discard => format!("_: {}", pat.ty),
    }
}

fn push_spec_summary(
    package: &Package,
    label: &str,
    spec: &qsc_fir::fir::SpecDecl,
    lines: &mut Vec<String>,
) {
    let block = package.get_block(spec.block);
    lines.push(format!("    {label}: block_ty={}", block.ty));
    for (index, stmt_id) in block.stmts.iter().enumerate() {
        let stmt = package.get_stmt(*stmt_id);
        let line = match &stmt.kind {
            StmtKind::Expr(expr_id) => {
                format!(
                    "        [{index}] Expr {}",
                    describe_expr(package, *expr_id)
                )
            }
            StmtKind::Semi(expr_id) => {
                format!(
                    "        [{index}] Semi {}",
                    describe_expr(package, *expr_id)
                )
            }
            StmtKind::Local(mutability, pat_id, expr_id) => format!(
                "        [{index}] Local({mutability:?}, {}): {}",
                describe_pat(package, *pat_id),
                describe_expr(package, *expr_id)
            ),
            StmtKind::Item(local_item_id) => format!("        [{index}] Item {local_item_id}"),
        };
        lines.push(line);
    }
}

fn summarize_callable(package: &Package, callable_name: &str) -> String {
    let decl = package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == callable_name => Some(decl),
            _ => None,
        })
        .unwrap_or_else(|| panic!("callable '{callable_name}' not found"));

    let mut lines = vec![format!(
        "callable {}: input_ty={}, output_ty={}",
        decl.name.name,
        package.get_pat(decl.input).ty,
        decl.output
    )];

    match &decl.implementation {
        CallableImpl::Intrinsic => lines.push("  intrinsic".to_string()),
        CallableImpl::Spec(spec_impl) => {
            push_spec_summary(package, "body", &spec_impl.body, &mut lines);
            for (label, spec) in [
                ("adj", spec_impl.adj.as_ref()),
                ("ctl", spec_impl.ctl.as_ref()),
                ("ctl_adj", spec_impl.ctl_adj.as_ref()),
            ] {
                if let Some(spec) = spec {
                    push_spec_summary(package, label, spec, &mut lines);
                }
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            push_spec_summary(package, "simulatable", spec, &mut lines);
        }
    }

    lines.join("\n")
}

/// Check the structure of callables after return unification.
pub(crate) fn check_structure(source: &str, callable_names: &[&str], expect: &Expect) {
    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);
    let summary = callable_names
        .iter()
        .map(|callable_name| summarize_callable(package, callable_name))
        .collect::<Vec<_>>()
        .join("\n");
    expect.assert_eq(&summary);
}

/// Compile, run the pipeline through `ReturnUnify`, assert no
/// `ExprKind::Return` survives in any reachable callable, and pin the
/// resulting FIR as formatted Q# via `expect_test`.
///
/// The `expect` snapshot is generated by
/// [`crate::pretty::write_package_qsharp`].
pub(crate) fn check_no_returns_q(source: &str, expect: &Expect) {
    let (store, pkg_id) = compile_return_unified(source);
    let rendered = crate::pretty::write_package_qsharp(&store, pkg_id);
    expect.assert_eq(&rendered);
}

fn check_pre_fir_transforms_to_return_unify_q(source: &str, expect: &Expect) {
    let (before_store, before_pkg_id) = compile_to_fir(source);
    let before = crate::pretty::write_package_qsharp(&before_store, before_pkg_id);

    let (after_store, after_pkg_id) = compile_return_unified(source);
    let after = crate::pretty::write_package_qsharp(&after_store, after_pkg_id);

    expect.assert_eq(&format!(
        "// before fir transforms\n{before}\n// post return_unify\n{after}"
    ));
}

fn find_local_init<'a>(
    package: &'a Package,
    callable_name: &str,
    local_name: &str,
) -> (&'a Pat, &'a Expr) {
    for item in package.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && decl.name.name.as_ref() == callable_name
            && let CallableImpl::Spec(spec) = &decl.implementation
        {
            let block = package.get_block(spec.body.block);
            for &stmt_id in &block.stmts {
                let stmt = package.get_stmt(stmt_id);
                let StmtKind::Local(_, pat_id, init_expr_id) = &stmt.kind else {
                    continue;
                };
                let pat = package.get_pat(*pat_id);
                if let PatKind::Bind(ident) = &pat.kind
                    && ident.name.as_ref() == local_name
                {
                    return (pat, package.get_expr(*init_expr_id));
                }
            }
        }
    }

    panic!("local '{local_name}' not found in callable '{callable_name}'");
}

fn find_callable_decl<'a>(
    package: &'a Package,
    callable_name: &str,
) -> &'a qsc_fir::fir::CallableDecl {
    package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == callable_name => Some(decl),
            _ => None,
        })
        .unwrap_or_else(|| panic!("callable '{callable_name}' not found"))
}

fn find_body_block_id(package: &Package, callable_name: &str) -> BlockId {
    let decl = find_callable_decl(package, callable_name);
    let CallableImpl::Spec(spec_impl) = &decl.implementation else {
        panic!("callable '{callable_name}' must have a body spec")
    };
    spec_impl.body.block
}

fn local_var_id_from_named_pat(pat: &Pat, local_name: &str) -> LocalVarId {
    let PatKind::Bind(ident) = &pat.kind else {
        panic!("local '{local_name}' should bind a single local var")
    };
    ident.id
}

fn expr_reads_local(package: &Package, expr_id: ExprId, expected_local: LocalVarId) -> bool {
    matches!(
        &package.get_expr(expr_id).kind,
        ExprKind::Var(Res::Local(local_id), _) if *local_id == expected_local
    )
}

fn is_not_flag_expr(package: &Package, expr_id: ExprId, has_returned_var_id: LocalVarId) -> bool {
    let ExprKind::UnOp(UnOp::NotL, inner_expr_id) = &package.get_expr(expr_id).kind else {
        return false;
    };
    expr_reads_local(package, *inner_expr_id, has_returned_var_id)
}

fn assert_while_condition_guarded_by_not_flag(
    package: &Package,
    cond_expr_id: ExprId,
    has_returned_var_id: LocalVarId,
) {
    let ExprKind::BinOp(BinOp::AndL, lhs_expr_id, _rhs_expr_id) =
        &package.get_expr(cond_expr_id).kind
    else {
        panic!("while condition should be rewritten to not __has_returned and cond")
    };

    assert!(
        is_not_flag_expr(package, *lhs_expr_id, has_returned_var_id),
        "while condition LHS should be not __has_returned"
    );
}

fn assignment_target_local(package: &Package, expr_id: ExprId) -> Option<LocalVarId> {
    let ExprKind::Assign(lhs_expr_id, _rhs_expr_id) = &package.get_expr(expr_id).kind else {
        return None;
    };
    let ExprKind::Var(Res::Local(local_id), _) = &package.get_expr(*lhs_expr_id).kind else {
        return None;
    };
    Some(*local_id)
}

fn assert_local_initializer_then_assign_order(
    package: &Package,
    init_expr_id: ExprId,
    ret_val_var_id: LocalVarId,
    has_returned_var_id: LocalVarId,
) -> bool {
    let ExprKind::If(_cond_expr_id, _then_expr_id, _else_expr_id) =
        &package.get_expr(init_expr_id).kind
    else {
        panic!("expected Local initializer to remain an if-expression")
    };

    let mut writes = Vec::new();
    for_each_expr(package, init_expr_id, &mut |_expr_id, expr| {
        let ExprKind::Assign(lhs_expr_id, _rhs_expr_id) = &expr.kind else {
            return;
        };
        if let Some(target_local) = assignment_target_local(package, *lhs_expr_id) {
            writes.push(target_local);
        }
    });

    let Some(ret_write_idx) = writes.iter().position(|local| *local == ret_val_var_id) else {
        return false;
    };
    let Some(flag_write_idx) = writes
        .iter()
        .position(|local| *local == has_returned_var_id)
    else {
        return false;
    };

    assert!(
        ret_write_idx < flag_write_idx,
        "rewritten return path must assign __ret_val before setting __has_returned"
    );

    true
}

fn assert_callable_assign_order(
    package: &Package,
    callable_name: &str,
    ret_val_var_id: LocalVarId,
    has_returned_var_id: LocalVarId,
) {
    let decl = find_callable_decl(package, callable_name);
    let mut writes = Vec::new();
    for_each_expr_in_callable_impl(package, &decl.implementation, &mut |expr_id, _expr| {
        if let Some(target_local) = assignment_target_local(package, expr_id) {
            writes.push(target_local);
        }
    });

    let ret_write_idx = writes
        .iter()
        .position(|local| *local == ret_val_var_id)
        .expect("rewritten return path should assign __ret_val");
    let flag_write_idx = writes
        .iter()
        .position(|local| *local == has_returned_var_id)
        .expect("rewritten return path should assign __has_returned");

    assert!(
        ret_write_idx < flag_write_idx,
        "rewritten return path must assign __ret_val before setting __has_returned"
    );
}

fn expr_calls_named_callable(
    store: &PackageStore,
    package: &Package,
    expr_id: ExprId,
    callable_name: &str,
) -> bool {
    let ExprKind::Call(callee_expr_id, _) = &package.get_expr(expr_id).kind else {
        return false;
    };
    let ExprKind::Var(Res::Item(item_id), _) = &package.get_expr(*callee_expr_id).kind else {
        return false;
    };

    let callee_package = store.get(item_id.package);
    matches!(
        &callee_package.get_item(item_id.item).kind,
        ItemKind::Callable(decl) if decl.name.name.as_ref() == callable_name
    )
}

fn stmt_calls_named_callable(
    store: &PackageStore,
    package: &Package,
    stmt_id: StmtId,
    callable_name: &str,
) -> bool {
    let expr_id = match &package.get_stmt(stmt_id).kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => *expr_id,
        StmtKind::Local(_, _, _) | StmtKind::Item(_) => return false,
    };

    expr_calls_named_callable(store, package, expr_id, callable_name)
}

fn expr_tree_calls_named_callable(
    store: &PackageStore,
    package: &Package,
    expr_id: ExprId,
    callable_name: &str,
) -> bool {
    let mut found = false;
    for_each_expr(package, expr_id, &mut |nested_expr_id, _expr| {
        found |= expr_calls_named_callable(store, package, nested_expr_id, callable_name);
    });
    found
}

fn stmt_tree_calls_named_callable(
    store: &PackageStore,
    package: &Package,
    stmt_id: StmtId,
    callable_name: &str,
) -> bool {
    let expr_id = match &package.get_stmt(stmt_id).kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) | StmtKind::Local(_, _, expr_id) => {
            *expr_id
        }
        StmtKind::Item(_) => return false,
    };

    expr_tree_calls_named_callable(store, package, expr_id, callable_name)
}

/// Short description of an expression for snapshot output.
fn describe_expr(package: &qsc_fir::fir::Package, expr_id: qsc_fir::fir::ExprId) -> String {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::If(cond, then_e, else_opt) => {
            let else_str = match else_opt {
                Some(e) => format!(", else={}", describe_expr(package, *e)),
                None => String::new(),
            };
            format!(
                "If(cond={}, then={}{})",
                describe_expr(package, *cond),
                describe_expr(package, *then_e),
                else_str
            )
        }
        ExprKind::Block(_) => format!("Block[ty={}]", expr.ty),
        ExprKind::Lit(lit) => format!("Lit({lit})"),
        ExprKind::Var(_, _) => format!("Var[ty={}]", expr.ty),
        ExprKind::Call(_, _) => format!("Call[ty={}]", expr.ty),
        ExprKind::Tuple(es) => format!("Tuple(len={})", es.len()),
        ExprKind::Assign(_, _) => "Assign".to_string(),
        ExprKind::While(_, _) => format!("While[ty={}]", expr.ty),
        ExprKind::BinOp(op, _, _) => format!("BinOp({op:?})[ty={}]", expr.ty),
        ExprKind::UnOp(op, _) => format!("UnOp({op:?})[ty={}]", expr.ty),
        _ => crate::test_utils::expr_kind_short(package, expr_id).clone(),
    }
}

fn try_eval_fir_entry(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
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
fn eval_qsharp_original(source: &str) -> Result<qsc_eval::val::Value, String> {
    use qsc_frontend::compile as frontend_compile;
    use qsc_hir::hir::PackageId;
    use qsc_lowerer::map_hir_package_to_fir;
    use qsc_passes::{PackageType, run_core_passes, run_default_passes};

    let mut lowerer = qsc_lowerer::Lowerer::new();
    let mut core = frontend_compile::core();
    run_core_passes(&mut core);
    let fir_store = qsc_fir::fir::PackageStore::new();
    let core_fir = lowerer.lower_package(&core.package, &fir_store);
    let mut hir_store = qsc_frontend::compile::PackageStore::new(core);

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

    let mut fir_store = qsc_fir::fir::PackageStore::new();
    fir_store.insert(map_hir_package_to_fir(PackageId::CORE), core_fir);
    fir_store.insert(map_hir_package_to_fir(std_id), std_fir);
    fir_store.insert(map_hir_package_to_fir(user_hir_id), unit_fir);

    try_eval_fir_entry(&fir_store, map_hir_package_to_fir(user_hir_id))
}

/// Compiles Q# source, runs the full FIR transform pipeline (including
/// `return_unify` and `exec_graph_rebuild`), and evaluates the entry exec
/// graph.
fn eval_qsharp_transformed(source: &str) -> Result<qsc_eval::val::Value, String> {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    try_eval_fir_entry(&store, pkg_id)
}

/// Asserts semantic equivalence of a Q# program before and after the
/// full FIR transform pipeline.
///
/// 1. Compiles the original Q# source (no transforms) and evaluates it to
///    get the expected return value.
/// 2. Compiles and runs the full FIR pipeline (including `return_unify`),
///    then evaluates to get the actual return value.
/// 3. Asserts the two results match (both succeed with equal values, or
///    both fail).
fn check_semantic_equivalence(source: &str) {
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

fn check_idempotency(source: &str) {
    let (mut store, pkg_id) = compile_return_unified(source);

    // Snapshot arena sizes before the second run.
    let before = format!("{:?}", Assigner::from_package(store.get(pkg_id)));

    // Run unify_returns a second time.
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let errors = super::unify_returns(&mut store, pkg_id, &mut assigner);
    assert!(
        errors.is_empty(),
        "second unify_returns pass produced errors: {errors:?}"
    );

    // Snapshot arena sizes after the second run — should be identical.
    let after = format!("{:?}", Assigner::from_package(store.get(pkg_id)));
    assert_eq!(
        before, after,
        "second unify_returns pass allocated new nodes (not idempotent)"
    );
}
