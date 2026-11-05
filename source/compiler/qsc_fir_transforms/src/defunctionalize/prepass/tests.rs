// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests the environment rewrite in isolation, before later passes can hide
//! incorrect or missing normalization.

use super::*;
use crate::{
    fir_builder::{alloc_item_var_expr, alloc_semi_stmt},
    test_utils::{
        assert_full_pipeline_succeeds, compile_to_fir, compile_to_fir_with_library,
        compile_to_monomorphized_fir, find_callable, find_callable_body_block,
        find_library_callable,
    },
    walk_utils::for_each_expr_in_callable_impl,
};
use qsc_eval::val::Value;
use qsc_fir::{fir::ItemId, ty::Arrow};

const FACTORY: &str = r#"
    function Make(environment : (Int, Int -> Int)) : Int -> Int {
        value -> {
            let (bias, action) = environment;
            bias + action(value)
        }
    }
"#;

const SHARED_CONSTRUCTOR: &str = r#"
    function Make(x : Int) : Int => Int { y => x + 2 * y }
    operation Main() : Int {
        use q = Qubit();
        let choose = MResetZ(q) == Zero;
        mutable a = 0;
        (if choose { Make(a) } else { Make(a) })({ a = 9; 2 })
    }
"#;

fn share_conditional_constructor_branch(package: &mut Package) {
    let selections: Vec<_> = package
        .exprs
        .iter()
        .filter_map(|(id, expr)| {
            if let ExprKind::If(condition, then, Some(_)) = expr.kind
                && matches!(expr.ty, Ty::Arrow(_))
            {
                Some((id, condition, then))
            } else {
                None
            }
        })
        .collect();
    let [(id, condition, then)] = selections[..] else {
        panic!("expected one callable-valued conditional");
    };
    package.exprs.get_mut(id).expect("conditional exists").kind =
        ExprKind::If(condition, then, Some(then));
}

fn normalize_scalar_inputs(package: &mut Package) -> Vec<ExprId> {
    let expressions: Vec<_> = package.exprs.iter().map(|(id, _)| id).collect();
    let mut assigner = Assigner::from_package(package);
    materialize_immediate_scalar_captures(package, &expressions, &mut assigner)
}

fn make_calls(package: &Package) -> Vec<(ExprId, ExprId, ExprId)> {
    package
        .exprs
        .iter()
        .filter_map(|(id, expr)| {
            let ExprKind::Call(callee, args) = expr.kind else {
                return None;
            };
            let ExprKind::Var(Res::Item(item), _) = package.get_expr(callee).kind else {
                return None;
            };
            (item.package == package.id
                && matches!(&package.get_item(item.item).kind,
                    ItemKind::Callable(decl) if decl.name.name.as_ref() == "Make"))
            .then_some((id, callee, args))
        })
        .collect()
}

#[test]
fn distinct_constructors_with_shared_operand_keep_snapshots_in_scope() {
    let (mut store, entry) = compile_to_fir(
        r#"
        function Make(x : Int) : Int => Int { y => x + 2 * y }
        operation Main() : Int {
            mutable a = 0;
            Make(a)({ a = 5; 2 }) + Make(a)({ a = 9; 3 })
        }
        "#,
    );
    let package = store.get_mut(entry);
    let [(first, _, arg), (second, callee, _)] = make_calls(package)[..] else {
        panic!("expected two constructors");
    };
    assert_ne!(first, second);
    package
        .exprs
        .get_mut(second)
        .expect("second constructor")
        .kind = ExprKind::Call(callee, arg);
    crate::invariants::check(&store, entry, crate::invariants::InvariantLevel::PostMono);

    assert_full_pipeline_succeeds(
        "distinct constructors sharing an operand",
        &mut store,
        entry,
    );
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, entry),
        Ok(Value::Int(15))
    );
}

#[test]
fn constructor_snapshot_does_not_rewrite_a_shared_later_read() {
    let (mut store, entry) = compile_to_fir(
        r#"
        function Make(x : Int) : Int => Int { y => x + 2 * y }
        operation Main() : Int {
            mutable a = 0;
            let value = Make(a)(2);
            a = 9;
            value * 100 + a
        }
        "#,
    );
    let package = store.get_mut(entry);
    let [(_, _, arg)] = make_calls(package)[..] else {
        panic!("expected one constructor");
    };
    let ExprKind::Var(Res::Local(local), _) = package.get_expr(arg).kind else {
        panic!("mutable input read");
    };
    let additions: Vec<_> = package
        .exprs
        .iter()
        .filter_map(|(id, expr)| {
            if let ExprKind::BinOp(qsc_fir::fir::BinOp::Add, lhs, rhs) = expr.kind
                && matches!(package.get_expr(rhs).kind, ExprKind::Var(Res::Local(id), _) if id == local)
            {
                Some((id, lhs))
            } else {
                None
            }
        })
        .collect();
    let [(sum, lhs)] = additions[..] else {
        panic!("expected one final addition");
    };
    package.exprs.get_mut(sum).expect("final addition").kind =
        ExprKind::BinOp(qsc_fir::fir::BinOp::Add, lhs, arg);
    crate::invariants::check(&store, entry, crate::invariants::InvariantLevel::PostMono);

    assert_full_pipeline_succeeds(
        "constructor operand shared with later read",
        &mut store,
        entry,
    );
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, entry),
        Ok(Value::Int(409))
    );
}

