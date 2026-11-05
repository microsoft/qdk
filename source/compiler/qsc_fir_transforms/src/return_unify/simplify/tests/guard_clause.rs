// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for [`crate::return_unify::simplify::guard_clause`].
//!
//! Snapshot tests use [`check_simplify_rule_q`]: a Q# snippet is compiled,
//! the pipeline runs through mono + return-unify-without-simplify, the
//! pre-simplify FIR is snapshotted, [`guard_clause::apply`] is invoked
//! on the named callable's body block, and the post-rule FIR is
//! snapshotted. The before/after snapshots pin the rule's effect
//! against what the lowerer actually emits, so the test inputs cannot
//! drift from the canonical flag-lowering output shape.
//!
//! The snapshot header records `fired=<bool>` so each case witnesses
//! whether the single-rule pass mutated the block. Refusals cover
//! noncanonical shapes, earlier or nested returns, and mismatched result
//! identities. Fixpoint tests separately check that let folding cannot
//! expose an unrelated local as the continuation on a later iteration.
//! Additional refusal tests mutate source-lowered FIR and assert that
//! both the package and assigner remain unchanged when the rule refuses.
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
use qsc_fir::{
    assigner::Assigner,
    fir::{
        BlockId, ExprId, ExprKind, Lit, LocalVarId, Mutability, Package, PackageLookup,
        PackageSpan, PatKind, StmtId, StmtKind,
    },
    ty::{Prim, Ty},
};

use crate::PipelineStage;
use crate::fir_builder::{
    alloc_block, alloc_block_expr, alloc_bool_lit, alloc_expr_stmt, alloc_local_var,
    alloc_local_var_expr, alloc_semi_stmt,
};
use crate::return_unify::lower::SynthSlots;
use crate::return_unify::simplify::guard_clause;
use crate::return_unify::tests::{check_simplify_rule_q, synth_slots_for_block};
use crate::test_utils::{compile_and_run_pipeline_to, find_callable_body_block};

fn check_rejected_guard_mutation(
    mutate: impl FnOnce(&mut Package, &mut Assigner, BlockId, &SynthSlots),
) {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(
        r#"
        @EntryPoint()
        operation Main() : Int {
            if true {} else { return 1; }
            2
        }
        "#,
        PipelineStage::Mono,
    );
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    let errors =
        crate::return_unify::unify_returns_without_simplify(&mut store, pkg_id, &mut assigner);
    assert!(
        errors.is_empty(),
        "unexpected return-unify errors: {errors:?}"
    );
    let package = store.get_mut(pkg_id);
    let block = find_callable_body_block(package, "Main");
    let slots = synth_slots_for_block(package, block);
    mutate(package, &mut assigner, block, &slots);
    assert_guard_clause_rejected_without_changes(package, &mut assigner, block, &slots);
}

fn assert_guard_clause_rejected_without_changes(
    package: &mut Package,
    assigner: &mut Assigner,
    block: BlockId,
    slots: &SynthSlots,
) {
    let package_before = format!("{package:?}");
    let assigner_before = format!("{assigner:?}");
    assert!(!guard_clause::apply(package, assigner, block, slots));
    assert_eq!(
        format!("{package:?}"),
        package_before,
        "refusal changed the FIR"
    );
    assert_eq!(
        format!("{assigner:?}"),
        assigner_before,
        "refusal consumed node IDs"
    );
}

fn local_initializer(package: &Package, stmt: StmtId) -> ExprId {
    let StmtKind::Local(_, _, init) = package.get_stmt(stmt).kind else {
        panic!("expected a local binding");
    };
    init
}

fn slot_assignment(package: &Package, slot: LocalVarId) -> ExprId {
    package
        .exprs
        .iter()
        .find_map(|(id, expr)| {
            let ExprKind::Assign(lhs, _) = expr.kind else {
                return None;
            };
            (super::super::extract_local_read(package, lhs, None) == Some(slot)).then_some(id)
        })
        .expect("canonical guard must assign the slot")
}

