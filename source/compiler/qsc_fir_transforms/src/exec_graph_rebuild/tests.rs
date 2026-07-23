// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Proptest applicability: N/A — exec_graph_rebuild is a structural reconstruction pass whose
// correctness is that rebuilt graphs match the format the original lowerer would produce.
// There is no semantic equivalence observable at the Q# level. Testing requires comparing
// graph node sequences, which is better served by targeted snapshot tests.

use crate::test_utils::{
    PipelineStage, assert_panics_with, assert_pipeline_succeeded, compile_and_run_pipeline_to,
    expr_kind_short, find_callable, stmt_kind_short,
};
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_fir::fir::{
    CallableDecl, CallableImpl, ExecGraphConfig, ExecGraphDebugNode, ExecGraphNode, ExprId, Field,
    ItemKind, LocalVarId, PackageLookup, PatId, PatKind, Res, StoreItemId,
};
use rustc_hash::FxHashMap;

#[derive(Clone, Copy)]
enum CallableSpecKind {
    Body,
    Adj,
    Ctl,
    CtlAdj,
    SimulatableIntrinsic,
}

/// Formats the body spec exec graph of the entry callable as a string for
/// snapshot testing. Each node is printed on its own line with its index.
fn format_callable_exec_graph(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    config: ExecGraphConfig,
) -> String {
    let package = store.get(pkg_id);
    let reachable = crate::reachability::collect_reachable_from_entry(store, pkg_id);

    // Find the entry callable (the one in our package).
    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind
            && decl.name.name.as_ref() == "Main"
            && let CallableImpl::Spec(spec) = &decl.implementation
        {
            let graph = spec.body.exec_graph.clone().select(config);
            return graph
                .iter()
                .enumerate()
                .map(|(i, node)| match node {
                    ExecGraphNode::Expr(expr_id) => {
                        let label = expr_kind_short(package, *expr_id);
                        format!("{i}: Expr({expr_id:?}) [{label}]")
                    }
                    ExecGraphNode::Debug(ExecGraphDebugNode::Stmt(stmt_id)) => {
                        let label = stmt_kind_short(package, *stmt_id);
                        format!("{i}: Debug(Stmt({stmt_id:?})) [{label}]")
                    }
                    _ => format!("{i}: {node:?}"),
                })
                .collect::<Vec<_>>()
                .join("\n");
        }
    }
    panic!("Main callable not found");
}

fn collect_pat_names(
    package: &qsc_fir::fir::Package,
    pat_id: PatId,
    names: &mut FxHashMap<LocalVarId, String>,
) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            names.insert(ident.id, ident.name.to_string());
        }
        PatKind::Tuple(sub_pats) => {
            for &sub_pat_id in sub_pats {
                collect_pat_names(package, sub_pat_id, names);
            }
        }
        PatKind::Discard => {}
    }
}

fn callable_local_names(
    package: &qsc_fir::fir::Package,
    callable: &CallableDecl,
) -> FxHashMap<LocalVarId, String> {
    let mut names = FxHashMap::default();
    collect_pat_names(package, callable.input, &mut names);

    match &callable.implementation {
        CallableImpl::Spec(spec_impl) => {
            for spec in std::iter::once(&spec_impl.body)
                .chain(spec_impl.adj.iter())
                .chain(spec_impl.ctl.iter())
                .chain(spec_impl.ctl_adj.iter())
            {
                if let Some(input_pat) = spec.input {
                    collect_pat_names(package, input_pat, &mut names);
                }
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            if let Some(input_pat) = spec.input {
                collect_pat_names(package, input_pat, &mut names);
            }
        }
        CallableImpl::Intrinsic => {}
    }

    names
}

fn bind_label(package: &qsc_fir::fir::Package, pat_id: PatId) -> String {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => format!("Bind({})", ident.name),
        PatKind::Tuple(_) => "Bind(tuple)".to_string(),
        PatKind::Discard => "Bind(_)".to_string(),
    }
}

fn item_name(store: &qsc_fir::fir::PackageStore, item_id: &qsc_fir::fir::ItemId) -> String {
    let package = store.get(item_id.package);
    match &package.get_item(item_id.item).kind {
        ItemKind::Callable(decl) => decl.name.name.to_string(),
        ItemKind::Ty(..) => format!("{item_id:?}"),
    }
}