#[test]
fn shared_tuple_operand_read_observes_mutation_between_occurrences() {
    let (mut store, entry) = compile_to_fir(
        r#"
        function Make(x : Int, y : Int) : Int => Int { z => x + 10 * y + z }
        operation Main() : Int {
            mutable a = 0;
            Make(a, { a = 3; a })(2)
        }
        "#,
    );
    let package = store.get_mut(entry);
    let [(_, _, args)] = make_calls(package)[..] else {
        panic!("expected one constructor");
    };
    let ExprKind::Tuple(items) = &package.get_expr(args).kind else {
        panic!("tuple input");
    };
    let [first, second] = items[..] else {
        panic!("two operands");
    };
    let ExprKind::Block(block) = package.get_expr(second).kind else {
        panic!("second operand block");
    };
    let last = *package.get_block(block).stmts.last().expect("block tail");
    package.stmts.get_mut(last).expect("tail statement").kind = StmtKind::Expr(first);
    crate::invariants::check(&store, entry, crate::invariants::InvariantLevel::PostMono);

    assert_full_pipeline_succeeds(
        "shared tuple operand read across mutation",
        &mut store,
        entry,
    );
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, entry),
        Ok(Value::Int(32))
    );
}

#[test]
fn shared_conditional_constructor_is_normalized_once_without_unbound_locals() {
    let (mut store, entry) = compile_to_monomorphized_fir(SHARED_CONSTRUCTOR);
    share_conditional_constructor_branch(store.get_mut(entry));
    crate::invariants::check(&store, entry, crate::invariants::InvariantLevel::PostMono);

    assert_eq!(normalize_scalar_inputs(store.get_mut(entry)).len(), 1);
    let package = store.get(entry);
    crate::invariants::check_local_var_consistency(package, find_callable(package, "Main"));
    let normalized = format!("{package:?}");
    assert!(normalize_scalar_inputs(store.get_mut(entry)).is_empty());
    assert_eq!(format!("{:?}", store.get(entry)), normalized);
    assert_normalized_result(&mut store, entry, 4);
}

#[test]
fn shared_conditional_constructor_preserves_bound_locals_through_full_pipeline() {
    let (mut store, entry) = compile_to_fir(SHARED_CONSTRUCTOR);
    share_conditional_constructor_branch(store.get_mut(entry));
    crate::invariants::check(&store, entry, crate::invariants::InvariantLevel::PostMono);

    assert_full_pipeline_succeeds("shared conditional constructor", &mut store, entry);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, entry),
        Ok(Value::Int(4))
    );
}

#[test]
fn shared_constructor_across_calls_snapshots_fresh_values_at_each_invocation() {
    let (mut store, entry) = compile_to_monomorphized_fir(
        r#"
        function Make(x : Int) : Int => Int { y => x + 2 * y }
        operation Main() : Int {
            mutable a = 0;
            Make(a)({ a = 5; 2 }) + Make(a)({ a = 9; 3 })
        }
        "#,
    );
    let package = store.get_mut(entry);
    let calls: Vec<_> = package
        .exprs
        .iter()
        .filter_map(|(id, expr)| {
            if let ExprKind::Call(callee, args) = expr.kind
                && matches!(package.get_expr(callee).kind, ExprKind::Call(..))
            {
                Some((id, callee, args))
            } else {
                None
            }
        })
        .collect();
    let [(_, producer, _), (second, _, args)] = calls[..] else {
        panic!("expected two immediately consumed constructors");
    };
    package
        .exprs
        .get_mut(second)
        .expect("second call exists")
        .kind = ExprKind::Call(producer, args);
    crate::invariants::check_local_var_consistency(package, find_callable(package, "Main"));

    assert_eq!(normalize_scalar_inputs(package).len(), 1);
    crate::invariants::check_local_var_consistency(package, find_callable(package, "Main"));
    assert_normalized_result(&mut store, entry, 15);
}

