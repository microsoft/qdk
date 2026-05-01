// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Proptest applicability: N/A — exec_graph_rebuild is a structural reconstruction pass whose
// correctness is that rebuilt graphs match the format the original lowerer would produce.
// There is no semantic equivalence observable at the Q# level. Testing requires comparing
// graph node sequences, which is better served by targeted snapshot tests.

use crate::test_utils::{
    PipelineStage, compile_and_run_pipeline_to, expr_kind_short, stmt_kind_short,
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

fn find_callable<'a>(package: &'a qsc_fir::fir::Package, callable_name: &str) -> &'a CallableDecl {
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
        _ => format!("{item_id:?}"),
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
                _ => None,
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
    // KEY TEST: classical tuple eq is now decomposed and the exec graph
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
fn exec_graph_update_index_emits_store() {
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
fn exec_graph_callable_with_adjoint_spec_rebuilds_both() {
    check_exec_graph(
        "operation Foo(q : Qubit) : Unit is Adj { body ... { H(q); } adjoint ... { H(q); } } operation Main() : Unit { use q = Qubit(); Foo(q); Adjoint Foo(q); }",
        &expect![[r#"
            0: Expr(ExprId(10)) [Var]
            1: Store
            2: Expr(ExprId(11)) [Tuple(len=0)]
            3: Expr(ExprId(9)) [Call]
            4: Bind(PatId(2))
            5: Expr(ExprId(13)) [Var]
            6: Store
            7: Expr(ExprId(14)) [Var]
            8: Expr(ExprId(12)) [Call]
            9: Expr(ExprId(22)) [Var]
            10: Expr(ExprId(16)) [UnOp(Functor(Adj))]
            11: Store
            12: Expr(ExprId(18)) [Var]
            13: Expr(ExprId(15)) [Call]
            14: Expr(ExprId(20)) [Var]
            15: Store
            16: Expr(ExprId(21)) [Var]
            17: Expr(ExprId(19)) [Call]
            18: Unit
            19: Ret"#]],
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
fn reachable_cross_package_callables_keep_existing_exec_graphs_while_local_specializations_rebuild()
{
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

    clear_store_callable_exec_graph(&mut store, local_specialization);
    clear_store_callable_exec_graph(&mut store, cross_package_callable);

    assert_eq!(
        callable_body_exec_graph_len(&store, local_specialization),
        0
    );
    assert_eq!(
        callable_body_exec_graph_len(&store, cross_package_callable),
        0
    );

    super::rebuild_exec_graphs(&mut store, pkg_id, &[]);

    assert_eq!(
        format_store_callable_exec_graph(&store, local_specialization, ExecGraphConfig::NoDebug),
        expected_local_graph,
        "reachable local specialization should be rebuilt"
    );
    assert_eq!(
        callable_body_exec_graph_len(&store, cross_package_callable),
        0,
        "reachable cross-package callable should not be rebuilt"
    );
}

#[test]
fn exec_graph_rebuild_preserves_invariants() {
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
}

#[test]
#[should_panic(expected = "Struct expressions should have been eliminated by udt_erase")]
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
    super::rebuild_exec_graphs(&mut store, pkg_id, &[]);
}
