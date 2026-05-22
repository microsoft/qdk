// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for [`crate::return_unify::simplify::bare_return`].
//!
//! Positive cases use [`check_simplify_rule_q`]: a Q# snippet is
//! compiled, the pipeline runs through mono + return-unify-without-
//! simplify, the pre-simplify FIR is snapshotted, the rule is applied
//! to the named callable's body block, and the post-rule FIR is
//! snapshotted. This pins the rule's effect against what the lowerer
//! actually emits, so the test inputs cannot drift from the canonical
//! flag-lowering output shape.
//!
//! Negative cases stay as direct-FIR construction. These pin matcher
//! discipline against shapes that normalize + `transform_block_with_flags`
//! never produces today — they exist so future lowering bugs that emit
//! malformed FIR are still rejected by the rule.
//!
//! Positive cases (rule must fire):
//!
//! 1. Canonical literal-valued bare return — the merge collapses to the
//!    literal expression id.
//! 2. Bare return whose value is a non-trivial call expression — the
//!    rule still fires; the call is reused as-is.
//! 3. Flat 2-semi terminal pair (rather than the nested-block form).
//!    Retained as direct-FIR because the lowerer normally emits the
//!    nested-block form, so the flat shape is not reachable via Q#.
//! 4. Single-body bare-return shape — the only statement is a `return`,
//!    so the body collapses to the slot RHS.
//! 5. Single-body bare-return shape with a side-effect-free user prefix
//!    — the prefix is preserved and the merge collapses to the slot RHS.
//!
//! Negative cases (rule must not fire):
//!
//! 1. A pre-stmt reads `__has_returned` — the safety net refuses.
//! 2. The terminal pair is missing the flag set (broken shape).
//! 3. A pre-stmt writes the slot through the slot's local id.
//! 4. The terminal pair's nested block carries an extra leading
//!    statement (3-stmt inner block instead of the canonical 2-Semi).
//! 5. The merge's then-arm reads a local other than `__ret_val`.

use expect_test::expect;
use indoc::indoc;
use qsc_data_structures::span::Span;
use qsc_fir::{
    assigner::Assigner,
    fir::{ExprId, ExprKind, Lit, LocalVarId, Package, PackageLookup, StmtId, StmtKind},
    ty::{Prim, Ty},
};

use crate::fir_builder::{
    alloc_assign_expr, alloc_block, alloc_block_expr, alloc_bool_lit, alloc_expr, alloc_expr_stmt,
    alloc_if_expr, alloc_local_var_expr, alloc_semi_stmt,
};
use crate::return_unify::simplify::bare_return;
use crate::return_unify::tests::check_simplify_rule_q;

/// Slot identities shared by every test fixture.
struct Slots {
    has_returned: LocalVarId,
    ret_val: LocalVarId,
}

/// Allocate `__has_returned : Bool` and `__ret_val : T` local var ids.
fn alloc_slots(assigner: &mut Assigner) -> Slots {
    Slots {
        has_returned: assigner.next_local(),
        ret_val: assigner.next_local(),
    }
}

/// Build the canonical merge expression
/// `if __has_returned { __ret_val } else { fallthrough }` and return
/// its enclosing `Expr` statement id.
fn build_merge_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    slots: &Slots,
    fallthrough: ExprId,
    return_ty: &Ty,
) -> StmtId {
    let cond = alloc_local_var_expr(
        package,
        assigner,
        slots.has_returned,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );
    let then_var = alloc_local_var_expr(
        package,
        assigner,
        slots.ret_val,
        return_ty.clone(),
        Span::default(),
    );
    let then_stmt = alloc_expr_stmt(package, assigner, then_var, Span::default());
    let then_bid = alloc_block(
        package,
        assigner,
        vec![then_stmt],
        return_ty.clone(),
        Span::default(),
    );
    let then_expr = alloc_block_expr(
        package,
        assigner,
        then_bid,
        return_ty.clone(),
        Span::default(),
    );
    let merge = alloc_if_expr(
        package,
        assigner,
        cond,
        then_expr,
        Some(fallthrough),
        return_ty.clone(),
        Span::default(),
    );
    alloc_expr_stmt(package, assigner, merge, Span::default())
}