#[test]
fn immediate_scalar_constructor_snapshots_tuple_leaves_in_order() {
    let (mut store, entry) = compile_to_monomorphized_fir(
        r#"
        function Make(x : Int, y : Int) : Int => Int { z => x + 10 * y + z }
        operation Main() : Int {
            mutable a = 0;
            Make(a, { set a = 3; 2 })({ set a = 9; 1 })
        }
        "#,
    );
    let package = store.get(entry);
    let (producer, args) = package
        .exprs
        .iter()
        .find_map(|(_, expr)| {
            let ExprKind::Call(producer, _) = expr.kind else {
                return None;
            };
            let ExprKind::Call(_, args) = package.get_expr(producer).kind else {
                return None;
            };
            Some((producer, args))
        })
        .expect("immediate constructor call");
    let original_producer = package.get_expr(producer).clone();
    let ExprKind::Tuple(leaves) = &package.get_expr(args).kind else {
        panic!("tuple producer input");
    };
    let leaves = leaves.clone();
    let original_leaves: Vec<_> = leaves
        .iter()
        .map(|&id| package.get_expr(id).clone())
        .collect();

    let initializers = normalize_scalar_inputs(store.get_mut(entry));
    let package = store.get(entry);
    assert_eq!(initializers.len(), 2);
    let normalized_producer = package.get_expr(producer);
    assert_eq!(normalized_producer.ty, original_producer.ty);
    assert_eq!(normalized_producer.span, original_producer.span);
    let ExprKind::Block(block) = normalized_producer.kind else {
        panic!("snapshots must precede the producer at its original evaluation point");
    };
    let statements = &package.get_block(block).stmts;
    assert_eq!(statements.len(), 3);
    let StmtKind::Expr(tail) = package.get_stmt(statements[2]).kind else {
        panic!("producer must follow both snapshots");
    };
    let ExprKind::Call(callee, rewritten_args) = package.get_expr(tail).kind else {
        panic!("block must end in the constructor");
    };
    assert_eq!(original_producer.kind, ExprKind::Call(callee, args));
    let ExprKind::Tuple(rewritten_leaves) = &package.get_expr(rewritten_args).kind else {
        panic!("input must remain a tuple");
    };
    assert_eq!(rewritten_leaves.len(), original_leaves.len());
    assert_eq!(
        package.get_expr(rewritten_args).ty,
        package.get_expr(args).ty
    );
    assert_eq!(
        package.get_expr(rewritten_args).span,
        package.get_expr(args).span
    );
    for (index, original) in original_leaves.iter().enumerate() {
        let StmtKind::Local(Mutability::Immutable, pat, init) =
            package.get_stmt(statements[index]).kind
        else {
            panic!("each operand must have an immutable snapshot");
        };
        assert_eq!(init, initializers[index]);
        let initializer = package.get_expr(init);
        assert_eq!(initializer.kind, original.kind);
        assert_eq!(initializer.ty, original.ty);
        assert_eq!(initializer.span, original.span);
        let PatKind::Bind(binding) = &package.get_pat(pat).kind else {
            panic!("snapshot must bind one leaf");
        };
        assert_eq!(
            package.get_expr(rewritten_leaves[index]).kind,
            ExprKind::Var(Res::Local(binding.id), Vec::new())
        );
        assert_eq!(
            package.get_expr(leaves[index]),
            original,
            "snapshotting must not change other uses of the original operand"
        );
    }
    assert_normalized_result(&mut store, entry, 21);
}

#[test]
fn immediate_scalar_constructor_normalization_is_idempotent_after_callee_rewrite() {
    let (mut store, entry) = compile_to_monomorphized_fir(
        r#"
        function Make(x : Int) : Int => Int { y => x + 2 * y }
        operation Main() : Int {
            mutable a = 0;
            Make(a)(2)
        }
        "#,
    );
    let package = store.get_mut(entry);
    assert_eq!(normalize_scalar_inputs(package).len(), 1);
    let expressions = package.exprs.iter().map(|(id, _)| id).collect();
    let mut assigner = Assigner::from_package(package);
    assert!(super::super::rewrite::normalize_direct_callee_control_flow(
        package,
        expressions,
        &mut assigner
    ));
    let normalized = format!("{package:?}");

    assert!(normalize_scalar_inputs(package).is_empty());
    assert_eq!(format!("{package:?}"), normalized);
}