#[test]
fn foreign_continuation_binding_is_not_collapsed() {
    check_rejected_guard_mutation(|package, assigner, block, _slots| {
        let stmts = &package.get_block(block).stmts;
        let StmtKind::Local(_, pat_id, _) = package.get_stmt(stmts[stmts.len() - 2]).kind else {
            panic!("expected a continuation binding");
        };
        let PatKind::Bind(ident) = &mut package.pats.get_mut(pat_id).expect("pattern").kind else {
            panic!("expected a bound local");
        };
        ident.id = assigner.next_local();
    });
}

#[test]
fn unrelated_merge_fallthrough_is_not_collapsed() {
    check_rejected_guard_mutation(|package, assigner, block, slots| {
        let stmt = *package.get_block(block).stmts.last().expect("merge");
        let StmtKind::Expr(merge) = package.get_stmt(stmt).kind else {
            panic!("expected a merge expression");
        };
        let read = alloc_local_var_expr(
            package,
            assigner,
            slots.return_slot.var_id,
            Ty::Prim(Prim::Int),
            PackageSpan::default(),
        );
        let ExprKind::If(_, _, otherwise) = &mut package.exprs.get_mut(merge).expect("merge").kind
        else {
            panic!("expected a conditional merge");
        };
        *otherwise = Some(read);
    });
}

#[test]
fn missing_flag_declaration_prevents_guard_collapse() {
    check_rejected_guard_mutation(|package, _assigner, block, _slots| {
        package
            .blocks
            .get_mut(block)
            .expect("block")
            .stmts
            .remove(0);
    });
}

#[test]
fn initially_true_flag_prevents_guard_collapse() {
    check_rejected_guard_mutation(|package, _assigner, block, _slots| {
        let init = local_initializer(package, package.get_block(block).stmts[0]);
        package.exprs.get_mut(init).expect("initializer").kind = ExprKind::Lit(Lit::Bool(true));
    });
}

#[test]
fn nested_prefix_flag_write_prevents_guard_collapse() {
    check_rejected_guard_mutation(|package, assigner, block, slots| {
        let write = slot_assignment(package, slots.has_returned);
        let stmt = alloc_semi_stmt(package, assigner, write, PackageSpan::default());
        let body = alloc_block(
            package,
            assigner,
            vec![stmt],
            Ty::UNIT,
            PackageSpan::default(),
        );
        let init = alloc_block_expr(package, assigner, body, Ty::UNIT, PackageSpan::default());
        let (_, binding) = alloc_local_var(
            package,
            assigner,
            "prefix",
            &Ty::UNIT,
            init,
            Mutability::Immutable,
        );
        package
            .blocks
            .get_mut(block)
            .expect("block")
            .stmts
            .insert(2, binding);
    });
}

#[test]
fn prefix_return_slot_write_prevents_guard_collapse() {
    check_rejected_guard_mutation(|package, assigner, block, slots| {
        let write = slot_assignment(package, slots.return_slot.var_id);
        let stmt = alloc_semi_stmt(package, assigner, write, PackageSpan::default());
        package
            .blocks
            .get_mut(block)
            .expect("block")
            .stmts
            .insert(2, stmt);
    });
}

#[test]
fn effectful_skipped_continuation_arm_prevents_guard_collapse() {
    check_rejected_guard_mutation(|package, assigner, block, slots| {
        let stmts = &package.get_block(block).stmts;
        let init = local_initializer(package, stmts[stmts.len() - 2]);
        let ExprKind::If(_, _, Some(otherwise)) = package.get_expr(init).kind else {
            panic!("expected a lazy continuation");
        };
        let write = slot_assignment(package, slots.return_slot.var_id);
        let write_stmt = alloc_semi_stmt(package, assigner, write, PackageSpan::default());
        let read_stmt = alloc_expr_stmt(package, assigner, otherwise, PackageSpan::default());
        let body = alloc_block(
            package,
            assigner,
            vec![write_stmt, read_stmt],
            Ty::Prim(Prim::Int),
            PackageSpan::default(),
        );
        let replacement = alloc_block_expr(
            package,
            assigner,
            body,
            Ty::Prim(Prim::Int),
            PackageSpan::default(),
        );
        let ExprKind::If(_, _, otherwise) =
            &mut package.exprs.get_mut(init).expect("continuation").kind
        else {
            panic!("expected a lazy continuation");
        };
        *otherwise = Some(replacement);
    });
}