/// Build a `__ret_val = v;` Semi statement.
fn build_slot_assign_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    slots: &Slots,
    v_id: ExprId,
    return_ty: &Ty,
) -> StmtId {
    let lhs = alloc_local_var_expr(
        package,
        assigner,
        slots.ret_val,
        return_ty.clone(),
        Span::default(),
    );
    let assign = alloc_assign_expr(package, assigner, lhs, v_id, Span::default());
    alloc_semi_stmt(package, assigner, assign, Span::default())
}

/// Build a `__has_returned = true;` Semi statement.
fn build_flag_set_stmt(package: &mut Package, assigner: &mut Assigner, slots: &Slots) -> StmtId {
    let bool_ty = Ty::Prim(Prim::Bool);
    let lhs = alloc_local_var_expr(
        package,
        assigner,
        slots.has_returned,
        bool_ty.clone(),
        Span::default(),
    );
    let rhs = alloc_bool_lit(package, assigner, true, Span::default());
    let assign = alloc_assign_expr(package, assigner, lhs, rhs, Span::default());
    alloc_semi_stmt(package, assigner, assign, Span::default())
}

/// Build the nested-block terminal pair
/// `Semi(Block([Semi(slot_assign), Semi(flag_assign)]))`.
fn build_nested_pair_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    slots: &Slots,
    v_id: ExprId,
    return_ty: &Ty,
) -> StmtId {
    let slot_stmt = build_slot_assign_stmt(package, assigner, slots, v_id, return_ty);
    let flag_stmt = build_flag_set_stmt(package, assigner, slots);
    let inner_bid = alloc_block(
        package,
        assigner,
        vec![slot_stmt, flag_stmt],
        Ty::UNIT,
        Span::default(),
    );
    let inner_expr = alloc_block_expr(package, assigner, inner_bid, Ty::UNIT, Span::default());
    alloc_semi_stmt(package, assigner, inner_expr, Span::default())
}

/// Build an arbitrary `__ret_val` fallthrough expression of the given
/// type. Used as the merge's else arm in every fixture; its exact value
/// is irrelevant because the rule replaces the merge with `v` when it
/// fires.
fn build_fallthrough(
    package: &mut Package,
    assigner: &mut Assigner,
    slots: &Slots,
    return_ty: &Ty,
) -> ExprId {
    alloc_local_var_expr(
        package,
        assigner,
        slots.ret_val,
        return_ty.clone(),
        Span::default(),
    )
}

#[test]
fn canonical_literal_bare_return_collapses() {
    // Compiled from a single `return 42;` body. The lowerer emits the
    // canonical nested-block terminal pair plus a `__has_returned`
    // merge, which `bare_return` must collapse to the literal `42`.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                return 42;
            }
        }
        "#},
        "Main",
        "bare_return",
        bare_return::apply,
        &expect![[r#"
            // before bare_return (fired=true)
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    {
                        __ret_val = 42;
                        __has_returned = true;
                    };
                    if __has_returned __ret_val else __ret_val
                }
            }
            // entry
            Main()

            // after bare_return
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    42
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn bare_return_with_call_value_collapses() {
    // Compiled from a `return Helper();` body. The slot RHS is a Call
    // expression, exercising non-trivial RHS reuse.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Helper() : Int {
                0
            }
            function Main() : Int {
                return Helper();
            }
        }
        "#},
        "Main",
        "bare_return",
        bare_return::apply,
        &expect![[r#"
            // before bare_return (fired=true)
            // namespace Test
            function Helper() : Int {
                body {
                    0
                }
            }
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    {
                        __ret_val = Helper();
                        __has_returned = true;
                    };
                    if __has_returned __ret_val else __ret_val
                }
            }
            // entry
            Main()

            // after bare_return
            // namespace Test
            function Helper() : Int {
                body {
                    0
                }
            }
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    Helper()
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn flat_two_semi_pair_collapses() {
    // MANUAL-FIR fixture: the flat 2-Semi terminal pair shape is not
    // produced by the lowerer (normalize + `transform_block_with_flags`
    // emit the nested-block form), so this case is not reachable via
    // Q#. It pins matcher discipline against future lowering bugs that
    // would emit the flat form.
    //
    // Pattern (flat form):
    //   __ret_val = 7;
    //   __has_returned = true;
    //   if __has_returned { __ret_val } else { __ret_val }
    // After:
    //   7
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let int_ty = Ty::Prim(Prim::Int);

    let v_id = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(7)),
        Span::default(),
    );
    let slot_stmt = build_slot_assign_stmt(&mut package, &mut assigner, &slots, v_id, &int_ty);
    let flag_stmt = build_flag_set_stmt(&mut package, &mut assigner, &slots);
    let fallthrough = build_fallthrough(&mut package, &mut assigner, &slots, &int_ty);
    let merge_stmt = build_merge_stmt(&mut package, &mut assigner, &slots, fallthrough, &int_ty);
    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![slot_stmt, flag_stmt, merge_stmt],
        int_ty.clone(),
        Span::default(),
    );

    let fired = bare_return::apply(&mut package, &mut assigner, block_id);
    assert!(fired, "bare_return must collapse the flat 2-semi shape");

    let stmts = &package.get_block(block_id).stmts;
    assert_eq!(
        stmts.len(),
        1,
        "block should collapse to a single trailing Expr stmt"
    );
    let StmtKind::Expr(tail_id) = package.get_stmt(stmts[0]).kind else {
        panic!("trailing stmt should be an Expr stmt");
    };
    assert_eq!(tail_id, v_id, "trailing expr should be the slot RHS");
}