#[test]
fn conditional_scalar_constructor_normalization_is_idempotent() {
    let (mut store, entry) = compile_to_monomorphized_fir(
        r#"
        function Make(x : Int) : Int => Int { y => x + 2 * y }
        operation Main() : Int {
            use q = Qubit();
            mutable a = 0;
            (if MResetZ(q) == Zero { Make(a) } else { Make(a + 1) })(
                { a = 9; 2 }
            )
        }
        "#,
    );
    let package = store.get_mut(entry);
    assert_eq!(normalize_scalar_inputs(package).len(), 2);
    let normalized = format!("{package:?}");

    assert!(normalize_scalar_inputs(package).is_empty());
    assert_eq!(format!("{package:?}"), normalized);
    assert_normalized_result(&mut store, entry, 4);
}

#[test]
fn immediate_scalar_constructor_with_effectful_producer_is_unchanged() {
    let (mut store, entry) = compile_to_monomorphized_fir(
        r#"
        function Make(x : Int) : Int => Int {
            Message("factory");
            y => x + 2 * y
        }
        operation Main() : Int {
            mutable a = 0;
            Make(a)(2)
        }
        "#,
    );
    let package = store.get_mut(entry);
    let original = format!("{package:?}");

    assert!(normalize_scalar_inputs(package).is_empty());
    assert_eq!(format!("{package:?}"), original);
}

#[test]
fn immediate_scalar_constructor_with_fallible_returned_body_is_unchanged() {
    let (mut store, entry) = compile_to_monomorphized_fir(
        r#"
        function Make(x : Int) : Int => Int { y => x / y }
        operation Main() : Int {
            mutable a = 8;
            Make(a)(2)
        }
        "#,
    );
    let package = store.get_mut(entry);
    let (_, target) = closure_in(package, "Make");
    let ItemKind::Callable(target) = &package.get_item(target).kind else {
        panic!("returned closure target");
    };
    let body = single_body_expression(package, &target.implementation)
        .expect("single-expression returned body");
    assert!(matches!(
        package.get_expr(body).kind,
        ExprKind::BinOp(qsc_fir::fir::BinOp::Div, _, _)
    ));
    let original = format!("{package:?}");

    assert!(normalize_scalar_inputs(package).is_empty());
    assert_eq!(format!("{package:?}"), original);
}

#[test]
fn immediate_scalar_constructor_with_recursive_returned_body_is_unchanged() {
    let (mut store, entry) = compile_to_monomorphized_fir(
        r#"
        operation Recursive(x : Int) : Int { Recursive(x) }
        function Make(x : Int) : Int => Int { y => Recursive(x + y) }
        operation Main() : Int {
            mutable a = 0;
            Make(a)(2)
        }
        "#,
    );
    let package = store.get_mut(entry);
    let original = format!("{package:?}");

    assert!(normalize_scalar_inputs(package).is_empty());
    assert_eq!(format!("{package:?}"), original);
}

fn closure_in(package: &Package, owner: &str) -> (ExprId, LocalItemId) {
    let mut closures = Vec::new();
    for_each_expr_in_callable_impl(
        package,
        &find_callable(package, owner).implementation,
        &mut |id, expr| {
            if let ExprKind::Closure(_, target) = expr.kind {
                closures.push((id, target));
            }
        },
    );
    assert_eq!(closures.len(), 1, "{owner} must contain one closure");
    closures[0]
}

fn target_input(package: &Package, target: LocalItemId) -> Ty {
    let ItemKind::Callable(decl) = &package.get_item(target).kind else {
        panic!("closure target must be callable");
    };
    package.get_pat(decl.input).ty.clone()
}

fn normalize(store: &mut PackageStore, package: PackageId) {
    let mut assigner = Assigner::from_package(store.get(package));
    normalize_closure_environments(store, package, &mut assigner);
}

fn add_direct_reference(
    store: &mut PackageStore,
    target: qsc_fir::fir::StoreItemId,
    caller_package: PackageId,
    caller_name: &str,
) {
    let package = store.get(target.package);
    let ItemKind::Callable(decl) = &package.get_item(target.item).kind else {
        panic!("target must be callable");
    };
    let ty = Ty::Arrow(Box::new(Arrow {
        kind: decl.kind,
        input: Box::new(package.get_pat(decl.input).ty.clone()),
        output: Box::new(decl.output.clone()),
        functors: FunctorSet::Value(decl.functors),
    }));
    let caller = find_callable_body_block(store.get(caller_package), caller_name);
    let mut assigner = Assigner::from_package(store.get(caller_package));
    let package = store.get_mut(caller_package);
    let reference = alloc_item_var_expr(
        package,
        &mut assigner,
        ItemId {
            package: target.package,
            item: target.item,
        },
        ty,
        package.synthetic_span(),
    );
    let statement = alloc_semi_stmt(package, &mut assigner, reference, package.synthetic_span());
    package
        .blocks
        .get_mut(caller)
        .expect("caller exists")
        .stmts
        .insert(0, statement);
}

