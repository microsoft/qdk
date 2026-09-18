// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::formatdoc;
use proptest::prelude::*;

#[test]
fn deep_controlled_payload_preserves_active_inactive_controls_and_snapshot() {
    use crate::PipelineStage;
    use crate::test_utils::{
        compile_and_run_pipeline_to, compile_to_fir, try_eval_fir_entry_with_trace,
    };
    use qsc_fir::fir::{ExprKind, ItemKind, PackageLookup, PatKind};
    use qsc_fir::ty::Ty;

    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Inner = (Bias : Int, Action : (Qubit => Unit is Adj + Ctl));
            operation Toggle(count : Int, target : Qubit) : Unit is Adj + Ctl {
                for step in 1..count { X(target); }
            }
            operation Evaluate(payload : (Int, (Int, Inner)), target : Qubit) : Unit is Adj + Ctl {
                let (scale, (offset, inner)) = payload;
                let action = inner::Action;
                for step in 1..scale + offset + inner::Bias { action(target); }
            }
            function Make(payload : (Int, (Int, Inner))) : (Qubit => Unit is Adj + Ctl) {
                target => Evaluate(payload, target)
            }
            @EntryPoint()
            operation Main() : (Result, Result, Result, Result) {
                use inactive = Qubit();
                use active = Qubit();
                use first = Qubit();
                use second = Qubit();
                mutable count = 3;
                let action = Make((2, (3, Inner(6, Toggle(count, _)))));
                set count = 2;
                X(active);
                Controlled action([inactive], first);
                Controlled action([active], second);
                (MResetZ(inactive), MResetZ(active), MResetZ(first), MResetZ(second))
            }
        }
    "#};
    let (original, package_id) = compile_to_fir(source);
    let package = original.get(package_id);
    assert!(
        package.exprs.iter().any(|(_, expr)| {
            let ExprKind::Closure(captures, target) = &expr.kind else {
                return false;
            };
            let ItemKind::Callable(decl) = &package.items.get(*target).expect("target exists").kind
            else {
                return false;
            };
            let PatKind::Tuple(patterns) = &package.get_pat(decl.input).kind else {
                return false;
            };
            captures.len() == 1
                && matches!(&package.get_pat(patterns[0]).ty, Ty::Tuple(items)
                if matches!(items.as_slice(), [Ty::Prim(_), Ty::Tuple(nested)]
                    if matches!(nested.as_slice(), [Ty::Prim(_), Ty::Udt(_)])))
        }),
        "Q# must produce the deep aggregate capture"
    );
    let (expected, expected_trace) = try_eval_fir_entry_with_trace(&original, package_id);
    assert_eq!(
        expected
            .as_ref()
            .expect("original must succeed")
            .to_string(),
        "(Zero, One, Zero, One)"
    );
    assert!(!expected_trace.is_empty());
    let (normalized, normalized_id) = compile_and_run_pipeline_to(source, PipelineStage::Defunc);
    assert!(
        normalized.get(normalized_id).pats.iter().any(|(_, pat)| {
            matches!(&pat.kind, PatKind::Bind(ident) if ident.name.as_ref() == "capture_leaf")
        }),
        "the aggregate environment must normalize"
    );
    let (full, full_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    let (actual, actual_trace) = try_eval_fir_entry_with_trace(&full, full_id);
    assert_eq!(actual, expected);
    assert_eq!(actual_trace, expected_trace);
}

#[test]
fn deep_capture_functors_preserve_adjoint_phase_and_two_control_layers() {
    use crate::PipelineStage;
    use crate::test_utils::{
        compile_and_run_pipeline_to, compile_to_fir, try_eval_fir_entry_with_trace,
    };
    use crate::walk_utils::{for_each_expr, for_each_expr_in_callable_impl};
    use qsc_fir::fir::{ExprKind, Functor, ItemKind, PatKind, Res, UnOp};

    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Inner = (Bias : Int, Action : (Qubit => Unit is Adj + Ctl));
            operation Phase(count : Int, target : Qubit) : Unit is Adj + Ctl {
                for step in 1..count { S(target); }
            }
            operation Evaluate(payload : (Int, (Int, Inner)), target : Qubit) : Unit is Adj + Ctl {
                let (scale, (offset, inner)) = payload;
                let action = inner::Action;
                for step in 1..scale + offset + inner::Bias { action(target); }
            }
            function Make(payload : (Int, (Int, Inner))) : (Qubit => Unit is Adj + Ctl) {
                target => Evaluate(payload, target)
            }
            @EntryPoint()
            operation Main() : (Result, Result, Result, Result, Result, Result) {
                use outer = Qubit();
                use inner = Qubit();
                use inactive = Qubit();
                use inverseTarget = Qubit();
                use activeTarget = Qubit();
                use inactiveTarget = Qubit();
                let action = Make((2, (3, Inner(6, Phase(3, _)))));
                X(outer);
                X(inner);
                H(inverseTarget);
                S(inverseTarget);
                Controlled Adjoint action([outer], inverseTarget);
                H(inverseTarget);
                H(activeTarget);
                S(activeTarget);
                Controlled Controlled action([outer], ([inner], activeTarget));
                H(activeTarget);
                H(inactiveTarget);
                Controlled Controlled action([outer], ([inactive], inactiveTarget));
                H(inactiveTarget);
                (MResetZ(inverseTarget), MResetZ(activeTarget), MResetZ(inactiveTarget),
                 MResetZ(outer), MResetZ(inner), MResetZ(inactive))
            }
        }
    "#};
    let (original, package_id) = compile_to_fir(source);
    let package = original.get(package_id);
    let mut functor_counts = (0, 0);
    for expr in package.exprs.values() {
        match expr.kind {
            ExprKind::UnOp(UnOp::Functor(Functor::Adj), _) => functor_counts.0 += 1,
            ExprKind::UnOp(UnOp::Functor(Functor::Ctl), _) => functor_counts.1 += 1,
            _ => {}
        }
    }
    assert!(functor_counts.0 > 0);
    assert!(functor_counts.1 >= 5);
    let (expected, expected_trace) = try_eval_fir_entry_with_trace(&original, package_id);
    assert_eq!(
        expected
            .as_ref()
            .expect("original must succeed")
            .to_string(),
        "(Zero, One, Zero, One, One, Zero)"
    );
    let (normalized, normalized_id) = compile_and_run_pipeline_to(source, PipelineStage::Defunc);
    assert!(normalized.get(normalized_id).pats.values().any(|pat| {
        matches!(&pat.kind, PatKind::Bind(ident) if ident.name.as_ref() == "capture_leaf")
    }));
    let package = normalized.get(normalized_id);
    let mut closure_targets = rustc_hash::FxHashSet::default();
    let mut direct_targets = rustc_hash::FxHashSet::default();
    let mut inspect = |_, expr: &qsc_fir::fir::Expr| match &expr.kind {
        ExprKind::Closure(_, target) => {
            closure_targets.insert(*target);
        }
        ExprKind::Var(Res::Item(item), _) if item.package == normalized_id => {
            direct_targets.insert(item.item);
        }
        _ => {}
    };
    for item in package.items.values() {
        if let ItemKind::Callable(decl) = &item.kind {
            for_each_expr_in_callable_impl(package, &decl.implementation, &mut inspect);
        }
    }
    if let Some(entry) = package.entry {
        for_each_expr(package, entry, &mut inspect);
    }
    assert!(
        !closure_targets.is_disjoint(&direct_targets),
        "a lifted target retains both closure and direct references"
    );
    let (full, full_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    let (actual, actual_trace) = try_eval_fir_entry_with_trace(&full, full_id);
    assert_eq!(actual, expected);
    assert_eq!(actual_trace, expected_trace);
}