#[test]
fn pre_stmt_reads_flag_refuses_to_fold() {
    // MANUAL-FIR fixture: this shape is never produced by normalize +
    // transform; it pins matcher discipline against future lowering
    // bugs that would emit malformed FIR.
    //
    // A leading statement reads `__has_returned`. The bailout must
    // trip even though the terminal pair / merge shape is canonical.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let int_ty = Ty::Prim(Prim::Int);
    let bool_ty = Ty::Prim(Prim::Bool);

    // Pre-stmt: a `Semi(__has_returned)` expression-statement that
    // reads the flag value. The expression's result is unused (Semi
    // discards it); the read alone trips the bailout.
    let flag_read = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        slots.has_returned,
        bool_ty.clone(),
        Span::default(),
    );
    let pre_stmt = alloc_semi_stmt(&mut package, &mut assigner, flag_read, Span::default());

    let v_id = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(1)),
        Span::default(),
    );
    let pair_stmt = build_nested_pair_stmt(&mut package, &mut assigner, &slots, v_id, &int_ty);
    let fallthrough = build_fallthrough(&mut package, &mut assigner, &slots, &int_ty);
    let merge_stmt = build_merge_stmt(&mut package, &mut assigner, &slots, fallthrough, &int_ty);
    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![pre_stmt, pair_stmt, merge_stmt],
        int_ty.clone(),
        Span::default(),
    );

    let before = package.get_block(block_id).stmts.clone();
    let fired = bare_return::apply(&mut package, &mut assigner, block_id);
    assert!(
        !fired,
        "bare_return must refuse when a pre-stmt reads the flag"
    );
    assert_eq!(
        before,
        package.get_block(block_id).stmts,
        "block must be unchanged when the bailout fires"
    );
}

