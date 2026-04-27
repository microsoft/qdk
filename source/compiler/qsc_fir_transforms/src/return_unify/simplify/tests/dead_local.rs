// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for [`crate::return_unify::simplify::dead_local`].
//!
//! The suite is split three ways:
//!
//! * Q#-driven rule tests use [`check_simplify_rule_q`]: a Q# snippet is
//!   compiled, the pipeline runs through mono + return-unify-without-
//!   simplify, the pre-simplify FIR is snapshotted, `dead_local::apply`
//!   is applied to `Main`'s body block, and the post-rule FIR is
//!   snapshotted. The before/after snapshots pin the rule's effect
//!   against what the lowerer actually emits, so the test inputs cannot
//!   drift from the canonical user-binding shape.
//! * Direct-FIR matcher-discipline pins cover shapes that normalize +
//!   `transform_block_with_flags` does not reliably emit on its own —
//!   the dead-local rule normally runs inside a fixpoint loop after
//!   sibling rules collapse the surrounding merge, so direct
//!   construction is the only way to exercise these matcher branches
//!   in isolation.
//!
//! Positive cases (rule must fire):
//!
//! 1. Immutable `let _x = 7;` with no downstream reader — dropped (Q#).
//! 2. Mutable `mutable _x = 7;` with no downstream reader — dropped
//!    (Q#; mutability is unconstrained when the init is pure and the
//!    local is unused).
//! 3. Preserved Local with a synthesized default-value initializer:
//!    direct-FIR pin for the shape the normalize pre-pass emits when
//!    it preserves a user binding whose original init was hoisted out
//!    for return-unification.
//!
//! Negative cases (rule must not fire):
//!
//! 1. Tuple-bind pattern (`let (_a, _b) = (1, 2);`) — the matcher
//!    rejects non-Bind patterns regardless of downstream use (Q#).
//! 2. Call initializer (`let _x = Helper();`) — the side-effect-free
//!    check rejects `ExprKind::Call` (Q#).
//! 3. Closure capture downstream — direct-FIR pin for the
//!    `ExprKind::Closure` matcher path in `local_use_count`. Mono
//!    routinely lifts closures, so the raw Closure expression is not
//!    reliably reachable from Q# at the simplify stage.

use expect_test::expect;
use indoc::indoc;
use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{CallableKind, ExprKind, Lit, LocalItemId, Mutability, Package, PackageLookup, StmtId},
    ty::{Arrow, FunctorSet, FunctorSetValue, Prim, Ty},
};

use crate::fir_builder::{
    alloc_block, alloc_expr, alloc_expr_stmt, alloc_local_var, alloc_semi_stmt,
};
use crate::return_unify::simplify::dead_local::{self, eligible_local_binding};
use crate::return_unify::tests::check_simplify_rule_q;

/// Allocate an `Int` literal `ExprId`.
fn int_lit(package: &mut Package, assigner: &mut Assigner, value: i64) -> qsc_fir::fir::ExprId {
    alloc_expr(
        package,
        assigner,
        Ty::Prim(Prim::Int),
        ExprKind::Lit(Lit::Int(value)),
        Span::default(),
    )
}

/// Allocate a trailing `Expr(Int)` literal statement.
fn trailing_int(package: &mut Package, assigner: &mut Assigner, value: i64) -> StmtId {
    let lit = int_lit(package, assigner, value);
    alloc_expr_stmt(package, assigner, lit, Span::default())
}

