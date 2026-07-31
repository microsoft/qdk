// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, span::Span,
    target::TargetCapabilityFlags,
};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_hir::{
    assigner::Assigner,
    hir::{Block, Expr, ExprKind, Lit, PatKind, Res, Stmt, StmtKind},
    mut_visit::MutVisitor,
    ty::{Prim, Ty},
    validate::Validator,
    visit::{self, Visitor},
};

use crate::common::gen_ident;
use crate::loop_normalize::LoopNormalize;

/// Compiles `file`, runs [`LoopNormalize`] once over the package, asserts the
/// package still validates and no rejection diagnostics were produced, then
/// snapshots the transformed package.
fn check(file: &str, expect: &Expect) {
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let errors = {
        let mut pass = LoopNormalize::new(&mut unit.assigner);
        pass.visit_package(&mut unit.package);
        pass.errors
    };
    assert!(errors.is_empty(), "unexpected rejection errors: {errors:?}");
    Validator::default().visit_package(&unit.package);
    expect.assert_eq(&crate::qsharp_gen::write_package_qsharp(
        &store,
        &unit.package,
    ));
}

/// Compiles `file`, runs [`LoopNormalize`] once, snapshots the rejection
/// diagnostics it produced, and returns the transformed package text.
fn check_errors(file: &str, expect: &Expect) -> String {
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let errors = {
        let mut pass = LoopNormalize::new(&mut unit.assigner);
        pass.visit_package(&mut unit.package);
        pass.errors
    };
    // The package must remain structurally valid even on the rejection path.
    Validator::default().visit_package(&unit.package);
    expect.assert_debug_eq(&errors);
    crate::qsharp_gen::write_package_qsharp(&store, &unit.package)
}

/// Compiles `file`, runs [`LoopNormalize`] once, records the package, runs it a
/// second time, asserts the package is unchanged, confirming the pass is
/// idempotent, and returns the stable package text.
fn check_idempotent(file: &str) -> String {
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    LoopNormalize::new(&mut unit.assigner).visit_package(&mut unit.package);
    let after_first = crate::qsharp_gen::write_package_qsharp(&store, &unit.package);

    LoopNormalize::new(&mut unit.assigner).visit_package(&mut unit.package);
    let after_second = crate::qsharp_gen::write_package_qsharp(&store, &unit.package);

    assert_eq!(
        after_first, after_second,
        "second run of LoopNormalize changed the package"
    );
    after_second
}

/// Compiles `file`, runs [`LoopNormalize`] once, validates the result, and
/// returns the generated Q# text for targeted structural assertions.
fn normalize_to_string(file: &str) -> String {
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), file.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);

    let errors = {
        let mut pass = LoopNormalize::new(&mut unit.assigner);
        pass.visit_package(&mut unit.package);
        pass.errors
    };
    assert!(errors.is_empty(), "unexpected rejection errors: {errors:?}");
    Validator::default().visit_package(&unit.package);
    crate::qsharp_gen::write_package_qsharp(&store, &unit.package)
}

fn operand_temp_bind_count(package: &str) -> usize {
    package.matches("let _operand_tmp").count()
}

/// Returns whether `package` contains a chained operand-temp copy, that is a
/// generated temp whose initializer is nothing but a read of another generated
/// temp, such as `let _operand_tmp_9 = _operand_tmp_5;` or its array-backed
/// form `let _operand_tmp_9 = _operand_tmp_5[0];`.
fn has_chained_operand_temp_copy(package: &str) -> bool {
    package.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with("let _operand_tmp")
            && line
                .split_once('=')
                .is_some_and(|(_, init)| init.trim_start().starts_with("_operand_tmp"))
    })
}

#[derive(Default)]
struct AncestorSpanCollector {
    generated_bindings: Vec<(Span, Span, Span, qsc_hir::hir::NodeId)>,
    generated_reference_spans: Vec<(qsc_hir::hir::NodeId, Span)>,
    user_statement_spans: Vec<Span>,
}

impl<'a> Visitor<'a> for AncestorSpanCollector {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let StmtKind::Local(_, pat, initializer) = &stmt.kind
            && let PatKind::Bind(ident) = &pat.kind
            && ident.name.starts_with(".operand_tmp")
        {
            self.generated_bindings
                .push((stmt.span, pat.span, initializer.span, ident.id));
        }
        self.user_statement_spans.push(stmt.span);
        visit::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let ExprKind::Var(Res::Local(id), _) = &expr.kind {
            self.generated_reference_spans.push((*id, expr.span));
        }
        visit::walk_expr(self, expr);
    }
}

#[test]
fn ancestor_pin_binding_is_non_steppable_and_preserves_operand_spans() {
    let source = indoc! {"
        namespace Test {
            operation Effect(value : Int) : Unit {}
            function Inner(value : Int) : Int { value }
            operation Consume(value : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    ({ Effect(1); Consume })(Inner(if cond { Effect(2); break } else { 3 }));
                }
            }
        }
    "};
    let store = PackageStore::new(compile::core());
    let sources = SourceMap::new([("test".into(), source.into())], None);
    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::all(),
        LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    LoopNormalize::new(&mut unit.assigner).visit_package(&mut unit.package);

    let operand_text = "{ Effect(1); Consume }";
    let operand_lo = source
        .find(operand_text)
        .expect("source operand should exist");
    let operand_span = Span {
        lo: u32::try_from(operand_lo).expect("source offset should fit in u32"),
        hi: u32::try_from(operand_lo + operand_text.len())
            .expect("source offset should fit in u32"),
    };
    let user_stmt_text = "Effect(1);";
    let user_stmt_lo = source
        .find(user_stmt_text)
        .expect("source statement should exist");
    let user_stmt_span = Span {
        lo: u32::try_from(user_stmt_lo).expect("source offset should fit in u32"),
        hi: u32::try_from(user_stmt_lo + user_stmt_text.len())
            .expect("source offset should fit in u32"),
    };

    let mut collector = AncestorSpanCollector::default();
    collector.visit_package(&unit.package);
    let (stmt_span, pat_span, _, local_id) = collector
        .generated_bindings
        .iter()
        .copied()
        .find(|(_, _, initializer_span, _)| *initializer_span == operand_span)
        .expect("outer ancestor binding should retain the operand span");
    assert_eq!(
        stmt_span,
        Span::default(),
        "administrative binding must be non-steppable"
    );
    assert_eq!(
        pat_span, operand_span,
        "generated pattern should retain operand provenance"
    );
    assert!(
        collector
            .generated_reference_spans
            .iter()
            .any(|(id, span)| *id == local_id && *span == operand_span),
        "replacement reference should retain operand provenance"
    );
    assert!(
        collector.user_statement_spans.contains(&user_stmt_span),
        "nested user statement should retain its narrower source span"
    );
}

