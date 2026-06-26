// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for [`crate::return_unify::simplify::guard_clause`].
//!
//! Most tests use [`check_simplify_rule_q`]: a Q# snippet is compiled,
//! the pipeline runs through mono + return-unify-without-simplify, the
//! pre-simplify FIR is snapshotted, [`guard_clause::apply`] is invoked
//! on the named callable's body block, and the post-rule FIR is
//! snapshotted. The before/after snapshots pin the rule's effect
//! against what the lowerer actually emits, so the test inputs cannot
//! drift from the canonical flag-lowering output shape.
//!
//! The snapshot header records `fired=<bool>` so each case witnesses
//! whether the single-rule pass mutated the block. Three flavors of
//! `fired=false` appear here:
//!   * a no-returns body (no merge is ever synthesized);
//!   * a both-arms-return body (the `both_branches` rule's domain);
//!   * a chained-guard / let-in-rest body where the lowerer hoists
//!     intermediate stmts that break the single-pass matcher — the
//!     fixpoint driver is what bridges these gaps via interleaved
//!     `dead_flag` / `dead_local` passes.
//!
//! Inverted-orientation tests live in the nested
//! [`inverted_orientation`] module. The single positive case
//! (`if c { } else { return v; } rest`) maps to a Q# input and uses
//! the same helper. The remaining cases — broken inverted shapes that
//! the lowerer would never emit, and the rule-under-Local-init /
//! rule-under-nested-block contracts — stay as direct-FIR
//! construction (marked MANUAL-FIR) because either the shape isn't
//! reachable from Q# today, or the test pins behaviour the rule
//! exposes independent of the dispatch oracle.

use expect_test::expect;
use indoc::indoc;

use crate::return_unify::simplify::guard_clause;
use crate::return_unify::tests::check_simplify_rule_q;