#[test]
fn given_immutable_unused_let_with_literal_init_dead_local_drops_binding() {
    // Q# input: `let _x = 7; 42`. The lowerer preserves the binding
    // (the `_` prefix only suppresses unused-warning lints) and the
    // dead-local rule must drop it because the init is a literal and
    // the local has no downstream uses.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let _x = 7;
                42
            }
        }
        "#},
        "Main",
        "dead_local",
        |p, a, b, _| dead_local::apply(p, a, b),
        &expect![[r#"
            // before dead_local (fired=true)
            // namespace Test
            function Main() : Int {
                let _x : Int = 7;
                42
            }
            // entry
            Main()

            // after dead_local
            // namespace Test
            function Main() : Int {
                42
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn given_mutable_unused_let_with_literal_init_dead_local_drops_binding() {
    // Q# input: `mutable _x = 7; 42`. The rule must drop the binding
    // even though it was declared mutable — mutability is irrelevant
    // when the local has no downstream uses and the init is pure.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                mutable _x = 7;
                42
            }
        }
        "#},
        "Main",
        "dead_local",
        |p, a, b, _| dead_local::apply(p, a, b),
        &expect![[r#"
            // before dead_local (fired=true)
            // namespace Test
            function Main() : Int {
                mutable _x : Int = 7;
                42
            }
            // entry
            Main()

            // after dead_local
            // namespace Test
            function Main() : Int {
                42
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn given_preserved_local_with_default_init_dead_local_drops_binding() {
    // MANUAL-FIR fixture: this shape mimics the normalize preserved-
    // Local emit (a user-bound name reused with a synthesized
    // default-value init), which surfaces only after sibling rules
    // fold the shape the binding was reserving. Direct construction
    // pins the rule's local invariants on the preserved-binding
    // branch independently of the dispatch oracle and the other
    // catalogue rules.
    //
    // Block shape:
    //   let result : Int = 0;     // user name preserved with default-value init
    //   42
    // The default-value init is a literal (Int's default is 0), which
    // the side-effect-free check accepts. The rule must drop the
    // binding.
    let mut package = Package::default();
    let mut assigner = Assigner::default();

    let init = int_lit(&mut package, &mut assigner, 0);
    let (_result, decl) = alloc_local_var(
        &mut package,
        &mut assigner,
        "result",
        &Ty::Prim(Prim::Int),
        init,
        Mutability::Immutable,
    );
    let tail = trailing_int(&mut package, &mut assigner, 42);
    let block = alloc_block(
        &mut package,
        &mut assigner,
        vec![decl, tail],
        Ty::Prim(Prim::Int),
        Span::default(),
    );

    let fired = dead_local::apply(&mut package, &mut assigner, block);
    assert!(
        fired,
        "dead_local must drop the preserved user binding with a default-value init",
    );
    assert_eq!(
        package.get_block(block).stmts.len(),
        1,
        "block should retain only the trailing literal",
    );
}

#[test]
fn given_tuple_bind_dead_local_does_not_drop() {
    // Q# input: `let (_a, _b) = (1, 2); 42`. The lowerer keeps the
    // tuple-bind pattern, so `eligible_local_binding` rejects the
    // statement (it only matches single-Bind Locals) and the rule
    // must not fire even though both tuple elements are unused.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let (_a, _b) = (1, 2);
                42
            }
        }
        "#},
        "Main",
        "dead_local",
        |p, a, b, _| dead_local::apply(p, a, b),
        &expect![[r#"
            // before dead_local (fired=false)
            // namespace Test
            function Main() : Int {
                let (_a : Int, _b : Int) = (1, 2);
                42
            }
            // entry
            Main()

            // after dead_local
            // namespace Test
            function Main() : Int {
                let (_a : Int, _b : Int) = (1, 2);
                42
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn given_call_init_dead_local_does_not_drop() {
    // Q# input: `let _x = Helper(); 42`. The initializer is a call
    // expression; the side-effect-free check rejects `ExprKind::Call`
    // and the rule must not drop the binding even though `_x` is
    // unused.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Helper() : Int {
                0
            }
            function Main() : Int {
                let _x = Helper();
                42
            }
        }
        "#},
        "Main",
        "dead_local",
        |p, a, b, _| dead_local::apply(p, a, b),
        &expect![[r#"
            // before dead_local (fired=false)
            // namespace Test
            function Helper() : Int {
                0
            }
            function Main() : Int {
                let _x : Int = Helper();
                42
            }
            // entry
            Main()

            // after dead_local
            // namespace Test
            function Helper() : Int {
                0
            }
            function Main() : Int {
                let _x : Int = Helper();
                42
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn given_local_used_in_closure_capture_dead_local_does_not_drop() {
    // MANUAL-FIR fixture: mono routinely lifts closures into top-level
    // callables, so the raw `ExprKind::Closure` capture shape is not
    // reliably reachable from Q# at the simplify stage. Direct
    // construction pins the matcher path in `local_use_count` that
    // walks closure capture lists.
    //
    // Block shape:
    //   let x : Int = 7;
    //   <semi-discarded closure that captures x>;
    //   42
    // Even though the closure construction is itself pure and the
    // surrounding stmt is a Semi that discards the closure value,
    // local_use_count walks the Closure expression's capture list and
    // counts x. The rule must therefore refuse to drop the binding.
    let mut package = Package::default();
    let mut assigner = Assigner::default();

    let init = int_lit(&mut package, &mut assigner, 7);
    let (x_local, decl) = alloc_local_var(
        &mut package,
        &mut assigner,
        "x",
        &Ty::Prim(Prim::Int),
        init,
        Mutability::Immutable,
    );

    let closure_ty = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Function,
        input: Box::new(Ty::UNIT),
        output: Box::new(Ty::Prim(Prim::Int)),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }));
    let closure_expr = alloc_expr(
        &mut package,
        &mut assigner,
        closure_ty,
        ExprKind::Closure(vec![x_local], LocalItemId::from(0)),
        Span::default(),
    );
    let semi = alloc_semi_stmt(&mut package, &mut assigner, closure_expr, Span::default());

    let tail = trailing_int(&mut package, &mut assigner, 42);
    let block = alloc_block(
        &mut package,
        &mut assigner,
        vec![decl, semi, tail],
        Ty::Prim(Prim::Int),
        Span::default(),
    );

    let fired = dead_local::apply(&mut package, &mut assigner, block);
    assert!(
        !fired,
        "dead_local must not drop a binding whose local is captured by a downstream closure",
    );
    assert_eq!(
        package.get_block(block).stmts.len(),
        3,
        "block should retain all three statements",
    );
}

#[test]
fn given_var_eligibility_extracts_local_id() {
    // eligible_local_binding returns Some for a single-bind Local.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let init = int_lit(&mut package, &mut assigner, 0);
    let (local, decl) = alloc_local_var(
        &mut package,
        &mut assigner,
        "x",
        &Ty::Prim(Prim::Int),
        init,
        Mutability::Immutable,
    );
    let got = eligible_local_binding(&package, decl);
    let (got_local, got_init) = got.expect("eligible_local_binding should match single-bind");
    assert_eq!(got_local, local);
    assert_eq!(got_init, init);
}
