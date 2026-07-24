// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn triple_nested_if_return_with_else_return_value_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if 0 > 0 {
                    if 0 > 0 {
                        if 0 > 0 { return 1; }
                        return 0;
                    }
                    0
                } else {
                    return 2;
                }
            }
        }
    "#});
}

/// Simpler variant: return only in else branch with false condition.
/// Checks whether the bug requires deep nesting or just else-return under
/// a false condition. Driven through `check_semantic_equivalence`.

#[test]
fn else_return_under_false_condition_semantic() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            function Main() : Int {
                if 0 > 0 { 42 } else { return 0; }
            }
        }
    "#});
}

/// Structural snapshot: verifies the bind-then-check pattern in the FIR
/// output for the triple-nested if-return case. The trailing
/// expression is bound to `__trailing_result` before the `__has_returned`
/// flag is checked.

#[test]
fn triple_nested_if_return_with_else_return() {
    check_no_returns_q(
        indoc! {r#"
        namespace Test {
            function Main() : Int {
                if 0 > 0 {
                    if 0 > 0 {
                        if 0 > 0 { return 1; }
                        return 0;
                    }
                    0
                } else {
                    return 2;
                }
            }
        }
    "#},
        &expect![[r#"
            function Main() : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let __trailing_result : Int = if (0 > 0) {
                    if (0 > 0) {
                        if (0 > 0) {
                            {
                                __ret_val = 1;
                                __has_returned = true;
                            };
                        }

                        if (not __has_returned) {
                            {
                                __ret_val = 0;
                                __has_returned = true;
                            };
                        };
                    }

                    0
                } else {
                    {
                        __ret_val = 2;
                        __has_returned = true;
                    };
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
fn guard_clause_simplification_preserves_releases_on_all_paths() {
    let source = indoc! {r#"
        namespace Test {
            operation Foo(flag : Bool) : Int {
                use q = Qubit();
                if flag {
                    return 1;
                }
                0
            }

            @EntryPoint()
            operation Main() : Int {
                Foo(true)
            }
        }
    "#};

    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);

    let body_block_id = find_body_block_id(package, "Foo");
    let body_block = package.get_block(body_block_id);

    let release_callables = collect_release_callables(&store);
    let release_indices = body_block
        .stmts
        .iter()
        .enumerate()
        .filter_map(|(index, &stmt_id)| {
            is_release_call_test(package, stmt_id, &release_callables).then_some(index)
        })
        .collect::<Vec<_>>();
    assert!(
        release_indices.is_empty(),
        "return-unify simplification should not keep a top-level release suffix after path-local releases"
    );

    let has_path_local_release = body_block.stmts.iter().any(|&stmt_id| {
        stmt_contains_path_local_release_value(package, stmt_id, &release_callables)
    });
    assert!(
        has_path_local_release,
        "return-unify simplification must preserve release calls inside value-producing paths"
    );

    let trailing_stmt_id = *body_block
        .stmts
        .last()
        .expect("Foo body should not be empty");
    let StmtKind::Expr(trailing_expr_id) = package.get_stmt(trailing_stmt_id).kind else {
        panic!("Foo body should end with a trailing expression");
    };
    assert_eq!(
        package.get_expr(trailing_expr_id).ty,
        Ty::Prim(Prim::Int),
        "Foo body should keep an Int-producing trailing expression"
    );

    check_semantic_equivalence(source);
}

#[test]
fn if_both_return_release_suffix_before_after_qsharp() {
    check_pre_fir_transforms_to_return_unify_q(
        indoc! {r#"
            namespace Test {
                operation Foo(flag : Bool) : Int {
                    use q = Qubit();
                    if flag {
                        return 1;
                    } else {
                        return 0;
                    }
                }

                @EntryPoint()
                operation Main() : Int {
                    Foo(true)
                }
            }
        "#},
        &expect![[r#"
            // before fir transforms
            operation Foo(flag : Bool) : Int {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_65 : Unit = if flag {
                    {
                        let _generated_ident_41 : Int = 1;
                        __quantum__rt__qubit_release(q);
                        return _generated_ident_41;
                    };
                } else {
                    {
                        let _generated_ident_53 : Int = 0;
                        __quantum__rt__qubit_release(q);
                        return _generated_ident_53;
                    };
                };
                __quantum__rt__qubit_release(q);
                _generated_ident_65
            }
            operation Main() : Int {
                Foo(true)
            }
            // entry
            Main()

            // post return_unify
            operation Foo(flag : Bool) : Int {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_65 : Unit = if flag {
                    {
                        let _generated_ident_41 : Int = 1;
                        __quantum__rt__qubit_release(q);
                        {
                            __ret_val = _generated_ident_41;
                            __has_returned = true;
                        };
                    };
                } else {
                    {
                        let _generated_ident_53 : Int = 0;
                        __quantum__rt__qubit_release(q);
                        {
                            __ret_val = _generated_ident_53;
                            __has_returned = true;
                        };
                    };
                };
                if (not __has_returned) {
                    __quantum__rt__qubit_release(q);
                };
                __ret_val
            }
            operation Main() : Int {
                Foo(true)
            }
            // entry
            Main()
        "#]],
    );
}

/// A user binding whose name collides with the
/// synthesized `__trailing_result` slot must survive the simplifier
/// unfolded.
///
/// The trailing-result fold rules ([`crate::return_unify::simplify::let_folding`]
/// and [`crate::return_unify::simplify::single_branch`]) recognize the
/// synthesized trailing binding by its [`LocalVarId`] — threaded as
/// `SynthSlots` from the transform phase — and not by the
/// `"__trailing_result"` name that is still emitted into FIR for readable
/// dumps. Leading-underscore identifiers are not language-reserved, so a
/// user may legally bind a local named `__trailing_result`.
///
/// This fixture declares such a user local, of the matching `Int` type,
/// ahead of an early `return` that forces the merge to be synthesized. The
/// initializer carries an unconditional side effect (`set observed += 1`)
/// that the early-return guard then observes. Because the user local's id
/// differs from the synthesized slot id, the fold rules must leave the
/// binding — and its side effect — in unconditional binding position.
///
/// Were the rules to match by the `__trailing_result` *name* (the behavior
/// before the id-based threading), the user binding could be folded into a
/// conditional merge arm, deferring its side effect off the early-return
/// path. The guard `if observed == 1` would then read the pre-increment
/// value, fall through, and the program would return `99` instead of `1` —
/// a silent miscompile. Driving the assertion through the real
/// `compile_return_unified` pipeline (rather than a name-scanning slot
/// helper) is what makes the collision check meaningful.
#[test]
fn user_local_named_like_trailing_result_survives_simplify() {
    let source = indoc! {r#"
        namespace Test {
            operation Probe() : Int {
                mutable observed = 0;
                let __trailing_result = {
                    set observed += 1;
                    99
                };
                if observed == 1 {
                    return observed;
                }
                __trailing_result
            }

            @EntryPoint()
            operation Main() : Int {
                Probe()
            }
        }
    "#};

    // Run the full mono + return_unify (including simplify) pipeline. This
    // threads the authoritative synthesized slot ids captured at transform
    // time; the user `__trailing_result` is a distinct local that the
    // id-based fold rules must not touch.
    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);

    // The user binding (a side-effecting block initializer) must still be
    // present, unfolded, in the body block.
    let user_init_id = package
        .get_block(find_body_block_id(package, "Probe"))
        .stmts
        .iter()
        .find_map(|&stmt_id| {
            let StmtKind::Local(_, pat_id, init_id) = package.get_stmt(stmt_id).kind else {
                return None;
            };
            let PatKind::Bind(ident) = &package.get_pat(pat_id).kind else {
                return None;
            };
            // The user binding keeps its literal source name `__trailing_result`;
            // the sentinel rename only affects *synthesized* names (which now
            // carry a `.`, e.g. `_.trailing_result`), so match the user's name
            // directly rather than `symbols::TRAILING_RESULT`.
            (ident.name.as_ref() == "__trailing_result").then_some(init_id)
        })
        .expect("user `__trailing_result` binding must survive simplify");

    assert!(
        matches!(package.get_expr(user_init_id).kind, ExprKind::Block(_)),
        "user `__trailing_result` initializer must stay an unconditional block, \
         not be folded into a conditional merge arm"
    );

    // End-to-end correctness: the unconditional side effect still runs on
    // the early-return path, so the program returns 1 (not 99).
    check_semantic_equivalence(source);
}

/// A user binding whose name collides with the synthesized `__ret_val`
/// return-value slot must not be confused with that slot.
///
/// The return-value collapse rules in
/// [`crate::return_unify::simplify`] and the flag-elimination rule in
/// [`crate::return_unify::simplify::dead_flag`] recover the synthesized
/// return-value slot by its [`LocalVarId`] — threaded as `SynthSlots` from
/// the transform phase — and never by the `"__ret_val"` name still emitted
/// into FIR for readable dumps. Because leading-underscore identifiers are
/// legal Q# (the lexer admits any identifier starting with `_`), a user may
/// bind a local literally named `__ret_val`, colliding with
/// [`symbols::RET_VAL`].
///
/// This fixture declares such a user local alongside an early `return` that
/// forces a synthesized `__ret_val` slot to be created. After the pipeline,
/// the body contains two *distinct* `__ret_val` locals — the synthesized
/// slot (initialized to `0`) and the user binding (initialized to `99`) —
/// proving the id-based gates kept them separate rather than merging the
/// collision into one slot. The user binding is then reassigned
/// (`set __ret_val = __ret_val + observed`) on the fall-through path; the
/// merge reads the *synthesized* slot on the returned path and the *user*
/// slot on the trailing path, so the two ids must stay distinct for the
/// program to stay correct.
///
/// Were the rules to match by the `__ret_val` *name* (the behavior before
/// id-based threading), the synthesized return value could be written into,
/// or read from, the user's `__ret_val`, and `Probe(false)` would no longer
/// return `100`. Driving the assertion through the real
/// `compile_return_unified` pipeline plus `check_semantic_equivalence` is
/// what makes the collision check meaningful.
#[test]
fn user_binding_named_like_synth_slot_is_not_confused() {
    let source = indoc! {r#"
        namespace Test {
            operation Probe(flag : Bool) : Int {
                mutable observed = 0;
                mutable __ret_val = 99;
                if flag {
                    return 1;
                }
                set observed += 1;
                set __ret_val = __ret_val + observed;
                __ret_val
            }

            @EntryPoint()
            operation Main() : Int {
                Probe(false)
            }
        }
    "#};

    // Run the full mono + return_unify (including simplify) pipeline. This
    // threads the authoritative synthesized slot ids captured at transform
    // time; the user `__ret_val` is a distinct local that the id-based
    // collapse and flag-elimination rules must not fold into the slot.
    let (store, pkg_id) = compile_return_unified(source);
    let package = store.get(pkg_id);

    // Collect every `__ret_val`-named local binding in Probe's body, keyed
    // by its (distinct) `LocalVarId`, recording mutability, type, and the
    // initializer literal so the user binding and synth slot can be told
    // apart by id even though they share the emitted name.
    let mut ret_val_locals: Vec<(LocalVarId, qsc_fir::fir::Mutability, Ty, Option<i64>)> = package
        .get_block(find_body_block_id(package, "Probe"))
        .stmts
        .iter()
        .filter_map(|&stmt_id| {
            let StmtKind::Local(mutability, pat_id, init_id) = package.get_stmt(stmt_id).kind
            else {
                return None;
            };
            let pat = package.get_pat(pat_id);
            let PatKind::Bind(ident) = &pat.kind else {
                return None;
            };
            // The synthesized return-value slot now carries the `.` sentinel
            // (`_.ret_val` = `symbols::RET_VAL`) while the user binding keeps
            // its literal source name `__ret_val`; the two render identically
            // (`render_ident` maps `.` -> `_`) but are distinct in-memory.
            // Collect both so the synth slot and user binding can be told apart
            // by their (distinct) `LocalVarId`.
            if ident.name.as_ref() != symbols::RET_VAL && ident.name.as_ref() != "__ret_val" {
                return None;
            }
            let init_lit = match package.get_expr(init_id).kind {
                ExprKind::Lit(Lit::Int(value)) => Some(value),
                _ => None,
            };
            Some((ident.id, mutability, pat.ty.clone(), init_lit))
        })
        .collect();
    ret_val_locals.sort_by_key(|(_, _, _, init_lit)| *init_lit);

    // Exactly two `__ret_val` locals survive, and they carry distinct ids:
    // the collision was preserved as two separate slots, never merged.
    assert_eq!(
        ret_val_locals.len(),
        2,
        "expected the synth `__ret_val` slot and the user `__ret_val` binding \
         to coexist; found {ret_val_locals:?}"
    );
    assert_ne!(
        ret_val_locals[0].0, ret_val_locals[1].0,
        "synth and user `__ret_val` must have distinct LocalVarIds"
    );

    // The synthesized return-value slot is the `Int`-typed `mutable` local
    // initialized to `0`.
    assert!(
        matches!(
            &ret_val_locals[0],
            (
                _,
                qsc_fir::fir::Mutability::Mutable,
                Ty::Prim(Prim::Int),
                Some(0)
            )
        ),
        "synthesized `__ret_val` slot must remain a mutable Int initialized to 0; \
         found {:?}",
        ret_val_locals[0]
    );

    // The user binding survives untouched: still a `mutable Int` initialized
    // to its source literal `99`.
    assert!(
        matches!(
            &ret_val_locals[1],
            (
                _,
                qsc_fir::fir::Mutability::Mutable,
                Ty::Prim(Prim::Int),
                Some(99)
            )
        ),
        "user `__ret_val` binding must survive as a mutable Int initialized to 99; \
         found {:?}",
        ret_val_locals[1]
    );

    // End-to-end correctness: with `flag` false the early return is skipped,
    // the user `__ret_val` is reassigned to 99 + 1, and the program returns
    // 100 — proving the synth slot and user binding were never conflated.
    check_semantic_equivalence(source);
}

/// Minimal reproducer: a `return` buried in a `BinOp` operand block must
/// short-circuit before the addition (and its quantum side effects) run.
#[test]
fn operand_return_in_binop_block_short_circuits() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Unit {
                use q = Qubit();
                let x = 1 + { X(q); return (); 2 };
            }
        }
    "#});
}

#[test]
fn return_buried_in_if_condition_block_short_circuits() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Int {
                if { let y = { return 5; 0 }; y > 0 } {
                    return 1;
                }
                return 2;
            }
        }
    "#});
}

#[test]
fn while_with_return_in_if_condition_short_circuits() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Foo(cond : Bool) : Int {
                if { while cond { return 7; } true } {
                    return 1;
                }
                return 2;
            }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let c = M(q) == One;
                Foo(c)
            }
        }
    "#});
}

#[test]
fn return_in_if_condition_with_else_skips_both_branches() {
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable hit = 0;
                if { return 5; true } {
                    set hit = 1;
                } else {
                    set hit = 2;
                }
                return hit;
            }
        }
    "#});
}

#[test]
fn return_in_both_while_condition_and_body_short_circuits() {
    // Interaction: a `while` whose condition block contains a conditional
    // `return` *and* whose body contains a conditional `return`. The
    // condition return must win when it fires first (loop iteration that
    // re-evaluates the condition after the body bumped `i` past the guard),
    // exercising the condition-guard and body-guard flag lowering together
    // rather than in isolation.
    check_semantic_equivalence(indoc! {r#"
        namespace Test {
            operation Main() : Int {
                mutable i = 0;
                while { if i > 5 { return 99; } i < 10 } {
                    if i == 3 {
                        return i;
                    }
                    i += 2;
                }
                -1
            }
        }
    "#});
}