fn assert_normalized_result(store: &mut PackageStore, entry: PackageId, expected: i64) {
    crate::exec_graph_rebuild::rebuild_exec_graphs(store, entry, &[]);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(store, entry),
        Ok(Value::Int(expected))
    );
}

/// A scalar sibling and callable leaf become separate capture parameters.
/// Assert the target layout and the occurrence's immutable projections, not
/// a generated binding name or successful execution of the full pipeline.
#[test]
fn tuple_capture_expands_target_and_occurrence_in_field_order() {
    let source = format!(
        "{FACTORY}
        function Add3(value : Int) : Int {{ value + 3 }}
        operation Main() : Int {{ Make((7, Add3))(2) }}"
    );
    let (mut store, package_id) = compile_to_monomorphized_fir(&source);
    let (occurrence, target) = closure_in(store.get(package_id), "Make");
    let original = store.get(package_id).get_expr(occurrence).clone();
    assert_eq!(
        target_input(store.get(package_id), target).to_string(),
        "((Int, (Int -> Int)), Int)"
    );

    normalize(&mut store, package_id);
    let package = store.get(package_id);
    assert_eq!(
        target_input(package, target).to_string(),
        "(Int, (Int -> Int), Int)"
    );
    let expr = package.get_expr(occurrence);
    assert_eq!(expr.ty, original.ty);
    assert_eq!(expr.span, original.span);
    let ExprKind::Block(block) = expr.kind else {
        panic!("capture projections must execute at closure creation");
    };
    let [first, second, tail] = package.get_block(block).stmts[..] else {
        panic!("expected two leaf bindings followed by the closure");
    };
    let mut leaves = Vec::new();
    for (index, statement) in [first, second].into_iter().enumerate() {
        let StmtKind::Local(Mutability::Immutable, pattern, value) =
            package.get_stmt(statement).kind
        else {
            panic!("each projected capture must be an immutable binding");
        };
        let PatKind::Bind(ident) = &package.get_pat(pattern).kind else {
            panic!("projection must bind a leaf");
        };
        let ExprKind::Field(_, qsc_fir::fir::Field::Path(path)) = &package.get_expr(value).kind
        else {
            panic!("leaf must project the original environment");
        };
        assert_eq!(path.indices, [index]);
        leaves.push(ident.id);
    }
    let StmtKind::Expr(closure) = package.get_stmt(tail).kind else {
        panic!("block must end in the normalized closure");
    };
    assert_eq!(
        package.get_expr(closure).kind,
        ExprKind::Closure(leaves, target)
    );

    let normalized = format!("{package:?}");
    normalize(&mut store, package_id);
    assert_eq!(
        format!("{:?}", store.get(package_id)),
        normalized,
        "rerunning normalization must not allocate or rewrite again"
    );
}

/// A direct reference has no capture list to expand alongside the target input.
/// References in a different package must block that target's rewrite too,
/// without preventing an unrelated eligible target in the same package.
#[test]
fn direct_item_references_preserve_target_layout_across_packages() {
    for foreign_reference in [false, true] {
        let library = format!(
            "namespace Lib {{
                {FACTORY}
                function Other(environment : (Int, Int -> Int)) : Int -> Int {{
                    value -> {{
                        let (bias, action) = environment;
                        bias + action(value)
                    }}
                }}
                export Make, Other;
            }}"
        );
        let (mut store, entry) = compile_to_fir_with_library(
            &library,
            "function Add3(value : Int) : Int { value + 3 }
             operation Main() : Int { Lib.Make((7, Add3))(2) + Lib.Other((7, Add3))(2) }",
        );
        let owner = find_library_callable(&store, entry, "Make").package;
        let (occurrence, target) = closure_in(store.get(owner), "Make");
        let (_, other_target) = closure_in(store.get(owner), "Other");
        let original_input = target_input(store.get(owner), target);
        let original_closure = store.get(owner).get_expr(occurrence).clone();
        let (caller_package, caller_name) = if foreign_reference {
            (entry, "Main")
        } else {
            (owner, "Make")
        };
        add_direct_reference(
            &mut store,
            (owner, target).into(),
            caller_package,
            caller_name,
        );

        normalize(&mut store, owner);
        assert_eq!(
            target_input(store.get(owner), target),
            original_input,
            "foreign reference: {foreign_reference}"
        );
        assert_eq!(
            store.get(owner).get_expr(occurrence).kind,
            original_closure.kind,
            "blocked targets must retain their original occurrences"
        );
        assert_eq!(
            target_input(store.get(owner), other_target).to_string(),
            "(Int, (Int -> Int), Int)",
            "the unrelated target must still expand"
        );
        assert_normalized_result(&mut store, entry, 24);
    }
}

