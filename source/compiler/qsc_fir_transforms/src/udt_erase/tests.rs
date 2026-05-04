// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};
use indoc::indoc;

use super::*;
use qsc_data_structures::index_map::IndexMap;
use qsc_data_structures::span::Span;
use qsc_fir::fir::{
    Block, CallableDecl, CallableImpl, CallableKind, ExecGraph, Expr, ExprKind, Field, FieldAssign,
    FieldPath, Ident, Item, ItemId, LocalVarId, NodeId, PackageLookup, Pat, PatKind, SpecDecl,
    SpecImpl, Stmt, StmtId, StmtKind, Visibility,
};
use qsc_fir::ty::{FunctorSet, FunctorSetValue, Prim, Udt, UdtDef, UdtDefKind, UdtField};
use rustc_hash::FxHashMap;
use std::rc::Rc;

use crate::EMPTY_EXEC_RANGE;

fn default_span() -> Span {
    Span::default()
}

/// Creates a minimal UDT type item (like `newtype Pair = (Int, Double)`).
fn make_udt_item(item_id: LocalItemId, fields: Vec<(Option<Rc<str>>, Ty)>) -> Item {
    let def = if fields.len() == 1 {
        UdtDef {
            span: default_span(),
            kind: UdtDefKind::Field(UdtField {
                name_span: None,
                name: fields[0].0.clone(),
                ty: fields[0].1.clone(),
            }),
        }
    } else {
        UdtDef {
            span: default_span(),
            kind: UdtDefKind::Tuple(
                fields
                    .into_iter()
                    .map(|(name, ty)| UdtDef {
                        span: default_span(),
                        kind: UdtDefKind::Field(UdtField {
                            name_span: None,
                            name,
                            ty,
                        }),
                    })
                    .collect(),
            ),
        }
    };
    let udt = Udt {
        span: default_span(),
        name: Rc::from("TestUdt"),
        definition: def,
    };
    Item {
        id: item_id,
        span: default_span(),
        parent: None,
        doc: Rc::from(""),
        attrs: vec![],
        visibility: Visibility::Public,
        kind: ItemKind::Ty(
            Ident {
                id: LocalVarId::default(),
                span: default_span(),
                name: Rc::from("TestUdt"),
            },
            udt,
        ),
    }
}

/// Creates a store with one package containing the given items.
fn make_store_with_items(items: Vec<Item>) -> (PackageStore, PackageId) {
    let pkg_id = PackageId::from(0usize);
    let mut store = PackageStore::new();
    let mut package = Package {
        items: IndexMap::new(),
        entry: None,
        entry_exec_graph: ExecGraph::default(),
        blocks: IndexMap::new(),
        exprs: IndexMap::new(),
        pats: IndexMap::new(),
        stmts: IndexMap::new(),
    };
    for item in items {
        package.items.insert(item.id, item);
    }
    store.insert(pkg_id, package);
    (store, pkg_id)
}

fn make_ident(name: &str) -> Ident {
    Ident {
        id: LocalVarId::default(),
        span: default_span(),
        name: Rc::from(name),
    }
}

fn make_empty_package() -> Package {
    Package {
        items: IndexMap::new(),
        entry: None,
        entry_exec_graph: ExecGraph::default(),
        blocks: IndexMap::new(),
        exprs: IndexMap::new(),
        pats: IndexMap::new(),
        stmts: IndexMap::new(),
    }
}

fn insert_unit_pat(package: &mut Package, pat_id: PatId) {
    package.pats.insert(
        pat_id,
        Pat {
            id: pat_id,
            span: default_span(),
            ty: Ty::UNIT,
            kind: PatKind::Tuple(vec![]),
        },
    );
}

fn insert_unit_expr(package: &mut Package, expr_id: ExprId) {
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: default_span(),
            ty: Ty::UNIT,
            kind: ExprKind::Tuple(vec![]),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
}