#[test]
fn simple_guard_clause_collapses_to_if_else() {
    // Canonical `if c { return v; } rest` → `if c { v } else { rest }`.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                }
                0
            }
        }
        "#},
        "Main",
        "guard_clause",
        guard_clause::apply,
        &expect![[r#"
            // before guard_clause (fired=true)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                let __trailing_result : Int = if not __has_returned {
                    0
                } else {
                    __ret_val
                };
                if __has_returned {
                    __ret_val
                } else {
                    __trailing_result
                }
            }
            // entry
            Main()

            // after guard_clause
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    1
                } else {
                    0
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn guard_clause_with_let_in_rest_block() {
    // Q# input pairs a guard clause with a `let`-bound trailing
    // expression in the rest sequence. The lowerer per-stmt hoists the
    // `let` into a `let y = if not __has_returned { 2 } else { 0 };`,
    // which sits between the guard set and the trailing merge and
    // breaks the single-pass `guard_clause` matcher (it requires the
    // guard set to be immediately followed by the lazy continuation
    // and merge). The snapshot records `fired=false`; the fixpoint
    // driver is what bridges this gap by running `dead_flag` /
    // `dead_local` first to clean up the intermediate `let` before
    // re-entering `guard_clause`.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                }
                let y = 2;
                y
            }
        }
        "#},
        "Main",
        "guard_clause",
        guard_clause::apply,
        &expect![[r#"
            // before guard_clause (fired=false)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                let y : Int = if not __has_returned {
                    2
                } else {
                    0
                };
                let __trailing_result : Int = if not __has_returned {
                    y
                } else {
                    __ret_val
                };
                if __has_returned {
                    __ret_val
                } else {
                    __trailing_result
                }
            }
            // entry
            Main()

            // after guard_clause
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                let y : Int = if not __has_returned {
                    2
                } else {
                    0
                };
                let __trailing_result : Int = if not __has_returned {
                    y
                } else {
                    __ret_val
                };
                if __has_returned {
                    __ret_val
                } else {
                    __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn multiple_guard_clauses_chain_into_nested_if_else() {
    // Q# input chains two guard clauses before a trailing literal.
    // The lowerer emits the second guard inside a `if not
    // __has_returned { ... };` continuation, so the outer block does
    // not present the canonical guard/cont/merge triple to a single
    // `guard_clause::apply` invocation. The snapshot records
    // `fired=false`; collapsing chained guards is the fixpoint
    // driver's job (see `fixpoint::guard_clause_via_run_to_fixpoint`
    // for the same Q# input run through `run_to_fixpoint`, which does
    // converge to the nested if/else chain).
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                }
                if false {
                    return 2;
                }
                0
            }
        }
        "#},
        "Main",
        "guard_clause",
        guard_clause::apply,
        &expect![[r#"
            // before guard_clause (fired=false)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                if not __has_returned {
                    if false {
                        {
                            __ret_val = 2;
                            __has_returned = true;
                        };
                    }

                };
                let __trailing_result : Int = if not __has_returned {
                    0
                } else {
                    __ret_val
                };
                if __has_returned {
                    __ret_val
                } else {
                    __trailing_result
                }
            }
            // entry
            Main()

            // after guard_clause
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                if not __has_returned {
                    if false {
                        {
                            __ret_val = 2;
                            __has_returned = true;
                        };
                    }

                };
                let __trailing_result : Int = if not __has_returned {
                    0
                } else {
                    __ret_val
                };
                if __has_returned {
                    __ret_val
                } else {
                    __trailing_result
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn no_returns_block_has_no_merge_so_rule_does_not_fire() {
    // Negative: the function has no returns, so no merge pattern is ever
    // synthesized and the rule must not fire. The before/after FIR is
    // identical and the snapshot header records `fired=false`.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                let x = 1;
                x + 2
            }
        }
        "#},
        "Main",
        "guard_clause",
        guard_clause::apply,
        &expect![[r#"
            // before guard_clause (fired=false)
            function Main() : Int {
                let x : Int = 1;
                x + 2
            }
            // entry
            Main()

            // after guard_clause
            function Main() : Int {
                let x : Int = 1;
                x + 2
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn both_branches_return_shape_not_collapsed_by_guard_clause_rule() {
    // Negative: `if c { return a; } else { return b; }` is the
    // both_branches shape (the guard-set `if` has an `else` arm). The
    // guard_clause rule must refuse to fire on this shape; the
    // before/after FIR is identical and the snapshot header records
    // `fired=false`.
    check_simplify_rule_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if true {
                    return 1;
                } else {
                    return 2;
                }
            }
        }
        "#},
        "Main",
        "guard_clause",
        guard_clause::apply,
        &expect![[r#"
            // before guard_clause (fired=false)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                } else {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
                }

                if __has_returned {
                    __ret_val
                } else {
                    __ret_val
                }
            }
            // entry
            Main()

            // after guard_clause
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                } else {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
                }

                if __has_returned {
                    __ret_val
                } else {
                    __ret_val
                }
            }
            // entry
            Main()
        "#]],
    );
}

// ---------------------------------------------------------------------------
// Inverted-orientation guard-clause regressions.
//
// One positive test
// ([`inverted_orientation::given_inverted_guard_else_arm_guard_clause_rewrites_with_not`])
// drives the canonical inverted shape from a Q# input via
// [`check_simplify_rule_q`]. The remaining tests in the module stay as
// direct FIR construction (marked MANUAL-FIR):
//   * the Local-init and nested-block positives need to invoke
//     `guard_clause::apply` on a *specific nested block id*, not on the
//     named callable's body block, so the Q#-driven helper cannot
//     express the contract;
//   * the two negatives build unnatural shapes (asymmetric slot-set
//     sequence, foreign stmt between sets) that the flag-lowering
//     lowering would not produce, but that pin the matcher's discipline
//     against future drift.
// ---------------------------------------------------------------------------

mod inverted_orientation {
    use expect_test::expect;
    use indoc::indoc;
    use qsc_data_structures::span::Span;
    use qsc_fir::{
        assigner::Assigner,
        fir::{
            ExprId, ExprKind, Lit, LocalVarId, Mutability, Package, PackageLookup, Res, StmtId,
            StmtKind, UnOp,
        },
        ty::{Prim, Ty},
    };

    use crate::fir_builder::{
        alloc_assign_expr, alloc_bind_pat, alloc_block, alloc_block_expr, alloc_bool_lit,
        alloc_expr, alloc_expr_stmt, alloc_if_expr, alloc_local_stmt, alloc_local_var_expr,
        alloc_not_expr, alloc_semi_stmt,
    };
    use crate::return_unify::simplify::guard_clause;
    use crate::return_unify::tests::check_simplify_rule_q;