/// Unit contributes no leaves and singleton tuples retain their shape when
/// reconstructed. Arrays stay opaque even when their elements are callables.
#[test]
fn aggregate_capture_expands_callable_leaves_and_preserves_opaque_shapes() {
    for (declaration, ty, value, body, expected_layout, eligible) in [
        (
            "",
            "(Unit, (Int -> Int,), Int)",
            "((), (Add3,), 7)",
            "let (_, (action,), bias) = environment; bias + action(value)",
            "((Int -> Int), Int, Int)",
            true,
        ),
        (
            "",
            "(Int[], Int -> Int)",
            "([7], Add3)",
            "let (values, action) = environment; values[0] + action(value)",
            "((Int)[], (Int -> Int), Int)",
            true,
        ),
        (
            "",
            "(Int, Int[])",
            "(7, [3])",
            "let (bias, values) = environment; bias + values[0] + value",
            "((Int, (Int)[]), Int)",
            false,
        ),
        (
            "",
            "(Int -> Int)[]",
            "[Add3]",
            "environment[0](value) + 7",
            "(((Int -> Int))[], Int)",
            false,
        ),
        (
            "",
            "((Int -> Int)[], Int -> Int)",
            "([Add3], Add3)",
            "let (actions, action) = environment; actions[0](value) + action(value) + 2",
            "(((Int -> Int))[], (Int -> Int), Int)",
            true,
        ),
        (
            "newtype Action = (Int -> Int); newtype Env = (Bias : Int, Action : Action);",
            "Env",
            "Env(7, Action(Add3))",
            "let action = (environment::Action)!; environment::Bias + action(value)",
            "(Int, (Int -> Int), Int)",
            true,
        ),
    ] {
        let source = format!(
            "{declaration}
                 function Add3(value : Int) : Int {{ value + 3 }}
                 function Evaluate(environment : {ty}, value : Int) : Int {{ {body} }}
                 function Make(environment : {ty}) : Int -> Int {{
                     value -> Evaluate(environment, value)
                 }}
                 operation Main() : Int {{ Make({value})(2) }}"
        );
        let (mut store, entry) = compile_to_monomorphized_fir(&source);
        let (occurrence, target) = closure_in(store.get(entry), "Make");
        let before = format!("{:?}", store.get(entry));
        normalize(&mut store, entry);
        assert_eq!(
            target_input(store.get(entry), target).to_string(),
            expected_layout,
            "{ty}"
        );
        assert_eq!(
            matches!(
                store.get(entry).get_expr(occurrence).kind,
                ExprKind::Block(_)
            ),
            eligible,
            "{ty}"
        );
        if !eligible {
            assert_eq!(format!("{:?}", store.get(entry)), before);
        }
        assert_normalized_result(&mut store, entry, 12);
    }
}

/// Reachable and unreachable owners share a lifted target. Changing its input
/// must update every occurrence, with independent capture-time projections.
#[test]
fn shared_target_occurrences_keep_distinct_capture_values() {
    let source = format!(
        "{FACTORY}
             function Add3(value : Int) : Int {{ value + 3 }}
             function Unused() : Int -> Int {{
                 let environment = (30, Add3);
                 value -> {{ let (bias, action) = environment; bias + action(value) }}
             }}
             operation Main() : Int {{
                 mutable environment = (7, Add3);
                 let snapshot = environment;
                 let first = value -> {{ let (bias, action) = snapshot; bias + action(value) }};
                 set environment = (20, Add3);
                 let second = Make(environment);
                 set environment = (100, Add3);
                 first(2) * 100 + second(2)
             }}"
    );
    let (mut store, entry) = compile_to_monomorphized_fir(&source);
    let (first, first_target) = closure_in(store.get(entry), "Main");
    let (second, target) = closure_in(store.get(entry), "Make");
    let (third, third_target) = closure_in(store.get(entry), "Unused");
    // These bodies implement the same expression. Share the target explicitly,
    // as generated callable copies can, without relying on lambda deduplication.
    let package = store.get_mut(entry);
    for (occurrence, original_target) in [(first, first_target), (third, third_target)] {
        let ExprKind::Closure(captures, _) = package.get_expr(occurrence).kind.clone() else {
            panic!("expected closure");
        };
        assert_eq!(
            target_input(package, original_target),
            target_input(package, target)
        );
        package
            .exprs
            .get_mut(occurrence)
            .expect("closure exists")
            .kind = ExprKind::Closure(captures, target);
    }
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, entry),
        Ok(Value::Int(1225))
    );

    normalize(&mut store, entry);
    let package = store.get(entry);
    assert_eq!(
        target_input(package, target).to_string(),
        "(Int, (Int -> Int), Int)"
    );
    let captures: Vec<_> = [first, second, third]
        .map(|occurrence| {
            let ExprKind::Block(block) = package.get_expr(occurrence).kind else {
                panic!("every occurrence must project its capture");
            };
            let tail = *package.get_block(block).stmts.last().expect("tail exists");
            let StmtKind::Expr(expr) = package.get_stmt(tail).kind else {
                panic!("expected expression tail");
            };
            let ExprKind::Closure(captures, actual_target) = &package.get_expr(expr).kind else {
                panic!("expected closure tail");
            };
            assert_eq!(*actual_target, target);
            assert_eq!(captures.len(), 2);
            captures.clone()
        })
        .into();
    let unique: FxHashSet<_> = captures.iter().flatten().collect();
    assert_eq!(
        unique.len(),
        6,
        "each occurrence must own its two projected locals"
    );
    assert_normalized_result(&mut store, entry, 1225);
}