fn insert_bool_lit(package: &mut Package, expr_id: ExprId, value: bool) {
    package.exprs.insert(
        expr_id,
        Expr {
            id: expr_id,
            span: default_span(),
            ty: Ty::Prim(Prim::Bool),
            kind: ExprKind::Lit(qsc_fir::fir::Lit::Bool(value)),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
}

fn insert_struct_callable_package(
    store: &mut PackageStore,
    package_id: PackageId,
    callable_name: &str,
    bool_value: bool,
) -> (LocalItemId, LocalItemId, ExprId) {
    let udt_item_id = LocalItemId::from(0usize);
    let callable_item_id = LocalItemId::from(1usize);
    let input_pat_id = PatId::from(0usize);
    let value_expr_id = ExprId::from(0usize);
    let struct_expr_id = ExprId::from(1usize);
    let stmt_id = StmtId::from(0usize);
    let block_id = BlockId::from(0usize);

    let mut package = make_empty_package();
    insert_unit_pat(&mut package, input_pat_id);
    insert_bool_lit(&mut package, value_expr_id, bool_value);

    let udt_res = Res::Item(ItemId {
        package: package_id,
        item: udt_item_id,
    });
    let udt_ty = Ty::Udt(udt_res);

    package.exprs.insert(
        struct_expr_id,
        Expr {
            id: struct_expr_id,
            span: default_span(),
            ty: udt_ty.clone(),
            kind: ExprKind::Struct(
                udt_res,
                None,
                vec![FieldAssign {
                    id: NodeId::from(0usize),
                    span: default_span(),
                    field: Field::Path(FieldPath { indices: vec![0] }),
                    value: value_expr_id,
                }],
            ),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    package.stmts.insert(
        stmt_id,
        Stmt {
            id: stmt_id,
            span: default_span(),
            kind: StmtKind::Expr(struct_expr_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    package.blocks.insert(
        block_id,
        Block {
            id: block_id,
            span: default_span(),
            ty: udt_ty.clone(),
            stmts: vec![stmt_id],
        },
    );
    package.items.insert(
        udt_item_id,
        make_udt_item(
            udt_item_id,
            vec![(Some(Rc::from("Value")), Ty::Prim(Prim::Bool))],
        ),
    );
    package.items.insert(
        callable_item_id,
        Item {
            id: callable_item_id,
            span: default_span(),
            parent: None,
            doc: Rc::from(""),
            attrs: vec![],
            visibility: Visibility::Public,
            kind: ItemKind::Callable(Box::new(CallableDecl {
                id: NodeId::from(1usize),
                span: default_span(),
                kind: CallableKind::Function,
                name: make_ident(callable_name),
                generics: vec![],
                input: input_pat_id,
                output: udt_ty,
                functors: FunctorSetValue::Empty,
                implementation: CallableImpl::Spec(SpecImpl {
                    body: SpecDecl {
                        id: NodeId::from(2usize),
                        span: default_span(),
                        block: block_id,
                        input: Some(input_pat_id),
                        exec_graph: ExecGraph::default(),
                    },
                    adj: None,
                    ctl: None,
                    ctl_adj: None,
                }),
                attrs: vec![],
            })),
        },
    );
    store.insert(package_id, package);

    (udt_item_id, callable_item_id, struct_expr_id)
}

fn make_entry_package_for_external_callable(
    callee_package_id: PackageId,
    callee_item_id: LocalItemId,
    callee_udt_item_id: LocalItemId,
) -> Package {
    let mut package = make_empty_package();
    let unit_expr_id = ExprId::from(0usize);
    let callee_expr_id = ExprId::from(1usize);
    let call_expr_id = ExprId::from(2usize);

    let output_ty = Ty::Udt(Res::Item(ItemId {
        package: callee_package_id,
        item: callee_udt_item_id,
    }));

    insert_unit_expr(&mut package, unit_expr_id);
    package.exprs.insert(
        callee_expr_id,
        Expr {
            id: callee_expr_id,
            span: default_span(),
            ty: Ty::Arrow(Box::new(Arrow {
                kind: CallableKind::Function,
                input: Box::new(Ty::UNIT),
                output: Box::new(output_ty.clone()),
                functors: FunctorSet::Value(FunctorSetValue::Empty),
            })),
            kind: ExprKind::Var(
                Res::Item(ItemId {
                    package: callee_package_id,
                    item: callee_item_id,
                }),
                vec![],
            ),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    package.exprs.insert(
        call_expr_id,
        Expr {
            id: call_expr_id,
            span: default_span(),
            ty: output_ty,
            kind: ExprKind::Call(callee_expr_id, unit_expr_id),
            exec_graph_range: EMPTY_EXEC_RANGE,
        },
    );
    package.entry = Some(call_expr_id);

    package
}

#[test]
fn resolve_ty_replaces_udt_with_pure_type() {
    let item_id = LocalItemId::from(0usize);
    let udt_item = make_udt_item(
        item_id,
        vec![
            (Some(Rc::from("fst")), Ty::Prim(Prim::Int)),
            (Some(Rc::from("snd")), Ty::Prim(Prim::Double)),
        ],
    );
    let (store, pkg_id) = make_store_with_items(vec![udt_item]);
    let cache = build_udt_cache(&store);

    let udt_ty = Ty::Udt(Res::Item(ItemId {
        package: pkg_id,
        item: item_id,
    }));
    let resolved = resolve_ty(&cache, &udt_ty);
    assert_eq!(
        resolved,
        Ty::Tuple(vec![Ty::Prim(Prim::Int), Ty::Prim(Prim::Double)])
    );
}

#[test]
fn resolve_ty_single_field_udt_unwraps() {
    let item_id = LocalItemId::from(0usize);
    let udt_item = make_udt_item(item_id, vec![(Some(Rc::from("val")), Ty::Prim(Prim::Int))]);
    let (store, pkg_id) = make_store_with_items(vec![udt_item]);
    let cache = build_udt_cache(&store);

    let udt_ty = Ty::Udt(Res::Item(ItemId {
        package: pkg_id,
        item: item_id,
    }));
    let resolved = resolve_ty(&cache, &udt_ty);
    assert_eq!(resolved, Ty::Prim(Prim::Int));
}

#[test]
fn resolve_ty_handles_nested_udt() {
    let inner_id = LocalItemId::from(0usize);
    let outer_id = LocalItemId::from(1usize);
    let pkg_id = PackageId::from(0usize);

    let inner_item = make_udt_item(
        inner_id,
        vec![
            (Some(Rc::from("a")), Ty::Prim(Prim::Int)),
            (Some(Rc::from("b")), Ty::Prim(Prim::Int)),
        ],
    );
    // Outer UDT has one field of type Inner UDT + one Int.
    let outer_fields = vec![
        (
            Some(Rc::from("inner")),
            Ty::Udt(Res::Item(ItemId {
                package: pkg_id,
                item: inner_id,
            })),
        ),
        (Some(Rc::from("extra")), Ty::Prim(Prim::Bool)),
    ];
    let outer_item = make_udt_item(outer_id, outer_fields);

    let (store, _) = make_store_with_items(vec![inner_item, outer_item]);
    let cache = build_udt_cache(&store);

    let outer_ty = Ty::Udt(Res::Item(ItemId {
        package: pkg_id,
        item: outer_id,
    }));
    let resolved = resolve_ty(&cache, &outer_ty);
    assert_eq!(
        resolved,
        Ty::Tuple(vec![
            Ty::Tuple(vec![Ty::Prim(Prim::Int), Ty::Prim(Prim::Int)]),
            Ty::Prim(Prim::Bool),
        ])
    );
}

#[test]
fn resolve_ty_in_array() {
    let item_id = LocalItemId::from(0usize);
    let udt_item = make_udt_item(
        item_id,
        vec![(None, Ty::Prim(Prim::Int)), (None, Ty::Prim(Prim::Int))],
    );
    let (store, pkg_id) = make_store_with_items(vec![udt_item]);
    let cache = build_udt_cache(&store);

    let arr_ty = Ty::Array(Box::new(Ty::Udt(Res::Item(ItemId {
        package: pkg_id,
        item: item_id,
    }))));
    let resolved = resolve_ty(&cache, &arr_ty);
    assert_eq!(
        resolved,
        Ty::Array(Box::new(Ty::Tuple(vec![
            Ty::Prim(Prim::Int),
            Ty::Prim(Prim::Int)
        ])))
    );
}

#[test]
fn resolve_ty_in_arrow() {
    let item_id = LocalItemId::from(0usize);
    let udt_item = make_udt_item(
        item_id,
        vec![(None, Ty::Prim(Prim::Int)), (None, Ty::Prim(Prim::Double))],
    );
    let (store, pkg_id) = make_store_with_items(vec![udt_item]);
    let cache = build_udt_cache(&store);

    let udt_ty = Ty::Udt(Res::Item(ItemId {
        package: pkg_id,
        item: item_id,
    }));
    let arrow_ty = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Operation,
        input: Box::new(udt_ty),
        output: Box::new(Ty::UNIT),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }));
    let resolved = resolve_ty(&cache, &arrow_ty);
    let expected_input = Ty::Tuple(vec![Ty::Prim(Prim::Int), Ty::Prim(Prim::Double)]);
    if let Ty::Arrow(a) = &resolved {
        assert_eq!(*a.input, expected_input);
        assert_eq!(*a.output, Ty::UNIT);
    } else {
        panic!("expected Arrow type");
    }
}

/// Compiles Q# through defunctionalization, runs UDT erasure, and
/// returns a snapshot of callable signatures in the user package.
fn extract_types_after_erasure(source: &str) -> String {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Defunc);
    let mut assigner = qsc_fir::assigner::Assigner::from_package(store.get(pkg_id));
    erase_udts(&mut store, pkg_id, &mut assigner);

    let package = store.get(pkg_id);
    let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
    let mut lines: Vec<String> = Vec::new();
    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            let pat = package.get_pat(decl.input);
            lines.push(format!(
                "{}: input={}, output={}",
                decl.name.name, pat.ty, decl.output
            ));
        }
    }
    lines.sort();
    lines.join("\n")
}

fn check_erasure(source: &str, expect: &Expect) {
    expect.assert_eq(&extract_types_after_erasure(source));
}

fn find_callable_body_block(package: &Package, callable_name: &str) -> BlockId {
    for item in package.items.values() {
        if let ItemKind::Callable(decl) = &item.kind
            && decl.name.name.as_ref() == callable_name
        {
            return match &decl.implementation {
                CallableImpl::Spec(spec_impl) => spec_impl.body.block,
                CallableImpl::SimulatableIntrinsic(spec) => spec.block,
                CallableImpl::Intrinsic => continue,
            };
        }
    }

    panic!("callable '{callable_name}' not found");
}

fn local_names(package: &Package) -> FxHashMap<LocalVarId, String> {
    package
        .pats
        .values()
        .filter_map(|pat| match &pat.kind {
            PatKind::Bind(ident) => Some((ident.id, ident.name.to_string())),
            PatKind::Tuple(_) | PatKind::Discard => None,
        })
        .collect()
}

fn local_name(local_names: &FxHashMap<LocalVarId, String>, local_id: LocalVarId) -> String {
    local_names
        .get(&local_id)
        .cloned()
        .unwrap_or_else(|| format!("<{local_id:?}>"))
}

fn format_pat_name(package: &Package, pat_id: PatId) -> String {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => ident.name.to_string(),
        PatKind::Tuple(sub_pats) => format!(
            "({})",
            sub_pats
                .iter()
                .map(|&sub_pat_id| format_pat_name(package, sub_pat_id))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        PatKind::Discard => "_".to_string(),
    }
}

fn describe_expr(
    package: &Package,
    expr_id: ExprId,
    local_names: &FxHashMap<LocalVarId, String>,
) -> String {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Assign(lhs, rhs) => format!(
            "Assign({}, {})",
            describe_expr(package, *lhs, local_names),
            describe_expr(package, *rhs, local_names)
        ),
        ExprKind::Field(target, field) => format!(
            "Field({}, {field})",
            describe_expr(package, *target, local_names)
        ),
        ExprKind::Lit(lit) => format!("Lit({lit:?})"),
        ExprKind::Tuple(items) => format!(
            "Tuple({})",
            items
                .iter()
                .map(|&item_id| describe_expr(package, item_id, local_names))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ExprKind::Var(Res::Local(local_id), _) => {
            format!("Var({})", local_name(local_names, *local_id))
        }
        ExprKind::Var(res, _) => format!("Var({res})"),
        _ => crate::test_utils::expr_kind_short(package, expr_id),
    }
}

fn callable_local_summaries_after_erasure(source: &str, callable_name: &str) -> String {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::UdtErase);
    let package = store.get(pkg_id);
    let block = package.get_block(find_callable_body_block(package, callable_name));
    let local_names = local_names(package);

    block
        .stmts
        .iter()
        .filter_map(|&stmt_id| {
            let stmt = package.get_stmt(stmt_id);
            match &stmt.kind {
                StmtKind::Local(mutability, pat_id, init_expr_id) => Some(format!(
                    "{mutability:?} {} = {}",
                    format_pat_name(package, *pat_id),
                    describe_expr(package, *init_expr_id, &local_names)
                )),
                _ => None,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn callable_body_summary_after_erasure(source: &str, callable_name: &str) -> String {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::UdtErase);
    let package = store.get(pkg_id);
    let block = package.get_block(find_callable_body_block(package, callable_name));
    let local_names = local_names(package);

    block
        .stmts
        .iter()
        .enumerate()
        .map(|(index, &stmt_id)| {
            let stmt = package.get_stmt(stmt_id);
            let summary = match &stmt.kind {
                StmtKind::Expr(expr_id) => {
                    format!("Expr {}", describe_expr(package, *expr_id, &local_names))
                }
                StmtKind::Semi(expr_id) => {
                    format!("Semi {}", describe_expr(package, *expr_id, &local_names))
                }
                StmtKind::Local(mutability, pat_id, init_expr_id) => format!(
                    "Local {mutability:?} {} = {}",
                    format_pat_name(package, *pat_id),
                    describe_expr(package, *init_expr_id, &local_names)
                ),
                StmtKind::Item(local_item_id) => format!("Item {local_item_id}"),
            };

            format!("[{index}] {summary}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn main_local_summaries_after_erasure(source: &str) -> String {
    callable_local_summaries_after_erasure(source, "Main")
}

fn main_body_summary_after_erasure(source: &str) -> String {
    callable_body_summary_after_erasure(source, "Main")
}

fn check_callable_body_summary_after_erasure(source: &str, callable_name: &str, expect: &Expect) {
    expect.assert_eq(&callable_body_summary_after_erasure(source, callable_name));
}

fn check_main_local_summaries_after_erasure(source: &str, expect: &Expect) {
    expect.assert_eq(&main_local_summaries_after_erasure(source));
}

fn check_main_body_summary_after_erasure(source: &str, expect: &Expect) {
    expect.assert_eq(&main_body_summary_after_erasure(source));
}

#[test]
fn simple_newtype_erased_to_inner_type() {
    check_erasure(
        indoc! {"
                namespace Test {
                    newtype Wrapper = Int;
                    @EntryPoint()
                    function Main() : Unit {
                        let w = Wrapper(42);
                    }
                }
            "},
        &expect![[r#"
                Main: input=Unit, output=Unit"#]],
    );
}

#[test]
fn tuple_udt_erased_to_tuple() {
    check_erasure(
        indoc! {"
                namespace Test {
                    newtype Pair = (Fst: Int, Snd: Double);
                    function MakePair() : (Int, Double) {
                        let p = Pair(1, 2.0);
                        (p::Fst, p::Snd)
                    }
                    @EntryPoint()
                    function Main() : Unit {
                        let _ = MakePair();
                    }
                }
            "},
        &expect![[r#"
                Main: input=Unit, output=Unit
                MakePair: input=Unit, output=(Int, Double)"#]],
    );
}

#[test]
fn nested_udt_erased_to_nested_tuple() {
    check_erasure(
        indoc! {"
                namespace Test {
                    newtype Inner = (A: Int, B: Int);
                    newtype Outer = (First: Inner, Extra: Bool);
                    function MakeOuter() : Outer {
                        let i = Inner(1, 2);
                        Outer(i, true)
                    }
                    @EntryPoint()
                    function Main() : Unit {
                        let _ = MakeOuter();
                    }
                }
            "},
        &expect![[r#"
                Main: input=Unit, output=Unit
                MakeOuter: input=Unit, output=((Int, Int), Bool)"#]],
    );
}

/// Verifies that `p w/ Fst <- 42` on a two-field UDT is lowered to a
/// tuple construction after UDT erasure. The `PostUdtErase` invariant
/// check (run inside the pipeline) asserts that no
/// `UpdateField(_, Field::Path(_), _)` survives.
#[test]
fn udt_update_field_simple() {
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                newtype Pair = (Fst: Int, Snd: Double);
                @EntryPoint()
                function Main() : Unit {
                    let p = Pair(1, 2.0);
                    let p2 = p w/ Fst <- 42;
                }
            }
        "},
        &expect![[r#"
            Immutable p = Tuple(Lit(Int(1)), Lit(Double(2.0)))
            Immutable p2 = Tuple(Lit(Int(42)), Field(Var(p), Path([1])))"#]],
    );
}

/// Verifies multi-level path lowering: `f w/ b <- 3.14` on a UDT with
/// nested anonymous tuple `(a: Int, (b: Double, c: Bool))` produces
/// field path `[1, 0]` which must be recursively lowered.
#[test]
fn udt_update_field_nested() {
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                newtype Foo = (a: Int, (b: Double, c: Bool));
                @EntryPoint()
                function Main() : Unit {
                    let f = Foo(1, (2.0, true));
                    let f2 = f w/ b <- 3.14;
                }
            }
        "},
        &expect![[r#"
            Immutable f = Tuple(Lit(Int(1)), Tuple(Lit(Double(2.0)), Lit(Bool(true))))
            Immutable f2 = Tuple(Field(Var(f), Path([0])), Tuple(Lit(Double(3.14)), Field(Field(Var(f), Path([1])), Path([1]))))"#]],
    );
}

/// Verifies that `w w/ val <- 42` on a single-field UDT (where the
/// pure type is scalar, not a tuple) is lowered to the replacement
/// value directly.
#[test]
fn udt_update_field_single_field() {
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                newtype Wrapper = (val: Int);
                @EntryPoint()
                function Main() : Unit {
                    let w = Wrapper(99);
                    let w2 = w w/ val <- 42;
                }
            }
        "},
        &expect![[r#"
            Immutable w = Lit(Int(99))
            Immutable w2 = Lit(Int(42))"#]],
    );
}

/// Verifies that `set p w/= Fst <- 42` (`AssignField`) is lowered to
/// `Assign(p, Tuple(...))` after UDT erasure.
#[test]
fn udt_assign_field() {
    check_main_body_summary_after_erasure(
        indoc! {"
            namespace Test {
                newtype Pair = (Fst: Int, Snd: Double);
                @EntryPoint()
                function Main() : Unit {
                    mutable p = Pair(1, 2.0);
                    p w/= Fst <- 42;
                }
            }
        "},
        &expect![[r#"
            [0] Local Mutable p = Tuple(Lit(Int(1)), Lit(Double(2.0)))
            [1] Semi Assign(Var(p), Tuple(Lit(Int(42)), Field(Var(p), Path([1]))))"#]],
    );
}

/// Verifies that two successive `w/` updates are each independently
/// lowered into tuple constructions.
#[test]
fn udt_chained_update() {
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                newtype Pair = (Fst: Int, Snd: Double);
                @EntryPoint()
                function Main() : Unit {
                    let p = Pair(1, 2.0);
                    let p2 = p w/ Fst <- 42;
                    let p3 = p2 w/ Snd <- 3.14;
                }
            }
        "},
        &expect![[r#"
            Immutable p = Tuple(Lit(Int(1)), Lit(Double(2.0)))
            Immutable p2 = Tuple(Lit(Int(42)), Field(Var(p), Path([1])))
            Immutable p3 = Tuple(Field(Var(p2), Path([0])), Lit(Double(3.14)))"#]],
    );
}

/// Verifies 3-level field path lowering: updating a deeply nested named
/// field within anonymous tuples exercises recursive `lower_update_field`
/// with a 3-element path `[1, 1, 0]`.
#[test]
fn udt_update_field_deeply_nested() {
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                newtype Deep = (a: Int, (b: Bool, (c: Double, d: Int)));
                @EntryPoint()
                function Main() : Unit {
                    let f = Deep(1, (true, (2.0, 3)));
                    let f2 = f w/ c <- 3.14;
                }
            }
        "},
        &expect![[r#"
            Immutable f = Tuple(Lit(Int(1)), Tuple(Lit(Bool(true)), Tuple(Lit(Double(2.0)), Lit(Int(3)))))
            Immutable f2 = Tuple(Field(Var(f), Path([0])), Tuple(Field(Field(Var(f), Path([1])), Path([0])), Tuple(Lit(Double(3.14)), Field(Field(Field(Var(f), Path([1])), Path([1])), Path([1])))))"#]],
    );
}

/// Verifies `UpdateField` lowering when a UDT contains another UDT:
/// `Outer = (First: Inner, Extra: Bool)` where `Inner = (x: Int, y: Int)`.
/// Updating `Extra` (a top-level field) exercises single-level path
/// lowering on a record whose sub-elements are themselves tuples.
#[test]
fn udt_nested_udt_update() {
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                newtype Inner = (x: Int, y: Int);
                newtype Outer = (First: Inner, Extra: Bool);
                @EntryPoint()
                function Main() : Unit {
                    let i = Inner(1, 2);
                    let o = Outer(i, true);
                    let o2 = o w/ Extra <- false;
                }
            }
        "},
        &expect![[r#"
            Immutable i = Tuple(Lit(Int(1)), Lit(Int(2)))
            Immutable o = Tuple(Var(i), Lit(Bool(true)))
            Immutable o2 = Tuple(Field(Var(o), Path([0])), Lit(Bool(false)))"#]],
    );
}

#[test]
fn resolve_ty_udt_with_array_field() {
    // UDT with Int[] field: the array element type is unchanged but
    // the UDT wrapper is erased.
    let item_id = LocalItemId::from(0usize);
    let udt_item = make_udt_item(
        item_id,
        vec![
            (
                Some(Rc::from("vals")),
                Ty::Array(Box::new(Ty::Prim(Prim::Int))),
            ),
            (Some(Rc::from("flag")), Ty::Prim(Prim::Bool)),
        ],
    );
    let (store, pkg_id) = make_store_with_items(vec![udt_item]);
    let cache = build_udt_cache(&store);

    let udt_ty = Ty::Udt(Res::Item(ItemId {
        package: pkg_id,
        item: item_id,
    }));
    let resolved = resolve_ty(&cache, &udt_ty);
    assert_eq!(
        resolved,
        Ty::Tuple(vec![
            Ty::Array(Box::new(Ty::Prim(Prim::Int))),
            Ty::Prim(Prim::Bool),
        ])
    );
}

#[test]
fn udt_as_callable_parameter_type() {
    // UDT in callable parameter position is erased to tuple.
    check_erasure(
        indoc! {"
            namespace Test {
                newtype Pair = (Fst: Int, Snd: Double);
                function UsePair(p : Pair) : Int { p::Fst }
                @EntryPoint()
                function Main() : Unit {
                    let _ = UsePair(Pair(1, 2.0));
                }
            }
        "},
        &expect![[r#"
            Main: input=Unit, output=Unit
            UsePair: input=(Int, Double), output=Int"#]],
    );
}

#[test]
fn udt_as_callable_return_type() {
    // UDT in callable return type is erased to tuple.
    check_erasure(
        indoc! {"
            namespace Test {
                newtype Pair = (Fst: Int, Snd: Double);
                function MakeIt() : Pair { Pair(1, 2.0) }
                @EntryPoint()
                function Main() : Unit {
                    let _ = MakeIt();
                }
            }
        "},
        &expect![[r#"
            Main: input=Unit, output=Unit
            MakeIt: input=Unit, output=(Int, Double)"#]],
    );
}

#[test]
fn udt_zero_fields_erased_to_unit() {
    // `newtype Marker = Unit` maps to a single-field UDT whose inner type
    // is Unit. After erasure the type becomes Unit (scalar).
    check_erasure(
        indoc! {"
            namespace Test {
                newtype Marker = Unit;
                @EntryPoint()
                function Main() : Unit {
                    let m = Marker(());
                }
            }
        "},
        &expect![[r#"
            Main: input=Unit, output=Unit"#]],
    );
}

#[test]
fn udt_used_in_nested_callable() {
    // UDT created and used inside a helper callable (not Main).
    // The erasure should apply to all callables in the package.
    check_erasure(
        indoc! {"
            namespace Test {
                newtype Pair = (Fst: Int, Snd: Int);
                function MakeAndSum(x : Int) : Int {
                    let p = Pair(x, x + 1);
                    p::Fst + p::Snd
                }
                @EntryPoint()
                function Main() : Unit {
                    let _ = MakeAndSum(5);
                }
            }
        "},
        &expect![[r#"
            Main: input=Unit, output=Unit
            MakeAndSum: input=Int, output=Int"#]],
    );
}

#[test]
fn resolve_ty_udt_in_tuple() {
    // `(MyPair, Int)` — the inner UDT within a tuple wrapper is resolved.
    let item_id = LocalItemId::from(0usize);
    let udt_item = make_udt_item(
        item_id,
        vec![
            (Some(Rc::from("a")), Ty::Prim(Prim::Int)),
            (Some(Rc::from("b")), Ty::Prim(Prim::Int)),
        ],
    );
    let (store, pkg_id) = make_store_with_items(vec![udt_item]);
    let cache = build_udt_cache(&store);

    let tuple_ty = Ty::Tuple(vec![
        Ty::Udt(Res::Item(ItemId {
            package: pkg_id,
            item: item_id,
        })),
        Ty::Prim(Prim::Bool),
    ]);
    let resolved = resolve_ty(&cache, &tuple_ty);
    assert_eq!(
        resolved,
        Ty::Tuple(vec![
            Ty::Tuple(vec![Ty::Prim(Prim::Int), Ty::Prim(Prim::Int)]),
            Ty::Prim(Prim::Bool),
        ])
    );
}

#[test]
fn udt_copy_update_expression() {
    // `p w/ Fst <- 10` on a two-field UDT should lower to an erased tuple
    // that keeps the untouched field as a projection from the source value.
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                newtype Pair = (Fst: Int, Snd: Int);
                @EntryPoint()
                function Main() : Unit {
                    let p = Pair(1, 2);
                    let p2 = p w/ Fst <- 10;
                }
            }
        "},
        &expect![[r#"
            Immutable p = Tuple(Lit(Int(1)), Lit(Int(2)))
            Immutable p2 = Tuple(Lit(Int(10)), Field(Var(p), Path([1])))"#]],
    );
}

/// Verifies that `new Pair { ...p, Fst = 42 }` on a two-field UDT is
/// lowered to a tuple with the replacement at index 0 after UDT erasure.
#[test]
fn udt_copy_update_single_field() {
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                newtype Pair = (Fst: Int, Snd: Double);
                @EntryPoint()
                function Main() : Unit {
                    let p = Pair(1, 2.0);
                    let p2 = new Pair { ...p, Fst = 42 };
                }
            }
        "},
        &expect![[r#"
            Immutable p = Tuple(Lit(Int(1)), Lit(Double(2.0)))
            Immutable p2 = Tuple(Lit(Int(42)), Field(Var(p), Path([1])))"#]],
    );
}

/// Verifies that `new Triple { ...t, A = 1, C = 3 }` on a three-field UDT
/// is lowered to a tuple with replacements at indices 0 and 2.
#[test]
fn udt_copy_update_multiple_fields() {
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                newtype Triple = (A: Int, B: Double, C: Bool);
                @EntryPoint()
                function Main() : Unit {
                    let t = Triple(1, 2.0, false);
                    let t2 = new Triple { ...t, A = 10, C = true };
                }
            }
        "},
        &expect![[r#"
            Immutable t = Tuple(Lit(Int(1)), Lit(Double(2.0)), Lit(Bool(false)))
            Immutable t2 = Tuple(Lit(Int(10)), Field(Var(t), Path([1])), Lit(Bool(true)))"#]],
    );
}

/// Verifies that copy-update on a single-field UDT is lowered to the scalar
/// replacement value directly.
#[test]
fn udt_copy_update_single_field_udt() {
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                newtype Wrapper = (val: Int);
                @EntryPoint()
                function Main() : Unit {
                    let w = Wrapper(99);
                    let w2 = w w/ val <- 10;
                }
            }
        "},
        &expect![[r#"
            Immutable w = Lit(Int(99))
            Immutable w2 = Lit(Int(10))"#]],
    );
}

/// Verifies copy-update on a UDT with nested UDT fields. Updating
/// a top-level field should produce a tuple with the replacement
/// and field extractions for the remaining fields.
#[test]
fn udt_copy_update_nested() {
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                newtype Inner = (x: Int, y: Int);
                newtype Outer = (First: Inner, Extra: Bool);
                @EntryPoint()
                function Main() : Unit {
                    let i = Inner(1, 2);
                    let o = Outer(i, true);
                    let o2 = new Outer { ...o, Extra = false };
                }
            }
        "},
        &expect![[r#"
            Immutable i = Tuple(Lit(Int(1)), Lit(Int(2)))
            Immutable o = Tuple(Var(i), Lit(Bool(true)))
            Immutable o2 = Tuple(Field(Var(o), Path([0])), Lit(Bool(false)))"#]],
    );
}

#[test]
fn zero_field_udt_erased_to_unit() {
    // Zero-field struct: `struct Empty {}` — boundary condition for
    // UDT erasure where the underlying type collapses to Unit.
    check_erasure(
        indoc! {"
            struct Empty {}

            function Main() : Unit {
                let e = new Empty {};
            }
        "},
        &expect![[r#"
            Main: input=Unit, output=Unit"#]],
    );
}

#[test]
fn three_level_nested_udt_fully_erased() {
    // 3-level nested UDTs: verifies recursive resolution cache handles
    // Inner → Middle → Outer chain correctly.
    check_erasure(
        indoc! {"
            struct Inner { X : Int }
            struct Middle { I : Inner, Y : Double }
            struct Outer { M : Middle, Z : Bool }

            function Main() : Int {
                let o = new Outer { M = new Middle { I = new Inner { X = 42 }, Y = 1.0 }, Z = true };
                o.M.I.X
            }
        "},
        &expect![[r#"
            Main: input=Unit, output=Int"#]],
    );
}

#[test]
fn udt_as_callable_return_type_erased() {
    // UDT used as the return type of a callable: the output type
    // should be resolved from Ty::Udt to (Int, Double) tuple.
    check_erasure(
        indoc! {"
            struct Pair { Fst : Int, Snd : Double }

            function MakePair(x : Int, y : Double) : Pair {
                new Pair { Fst = x, Snd = y }
            }

            function Main() : Int {
                let p = MakePair(1, 2.0);
                p.Fst
            }
        "},
        &expect![[r#"
            Main: input=Unit, output=Int
            MakePair: input=(Int, Double), output=(Int, Double)"#]],
    );
}

#[test]
fn resolve_ty_cache_miss_returns_original_udt() {
    // When a Ty::Udt references an item not present in the cache,
    // resolve_ty returns the original type unchanged. This is a
    // defensive code path — in practice, all UDT items should be
    // present in the cache after build_udt_cache.
    let item_id = LocalItemId::from(0usize);
    let udt_item = make_udt_item(
        item_id,
        vec![
            (Some(Rc::from("a")), Ty::Prim(Prim::Int)),
            (Some(Rc::from("b")), Ty::Prim(Prim::Double)),
        ],
    );
    let (store, pkg_id) = make_store_with_items(vec![udt_item]);
    let cache = build_udt_cache(&store);

    // Reference a different package that has no UDT items in the cache.
    let missing_pkg = PackageId::from(99usize);
    let missing_ty = Ty::Udt(Res::Item(ItemId {
        package: missing_pkg,
        item: item_id,
    }));
    let resolved = resolve_ty(&cache, &missing_ty);
    // Cache miss: original type returned unchanged.
    assert_eq!(resolved, missing_ty);

    // Also verify a missing item within the same package.
    let missing_item = LocalItemId::from(99usize);
    let missing_ty2 = Ty::Udt(Res::Item(ItemId {
        package: pkg_id,
        item: missing_item,
    }));
    let resolved2 = resolve_ty(&cache, &missing_ty2);
    assert_eq!(resolved2, missing_ty2);
}

#[test]
fn erase_udts_rewrites_reachable_external_package_but_leaves_unreachable_package_untouched() {
    let target_pkg_id = PackageId::from(1usize);
    let reachable_pkg_id = PackageId::from(2usize);
    let unreachable_pkg_id = PackageId::from(3usize);

    let mut store = PackageStore::new();
    let (reachable_udt_item_id, reachable_callable_item_id, reachable_struct_expr_id) =
        insert_struct_callable_package(&mut store, reachable_pkg_id, "Reachable", true);
    let (_unreachable_udt_item_id, _unreachable_callable_item_id, unreachable_struct_expr_id) =
        insert_struct_callable_package(&mut store, unreachable_pkg_id, "Unreachable", false);

    store.insert(
        target_pkg_id,
        make_entry_package_for_external_callable(
            reachable_pkg_id,
            reachable_callable_item_id,
            reachable_udt_item_id,
        ),
    );

    let mut assigner = qsc_fir::assigner::Assigner::from_package(store.get(target_pkg_id));
    erase_udts(&mut store, target_pkg_id, &mut assigner);
    crate::invariants::check(
        &store,
        target_pkg_id,
        crate::invariants::InvariantLevel::PostUdtErase,
    );

    let target_package = store.get(target_pkg_id);
    let entry_expr = target_package.get_expr(target_package.entry.expect("entry should exist"));
    assert_eq!(entry_expr.ty, Ty::Prim(Prim::Bool));

    let reachable_package = store.get(reachable_pkg_id);
    let ItemKind::Callable(reachable_callable) =
        &reachable_package.get_item(reachable_callable_item_id).kind
    else {
        panic!("reachable item should be callable");
    };
    assert_eq!(reachable_callable.output, Ty::Prim(Prim::Bool));
    let reachable_struct_expr = reachable_package.get_expr(reachable_struct_expr_id);
    assert_eq!(reachable_struct_expr.ty, Ty::Prim(Prim::Bool));
    assert!(
        !matches!(reachable_struct_expr.kind, ExprKind::Struct(_, _, _)),
        "reachable external package should have struct expressions erased"
    );

    let unreachable_package = store.get(unreachable_pkg_id);
    let ItemKind::Callable(unreachable_callable) =
        &unreachable_package.get_item(LocalItemId::from(1usize)).kind
    else {
        panic!("unreachable item should be callable");
    };
    assert!(
        matches!(unreachable_callable.output, Ty::Udt(_)),
        "unreachable package callable output should remain untouched"
    );
    let unreachable_struct_expr = unreachable_package.get_expr(unreachable_struct_expr_id);
    assert!(
        matches!(unreachable_struct_expr.kind, ExprKind::Struct(_, _, _)),
        "unreachable package struct should remain untouched"
    );
    assert!(
        matches!(unreachable_struct_expr.ty, Ty::Udt(_)),
        "unreachable package expression type should remain untouched"
    );
}

#[test]
#[should_panic(expected = "contains Ty::Udt after UDT erasure")]
fn post_udt_erase_invariants_cover_reachable_external_packages() {
    let target_pkg_id = PackageId::from(1usize);
    let reachable_pkg_id = PackageId::from(2usize);

    let mut store = PackageStore::new();
    let (reachable_udt_item_id, reachable_callable_item_id, _reachable_struct_expr_id) =
        insert_struct_callable_package(&mut store, reachable_pkg_id, "Reachable", true);

    store.insert(
        target_pkg_id,
        make_entry_package_for_external_callable(
            reachable_pkg_id,
            reachable_callable_item_id,
            reachable_udt_item_id,
        ),
    );

    let mut assigner = qsc_fir::assigner::Assigner::from_package(store.get(target_pkg_id));
    erase_udts(&mut store, target_pkg_id, &mut assigner);

    let reachable_package = store.get_mut(reachable_pkg_id);
    let reachable_item = reachable_package
        .items
        .get_mut(reachable_callable_item_id)
        .expect("reachable callable should exist");
    let ItemKind::Callable(reachable_callable) = &mut reachable_item.kind else {
        panic!("reachable item should be callable");
    };
    reachable_callable.output = Ty::Udt(Res::Item(ItemId {
        package: reachable_pkg_id,
        item: reachable_udt_item_id,
    }));

    crate::invariants::check(
        &store,
        target_pkg_id,
        crate::invariants::InvariantLevel::PostUdtErase,
    );
}

/// Single-field struct declared with struct syntax: `get_pure_ty` returns
/// `Tuple([Int])`, so UDT erase must keep the tuple wrapper rather than
/// unwrapping to scalar. The `PostAll` invariant checks pat/init type
/// alignment and would panic if the expression were incorrectly unwrapped.
#[test]
fn single_field_struct_passes_post_all_invariant() {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let _ = compile_and_run_pipeline_to(
        indoc! {"
            struct Single { Value : Int }

            function Main() : Int {
                let s = new Single { Value = 42 };
                s.Value
            }
        "},
        PipelineStage::Full,
    );
}

/// Single-field struct syntax has a constructor whose pure type is
/// `Tuple([T])`. UDT erase eliminates the constructor call while preserving
/// the tuple wrapper, so the full pipeline passes without type mismatches.
#[test]
fn single_field_struct_constructor_passes_post_all_invariant() {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let _ = compile_and_run_pipeline_to(
        indoc! {"
            struct Wrapper { Value : Int }

            function Main() : Int {
                let w = new Wrapper { Value = 42 };
                0
            }
        "},
        PipelineStage::Full,
    );
}

/// Single-field struct syntax produces `UdtDefKind::Tuple([Field])`. Verify
/// UDT erase keeps the tuple wrapper.
#[test]
fn single_field_struct_erased_to_tuple() {
    check_main_local_summaries_after_erasure(
        indoc! {"
            namespace Test {
                struct Wrapper { Value : Int }
                @EntryPoint()
                function Main() : Unit {
                    let w = new Wrapper { Value = 42 };
                }
            }
        "},
        &expect![[r#"
            Immutable w = Tuple(Lit(Int(42)))"#]],
    );
}

/// Single-field struct variant with a function returning the wrapper type: the
/// erased output type is `(Int,)` (single-element tuple), confirming
/// `UdtDefKind::Tuple([Field])` preserves the tuple wrapper in return position.
#[test]
fn single_field_struct_return_type_erased_to_single_element_tuple() {
    check_erasure(
        indoc! {"
            namespace Test {
                struct Wrapper { Value : Int }
                function Make() : Wrapper { new Wrapper { Value = 42 } }
                @EntryPoint()
                function Main() : Unit {
                    let _ = Make();
                }
            }
        "},
        &expect![[r#"
            Main: input=Unit, output=Unit
            Make: input=Unit, output=(Int,)"#]],
    );
}

/// Control test: non-trailing-comma single-field newtype `(Value : Int)` is
/// erased to scalar `Int` (not a single-element tuple), confirming the
/// `UdtDefKind::Field` → scalar unwrap path.
#[test]
fn non_trailing_comma_newtype_single_field_erased_to_scalar() {
    check_erasure(
        indoc! {"
            namespace Test {
                newtype Wrapper = (Value : Int);
                function Make() : Wrapper { Wrapper(42) }
                @EntryPoint()
                function Main() : Unit {
                    let _ = Make();
                }
            }
        "},
        &expect![[r#"
            Main: input=Unit, output=Unit
            Make: input=Unit, output=Int"#]],
    );
}

#[test]
fn scalar_erased_newtype_field_read_lowered() {
    // Field read access on a scalar-erased single-field newtype should be
    // lowered. For example:
    // - `newtype Wrapper = (x: Int); function Extract(w: Wrapper) : Int { w::x }`
    // - After UDT erasure: `w: Prim(Int)` and `w::x` should become just `w`
    // - The PostUdtErase invariant requires Field::Path only on Ty::Tuple,
    //   so this lowering is necessary to satisfy the invariant.
    check_callable_body_summary_after_erasure(
        indoc! {"
            namespace Test {
                newtype Wrapper = (Value : Int);
                function Extract(w : Wrapper) : Int { w::Value }
                @EntryPoint()
                function Main() : Unit {
                    let x = Wrapper(42);
                    let _ = Extract(x);
                }
            }
        "},
        "Extract",
        &expect![[r#"
            [0] Expr Var(x)"#]],
    );
}

#[test]
fn udt_erase_is_idempotent() {
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
    let (mut store, pkg_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::UdtErase);
    let first = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigner = qsc_fir::assigner::Assigner::from_package(store.get(pkg_id));
    erase_udts(&mut store, pkg_id, &mut assigner);
    let second = crate::pretty::write_package_qsharp(&store, pkg_id);
    assert_eq!(first, second, "udt_erase should be idempotent");
}

fn render_before_after_udt_erase(source: &str) -> (String, String) {
    let (mut store, pkg_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
    let before = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigner = qsc_fir::assigner::Assigner::from_package(store.get(pkg_id));
    erase_udts(&mut store, pkg_id, &mut assigner);
    let after = crate::pretty::write_package_qsharp(&store, pkg_id);
    (before, after)
}

fn check_before_after_udt_erase(source: &str, expect: &Expect) {
    let (before, after) = render_before_after_udt_erase(source);
    expect.assert_eq(&format!("BEFORE:\n{before}\nAFTER:\n{after}"));
}

#[test]
fn before_after_udt_erasure_snapshot() {
    check_before_after_udt_erase(
        indoc! {"
            namespace Test {
                struct Pair { X : Int, Y : Int }
                @EntryPoint()
                function Main() : (Int, Int) {
                    let p = new Pair { X = 1, Y = 2 };
                    (p.X, p.Y)
                }
            }
        "},
        &expect![[r#"
            BEFORE:
            // namespace Test
            newtype Pair = (Int, Int);
            function Main() : (Int, Int) {
                body {
                    let p : UDT < Item 1(Package 2) > = new Pair {
                        X = 1,
                        Y = 2
                    };
                    (p::X, p::Y)
                }
            }
            // entry
            Main()

            AFTER:
            // namespace Test
            newtype Pair = (Int, Int);
            function Main() : (Int, Int) {
                body {
                    let p : (Int, Int) = (1, 2);
                    (p::Item < 0 >, p::Item < 1 >)
                }
            }
            // entry
            Main()
        "#]], // snapshot populated by UPDATE_EXPECT=1
    );
}