#[test]
fn plain_and_field_assignment_preserve_abrupt_rhs_order() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            newtype Pair = (First : Int, Second : Int);
            operation Main() : Unit {
                mutable cond = false;
                mutable x = 0;
                mutable pair = Pair(1, 2);
                while true {
                    x = { x = 3; if cond { break; } 4 };
                    pair w/= First <- { x = 5; if cond { break; } 6 };
                    break;
                }
            }
        }
    "});

    let plain_rhs = package
        .find("x = 3")
        .expect("plain RHS effect should remain");
    let plain_write = package
        .rfind("x = _operand_tmp")
        .expect("plain write should remain");
    let field_rhs = package
        .find("x = 5")
        .expect("field RHS effect should remain");
    let field_write = package
        .find("pair w/=::First <- _operand_tmp")
        .expect("field write should remain");
    assert!(
        plain_rhs < plain_write && field_rhs < field_write,
        "writes must follow RHS fall-through\n{package}"
    );
}

#[test]
fn nested_candidate_in_untaken_branch_remains_conditional() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Effect(value : Int) : Unit {}
            operation Consume(value : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = false;
                while true {
                    Consume(if cond { Effect(1); if true { break } else { 2 } } else { 3 });
                    break;
                }
            }
        }
    "});

    let branch = package
        .find("if cond {")
        .expect("conditional branch should remain");
    let effect = package
        .find("Effect(1)")
        .expect("branch effect should remain");
    let abrupt = package.find("break").expect("nested break should remain");
    assert!(
        branch < effect && effect < abrupt,
        "untaken branch work must stay under its condition\n{package}"
    );
}

#[test]
fn outer_for_iterable_evaluates_once_before_nested_break() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Effect(value : Int) : Unit {}
            function MakeRange(value : Int) : Range { value..value }
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    for item in ({ Effect(1); MakeRange })(if cond { Effect(2); break } else { 3 }) {
                        Effect(item);
                    }
                }
            }
        }
    "});

    let first = package
        .find("Effect(1)")
        .expect("iterable prefix should remain");
    let second = package
        .find("Effect(2)")
        .expect("nested effect should remain");
    let for_loop = package.find("for item in").expect("for loop should remain");
    assert!(
        first < second && second < for_loop,
        "iterable work must be staged once before the for loop\n{package}"
    );
}

#[test]
fn range_struct_and_string_prefixes_preserve_order() {
    let package = normalize_to_string(indoc! {r#"
        namespace Test {
            struct Pair { First : Int, Second : Int }
            operation Effect(value : Int) : Int { value }
            function Identity(value : Int) : Int { value }
            operation ConsumeRange(value : Range) : Unit {}
            operation ConsumePair(value : Pair) : Unit {}
            operation ConsumeString(value : String) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    ConsumeRange(Effect(1)..Effect(2)..(if cond { break } else { 3 }));
                    ConsumePair(new Pair { First = Effect(4), Second = if cond { break } else { 5 } });
                    ConsumeString($"{Effect(6)}:{Identity(break)}");
                }
            }
        }
    "#});

    for (prefix, abrupt) in [
        ("Effect(1)", "Effect(2)"),
        ("Effect(4)", "Second"),
        ("Effect(6)", "break"),
    ] {
        let prefix_pos = package
            .find(prefix)
            .unwrap_or_else(|| panic!("missing {prefix}\n{package}"));
        let abrupt_pos = package[prefix_pos..].find(abrupt).map_or_else(
            || panic!("missing {abrupt} after {prefix}\n{package}"),
            |pos| prefix_pos + pos,
        );
        assert!(
            prefix_pos <= abrupt_pos,
            "prefix {prefix} must remain before {abrupt}\n{package}"
        );
    }
}

#[test]
fn controlled_adjoint_nondefaultable_prefix_preserves_shape() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Target(value : Qubit) : Unit is Adj + Ctl {}
            operation Main() : Unit {
                use (control, target) = (Qubit(), Qubit());
                mutable cond = true;
                while cond {
                    (Controlled Adjoint Target)([control], if cond { break } else { target });
                }
            }
        }
    "});

    assert!(
        package.contains("Controlled Adjoint Target"),
        "functor shape must survive\n{package}"
    );
    assert!(
        package.contains("[control]"),
        "control tuple layer must survive\n{package}"
    );
    assert!(
        package.contains("[target]"),
        "non-defaultable target must remain array-backed\n{package}"
    );
}