#[test]
fn missing_flag_set_refuses_to_fold() {
    // MANUAL-FIR fixture: this shape is never produced by normalize +
    // transform; it pins matcher discipline against future lowering
    // bugs that would emit malformed FIR.
    //
    // The terminal block contains only the slot assign — the flag
    // set is missing. The matcher must refuse because the shape is
    // not the canonical terminal pair.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let int_ty = Ty::Prim(Prim::Int);

    let v_id = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(1)),
        Span::default(),
    );
    let slot_stmt = build_slot_assign_stmt(&mut package, &mut assigner, &slots, v_id, &int_ty);
    // Wrap just the slot assign in a Unit block to mimic the
    // nested-block form but with the flag set missing.
    let inner_bid = alloc_block(
        &mut package,
        &mut assigner,
        vec![slot_stmt],
        Ty::UNIT,
        Span::default(),
    );
    let inner_expr = alloc_block_expr(
        &mut package,
        &mut assigner,
        inner_bid,
        Ty::UNIT,
        Span::default(),
    );
    let broken_pair_stmt =
        alloc_semi_stmt(&mut package, &mut assigner, inner_expr, Span::default());
    let fallthrough = build_fallthrough(&mut package, &mut assigner, &slots, &int_ty);
    let merge_stmt = build_merge_stmt(&mut package, &mut assigner, &slots, fallthrough, &int_ty);
    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![broken_pair_stmt, merge_stmt],
        int_ty.clone(),
        Span::default(),
    );

    let before = package.get_block(block_id).stmts.clone();
    let fired = bare_return::apply(&mut package, &mut assigner, block_id);
    assert!(
        !fired,
        "bare_return must refuse when the flag set is missing"
    );
    assert_eq!(
        before,
        package.get_block(block_id).stmts,
        "block must be unchanged when the matcher refuses"
    );
}

// ---------------------------------------------------------------------
// Single-body emit-shape regression tests
// ---------------------------------------------------------------------
//
// These tests target the single-body `return v` emit shape produced by
// `super::super::super::create_flag_trailing_expr_for_slot` on its
// no-trailing-expression branch. The shape that path emits is:
//
//   Block(ty=Int, [
//     Local(Mut, __has_returned : Bool = false),  // (decls live in
//     Local(Mut, __ret_val : Int = 0),            //  the outer block)
//     Semi(Block(ty=Unit, [Semi(slot=v), Semi(flag=true)])),
//     Expr(if __has_returned { __ret_val } else { <literal default> }),
//   ])
//
// The defining difference from the canonical positive cases above is
// the merge's else-arm: instead of a `__ret_val` read (the test fixture
// convention used to keep let-folding inactive), it is a literal
// default value because the no-trailing-expression branch does not
// allocate a `__trailing_result` binding. The existing `bare_return`
// matcher handles this shape verbatim because `identify_merge` only
// type-checks the else-arm (`block_ty`) and never inspects its value.
//
// The fixtures below omit the synthetic slot decls (`alloc_slots`
// fabricates the slot ids directly) because the per-rule tests focus
// on the rule's local invariants, not on the dead-decl cleanup that
// `dead_local` later performs.

/// Build the single-body merge expression
/// `if __has_returned { __ret_val } else { <literal default> }` and
/// return its enclosing `Expr` statement id. Differs from
/// [`build_merge_stmt`] only in the else-arm (literal default vs.
/// `__ret_val` read).
fn build_single_body_merge_stmt(
    package: &mut Package,
    assigner: &mut Assigner,
    slots: &Slots,
    return_ty: &Ty,
) -> StmtId {
    let cond = alloc_local_var_expr(
        package,
        assigner,
        slots.has_returned,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );
    let then_var = alloc_local_var_expr(
        package,
        assigner,
        slots.ret_val,
        return_ty.clone(),
        Span::default(),
    );
    let then_stmt = alloc_expr_stmt(package, assigner, then_var, Span::default());
    let then_bid = alloc_block(
        package,
        assigner,
        vec![then_stmt],
        return_ty.clone(),
        Span::default(),
    );
    let then_expr = alloc_block_expr(
        package,
        assigner,
        then_bid,
        return_ty.clone(),
        Span::default(),
    );
    let else_default = alloc_expr(
        package,
        assigner,
        return_ty.clone(),
        ExprKind::Lit(Lit::Int(0)),
        Span::default(),
    );
    let merge = alloc_if_expr(
        package,
        assigner,
        cond,
        then_expr,
        Some(else_default),
        return_ty.clone(),
        Span::default(),
    );
    alloc_expr_stmt(package, assigner, merge, Span::default())
}