/// Expanding an outer environment removes its old local. Nested closures must
/// recapture the new input leaves, not project from that removed local.
#[test]
fn nested_recapture_uses_the_enclosing_targets_new_leaf_parameters() {
    let source = r#"
            function Add3(value : Int) : Int { value + 3 }
            function Make(environment : (Int, Int -> Int)) : Unit -> Int {
                () -> {
                    let inner = value -> {
                        let (bias, action) = environment;
                        bias + action(value)
                    };
                    inner(2)
                }
            }
            operation Main() : Int { Make((7, Add3))() }
        "#;
    let (mut store, entry) = compile_to_monomorphized_fir(source);
    let (_, outer) = closure_in(store.get(entry), "Make");
    let ItemKind::Callable(decl) = &store.get(entry).get_item(outer).kind else {
        panic!("outer target must be callable");
    };
    let (inner_expr, inner) = closure_in(store.get(entry), &decl.name.name);
    normalize(&mut store, entry);
    let package = store.get(entry);
    assert_eq!(
        target_input(package, outer).to_string(),
        "(Int, (Int -> Int), Unit)"
    );
    assert_eq!(
        target_input(package, inner).to_string(),
        "(Int, (Int -> Int), Int)"
    );
    let ItemKind::Callable(decl) = &package.get_item(outer).kind else {
        panic!("outer target must be callable");
    };
    let PatKind::Tuple(inputs) = &package.get_pat(decl.input).kind else {
        panic!("normalized input must be tuple");
    };
    let leaves: Vec<_> = inputs[..2]
        .iter()
        .map(|input| {
            let PatKind::Bind(ident) = &package.get_pat(*input).kind else {
                panic!("capture leaf must bind a local");
            };
            ident.id
        })
        .collect();
    assert_eq!(
        package.get_expr(inner_expr).kind,
        ExprKind::Closure(leaves, inner)
    );
    assert_normalized_result(&mut store, entry, 12);
}

/// Blocking the innermost target must propagate outward to a fixed point.
/// Partially expanding either enclosing capture would leave a nested closure
/// referring to an aggregate local that no longer exists.
#[test]
fn incompatible_nested_recapture_preserves_all_enclosing_layouts() {
    let source = r#"
        function Add3(value : Int) : Int { value + 3 }
        function Make(environment : (Int, Int -> Int)) : Unit -> Int {
            () -> {
                let middle = delta -> {
                    let inner = value -> {
                        let (bias, action) = environment;
                        bias + action(value)
                    };
                    inner(2) + delta
                };
                middle(1)
            }
        }
        operation Main() : Int { Make((7, Add3))() }
    "#;
    let (mut store, entry) = compile_to_monomorphized_fir(source);
    let mut owner = "Make".to_string();
    let mut targets = Vec::new();
    for _ in 0..3 {
        let (_, target) = closure_in(store.get(entry), &owner);
        targets.push(target);
        let ItemKind::Callable(decl) = &store.get(entry).get_item(target).kind else {
            panic!("closure target must be callable");
        };
        owner = decl.name.name.to_string();
    }
    add_direct_reference(&mut store, (entry, targets[2]).into(), entry, "Main");
    let before = format!("{:?}", store.get(entry));
    normalize(&mut store, entry);
    assert_eq!(format!("{:?}", store.get(entry)), before);
    assert_normalized_result(&mut store, entry, 13);
}