#[test]
fn logical_assign_if_rhs_is_reshaped() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Main() : Unit {
                mutable cond = false;
                mutable keepGoing = false;
                while cond {
                    keepGoing and= if cond { break } else { true };
                }
            }
        }
    "});

    assert_eq!(
        operand_temp_bind_count(&package),
        1,
        "reshaped assignment RHS should be lifted inside its conditional branch\n{package}"
    );

    expect![[r#"
        operation Main() : Unit {
            mutable cond = false;
            mutable keepGoing = false;
            while cond {
                if keepGoing {
                    let _operand_tmp_36 = if cond {
                        break
                    } else {
                        true
                    };
                    keepGoing = _operand_tmp_36;
                };
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn update_field_evaluates_replacement_before_record_when_hoisting() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            newtype Pair = (A : Int, B : Int);

            operation Main() : Unit {
                mutable cond = true;
                mutable marker = 0;
                while cond {
                    let updated = { marker += 1; Pair(1, 2) } w/ B <- if cond { break } else { 3 };
                }
            }
        }
    "});

    assert_eq!(
        operand_temp_bind_count(&package),
        1,
        "only the replacement operand should be hoisted before a record update\n{package}"
    );

    expect![[r#"
        // newtype Pair
        operation Main() : Unit {
            mutable cond = true;
            mutable marker = 0;
            while cond {
                let _operand_tmp_45 = if cond {
                    break
                } else {
                    3
                };
                let updated = {
                    marker += 1;
                    Pair(1, 2)
                } w/::B <- _operand_tmp_45;
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn update_index_evaluates_index_and_replacement_before_container_when_hoisting() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Main() : Unit {
                mutable cond = true;
                mutable marker = 0;
                while cond {
                    let updated = { marker += 1; [1, 2] } w/ if cond { break } else { 0 } <- 3;
                }
            }
        }
    "});

    assert_eq!(
        operand_temp_bind_count(&package),
        1,
        "only the index operand should be hoisted before an array update\n{package}"
    );

    expect![[r#"
        operation Main() : Unit {
            mutable cond = true;
            mutable marker = 0;
            while cond {
                let _operand_tmp_43 = if cond {
                    break
                } else {
                    0
                };
                let updated = {
                    marker += 1;
                    [1, 2]
                } w/ _operand_tmp_43 <- 3;
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn hoist_break_in_call_argument() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { break } else { 3 });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_33 = if cond {
                        break
                    } else {
                        3
                    };
                    Foo(_operand_tmp_33);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_continue_in_call_argument() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { continue } else { 3 });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_33 = if cond {
                        continue
                    } else {
                        3
                    };
                    Foo(_operand_tmp_33);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_binop_operand() {
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable cond = true;
                    mutable acc = 0;
                    while cond {
                        let y = acc + (if cond { break } else { 3 });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable cond = true;
                mutable acc = 0;
                while cond {
                    let _operand_tmp_33 = acc;
                    let _operand_tmp_37 = if cond {
                        break
                    } else {
                        3
                    };
                    let y = _operand_tmp_33 + _operand_tmp_37;
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_operand_block() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo({ if cond { break }; 3 });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_34 = {
                        if cond {
                            break
                        };
                        3
                    };
                    Foo(_operand_tmp_34);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_nested_operand_blocks() {
    check(
        indoc! {"
            namespace Test {
                function Bar(x : Int) : Int { x }
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(Bar(if cond { break } else { 3 }));
                    }
                }
            }
        "},
        &expect![[r#"
            function Bar(x : Int) : Int {
                x
            }
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_43 = if cond {
                        break
                    } else {
                        3
                    };
                    Foo(Bar(_operand_tmp_43));
                }
            }
        "#]],
    );
}

#[test]
fn ancestor_prefix_preserves_effectful_outer_callee_before_nested_break() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Effect(value : Int) : Unit {}
            operation Inner(value : Int) : Int { value }
            operation Outer(value : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    ({ Effect(1); Outer })(Inner([if cond { Effect(2); break } else { 3 }, 4][0]));
                }
            }
        }
    "});

    let outer_effect = package
        .find("Effect(1)")
        .expect("outer effect should remain");
    let nested_effect = package
        .find("Effect(2)")
        .expect("nested effect should remain");
    assert!(
        outer_effect < nested_effect,
        "outer callee effects must be hoisted before the nested break: {package}"
    );
}

#[test]
fn ancestor_prefix_preserves_effectful_outer_callee_before_nested_continue() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Effect(value : Int) : Unit {}
            operation Inner(value : Int) : Int { value }
            operation Outer(value : Int) : Unit {}
            operation Main() : Unit {
                mutable iterations = 0;
                while iterations < 1 {
                    iterations += 1;
                    ({ Effect(1); Outer })(Inner([if true { Effect(2); continue } else { 3 }, 4][0]));
                }
            }
        }
    "});

    let outer_effect = package
        .find("Effect(1)")
        .expect("outer effect should remain");
    let nested_effect = package
        .find("Effect(2)")
        .expect("nested effect should remain");
    assert!(
        outer_effect < nested_effect,
        "outer callee effects must be hoisted before the nested continue: {package}"
    );
}

#[test]
fn multiple_nested_break_candidates_converge_without_dropping_outer_prefix() {
    // HIR analogue of the FIR
    // `multiple_nested_candidates_converge_without_dropping_outer_prefix`: two
    // candidates in one statement at different depths — a bare operand and one
    // buried a call deeper — ahead of which an effectful callee must stay
    // pinned. Each fixpoint pass lifts one candidate, so the risk is the outer
    // prefix being dropped or re-pinned as later passes rewrite the argument
    // tuple. The snapshot pins the resulting spine; the assertion pins the
    // ordering property the snapshot would otherwise only imply.
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Effect(value : Int) : Unit {}
            operation Inner(value : Int) : Int { value }
            operation Consume(pair : (Int, Int)) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    ({ Effect(1); Consume })((
                        if cond { break } else { 0 },
                        Inner(if cond { break } else { 1 })
                    ));
                }
            }
        }
    "});

    expect![[r#"
        operation Effect(value : Int) : Unit {}
        operation Inner(value : Int) : Int {
            value
        }
        operation Consume(pair : (Int, Int)) : Unit {}
        operation Main() : Unit {
            mutable cond = true;
            while cond {
                let _operand_tmp_71 = {
                    Effect(1);
                    Consume
                };
                let _operand_tmp_67 = if cond {
                    break
                } else {
                    0
                };
                let _operand_tmp_75 = if cond {
                    break
                } else {
                    1
                };
                _operand_tmp_71(_operand_tmp_67, Inner(_operand_tmp_75));
            }
        }
    "#]]
    .assert_eq(&package);

    let outer_effect = package
        .find("Effect(1)")
        .expect("the outer callee effect should survive both lifts");
    let first_candidate = package
        .find("break")
        .expect("the first candidate should be lifted");
    assert!(
        outer_effect < first_candidate,
        "the effectful callee must stay pinned ahead of every nested candidate: {package}"
    );
}

#[test]
fn multiple_nested_continue_candidates_converge_without_dropping_outer_prefix() {
    // Same two-candidate shape with `continue`. The pass must reach the same
    // fixed point: one spine temp per candidate, with the effectful callee
    // pinned once ahead of both and never re-pinned by the second pass.
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Effect(value : Int) : Unit {}
            operation Inner(value : Int) : Int { value }
            operation Consume(pair : (Int, Int)) : Unit {}
            operation Main() : Unit {
                mutable iterations = 0;
                while iterations < 1 {
                    iterations += 1;
                    ({ Effect(1); Consume })((
                        if iterations > 0 { continue } else { 0 },
                        Inner(if iterations > 0 { continue } else { 1 })
                    ));
                }
            }
        }
    "});

    expect![[r#"
        operation Effect(value : Int) : Unit {}
        operation Inner(value : Int) : Int {
            value
        }
        operation Consume(pair : (Int, Int)) : Unit {}
        operation Main() : Unit {
            mutable iterations = 0;
            while iterations < 1 {
                iterations += 1;
                let _operand_tmp_81 = {
                    Effect(1);
                    Consume
                };
                let _operand_tmp_77 = if iterations > 0 {
                    continue
                } else {
                    0
                };
                let _operand_tmp_85 = if iterations > 0 {
                    continue
                } else {
                    1
                };
                _operand_tmp_81(_operand_tmp_77, Inner(_operand_tmp_85));
            }
        }
    "#]]
    .assert_eq(&package);

    let outer_effect = package
        .find("Effect(1)")
        .expect("the outer callee effect should survive both lifts");
    let first_candidate = package
        .find("continue")
        .expect("the first candidate should be lifted");
    assert!(
        outer_effect < first_candidate,
        "the effectful callee must stay pinned ahead of every nested candidate: {package}"
    );
    assert!(
        !has_chained_operand_temp_copy(&package),
        "the second pass must not re-pin the first pass's temps: {package}"
    );
}

#[test]
fn earlier_direct_candidate_wins_before_later_nested_candidate() {
    let package = check_idempotent(indoc! {"
        namespace Test {
            operation Inner(value : Int) : Int { value }
            operation Consume(value : (Qubit, Int)) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                mutable cond = true;
                while cond {
                    Consume((if cond { break } else { q }, Inner(if cond { break } else { 3 })));
                }
            }
        }
    "});

    let first_candidate = package
        .find("if cond {\n            break\n        } else {\n            [q]")
        .expect("the earlier non-defaultable candidate should be array-backed first");
    let later_candidate = package
        .rfind("if cond")
        .expect("the later nested candidate should remain in the consumer");
    assert!(
        first_candidate < later_candidate,
        "the earlier direct candidate must win before a later nested candidate: {package}"
    );
}

#[test]
fn later_candidate_does_not_re_pin_already_lifted_operands() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Effect(value : Int) : Int { value }
            operation Consume(first : Int, second : Int, third : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    Consume(Effect(1), { if cond { break; } 2 }, { if cond { continue; } 3 });
                }
            }
        }
    "});

    assert!(
        !has_chained_operand_temp_copy(&package),
        "a slot pinned by an earlier fixpoint iteration must not be pinned again\n{package}"
    );
    assert_eq!(
        operand_temp_bind_count(&package),
        3,
        "the effectful prefix and each of the two candidates should be bound once; the \
         value-stable callee is not pinned at all\n{package}"
    );

    expect![[r#"
        operation Effect(value : Int) : Int {
            value
        }
        operation Consume(first : Int, second : Int, third : Int) : Unit {}
        operation Main() : Unit {
            mutable cond = true;
            while cond {
                let _operand_tmp_62 = Effect(1);
                let _operand_tmp_66 = {
                    if cond {
                        break;
                    }
                    2
                };
                let _operand_tmp_70 = {
                    if cond {
                        continue;
                    }
                    3
                };
                Consume(_operand_tmp_62, _operand_tmp_66, _operand_tmp_70);
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn later_candidate_does_not_re_pin_an_array_backed_read() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Consume(first : Qubit, second : Int) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                mutable cond = true;
                while cond {
                    Consume(if cond { break } else { q }, if cond { continue } else { 3 });
                }
            }
        }
    "});

    assert!(
        !has_chained_operand_temp_copy(&package),
        "the array-backed read must not be pinned again by the later candidate\n{package}"
    );
    assert_eq!(
        operand_temp_bind_count(&package),
        2,
        "each of the two candidates should be bound once; the value-stable callee is not \
         pinned at all\n{package}"
    );

    expect![[r#"
        operation Consume(first : Qubit, second : Int) : Unit {}
        operation Main() : Unit {
            use q = Qubit();
            mutable cond = true;
            while cond {
                let _operand_tmp_51 = if cond {
                    break
                } else {
                    [q]
                };
                let _operand_tmp_58 = if cond {
                    continue
                } else {
                    3
                };
                Consume(_operand_tmp_51[0], _operand_tmp_58);
            }
        }
    "#]]
    .assert_eq(&package);
}

/// Builds `{ break; 1 }` as a fresh `Int`-typed block expression, the smallest
/// shape the direct-candidate path of `lift_operands` accepts.
fn break_bearing_block_expr(assigner: &mut Assigner) -> Expr {
    let span = Span::default();
    let break_stmt = Stmt {
        id: assigner.next_node(),
        span,
        kind: StmtKind::Semi(Expr {
            id: assigner.next_node(),
            span,
            ty: Ty::UNIT,
            kind: ExprKind::Break,
        }),
    };
    let value_stmt = Stmt {
        id: assigner.next_node(),
        span,
        kind: StmtKind::Expr(Expr {
            id: assigner.next_node(),
            span,
            ty: Ty::Prim(Prim::Int),
            kind: ExprKind::Lit(Lit::Int(1)),
        }),
    };
    Expr {
        id: assigner.next_node(),
        span,
        ty: Ty::Prim(Prim::Int),
        kind: ExprKind::Block(Block {
            id: assigner.next_node(),
            span,
            ty: Ty::Prim(Prim::Int),
            stmts: vec![break_stmt, value_stmt],
        }),
    }
}

#[test]
fn direct_candidate_is_bound_even_when_registered_as_a_generated_read() {
    let mut assigner = Assigner::new();
    let pin = gen_ident(
        &mut assigner,
        "operand_tmp",
        Ty::Prim(Prim::Int),
        Span::default(),
    );
    let mut earlier = pin.gen_local_ref(&mut assigner);
    let mut candidate = break_bearing_block_expr(&mut assigner);
    let earlier_id = earlier.id;
    let candidate_id = candidate.id;

    let mut pass = LoopNormalize::new(&mut assigner);
    // Register the candidate alongside the earlier read, so the only thing that
    // can suppress the candidate's binding is an over-broad earlier-operand
    // skip. Widening the skip to `operand_idx <= idx` leaves the candidate slot
    // unbound, which both starves the desugar of the guarded binding and stalls
    // the per-statement fixpoint on an unchanged operand.
    pass.generated_operand_reads.insert(earlier_id);
    pass.generated_operand_reads.insert(candidate_id);

    let lifted = pass
        .lift_operands(vec![&mut earlier, &mut candidate])
        .expect("the direct candidate should still be lifted");

    assert_eq!(
        lifted.len(),
        1,
        "only the candidate should be bound, because the earlier read is already pinned"
    );
    assert_eq!(
        earlier.id, earlier_id,
        "the earlier generated read must be left in place"
    );
    assert!(
        matches!(candidate.kind, ExprKind::Var(Res::Local(_), _)),
        "the candidate slot must be rewritten to read its own temp"
    );
}

#[test]
fn earlier_local_read_is_pinned_while_item_and_literal_are_skipped() {
    // The callee is a global item reference and the first argument is a
    // literal. Neither can take a different value at the pin point than at its
    // original evaluation point, so both stay inline. The second argument reads
    // a mutable local that the candidate operand then assigns, so it must still
    // be pinned: a plain local read is side-effect-free, and widening the skip
    // from value stability to plain purity would drop that pin and leave
    // `Consume` reading 99 where the source program reads 1.
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Consume(first : Int, second : Int, third : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                mutable acc = 1;
                while cond {
                    Consume(7, acc, { acc = 99; if cond { break; } 3 });
                }
            }
        }
    "});

    let generated: Vec<&str> = package
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("let _operand_tmp"))
        .collect();
    assert_eq!(
        generated.len(),
        2,
        "only the local read and the candidate should be bound\n{package}"
    );
    assert!(
        generated[0].ends_with("= acc;"),
        "the earlier local read must still be pinned ahead of the assigning candidate\n{package}"
    );
    assert!(
        generated[1].ends_with("= {"),
        "the candidate block must be the second generated binding\n{package}"
    );

    expect![[r#"
        operation Consume(first : Int, second : Int, third : Int) : Unit {}
        operation Main() : Unit {
            mutable cond = true;
            mutable acc = 1;
            while cond {
                let _operand_tmp_50 = acc;
                let _operand_tmp_54 = {
                    acc = 99;
                    if cond {
                        break;
                    }
                    3
                };
                Consume(7, _operand_tmp_50, _operand_tmp_54);
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn hoist_break_in_tuple_operand() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : (Int, Int)) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo((1, if cond { break } else { 3 }));
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : (Int, Int)) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_35 = if cond {
                        break
                    } else {
                        3
                    };
                    Foo(1, _operand_tmp_35);
                }
            }
        "#]],
    );
}

#[test]
fn idempotent_after_hoisting_break() {
    let package = check_idempotent(indoc! {"
        namespace Test {
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    Foo(if cond { break } else { 3 });
                }
            }
        }
    "});

    expect![[r#"
        operation Foo(x : Int) : Unit {}
        operation Main() : Unit {
            mutable cond = true;
            while cond {
                let _operand_tmp_33 = if cond {
                    break
                } else {
                    3
                };
                Foo(_operand_tmp_33);
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn idempotent_after_hoisting_nested_operands() {
    let package = check_idempotent(indoc! {"
        namespace Test {
            function Bar(x : Int) : Int { x }
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    Foo(Bar(if cond { break } else { 3 }));
                }
            }
        }
    "});

    expect![[r#"
        function Bar(x : Int) : Int {
            x
        }
        operation Foo(x : Int) : Unit {}
        operation Main() : Unit {
            mutable cond = true;
            while cond {
                let _operand_tmp_43 = if cond {
                    break
                } else {
                    3
                };
                Foo(Bar(_operand_tmp_43));
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn preserves_type_of_surface_if() {
    // The `if` is the direct initializer of the `let`, a statement position rather
    // than an operand, so it is left in place and `x` keeps its `Int` type.
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        let x = if cond { break } else { 3 };
                        let y = x + 1;
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let x = if cond {
                        break
                    } else {
                        3
                    };
                    let y = x + 1;
                }
            }
        "#]],
    );
}

#[test]
fn no_op_for_statement_position_break() {
    // A break that is already a statement, inside a statement-position `if`, is
    // not in operand position, so nothing is hoisted.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        if cond { break }
                        Foo(3);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    if cond {
                        break
                    }
                    Foo(3);
                }
            }
        "#]],
    );
}

#[test]
fn no_op_for_operand_block_without_control_flow() {
    // An operand-position block that contains no escaping control flow is left
    // untouched.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo({ let z = 1; z });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    Foo({
                        let z = 1;
                        z
                    });
                }
            }
        "#]],
    );
}

#[test]
fn no_op_for_break_bound_to_nested_loop() {
    // The break binds to the inner `while`, so it does not escape the operand
    // block and no hoist is performed.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { while cond { break }; 3 } else { 4 });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    Foo(if cond {
                        while cond {
                            break
                        };
                        3
                    } else {
                        4
                    });
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_tuple_qubit_initializer_preserves_evaluation_order() {
    check(
        indoc! {"
            namespace Test {
                operation Length(value : Int) : Int { value }
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        use (first, second) = (
                            Qubit[Length(1)],
                            Qubit[if cond { break } else { Length(2) }]
                        );
                    }
                }
            }
        "},
        &expect![[r#"
            operation Length(value : Int) : Int {
                value
            }
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    use _qubit_46 = Qubit[Length(1)];
                    let _operand_tmp_55 = if cond {
                        break
                    } else {
                        Length(2)
                    };
                    use _qubit_49 = Qubit[_operand_tmp_55];
                    let (first, second) = (_qubit_46, _qubit_49);
                }
            }
        "#]],
    );
}

#[test]
fn stage_break_after_single_qubit_initializer() {
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        use (first, second) = (
                            Qubit(),
                            Qubit[if cond { break } else { 2 }]
                        );
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    use _qubit_33 = Qubit();
                    let _operand_tmp_42 = if cond {
                        break
                    } else {
                        2
                    };
                    use _qubit_36 = Qubit[_operand_tmp_42];
                    let (first, second) = (_qubit_33, _qubit_36);
                }
            }
        "#]],
    );
}