fn semantic_expr_label(
    store: &qsc_fir::fir::PackageStore,
    package: &qsc_fir::fir::Package,
    local_names: &FxHashMap<LocalVarId, String>,
    expr_id: ExprId,
) -> String {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        qsc_fir::fir::ExprKind::Field(record_id, Field::Path(path)) => {
            let mut formatted = semantic_expr_label(store, package, local_names, *record_id);
            for index in &path.indices {
                formatted.push('.');
                formatted.push_str(&index.to_string());
            }
            formatted
        }
        qsc_fir::fir::ExprKind::Lit(lit) => format!("Lit({lit:?})"),
        qsc_fir::fir::ExprKind::Tuple(items) => format!("Tuple(len={})", items.len()),
        qsc_fir::fir::ExprKind::UnOp(op, operand_id) => format!(
            "{op:?}({})",
            semantic_expr_label(store, package, local_names, *operand_id)
        ),
        qsc_fir::fir::ExprKind::Var(Res::Item(item_id), _) => item_name(store, item_id),
        qsc_fir::fir::ExprKind::Var(Res::Local(local_id), _) => {
            local_names.get(local_id).map_or_else(
                || format!("Var({local_id:?})"),
                |name| format!("Var({name})"),
            )
        }
        _ => expr_kind_short(package, expr_id),
    }
}

fn format_callable_spec_exec_graph(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
    spec_kind: CallableSpecKind,
) -> String {
    let package = store.get(pkg_id);
    let callable = find_callable(package, callable_name);
    let local_names = callable_local_names(package, callable);
    let spec = match (spec_kind, &callable.implementation) {
        (CallableSpecKind::Body, CallableImpl::Spec(spec_impl)) => &spec_impl.body,
        (CallableSpecKind::Adj, CallableImpl::Spec(spec_impl)) => {
            spec_impl.adj.as_ref().expect("adjoint spec should exist")
        }
        (CallableSpecKind::Ctl, CallableImpl::Spec(spec_impl)) => spec_impl
            .ctl
            .as_ref()
            .expect("controlled spec should exist"),
        (CallableSpecKind::CtlAdj, CallableImpl::Spec(spec_impl)) => spec_impl
            .ctl_adj
            .as_ref()
            .expect("controlled adjoint spec should exist"),
        (CallableSpecKind::SimulatableIntrinsic, CallableImpl::SimulatableIntrinsic(spec)) => spec,
        _ => panic!("requested spec kind is not present on '{callable_name}'"),
    };

    format_exec_graph_nodes(
        store,
        package,
        &local_names,
        spec.exec_graph.select_ref(ExecGraphConfig::NoDebug),
    )
}