/// A foreign UDT stays opaque when its tuple capture is expanded. Its nominal
/// identity must still refer to the original declaring package.
#[test]
fn expanded_capture_preserves_foreign_udt_identity_as_one_leaf() {
    let (mut store, entry) = compile_to_fir_with_library(
        "namespace Lib { newtype Data = (Bias : Int, Offset : Int); export Data; }",
        r#"
        function Add3(value : Int) : Int { value + 3 }
        function Make(environment : (Lib.Data, Int -> Int)) : Int -> Int {
            value -> {
                let (data, action) = environment;
                data::Bias + data::Offset + action(value)
            }
        }
        operation Main() : Int { Make((Lib.Data(4, 3), Add3))(2) }
        "#,
    );
    let (_, target) = closure_in(store.get(entry), "Make");
    let Ty::Tuple(original_input) = target_input(store.get(entry), target) else {
        panic!("lifted input tuple");
    };
    let Ty::Tuple(environment) = &original_input[0] else {
        panic!("aggregate capture");
    };
    let Ty::Udt(Res::Item(foreign)) = environment[0] else {
        panic!("nominal leaf");
    };
    assert_ne!(foreign.package, entry);
    let expected = Ty::Tuple(vec![
        environment[0].clone(),
        environment[1].clone(),
        original_input[1].clone(),
    ]);
    normalize(&mut store, entry);
    assert_eq!(target_input(store.get(entry), target), expected);
    assert_normalized_result(&mut store, entry, 12);
}

/// Expand two aggregates without moving the scalar, array, or ordinary argument
/// slots between them. Different weights make a misplaced capture observable.
#[test]
fn multiple_aggregate_captures_preserve_interleaved_slots_and_argument_order() {
    let source = r#"
        function Add4(value : Int) : Int { value + 4 }
        function Times8(value : Int) : Int { value * 8 }
        function Make(
            prefix : Int,
            first : (Int, Int -> Int),
            opaque : Int[],
            second : (Int, Int -> Int),
            suffix : Int
        ) : Int -> Int {
            value -> {
                let (firstBias, firstAction) = first;
                let (secondBias, secondAction) = second;
                prefix * 100000 + firstBias * 10000 + firstAction(value) * 1000
                    + opaque[0] * 100 + secondBias * 10 + secondAction(value) + suffix
            }
        }
        operation Main() : Int {
            Make(2, (3, Add4), [5], (7, Times8), 11)(1)
        }
    "#;
    let (mut store, entry) = compile_to_monomorphized_fir(source);
    let (occurrence, target) = closure_in(store.get(entry), "Make");
    let package = store.get(entry);
    let ExprKind::Closure(captures, _) = &package.get_expr(occurrence).kind else {
        panic!("expected closure");
    };
    let original_captures = captures.clone();
    assert_eq!(original_captures.len(), 5);
    let ItemKind::Callable(decl) = &package.get_item(target).kind else {
        panic!("expected callable target");
    };
    let input_id = decl.input;
    let PatKind::Tuple(inputs) = &package.get_pat(input_id).kind else {
        panic!("expected lifted tuple input");
    };
    let original_inputs = inputs.clone();
    assert_eq!(
        target_input(package, target).to_string(),
        "(Int, (Int, (Int -> Int)), (Int)[], (Int, (Int -> Int)), Int, Int)"
    );

    normalize(&mut store, entry);
    let package = store.get(entry);
    assert_eq!(
        target_input(package, target).to_string(),
        "(Int, Int, (Int -> Int), (Int)[], Int, (Int -> Int), Int, Int)"
    );
    let PatKind::Tuple(inputs) = &package.get_pat(input_id).kind else {
        panic!("normalized input must remain a tuple");
    };
    for (old, new) in [(0, 0), (2, 3), (4, 6), (5, 7)] {
        assert_eq!(
            inputs[new], original_inputs[old],
            "unchanged input slot {old}"
        );
    }
    let ExprKind::Block(block) = package.get_expr(occurrence).kind else {
        panic!("expanded captures must be bound at closure creation");
    };
    let statements = &package.get_block(block).stmts;
    assert_eq!(statements.len(), 5, "four projections and one closure tail");
    let StmtKind::Expr(tail) = package.get_stmt(statements[4]).kind else {
        panic!("expected closure tail");
    };
    let ExprKind::Closure(captures, actual_target) = &package.get_expr(tail).kind else {
        panic!("expected normalized closure");
    };
    assert_eq!(*actual_target, target);
    assert_eq!(captures.len(), 7);
    for (old, new) in [(0, 0), (2, 3), (4, 6)] {
        assert_eq!(
            captures[new], original_captures[old],
            "unchanged capture {old}"
        );
    }
    assert_normalized_result(&mut store, entry, 235_589);
}