#[test]
fn stage_continue_after_array_borrow_initializer() {
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        borrow (first, second) = (
                            Qubit[1],
                            Qubit[if cond { continue } else { 2 }]
                        );
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    borrow _qubit_34 = Qubit[1];
                    let _operand_tmp_43 = if cond {
                        continue
                    } else {
                        2
                    };
                    borrow _qubit_37 = Qubit[_operand_tmp_43];
                    let (first, second) = (_qubit_34, _qubit_37);
                }
            }
        "#]],
    );
}

#[test]
fn stage_break_in_nested_mixed_qubit_initializer() {
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        use (first, (second, third)) = (
                            Qubit[1],
                            (Qubit(), Qubit[if cond { break } else { 2 }])
                        );
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    use _qubit_39 = Qubit[1];
                    use _qubit_42 = Qubit();
                    let _operand_tmp_53 = if cond {
                        break
                    } else {
                        2
                    };
                    use _qubit_45 = Qubit[_operand_tmp_53];
                    let (first, (second, third)) = (_qubit_39, (_qubit_42, _qubit_45));
                }
            }
        "#]],
    );
}

#[test]
fn stage_break_in_scoped_tuple_qubit_initializer() {
    check(
        indoc! {"
            namespace Test {
                operation Main() : Int {
                    mutable cond = true;
                    while cond {
                        use (first, second) = (
                            Qubit(),
                            Qubit[if cond { break } else { 2 }]
                        ) {
                            return 1;
                        }
                    }
                    0
                }
            }
        "},
        &expect![[r#"
            operation Main() : Int {
                mutable cond = true;
                while cond {
                    {
                        use _qubit_39 = Qubit();
                        let _operand_tmp_50 = if cond {
                            break
                        } else {
                            2
                        };
                        use _qubit_42 = Qubit[_operand_tmp_50];
                        let (first, second) = (_qubit_39, _qubit_42);
                        return 1;
                    }
                }
                0
            }
        "#]],
    );
}

#[test]
fn first_leaf_abrupt_qubit_initializer_avoids_unneeded_staging() {
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        use (first, second) = (
                            Qubit[if cond { break } else { 1 }],
                            Qubit()
                        );
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_33 = if cond {
                        break
                    } else {
                        1
                    };
                    use (first, second) = (Qubit[_operand_tmp_33], Qubit());
                }
            }
        "#]],
    );
}