#[test]
fn opaque_array_preserves_elements_beside_normalized_arrow() {
    use crate::PipelineStage;
    use crate::test_utils::{
        compile_and_run_pipeline_to, compile_to_fir, try_eval_fir_entry_with_trace,
    };
    use qsc_fir::fir::{ExprKind, PatKind};
    use qsc_fir::ty::Ty;

    for (mixed, binding, answer) in [
        (
            false,
            "let environment = actions; let saved = value -> Evaluate(environment, value);",
            510,
        ),
        (
            true,
            "let environment = (actions, Add17); let saved = value -> { let (opaque, action) = environment; Evaluate(opaque, value) + action(value) };",
            529,
        ),
    ] {
        let source = formatdoc! {r#"
            namespace Test {{
                function Add17(value : Int) : Int {{ value + 17 }}
                function Evaluate(actions : Int[], value : Int) : Int {{
                    (actions[0] + value) * 100 + actions[1] * value
                }}
                @EntryPoint()
                operation Main() : Int {{
                    let actions = [3, 5];
                    {binding}
                    saved(2)
                }}
            }}
        "#};
        let (original, package_id) = compile_to_fir(&source);
        assert!(original.get(package_id).exprs.values().any(|expr| {
            matches!(&expr.kind, ExprKind::Closure(captures, _) if captures.len() == 1)
        }));
        let expected = try_eval_fir_entry_with_trace(&original, package_id);
        assert_eq!(expected.0, Ok(qsc_eval::val::Value::Int(answer)));
        let (normalized, normalized_id) =
            compile_and_run_pipeline_to(&source, PipelineStage::Defunc);
        let leaves: Vec<_> = normalized.get(normalized_id).pats.values().filter(|pat| {
            matches!(&pat.kind, PatKind::Bind(ident) if ident.name.as_ref() == "capture_leaf")
        }).collect();
        assert_eq!(!leaves.is_empty(), mixed);
        if mixed {
            assert!(leaves.iter().any(|pat| matches!(&pat.ty, Ty::Array(element) if matches!(element.as_ref(), Ty::Prim(_)))));
            assert!(leaves.iter().any(|pat| matches!(&pat.ty, Ty::Arrow(_))));
        }
        let (full, full_id) = compile_and_run_pipeline_to(&source, PipelineStage::Full);
        assert_eq!(try_eval_fir_entry_with_trace(&full, full_id), expected);
    }
}

#[test]
fn nested_recaptures_preserve_compatible_and_opaque_snapshots() {
    use crate::PipelineStage;
    use crate::test_utils::{
        compile_and_run_pipeline_to, compile_to_fir, try_eval_fir_entry_with_trace,
    };
    use qsc_fir::fir::{ExprKind, ItemKind, PackageLookup, PatKind};

    for replace_seed in [false, true] {
        let replacement = if replace_seed { "set seed = 100;" } else { "" };
        for (eligible, environment, evaluate) in [
            (
                true,
                "(seed, Add3)",
                "let (bias, action) = environment; bias + action(delta + value)",
            ),
            (false, "[seed]", "environment[0] + Add3(delta + value)"),
        ] {
            let source = formatdoc! {r#"
            namespace Test {{
                function Add3(value : Int) : Int {{ value + 3 }}
                @EntryPoint()
                operation Main() : Int {{
                    mutable seed = 7;
                    let environment = {environment};
                    let factory = delta -> {{
                        let inner = value -> {{ {evaluate} }};
                        inner(2)
                    }};
                    {replacement}
                    factory(11)
                }}
            }}
        "#};
            let (original, package_id) = compile_to_fir(&source);
            assert_eq!(
                original
                    .get(package_id)
                    .exprs
                    .values()
                    .filter(|expr| {
                        matches!(&expr.kind, ExprKind::Closure(captures, _) if !captures.is_empty())
                    })
                    .count(),
                2
            );
            let expected = try_eval_fir_entry_with_trace(&original, package_id);
            assert_eq!(expected.0, Ok(qsc_eval::val::Value::Int(23)));
            let (normalized, normalized_id) =
                compile_and_run_pipeline_to(&source, PipelineStage::Defunc);
            let package = normalized.get(normalized_id);
            let normalized_targets = package.items.values().filter(|item| {
            let ItemKind::Callable(decl) = &item.kind else { return false; };
            let PatKind::Tuple(patterns) = &package.get_pat(decl.input).kind else { return false; };
            patterns.iter().any(|pattern| {
                matches!(&package.get_pat(*pattern).kind, PatKind::Bind(ident) if ident.name.as_ref() == "capture_leaf")
            })
        }).count();
            if eligible {
                assert!(
                    normalized_targets >= 2,
                    "both nested capture targets must normalize"
                );
            } else {
                assert_eq!(normalized_targets, 0, "opaque recaptures remain unchanged");
            }
            let (full, full_id) = compile_and_run_pipeline_to(&source, PipelineStage::Full);
            assert_eq!(try_eval_fir_entry_with_trace(&full, full_id), expected);
        }
    }
}

mod round8_effects {
    use qsc_eval::val::Value;

    use crate::test_utils::{
        check_semantic_equivalence, compile_to_fir, try_eval_fir_entry_with_trace,
    };

    #[test]
    fn root_array_elements_preserve_mutations_and_capture_snapshots() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Make(offset : Int) : Int -> Int { value -> offset + value }
                function Times5(value : Int) : Int { value * 5 }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    saved[0](2) * 10000 + saved[1](3) * 100 + saved[2](5)
                }
                @EntryPoint()
                operation Main() : Int {
                    mutable offset = 3;
                    mutable visits = 0;
                    let answer = Relay([
                        { set visits = visits * 10 + 1; Make(offset) },
                        { set offset = 17; set visits = visits * 10 + 2; Times5 },
                        { set visits = visits * 10 + 3; Make(offset) }
                    ]);
                    answer * 1000 + visits
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(Value::Int(51_522_123)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn unused_array_candidate_producer_still_fails() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Make(offset : Int) : Int -> Int {
                    if offset < 0 { fail "unused callable producer"; }
                    value -> offset + value
                }
                function Times5(value : Int) : Int { value * 5 }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    saved[0](2) * 100 + saved[1](3)
                }
                @EntryPoint()
                operation Main() : Int { Relay([Make(7), Times5, Make(-1)]) }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert!(result.is_err(), "the unused candidate producer must fail");
        check_semantic_equivalence(source);
    }

    #[test]
    fn repeated_array_size_preserves_initializer_snapshot_and_effects() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Make(offset : Int) : Int -> Int { value -> offset + value }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    saved[0](2) * 100 + saved[2](3)
                }
                @EntryPoint()
                operation Main() : Int {
                    mutable offset = 7;
                    mutable order = 0;
                    let answer = Relay([
                        { set order = order * 10 + 1; Make(offset) },
                        size = { set order = order * 10 + 2; set offset = 19; 3 }
                    ]);
                    answer * 10000 + order * 100 + offset
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(Value::Int(9_101_219)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn concatenated_array_forwarding_preserves_operand_order_and_captures() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Make(offset : Int) : Int -> Int { value -> offset + value }
                function Times5(value : Int) : Int { value * 5 }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    saved[0](2) * 10000 + saved[1](3) * 100 + saved[2](5)
                }
                @EntryPoint()
                operation Main() : Int {
                    mutable offset = 3;
                    mutable order = 0;
                    let answer = Relay(
                        { set order = order * 10 + 1; [Make(offset), Times5] } +
                        { set offset = 19; set order = order * 10 + 2; [Make(offset)] }
                    );
                    answer * 100 + order
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(Value::Int(5_152_412)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn sliced_array_forwarding_preserves_discarded_element_and_bound_effects() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Make(offset : Int) : Int -> Int { value -> offset + value }
                function Times5(value : Int) : Int { value * 5 }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    saved[0](2) * 100 + saved[1](3)
                }
                @EntryPoint()
                operation Main() : Int {
                    mutable offset = 3;
                    mutable order = 0;
                    let answer = Relay([
                        { set order = order * 10 + 1; Make(offset) },
                        { set offset = 17; set order = order * 10 + 2; Make(offset) },
                        Times5
                    ][{ set offset = 41; set order = order * 10 + 3; 1 }..2]);
                    answer * 100000 + order * 100 + offset
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(Value::Int(191_512_341)));
        check_semantic_equivalence(source);
    }
}

mod round8_captures {
    use crate::test_utils::{
        check_semantic_equivalence, compile_to_fir, try_eval_fir_entry_with_trace,
    };

    #[test]
    fn forwarded_tuple_array_captures_preserve_each_candidate() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Evaluate(payload : (Int, Int[]), value : Int) : Int {
                    let (offset, weights) = payload;
                    offset + weights[0] * value + weights[1]
                }
                function Make(payload : (Int, Int[])) : Int -> Int {
                    value -> Evaluate(payload, value)
                }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Select(actions : (Int -> Int)[], index : Int, value : Int) : Int {
                    actions[index](value)
                }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    mutable answer = 0;
                    for index in 0..2 {
                        set answer = answer * 1000 + Select(saved, index, index + 1);
                    }
                    answer
                }
                @EntryPoint()
                operation Main() : Int {
                    Relay([
                        Make((3, [2, 5])),
                        Make((17, [7, 11])),
                        Make((41, [13, 19]))
                    ])
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(10_042_099)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn generic_forwarder_preserves_tuple_udt_captures() {
        let source = indoc::indoc! {r#"
            namespace Test {
                newtype Weights = (Offset : Int, Factors : Int[]);
                function Evaluate(payload : (Int, Weights), value : Int) : Int {
                    let (bias, weights) = payload;
                    let factors = weights::Factors;
                    bias + weights::Offset + factors[0] * value + factors[1]
                }
                function Make(payload : (Int, Weights)) : Int -> Int {
                    value -> Evaluate(payload, value)
                }
                function Forward<'T>(items : 'T[]) : 'T[] { items }
                function Select(actions : (Int -> Int)[], index : Int, value : Int) : Int {
                    actions[index](value)
                }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    mutable answer = 0;
                    for index in 0..1 {
                        set answer = answer * 1000 + Select(saved, index, index + 2);
                    }
                    answer
                }
                @EntryPoint()
                operation Main() : Int {
                    Relay([
                        Make((3, Weights(5, [7, 11]))),
                        Make((17, Weights(19, [23, 29])))
                    ])
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(33_134)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn forwarded_nested_callable_captures_preserve_inner_values() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Shift(offset : Int) : Int -> Int { value -> offset + value }
                function Evaluate(payload : (Int, Int -> Int), value : Int) : Int {
                    let (scale, action) = payload;
                    scale * action(value)
                }
                function Make(payload : (Int, Int -> Int)) : Int -> Int {
                    value -> Evaluate(payload, value)
                }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Select(actions : (Int -> Int)[], index : Int, value : Int) : Int {
                    actions[index](value)
                }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    mutable answer = 0;
                    for index in 0..2 {
                        set answer = answer * 1000 + Select(saved, index, index + 1);
                    }
                    answer
                }
                @EntryPoint()
                operation Main() : Int {
                    Relay([
                        Make((2, Shift(3))),
                        Make((5, Shift(17))),
                        Make((7, Shift(41)))
                    ])
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(8_095_308)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn deeply_nested_tuple_udt_callable_captures_preserve_scalar_siblings() {
        let source = indoc::indoc! {r#"
            namespace Test {
                newtype Inner = (Bias : Int, Action : Int -> Int);
                function Shift(offset : Int) : Int -> Int { value -> offset + value }
                function Evaluate(payload : (Int, (Int, Inner)), value : Int) : Int {
                    let (scale, (offset, inner)) = payload;
                    let action = inner::Action;
                    scale * action(value) + offset + inner::Bias
                }
                function Make(payload : (Int, (Int, Inner))) : Int -> Int {
                    value -> Evaluate(payload, value)
                }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    mutable answer = 0;
                    for index in 0..1 {
                        set answer = answer * 1000 + saved[index](index + 1);
                    }
                    answer
                }
                @EntryPoint()
                operation Main() : Int {
                    Relay([
                        Make((2, (3, Inner(5, Shift(7))))),
                        Make((11, (13, Inner(17, Shift(19)))))
                    ])
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(24_261)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn deep_inline_payload_preserves_effect_order_and_capture_snapshot() {
        let source = indoc::indoc! {r#"
            namespace Test {
                newtype Inner = (Bias : Int, Action : Int -> Int);
                operation Mark(value : Int) : Int {
                    Message($"field {value}");
                    value
                }
                function Shift(offset : Int) : Int -> Int { value -> offset + value }
                function Evaluate(payload : (Int, (Int, Inner)), value : Int) : Int {
                    let (scale, (offset, inner)) = payload;
                    let action = inner::Action;
                    scale * action(value) + offset + inner::Bias
                }
                function Make(payload : (Int, (Int, Inner))) : Int -> Int {
                    value -> Evaluate(payload, value)
                }
                @EntryPoint()
                operation Main() : Int {
                    mutable seed = 7;
                    let action = Make((Mark(2), (Mark(3), Inner(Mark(5), Shift(seed)))));
                    set seed = 100;
                    action(1)
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(24)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn two_callable_fields_in_captured_payload_preserve_distinct_inner_values() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Shift(offset : Int) : Int -> Int { value -> offset + value }
                function Multiply(factor : Int) : Int -> Int { value -> factor * value }
                function Evaluate(payload : (Int, (Int -> Int, Int -> Int)), value : Int) : Int {
                    let (bias, (first, second)) = payload;
                    bias + 10 * first(value) + second(value)
                }
                function Make(payload : (Int, (Int -> Int, Int -> Int))) : Int -> Int {
                    value -> Evaluate(payload, value)
                }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    mutable answer = 0;
                    for index in 0..1 {
                        set answer = answer * 1000 + saved[index](index + 1);
                    }
                    answer
                }
                @EntryPoint()
                operation Main() : Int {
                    Relay([
                        Make((3, (Shift(5), Multiply(7)))),
                        Make((11, (Shift(13), Multiply(17))))
                    ])
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(70_195)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn fully_consumed_nested_single_tuple_captures_preserve_inner_values() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Shift(offset : Int) : Int -> Int { value -> offset + value }
                function Evaluate(payload : ((Int -> Int,),), value : Int) : Int {
                    let ((action,),) = payload;
                    action(value)
                }
                function Make(payload : ((Int -> Int,),)) : Int -> Int {
                    value -> Evaluate(payload, value)
                }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    mutable answer = 0;
                    for index in 0..1 {
                        set answer = answer * 1000 + saved[index](index + 1);
                    }
                    answer
                }
                @EntryPoint()
                operation Main() : Int {
                    Relay([
                        Make(((Shift(3),),)),
                        Make(((Shift(17),),))
                    ])
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(4_019)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn destructured_callable_leaf_alias_captured_in_returned_lambda_preserves_values() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Shift(offset : Int) : Int -> Int { value -> offset + value }
                function Make(payload : (Int, (Int, Int -> Int))) : Int -> Int {
                    let (_, (_, action)) = payload;
                    let alias = action;
                    value -> alias(value)
                }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let saved = Forward(actions);
                    mutable answer = 0;
                    for index in 0..1 {
                        set answer = answer * 1000 + saved[index](index + 1);
                    }
                    answer
                }
                @EntryPoint()
                operation Main() : Int {
                    Relay([
                        Make((101, (103, Shift(5)))),
                        Make((107, (109, Shift(23))))
                    ])
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(6_025)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn aliased_forwarded_arrays_preserve_captured_array_versions() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Evaluate(payload : (Int, Int[]), value : Int) : Int {
                    let (offset, weights) = payload;
                    offset + weights[0] * value + weights[1]
                }
                function Make(payload : (Int, Int[])) : Int -> Int {
                    value -> Evaluate(payload, value)
                }
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
                function Consume(actions : (Int -> Int)[]) : Int {
                    mutable answer = 0;
                    for index in 0..1 {
                        set answer = answer * 1000 + actions[index](index + 2);
                    }
                    answer
                }
                function Relay(actions : (Int -> Int)[]) : Int {
                    let alias = actions;
                    let forwarded = Forward(alias);
                    let saved = forwarded;
                    Consume(saved)
                }
                @EntryPoint()
                operation Main() : Int {
                    mutable weights = [2, 5];
                    let first = Make((3, weights));
                    set weights w/= 0 <- 7;
                    set weights w/= 1 <- 11;
                    let second = Make((17, weights));
                    set weights w/= 0 <- 101;
                    let actions = [first, second];
                    let alias = actions;
                    Relay(alias) * 1000 + weights[0]
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(12_049_101)));
        check_semantic_equivalence(source);
    }

    #[test]
    fn nested_array_wrapper_fields_preserve_compound_captures() {
        let source = indoc::indoc! {r#"
            namespace Test {
                struct ActionSet { Marker : Int, Actions : (Int -> Int)[] }
                struct Envelope { Header : Int, Inner : ActionSet, Tail : Int }
                function Evaluate(payload : (Int, Int[]), value : Int) : Int {
                    let (offset, weights) = payload;
                    offset + weights[0] * value + weights[1]
                }
                function Make(payload : (Int, Int[])) : Int -> Int {
                    value -> Evaluate(payload, value)
                }
                function Forward(whole : Envelope) : Envelope { whole }
                function Consume(whole : Envelope) : Int {
                    let inner = whole.Inner;
                    let actions = inner.Actions;
                    mutable answer = 0;
                    for index in 0..1 {
                        set answer = answer * 1000 + actions[index](index + 2);
                    }
                    answer * 1000 + whole.Header * 100 + inner.Marker * 10 + whole.Tail
                }
                function Relay(whole : Envelope) : Int {
                    Consume(Forward(whole))
                }
                @EntryPoint()
                operation Main() : Int {
                    let actions = [Make((3, [2, 5])), Make((17, [7, 11]))];
                    let inner = new ActionSet { Marker = 7, Actions = actions };
                    Relay(new Envelope { Header = 5, Inner = inner, Tail = 19 })
                }
            }
        "#};
        let (store, package_id) = compile_to_fir(source);
        let (result, _) = try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(12_049_589)));
        check_semantic_equivalence(source);
    }
}

#[test]
fn forwarded_closure_array_keeps_unequal_capture_values() {
    let source = indoc::indoc! {r#"
        namespace Test {
            function Make(offset : Int, scale : Int) : Int -> Int {
                value -> offset + scale * value
            }
            function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] { actions }
            function Invoke(action : Int -> Int, value : Int) : Int { action(value) }
            function Select(actions : (Int -> Int)[], index : Int, value : Int) : Int {
                Invoke(actions[index], value)
            }
            function Relay(actions : (Int -> Int)[], index : Int, value : Int) : Int {
                Select(Forward(actions), index, value)
            }
            @EntryPoint()
            operation Main() : Int {
                let actions = [Make(3, 2), Make(17, 5), Make(41, 7)];
                mutable answer = 0;
                for index in 0..2 {
                    set answer = answer * 1000 + Relay(actions, index, index + 1);
                }
                answer
            }
        }
    "#};
    let (store, package_id) = crate::test_utils::compile_to_fir(source);
    let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
    assert_eq!(result, Ok(qsc_eval::val::Value::Int(5_027_062)));
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn root_callable_array_forwarding_preserves_capture_arity() {
    for (actions, expected) in [
        ("[Add3, Times5]", 515),
        ("[Make(7), Times5]", 915),
        ("[Make(7), Make(19)]", 922),
    ] {
        let source = formatdoc! {r#"
            namespace Test {{
                function Add3(value : Int) : Int {{ value + 3 }}
                function Times5(value : Int) : Int {{ value * 5 }}
                function Make(offset : Int) : Int -> Int {{ value -> offset + value }}
                function Forward(actions : (Int -> Int)[]) : (Int -> Int)[] {{ actions }}
                function Relay(actions : (Int -> Int)[]) : Int {{
                    let saved = Forward(actions);
                    saved[0](2) * 100 + saved[1](3)
                }}
                @EntryPoint()
                operation Main() : Int {{ Relay({actions}) }}
            }}
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(&source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(expected)), "{actions}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn controlled_root_callable_array_preserves_control_register() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Stamp(width : Int, input : Unit) : Unit is Adj + Ctl {
                body (...) { use scratch = Qubit[width]; }
                adjoint (...) { use scratch = Qubit[width + 1]; }
                controlled (controls, ...) {
                    mutable count = width + 2;
                    for control in controls { set count += 1; }
                    use scratch = Qubit[count];
                }
                controlled adjoint (controls, ...) {
                    mutable count = width + 3;
                    for control in controls { set count += 1; }
                    use scratch = Qubit[count];
                }
            }
            operation Apply(actions : (Unit => Unit is Adj + Ctl)[]) : Unit is Adj + Ctl {
                for action in actions { action(); }
            }
            @EntryPoint()
            operation Main() : Int {
                use controls = Qubit[2];
                let actions = [Stamp(1, _), Stamp(3, _)];
                Controlled Apply(controls, actions);
                Controlled Adjoint Apply(controls, actions);
                17
            }
        }
    "#};
    let (store, package_id) = crate::test_utils::compile_to_fir(source);
    let (result, trace) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
    assert_eq!(result, Ok(qsc_eval::val::Value::Int(17)));
    assert!(!trace.is_empty());
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn nested_newtype_payload_preserves_named_and_unnamed_ancestors() {
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype NestedPayload = (
                Header : Int,
                Payload : (Int, (Int -> Int, Int)),
                (Tail : Int, Bool)
            );
            function Make(offset : Int) : Int -> Int { value -> value + offset }
            function Forward(whole : NestedPayload) : NestedPayload { whole }
            function Consume(whole : NestedPayload) : Int {
                let (marker, (action, stored)) = whole::Payload;
                let (_, _, (tail, enabled)) = whole!;
                if enabled {
                    whole::Header * 1000 + marker * 100 + stored + action(2) + tail
                } else { 0 }
            }
            function Relay(whole : NestedPayload) : Int {
                Consume(Forward(whole))
            }
            @EntryPoint()
            operation Main() : Int {
                Relay(NestedPayload(5, (7, (Make(13), 19)), (23, true)))
            }
        }
    "#};
    let (store, package_id) = crate::test_utils::compile_to_fir(source);
    let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
    assert_eq!(result, Ok(qsc_eval::val::Value::Int(5757)));
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn nested_constructor_arguments_preserve_payload_and_capture_placement() {
    for expression in [
        "Consume(Payload(5, (Add13, 19)))",
        "Consume(Payload(5, (Make(13), 19)))",
        "let saved = Payload(5, (Make(13), 19)); Consume(saved)",
        "Relay(7, Payload(5, (Make(13), 19)), 11) - 18",
    ] {
        let source = formatdoc! {r#"
            namespace Test {{
                newtype Payload = (Header : Int, Contents : (Int -> Int, Int));
                function Make(offset : Int) : Int -> Int {{ value -> value + offset }}
                function Add13(value : Int) : Int {{ value + 13 }}
                function Consume(payload : Payload) : Int {{
                    let (action, stored) = payload::Contents;
                    payload::Header * 100 + stored + action(2)
                }}
                function Relay(first : Int, payload : Payload, last : Int) : Int {{
                    first + Consume(payload) + last
                }}
                @EntryPoint()
                operation Main() : Int {{ {expression} }}
            }}
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(&source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(534)), "{expression}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn specialized_callable_array_preserves_signed_indices_and_bounds() {
    for index in [-4_i64, -3, -2, -1, 0, 1, 2, 3] {
        let source = formatdoc! {r#"
            namespace Test {{
                function Add11(value : Int) : Int {{ value + 11 }}
                function Times3(value : Int) : Int {{ value * 3 }}
                function Minus5(value : Int) : Int {{ value - 5 }}
                function Invoke(actions : (Int -> Int)[], index : Int) : Int {{
                    actions[index](5)
                }}
                @EntryPoint()
                operation Main() : Int {{ Invoke([Add11, Times3, Minus5], {index}) }}
            }}
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(&source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        if (-3..3).contains(&index) {
            let expected = [16, 15, 0][usize::try_from(index.rem_euclid(3)).expect("valid index")];
            assert_eq!(result, Ok(qsc_eval::val::Value::Int(expected)));
        } else {
            assert!(result.is_err(), "out-of-range index must fail");
        }
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn fir_value_preservation_forward_identity() {
    use qsc_fir::fir::{ExprKind, Lit, PackageLookup};

    let source = indoc::indoc! {r#"
        namespace Test {
            function Make(offset : Int) : Int -> Int { value -> value + offset }
            function Forward(callable : Int -> Int) : Int -> Int { callable }
            @EntryPoint()
            operation Main() : Int {
                let callable = Forward(Make(17));
                callable(1)
            }
        }
    "#};
    let (store, package_id) = crate::test_utils::compile_to_fir(source);
    let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
    assert_eq!(result.expect("original must succeed").to_string(), "18");
    crate::test_utils::check_semantic_equivalence(source);
    let (store, package_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
    let package = store.get(package_id);
    let mut found_capture = false;
    for (_, expr) in &package.exprs {
        if let ExprKind::Call(callee, args) = expr.kind
            && let qsc_fir::ty::Ty::Arrow(arrow) = &package.get_expr(callee).ty
            && let ExprKind::Tuple(elements) = &package.get_expr(args).kind
            && elements.len() == 2
            && matches!(
                package.get_expr(elements[0]).kind,
                ExprKind::Lit(Lit::Int(17))
            )
        {
            assert_eq!(package.get_expr(args).ty, *arrow.input);
            found_capture = true;
        }
    }
    assert!(
        found_capture,
        "expected the scalar capture in the lifted call"
    );
}

#[test]
fn fir_value_preservation_forward_relay_and_saved_captures() {
    for (body, expected) in [
        ("let callable = Relay(Make(17)); callable(1)", 18),
        (
            "mutable current = Relay(Make(3)); set current = Relay(Make(17)); let captured = Forward(current); set current = Relay(Make(41)); 100 * captured(1) + current(3)",
            1844,
        ),
        (
            "let current = Relay(Make(17)); let captured = Forward(current); let nested = value -> captured(value) + 5; 100 * nested(1) + nested(2)",
            2324,
        ),
        (
            "mutable current = Relay(Make(3)); set current = Relay(Make(17)); let captured = Forward(current); let nested = value -> captured(value) + 5; set current = Relay(Make(41)); 10000 * nested(1) + 100 * nested(2) + current(3)",
            232_444,
        ),
        (
            "mutable current = Relay(Make(3)); set current = Relay(Make(17)); let captured = Forward(current); let nested = value -> captured(value) + 5; set current = Relay(Make(41)); 10000 * Invoke(nested, 1) + 100 * nested(2) + current(3)",
            232_444,
        ),
        (
            "let first = Forward(Make(3)); let second = Relay(Make(17)); 10000 * first(1) + 100 * second(2) + Invoke(first, 3)",
            41906,
        ),
    ] {
        let source = formatdoc! {r#"
            namespace Test {{
                function Make(offset : Int) : Int -> Int {{ value -> value + offset }}
                function Forward(callable : Int -> Int) : Int -> Int {{ callable }}
                function Relay(callable : Int -> Int) : Int -> Int {{ Forward(callable) }}
                function Invoke(callable : Int -> Int, value : Int) : Int {{ callable(value) }}
                @EntryPoint()
                operation Main() : Int {{ {body} }}
            }}
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(&source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(
            result.expect("original must succeed").to_string(),
            expected.to_string()
        );
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn fir_value_preservation_tuple_assignment_targets() {
    for (body, expected) in [
        (
            "mutable (value, callable) = (14, Add11); let callables = [Add11, Times3]; set (value, callable) = (9, Times3); let answer = Use(value, callables[0]); answer * 100 + value * 10 + callable(2)",
            2096,
        ),
        (
            "mutable (value, callable) = (14, Add11); let callables = [Add11, Minus5]; let answer = Use(value, callables[{ set (value, callable) = (9, Times3); 1 }]); answer * 10000 + value * 100 + callable(2)",
            90906,
        ),
        (
            "mutable (value, callable) = (14, Add11); let callables = [Add11, Times3]; let answer = Use(value, callables[{ set (value, callable) = (9, Times3); 0 }]); answer * 100 + value * 10 + callable(2)",
            2596,
        ),
        (
            "mutable (value, callable) = (14, Add11); let answer = Use(value, { set (value, callable) = (9, Times3); Add11 }); answer * 100 + value * 10 + callable(2)",
            2596,
        ),
        (
            "mutable (value, callable) = (14, Add11); let answer = Use(value, { set value = 9; set callable = Times3; Add11 }); answer * 100 + value * 10 + callable(2)",
            2596,
        ),
    ] {
        check_tuple_assignment_result(body, expected);
    }
}

#[test]
fn fir_value_preservation_tuple_assignment_simultaneous_values() {
    for (body, expected) in [
        (
            "mutable (first, second) = (Add11, Times3); let saved = first; set (first, second) = (second, first); 10000 * first(2) + 100 * second(2) + saved(2)",
            61313,
        ),
        (
            "mutable (value, (first, second)) = (0, (Add11, Times3)); set (value, (first, second)) = (9, (second, first)); value * 10000 + first(2) * 100 + second(2)",
            90613,
        ),
        (
            "mutable (value, callable) = (14, Add11); let pair = (9, Times3); set (value, callable) = pair; value * 100 + callable(2)",
            906,
        ),
        (
            "mutable (value, callable) = (14, Add11); set (value, callable) = Pair(); value * 100 + callable(2)",
            906,
        ),
        (
            "mutable (value, callable) = (14, Add11); set (value, callable) = { let pair = Pair(); pair }; value * 100 + callable(2)",
            906,
        ),
    ] {
        check_tuple_assignment_result(body, expected);
    }
}

#[test]
fn fir_value_preservation_tuple_assignment_rhs_effects() {
    for (body, expected) in [
        (
            "mutable (value, callable) = (14, Add11); mutable order = 0; let pair = { set order = order * 10 + 1; (9, Times3) }; set (value, callable) = pair; set order = order * 10 + 2; order * 10000 + value * 100 + callable(2)",
            120_906,
        ),
        (
            "mutable (first, second) = (Add11, Times3); set (first, second) = (first, { set first = Times3; Minus5 }); first(2) * 100 + second(2)",
            1297,
        ),
        (
            "mutable (value, callable) = (14, Add11); set (value, callable) = { set value = 7; (value, Times3) }; value * 100 + callable(2)",
            706,
        ),
    ] {
        check_tuple_assignment_result(body, expected);
    }
}

#[test]
fn tuple_assignment_preserves_nonadjacent_immutable_snapshot() {
    check_tuple_assignment_result(
        indoc::indoc! {r#"
            mutable (value, callable) = (14, Add11);
            let pair = (value, callable);
            set value = 9;
            set callable = Times3;
            set (value, callable) = pair;
            value * 100 + callable(2)
        "#},
        1413,
    );
}

#[test]
fn tuple_assignment_preserves_effectful_alias_chain_snapshot() {
    check_tuple_assignment_result(
        indoc::indoc! {r#"
            mutable (value, callable) = (14, Add11);
            mutable order = 0;
            let pair = { set order = order * 10 + 1; (value, callable) };
            set callable = Times3;
            let forwarded = pair;
            set order = order * 10 + 2;
            set value = 9;
            set (value, callable) = forwarded;
            order * 10000 + value * 100 + callable(2)
        "#},
        121_413,
    );
}

#[test]
fn tuple_assignment_preserves_nested_and_reused_alias_snapshots() {
    for (body, expected) in [
        (
            "mutable (value, (first, second)) = (14, (Add11, Times3)); let saved = (value, (first, second)); set value = 9; set first = Minus5; set (value, (first, second)) = saved; value * 10000 + first(2) * 100 + second(2)",
            141_306,
        ),
        (
            "mutable offset = 3; mutable (value, callable) = (14, { let captured = offset; input -> input + captured }); let saved = (value, callable); set offset = 17; set callable = { let captured = offset; input -> input + captured }; set (value, callable) = saved; value * 100 + callable(2)",
            1405,
        ),
        (
            "mutable (value, callable) = (14, Add11); let saved = (value, callable); set value = 9; set (value, callable) = saved; let first = value * 100 + callable(2); set callable = Times3; set (value, callable) = saved; first * 10000 + value * 100 + callable(2)",
            14_131_413,
        ),
        (
            "mutable (value, callable) = (14, Add11); let saved = Pair(); set value = 7; set (value, callable) = saved; value * 100 + callable(2)",
            906,
        ),
    ] {
        check_tuple_assignment_result(body, expected);
    }
}

#[test]
fn tuple_snapshot_nested_bindings_preserve_callable_values() {
    check_tuple_assignment_result(
        indoc::indoc! {r#"
            mutable (value, callable) = (14, Add11);
            let (tag, saved) = (3, (value, callable));
            set (value, callable) = (9, Times3);
            set (value, callable) = saved;
            tag * 10000 + value * 100 + callable(2)
        "#},
        31_413,
    );
    check_tuple_assignment_result(
        indoc::indoc! {r#"
            mutable (value, (first, second)) = (14, (Add11, Times3));
            let ((saved, tag), tail) = (((value, (first, second)), 3), 2);
            set (value, (first, second)) = (9, (Minus5, Add11));
            set (value, (first, second)) = saved;
            tail * 10000000 + tag * 1000000 + value * 10000
                + first(2) * 100 + second(2)
        "#},
        23_141_306,
    );
}

#[test]
fn tuple_snapshot_captured_alias_preserves_callable_values() {
    check_tuple_assignment_result(
        indoc::indoc! {r#"
            mutable (value, callable) = (14, Add11);
            let saved = (value, callable);
            let observe = input -> {
                let (stored, action) = saved;
                stored * 100 + action(input)
            };
            set (value, callable) = (9, Times3);
            let before = observe(2);
            set (value, callable) = saved;
            before * 10000 + value * 100 + callable(2)
        "#},
        14_131_413,
    );
}

#[test]
fn tuple_snapshot_assignment_inside_closure_preserves_callable_values() {
    check_tuple_assignment_result(
        indoc::indoc! {r#"
            mutable (value, callable) = (14, Add11);
            let saved = (value, callable);
            let restore = input -> {
                mutable (localValue, localCallable) = (7, Minus5);
                set (localValue, localCallable) = saved;
                localValue * 100 + localCallable(input)
            };
            set (value, callable) = (9, Times3);
            set (value, callable) = saved;
            restore(3) * 10000 + value * 100 + callable(2)
        "#},
        14_141_413,
    );
}

#[test]
fn tuple_snapshot_captured_nested_initializer_runs_once_in_order() {
    check_tuple_assignment_result(
        indoc::indoc! {r#"
            mutable count = 0;
            mutable (value, callable) = (14, Add11);
            let (tag, saved) = ({ set count += 1; count }, {
                set count *= 10;
                (value, callable)
            });
            let observe = input -> {
                let (stored, action) = saved;
                stored * 100 + action(input)
            };
            set count += 2;
            set (value, callable) = (9, Times3);
            set (value, callable) = saved;
            count * 1000000 + tag * 100000 + observe(2) * 10 + callable(2)
        "#},
        12_114_143,
    );
}

#[test]
fn tuple_snapshot_forwarding_preserves_nested_noncallable_fields() {
    let source = indoc::indoc! {r#"
        namespace Test {
            function Make(offset : Int) : Int -> Int { value -> value + offset }
            function Forward(pair : (Bool, (Int, Int -> Int), Int))
                : (Bool, (Int, Int -> Int), Int) { pair }
            @EntryPoint()
            operation Main() : Int {
                let saved = Forward(Forward((true, (14, Make(3)), 7)));
                let (tag, (value, callable), tail) = saved;
                if tag { value * 1000 + callable(2) * 10 + tail } else { 0 }
            }
        }
    "#};
    let (store, package_id) = crate::test_utils::compile_to_fir(source);
    let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
    assert_eq!(result, Ok(qsc_eval::val::Value::Int(14_057)));
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn tuple_snapshot_forwarding_preserves_closure_payload_shape() {
    let source = indoc::indoc! {r#"
        namespace Test {
            function Forward(pair : (Int, Int -> Int)) : (Int, Int -> Int) { pair }
            @EntryPoint()
            operation Main() : Int {
                mutable offset = 3;
                mutable (value, callable) = (14, {
                    let captured = offset;
                    input -> input + captured
                });
                let saved = Forward((value, callable));
                set offset = 17;
                set (value, callable) = (9, {
                    let captured = offset;
                    input -> input + captured
                });
                let forwarded = Forward(saved);
                set (value, callable) = forwarded;
                offset * 10000 + value * 100 + callable(2)
            }
        }
    "#};
    let (store, package_id) = crate::test_utils::compile_to_fir(source);
    let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
    assert_eq!(result, Ok(qsc_eval::val::Value::Int(171_405)));
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn nested_wrapper_array_preserves_inner_capture_shapes() {
    let source = indoc::indoc! {r#"
        namespace Test {
            function Make(offset : Int) : Int -> Int { value -> value + offset }
            function Wrap(inner : Int -> Int, scale : Int) : Int -> Int {
                value -> inner(value) * scale
            }
            function Invoke(callable : Int -> Int, value : Int) : Int { callable(value) }
            @EntryPoint()
            operation Main() : Int {
                let wrappers = [Wrap(Wrap(Make(3), 2), 3), Wrap(Wrap(Make(17), 5), 7)];
                mutable answer = 0;
                for index in 0..0 {
                    set answer = 1000 * Invoke(wrappers[index], 1) + wrappers[1 - index](1);
                }
                answer
            }
        }
    "#};
    let (store, package_id) = crate::test_utils::compile_to_fir(source);
    let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
    assert_eq!(result, Ok(qsc_eval::val::Value::Int(24_630)));
    crate::test_utils::check_semantic_equivalence(source);
}

fn check_tuple_assignment_result(body: &str, expected: i64) {
    let source = formatdoc! {r#"
        namespace Test {{
            function Add11(value : Int) : Int {{ value + 11 }}
            function Times3(value : Int) : Int {{ value * 3 }}
            function Minus5(value : Int) : Int {{ value - 5 }}
            function Pair() : (Int, Int -> Int) {{ (9, Times3) }}
            function Use(value : Int, callable : Int -> Int) : Int {{ callable(value) }}
            @EntryPoint()
            operation Main() : Int {{ {body} }}
        }}
    "#};
    let (store, package_id) = crate::test_utils::compile_to_fir(&source);
    let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
    assert_eq!(result, Ok(qsc_eval::val::Value::Int(expected)));
    crate::test_utils::check_semantic_equivalence(&source);
}

#[test]
fn exploration_capture_nested_callable_producer_instances() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function Make(offset : Int) : Int -> Int { value -> value + offset }
            function Wrap(inner : Int -> Int, scale : Int) : Int -> Int {
                value -> inner(value) * scale
            }
            function Invoke(callable : Int -> Int, value : Int) : Int { callable(value) }
            @EntryPoint()
            operation Main() : (Int, Int, Int, Int) {
                let first = Wrap(Make(3), 2);
                let second = Wrap(Make(17), 5);
                (first(1), Invoke(second, 2), Invoke(first, 3), second(4))
            }
        }
    "#});
}

fn exploration_nested_source(body: &str) -> String {
    formatdoc! {r#"
        namespace Test {{
            function Make(offset : Int) : Int -> Int {{ value -> value + offset }}
            function Wrap(inner : Int -> Int, scale : Int) : Int -> Int {{
                value -> inner(value) * scale
            }}
            function Flat(offset : Int, scale : Int) : Int -> Int {{
                value -> (value + offset) * scale
            }}
            function Invoke(callable : Int -> Int, value : Int) : Int {{ callable(value) }}
            @EntryPoint()
            operation Main() : (Int, Int) {{ {body} }}
        }}
    "#}
}

#[test]
fn exploration_capture_reduced_mixed() {
    crate::test_utils::check_semantic_equivalence(&exploration_nested_source(
        "let first = Wrap(Make(3), 2); (first(1), Invoke(first, 3))",
    ));
}

#[test]
fn exploration_capture_reduced_direct_only() {
    crate::test_utils::check_semantic_equivalence(&exploration_nested_source(
        "let first = Wrap(Make(3), 2); (first(1), first(3))",
    ));
}

#[test]
fn exploration_capture_reduced_hof_only() {
    crate::test_utils::check_semantic_equivalence(&exploration_nested_source(
        "let first = Wrap(Make(3), 2); (Invoke(first, 1), Invoke(first, 3))",
    ));
}

#[test]
fn exploration_capture_reduced_flat_control() {
    crate::test_utils::check_semantic_equivalence(&exploration_nested_source(
        "let first = Flat(3, 2); (first(1), Invoke(first, 3))",
    ));
}

#[test]
fn closure_used_in_capture_assignment_preserves_execution() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use target = Qubit();
                mutable angle = 0.0;
                let op = Rx(angle, _);
                set angle = { op(target); 0.0 };
                op(target);
                Reset(target);
            }
        }
    "#});
}

#[test]
fn indexed_callable_argument_preserves_failure_order() {
    let cases =
        [("first_arg_failure", 1), ("in_bounds_failure_control", 0)].map(|(name, index)| {
            (
                name,
                formatdoc! {r#"
                namespace Test {{
                    function Identity(value : Int) : Int {{ value }}
                    function FailFirst() : Int {{ fail "first argument" }}
                    operation Use(value : Int, op : Int -> Int) : Int {{ op(value) }}
                    @EntryPoint()
                    operation Main() : Int {{ Use(FailFirst(), [Identity][{index}]) }}
                }}
            "#},
            )
        });
    check_indexed_callable_argument_cases(cases);
}

#[test]
fn indexed_callable_argument_preserves_effect_order() {
    let cases = [
        ("single_hof_prior_effect_invalid", "Use(Earlier(target), [Z][1], target);"),
        ("single_hof_ordered_success", "let ops = [Z]; Use({ X(target); 42 }, ops[{ Y(target); 0 }], target);"),
        ("multi_hof_ordered_success_zero", "let ops = [Z, S]; for index in 0..0 { Use({ X(target); 42 }, ops[{ Y(target); index }], target); }"),
        ("multi_hof_ordered_success_one", "let ops = [Z, S]; for index in 1..1 { Use({ X(target); 42 }, ops[{ Y(target); index }], target); }"),
        ("direct_callee_ordered_success", "[Z][Index(target, 0)](Argument(target));"),
        ("single_hof_pure_index_success", "Use(Earlier(target), [Z][0], target);"),
        ("multi_hof_pure_index_success_zero", "let ops = [Z, S]; for index in 0..0 { Use(Earlier(target), ops[index], target); }"),
        ("multi_hof_pure_index_success_one", "let ops = [Z, S]; for index in 1..1 { Use(Earlier(target), ops[index], target); }"),
    ].map(|(name, body)| {
        (name, formatdoc! {r#"
            namespace Test {{
                operation Earlier(target : Qubit) : Int {{ X(target); 42 }}
                operation Index(target : Qubit, index : Int) : Int {{ Y(target); index }}
                operation Argument(target : Qubit) : Qubit {{ X(target); target }}
                operation Use(value : Int, op : Qubit => Unit, target : Qubit) : Unit {{
                    if value != 42 {{ fail "earlier argument changed"; }}
                    op(target);
                }}
                @EntryPoint()
                operation Main() : Unit {{
                    use target = Qubit();
                    {body}
                    Reset(target);
                }}
            }}
        "#})
    });
    check_indexed_callable_argument_cases(cases);
}

fn check_indexed_callable_argument_cases(cases: impl IntoIterator<Item = (&'static str, String)>) {
    let mut failures = Vec::new();
    for (name, source) in cases {
        let passed = std::panic::catch_unwind(|| {
            if name != "first_arg_failure" && name != "in_bounds_failure_control" {
                use crate::test_utils::{TraceOp, compile_to_fir, try_eval_fir_entry_with_trace};
                let (store, package_id) = compile_to_fir(&source);
                let (result, trace) = try_eval_fir_entry_with_trace(&store, package_id);
                let gates = trace
                    .iter()
                    .filter_map(|operation| match operation {
                        TraceOp::Gate { name, .. } => Some(name.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let expected = match name {
                    "single_hof_prior_effect_invalid" => vec!["X"],
                    "multi_hof_ordered_success_one" => vec!["X", "Y", "S"],
                    "direct_callee_ordered_success" => vec!["Y", "X", "Z"],
                    "single_hof_pure_index_success" | "multi_hof_pure_index_success_zero" => {
                        vec!["X", "Z"]
                    }
                    "multi_hof_pure_index_success_one" => vec!["X", "S"],
                    _ => vec!["X", "Y", "Z"],
                };
                assert_eq!(gates, expected, "original gate order for {name}");
                assert_eq!(result.is_err(), name == "single_hof_prior_effect_invalid");
                let (store, package_id) = crate::test_utils::compile_and_run_pipeline_to(
                    &source,
                    crate::PipelineStage::Defunc,
                );
                let rendered = crate::pretty::write_package_qsharp(&store, package_id);
                let main = rendered
                    .split("operation Main()")
                    .nth(1)
                    .expect("entry operation must be emitted")
                    .split("\noperation ")
                    .next()
                    .expect("entry body must be emitted");
                if name.contains("pure_index_success") || name == "single_hof_prior_effect_invalid"
                {
                    assert!(
                        main.contains("{ Z }"),
                        "expected specialized Z dispatch: {main}"
                    );
                    if name.starts_with("multi_hof") {
                        assert!(
                            main.contains("{ S }") && main.contains("if (index == 0)"),
                            "expected both indexed dispatch branches: {main}"
                        );
                    }
                } else if name == "direct_callee_ordered_success" {
                    assert!(
                        main.contains("Z(Argument(target))"),
                        "expected direct Z call: {main}"
                    );
                }
            }
            crate::test_utils::check_semantic_equivalence(&source);
        })
        .is_ok();
        eprintln!("{name}: {}", if passed { "passed" } else { "failed" });
        if !passed {
            failures.push(name);
        }
    }
    assert!(failures.is_empty(), "semantic failures: {failures:?}");
}

#[test]
fn residual_callable_sources_preserve_semantics() {
    let mut failures = Vec::new();
    for (name, source) in residual_callable_sources() {
        if std::panic::catch_unwind(|| crate::test_utils::check_semantic_equivalence(&source))
            .is_err()
        {
            failures.push(name);
        }
        eprintln!("checked {name}");
    }
    assert!(failures.is_empty(), "semantic failures: {failures:?}");
}

fn residual_callable_sources() -> Vec<(&'static str, String)> {
    let mut sources = Vec::new();
    for (name, body) in [
        ("false_branch", "if false { ApplyOp(ops[index], q); }"),
        ("false_loop", "while false { ApplyOp(ops[index], q); }"),
        ("post_return", "return (); ApplyOp(ops[index], q);"),
    ] {
        sources.push((
            name,
            format!(
                r#"
            namespace Test {{
                operation MakeCandidates(q : Qubit) : (Qubit => Unit)[] {{ Y(q); [H, X] }}
                operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {{ op(target); }}
                @EntryPoint()
                operation Main() : Unit {{
                    use q = Qubit();
                    let ops = MakeCandidates(q);
                    let index = if MResetZ(q) == Zero {{ 0 }} else {{ 1 }};
                    {body}
                }}
            }}
        "#
            ),
        ));
    }
    sources.push((
        "killed_producer",
        r#"
        namespace Test {
            operation MakeOp(q : Qubit) : Qubit => Unit { X(q); Rx(0.0, _) }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit { op(target); }
            operation Replacement(q : Qubit) : Unit { H(q); }
            operation LoopValue(q : Qubit) : Unit { X(q); }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                mutable op = MakeOp(q);
                op = Replacement;
                for _ in 0..2 { op = LoopValue; }
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#
        .to_string(),
    ));
    sources.push((
        "unrelated_callable",
        r#"
        namespace Test {
            function Identity(value : Int) : Int { value }
            operation Unrelated() : Unit { let decoy = Identity; }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Unrelated();
                mutable op = H;
                for _ in 0..3 { op = X; }
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#
        .to_string(),
    ));
    sources
}

/// Regression for a consumed-closure stand-in reaching a live call.
///
/// A call expression dispatched over several distinct closure candidates gets a
/// per-row specialization for each, but the rewrite cannot discriminate between
/// them when the value arrives through a dynamic index and identical conditional
/// arms. Consuming the producers anyway replaced them with `fail`-bodied
/// stand-ins that the surviving read of `ops[idx]` still invoked, so the
/// pipeline succeeded and the program aborted at runtime with a
/// compiler-internal message instead of applying `Rx` or `Ry`.
///
/// The rotation angle is zero so both qubits end in a deterministic, releasable
/// state; the dispatched gate still appears in the effect trace, which is what
/// distinguishes correct dispatch from a call into the stand-in.
#[test]
fn consumed_closure_stand_in_is_not_specialized_against() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation Run(f : Qubit => Unit, q : Qubit) : Unit { f(q); }
            @EntryPoint()
            operation Main() : (Result, Result) {
                use q = Qubit();
                use target = Qubit();
                let a = 0.0;
                let ops = [q0 => Rx(a, q0), q0 => Ry(a, q0)];
                let m = MResetZ(q);
                let idx = m == One ? 0 | 1;
                let cond2 = m == One;
                let f = cond2 ? ops[idx] | ops[idx];
                Run(f, target);
                return (m, MResetZ(target));
            }
        }
    "#});
}

/// Regression for controlled dispatch of a *capturing* closure passed to a
/// higher-order operation whose callable parameter is **not** the first
/// argument. The HOF applies `Controlled op(ctls, q)`, so rewrite must nest the
/// closure's captures inside the base input tuple beneath the control register
/// (`([ctls], (q, capture0, capture1))`) rather than appending them as trailing
/// top-level siblings of `([ctls], q)`. A mis-placed capture would either crash
/// downstream control/input splitting or diverge from the original semantics.
///
/// The control qubit is prepared |1> so the controlled rotation actually fires;
/// the captured angles are threaded through a partial application so the closure
/// carries two ordered captures across the control boundary (exercising the
/// multi-capture nesting order, not just placement).
#[test]
fn controlled_capturing_closure_nonzero_param_slot_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation RotOp(a : Double, b : Double, q : Qubit) : Unit is Adj + Ctl {
                Rx(a, q);
                Rz(b, q);
            }
            operation ApplyCtl(ctls : Qubit[], op : Qubit => Unit is Ctl, q : Qubit) : Unit {
                Controlled op(ctls, q);
            }
            @EntryPoint()
            operation Main() : Result {
                use ctl = Qubit();
                use q = Qubit();
                X(ctl);
                let a = 3.141592653589793;
                let b = 1.5707963267948966;
                let op = RotOp(a, b, _);
                ApplyCtl([ctl], op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Regression for recorded direct-rewrite cleanup. `GetOp(q)` performs `X(q)`
/// before returning the named callable `X`; direct dispatch consumes `op`, so
/// the cleanup must retain the now-unused immutable binding and its effect.
#[test]
fn recorded_direct_rewrite_cleanup_retains_effectful_callable_initializer() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetOp(q : Qubit) : (Qubit => Unit) {
                X(q);
                X
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let op = GetOp(q);
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for the removal gate on a rewritten higher-order argument. The
/// factory is a pure `function`, so its consumed result makes the binding a
/// deletion candidate, but its body can still fail. Cleanup must keep the
/// binding so the division failure stays observable.
#[test]
fn rewritten_hof_arg_cleanup_retains_fallible_function_factory() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function GetOp(divisor : Int) : Qubit => Unit {
                let ignored = 1 / divisor;
                X
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let op = GetOp(0);
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for demoting a dead callable binding that must still run. The
/// captured angle comes from `GetAngle`, which flips the qubit, so dropping the
/// binding outright would lose that effect. Cleanup keeps the evaluation and
/// discards only the consumed callable value.
#[test]
fn demoted_dead_callable_binding_retains_capture_effect() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetAngle(q : Qubit) : Double {
                X(q);
                0.0
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let op = Rx(GetAngle(q), _);
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for `prune_dead_callable_locals_in_block`. The initializer is
/// intentionally unused and never passed to a higher-order operation, so it
/// must be retained by the global dead-local pruner solely for its `X(q)`.
#[test]
fn global_dead_local_pruner_retains_effectful_callable_initializer() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetOp(q : Qubit) : (Qubit => Unit) {
                X(q);
                X
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let unused = GetOp(q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for the orphaned-producer skip. `MakeOp` is a pure producer whose
/// callable result is consumed by specialization, so the binding in `Main`
/// disappears and `MakeOp` loses its only caller. Cleanup no longer visits it,
/// which leaves the closure in its body untouched to disappear with the item at
/// DCE.
///
/// The caller keeps observable evaluation on both sides of the consumed call,
/// and `ApplyOp` wraps the dispatched operation in its own gates, so the effect
/// trace pins the count and the order of `X`, `H`, `Rx`, `H`, `Y`. Structure
/// alone cannot catch a producer whose evaluation is dropped or replayed here;
/// the trace can.
#[test]
fn orphaned_producer_body_preserves_caller_evaluation_order() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation InnerOp(angle : Double, q : Qubit) : Unit {
                Rx(angle, q);
            }
            function MakeOp(angle : Double) : Qubit => Unit {
                return InnerOp(angle, _);
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                X(q);
                let op = MakeOp(1.5707963267948966);
                ApplyOp(op, q);
                Y(q);
                MResetZ(q)
            }
        }
    "#});
}

#[test]
fn producer_factory_unsafe_expressions_preserve_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation Mark(enabled : Bool, q : Qubit) : Unit {
                if enabled {
                    X(q);
                }
            }
            function Make(enabled : Bool) : Qubit => Unit {
                Mark(enabled, _)
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                mutable enabled = false;
                let op = Make(enabled);
                set enabled = true;
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function Choose(flag : Bool) : Qubit => Unit {
                if not flag {
                    X
                } else {
                    Z
                }
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let source = false;
                let op = Choose(source);
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for the aggregate-slot replacement: a consumed closure that is a
/// direct element of a UDT constructor's argument tuple. Both branches of
/// `Choose` are taken so each closure is specialized, and every read of the
/// arrow-typed `F` field is rewritten to a direct call. `Offset` is read too, so
/// the constructor call stays entry-reachable and cleanup replaces the closure
/// inside it. The closures capture nothing, so the replacement is a reference to
/// each closure's own target callable and the slot keeps its arrow type.
///
/// The caller drives quantum effects from the dispatched results, so the effect
/// trace pins how many times each specialized function ran and in what order:
/// `fT(2)` is 3 and `fF(2)` is 4, giving `X`, three `H`, `Z`, four `Y`. Dropping
/// or replaying a dispatch changes the gate counts, which structure alone would
/// not reveal. The returned sum independently pins the second pair of
/// applications and both surviving non-arrow fields.
#[test]
fn aggregate_slot_capture_free_replacement_preserves_dispatch_count_and_order() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            newtype Choice = (F : Int -> Int, Offset : Int);

            function Choose(flag : Bool) : Choice {
                if flag {
                    Choice(x -> x + 1, 100)
                } else {
                    Choice(x -> x * 2, 7)
                }
            }

            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let selectedT = Choose(true);
                let selectedF = Choose(false);
                let fT = selectedT::F;
                let fF = selectedF::F;
                X(q);
                for _ in 1..fT(2) {
                    H(q);
                }
                Z(q);
                for _ in 1..fF(2) {
                    Y(q);
                }
                Reset(q);
                fT(10) + fF(10) + selectedT::Offset + selectedF::Offset
            }
        }
    "#});
}

/// The capturing counterpart of the test above, and the shape that motivated
/// the synthesized stand-in. `Std.TableLookup.MakeAndChain` builds
/// `AndChain(depth, helper => AndChainOperation(ctls, helper, target))`: a
/// closure capturing two values, sitting directly in a UDT-constructor argument
/// tuple, in a body that stays entry-reachable. There is no capture-free target
/// to name, so cleanup must replace it with a fail-bodied stand-in of the same
/// arrow type.
///
/// `Select` is driven with the address register prepared |1>, so the lookup
/// resolves to `data[1] = [true]` and the returned measurement is deterministic.
/// The effect trace pins the whole gate sequence the library emits, so a
/// stand-in that was accidentally reachable, or a dispatch dropped or replayed
/// by the replacement, changes the trace even where the result would not.
#[test]
fn std_table_lookup_select_capturing_aggregate_slot_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use address = Qubit[1];
                use output = Qubit[1];
                X(address[0]);
                Std.TableLookup.Select([[false], [true]], address, output);
                let result = MResetZ(output[0]);
                ResetAll(address);
                result
            }
        }
    "#});
}

/// `EvaluationDisposition::Discarded`. `MakeOp` is a pure, total factory, so
/// deleting the consumed binding drops an evaluation that was never observable.
///
/// The trace pins `Y`, `H`, `X`, `H`, `Z`: the surrounding gates fix where the
/// dispatch lands in the order, and `ApplyOp`'s own `H` pair fixes how many
/// times it ran. A dropped, duplicated, or reordered dispatch changes the
/// sequence even though the returned value would not.
#[test]
fn discarded_disposition_drops_only_unobservable_evaluation() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function MakeOp() : Qubit => Unit {
                X
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Y(q);
                let op = MakeOp();
                ApplyOp(op, q);
                Z(q);
                MResetZ(q)
            }
        }
    "#});
}

/// Exercises `EvaluationDisposition::Relocated`. `GetAngle` flips the qubit
/// while computing the captured angle, and the rewrite splices that initializer
/// into the specialized call, so deleting the binding *moves* the flip rather
/// than dropping it.
///
/// The trace pins `Y`, `X`, `H`, `Rx`, `H`, `Z`. Dropping the binding without
/// relocating loses the `X`; retaining it after relocation runs the `X` twice.
/// Both are invisible to structure and to the returned value, and both change
/// this sequence.
#[test]
fn relocated_disposition_moves_capture_evaluation_exactly_once() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetAngle(q : Qubit) : Double {
                X(q);
                0.0
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Y(q);
                let op = Rx(GetAngle(q), _);
                ApplyOp(op, q);
                Z(q);
                MResetZ(q)
            }
        }
    "#});
}

/// `EvaluationDisposition::Replayed` by branch dispatch. The binding is a
/// static callable selection, so deleting it is sound only because
/// `branch_split_direct_call_rewrite` emits the same `if` tree at the replaced
/// call site.
///
/// The selecting condition is a measurement, which makes the replay observable:
/// the condition is not safe to discard, so the binding reaches the replay rule
/// rather than the discard rule, and the trace records where and how often the
/// measurement ran. Replaying it twice, dropping it, or moving it across the
/// surrounding `X` and `Z` all change the sequence, and none of those changes
/// alters the returned value or the transformed program's structure in a way a
/// snapshot would flag.
#[test]
fn replayed_disposition_reruns_the_branch_selection() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use flag = Qubit();
                use q = Qubit();
                X(flag);
                X(q);
                let op = if MResetZ(flag) == One { Y } else { Z };
                ApplyOp(op, q);
                Z(q);
                MResetZ(q)
            }
        }
    "#});
}

/// `EvaluationDisposition::Replayed` by index dispatch at the *argument*
/// position, the one rule that differs between the two consumption sites. The
/// rewrite resolves `ops[1]` statically and calls the selected callable
/// directly, so the selection is replayed and only the bounds check is elided.
///
/// The trace pins `Z`, `H`, `Y`, `H`. Selecting the wrong element swaps `Y` for
/// `X`; dropping the dispatch removes it entirely.
#[test]
fn replayed_index_selection_at_argument_position_preserves_dispatch() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let ops = [X, Y];
                Z(q);
                ApplyOp(ops[1], q);
                MResetZ(q)
            }
        }
    "#});
}

#[test]
fn indexed_dispatch_preserves_out_of_range_failures() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyAt(ops : (Qubit => Unit)[], idx : Int, q : Qubit) : Unit {
                ops[idx](q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                ApplyAt([Z, X], 2, q);
                MResetZ(q)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let ops = [Z, X];
                ops[2](q);
                MResetZ(q)
            }
        }
    "#});
}

#[test]
fn indexed_dispatch_preserves_duplicate_physical_positions() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use flag = Qubit();
                use target = Qubit();
                X(flag);
                let index = if MResetZ(flag) == One { 1 } else { 0 };
                let ops = [I, I, X];
                ops[index](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyAt(ops : (Qubit => Unit)[], index : Int, target : Qubit) : Unit {
                ops[index](target);
            }
            @EntryPoint()
            operation Main() : Result {
                use flag = Qubit();
                use target = Qubit();
                X(flag);
                let index = if MResetZ(flag) == One { 1 } else { 0 };
                ApplyAt([I, I, X], index, target);
                MResetZ(target)
            }
        }
    "#});
}

#[test]
fn indexed_dispatch_preserves_singleton_bounds_and_effectful_index_evaluation() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyAt(ops : (Qubit => Unit)[], index : Int, target : Qubit) : Unit {
                ops[index](target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                ApplyAt([X], 1, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let ops = [X];
                ops[1](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyAt(ops : (Qubit => Unit)[], index : Int, target : Qubit) : Unit {
                ops[index](target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                ApplyAt([Z], {
                    X(target);
                    0
                }, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let ops = [Z];
                ops[{
                    X(target);
                    0
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let ops = [I, Z];
                for index in 1..1 {
                    let op = ops[{
                        X(target);
                        index
                    }];
                    op(target);
                }
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable ops = [I, Z];
                set ops = [Z, I];
                ops[{
                    X(target);
                    0
                }](target);
                MResetZ(target)
            }
        }
    "#});
}

#[test]
#[allow(clippy::too_many_lines)]
fn indexed_struct_field_source_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [Z] };
                config.Ops[{
                    X(target);
                    0
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [I, Z] };
                config.Ops[{
                    X(target);
                    1
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [X] };
                config.Ops[1](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [X] };
                ApplyOp(config.Ops[1], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [Z] };
                ApplyOp(config.Ops[{
                    X(target);
                    0
                }], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyValue(value : Int, target : Qubit) : Unit {
                if value == 1 {
                    Z(target);
                }
            }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let value = 1;
                let config = new Config { Ops = [ApplyValue(value, _)] };
                ApplyOp(config.Ops[{
                    X(target);
                    0
                }], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [I, X] };
                ApplyOp(config.Ops[1], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [I, X] };
                ApplyOp(config.Ops[2], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            struct Outer { Inner : Config }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let outer = new Outer {
                    Inner = new Config { Ops = [I, Z] }
                };
                outer.Inner.Ops[{
                    X(target);
                    1
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let base = new Config { Ops = [X] };
                let config = new Config { ...base, Ops = [I, X] };
                config.Ops[1](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable config = new Config { Ops = [X, I] };
                set config w/= Ops <- [I, X];
                config.Ops[0](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            newtype Wrapped = (Ops : (Qubit => Unit is Adj + Ctl)[]);
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let wrapped = Wrapped([I, X]);
                wrapped::Ops[1](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [I, X] };
                ApplyBoth(config.Ops[1], I, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [X] };
                ApplyBoth(config.Ops[1], I, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [X] };
                ApplyBoth(config.Ops[{
                    X(target);
                    1
                }], I, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use (flag, target) = (Qubit(), Qubit());
                X(flag);
                let index = if MResetZ(flag) == One { 1 } else { 0 };
                let config = new Config { Ops = [I, X] };
                X(target);
                ApplyBoth(config.Ops[index], I, target);
                MResetZ(target)
            }
        }
    "#});
}

#[test]
#[allow(clippy::too_many_lines)]
fn unresolved_indexed_struct_field_source_declines_atomically() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [Z] }, 0);
                config.Ops[{
                    X(target);
                    ignored
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [I, Z] }, 0);
                config.Ops[{
                    X(target);
                    ignored + 1
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [X] }, 0);
                config.Ops[ignored + 1](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [X] }, 0);
                ApplyOp(config.Ops[ignored + 1], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [I, Z] }, 0);
                ApplyOp(config.Ops[{
                    X(target);
                    ignored + 1
                }], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [X] }, 0);
                ApplyBoth(config.Ops[ignored + 1], I, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use (flag, target) = (Qubit(), Qubit());
                X(flag);
                let index = if MResetZ(flag) == One { 1 } else { 0 };
                let (config, ignored) = (new Config { Ops = [I, X] }, 0);
                X(target);
                ApplyBoth(config.Ops[index + ignored], I, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyValue(value : Int, target : Qubit) : Unit {
                if value == 1 {
                    Z(target);
                }
            }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let value = 1;
                let (config, ignored) = (
                    new Config { Ops = [ApplyValue(value, _)] },
                    0
                );
                ApplyOp(config.Ops[{
                    X(target);
                    ignored
                }], target);
                MResetZ(target)
            }
        }
    "#});
}

/// `EvaluationDisposition::Retained`. `GetOp` applies `X` before returning the
/// named callable it produces, and nothing relocates or replays that `X`, so
/// the binding must survive even though its callable value is consumed.
///
/// The trace pins `Y`, `X`, `H`, `Z`, `H`, `Y`. Deleting the binding drops the
/// leading `X`; hoisting it past the surrounding gates reorders the sequence.
#[test]
fn retained_disposition_keeps_observable_producer_evaluation_in_place() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetOp(q : Qubit) : (Qubit => Unit) {
                X(q);
                Z
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Y(q);
                let op = GetOp(q);
                ApplyOp(op, q);
                Y(q);
                MResetZ(q)
            }
        }
    "#});
}

/// The recursive self-call slot deleted by `remove_arg_at_path` can only hold a
/// global item reference or a closure, so the deletion discards nothing
/// observable. `Repeat`'s self-call forwards the named `H`, which is exactly the
/// slot shape `assert_discarded_slot_is_pure` states, and running the pipeline
/// exercises that assertion.
///
/// The trace pins `X`, four `H`, then `Y`. Dropping or duplicating a recursion
/// step changes the number of `H`s. The count is even so the four gates compose
/// to the identity and the measured result stays deterministic, which keeps the
/// value comparison meaningful alongside the trace comparison.
#[test]
fn recursive_self_call_slot_removal_preserves_recursion_count() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation Repeat(op : Qubit => Unit, n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat(H, n - 1, q);
                }
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                X(q);
                Repeat(H, 4, q);
                Y(q);
                MResetZ(q)
            }
        }
    "#});
}

/// Generates syntactically valid Q# programs exercising defunctionalization's
/// key code paths: lambda arguments, partial application, and direct callable
/// references passed to higher-order functions.
fn defunc_pattern_strategy() -> impl Strategy<Value = String> {
    let val = || 0..50i64;

    prop_oneof![
        // 1. Lambda passed as argument to a higher-order function.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(x -> x + {a}, {b})
                }}
            }}
        "}),
        // 2. Partial application of a two-argument function.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function Add(x : Int, y : Int) : Int {{ x + y }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(Add({a}, _), {b})
                }}
            }}
        "}),
        // 3. Direct callable reference as argument.
        val().prop_map(|a| formatdoc! {"
            namespace Test {{
                function Double(x : Int) : Int {{ x * 2 }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(Double, {a})
                }}
            }}
        "}),
        // 4. Nested higher-order calls: function returning a lambda.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function MakeAdder(n : Int) : Int -> Int {{ x -> x + n }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(MakeAdder({a}), {b})
                }}
            }}
        "}),
    ]
}

/// Generates programs with multi-capture closures where the captures have
/// distinct values and are used in non-commutative operations, ensuring
/// capture ordering is exercised.
fn multi_capture_strategy() -> impl Strategy<Value = String> {
    // Use distinct non-zero values so swapped captures produce a different result.
    (2..20i64, 1..10i64)
        .prop_filter("a must differ from b", |(a, b)| a != b && *b != 0)
        .prop_flat_map(|(a, b)| {
            prop_oneof![
                // Two captures used in non-commutative subtraction.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            Apply(x -> a - b + x, 0)
                        }}
                    }}
                "}),
                // Two captures used in non-commutative division.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            Apply(x -> a / b + x, 0)
                        }}
                    }}
                "}),
                // Three captures in position-sensitive expression.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            let c = 1;
                            Apply(x -> (a - b) * c + x, 0)
                        }}
                    }}
                "}),
            ]
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn proptest_defunctionalize_preserves_semantics(source in defunc_pattern_strategy()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]
    #[test]
    fn proptest_multi_capture_ordering_preserves_semantics(source in multi_capture_strategy()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

/// Regression for the `Multi ⊔ Multi` (nested dispatch on both sides) join: a
/// callable-valued local is selected by an outer dynamic `if` whose *both*
/// branches are themselves dynamic conditionals, and the *same* callable (`X`)
/// reaches the local from both branches under different guards.
///
/// The lattice merge must not deduplicate the false-branch occurrence of `X`
/// by callable identity — doing so drops the `!outer && rb` dispatch arm and
/// makes that path fall through to the outer default (`Z`) instead of applying
/// `X`. The fixture pins `outer == false` (`a` stays |0>) and the false-branch
/// inner guard `rb == One` (`b` is |1>), so the dropped arm is exactly the path
/// taken: the original applies `X(q)` (measuring `One`) while the buggy rewrite
/// applies `Z(q)` (measuring `Zero`), diverging in both return value and effect
/// trace. The guards are pure reads of pre-measured `Result` locals so the
/// fixture isolates the lattice merge from condition-hoisting concerns.
#[test]
fn multi_multi_shared_callable_across_branches_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                use b = Qubit();
                // a stays |0> so the outer guard is false; b is |1> so the
                // false-branch inner guard is true — the dispatch arm the
                // identity-dedup would drop.
                X(b);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let op = if ra == One {
                             if rb == One { X } else { Y }
                         } else {
                             if rb == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Regression for the `Single ⊔ Multi` join: a callable-valued local is
/// selected by an outer dynamic `if` whose *true* branch is a single concrete
/// callable (`X`) and whose *false* branch is itself a dynamic conditional that
/// can also yield `X` (under its own guard).
///
/// The lattice merge must not deduplicate the true-branch `X` against the
/// occurrence already present in the false-branch `Multi` — doing so drops the
/// `outer` dispatch arm and reroutes the `outer == true` path through the
/// false-branch's inner guards instead of unconditionally applying `X`. The
/// fixture pins `outer == true` (`a` is |1>) and the false-branch inner guard
/// `rb == One` false (`b` stays |0>), so the dropped arm is exactly the path
/// taken: the original applies `X(q)` (measuring `One`) while the buggy rewrite
/// falls through to the false-branch default `Z(q)` (measuring `Zero`),
/// diverging in both return value and effect trace. The guards are pure reads
/// of pre-measured `Result` locals so the fixture isolates the lattice merge
/// from condition-hoisting concerns.
#[test]
fn single_multi_shared_callable_across_branches_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                use b = Qubit();
                // a is |1> so the outer guard is true — op must be the
                // true-branch `X`; b stays |0> so the false-branch inner guard
                // is false, the arm the identity-dedup would route through.
                X(a);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let op = if ra == One {
                             X
                         } else {
                             if rb == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Regression for the `Multi ⊔ Multi` join's "unmodified variable" fast path: a
/// callable-valued local is selected by an outer dynamic `if` whose *both*
/// branches are dynamic conditionals that yield the *same set of callables*
/// (`X`/`Z`) but under *different* inner guards (`rb` in the true branch, `rc`
/// in the false branch).
///
/// The merge must not treat the two branches as an unmodified variable just
/// because the callable identities coincide — the guards differ, so keeping the
/// true-branch chain drops the outer condition and reroutes the `outer == false`
/// path through the true branch's `rb` guard instead of the false branch's `rc`
/// guard. The fixture pins `outer == false` (`a` stays |0>), `rb == One`
/// (`b` is |1>), and `rc == Zero` (`c` stays |0>): the original applies `Z(q)`
/// (measuring `Zero`) while the buggy rewrite applies `X(q)` (measuring `One`),
/// diverging in both return value and effect trace.
#[test]
fn multi_multi_same_callables_different_guards_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                use b = Qubit();
                use c = Qubit();
                // a stays |0> (outer guard false); b is |1> (rb == One);
                // c stays |0> (rc == Zero).
                X(b);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let rc = MResetZ(c);
                let op = if ra == One {
                             if rb == One { X } else { Z }
                         } else {
                             if rc == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Probe: a conditional callable is bound from a guard variable that is then
/// mutated before the callable is applied. The original captures the callable
/// value at binding time (guard true -> `X`); a defunctionalization that
/// re-evaluates the guard at the apply site would read the mutated guard
/// (now false -> `Z`) and diverge.
///
/// The safe-degradation regression asserting the pipeline rejects this rather
/// than silently miscompiling lives in
/// `defunctionalize::tests::guard_var_reassigned_after_binding_degrades_to_dynamic`.
#[test]
fn guard_var_never_reassigned_after_binding_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                X(a);
                let ra = MResetZ(a);
                // `flag` is mutable but never reassigned after the binding, so
                // hoisting its read to the apply site is safe and dispatch is
                // preserved.
                mutable flag = ra == One;
                let op = if flag { X } else { Z };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj + Ctl, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable angle = 0.0;
                let op = Rx(angle + 0.0, _);
                set angle = 3.141592653589793;
                ApplyOp(op, target);
                MResetZ(target)
            }
        }
    "#});
}

/// Regression for the effectful-producer decline gate's *accept* side. The
/// producer is a pure `function`, so its call is deletable and the closure it
/// returns is still consumed; the gate must let that through unchanged. The
/// captured angle is observable in the final state, so an evaluation the
/// rewrite dropped, duplicated, or reordered while relocating the capture would
/// diverge.
///
/// The decline side cannot have an equivalence test: a declined shape reports a
/// fatal `DynamicCallable`, so there is no transformed program to compare. It is
/// pinned by
/// `defunctionalize::tests::invariants::effectful_producer_returning_consumed_closure_declines_to_dynamic`.
#[test]
fn pure_producer_returned_closure_consumption_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function MakeRot(angle : Double) : Qubit => Unit is Adj + Ctl {
                Rx(angle, _)
            }
            operation ApplyOp(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let op = MakeRot(3.141592653589793);
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

#[test]
fn capture_admissibility_producer_nested_mutable_snapshot() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function MakeRot(angle : Double) : Qubit => Unit is Adj + Ctl {
                Rx(angle, _)
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable angle = 0.0;
                let op = MakeRot(angle + 0.0);
                set angle = 3.141592653589793;
                op(target);
                MResetZ(target)
            }
        }
    "#});
}

#[test]
fn capture_admissibility_direct_mutable_snapshot() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable angle = 0.0;
                let op = Rx(angle + 0.0, _);
                set angle = 3.141592653589793;
                op(target);
                MResetZ(target)
            }
        }
    "#});
}

#[test]
fn capture_admissibility_loop_mutable_snapshot() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable angle = 0.0;
                let op = Rx(angle + 0.0, _);
                for _ in 0..0 {
                    set angle = 3.141592653589793;
                }
                op(target);
                MResetZ(target)
            }
        }
    "#});
}