#[test]
fn given_single_return_body_bare_return_collapses_to_value() {
    // Positive case: the single-body emit shape (no trailing user
    // expression — the function body is just a `return`) must collapse
    // to the slot RHS.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                return 17;
            }
        }
        "#},
        "Main",
        "bare_return",
        bare_return::apply,
        &expect![[r#"
            // before bare_return (fired=true)
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    {
                        __ret_val = 17;
                        __has_returned = true;
                    };
                    if __has_returned __ret_val else __ret_val
                }
            }
            // entry
            Main()

            // after bare_return
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    17
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn given_single_return_body_with_user_prefix_bare_return_collapses() {
    // Positive case: a side-effect-free user-code prefix must survive
    // the collapse. `pre_stmts_safe` accepts the prefix (it neither
    // writes either slot nor reads the flag), and the terminal pair +
    // merge still rewrite to the slot RHS.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let _x = 0;
                return 17;
            }
        }
        "#},
        "Main",
        "bare_return",
        bare_return::apply,
        &expect![[r#"
            // before bare_return (fired=true)
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    let _x : Int = 0;
                    {
                        __ret_val = 17;
                        __has_returned = true;
                    };
                    if __has_returned __ret_val else __ret_val
                }
            }
            // entry
            Main()

            // after bare_return
            // namespace Test
            function Main() : Int {
                body {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    let _x : Int = 0;
                    17
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn given_else_arm_writes_slot_with_aliased_lhs_bare_return_does_not_collapse() {
    // MANUAL-FIR fixture: this shape is never produced by normalize +
    // transform; it pins matcher discipline against future lowering
    // bugs that would emit malformed FIR.
    //
    // Negative case: a pre-stmt contains an assignment whose LHS
    // root local aliases `__ret_val`. `pre_stmts_safe` must refuse
    // because such a write would corrupt the value the collapsed
    // expression assumes is held in the slot RHS. The matcher
    // resolves the LHS root via [`extract_root_local`], so any
    // expression with the slot as its root (direct read or
    // path-rooted) is rejected.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let int_ty = Ty::Prim(Prim::Int);

    // Pre-stmt: `__ret_val = 99;` — a slot write through the slot's
    // own local id. This is the aliasing case the rule must reject.
    let bad_rhs = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(99)),
        Span::default(),
    );
    let bad_lhs = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        slots.ret_val,
        int_ty.clone(),
        Span::default(),
    );
    let bad_assign = alloc_assign_expr(
        &mut package,
        &mut assigner,
        bad_lhs,
        bad_rhs,
        Span::default(),
    );
    let bad_stmt = alloc_semi_stmt(&mut package, &mut assigner, bad_assign, Span::default());

    let v_id = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(17)),
        Span::default(),
    );
    let pair_stmt = build_nested_pair_stmt(&mut package, &mut assigner, &slots, v_id, &int_ty);
    let merge_stmt = build_single_body_merge_stmt(&mut package, &mut assigner, &slots, &int_ty);
    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![bad_stmt, pair_stmt, merge_stmt],
        int_ty.clone(),
        Span::default(),
    );

    let before = package.get_block(block_id).stmts.clone();
    let fired = bare_return::apply(&mut package, &mut assigner, block_id);
    assert!(
        !fired,
        "bare_return must refuse when a pre-stmt writes the slot"
    );
    assert_eq!(
        before,
        package.get_block(block_id).stmts,
        "block must be unchanged when the safety net refuses"
    );
}