#[test]
fn staged_qubit_initializer_is_idempotent() {
    let package = check_idempotent(indoc! {"
        namespace Test {
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    use (first, second) = (
                        Qubit(),
                        Qubit[if cond { break } else { 2 }]
                    );
                }
            }
        }
    "});

    assert_eq!(
        package.matches("use _qubit").count(),
        2,
        "staging should emit exactly one owner per initializer leaf: {package}"
    );
}

#[test]
fn hoist_break_in_qubit_operand_block_array_backed() {
    // Lifting the operand block introduces a temporary of type `Qubit`, which
    // has no classical default for the break path. Rather than reject it, the
    // pass array-backs the temp as `Qubit[]`: the block's trailing value `q` is
    // wrapped as `[q]`, and the operand slot reads it back through
    // `.operand_tmp_<id>[0]`. The later desugar seeds the break path with the
    // universal `[]` default and guards the read, so `[]` is never indexed.
    check(
        indoc! {"
            namespace Test {
                operation Foo(q : Qubit) : Unit {}
                operation Main() : Unit {
                    use q = Qubit();
                    mutable cond = true;
                    while cond {
                        Foo({ if cond { break }; q });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(q : Qubit) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                mutable cond = true;
                while cond {
                    let _operand_tmp_38 = {
                        if cond {
                            break
                        };
                        [q]
                    };
                    Foo(_operand_tmp_38[0]);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_arrow_operand_block_array_backed() {
    // An arrow-typed operand value-block has no classical default, so it is
    // array-backed as `(Qubit => Unit)[]`, which lets the desugar accept it
    // uniformly with the other array-backed operand types.
    check(
        indoc! {"
            namespace Test {
                operation Bar(q : Qubit) : Unit {}
                operation Foo(op : Qubit => Unit) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { break } else { Bar });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Bar(q : Qubit) : Unit {}
            operation Foo(op : (Qubit => Unit)) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_39 = if cond {
                        break
                    } else {
                        [Bar]
                    };
                    Foo(_operand_tmp_39[0]);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_udt_operand_block_array_backed() {
    // A user-defined-type operand value-block is array-backed as `Pair[]`,
    // uniformly with `Qubit` and arrow types and without constructing a `Pair`
    // default, so the normalize pass and the desugar handle it consistently.
    check(
        indoc! {"
            namespace Test {
                newtype Pair = (First : Int, Second : Int);
                operation Foo(p : Pair) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { break } else { Pair(1, 2) });
                    }
                }
            }
        "},
        &expect![[r#"
            // newtype Pair
            operation Foo(p : Pair) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_38 = if cond {
                        break
                    } else {
                        [Pair(1, 2)]
                    };
                    Foo(_operand_tmp_38[0]);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_tuple_with_qubit_operand_array_backed() {
    // A tuple containing a `Qubit` is non-defaultable but representable, so the
    // whole operand is array-backed as `(Int, Qubit)[]`; the trailing tuple
    // value `(1, q)` is wrapped as `[(1, q)]` without decomposing the tuple.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : (Int, Qubit)) : Unit {}
                operation Main() : Unit {
                    use q = Qubit();
                    mutable cond = true;
                    while cond {
                        Foo(if cond { break } else { (1, q) });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : (Int, Qubit)) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                mutable cond = true;
                while cond {
                    let _operand_tmp_39 = if cond {
                        break
                    } else {
                        [(1, q)]
                    };
                    Foo(_operand_tmp_39[0]::Item < 0 >, _operand_tmp_39[0]::Item < 1 >);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_controlled_call_preserves_control_tuple() {
    check(
        indoc! {"
            namespace Test {
                operation Foo(q : Qubit) : Unit is Ctl {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Controlled Foo(break);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(q : Qubit) : Unit is Ctl {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_25 = Controlled Foo;
                    let _operand_tmp_29 = break;
                    _operand_tmp_25(_operand_tmp_29[0]::Item < 0 >, _operand_tmp_29[0]::Item < 1 >);
                }
            }
        "#]],
    );
}

#[test]
fn idempotent_after_array_backing_qubit_operand() {
    let package = check_idempotent(indoc! {"
        namespace Test {
            operation Foo(q : Qubit) : Unit {}
            operation Main() : Unit {
                use q = Qubit();
                mutable cond = true;
                while cond {
                    Foo({ if cond { break }; q });
                }
            }
        }
    "});

    expect![[r#"
        operation Foo(q : Qubit) : Unit {}
        operation Main() : Unit {
            use q = Qubit();
            mutable cond = true;
            while cond {
                let _operand_tmp_38 = {
                    if cond {
                        break
                    };
                    [q]
                };
                Foo(_operand_tmp_38[0]);
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn reject_break_in_unrepresentable_operand_block() {
    // A type-parameter-typed operand value-block is conservatively excluded from
    // array-backing. This matches the `return_unify` transform, which treats
    // unresolved leaves such as type parameters as the sole rejecting case, so
    // the pass records its defensive rejection. Such an operand cannot occur for
    // a well-typed program post-typecheck once callables are monomorphized.
    let package = check_errors(
        indoc! {"
            namespace Test {
                operation Foo<'T>(x : 'T, g : 'T => Unit) : Unit {
                    mutable cond = true;
                    while cond {
                        g(if cond { break } else { x });
                    }
                }
            }
        "},
        &expect![[r#"
            [
                UnsupportedType(
                    "Param<\"'T\": 0>",
                    Span {
                        lo: 136,
                        hi: 164,
                    },
                ),
            ]
        "#]],
    );

    expect![[r#"
        operation Foo(x : 'T, g : ('T => Unit)) : Unit {
            mutable cond = true;
            while cond {
                let _operand_tmp_31 = g;
                let _operand_tmp_35 = if cond {
                    break
                } else {
                    x
                };
                _operand_tmp_31(_operand_tmp_35);
            }
        }
    "#]]
    .assert_eq(&package);
}

#[test]
fn hoist_bare_break_in_call_argument() {
    // A bare `break` sitting directly in a call-argument slot is itself the
    // escaping control flow, so it is lifted to its own spine temp; the later
    // desugar guards the call behind the break flag.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(break);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_24 = break;
                    Foo(_operand_tmp_24);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_bare_continue_in_call_argument() {
    // A bare `continue` operand is lifted identically to a bare `break`; the
    // two are handled uniformly by the operand lift.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(continue);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_24 = continue;
                    Foo(_operand_tmp_24);
                }
            }
        "#]],
    );
}

#[test]
fn scalar_assignop_pins_old_value_before_break_bearing_rhs() {
    // HIR analogue of the FIR `nonfiring_return_in_scalar_assignop_rhs_uses_pre_rhs_value`.
    // Q# reads a compound assignment's place before its RHS runs, so the RHS's
    // own `x = 20` must not reach the `+`. `lift_scalar_assign_op` rewrites the
    // statement into an old-value pin plus a plain assignment; the snapshot pins
    // that the pin is emitted *ahead* of the break-bearing candidate, which is
    // what the positional assertions below check structurally.
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Main() : Unit {
                mutable x = 10;
                mutable cond = true;
                while cond {
                    x += {
                        x = 20;
                        if cond { break; }
                        5
                    };
                }
            }
        }
    "});

    expect![[r#"
        operation Main() : Unit {
            mutable x = 10;
            mutable cond = true;
            while cond {
                let _operand_tmp_41 = x;
                let _operand_tmp_36 = {
                    x = 20;
                    if cond {
                        break;
                    }
                    5
                };
                x = _operand_tmp_41 + _operand_tmp_36;
            }
        }
    "#]]
    .assert_eq(&package);

    let old_value = package
        .find("let _operand_tmp")
        .expect("the scalar old value should be pinned before the RHS");
    let rhs_mutation = package
        .find("x = 20")
        .expect("the RHS mutation should remain represented");
    assert!(
        old_value < rhs_mutation && package.contains("x = _operand_tmp"),
        "scalar compound assignment should become an old-value pin plus plain assignment: {package}"
    );
}

#[test]
fn scalar_assignop_pins_old_value_before_continue_bearing_rhs() {
    // Same shape as the break case, with `continue` as the abrupt operand. The
    // desugar guards a `continue` differently from a `break`, but the pin
    // ordering this pass establishes must be identical: the old value is read
    // before the RHS regardless of which abrupt form the candidate carries.
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Main() : Unit {
                mutable x = 10;
                mutable iterations = 0;
                while iterations < 1 {
                    iterations += 1;
                    x += {
                        x = 20;
                        if iterations > 0 { continue; }
                        5
                    };
                }
            }
        }
    "});

    expect![[r#"
        operation Main() : Unit {
            mutable x = 10;
            mutable iterations = 0;
            while iterations < 1 {
                iterations += 1;
                let _operand_tmp_49 = x;
                let _operand_tmp_44 = {
                    x = 20;
                    if iterations > 0 {
                        continue;
                    }
                    5
                };
                x = _operand_tmp_49 + _operand_tmp_44;
            }
        }
    "#]]
    .assert_eq(&package);

    let old_value = package
        .find("let _operand_tmp")
        .expect("the scalar old value should be pinned before the RHS");
    let rhs_mutation = package
        .find("x = 20")
        .expect("the RHS mutation should remain represented");
    assert!(
        old_value < rhs_mutation && package.contains("x = _operand_tmp"),
        "scalar compound assignment should become an old-value pin plus plain assignment: {package}"
    );
}

#[test]
fn array_assignop_does_not_pin_old_value_before_break_bearing_rhs() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Main() : Unit {
                mutable xs = [1];
                mutable cond = true;
                while cond {
                    xs += {
                        xs = [2];
                        if cond { break; }
                        [3]
                    };
                }
            }
        }
    "});

    assert_eq!(
        operand_temp_bind_count(&package),
        1,
        "array append should lift only its RHS and retain post-RHS target semantics: {package}"
    );
}

#[test]
fn indexed_compound_assignment_preserves_pre_rhs_element_read() {
    let package = normalize_to_string(indoc! {"
        namespace Test {
            operation Main() : Unit {
                mutable xs = [10];
                mutable cond = true;
                while cond {
                    xs w/= 0 <- xs[0] + {
                        xs w/= 0 <- 20;
                        if cond { break; }
                        5
                    };
                }
            }
        }
    "});

    let element_read = package
        .find("xs[0]")
        .expect("the indexed compound assignment should retain its old-element read");
    let rhs_mutation = package
        .find("xs w/= 0 <- 20")
        .expect("the RHS mutation should remain represented");
    assert!(
        element_read < rhs_mutation,
        "the indexed old-element read must remain before RHS effects: {package}"
    );
}

#[test]
fn hoist_bare_break_in_assign_value() {
    // The value operand of an assignment is lifted, so a bare `break` there is
    // exposed at statement position before the assignment.
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable x = 0;
                    mutable cond = true;
                    while cond {
                        x = break;
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable x = 0;
                mutable cond = true;
                while cond {
                    let _operand_tmp_22 = break;
                    x = _operand_tmp_22;
                }
            }
        "#]],
    );
}

#[test]
fn hoist_bare_break_in_index_operand() {
    // The index operand of an array access is lifted, so a bare `break` used as
    // an index is exposed at statement position and the access is later guarded.
    // The access is consumed by a call so its divergent result type is fixed.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Unit {}
                operation Main() : Unit {
                    let arr = [1, 2, 3];
                    mutable cond = true;
                    while cond {
                        Foo(arr[break]);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Unit {}
            operation Main() : Unit {
                let arr = [1, 2, 3];
                mutable cond = true;
                while cond {
                    let _operand_tmp_33 = arr;
                    let _operand_tmp_37 = break;
                    Foo(_operand_tmp_33[_operand_tmp_37]);
                }
            }
        "#]],
    );
}

#[test]
fn hoist_bare_break_in_if_condition() {
    // An `if` condition is an unconditional operand site, so a bare `break` in
    // the condition is lifted to a spine temp ahead of the `if`.
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        let y = if break { 1 } else { 2 };
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_27 = break;
                    let y = if _operand_tmp_27 {
                        1
                    } else {
                        2
                    };
                }
            }
        "#]],
    );
}

#[test]
fn hoist_bare_break_in_short_circuit_lhs() {
    // The left operand of a short-circuit `or` evaluates unconditionally, so a
    // bare `break` there is lifted to a spine temp.
    check(
        indoc! {"
            namespace Test {
                operation Main() : Unit {
                    mutable y = true;
                    mutable cond = true;
                    while cond {
                        let z = break or y;
                    }
                }
            }
        "},
        &expect![[r#"
            operation Main() : Unit {
                mutable y = true;
                mutable cond = true;
                while cond {
                    let _operand_tmp_24 = break;
                    let z = _operand_tmp_24 or y;
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_short_circuit_or_rhs() {
    // The right operand of `or` is conditional, so when it buries escaping
    // control flow the `BinOp` is reshaped into `if y { true } else { <rhs> }`
    // and the buried `break` reaches a statement boundary inside the else block.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Bool { true }
                operation Main() : Unit {
                    mutable y = true;
                    mutable cond = true;
                    while cond {
                        let z = y or Foo(break);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Bool {
                true
            }
            operation Main() : Unit {
                mutable y = true;
                mutable cond = true;
                while cond {
                    let z = if y {
                        true
                    } else {
                        let _operand_tmp_41 = break;
                        Foo(_operand_tmp_41)
                    };
                }
            }
        "#]],
    );
}

#[test]
fn hoist_continue_in_short_circuit_and_rhs() {
    // The right operand of `and` is conditional, so when it buries escaping
    // control flow the `BinOp` is reshaped into `if y { <rhs> } else { false }`
    // and the buried `continue` reaches a statement boundary inside the then
    // block. Mirrors the `or` reshape with the branches swapped.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Bool { true }
                operation Main() : Unit {
                    mutable y = true;
                    mutable cond = true;
                    while cond {
                        let z = y and Foo(continue);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Bool {
                true
            }
            operation Main() : Unit {
                mutable y = true;
                mutable cond = true;
                while cond {
                    let z = if y {
                        let _operand_tmp_41 = continue;
                        Foo(_operand_tmp_41)
                    } else {
                        false
                    };
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_compound_short_circuit_and_assign_rhs() {
    // A compound `and=` whose right operand buries a `break` in a bare operand
    // position (`Foo(break)`, not a statement-carrying wrapper) is reshaped into
    // `if b { b = Foo(break) }`, so the buried `break` reaches a statement
    // boundary inside the guarded assignment block instead of running `Foo` with
    // a default on the divergence path. The guard preserves the short-circuit:
    // the assignment runs only when `b` is already true.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Bool { true }
                operation Main() : Unit {
                    mutable b = true;
                    mutable cond = true;
                    while cond {
                        b and= Foo(break);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Bool {
                true
            }
            operation Main() : Unit {
                mutable b = true;
                mutable cond = true;
                while cond {
                    if b {
                        let _operand_tmp_37 = break;
                        b = Foo(_operand_tmp_37);
                    };
                }
            }
        "#]],
    );
}

#[test]
fn hoist_continue_in_compound_short_circuit_or_assign_rhs() {
    // A compound `or=` whose right operand buries a `continue` in a bare operand
    // position is reshaped into `if not b { b = Foo(continue) }`: the `or`
    // short-circuits when `b` is already true, so the assignment, and the buried
    // `continue`, runs only when `b` is false.
    check(
        indoc! {"
            namespace Test {
                operation Foo(x : Int) : Bool { true }
                operation Main() : Unit {
                    mutable b = false;
                    mutable cond = true;
                    while cond {
                        b or= Foo(continue);
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(x : Int) : Bool {
                true
            }
            operation Main() : Unit {
                mutable b = false;
                mutable cond = true;
                while cond {
                    if not b {
                        let _operand_tmp_38 = continue;
                        b = Foo(_operand_tmp_38);
                    };
                }
            }
        "#]],
    );
}

#[test]
fn array_back_non_defaultable_let_rhs_break() {
    // A `let` binding whose value buries a `break` and whose type has no
    // classical default, such as `Pair`, is array-backed: the initializer becomes a
    // `Pair[]` temp whose `then` branch is a bare break and whose `else` branch is
    // the singleton `[Pair(1, 2)]`, and the binding reads it back through
    // `.operand_tmp_<id>[0]`, so no `Pair` default is needed on the divergence path.
    check(
        indoc! {"
            namespace Test {
                newtype Pair = (First : Int, Second : Int);
                operation Foo(p : Pair) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        let x = if cond { break } else { Pair(1, 2) };
                        Foo(x);
                    }
                }
            }
        "},
        &expect![[r#"
            // newtype Pair
            operation Foo(p : Pair) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_42 = if cond {
                        break
                    } else {
                        [Pair(1, 2)]
                    };
                    let x = _operand_tmp_42[0];
                    Foo(x);
                }
            }
        "#]],
    );
}

#[test]
fn array_back_discarded_value_block_break() {
    // A non-Unit block used as a statement with its result discarded, written
    // `{ … };`, whose value buries a `break` and has no classical default such as
    // `Pair`, is array-backed in place: the block's value type becomes `Pair[]`,
    // with its `then` branch a bare break and its `else` branch the singleton
    // `[Pair(1, 2)]`, so the buried break desugars with the universal `[]` default
    // instead of a `Pair` default. The value stays discarded; no temp binding is
    // introduced.
    check(
        indoc! {"
            namespace Test {
                newtype Pair = (First : Int, Second : Int);
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        { if cond { break } else { Pair(1, 2) } };
                    }
                }
            }
        "},
        &expect![[r#"
            // newtype Pair
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    {
                        if cond {
                            break
                        } else {
                            [Pair(1, 2)]
                        }
                    };
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_return_operand_block() {
    // A `return` operand may bury an escaping `break`; the operand is lifted to
    // a temp while the `return` node stays in place, so the buried `break` is
    // exposed without hoisting the `return` itself.
    check(
        indoc! {"
            namespace Test {
                operation Main() : Int {
                    mutable cond = true;
                    while cond {
                        return { if cond { break }; 5 };
                    }
                    0
                }
            }
        "},
        &expect![[r#"
            operation Main() : Int {
                mutable cond = true;
                while cond {
                    let _operand_tmp_29 = {
                        if cond {
                            break
                        };
                        5
                    };
                    return _operand_tmp_29;
                }
                0
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_for_iterable_nested_in_outer_loop() {
    // A `for` iterable is evaluated once in the enclosing loop scope. A `break`
    // buried in a compound iterable binds to the outer `while`, so the iterable
    // is lifted to a spine temp ahead of the `for` and the buried `break` is
    // exposed without hoisting the `for` itself.
    check(
        indoc! {"
            namespace Test {
                function F(a : Int[], b : Int) : Int[] { a }
                operation G() : Int { 0 }
                operation Main() : Unit {
                    let arr = [1, 2, 3];
                    mutable cond = true;
                    while cond {
                        for j in F({ if cond { break }; arr }, G()) {
                            let k = j;
                        }
                    }
                }
            }
        "},
        &expect![[r#"
            function F(a : Int[], b : Int) : Int[] {
                a
            }
            operation G() : Int {
                0
            }
            operation Main() : Unit {
                let arr = [1, 2, 3];
                mutable cond = true;
                while cond {
                    let _operand_tmp_65 = {
                        if cond {
                            break
                        };
                        arr
                    };
                    for j in F(_operand_tmp_65, G()) {
                        let k = j;
                    }
                }
            }
        "#]],
    );
}

#[test]
fn hoist_break_in_core_udt_operand_block_array_backed() {
    // A user-defined type defined in another package (`Complex`, from the core
    // library) is array-backed just like a local newtype: array-backing needs
    // only the universal `[]` default of `Complex[]`, never a default of the
    // user-defined type itself, so the operand is representable regardless of
    // which package defines it. This is the cross-package companion to
    // `hoist_break_in_udt_operand_block_array_backed`.
    check(
        indoc! {"
            namespace Test {
                operation Foo(c : Complex) : Unit {}
                operation Main() : Unit {
                    mutable cond = true;
                    while cond {
                        Foo(if cond { break } else { Complex(1.0, 2.0) });
                    }
                }
            }
        "},
        &expect![[r#"
            operation Foo(c : Complex) : Unit {}
            operation Main() : Unit {
                mutable cond = true;
                while cond {
                    let _operand_tmp_37 = if cond {
                        break
                    } else {
                        [Complex(1., 2.)]
                    };
                    Foo(_operand_tmp_37[0]);
                }
            }
        "#]],
    );
}