mod aggregate_forwarding {
    #[test]
    fn nested_udt_projection_preserves_nominal_fields_and_captures() {
        let source = indoc::indoc! {r#"
            namespace Test {
                struct Payload { Stored : Int, Action : Int -> Int }
                struct Packet { Enabled : Bool, Payload : Payload, Tail : Int }
                function Make(offset : Int) : Int -> Int { value -> value + offset }
                function Project(packet : Packet) : Payload { packet.Payload }
                function Forward(payload : Payload) : Payload { payload }
                function Consume(payload : Payload) : Int {
                    payload.Stored * 100 + payload.Action(2)
                }
                function Relay(packet : Packet) : Int {
                    if packet.Enabled { Consume(Forward(Project(packet))) + packet.Tail } else { 0 }
                }
                @EntryPoint()
                operation Main() : Int {
                    Relay(new Packet {
                        Tail = 7,
                        Payload = new Payload { Action = Make(3), Stored = 14 },
                        Enabled = true
                    })
                }
            }
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(1412)));
        crate::test_utils::check_semantic_equivalence(source);
    }

    #[test]
    fn udt_forwarding_preserves_reordered_fields_and_mutable_snapshots() {
        let source = indoc::indoc! {r#"
            namespace Test {
                struct Payload { Stored : Int, Action : Int -> Int }
                function Make(offset : Int) : Int -> Int { value -> value + offset }
                function Forward(payload : Payload) : Payload { payload }
                function Consume(payload : Payload) : Int {
                    payload.Stored * 100 + payload.Action(2)
                }
                @EntryPoint()
                operation Main() : Int {
                    mutable order = 0;
                    mutable whole = new Payload {
                        Action = { set order = order * 10 + 1; Make(3) },
                        Stored = { set order = order * 10 + 2; 14 }
                    };
                    let saved = Forward(whole);
                    set whole = new Payload { ...whole, Stored = 9, Action = Make(17) };
                    let changed = Forward(whole);
                    set whole = new Payload { Stored = 3, Action = Make(41) };
                    order * 1000000 + Consume(saved) * 1000 + Consume(changed)
                }
            }
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(13_405_919)));
        crate::test_utils::check_semantic_equivalence(source);
    }

    #[test]
    fn tuple_backed_newtype_forwarding_preserves_captures() {
        let source = indoc::indoc! {r#"
            namespace Test {
                newtype Payload = (Stored : Int, Action : Int -> Int);
                function Make(offset : Int) : Int -> Int { value -> value + offset }
                function Forward(payload : Payload) : Payload { payload }
                function Consume(payload : Payload) : Int {
                    payload::Stored * 100 + payload::Action(2)
                }
                @EntryPoint()
                operation Main() : Int { Consume(Forward(Forward(Payload(14, Make(3))))) }
            }
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(1405)));
        crate::test_utils::check_semantic_equivalence(source);
    }

    #[test]
    fn captured_nested_alias_restores_both_callables_repeatedly() {
        super::check_tuple_assignment_result(
            indoc::indoc! {r#"
                mutable (value, (first, second)) = (14, (Add11, Times3));
                let saved = (value, (first, second));
                let observe = input -> {
                    let (stored, (left, right)) = saved;
                    stored * 10000 + left(input) * 100 + right(input)
                };
                mutable total = 0;
                for iteration in 1..3 {
                    set (value, (first, second)) = (iteration, (Minus5, Add11));
                    set (value, (first, second)) = saved;
                    set total += observe(2) + value * 10000 + first(2) * 100 + second(2);
                }
                total
            "#},
            847_836,
        );
    }

    #[test]
    fn whole_tuple_forwarding_preserves_callable_and_payload() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Add11(value : Int) : Int { value + 11 }
                function Forward(pair : (Int, Int -> Int)) : (Int, Int -> Int) { pair }
                function Consume(pair : (Int, Int -> Int)) : Int {
                    let (stored, action) = pair;
                    stored * 100 + action(2)
                }
                function Relay(pair : (Int, Int -> Int)) : Int {
                    Consume(Forward(pair))
                }
                @EntryPoint()
                operation Main() : Int { Relay((14, Add11)) }
            }
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(1413)));
        crate::test_utils::check_semantic_equivalence(source);
    }

    #[test]
    fn whole_udt_forwarding_preserves_callable_and_payload() {
        let source = indoc::indoc! {r#"
            namespace Test {
                struct Payload { Stored : Int, Action : Int -> Int }
                function Add11(value : Int) : Int { value + 11 }
                function Forward(payload : Payload) : Payload { payload }
                function Consume(payload : Payload) : Int {
                    payload.Stored * 100 + payload.Action(2)
                }
                function Relay(payload : Payload) : Int { Consume(Forward(payload)) }
                @EntryPoint()
                operation Main() : Int {
                    Relay(new Payload { Stored = 14, Action = Add11 })
                }
            }
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(1413)));
        crate::test_utils::check_semantic_equivalence(source);
    }

    #[test]
    fn successive_tuple_callable_fields_preserve_order_and_captures() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Make(offset : Int) : Int -> Int { value -> value + offset }
                function Forward(bundle : (Int -> Int, Int, Int -> Int, Int -> Int, Int))
                    : (Int -> Int, Int, Int -> Int, Int -> Int, Int) { bundle }
                function Consume(bundle : (Int -> Int, Int, Int -> Int, Int -> Int, Int)) : Int {
                    let (first, marker, second, third, tail) = bundle;
                    marker * 1000000 + first(2) * 10000 + second(2) * 100 + third(2) * 10 + tail
                }
                function Relay(bundle : (Int -> Int, Int, Int -> Int, Int -> Int, Int)) : Int {
                    Consume(Forward(Forward(bundle)))
                }
                @EntryPoint()
                operation Main() : Int {
                    Relay((Make(3), 7, Make(17), Make(41), 9))
                }
            }
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(7_052_339)));
        crate::test_utils::check_semantic_equivalence(source);
    }

    #[test]
    fn successive_udt_callable_fields_preserve_order_and_captures() {
        let source = indoc::indoc! {r#"
            namespace Test {
                struct Bundle {
                    First : Int -> Int,
                    Marker : Int,
                    Second : Int -> Int,
                    Third : Int -> Int,
                    Tail : Int
                }
                function Make(offset : Int) : Int -> Int { value -> value + offset }
                function Forward(bundle : Bundle) : Bundle { bundle }
                function Consume(bundle : Bundle) : Int {
                    bundle.Marker * 1000000 + bundle.First(2) * 10000
                        + bundle.Second(2) * 100 + bundle.Third(2) * 10 + bundle.Tail
                }
                function Relay(bundle : Bundle) : Int { Consume(Forward(Forward(bundle))) }
                @EntryPoint()
                operation Main() : Int {
                    Relay(new Bundle {
                        First = Make(3), Marker = 7, Second = Make(17), Third = Make(41), Tail = 9
                    })
                }
            }
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(7_052_339)));
        crate::test_utils::check_semantic_equivalence(source);
    }

    #[test]
    fn partial_nested_tuple_projection_preserves_ancestor_shape() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Add11(value : Int) : Int { value + 11 }
                function Times3(value : Int) : Int { value * 3 }
                function Project(packet : ((Int, (Int -> Int, Int -> Int)), Bool, Int))
                    : (Int, (Int -> Int, Int -> Int)) {
                    let (head, _, _) = packet;
                    head
                }
                function Consume(head : (Int, (Int -> Int, Int -> Int))) : Int {
                    let (stored, (first, second)) = head;
                    stored * 10000 + first(2) * 100 + second(2)
                }
                function Relay(packet : ((Int, (Int -> Int, Int -> Int)), Bool, Int)) : Int {
                    let (_, enabled, tail) = packet;
                    if enabled { Consume(Project(packet)) + tail } else { 0 }
                }
                @EntryPoint()
                operation Main() : Int { Relay(((14, (Add11, Times3)), true, 7)) }
            }
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(141_313)));
        crate::test_utils::check_semantic_equivalence(source);
    }

    #[test]
    fn whole_tuple_mutation_return_preserves_latest_value() {
        let source = indoc::indoc! {r#"
            namespace Test {
                function Add11(value : Int) : Int { value + 11 }
                function Times3(value : Int) : Int { value * 3 }
                function Replace(pair : (Int, Int -> Int), replacement : (Int, Int -> Int))
                    : (Int, Int -> Int) {
                    mutable whole = pair;
                    set whole = replacement;
                    whole
                }
                function Consume(pair : (Int, Int -> Int)) : Int {
                    let (stored, action) = pair;
                    stored * 100 + action(2)
                }
                @EntryPoint()
                operation Main() : Int {
                    let saved = (14, Add11);
                    let changed = Replace(saved, (9, Times3));
                    Consume(saved) * 10000 + Consume(changed)
                }
            }
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(14_130_906)));
        crate::test_utils::check_semantic_equivalence(source);
    }

    #[test]
    fn whole_udt_mutation_return_preserves_latest_value() {
        let source = indoc::indoc! {r#"
            namespace Test {
                struct Payload { Stored : Int, Action : Int -> Int }
                function Add11(value : Int) : Int { value + 11 }
                function Times3(value : Int) : Int { value * 3 }
                function Replace(payload : Payload, replacement : Payload) : Payload {
                    mutable whole = payload;
                    set whole = replacement;
                    whole
                }
                function Consume(payload : Payload) : Int {
                    payload.Stored * 100 + payload.Action(2)
                }
                @EntryPoint()
                operation Main() : Int {
                    let saved = new Payload { Stored = 14, Action = Add11 };
                    let changed = Replace(saved, new Payload { Stored = 9, Action = Times3 });
                    Consume(saved) * 10000 + Consume(changed)
                }
            }
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(14_130_906)));
        crate::test_utils::check_semantic_equivalence(source);
    }

    #[test]
    fn mutable_tuple_alias_assignment_preserves_latest_snapshot() {
        super::check_tuple_assignment_result(
            indoc::indoc! {r#"
                mutable (value, callable) = (14, Add11);
                mutable saved = (value, callable);
                set (value, callable) = (9, Times3);
                set saved = (value, callable);
                set (value, callable) = (3, Minus5);
                set (value, callable) = saved;
                value * 100 + callable(2)
            "#},
            906,
        );
    }
}