#[test]
fn given_else_arm_lacks_set_pair_bare_return_does_not_collapse() {
    // MANUAL-FIR fixture: this shape is never produced by normalize +
    // transform; it pins matcher discipline against future lowering
    // bugs that would emit malformed FIR.
    //
    // Negative case: the terminal pair stmt's nested block carries
    // an extra leading statement, so it no longer matches the
    // canonical 2-Semi pair shape. `identify_nested_pair_stmt`
    // refuses because it requires the inner block to have exactly two
    // statements.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let int_ty = Ty::Prim(Prim::Int);

    let v_id = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(17)),
        Span::default(),
    );
    // Build the slot/flag set pair, then prepend a stray Semi-Unit
    // stmt so the inner block is 3 stmts instead of 2.
    let slot_stmt = build_slot_assign_stmt(&mut package, &mut assigner, &slots, v_id, &int_ty);
    let flag_stmt = build_flag_set_stmt(&mut package, &mut assigner, &slots);
    let stray_unit = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::UNIT,
        ExprKind::Tuple(Vec::new()),
        Span::default(),
    );
    let stray_stmt = alloc_semi_stmt(&mut package, &mut assigner, stray_unit, Span::default());
    let inner_bid = alloc_block(
        &mut package,
        &mut assigner,
        vec![stray_stmt, slot_stmt, flag_stmt],
        Ty::UNIT,
        Span::default(),
    );
    let inner_expr = alloc_block_expr(
        &mut package,
        &mut assigner,
        inner_bid,
        Ty::UNIT,
        Span::default(),
    );
    let broken_pair_stmt =
        alloc_semi_stmt(&mut package, &mut assigner, inner_expr, Span::default());

    let merge_stmt = build_single_body_merge_stmt(&mut package, &mut assigner, &slots, &int_ty);
    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![broken_pair_stmt, merge_stmt],
        int_ty.clone(),
        Span::default(),
    );

    let before = package.get_block(block_id).stmts.clone();
    let fired = bare_return::apply(&mut package, &mut assigner, block_id);
    assert!(
        !fired,
        "bare_return must refuse when the terminal pair stmt does not match the 2-Semi shape"
    );
    assert_eq!(
        before,
        package.get_block(block_id).stmts,
        "block must be unchanged when the matcher refuses"
    );
}

#[test]
fn given_then_arm_not_var_ret_val_bare_return_does_not_collapse() {
    // MANUAL-FIR fixture: this shape is never produced by normalize +
    // transform; it pins matcher discipline against future lowering
    // bugs that would emit malformed FIR.
    //
    // Negative case: the merge's then-arm reads a local other than
    // `__ret_val` (here, an unrelated `decoy` int local). The slot
    // identity recovered by `extract_then_arm_slot_read` therefore
    // disagrees with the slot written in the terminal pair, and
    // `identify_merge`'s slot-vs-pair check refuses the rewrite.
    let mut package = Package::default();
    let mut assigner = Assigner::default();
    let slots = alloc_slots(&mut assigner);
    let int_ty = Ty::Prim(Prim::Int);

    // Allocate an unrelated int local to serve as the decoy then-arm
    // read. The decoy's value never escapes the test fixture; it
    // exists solely to drive a non-`__ret_val` then-arm shape.
    let decoy_local = assigner.next_local();

    let cond = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        slots.has_returned,
        Ty::Prim(Prim::Bool),
        Span::default(),
    );
    let then_var = alloc_local_var_expr(
        &mut package,
        &mut assigner,
        decoy_local,
        int_ty.clone(),
        Span::default(),
    );
    let then_stmt = alloc_expr_stmt(&mut package, &mut assigner, then_var, Span::default());
    let then_bid = alloc_block(
        &mut package,
        &mut assigner,
        vec![then_stmt],
        int_ty.clone(),
        Span::default(),
    );
    let then_expr = alloc_block_expr(
        &mut package,
        &mut assigner,
        then_bid,
        int_ty.clone(),
        Span::default(),
    );
    let else_default = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(0)),
        Span::default(),
    );
    let merge = alloc_if_expr(
        &mut package,
        &mut assigner,
        cond,
        then_expr,
        Some(else_default),
        int_ty.clone(),
        Span::default(),
    );
    let merge_stmt = alloc_expr_stmt(&mut package, &mut assigner, merge, Span::default());

    let v_id = alloc_expr(
        &mut package,
        &mut assigner,
        int_ty.clone(),
        ExprKind::Lit(Lit::Int(17)),
        Span::default(),
    );
    let pair_stmt = build_nested_pair_stmt(&mut package, &mut assigner, &slots, v_id, &int_ty);
    let block_id = alloc_block(
        &mut package,
        &mut assigner,
        vec![pair_stmt, merge_stmt],
        int_ty.clone(),
        Span::default(),
    );

    let before = package.get_block(block_id).stmts.clone();
    let fired = bare_return::apply(&mut package, &mut assigner, block_id);
    assert!(
        !fired,
        "bare_return must refuse when the merge then-arm reads a local other than the pair's slot"
    );
    assert_eq!(
        before,
        package.get_block(block_id).stmts,
        "block must be unchanged when the matcher refuses"
    );
}