fn format_exec_graph_nodes(
    store: &qsc_fir::fir::PackageStore,
    package: &qsc_fir::fir::Package,
    local_names: &FxHashMap<LocalVarId, String>,
    graph: &[ExecGraphNode],
) -> String {
    graph
        .iter()
        .enumerate()
        .map(|(index, node)| match node {
            ExecGraphNode::Bind(pat_id) => format!("{index}: {}", bind_label(package, *pat_id)),
            ExecGraphNode::Expr(expr_id) => format!(
                "{index}: {}",
                semantic_expr_label(store, package, local_names, *expr_id)
            ),
            ExecGraphNode::Jump(target) => format!("{index}: Jump({target})"),
            ExecGraphNode::JumpIf(target) => format!("{index}: JumpIf({target})"),
            ExecGraphNode::JumpIfNot(target) => format!("{index}: JumpIfNot({target})"),
            ExecGraphNode::Ret => format!("{index}: Ret"),
            ExecGraphNode::Store => format!("{index}: Store"),
            ExecGraphNode::Unit => format!("{index}: Unit"),
            ExecGraphNode::Debug(_) => {
                unreachable!("NoDebug exec graph should not contain debug nodes")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_store_callable_exec_graph(
    store: &qsc_fir::fir::PackageStore,
    store_item_id: StoreItemId,
    config: ExecGraphConfig,
) -> String {
    let package = store.get(store_item_id.package);
    let item = package.get_item(store_item_id.item);
    let ItemKind::Callable(decl) = &item.kind else {
        panic!("reachable item should be callable");
    };
    let local_names = callable_local_names(package, decl);
    let spec = match &decl.implementation {
        CallableImpl::Spec(spec_impl) => &spec_impl.body,
        CallableImpl::SimulatableIntrinsic(spec) => spec,
        CallableImpl::Intrinsic => panic!("callable '{}' should have a body", decl.name.name),
    };

    format_exec_graph_nodes(
        store,
        package,
        &local_names,
        spec.exec_graph.select_ref(config),
    )
}

fn clear_store_callable_exec_graph(
    store: &mut qsc_fir::fir::PackageStore,
    store_item_id: StoreItemId,
) {
    let package = store.get_mut(store_item_id.package);
    let item = package
        .items
        .get_mut(store_item_id.item)
        .expect("reachable item should exist");
    let ItemKind::Callable(decl) = &mut item.kind else {
        panic!("reachable item should be callable");
    };

    match &mut decl.implementation {
        CallableImpl::Spec(spec_impl) => spec_impl.body.exec_graph = Default::default(),
        CallableImpl::SimulatableIntrinsic(spec) => spec.exec_graph = Default::default(),
        CallableImpl::Intrinsic => panic!("callable '{}' should have a body", decl.name.name),
    }
}

fn callable_body_exec_graph_len(
    store: &qsc_fir::fir::PackageStore,
    store_item_id: StoreItemId,
) -> usize {
    let package = store.get(store_item_id.package);
    let item = package.get_item(store_item_id.item);
    let ItemKind::Callable(decl) = &item.kind else {
        panic!("reachable item should be callable");
    };

    match &decl.implementation {
        CallableImpl::Spec(spec_impl) => spec_impl
            .body
            .exec_graph
            .select_ref(ExecGraphConfig::NoDebug)
            .len(),
        CallableImpl::SimulatableIntrinsic(spec) => {
            spec.exec_graph.select_ref(ExecGraphConfig::NoDebug).len()
        }
        CallableImpl::Intrinsic => panic!("callable '{}' should have a body", decl.name.name),
    }
}

fn assert_callable_exec_graph_is_empty(
    store: &qsc_fir::fir::PackageStore,
    store_item_id: StoreItemId,
    message: &str,
) {
    assert_eq!(
        callable_body_exec_graph_len(store, store_item_id),
        0,
        "{message}"
    );
}

fn assert_rebuild_restores_reachable_callables(
    store: &mut qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    local_callable: StoreItemId,
    cross_package_callable: StoreItemId,
    expected_local_graph: &str,
    expected_cross_graph: &str,
) {
    clear_store_callable_exec_graph(store, local_callable);
    clear_store_callable_exec_graph(store, cross_package_callable);

    assert_callable_exec_graph_is_empty(store, local_callable, "local graph should start cleared");
    assert_callable_exec_graph_is_empty(
        store,
        cross_package_callable,
        "cross-package graph should start cleared",
    );

    super::rebuild_exec_graphs(store, pkg_id, &[]);

    assert_eq!(
        format_store_callable_exec_graph(store, local_callable, ExecGraphConfig::NoDebug),
        expected_local_graph,
        "reachable local specialization should be rebuilt"
    );
    assert_eq!(
        format_store_callable_exec_graph(store, cross_package_callable, ExecGraphConfig::NoDebug),
        expected_cross_graph,
        "reachable cross-package callable should be rebuilt to its original graph",
    );
}

fn reachable_callable_names_with_packages(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) -> Vec<String> {
    let mut names = crate::reachability::collect_reachable_from_entry(store, pkg_id)
        .into_iter()
        .filter_map(|store_item_id| {
            let package = store.get(store_item_id.package);
            let item = package.get_item(store_item_id.item);
            match &item.kind {
                ItemKind::Callable(decl) => Some(format!(
                    "pkg={:?} {}",
                    store_item_id.package, decl.name.name
                )),
                ItemKind::Ty(..) => None,
            }
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn find_reachable_callable_by_name(
    store: &qsc_fir::fir::PackageStore,
    root_pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
    same_package_as_root: bool,
) -> StoreItemId {
    crate::reachability::collect_reachable_from_entry(store, root_pkg_id)
        .into_iter()
        .find(|store_item_id| {
            let package = store.get(store_item_id.package);
            let item = package.get_item(store_item_id.item);
            matches!(
                &item.kind,
                ItemKind::Callable(decl)
                    if decl.name.name.as_ref() == callable_name
                        && (store_item_id.package == root_pkg_id) == same_package_as_root
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "reachable callable '{callable_name}' not found\n{}",
                reachable_callable_names_with_packages(store, root_pkg_id).join("\n")
            )
        })
}

fn assert_external_body_exec_graph_rebuilt(
    store: &qsc_fir::fir::PackageStore,
    external_callable: StoreItemId,
) {
    let package = store.get(external_callable.package);
    let item = package.get_item(external_callable.item);
    let qsc_fir::fir::ItemKind::Callable(decl) = &item.kind else {
        panic!("external item should be callable");
    };
    let qsc_fir::fir::CallableImpl::Spec(spec_impl) = &decl.implementation else {
        panic!("external callable should have a body spec");
    };
    // Every live expression in the rebuilt external body must carry a non-empty
    // exec-graph range — the whole-closure rebuild populates ranges for the
    // (now decomposed) foreign body, not just entry-package specs.
    let mut saw_expr = false;
    crate::walk_utils::for_each_expr_in_block(
        package,
        spec_impl.body.block,
        &mut |expr_id, expr| {
            saw_expr = true;
            let range = &expr.exec_graph_range;
            assert!(
                range.start != range.end,
                "external body Expr {expr_id} has an un-rebuilt (empty) exec graph range"
            );
        },
    );
    assert!(
        saw_expr,
        "external body should contain at least one expression"
    );
}

/// Compiles Q# source through the pipeline (including exec graph rebuild)
/// and asserts the Main callable's body exec graph (`NoDebug` config) matches.
fn check_exec_graph(source: &str, expect: &Expect) {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ExecGraphRebuild);
    let result = format_callable_exec_graph(&store, pkg_id, ExecGraphConfig::NoDebug);
    expect.assert_eq(&result);
}

fn check_callable_spec_exec_graph(
    source: &str,
    callable_name: &str,
    spec_kind: CallableSpecKind,
    expect: &Expect,
) {
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ExecGraphRebuild);
    let result = format_callable_spec_exec_graph(&store, pkg_id, callable_name, spec_kind);
    expect.assert_eq(&result);
}

#[test]
fn literal_int_emits_single_expr_node() {
    check_exec_graph(
        "function Main() : Int { 42 }",
        &expect![[r#"
            0: Expr(ExprId(3)) [Lit(Int(42))]
            1: Ret"#]],
    );
}

#[test]
fn binop_add_evaluates_operands_then_expr() {
    check_exec_graph(
        "function Main() : Int { 1 + 2 }",
        &expect![[r#"
            0: Expr(ExprId(4)) [Lit(Int(1))]
            1: Store
            2: Expr(ExprId(5)) [Lit(Int(2))]
            3: Expr(ExprId(3)) [BinOp(Add)]
            4: Ret"#]],
    );
}

#[test]
fn tuple_construction_emits_store_per_element() {
    check_exec_graph(
        "function Main() : (Int, Int) { (1, 2) }",
        &expect![[r#"
            0: Expr(ExprId(4)) [Lit(Int(1))]
            1: Store
            2: Expr(ExprId(5)) [Lit(Int(2))]
            3: Store
            4: Expr(ExprId(3)) [Tuple(len=2)]
            5: Ret"#]],
    );
}

#[test]
fn if_else_emits_jump_if_not_with_both_branches() {
    check_exec_graph(
        "function Main() : Int { if true { 1 } else { 2 } }",
        &expect![[r#"
            0: Expr(ExprId(4)) [Lit(Bool(true))]
            1: JumpIfNot(4)
            2: Expr(ExprId(6)) [Lit(Int(1))]
            3: Jump(5)
            4: Expr(ExprId(8)) [Lit(Int(2))]
            5: Ret"#]],
    );
}

#[test]
fn while_loop_emits_jump_back_to_condition() {
    check_exec_graph(
        "function Main() : Unit {
            mutable i = 0;
            while i < 3 {
                i += 1;
            }
        }",
        &expect![[r#"
            0: Expr(ExprId(3)) [Lit(Int(0))]
            1: Bind(PatId(1))
            2: Expr(ExprId(6)) [Var]
            3: Store
            4: Expr(ExprId(7)) [Lit(Int(3))]
            5: Expr(ExprId(5)) [BinOp(Lt)]
            6: JumpIfNot(14)
            7: Expr(ExprId(9)) [Var]
            8: Store
            9: Expr(ExprId(10)) [Lit(Int(1))]
            10: Expr(ExprId(8)) [AssignOp(Add)]
            11: Unit
            12: Unit
            13: Jump(2)
            14: Unit
            15: Ret"#]],
    );
}

#[test]
fn andl_emits_jump_if_not_for_short_circuit() {
    check_exec_graph(
        "function Main() : Bool { true and false }",
        &expect![[r#"
            0: Expr(ExprId(4)) [Lit(Bool(true))]
            1: JumpIfNot(3)
            2: Expr(ExprId(5)) [Lit(Bool(false))]
            3: Ret"#]],
    );
}

#[test]
fn let_binding_stores_value_then_evaluates_body() {
    check_exec_graph(
        "function Main() : Int { let x = 42; x }",
        &expect![[r#"
            0: Expr(ExprId(3)) [Lit(Int(42))]
            1: Bind(PatId(1))
            2: Expr(ExprId(4)) [Var]
            3: Ret"#]],
    );
}

#[test]
fn tuple_eq_lowered_to_element_wise_andl_chain() {
    // Classical tuple eq is now decomposed and the exec graph
    // must contain the short-circuit AndL pattern instead of a single BinOp.
    check_exec_graph(
        "function Main() : Bool { (1, 2) == (1, 2) }",
        &expect![[r#"
            0: Expr(ExprId(5)) [Lit(Int(1))]
            1: Store
            2: Expr(ExprId(8)) [Lit(Int(1))]
            3: Expr(ExprId(10)) [BinOp(Eq)]
            4: JumpIfNot(9)
            5: Expr(ExprId(6)) [Lit(Int(2))]
            6: Store
            7: Expr(ExprId(9)) [Lit(Int(2))]
            8: Expr(ExprId(11)) [BinOp(Eq)]
            9: Ret"#]],
    );
}

#[test]
fn nested_blocks_flatten_to_sequential_nodes() {
    check_exec_graph(
        "function Main() : Int { let x = { let y = 1; y + 1 }; x }",
        &expect![[r#"
            0: Expr(ExprId(4)) [Lit(Int(1))]
            1: Bind(PatId(2))
            2: Expr(ExprId(6)) [Var]
            3: Store
            4: Expr(ExprId(7)) [Lit(Int(1))]
            5: Expr(ExprId(5)) [BinOp(Add)]
            6: Bind(PatId(1))
            7: Expr(ExprId(8)) [Var]
            8: Ret"#]],
    );
}

#[test]
fn orl_short_circuit_emits_jump_if() {
    check_exec_graph(
        "function Main() : Bool { true or false }",
        &expect![[r#"
            0: Expr(ExprId(4)) [Lit(Bool(true))]
            1: JumpIf(3)
            2: Expr(ExprId(5)) [Lit(Bool(false))]
            3: Ret"#]],
    );
}

#[test]
fn return_expression_emits_ret_node() {
    // After return unification, `return 42;` is simplified to a trailing `42`,
    // so the exec graph only contains the expression and the final Ret.
    check_exec_graph(
        "function Main() : Int { return 42; }",
        &expect![[r#"
            0: Expr(ExprId(4)) [Lit(Int(42))]
            1: Ret"#]],
    );
}

#[test]
fn fail_expression_evaluates_message_then_expr() {
    check_exec_graph(
        "function Main() : Unit { fail \"error\"; }",
        &expect![[r#"
            0: Expr(ExprId(4)) [String(parts=1)]
            1: Expr(ExprId(3)) [Fail]
            2: Unit
            3: Ret"#]],
    );
}

#[test]
fn assign_index_emits_store_and_expr_unit() {
    check_exec_graph(
        "function Main() : Int[] { mutable arr = [1, 2, 3]; set arr w/= 0 <- 42; arr }",
        &expect![[r#"
            0: Expr(ExprId(3)) [ArrayLit(len=3)]
            1: Bind(PatId(1))
            2: Expr(ExprId(8)) [Lit(Int(0))]
            3: Store
            4: Expr(ExprId(9)) [Lit(Int(42))]
            5: Expr(ExprId(7)) [AssignIndex]
            6: Unit
            7: Expr(ExprId(11)) [Var]
            8: Ret"#]],
    );
}

#[test]
fn exec_graph_array_repeat_emits_store_pattern() {
    check_exec_graph(
        "function Main() : Int[] { let arr = [0, size = 3]; arr }",
        &expect![[r#"
            0: Expr(ExprId(4)) [Lit(Int(0))]
            1: Store
            2: Expr(ExprId(5)) [Lit(Int(3))]
            3: Expr(ExprId(3)) [ArrayRepeat]
            4: Bind(PatId(1))
            5: Expr(ExprId(6)) [Var]
            6: Ret"#]],
    );
}

#[test]
fn exec_graph_range_expression() {
    check_exec_graph(
        "function Main() : Range { 0..10 }",
        &expect![[r#"
            0: Expr(ExprId(4)) [Lit(Int(0))]
            1: Store
            2: Expr(ExprId(5)) [Lit(Int(10))]
            3: Expr(ExprId(3)) [Range]
            4: Ret"#]],
    );
}

#[test]
fn exec_graph_string_interpolation() {
    check_exec_graph(
        r#"function Main() : String { let x = 42; $"value = {x}" }"#,
        &expect![[r#"
            0: Expr(ExprId(3)) [Lit(Int(42))]
            1: Bind(PatId(1))
            2: Expr(ExprId(5)) [Var]
            3: Store
            4: Expr(ExprId(4)) [String(parts=2)]
            5: Ret"#]],
    );
}

#[test]
fn exec_graph_unary_not() {
    check_exec_graph(
        "function Main() : Bool { not true }",
        &expect![[r#"
            0: Expr(ExprId(4)) [Lit(Bool(true))]
            1: Expr(ExprId(3)) [UnOp(NotL)]
            2: Ret"#]],
    );
}

#[test]
fn empty_unit_body_rebuilds_to_unit_ret() {
    // A function with no statements has nothing to evaluate, so the rebuilt
    // graph is just the implicit Unit value followed by Ret.
    check_exec_graph(
        "operation Main() : Unit {}",
        &expect![[r#"
            0: Unit
            1: Ret"#]],
    );
}

#[test]
fn divergent_only_body_rebuilds_fail_without_trailing_value() {
    // A non-Unit body whose sole statement diverges (`fail`) never yields a
    // value; the rebuilt graph evaluates the message and the Fail expr, then
    // terminates with Ret and no trailing value node.
    check_exec_graph(
        "function Main() : Int { fail \"boom\" }",
        &expect![[r#"
            0: Expr(ExprId(4)) [String(parts=1)]
            1: Expr(ExprId(3)) [Fail]
            2: Ret"#]],
    );
}

#[test]
fn nested_if_within_while_rebuilds_nested_control_flow() {
    // Deeply nested control flow (an if/else nested inside a while loop) must
    // reconstruct with correctly interleaved jump targets for both the loop
    // back-edge and the inner branch.
    check_exec_graph(
        "function Main() : Unit {
            mutable i = 0;
            while i < 3 {
                if i == 1 {
                    i += 10;
                } else {
                    i += 1;
                }
            }
        }",
        &expect![[r#"
            0: Expr(ExprId(3)) [Lit(Int(0))]
            1: Bind(PatId(1))
            2: Expr(ExprId(6)) [Var]
            3: Store
            4: Expr(ExprId(7)) [Lit(Int(3))]
            5: Expr(ExprId(5)) [BinOp(Lt)]
            6: JumpIfNot(26)
            7: Expr(ExprId(10)) [Var]
            8: Store
            9: Expr(ExprId(11)) [Lit(Int(1))]
            10: Expr(ExprId(9)) [BinOp(Eq)]
            11: JumpIfNot(19)
            12: Expr(ExprId(14)) [Var]
            13: Store
            14: Expr(ExprId(15)) [Lit(Int(10))]
            15: Expr(ExprId(13)) [AssignOp(Add)]
            16: Unit
            17: Unit
            18: Jump(25)
            19: Expr(ExprId(18)) [Var]
            20: Store
            21: Expr(ExprId(19)) [Lit(Int(1))]
            22: Expr(ExprId(17)) [AssignOp(Add)]
            23: Unit
            24: Unit
            25: Jump(2)
            26: Unit
            27: Ret"#]],
    );
}

#[test]
fn exec_graph_callable_with_adjoint_spec_rebuilds_body_and_adj_independently() {
    let source = "operation Foo(q : Qubit) : Unit is Adj { body ... { H(q); } adjoint ... { X(q); } } operation Main() : Unit { use q = Qubit(); Foo(q); Adjoint Foo(q); }";
    check_callable_spec_exec_graph(
        source,
        "Foo",
        CallableSpecKind::Body,
        &expect![[r#"
            0: H
            1: Store
            2: Var(q)
            3: Call
            4: Unit
            5: Ret"#]],
    );
    check_callable_spec_exec_graph(
        source,
        "Foo",
        CallableSpecKind::Adj,
        &expect![[r#"
            0: X
            1: Store
            2: Var(q)
            3: Call
            4: Unit
            5: Ret"#]],
    );
}

#[test]
fn controlled_spec_exec_graph_rebuilds_semantic_order() {
    check_callable_spec_exec_graph(
        "operation Foo(q : Qubit) : Unit is Ctl {
            body ... { X(q); }
            controlled (ctls, ...) { Controlled X(ctls, q); }
        }
        operation Main() : Unit {
            use ctl = Qubit();
            use q = Qubit();
            Controlled Foo([ctl], q);
        }",
        "Foo",
        CallableSpecKind::Ctl,
        &expect![[r#"
            0: X
            1: Functor(Ctl)(X)
            2: Store
            3: Var(ctls)
            4: Store
            5: Var(q)
            6: Store
            7: Tuple(len=2)
            8: Call
            9: Unit
            10: Ret"#]],
    );
}

#[test]
fn controlled_adjoint_spec_exec_graph_rebuilds_semantic_order() {
    check_callable_spec_exec_graph(
        "operation Foo(q : Qubit) : Unit is Adj + Ctl {
            body ... { S(q); }
            adjoint ... { Adjoint S(q); }
            controlled (ctls, ...) { Controlled S(ctls, q); }
            controlled adjoint (ctls, ...) { Controlled Adjoint S(ctls, q); }
        }
        operation Main() : Unit {
            use ctl = Qubit();
            use q = Qubit();
            Controlled Adjoint Foo([ctl], q);
        }",
        "Foo",
        CallableSpecKind::CtlAdj,
        &expect![[r#"
            0: S
            1: Functor(Adj)(S)
            2: Functor(Ctl)(Functor(Adj)(S))
            3: Store
            4: Var(ctls)
            5: Store
            6: Var(q)
            7: Store
            8: Tuple(len=2)
            9: Call
            10: Unit
            11: Ret"#]],
    );
}

#[test]
fn simulatable_intrinsic_spec_exec_graph_rebuilds_semantic_order() {
    check_callable_spec_exec_graph(
        "@SimulatableIntrinsic()
        operation MyMeasurement(q : Qubit) : Result {
            H(q);
            M(q)
        }
        @EntryPoint()
        operation Main() : Result {
            use q = Qubit();
            MyMeasurement(q)
        }",
        "MyMeasurement",
        CallableSpecKind::SimulatableIntrinsic,
        &expect![[r#"
            0: H
            1: Store
            2: Var(q)
            3: Call
            4: M
            5: Store
            6: Var(q)
            7: Call
            8: Ret"#]],
    );
}

#[test]
fn exec_graph_entry_expression_rebuilt_correctly() {
    check_exec_graph(
        "function Main() : Int { let x = 1 + 2; let y = x * 3; y }",
        &expect![[r#"
            0: Expr(ExprId(4)) [Lit(Int(1))]
            1: Store
            2: Expr(ExprId(5)) [Lit(Int(2))]
            3: Expr(ExprId(3)) [BinOp(Add)]
            4: Bind(PatId(1))
            5: Expr(ExprId(7)) [Var]
            6: Store
            7: Expr(ExprId(8)) [Lit(Int(3))]
            8: Expr(ExprId(6)) [BinOp(Mul)]
            9: Bind(PatId(2))
            10: Expr(ExprId(9)) [Var]
            11: Ret"#]],
    );
}

#[test]
fn exec_graph_rebuild_is_idempotent() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(
        "function Main() : Int { let x = 1 + 2; x }",
        PipelineStage::ExecGraphRebuild,
    );
    let first = format_callable_exec_graph(&store, pkg_id, ExecGraphConfig::NoDebug);

    // Run rebuild a second time — the result must be identical.
    super::rebuild_exec_graphs(&mut store, pkg_id, &[]);
    let second = format_callable_exec_graph(&store, pkg_id, ExecGraphConfig::NoDebug);

    assert_eq!(first, second, "exec graph rebuild is not idempotent");
}

#[test]
fn reachable_cross_package_callables_are_rebuilt_along_with_local_specializations() {
    let source = r#"
        open Std.Arrays;
        open Std.Math;

        @EntryPoint()
        operation Main() : Unit {
            let arr = [-1, 2, -3];
            let _ = Mapped(AbsI, arr);
        }
    "#;

    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ExecGraphRebuild);

    let local_specialization =
        find_reachable_callable_by_name(&store, pkg_id, "Mapped<Int, Int>{AbsI}", true);
    let cross_package_callable = find_reachable_callable_by_name(&store, pkg_id, "AbsI", false);

    assert_eq!(local_specialization.package, pkg_id);
    assert_ne!(cross_package_callable.package, pkg_id);

    let expected_local_graph =
        format_store_callable_exec_graph(&store, local_specialization, ExecGraphConfig::NoDebug);
    let expected_cross_graph =
        format_store_callable_exec_graph(&store, cross_package_callable, ExecGraphConfig::NoDebug);

    assert!(
        !expected_local_graph.is_empty(),
        "local specialization should have a rebuilt exec graph"
    );
    assert!(
        !expected_cross_graph.is_empty(),
        "reachable cross-package callable should start with a lowered exec graph"
    );

    // Whole-closure rebuild: both the local specialization and the reachable
    // cross-package callable are rebuilt, and rebuilding reproduces each spec's
    // original graph byte-for-byte.
    assert_rebuild_restores_reachable_callables(
        &mut store,
        pkg_id,
        local_specialization,
        cross_package_callable,
        &expected_local_graph,
        &expected_cross_graph,
    );
}

#[test]
fn external_udt_copy_update_exec_graph_rebuilds_mutated_external_spec() {
    let lib_source = indoc! {"
        namespace TestLib {
            struct Pair { Fst: Int, Snd: Int }
            function MakeUpdated() : Pair {
                let p = new Pair { Fst = 1, Snd = 2 };
                new Pair { ...p, Fst = 42 }
            }
            export Pair, MakeUpdated;
        }
    "};
    let user_source = indoc! {"
        import TestLib.*;

        @EntryPoint()
        function Main() : (Int, Int) {
            let r = MakeUpdated();
            (r.Fst, r.Snd)
        }
    "};

    let (mut store, pkg_id) =
        crate::test_utils::compile_to_fir_with_library(lib_source, user_source);
    let result = crate::run_pipeline_to_with_diagnostics(
        &mut store,
        pkg_id,
        PipelineStage::ExecGraphRebuild,
        &[],
    );

    assert_pipeline_succeeded("external UDT copy-update pipeline", &result);
    let external_callable = crate::test_utils::find_library_callable(&store, pkg_id, "MakeUpdated");
    let graph = format_store_callable_exec_graph(
        &store,
        external_callable,
        qsc_fir::fir::ExecGraphConfig::NoDebug,
    );
    assert!(
        graph.contains("Tuple(len=2)"),
        "external copy-update exec graph should include the erased update tuple:\n{graph}"
    );
    // The external library body is transformed cross-package: UDT erasure
    // lowers the copy-update into a tuple read, and tuple-decompose then
    // scalar-replaces the untouched-field projection, so the rebuilt graph
    // reads it through a decomposed local rather than a `.1` field path.
    assert!(
        graph.contains("Var(LocalVarId"),
        "external copy-update exec graph should read the untouched field through a \
         decomposed local after tuple-decompose:\n{graph}"
    );
    assert_external_body_exec_graph_rebuilt(&store, external_callable);
}

#[test]
fn exec_graph_rebuild_passes_post_all_invariant() {
    let source = indoc! {"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                H(q);
                Reset(q);
            }
        }
    "};
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ExecGraphRebuild);
    crate::invariants::check(&store, pkg_id, crate::invariants::InvariantLevel::PostAll);

    // Pin the actual rebuilt graph shape for `Main`, not just invariant
    // validity: the allocate / H / Reset / release sequence must reconstruct
    // to this exact node ordering.
    let main_local = store
        .get(pkg_id)
        .items
        .iter()
        .find_map(|(item_id, item)| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Main" => Some(item_id),
            _ => None,
        })
        .expect("Main callable should exist");
    let main_store_id = StoreItemId {
        package: pkg_id,
        item: main_local,
    };
    let graph = format_store_callable_exec_graph(&store, main_store_id, ExecGraphConfig::NoDebug);
    expect![[r#"
        0: __quantum__rt__qubit_allocate
        1: Store
        2: Tuple(len=0)
        3: Call
        4: Bind(q)
        5: H
        6: Store
        7: Var(LocalVarId(1))
        8: Call
        9: Reset
        10: Store
        11: Var(LocalVarId(1))
        12: Call
        13: __quantum__rt__qubit_release
        14: Store
        15: Var(LocalVarId(1))
        16: Call
        17: Unit
        18: Ret"#]]
    .assert_eq(&graph);
}

#[test]
fn exec_graph_rebuild_rejects_struct_expressions() {
    // Feed FIR that still contains ExprKind::Struct (pipeline stopped
    // before udt_erase) to exec_graph_rebuild. The pass should panic
    // because struct expressions must be erased before exec graph rebuild.
    let source = indoc! {"
        namespace Test {
            struct Pair { X : Int, Y : Int }
            @EntryPoint()
            function Main() : (Int, Int) {
                let p = new Pair { X = 1, Y = 2 };
                (p.X, p.Y)
            }
        }
    "};
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Defunc);
    assert_panics_with(
        "Struct expressions should have been eliminated by udt_erase",
        || {
            super::rebuild_exec_graphs(&mut store, pkg_id, &[]);
        },
    );
}

#[test]
fn pinned_item_rebuilt_in_exec_graph() {
    // After full pipeline with pinned items, verify the pinned callable has
    // the expected rebuilt exec graph nodes — proving it participates in exec graph rebuild.
    use crate::test_utils::compile_to_fir;

    let (mut store, pkg_id) = compile_to_fir(indoc! {"
        namespace Test {
            @EntryPoint()
            operation Main() : Int { 42 }
            // Unreachable from entry but will be pinned
            operation Pinned() : Int { 99 }
        }
    "});
    let package = store.get(pkg_id);
    let pinned_local = package
        .items
        .iter()
        .find_map(|(item_id, item)| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Pinned" => Some(item_id),
            _ => None,
        })
        .expect("Pinned callable should exist");
    let pinned_store_id = StoreItemId {
        package: pkg_id,
        item: pinned_local,
    };

    let result = crate::run_pipeline_to_with_diagnostics(
        &mut store,
        pkg_id,
        PipelineStage::ExecGraphRebuild,
        &[pinned_store_id],
    );
    assert!(result.is_success(), "pipeline errors: {:?}", result.errors);

    let graph = format_store_callable_exec_graph(&store, pinned_store_id, ExecGraphConfig::NoDebug);
    expect![[r#"
        0: Lit(Int(99))
        1: Ret"#]]
    .assert_eq(&graph);
}

#[test]
fn residual_hole_in_rebuilt_body_panics() {
    // exec_graph_rebuild defensively panics on a residual `ExprKind::Hole`,
    // which is not valid post-defunctionalization residue. (Residual closures,
    // by contrast, are tolerated and rebuilt into a single `Expr` node.) Inject
    // a residual `Hole` into the reachable `Main` body to pin that defensive
    // arm.
    use crate::test_utils::compile_to_fir;

    let (mut store, pkg_id) = compile_to_fir("function Main() : Int { 42 }");

    // Locate the tail expression of `Main`'s body and overwrite it with a
    // forbidden `Hole`, simulating a defunctionalization defect that left a
    // residual variant behind.
    let tail_expr_id = {
        let package = store.get(pkg_id);
        let main = find_callable(package, "Main");
        let CallableImpl::Spec(spec) = &main.implementation else {
            panic!("Main should have a spec body");
        };
        let block = package.get_block(spec.body.block);
        let &tail_stmt_id = block.stmts.last().expect("Main body has a statement");
        match &package.get_stmt(tail_stmt_id).kind {
            qsc_fir::fir::StmtKind::Expr(e) | qsc_fir::fir::StmtKind::Semi(e) => *e,
            other => panic!("expected a tail expression statement, found {other:?}"),
        }
    };
    store
        .get_mut(pkg_id)
        .exprs
        .get_mut(tail_expr_id)
        .expect("tail expr should exist")
        .kind = qsc_fir::fir::ExprKind::Hole;

    // Rebuilding the reachable `Main` body must hit the defensive panic.
    assert_panics_with(
        "Hole expressions should have been eliminated by post_defunc",
        || {
            super::rebuild_exec_graphs(&mut store, pkg_id, &[]);
        },
    );
}