#[test]
fn guard_condition_slot_write_prevents_guard_collapse() {
    check_rejected_guard_mutation(|package, assigner, block, slots| {
        let stmts = &package.get_block(block).stmts;
        let (StmtKind::Expr(guard) | StmtKind::Semi(guard)) =
            package.get_stmt(stmts[stmts.len() - 3]).kind
        else {
            panic!("expected a guard statement");
        };
        let ExprKind::If(cond, _, _) = package.get_expr(guard).kind else {
            panic!("expected a conditional guard");
        };
        let write = slot_assignment(package, slots.has_returned);
        let write_stmt = alloc_semi_stmt(package, assigner, write, PackageSpan::default());
        let value = alloc_bool_lit(package, assigner, true, PackageSpan::default());
        let value_stmt = alloc_expr_stmt(package, assigner, value, PackageSpan::default());
        let body = alloc_block(
            package,
            assigner,
            vec![write_stmt, value_stmt],
            Ty::Prim(Prim::Bool),
            PackageSpan::default(),
        );
        package.exprs.get_mut(cond).expect("condition").kind = ExprKind::Block(body);
    });
}

#[test]
fn continuation_return_preserves_guard_and_final_merge() {
    check_simplify_rule_q(
        indoc! {r#"
        function Main() : Int {
            if false { return 1; }
            if true { if true { return 9; } 5 } else { 6 }
        }
        "#},
        "Main",
        "guard_clause",
        |p, a, _pkg_id, b, s| {
            let before = p.get_block(b).stmts.clone();
            let fired = guard_clause::apply(p, a, b, s);
            assert!(!fired, "continuation return requires the final merge");
            assert_eq!(p.get_block(b).stmts, before);
            fired
        },
        &expect![[r#"
            // before guard_clause (fired=false)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if false {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                let __trailing_result : Int = if (not __has_returned) {
                    if true {
                        if true {
                            {
                                __ret_val = 9;
                                __has_returned = true;
                            };
                        }

                        5
                    } else {
                        6
                    }

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
                if false {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                let __trailing_result : Int = if (not __has_returned) {
                    if true {
                        if true {
                            {
                                __ret_val = 9;
                                __has_returned = true;
                            };
                        }

                        5
                    } else {
                        6
                    }

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
        |p, a, _pkg_id, b, s| guard_clause::apply(p, a, b, s),
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

                let __trailing_result : Int = if (not __has_returned) {
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
fn inverted_guard_continuation_return_preserves_final_merge() {
    check_simplify_rule_q(
        indoc! {r#"
        function Main() : Int {
            if true {} else { return 1; }
            if true { if true { return 9; } 5 } else { 6 }
        }
        "#},
        "Main",
        "guard_clause",
        |p, a, _pkg_id, b, s| {
            let before = p.get_block(b).stmts.clone();
            let fired = guard_clause::apply(p, a, b, s);
            assert!(!fired, "continuation return requires the final merge");
            assert_eq!(p.get_block(b).stmts, before);
            fired
        },
        &expect![[r#"
            // before guard_clause (fired=false)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if true {} else {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                let __trailing_result : Int = if (not __has_returned) {
                    if true {
                        if true {
                            {
                                __ret_val = 9;
                                __has_returned = true;
                            };
                        }

                        5
                    } else {
                        6
                    }

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
                if true {} else {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                let __trailing_result : Int = if (not __has_returned) {
                    if true {
                        if true {
                            {
                                __ret_val = 9;
                                __has_returned = true;
                            };
                        }

                        5
                    } else {
                        6
                    }

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
fn continuation_ordinary_local_mutation_still_collapses() {
    check_simplify_rule_q(
        indoc! {r#"
        function Main() : Int {
            mutable value = 0;
            if false { return 1; }
            if true { set value += 2; value } else { 6 }
        }
        "#},
        "Main",
        "guard_clause",
        |p, a, _pkg_id, b, s| {
            let fired = guard_clause::apply(p, a, b, s);
            assert!(fired, "ordinary local writes do not require the merge");
            fired
        },
        &expect![[r#"
            // before guard_clause (fired=true)
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                mutable value : Int = 0;
                if false {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                let __trailing_result : Int = if (not __has_returned) {
                    if true {
                        value += 2;
                        value
                    } else {
                        6
                    }

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
                mutable value : Int = 0;
                if false {
                    1
                } else {
                    if true {
                        value += 2;
                        value
                    } else {
                        6
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn guard_clause_with_intervening_user_binding_does_not_collapse() {
    // The guarded user binding separates the guard from the trailing result.
    // It must not be mistaken for that result after a later let-folding pass.
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
        |p, a, _pkg_id, b, s| guard_clause::apply(p, a, b, s),
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

                let y : Int = if (not __has_returned) {
                    2
                } else {
                    0
                };
                let __trailing_result : Int = if (not __has_returned) {
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

                let y : Int = if (not __has_returned) {
                    2
                } else {
                    0
                };
                let __trailing_result : Int = if (not __has_returned) {
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
fn guard_clause_rule_leaves_chained_guards_unchanged() {
    // The second guard is guarded by the flag that the first guard may set.
    // Preserve the merge rather than discarding an earlier return.
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
        |p, a, _pkg_id, b, s| guard_clause::apply(p, a, b, s),
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

                if (not __has_returned) {
                    if false {
                        {
                            __ret_val = 2;
                            __has_returned = true;
                        };
                    }

                };
                let __trailing_result : Int = if (not __has_returned) {
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

                if (not __has_returned) {
                    if false {
                        {
                            __ret_val = 2;
                            __has_returned = true;
                        };
                    }

                };
                let __trailing_result : Int = if (not __has_returned) {
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
        |p, a, _pkg_id, b, s| guard_clause::apply(p, a, b, s),
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
        |p, a, _pkg_id, b, s| guard_clause::apply(p, a, b, s),
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
//     sequence, foreign stmt between sets) that flag lowering
//     would not produce, but that pin the matcher's discipline
//     against future drift.
// ---------------------------------------------------------------------------

mod inverted_orientation {
    use expect_test::expect;
    use indoc::indoc;

    use qsc_fir::fir::PackageSpan;
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
        alloc_expr, alloc_expr_stmt, alloc_if_expr, alloc_local_stmt, alloc_local_var,
        alloc_local_var_expr, alloc_not_expr, alloc_semi_stmt,
    };
    use crate::return_unify::simplify::guard_clause;
    use crate::return_unify::symbols;
    use crate::return_unify::tests::check_simplify_rule_q;

    /// Slot identities and declarations for direct-FIR inverted-guard fixtures.
    struct Slots {
        has_returned: LocalVarId,
        ret_val: LocalVarId,
        declarations: [StmtId; 2],
    }

    fn alloc_slots(package: &mut Package, assigner: &mut Assigner) -> Slots {
        let flag = alloc_bool_lit(package, assigner, false, PackageSpan::default());
        let (has_returned, flag_decl) = alloc_local_var(
            package,
            assigner,
            symbols::HAS_RETURNED,
            &Ty::Prim(Prim::Bool),
            flag,
            Mutability::Mutable,
        );
        let value = alloc_expr(
            package,
            assigner,
            Ty::Prim(Prim::Int),
            ExprKind::Lit(Lit::Int(0)),
            PackageSpan::default(),
        );
        let (ret_val, value_decl) = alloc_local_var(
            package,
            assigner,
            symbols::RET_VAL,
            &Ty::Prim(Prim::Int),
            value,
            Mutability::Mutable,
        );
        Slots {
            has_returned,
            ret_val,
            declarations: [flag_decl, value_decl],
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
            PackageSpan::default(),
        );
        let assign = alloc_assign_expr(package, assigner, lhs, v_id, PackageSpan::default());
        alloc_semi_stmt(package, assigner, assign, PackageSpan::default())
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
            PackageSpan::default(),
        );
        let rhs = alloc_bool_lit(package, assigner, true, PackageSpan::default());
        let assign = alloc_assign_expr(package, assigner, lhs, rhs, PackageSpan::default());
        alloc_semi_stmt(package, assigner, assign, PackageSpan::default())
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
            PackageSpan::default(),
        );
        alloc_block_expr(package, assigner, bid, Ty::UNIT, PackageSpan::default())
    }

    /// Build an empty Unit-typed block expr — the only then-arm shape
    /// `identify_guard_else_arm` accepts.
    fn build_empty_unit_block_expr(package: &mut Package, assigner: &mut Assigner) -> ExprId {
        let bid = alloc_block(
            package,
            assigner,
            Vec::new(),
            Ty::UNIT,
            PackageSpan::default(),
        );
        alloc_block_expr(package, assigner, bid, Ty::UNIT, PackageSpan::default())
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
            PackageSpan::default(),
        );
        alloc_semi_stmt(package, assigner, if_expr, PackageSpan::default())
    }

    /// Build the lazy trailing-result binding and the merge's read of it.
    fn build_continuation_stmt(
        package: &mut Package,
        assigner: &mut Assigner,
        slots: &Slots,
        rest_block_expr_id: ExprId,
    ) -> (StmtId, ExprId) {
        let bool_ty = Ty::Prim(Prim::Bool);
        let return_ty = package.get_expr(rest_block_expr_id).ty.clone();
        let fallback = alloc_local_var_expr(
            package,
            assigner,
            slots.ret_val,
            return_ty.clone(),
            PackageSpan::default(),
        );
        let flag_read = alloc_local_var_expr(
            package,
            assigner,
            slots.has_returned,
            bool_ty.clone(),
            PackageSpan::default(),
        );
        let not_flag = alloc_not_expr(package, assigner, flag_read, PackageSpan::default());
        let if_expr = alloc_if_expr(
            package,
            assigner,
            not_flag,
            rest_block_expr_id,
            Some(fallback),
            return_ty.clone(),
            PackageSpan::default(),
        );
        let (local, binding) = alloc_local_var(
            package,
            assigner,
            symbols::TRAILING_RESULT,
            &return_ty,
            if_expr,
            Mutability::Immutable,
        );
        let read =
            alloc_local_var_expr(package, assigner, local, return_ty, PackageSpan::default());
        (binding, read)
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
            PackageSpan::default(),
        );
        let then_var = alloc_local_var_expr(
            package,
            assigner,
            slots.ret_val,
            return_ty.clone(),
            PackageSpan::default(),
        );
        let then_stmt = alloc_expr_stmt(package, assigner, then_var, PackageSpan::default());
        let then_bid = alloc_block(
            package,
            assigner,
            vec![then_stmt],
            return_ty.clone(),
            PackageSpan::default(),
        );
        let then_expr = alloc_block_expr(
            package,
            assigner,
            then_bid,
            return_ty.clone(),
            PackageSpan::default(),
        );
        let merge = alloc_if_expr(
            package,
            assigner,
            cond,
            then_expr,
            Some(fallthrough),
            return_ty.clone(),
            PackageSpan::default(),
        );
        alloc_expr_stmt(package, assigner, merge, PackageSpan::default())
    }

    /// Build the rest-block expression `{ rest_value }` (single trailing
    /// `Expr` carrying the supplied value of type `return_ty`). Returns
    /// `(block_id, block_expr_id)` so callers can use the expression in
    /// the fixture and assert that the rewrite preserves the rest block.
    fn build_rest_block_expr(
        package: &mut Package,
        assigner: &mut Assigner,
        rest_value: ExprId,
        return_ty: &Ty,
    ) -> (qsc_fir::fir::BlockId, ExprId) {
        let rest_stmt = alloc_expr_stmt(package, assigner, rest_value, PackageSpan::default());
        let rest_bid = alloc_block(
            package,
            assigner,
            vec![rest_stmt],
            return_ty.clone(),
            PackageSpan::default(),
        );
        let expr_id = alloc_block_expr(
            package,
            assigner,
            rest_bid,
            return_ty.clone(),
            PackageSpan::default(),
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
            PackageSpan::default(),
        );
        (cond_local, cond_expr)
    }

    /// Assert the rewrite output is the canonical
    /// `if not <cond_local> { v_id } else { <rest_bid> }` shape.
    /// The rewriter reuses the continuation expression; checking its
    /// block id verifies that the original rest block is preserved.
    fn assert_rewrite_shape(
        package: &Package,
        block_id: qsc_fir::fir::BlockId,
        cond_local: LocalVarId,
        v_id: ExprId,
        rest_bid: qsc_fir::fir::BlockId,
        prefix: &[StmtId],
    ) {
        let stmts = &package.get_block(block_id).stmts;
        assert_eq!(
            stmts.len(),
            prefix.len() + 1,
            "block should preserve its prefix and collapse to a trailing Expr stmt"
        );
        assert_eq!(&stmts[..prefix.len()], prefix);
        let StmtKind::Expr(tail_id) = package.get_stmt(stmts[prefix.len()]).kind else {
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
        // Else-arm: the original continuation block expression.
        let ExprKind::Block(else_bid) = &package.get_expr(*new_else_id).kind else {
            panic!("rewritten else-arm should be a Block");
        };
        assert_eq!(
            *else_bid, rest_bid,
            "else-arm should wrap the original rest block id"
        );
    }

    /// Build the fixed-shape inverted-guard block:
    ///   `[ flag_decl, slot_decl, guard_stmt, cont_binding, merge_stmt ]`
    /// Returns the block id, the user-cond local id, the slot RHS expr id,
    /// and the rest-block id preserved by the rewrite.
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
            PackageSpan::default(),
        );
        let then_id = build_empty_unit_block_expr(package, assigner);
        let else_id = build_slot_set_arm_expr(package, assigner, slots, v_id, int_ty);
        let guard_stmt = build_inverted_guard_stmt(package, assigner, cond_expr, then_id, else_id);

        let rest_value = alloc_expr(
            package,
            assigner,
            int_ty.clone(),
            ExprKind::Lit(Lit::Int(7)),
            PackageSpan::default(),
        );
        let (rest_bid, rest_block_expr) =
            build_rest_block_expr(package, assigner, rest_value, int_ty);
        let (cont_stmt, fallthrough) =
            build_continuation_stmt(package, assigner, slots, rest_block_expr);
        let merge_stmt = build_merge_stmt(package, assigner, slots, fallthrough, int_ty);

        let block_id = alloc_block(
            package,
            assigner,
            vec![
                slots.declarations[0],
                slots.declarations[1],
                guard_stmt,
                cont_stmt,
                merge_stmt,
            ],
            int_ty.clone(),
            PackageSpan::default(),
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
            |p, a, _pkg_id, b, s| guard_clause::apply(p, a, b, s),
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

                    let __trailing_result : Int = if (not __has_returned) {
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
                    if (not true) {
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
        let slots = alloc_slots(&mut package, &mut assigner);
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
            PackageSpan::default(),
        );
        let (_outer_local, outer_pat) = alloc_bind_pat(
            &mut package,
            &mut assigner,
            "x",
            int_ty.clone(),
            PackageSpan::default(),
        );
        let outer_local_stmt = alloc_local_stmt(
            &mut package,
            &mut assigner,
            Mutability::Immutable,
            outer_pat,
            init_expr,
            PackageSpan::default(),
        );
        let _outer_block = alloc_block(
            &mut package,
            &mut assigner,
            vec![outer_local_stmt],
            int_ty.clone(),
            PackageSpan::default(),
        );

        let synth_slots =
            crate::return_unify::tests::synth_slots_for_block(&package, inner_block_id);
        let fired = guard_clause::apply(&mut package, &mut assigner, inner_block_id, &synth_slots);
        assert!(
            fired,
            "guard_clause must fire on the inverted shape inside a Local init"
        );
        assert_rewrite_shape(
            &package,
            inner_block_id,
            cond_local,
            v_id,
            rest_bid,
            &slots.declarations,
        );
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
        let slots = alloc_slots(&mut package, &mut assigner);
        let int_ty = Ty::Prim(Prim::Int);

        let (inner_block_id, cond_local, v_id, rest_bid) =
            build_canonical_block(&mut package, &mut assigner, &slots, &int_ty);

        let inner_block_expr = alloc_block_expr(
            &mut package,
            &mut assigner,
            inner_block_id,
            int_ty.clone(),
            PackageSpan::default(),
        );
        let wrapper_stmt = alloc_expr_stmt(
            &mut package,
            &mut assigner,
            inner_block_expr,
            PackageSpan::default(),
        );
        let _outer_block = alloc_block(
            &mut package,
            &mut assigner,
            vec![wrapper_stmt],
            int_ty.clone(),
            PackageSpan::default(),
        );

        let synth_slots =
            crate::return_unify::tests::synth_slots_for_block(&package, inner_block_id);
        let fired = guard_clause::apply(&mut package, &mut assigner, inner_block_id, &synth_slots);
        assert!(
            fired,
            "guard_clause must fire on the inverted shape inside a nested block"
        );
        assert_rewrite_shape(
            &package,
            inner_block_id,
            cond_local,
            v_id,
            rest_bid,
            &slots.declarations,
        );
    }

    fn inverted_return_arm(
        package: &Package,
        block: qsc_fir::fir::BlockId,
    ) -> qsc_fir::fir::BlockId {
        let stmts = &package.get_block(block).stmts;
        let StmtKind::Semi(guard) = package.get_stmt(stmts[stmts.len() - 3]).kind else {
            panic!("expected a guard statement");
        };
        let ExprKind::If(_, _, Some(otherwise)) = package.get_expr(guard).kind else {
            panic!("expected an inverted guard");
        };
        let ExprKind::Block(arm) = package.get_expr(otherwise).kind else {
            panic!("expected a return arm block");
        };
        arm
    }

    #[test]
    fn inverted_guard_without_flag_assignment_is_not_collapsed() {
        // MANUAL-FIR: source lowering never omits the flag write from a return arm.
        let mut package = Package::default();
        let mut assigner = Assigner::default();
        let slots = alloc_slots(&mut package, &mut assigner);
        let (block, _, _, _) =
            build_canonical_block(&mut package, &mut assigner, &slots, &Ty::Prim(Prim::Int));
        let arm = inverted_return_arm(&package, block);
        package
            .blocks
            .get_mut(arm)
            .expect("return arm")
            .stmts
            .pop()
            .expect("flag assignment");
        let synth_slots = crate::return_unify::tests::synth_slots_for_block(&package, block);
        super::assert_guard_clause_rejected_without_changes(
            &mut package,
            &mut assigner,
            block,
            &synth_slots,
        );
    }

    #[test]
    fn inverted_guard_with_extra_statement_is_not_collapsed() {
        // MANUAL-FIR: source lowering never interposes a statement between the slot writes.
        let mut package = Package::default();
        let mut assigner = Assigner::default();
        let slots = alloc_slots(&mut package, &mut assigner);
        let (block, _, _, _) =
            build_canonical_block(&mut package, &mut assigner, &slots, &Ty::Prim(Prim::Int));
        let arm = inverted_return_arm(&package, block);
        let foreign_expr = alloc_expr(
            &mut package,
            &mut assigner,
            Ty::UNIT,
            ExprKind::Tuple(Vec::new()),
            PackageSpan::default(),
        );
        let foreign_stmt = alloc_semi_stmt(
            &mut package,
            &mut assigner,
            foreign_expr,
            PackageSpan::default(),
        );
        package
            .blocks
            .get_mut(arm)
            .expect("return arm")
            .stmts
            .insert(1, foreign_stmt);
        let synth_slots = crate::return_unify::tests::synth_slots_for_block(&package, block);
        super::assert_guard_clause_rejected_without_changes(
            &mut package,
            &mut assigner,
            block,
            &synth_slots,
        );
    }
}