    /// Slot identities shared by every inverted-orientation fixture.
    struct Slots {
        has_returned: LocalVarId,
        ret_val: LocalVarId,
    }

    fn alloc_slots(assigner: &mut Assigner) -> Slots {
        Slots {
            has_returned: assigner.next_local(),
            ret_val: assigner.next_local(),
        }
    }

    /// Build `__ret_val = v;` Semi statement.
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

    /// Build `__has_returned = true;` Semi statement.
    fn build_flag_set_stmt(
        package: &mut Package,
        assigner: &mut Assigner,
        slots: &Slots,
    ) -> StmtId {
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

    /// Build a Unit-typed block expr containing the slot-set sequence
    /// `[Semi(__ret_val = v), Semi(__has_returned = true)]`. Used as
    /// the else-arm of the inverted guard's if-expression.
    fn build_slot_set_arm_expr(
        package: &mut Package,
        assigner: &mut Assigner,
        slots: &Slots,
        v_id: ExprId,
        return_ty: &Ty,
    ) -> ExprId {
        let slot_stmt = build_slot_assign_stmt(package, assigner, slots, v_id, return_ty);
        let flag_stmt = build_flag_set_stmt(package, assigner, slots);
        let bid = alloc_block(
            package,
            assigner,
            vec![slot_stmt, flag_stmt],
            Ty::UNIT,
            Span::default(),
        );
        alloc_block_expr(package, assigner, bid, Ty::UNIT, Span::default())
    }

    /// Build an empty Unit-typed block expr — the only then-arm shape
    /// `identify_guard_else_arm` accepts.
    fn build_empty_unit_block_expr(package: &mut Package, assigner: &mut Assigner) -> ExprId {
        let bid = alloc_block(package, assigner, Vec::new(), Ty::UNIT, Span::default());
        alloc_block_expr(package, assigner, bid, Ty::UNIT, Span::default())
    }

    /// Build the inverted guard `Semi(If(cond, empty_unit, Some(slot_sets)))`.
    fn build_inverted_guard_stmt(
        package: &mut Package,
        assigner: &mut Assigner,
        cond_id: ExprId,
        then_id: ExprId,
        else_id: ExprId,
    ) -> StmtId {
        let if_expr = alloc_if_expr(
            package,
            assigner,
            cond_id,
            then_id,
            Some(else_id),
            Ty::UNIT,
            Span::default(),
        );
        alloc_semi_stmt(package, assigner, if_expr, Span::default())
    }

    /// Build the lazy continuation `Semi(If(not __has_returned, rest_block, None))`.
    fn build_continuation_stmt(
        package: &mut Package,
        assigner: &mut Assigner,
        slots: &Slots,
        rest_block_expr_id: ExprId,
    ) -> StmtId {
        let bool_ty = Ty::Prim(Prim::Bool);
        let flag_read = alloc_local_var_expr(
            package,
            assigner,
            slots.has_returned,
            bool_ty.clone(),
            Span::default(),
        );
        let not_flag = alloc_not_expr(package, assigner, flag_read, Span::default());
        let if_expr = alloc_if_expr(
            package,
            assigner,
            not_flag,
            rest_block_expr_id,
            None,
            Ty::UNIT,
            Span::default(),
        );
        alloc_semi_stmt(package, assigner, if_expr, Span::default())
    }

    /// Build the canonical merge `Expr(If(__has_returned, __ret_val, Some(fallthrough)))`.
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

    /// Build the rest-block expression `{ rest_value }` (single trailing
    /// `Expr` carrying the supplied value of type `return_ty`). Returns
    /// `(block_id, block_expr_id)` because the rewriter wraps the inner
    /// `BlockId` in a fresh `Block` expression, so callers need the
    /// `BlockId` for shape assertions.
    fn build_rest_block_expr(
        package: &mut Package,
        assigner: &mut Assigner,
        rest_value: ExprId,
        return_ty: &Ty,
    ) -> (qsc_fir::fir::BlockId, ExprId) {
        let rest_stmt = alloc_expr_stmt(package, assigner, rest_value, Span::default());
        let rest_bid = alloc_block(
            package,
            assigner,
            vec![rest_stmt],
            return_ty.clone(),
            Span::default(),
        );
        let expr_id = alloc_block_expr(
            package,
            assigner,
            rest_bid,
            return_ty.clone(),
            Span::default(),
        );
        (rest_bid, expr_id)
    }

    /// Allocate a fresh Bool local-var read; used as the user condition
    /// for the inverted guard.
    fn alloc_user_cond(package: &mut Package, assigner: &mut Assigner) -> (LocalVarId, ExprId) {
        let cond_local = assigner.next_local();
        let cond_expr = alloc_local_var_expr(
            package,
            assigner,
            cond_local,
            Ty::Prim(Prim::Bool),
            Span::default(),
        );
        (cond_local, cond_expr)
    }

    /// Assert the rewrite output is the canonical
    /// `if not <cond_local> { v_id } else { <rest_bid> }` shape.
    /// `rest_bid` is the inner block id; the rewriter wraps it in a
    /// fresh `Block` expression so the assertion compares block ids
    /// rather than expression ids.
    fn assert_rewrite_shape(
        package: &Package,
        block_id: qsc_fir::fir::BlockId,
        cond_local: LocalVarId,
        v_id: ExprId,
        rest_bid: qsc_fir::fir::BlockId,
    ) {
        let stmts = &package.get_block(block_id).stmts;
        assert_eq!(
            stmts.len(),
            1,
            "block should collapse to a single trailing Expr stmt"
        );
        let StmtKind::Expr(tail_id) = package.get_stmt(stmts[0]).kind else {
            panic!("trailing stmt should be an Expr stmt");
        };
        let ExprKind::If(new_cond_id, new_then_id, Some(new_else_id)) =
            &package.get_expr(tail_id).kind
        else {
            panic!("trailing expr should be an If-with-else");
        };
        // Cond must be `not <cond_local>`.
        let ExprKind::UnOp(UnOp::NotL, inner_id) = &package.get_expr(*new_cond_id).kind else {
            panic!("rewritten cond should be UnOp(NotL, _)");
        };
        let ExprKind::Var(Res::Local(read_local), _) = &package.get_expr(*inner_id).kind else {
            panic!("not-operand should read a Local");
        };
        assert_eq!(*read_local, cond_local, "not should wrap the user cond");
        // Then-arm: { v_id }.
        let ExprKind::Block(then_bid) = &package.get_expr(*new_then_id).kind else {
            panic!("rewritten then-arm should be a Block");
        };
        let then_stmts = &package.get_block(*then_bid).stmts;
        assert_eq!(then_stmts.len(), 1);
        let StmtKind::Expr(then_tail_id) = package.get_stmt(then_stmts[0]).kind else {
            panic!("then-block trailing stmt should be Expr");
        };
        assert_eq!(then_tail_id, v_id, "then-arm should be the slot RHS");
        // Else-arm: a fresh `Block` expression wrapping the original
        // rest block id.
        let ExprKind::Block(else_bid) = &package.get_expr(*new_else_id).kind else {
            panic!("rewritten else-arm should be a Block");
        };
        assert_eq!(
            *else_bid, rest_bid,
            "else-arm should wrap the original rest block id"
        );
    }

    /// Build the fixed-shape inverted-guard block:
    ///   `[ guard_stmt, cont_stmt, merge_stmt ]`
    /// Returns the block id, the user-cond local id, the slot RHS expr id,
    /// and the inner rest-block id (the wrapping `Block` expression
    /// is discarded after the rewriter unwraps and re-wraps it).
    fn build_canonical_block(
        package: &mut Package,
        assigner: &mut Assigner,
        slots: &Slots,
        int_ty: &Ty,
    ) -> (
        qsc_fir::fir::BlockId,
        LocalVarId,
        ExprId,
        qsc_fir::fir::BlockId,
    ) {
        let (cond_local, cond_expr) = alloc_user_cond(package, assigner);
        let v_id = alloc_expr(
            package,
            assigner,
            int_ty.clone(),
            ExprKind::Lit(Lit::Int(42)),
            Span::default(),
        );
        let then_id = build_empty_unit_block_expr(package, assigner);
        let else_id = build_slot_set_arm_expr(package, assigner, slots, v_id, int_ty);
        let guard_stmt = build_inverted_guard_stmt(package, assigner, cond_expr, then_id, else_id);

        let rest_value = alloc_expr(
            package,
            assigner,
            int_ty.clone(),
            ExprKind::Lit(Lit::Int(7)),
            Span::default(),
        );
        let (rest_bid, rest_block_expr) =
            build_rest_block_expr(package, assigner, rest_value, int_ty);
        let cont_stmt = build_continuation_stmt(package, assigner, slots, rest_block_expr);

        let fallthrough = alloc_local_var_expr(
            package,
            assigner,
            slots.ret_val,
            int_ty.clone(),
            Span::default(),
        );
        let merge_stmt = build_merge_stmt(package, assigner, slots, fallthrough, int_ty);

        let block_id = alloc_block(
            package,
            assigner,
            vec![guard_stmt, cont_stmt, merge_stmt],
            int_ty.clone(),
            Span::default(),
        );
        (block_id, cond_local, v_id, rest_bid)
    }

    #[test]
    fn given_inverted_guard_else_arm_guard_clause_rewrites_with_not() {
        // Q# input `if c { } else { return v; } rest` lowers to the
        // inverted-orientation guard shape: the `if`'s then-arm is an
        // empty Unit block and the slot/flag sets live in the else-arm.
        // `guard_clause` rewrites this to `if not c { v } else { rest }`.
        check_simplify_rule_q(
            indoc! {r#"
            namespace Test {
                function Main() : Int {
                    if true {
                    } else {
                        return 1;
                    }
                    2
                }
            }
            "#},
            "Main",
            "guard_clause",
            guard_clause::apply,
            &expect![[r#"
                // before guard_clause (fired=true)
                function Main() : Int {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    if true {} else {
                        {
                            __ret_val = 1;
                            __has_returned = true;
                        };
                    }

                    let __trailing_result : Int = if not __has_returned {
                        2
                    } else {
                        __ret_val
                    };
                    if __has_returned {
                        __ret_val
                    } else {
                        __trailing_result
                    }
                }
                // entry
                Main()

                // after guard_clause
                function Main() : Int {
                    mutable __has_returned : Bool = false;
                    mutable __ret_val : Int = 0;
                    if not true {
                        1
                    } else {
                        2
                    }

                }
                // entry
                Main()
            "#]],
        );
    }

    #[test]
    fn given_inverted_guard_in_local_init_guard_clause_rewrites_with_not() {
        // MANUAL-FIR: this test pins the rule's contract when its
        // input block is the initializer body of a Local stmt inside a
        // larger outer block. `check_simplify_rule_q` always targets
        // the named callable's body block, so it cannot express
        // "invoke the rule on this specific nested block id". Direct
        // FIR construction lets us position the inverted-guard block
        // exactly where we need it and then call `guard_clause::apply`
        // on the inner block_id directly.
        let mut package = Package::default();
        let mut assigner = Assigner::default();
        let slots = alloc_slots(&mut assigner);
        let int_ty = Ty::Prim(Prim::Int);

        let (inner_block_id, cond_local, v_id, rest_bid) =
            build_canonical_block(&mut package, &mut assigner, &slots, &int_ty);

        // Wrap the inner block as the initializer of a Local stmt in an
        // outer block. The outer block is built only to give the inner
        // block a parent context; nothing in the rule reads it.
        let init_expr = alloc_block_expr(
            &mut package,
            &mut assigner,
            inner_block_id,
            int_ty.clone(),
            Span::default(),
        );
        let (_outer_local, outer_pat) = alloc_bind_pat(
            &mut package,
            &mut assigner,
            "x",
            int_ty.clone(),
            Span::default(),
        );
        let outer_local_stmt = alloc_local_stmt(
            &mut package,
            &mut assigner,
            Mutability::Immutable,
            outer_pat,
            init_expr,
            Span::default(),
        );
        let _outer_block = alloc_block(
            &mut package,
            &mut assigner,
            vec![outer_local_stmt],
            int_ty.clone(),
            Span::default(),
        );

        let synth_slots =
            crate::return_unify::tests::synth_slots_for_block(&package, inner_block_id);
        let fired = guard_clause::apply(&mut package, &mut assigner, inner_block_id, &synth_slots);
        assert!(
            fired,
            "guard_clause must fire on the inverted shape inside a Local init"
        );
        assert_rewrite_shape(&package, inner_block_id, cond_local, v_id, rest_bid);
    }

    #[test]
    fn given_inverted_guard_in_nested_block_guard_clause_rewrites_with_not() {
        // MANUAL-FIR: this test pins the rule's contract when its
        // input block is nested inside an outer Block statement.
        // `check_simplify_rule_q` always targets the named callable's
        // body block, so it cannot express "invoke the rule on this
        // specific nested block id". Direct FIR construction is the
        // only way to invoke `guard_clause::apply` on the inner block
        // while still keeping the outer block as containing context.
        let mut package = Package::default();
        let mut assigner = Assigner::default();
        let slots = alloc_slots(&mut assigner);
        let int_ty = Ty::Prim(Prim::Int);

        let (inner_block_id, cond_local, v_id, rest_bid) =
            build_canonical_block(&mut package, &mut assigner, &slots, &int_ty);

        let inner_block_expr = alloc_block_expr(
            &mut package,
            &mut assigner,
            inner_block_id,
            int_ty.clone(),
            Span::default(),
        );
        let wrapper_stmt = alloc_expr_stmt(
            &mut package,
            &mut assigner,
            inner_block_expr,
            Span::default(),
        );
        let _outer_block = alloc_block(
            &mut package,
            &mut assigner,
            vec![wrapper_stmt],
            int_ty.clone(),
            Span::default(),
        );

        let synth_slots =
            crate::return_unify::tests::synth_slots_for_block(&package, inner_block_id);
        let fired = guard_clause::apply(&mut package, &mut assigner, inner_block_id, &synth_slots);
        assert!(
            fired,
            "guard_clause must fire on the inverted shape inside a nested block"
        );
        assert_rewrite_shape(&package, inner_block_id, cond_local, v_id, rest_bid);
    }

    #[test]
    fn given_else_arm_with_only_one_set_guard_clause_does_not_match() {
        // MANUAL-FIR: this test pins matcher discipline on a broken
        // inverted-shape input — the else-arm contains only the slot
        // assignment, missing the flag set. `match_slot_set_arm`
        // requires exactly two Semi statements; the matcher must
        // refuse. Flag lowering never emits this shape,
        // so it isn't reachable from Q#; direct construction is the
        // only way to feed the matcher a malformed slot-set sequence.
        let mut package = Package::default();
        let mut assigner = Assigner::default();
        let slots = alloc_slots(&mut assigner);
        let int_ty = Ty::Prim(Prim::Int);

        let (_, cond_expr) = alloc_user_cond(&mut package, &mut assigner);
        let v_id = alloc_expr(
            &mut package,
            &mut assigner,
            int_ty.clone(),
            ExprKind::Lit(Lit::Int(42)),
            Span::default(),
        );
        let slot_stmt = build_slot_assign_stmt(&mut package, &mut assigner, &slots, v_id, &int_ty);
        // Else-arm is a Unit block carrying only the slot set — flag set absent.
        let asymmetric_bid = alloc_block(
            &mut package,
            &mut assigner,
            vec![slot_stmt],
            Ty::UNIT,
            Span::default(),
        );
        let asymmetric_else = alloc_block_expr(
            &mut package,
            &mut assigner,
            asymmetric_bid,
            Ty::UNIT,
            Span::default(),
        );
        let then_id = build_empty_unit_block_expr(&mut package, &mut assigner);
        let guard_stmt = build_inverted_guard_stmt(
            &mut package,
            &mut assigner,
            cond_expr,
            then_id,
            asymmetric_else,
        );

        let rest_value = alloc_expr(
            &mut package,
            &mut assigner,
            int_ty.clone(),
            ExprKind::Lit(Lit::Int(7)),
            Span::default(),
        );
        let (_rest_bid, rest_block_expr) =
            build_rest_block_expr(&mut package, &mut assigner, rest_value, &int_ty);
        let cont_stmt =
            build_continuation_stmt(&mut package, &mut assigner, &slots, rest_block_expr);
        let fallthrough = alloc_local_var_expr(
            &mut package,
            &mut assigner,
            slots.ret_val,
            int_ty.clone(),
            Span::default(),
        );
        let merge_stmt =
            build_merge_stmt(&mut package, &mut assigner, &slots, fallthrough, &int_ty);

        let block_id = alloc_block(
            &mut package,
            &mut assigner,
            vec![guard_stmt, cont_stmt, merge_stmt],
            int_ty.clone(),
            Span::default(),
        );

        let pre_stmts = package.get_block(block_id).stmts.clone();
        let synth_slots = crate::return_unify::tests::synth_slots_for_block(&package, block_id);
        let fired = guard_clause::apply(&mut package, &mut assigner, block_id, &synth_slots);
        assert!(
            !fired,
            "guard_clause must reject an else-arm missing the flag set"
        );
        assert_eq!(
            package.get_block(block_id).stmts,
            pre_stmts,
            "block stmts must be unchanged when the matcher refuses"
        );
    }

    #[test]
    fn given_else_arm_with_extra_stmt_guard_clause_does_not_match() {
        // MANUAL-FIR: this test pins matcher discipline on a broken
        // inverted-shape input — the else-arm carries three Semi
        // statements (`__ret_val = v; <foreign>; __has_returned =
        // true;`). `match_slot_set_arm` requires exactly two
        // statements; the foreign middle stmt makes the matcher
        // refuse. Flag lowering never emits this shape,
        // so it isn't reachable from Q#; direct construction is the
        // only way to feed the matcher a slot-set sequence with a
        // foreign interloper.
        let mut package = Package::default();
        let mut assigner = Assigner::default();
        let slots = alloc_slots(&mut assigner);
        let int_ty = Ty::Prim(Prim::Int);

        let (_, cond_expr) = alloc_user_cond(&mut package, &mut assigner);
        let v_id = alloc_expr(
            &mut package,
            &mut assigner,
            int_ty.clone(),
            ExprKind::Lit(Lit::Int(42)),
            Span::default(),
        );
        let slot_stmt = build_slot_assign_stmt(&mut package, &mut assigner, &slots, v_id, &int_ty);
        // Foreign stmt: a Semi(Unit-literal) — innocuous but breaks the
        // expected 2-stmt slot-set shape.
        let foreign_expr = alloc_expr(
            &mut package,
            &mut assigner,
            Ty::UNIT,
            ExprKind::Tuple(Vec::new()),
            Span::default(),
        );
        let foreign_stmt =
            alloc_semi_stmt(&mut package, &mut assigner, foreign_expr, Span::default());
        let flag_stmt = build_flag_set_stmt(&mut package, &mut assigner, &slots);
        let bloated_bid = alloc_block(
            &mut package,
            &mut assigner,
            vec![slot_stmt, foreign_stmt, flag_stmt],
            Ty::UNIT,
            Span::default(),
        );
        let bloated_else = alloc_block_expr(
            &mut package,
            &mut assigner,
            bloated_bid,
            Ty::UNIT,
            Span::default(),
        );
        let then_id = build_empty_unit_block_expr(&mut package, &mut assigner);
        let guard_stmt = build_inverted_guard_stmt(
            &mut package,
            &mut assigner,
            cond_expr,
            then_id,
            bloated_else,
        );

        let rest_value = alloc_expr(
            &mut package,
            &mut assigner,
            int_ty.clone(),
            ExprKind::Lit(Lit::Int(7)),
            Span::default(),
        );
        let (_rest_bid, rest_block_expr) =
            build_rest_block_expr(&mut package, &mut assigner, rest_value, &int_ty);
        let cont_stmt =
            build_continuation_stmt(&mut package, &mut assigner, &slots, rest_block_expr);
        let fallthrough = alloc_local_var_expr(
            &mut package,
            &mut assigner,
            slots.ret_val,
            int_ty.clone(),
            Span::default(),
        );
        let merge_stmt =
            build_merge_stmt(&mut package, &mut assigner, &slots, fallthrough, &int_ty);

        let block_id = alloc_block(
            &mut package,
            &mut assigner,
            vec![guard_stmt, cont_stmt, merge_stmt],
            int_ty.clone(),
            Span::default(),
        );

        let pre_stmts = package.get_block(block_id).stmts.clone();
        let synth_slots = crate::return_unify::tests::synth_slots_for_block(&package, block_id);
        let fired = guard_clause::apply(&mut package, &mut assigner, block_id, &synth_slots);
        assert!(
            !fired,
            "guard_clause must reject an else-arm with a foreign stmt between sets"
        );
        assert_eq!(
            package.get_block(block_id).stmts,
            pre_stmts,
            "block stmts must be unchanged when the matcher refuses"
        );
    }
}
