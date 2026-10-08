// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Many tests pair a primary assertion with a `check_rewrite` before/after
// snapshot, so the generated Q# pushes function bodies past the line limit.
#![allow(clippy::too_many_lines)]

use crate::{
    defunctionalize::analysis::{LocalState, resolve_captures},
    package_assigners::PackageAssigners,
};

use super::*;
use expect_test::expect;
use qsc_data_structures::index_map::IndexMap;
use qsc_fir::fir::{LocalVarId, Package, PatKind};
use qsc_fir::ty::{Prim, Ty};
use rustc_hash::FxHashSet;

fn check_flow_value(source: &str, expected: i64) {
    // Residual struct values need erasure before their exec graphs can be rebuilt.
    let (mut store, package) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::UdtErase);
    crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package, &[]);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, package),
        Ok(qsc_eval::val::Value::Int(expected)),
        "defunctionalization must preserve the value:\n{source}"
    );
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(expected),
    );
}

#[test]
fn unresolved_factory_captures_defer_whole_dispatch_without_losing_known_candidates() {
    // A mutable capture can be unresolved before factory specialization. The
    // whole dispatch must then stay dynamic, whichever producer branch holds it.
    // The immutable-capture control must retain both statically known choices.
    for (binding, body, unresolved) in [
        ("mutable", "x -> f(f(x))", true),
        ("mutable", "if true { x -> f(f(x)) } else { Inc }", true),
        ("mutable", "if false { Inc } else { x -> f(f(x)) }", true),
        ("let", "x -> f(f(x))", false),
    ] {
        let source = format!(
            r#"
            function Inc(x : Int) : Int {{ x + 1 }}
            function Dbl(x : Int) : Int {{ 2 * x }}
            function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
            function Twice(f : Int -> Int) : Int -> Int {{ {body} }}
            @EntryPoint() operation Main() : Int {{
                mutable f = Inc;
                {binding} g = Dbl;
                if true {{ set f = Twice(g); }}
                Apply(f, 3)
            }}
            "#
        );
        let (mut store, package) = compile_to_monomorphized_fir(&source);
        let apply = crate::test_utils::callable_id_by_name(store.get(package), "Apply");
        let result = super::run_prepass_and_analysis(&mut store, package);
        let sites: Vec<_> = result
            .call_sites
            .iter()
            .filter(|site| site.hof_item_id.package == package && site.hof_item_id.item == apply)
            .collect();
        if unresolved {
            assert_eq!(
                sites.len(),
                1,
                "an unresolved alternative must defer the entire dispatch"
            );
            assert!(matches!(sites[0].callable_arg, ConcreteCallable::Dynamic));
        } else {
            assert_eq!(
                sites.len(),
                2,
                "both the factory closure and original Inc must remain known"
            );
            assert!(
                sites
                    .iter()
                    .all(|site| !matches!(site.callable_arg, ConcreteCallable::Dynamic))
            );
        }
        // Full lowering also checks that producer-owned guards were copied,
        // rather than sharing ExprIds with the generated dispatch.
        check_flow_value(&source, 12);
    }
}

#[test]
fn flow_indexed_fields_keep_selection_before_later_initializer_writes() {
    for (index, expected) in [(0, 1), (-1, 2)] {
        for replacement in [1, 2] {
            for selection in ["[A, B][index]", "alias"] {
                for invocation in ["saved.F(saved.Tag)", "Apply(saved.F, saved.Tag)"] {
                    let source = format!(
                        r#"
                        struct Holder {{ F : Int -> Int, Tag : Int }}
                        function A(x : Int) : Int {{ Message("A"); x + 1 }}
                        function B(x : Int) : Int {{ Message("B"); x + 2 }}
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        @EntryPoint() operation Main() : Int {{
                            mutable index = {index};
                            let selected = [A, B][index];
                            let alias = selected;
                            let original = new Holder {{
                                F = {selection},
                                Tag = {{
                                    Message("store");
                                    set index = {replacement};
                                    0
                                }}
                            }};
                            let saved = original;
                            Message("invoke");
                            {invocation}
                        }}
                        "#
                    );
                    let (mut store, package) = compile_to_monomorphized_fir(&source);
                    let result = super::run_prepass_and_analysis(&mut store, package);
                    if invocation.starts_with("Apply") {
                        assert_eq!(result.call_sites.len(), 1);
                        assert!(
                            result.call_sites.iter().all(|site| {
                                matches!(site.callable_arg, ConcreteCallable::Dynamic)
                            }),
                            "aggregate fallback must not recover an invalidated selector"
                        );
                    } else {
                        assert!(
                            result.direct_call_sites.is_empty(),
                            "a stale index must not produce concrete dispatch"
                        );
                        assert_eq!(result.unresolved_direct_call_sites.len(), 1);
                    }
                    check_flow_value(&source, expected);
                }
            }
        }
    }
}

#[test]
fn flow_indexed_tuple_rows_preserve_all_physical_positions() {
    for (rows, names, values) in [
        (
            "[(A, 0), (A, 0), (B, 0)]",
            ["A:Body", "A:Body", "B:Body"],
            [1, 1, 2],
        ),
        (
            "[(A, 0), (B, 0), (A, 0)]",
            ["A:Body", "B:Body", "A:Body"],
            [1, 2, 1],
        ),
        (
            "[(A, 0), (A, 0), (A, 0)]",
            ["A:Body", "A:Body", "A:Body"],
            [1, 1, 1],
        ),
    ] {
        for index in -3_i64..3 {
            for selector in [index.to_string(), "index".to_string()] {
                let source = format!(
                    r#"
                    function A(x : Int) : Int {{ Message("A"); x + 1 }}
                    function B(x : Int) : Int {{ Message("B"); x + 2 }}
                    @EntryPoint() operation Main() : Int {{
                        mutable index = {index};
                        let rows = {rows};
                        let alias = rows;
                        let (f, _) = alias[{selector}];
                        f(0)
                    }}
                    "#
                );
                let (mut store, package) = compile_to_monomorphized_fir(&source);
                let result = super::run_prepass_and_analysis(&mut store, package);
                let candidates: Vec<_> = result
                    .direct_call_sites
                    .iter()
                    .map(|site| format_concrete_callable(&site.callable, &store))
                    .collect();
                assert_eq!(candidates, names, "every physical row needs a candidate");
                let position = usize::try_from(index.rem_euclid(3)).expect("normalized index");
                check_flow_value(&source, values[position]);
            }
        }
    }
}

#[test]
fn flow_joined_indexed_singleton_preserves_false_alternatives() {
    let source = r#"
        function A(x : Int) : Int { x + 1 }
        function B(x : Int) : Int { x + 2 }
        function C(x : Int) : Int { x + 3 }
        function Pick(flag : Bool, index : Int) : Int {
            let (left, _) = [(A, 0)][0];
            let selected = if flag { left } else { [B, C][index] };
            selected(0)
        }
        @EntryPoint() operation Main() : Int { Pick(false, 0) }
    "#;
    let (mut store, package) = compile_to_monomorphized_fir(source);
    let result = super::run_prepass_and_analysis(&mut store, package);
    let candidates: Vec<_> = result
        .direct_call_sites
        .iter()
        .map(|site| {
            (
                format_concrete_callable(&site.callable, &store),
                site.condition.len(),
            )
        })
        .collect();
    // A singleton positional Multi must not hide the false branch's missing
    // index discriminator by turning B and C into one trailing default.
    assert_eq!(
        candidates,
        [("A:Body", 1), ("B:Body", 0), ("C:Body", 0)]
            .map(|(name, guards)| (name.to_string(), guards)),
    );
    let (mut store, package) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
    crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package, &[]);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, package),
        Ok(qsc_eval::val::Value::Int(2)),
        "ambiguous indexed alternatives must not become a guarded direct call",
    );
    check_flow_value(source, 2);
}

#[test]
fn flow_joined_indexed_branches_preserve_all_positions() {
    for (left_rows, left_values, right_rows, right_values) in [
        (
            "[(A, 0)]",
            &[1_i64][..],
            "[(B, 0), (C, 0), (B, 0)]",
            &[2, 3, 2][..],
        ),
        (
            "[(A, 0), (B, 0)]",
            &[1, 2][..],
            "[(C, 0), (A, 0), (C, 0)]",
            &[3, 1, 3][..],
        ),
    ] {
        for (left_rows, left_values, right_rows, right_values) in [
            (left_rows, left_values, right_rows, right_values),
            (right_rows, right_values, left_rows, left_values),
        ] {
            for (flag, values) in [(true, left_values), (false, right_values)] {
                let length = i64::try_from(values.len()).expect("small test array");
                for index in -length..length {
                    let position =
                        usize::try_from(index.rem_euclid(length)).expect("normalized index");
                    for binding in [
                        "let selected = if flag { left } else { right };",
                        "mutable selected = left;
                         if flag { set selected = left; } else { set selected = right; }",
                    ] {
                        for invocation in ["selected(0)", "Apply(selected, 0)"] {
                            let source = format!(
                                r#"
                                function A(x : Int) : Int {{ x + 1 }}
                                function B(x : Int) : Int {{ x + 2 }}
                                function C(x : Int) : Int {{ x + 3 }}
                                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                                function Pick(flag : Bool, first : Int, second : Int) : Int {{
                                    let (left, _) = {left_rows}[first];
                                    let (right, _) = {right_rows}[second];
                                    {binding}
                                    {invocation}
                                }}
                                @EntryPoint() operation Main() : Int {{
                                    Pick({flag}, {first}, {second})
                                }}
                                "#,
                                first = if flag { index } else { 0 },
                                second = if flag { 0 } else { index },
                            );
                            check_flow_value(&source, values[position]);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn flow_joined_indexed_branches_preserve_effect_order() {
    for (flag, values, order) in [
        (true, &[1_i64, 2][..], 12349),
        (false, &[2, 3, 2][..], 156_789),
    ] {
        let length = i64::try_from(values.len()).expect("small test array");
        for index in -length..length {
            let source = format!(
                r#"
                function A(x : Int) : Int {{ Message("A"); x + 1 }}
                function B(x : Int) : Int {{ Message("B"); x + 2 }}
                function C(x : Int) : Int {{ Message("C"); x + 3 }}
                function Pick(flag : Bool, index : Int) : Int {{
                    mutable order = 0;
                    let selected = if {{ set order = 10 * order + 1; flag }} {{
                        let (f, _) = [
                            (A, {{ set order = 10 * order + 2; 0 }}),
                            (B, {{ set order = 10 * order + 3; 0 }})
                        ][{{ set order = 10 * order + 4; index }}];
                        f
                    }} else {{
                        let (f, _) = [
                            (B, {{ set order = 10 * order + 5; 0 }}),
                            (C, {{ set order = 10 * order + 6; 0 }}),
                            (B, {{ set order = 10 * order + 7; 0 }})
                        ][{{ set order = 10 * order + 8; index }}];
                        f
                    }};
                    let result = selected({{ set order = 10 * order + 9; 0 }});
                    100 * order + result
                }}
                @EntryPoint() operation Main() : Int {{ Pick({flag}, {index}) }}
                "#
            );
            let position = usize::try_from(index.rem_euclid(length)).expect("normalized index");
            check_flow_value(&source, 100 * order + values[position]);
        }
    }
}

#[test]
fn flow_joined_indexed_branches_preserve_bounds_before_argument_failure() {
    for (flag, length) in [(true, 2), (false, 3)] {
        for index in [-length - 1, length] {
            let source = format!(
                r#"
                function A(x : Int) : Int {{ x + 1 }}
                function B(x : Int) : Int {{ x + 2 }}
                function C(x : Int) : Int {{ x + 3 }}
                function Mark(label : String) : Int {{ Message(label); 0 }}
                function Pick(flag : Bool, index : Int) : Int {{
                    let selected = if {{ Message("guard"); flag }} {{
                        let (f, _) = [
                            (A, Mark("true-0")), (B, Mark("true-1"))
                        ][{{ Message("true-index"); index }}];
                        f
                    }} else {{
                        let (f, _) = [
                            (B, Mark("false-0")), (C, Mark("false-1")), (B, Mark("false-2"))
                        ][{{ Message("false-index"); index }}];
                        f
                    }};
                    selected({{ Message("argument"); fail "argument ran before bounds" }})
                }}
                @EntryPoint() operation Main() : Int {{ Pick({flag}, {index}) }}
                "#
            );
            let error = crate::test_utils::eval_qsharp_original(&source)
                .expect_err("the selected branch's index must fail first");
            assert!(error.starts_with("IndexOutOfRange("), "{error}");
            crate::test_utils::check_semantic_equivalence(&source);
        }
    }
}

#[test]
fn flow_indexed_tuple_aliases_do_not_replay_later_selector_writes() {
    for (index, expected) in [(0, 1), (1, 2), (-1, 1)] {
        for invocation in ["alias(0)", "Apply(alias, 0)"] {
            check_flow_value(
                &format!(
                    r#"
                    function A(x : Int) : Int {{ x + 1 }}
                    function B(x : Int) : Int {{ x + 2 }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    @EntryPoint() operation Main() : Int {{
                        mutable index = {index};
                        let (selected, _) = [(A, 0), (B, 0), (A, 0)][index];
                        let alias = selected;
                        set index = 3;
                        {invocation}
                    }}
                    "#
                ),
                expected,
            );
        }
    }
}

#[test]
fn flow_indexed_tuple_rows_preserve_bounds_failures() {
    for index in [-4, 3] {
        for selector in [index.to_string(), "index".to_string()] {
            let source = format!(
                r#"
                function A(x : Int) : Int {{ Message("A"); x + 1 }}
                function B(x : Int) : Int {{ Message("B"); x + 2 }}
                @EntryPoint() operation Main() : Int {{
                    mutable index = {index};
                    let (f, _) = [(A, 0), (A, 0), (B, 0)][{selector}];
                    f(0)
                }}
                "#
            );
            assert!(
                crate::test_utils::eval_qsharp_original(&source).is_err(),
                "the original tuple-array access must fail its bounds check"
            );
            crate::test_utils::check_semantic_equivalence(&source);
        }
    }
}

#[test]
fn flow_indexed_tuple_branch_sources_remain_invalidatable() {
    for (flag, expected) in [(true, 1), (false, 2)] {
        check_flow_value(
            &format!(
                r#"
                function A(x : Int) : Int {{ x + 1 }}
                function B(x : Int) : Int {{ x + 2 }}
                @EntryPoint() operation Main() : Int {{
                    mutable first = 0;
                    mutable second = 1;
                    mutable selected = A;
                    if {flag} {{
                        let (f, _) = [(A, 0), (B, 0)][first];
                        set selected = f;
                    }} else {{
                        let (f, _) = [(A, 0), (B, 0)][second];
                        set selected = f;
                    }}
                    let alias = selected;
                    set first = 1;
                    set second = 0;
                    alias(0)
                }}
                "#
            ),
            expected,
        );
    }
}

#[test]
fn flow_indexed_tuple_dispatch_preserves_defunc_call_abi() {
    let source = r#"
        function A(pair : (Int, Int)) : Int {
            let (x, y) = pair;
            x + y + 1
        }
        function B(pair : (Int, Int)) : Int {
            let (x, y) = pair;
            x + y + 2
        }
        @EntryPoint() operation Main() : Int {
            let (f, _) = [(A, 0), (A, 0), (B, 0)][1];
            f((2, 3))
        }
    "#;
    check_invariants(source);
    let (mut store, package) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
    crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package, &[]);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, package),
        Ok(qsc_eval::val::Value::Int(6)),
        "dispatch must retain the declared tuple input immediately after Defunc"
    );
    check_flow_value(source, 6);
}

#[test]
fn flow_effectful_callable_argument_guards_are_retained() {
    // Equal targets may collapse to Single; distinct targets may form Multi.
    // Neither permits deleting the guard or moving it past the data operand.
    for flag in [false, true] {
        for other in ["A", "B"] {
            for selection in [
                format!("if Guard() {{ A }} else {{ {other} }}"),
                format!("{{ if Guard() {{ A }} else {{ {other} }} }}"),
                format!("if true {{ if Guard() {{ A }} else {{ {other} }} }} else {{ B }}"),
            ] {
                let source = format!(
                    r#"
                    function A(x : Int) : Int {{ x + 1 }}
                    function B(x : Int) : Int {{ x + 2 }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    @EntryPoint() operation Main() : Int {{
                        mutable visits = 0;
                        let result = Apply({selection}, {{ set visits = 10 * visits + 2; 0 }});
                        100 * visits + result
                    }}
                    "#
                )
                .replace(
                    "Guard()",
                    &format!("{{ set visits = 10 * visits + 1; {flag} }}"),
                );
                check_flow_value(&source, if flag || other == "A" { 1201 } else { 1202 });
            }
        }
    }
}

#[test]
fn flow_callable_argument_guard_failures_are_not_erased() {
    for other in ["A", "B"] {
        let source = format!(
            r#"
            function A(x : Int) : Int {{ x + 1 }}
            function B(x : Int) : Int {{ x + 2 }}
            function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
            function Guard() : Bool {{ fail "guard must run" }}
            @EntryPoint() operation Main() : Int {{
                Apply(if Guard() {{ A }} else {{ {other} }}, {{ fail "argument ran first" }})
            }}
            "#
        );
        let error = crate::test_utils::eval_qsharp_original(&source)
            .expect_err("the guard must fail before the data argument");
        assert!(error.contains("guard must run"), "{error}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn flow_discardable_callable_argument_guards_still_specialize() {
    for flag in [false, true] {
        let source = format!(
            r#"
            function A(x : Int) : Int {{ x + 1 }}
            function B(x : Int) : Int {{ x + 2 }}
            function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
            @EntryPoint() operation Main() : Int {{
                Apply(if {flag} {{ A }} else {{ B }}, 0)
            }}
            "#
        );
        let (mut store, package) = compile_to_monomorphized_fir(&source);
        let result = super::run_prepass_and_analysis(&mut store, package);
        assert!(!result.call_sites.is_empty());
        assert!(
            result
                .call_sites
                .iter()
                .all(|site| !matches!(site.callable_arg, ConcreteCallable::Dynamic))
        );
        check_flow_value(&source, if flag { 1 } else { 2 });
    }
}

#[test]
fn flow_parallel_capture_limits_rebind_across_producer_environments() {
    // The body is deliberately local-free. A local in the body remains an
    // unsupported block capture; the limit is a reconstructible expression.
    for limit in ["2", "n", "n + 1"] {
        for producer in ["Make(2)", "Forward(2)", "Again(2)"] {
            let source = format!(
                r#"
                operation Make(n : Int) : Int -> Int {{
                    let value = parallel within {limit} {{ 7 }};
                    x -> value + x
                }}
                operation Forward(n : Int) : Int -> Int {{ Make(n + 1) }}
                operation Again(padding : Int) : Int -> Int {{ Forward(2 * padding) }}
                @EntryPoint() operation Main() : Int {{ {producer}(3) }}
                "#
            );
            check_flow_value(&source, 10);
        }
    }
}

#[test]
fn flow_unsupported_capture_forms_remain_dynamic() {
    // These local-bearing leaves cannot be reconstructed by the capture writer.
    // Also reject operation calls with no locals: absence of a scope leak is
    // not permission to move an operation's evaluation.
    for initializer in [
        "{ n }",
        "if true { n } else { 0 }",
        "parallel within 2 { n }",
        "Read()",
    ] {
        let source = format!(
            r#"
            operation Read() : Int {{ 7 }}
            operation Make(n : Int) : Int -> Int {{
                let value = {initializer};
                x -> value + x
            }}
            @EntryPoint() operation Main() : Int {{ Make(7)(3) }}
            "#
        );
        let (mut store, package) = compile_to_monomorphized_fir(&source);
        let result = super::run_prepass_and_analysis(&mut store, package);
        assert!(
            result
                .direct_call_sites
                .iter()
                .all(|site| matches!(site.callable, ConcreteCallable::Dynamic)),
            "unsupported producer capture must not yield a concrete call: {initializer}"
        );
        check_flow_value(&source, 10);
    }
}

#[test]
fn analysis_collapsed_spans_are_qualified_by_call_package() {
    use qsc_data_structures::span::Span;

    let (mut store, package_id) = crate::test_utils::compile_to_fir_with_library(
        r#"
        namespace Lib {
            function Inc(x : Int) : Int { x + 1 }
            function Run() : Int { let f = Inc; f(2) }
            export Run;
        }
        "#,
        r#"
        function Inc(x : Int) : Int { x + 1 }
        @EntryPoint() operation Main() : Int { let f = Inc; f(1) + Lib.Run() }
        "#,
    );
    let reachable = collect_reachable_from_entry(&store, package_id);
    let mut collapsed_spans = rustc_hash::FxHashMap::default();
    let local_span = Span { lo: 1000, hi: 1001 };
    let foreign_span = Span { lo: 2000, hi: 2001 };
    // Deliberately overlap numeric expression IDs while assigning each package
    // a distinct marker. Analyze without promotion so both local reads survive.
    for (owner, package) in &store {
        let span = if owner == package_id {
            local_span
        } else {
            foreign_span
        };
        for (expr_id, _) in &package.exprs {
            collapsed_spans.insert((owner, expr_id), span);
        }
    }
    let total_foreign = crate::walk_utils::collect_total_foreign_callables(&store);
    let result = super::super::analysis::analyze(
        &mut store,
        package_id,
        &reachable,
        &Default::default(),
        &collapsed_spans,
        &[],
        &total_foreign,
    );
    assert!(
        result
            .direct_call_sites
            .iter()
            .any(|site| site.call_pkg_id == package_id)
    );
    assert!(
        result
            .direct_call_sites
            .iter()
            .any(|site| site.call_pkg_id != package_id)
    );
    for site in &result.direct_call_sites {
        assert_eq!(
            site.def_span,
            Some(if site.call_pkg_id == package_id {
                local_span
            } else {
                foreign_span
            }),
            "call site metadata must come from its owning package"
        );
    }
}

#[test]
fn flow_guard_values_survive_later_initializer_writes() {
    for flag in [false, true] {
        for binding in ["let", "mutable"] {
            for (pattern, initializer, callable) in [
                (
                    "(saved, _)",
                    "(if flag { A } else { B }, { set flag = not flag; 0 })",
                    "saved",
                ),
                (
                    "(saved, _)",
                    "(selected, { set flag = not flag; 0 })",
                    "saved",
                ),
                (
                    "saved",
                    "new Holder { F = if flag { A } else { B }, Tag = { set flag = not flag; 0 } }",
                    "saved.F",
                ),
                (
                    "((saved, _), _)",
                    "(if flag { (A, 7) } else { (B, 7) }, { set flag = not flag; 0 })",
                    "saved",
                ),
            ] {
                for invocation in [format!("{callable}(0)"), format!("Apply({callable}, 0)")] {
                    check_flow_value(
                        &format!(
                            r#"
                            struct Holder {{ F : Int -> Int, Tag : Int }}
                            function A(x : Int) : Int {{ x + 1 }}
                            function B(x : Int) : Int {{ x + 2 }}
                            function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                            @EntryPoint() operation Main() : Int {{
                                mutable flag = {flag};
                                let selected = if flag {{ A }} else {{ B }};
                                {binding} {pattern} = {initializer};
                                {invocation}
                            }}
                            "#
                        ),
                        if flag { 1 } else { 2 },
                    );
                }
            }
        }
    }
}

#[test]
fn flow_guard_values_survive_simultaneous_stores() {
    for flag in [false, true] {
        for rhs in ["if flag { A } else { B }", "selected", "{ selected }"] {
            for invocation in ["saved(0)", "Apply(saved, 0)"] {
                check_flow_value(
                    &format!(
                        r#"
                        function A(x : Int) : Int {{ x + 1 }}
                        function B(x : Int) : Int {{ x + 2 }}
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        @EntryPoint() operation Main() : Int {{
                            mutable flag = {flag};
                            let selected = if flag {{ A }} else {{ B }};
                            mutable saved = A;
                            set (saved, flag) = ({rhs}, not flag);
                            {invocation}
                        }}
                        "#
                    ),
                    if flag { 1 } else { 2 },
                );
            }
        }
    }
}

#[test]
fn flow_unrelated_writes_keep_guarded_calls_resolvable() {
    let source = r#"
        function A(x : Int) : Int { x + 1 }
        function B(x : Int) : Int { x + 2 }
        @EntryPoint() operation Main() : Int {
            mutable flag = true;
            mutable unrelated = 0;
            let (saved, _) = (if flag { A } else { B }, { set unrelated = 1; 0 });
            saved(0)
        }
    "#;
    let (mut store, package) = compile_to_monomorphized_fir(source);
    let result = super::run_prepass_and_analysis(&mut store, package);
    assert_eq!(result.direct_call_sites.len(), 2);
    assert!(result.unresolved_direct_call_sites.is_empty());
    check_flow_value(source, 1);
}

#[test]
fn flow_guarded_factory_failure_is_preserved() {
    let source = r#"
        function A(x : Int) : Int { x + 1 }
        function B(x : Int) : Int { x + 2 }
        function Guard() : Bool { true }
        function Choose(flag : Bool) : Int -> Int {
            fail "producer must run";
            if flag { A } else { B }
        }
        @EntryPoint() operation Main() : Int { Choose(Guard())(0) }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source)
        .expect_err("the producer must fail before its returned callable is invoked");
    assert!(error.contains("producer must run"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn flow_guarded_factory_messages_run_once_in_order() {
    for flag in [false, true] {
        for (guard_prefix, producer_prefix) in [
            ("", "Message(\"producer\");"),
            ("Message(\"guard\");", ""),
            ("Message(\"guard\");", "Message(\"producer\");"),
        ] {
            for selection in [
                "Choose(Guard())(0)",
                "{ let saved = Choose(Guard()); saved(0) }",
                "{ let saved = Choose(Guard()); Apply(saved, 0) }",
            ] {
                check_flow_value(
                    &format!(
                        r#"
                        function A(x : Int) : Int {{ x + 1 }}
                        function B(x : Int) : Int {{ x + 2 }}
                        function Guard() : Bool {{
                            {guard_prefix}
                            {flag}
                        }}
                        function Choose(flag : Bool) : Int -> Int {{
                            {producer_prefix}
                            if flag {{ A }} else {{ B }}
                        }}
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        @EntryPoint() operation Main() : Int {{
                            Message("before");
                            let result = {selection};
                            Message("after");
                            result
                        }}
                        "#
                    ),
                    if flag { 1 } else { 2 },
                );
            }
        }
    }
}

#[test]
fn flow_static_index_negation_preserves_wrapping_and_bounds() {
    for (index, execute_index) in [("0x8000000000000000", false), ("2", true)] {
        let source = format!(
            r#"
            function A(x : Int) : Int {{ x + 1 }}
            @EntryPoint() operation Main() : Int {{
                let index = {index};
                if {execute_index} {{ [A][-index](0) }} else {{ 1 }}
            }}
            "#
        );
        if execute_index {
            assert!(
                crate::test_utils::eval_qsharp_original(&source).is_err(),
                "the executed negative index must retain its bounds failure"
            );
            crate::test_utils::check_semantic_equivalence(&source);
        } else {
            check_flow_value(&source, 1);
        }
    }
}

#[test]
fn flow_tuple_declarations_observe_ordered_initializer_values() {
    for binding in ["let", "mutable"] {
        for (pattern, initializer, result, expected) in [
            ("(tag, g)", "({ set f = B; 0 }, f)", "g(tag)", 2),
            ("(g, tag)", "(f, { set f = B; 0 })", "g(tag)", 1),
            (
                "((before, tag), after)",
                "((f, { set f = B; 0 }), f)",
                "10 * before(tag) + after(tag)",
                12,
            ),
        ] {
            check_flow_value(
                &format!(
                    r#"
                    function A(x : Int) : Int {{ x + 1 }}
                    function B(x : Int) : Int {{ x + 2 }}
                    @EntryPoint() operation Main() : Int {{
                        mutable f = A;
                        {binding} {pattern} = {initializer};
                        {result}
                    }}
                    "#
                ),
                expected,
            );
        }
    }
}

#[test]
fn flow_block_reanalysis_keeps_already_evaluated_callable_operands() {
    for call in [
        "Apply(if true { f } else { B }, { set f = B; 0 })",
        "Apply({ f }, { set f = B; 0 })",
        "Identity(f)({ set f = B; 0 })",
    ] {
        check_flow_value(
            &format!(
                r#"
                function A(x : Int) : Int {{ x + 1 }}
                function B(x : Int) : Int {{ x + 2 }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Identity(f : Int -> Int) : Int -> Int {{ f }}
                @EntryPoint() operation Main() : Int {{
                    mutable f = A;
                    {call}
                }}
                "#
            ),
            1,
        );
    }
}

#[test]
fn flow_producer_aggregates_keep_creation_time_callable_values() {
    for (initializer, result) in [
        ("[f]", "saved[0]"),
        ("(f, 7)", "{ let (selected, _) = saved; selected }"),
        ("new Holder { F = f }", "saved.F"),
    ] {
        check_flow_value(
            &format!(
                r#"
                struct Holder {{ F : Int -> Int }}
                function A(x : Int) : Int {{ x + 1 }}
                function B(x : Int) : Int {{ x + 2 }}
                function Make() : Int -> Int {{
                    mutable f = A;
                    let saved = {initializer};
                    set f = B;
                    {result}
                }}
                @EntryPoint() operation Main() : Int {{ Make()(0) }}
                "#
            ),
            1,
        );
    }
}

#[test]
fn flow_return_guards_use_caller_not_producer_local_bindings() {
    for (flag, expected) in [(true, 1), (false, 2)] {
        check_flow_value(
            &format!(
                r#"
                function A(x : Int) : Int {{ x + 1 }}
                function B(x : Int) : Int {{ x + 2 }}
                function Choose(flag : Bool) : Int -> Int {{
                    let unrelated = {opposite};
                    if flag {{ A }} else {{ B }}
                }}
                function Forward(flag : Bool) : Int -> Int {{ Choose(flag) }}
                @EntryPoint() operation Main() : Int {{
                    let padding = 0;
                    let flag = {flag};
                    Choose(flag)(padding) + 10 * Forward(flag)(padding)
                }}
                "#,
                opposite = !flag,
            ),
            11 * expected,
        );
    }
}

#[test]
fn flow_aggregate_parameters_resolve_in_the_callers_environment() {
    for body in [
        "Get(p)(0)",
        "Forward(p)(0)",
        "Select(fs, index)(0)",
        "Project(new Holder { F = A })(0)",
    ] {
        check_flow_value(
            &format!(
                r#"
                struct Holder {{ F : Int -> Int }}
                function A(x : Int) : Int {{ x + 1 }}
                function B(x : Int) : Int {{ x + 2 }}
                function Get(p : (Int -> Int, Int)) : Int -> Int {{
                    let (f, _) = p;
                    f
                }}
                function Forward(p : (Int -> Int, Int)) : Int -> Int {{ Get(p) }}
                function Select(fs : (Int -> Int)[], index : Int) : Int -> Int {{
                    let unrelated = [B];
                    fs[index]
                }}
                function Project(p : Holder) : Int -> Int {{ p.F }}
                @EntryPoint() operation Main() : Int {{
                    let p = (A, 0);
                    let fs = [A, B];
                    let index = 0;
                    {body}
                }}
                "#
            ),
            1,
        );
    }
}

#[test]
fn flow_recursive_factory_analysis_terminates_through_bindings() {
    for recursive_call in ["Make(n - 1)", "Forward(n - 1)"] {
        check_flow_value(
            &format!(
                r#"
                function A(x : Int) : Int {{ x + 1 }}
                function Make(n : Int) : Int -> Int {{
                    if n == 0 {{ A }} else {{
                        mutable next = {recursive_call};
                        next
                    }}
                }}
                function Forward(n : Int) : Int -> Int {{ Make(n) }}
                @EntryPoint() operation Main() : Int {{ Make(2)(0) }}
                "#
            ),
            1,
        );
    }
}

#[test]
fn flow_foreign_return_guards_never_escape_their_expression_package() {
    let source = "@EntryPoint() operation Main() : Int { Lib.Choose()(0) }";
    for (flag, expected) in [(true, 1), (false, 2)] {
        let library = format!(
            r#"
            namespace Lib {{
                function A(x : Int) : Int {{ x + 1 }}
                function B(x : Int) : Int {{ x + 2 }}
                function Choose() : Int -> Int {{ if {flag} {{ A }} else {{ B }} }}
                export Choose;
            }}
            "#
        );
        assert_eq!(
            crate::test_utils::eval_qsharp_original_with_library(&library, source),
            Ok(qsc_eval::val::Value::Int(expected))
        );
        let (mut store, package) = crate::test_utils::compile_and_run_pipeline_to_with_library(
            &library,
            source,
            crate::PipelineStage::Defunc,
        );
        crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package, &[]);
        assert_eq!(
            crate::test_utils::try_eval_fir_entry(&store, package),
            Ok(qsc_eval::val::Value::Int(expected))
        );
        crate::test_utils::check_semantic_equivalence_with_library(&library, source);
    }
}

#[test]
fn flow_indexed_aliases_invalidate_transitive_selector_provenance() {
    for aliases in [
        "let alias = saved;",
        "let middle = saved; let alias = middle;",
        "let middle = (saved, 7); let (alias, _) = middle;",
        "let middle = (saved, 7); let unrelated = 0; let (alias, _) = middle;",
    ] {
        for invocation in ["alias(3)", "Apply(alias, 3)"] {
            check_flow_value(
                &format!(
                    r#"
                    function Inc(x : Int) : Int {{ x + 1 }}
                    function Twice(x : Int) : Int {{ 2 * x }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    @EntryPoint() operation Main() : Int {{
                        let fs = [Inc, Twice];
                        mutable index = 0;
                        let saved = fs[index];
                        {aliases}
                        set index = 1;
                        {invocation}
                    }}
                    "#
                ),
                4,
            );
        }
    }
}

#[test]
fn flow_aggregate_guard_invalidation_blocks_initializer_replay() {
    for flag in [false, true] {
        for (initializer, invocation) in [
            ("new Holder { F = if flag { A } else { B } }", "saved.F(0)"),
            ("new Holder { F = selected }", "Apply(saved.F, 0)"),
            (
                "(if flag { A } else { B }, 7)",
                "{ let (f, _) = saved; f(0) }",
            ),
            ("[if flag { A } else { B }]", "saved[0](0)"),
        ] {
            check_flow_value(
                &format!(
                    r#"
                    struct Holder {{ F : Int -> Int }}
                    function A(x : Int) : Int {{ x + 1 }}
                    function B(x : Int) : Int {{ x + 2 }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    @EntryPoint() operation Main() : Int {{
                        mutable flag = {flag};
                        let selected = if flag {{ A }} else {{ B }};
                        let original = {initializer};
                        let saved = original;
                        set flag = not flag;
                        {invocation}
                    }}
                    "#
                ),
                if flag { 1 } else { 2 },
            );
        }
    }
}

#[test]
fn flow_return_guards_are_not_substituted_twice() {
    for flag in [false, true] {
        for other in [false, true] {
            for producer in ["Choose", "Forward"] {
                check_flow_value(
                    &format!(
                        r#"
                        function A(x : Int) : Int {{ x + 1 }}
                        function B(x : Int) : Int {{ x + 2 }}
                        function Choose(flag : Bool, other : Bool) : Int -> Int {{
                            if flag {{ A }} else {{ B }}
                        }}
                        function Forward(flag : Bool, other : Bool) : Int -> Int {{
                            Choose(flag, other)
                        }}
                        @EntryPoint() operation Main() : Int {{
                            let padding = 0;
                            mutable flag = {flag};
                            {producer}(flag, {other})(padding)
                        }}
                        "#
                    ),
                    if flag { 1 } else { 2 },
                );
            }
        }
    }
}

#[test]
fn flow_tuple_array_guards_preserve_row_selection() {
    for flag in [false, true] {
        for invocation in ["f(0)", "Apply(f, 0)"] {
            check_flow_value(
                &format!(
                    r#"
                    function A(x : Int) : Int {{ x + 1 }}
                    function B(x : Int) : Int {{ x + 2 }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    @EntryPoint() operation Main() : Int {{
                        mutable flag = {flag};
                        let rows = [(if flag {{ A }} else {{ B }}, 0), (A, 0)];
                        mutable total = 0;
                        for (f, _) in rows {{ set total += {invocation}; }}
                        total
                    }}
                    "#
                ),
                if flag { 2 } else { 3 },
            );
        }
    }
}

#[test]
fn flow_tuple_array_nonliteral_rows_keep_all_candidates() {
    for row in ["hidden", "{ hidden }", "Row()"] {
        for (index, expected) in [(0, 1), (1, 2), (-1, 2), (-2, 1)] {
            let source = format!(
                r#"
                function A(x : Int) : Int {{ x + 1 }}
                function B(x : Int) : Int {{ x + 2 }}
                function Row() : (Int -> Int, Int) {{ (B, 0) }}
                function Pick(index : Int) : Int -> Int {{
                    let hidden = (B, 0);
                    let rows = [(A, 0), {row}];
                    let (f, _) = rows[index];
                    f
                }}
                @EntryPoint() operation Main() : Int {{ Pick({index})(0) }}
                "#
            );
            let (mut store, package) = compile_to_monomorphized_fir(&source);
            let result = super::run_prepass_and_analysis(&mut store, package);
            let candidates = result
                .lattice_states
                .values()
                .flatten()
                .find_map(|(_, lattice)| match lattice {
                    CalleeLattice::Multi(entries) => Some(entries),
                    _ => None,
                })
                .expect("the producer must retain the complete row candidate union");
            let names: Vec<_> = candidates
                .iter()
                .map(|(callable, _)| format_concrete_callable(callable, &store))
                .collect();
            assert_eq!(names, ["A:Body", "B:Body"]);
            assert_eq!(
                result.unresolved_direct_call_sites.len(),
                1,
                "the producer's index cannot be replayed at the caller"
            );
            check_flow_value(&source, expected);
        }
    }
}

#[test]
fn flow_tuple_array_guards_without_index_provenance_remain_residual() {
    for flag in [false, true] {
        for index in [0, 1, -1, -2] {
            let source = format!(
                r#"
                function A(x : Int) : Int {{ x + 1 }}
                function B(x : Int) : Int {{ x + 2 }}
                function Run(flag : Bool) : Int {{
                    let rows = [(if flag {{ A }} else {{ B }}, 0), (A, 0)];
                    let (f, _) = rows[{index}];
                    f(0)
                }}
                @EntryPoint() operation Main() : Int {{ Run({flag}) }}
                "#
            );
            let first_row = index == 0 || index == -2;
            check_flow_value(&source, if first_row && !flag { 2 } else { 1 });
            let (mut store, package) = compile_to_monomorphized_fir(&source);
            let result = super::run_prepass_and_analysis(&mut store, package);
            assert!(result.direct_call_sites.is_empty());
            assert_eq!(
                result.unresolved_direct_call_sites.len(),
                1,
                "row guards alone must not replace array-index selection"
            );
        }
    }
}

#[test]
fn flow_tuple_array_unknown_rows_are_not_omitted() {
    let source = r#"
        function A(x : Int) : Int { x + 1 }
        function B(x : Int) : Int { x + 2 }
        function Run(row : (Int -> Int, Int)) : Int {
            let rows = [(A, 0), row];
            mutable total = 0;
            for (f, _) in rows { set total += f(0); }
            total
        }
        @EntryPoint() operation Main() : Int { Run((B, 0)) }
    "#;
    check_flow_value(source, 3);
    let (mut store, package) = compile_to_monomorphized_fir(source);
    let result = super::run_prepass_and_analysis(&mut store, package);
    assert_eq!(
        result.unresolved_direct_call_sites.len(),
        1,
        "the unseeded row parameter must make the loop callee unresolved"
    );
}

#[test]
fn flow_computed_callee_residue_is_scoped_to_its_owners() {
    let source = r#"
        function A(x : Int) : Int { x + 1 }
        function B(x : Int) : Int { x + 2 }
        function Plain(x : Int) : Int { x }
        function Pick(index : Int) : Int -> Int {
            let hidden = (B, 0);
            let rows = [(A, 0), hidden];
            let (f, _) = rows[index];
            f
        }
        @EntryPoint() operation Main() : Int { Plain(Pick(1)(0)) }
    "#;
    let (mut store, package) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&store, package);
    let outcome = super::defunctionalize(&mut store, package, &mut assigners);
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|error| matches!(error, crate::defunctionalize::Error::DynamicCallable(_)))
    );
    let owners: FxHashSet<_> = outcome
        .residue_items
        .iter()
        .map(|item| {
            let ItemKind::Callable(decl) = &store.get(item.package).get_item(item.item).kind else {
                panic!("residue owners must be callables");
            };
            decl.name.name.as_ref()
        })
        .collect();
    assert_eq!(owners, FxHashSet::from_iter(["Pick", "Main"]));
    assert!(!outcome.entry_has_residue);
    check_flow_value(source, 2);
}

#[test]
fn analysis_no_callable_params() {
    let source = "operation Main() : Unit { }";
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 0
            call_sites: 0"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {}
            // entry
            Main()

            AFTER:
            operation Main() : Unit {}
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_single_callable_param() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_multiple_callable_params() {
    let source = r#"
        operation ApplyTwo(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit {
            f(q);
            g(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyTwo(H, X, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 2
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
              param: callable_id=<item 3 in package 2>, path=[1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=ApplyTwo<AdjCtl, AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyTwo<AdjCtl, AdjCtl>, arg=Global(X, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyTwo(f : (Qubit => Unit), g : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyTwo_AdjCtl__AdjCtl_(H, X, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl_(f : (Qubit => Unit is Adj + Ctl), g : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyTwo(f : (Qubit => Unit), g : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyTwo_AdjCtl__AdjCtl__H__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl_(f : (Qubit => Unit is Adj + Ctl), g : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl__H__X_(q : Qubit) : Unit {
                H(q);
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_callable_param_in_tuple() {
    let source = r#"
        operation ApplySecond(q : Qubit, op : Qubit => Unit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplySecond(q, H);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplySecond<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplySecond(q : Qubit, op : (Qubit => Unit)) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplySecond_AdjCtl_(q, H);
                __quantum__rt__qubit_release(q);
            }
            operation ApplySecond_AdjCtl_(q : Qubit, op : (Qubit => Unit is Adj + Ctl)) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplySecond(q : Qubit, op : (Qubit => Unit)) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplySecond_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplySecond_AdjCtl_(q : Qubit, op : (Qubit => Unit is Adj + Ctl)) : Unit {
                op(q);
            }
            operation ApplySecond_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_global_callable_arg() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(X, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(X, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_closure_callable_arg() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_adjoint_callable_arg() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(Adjoint S, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(S, Adj)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(Adjoint S, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__Adj_S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__Adj_S_(q : Qubit) : Unit {
                Adjoint S(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_controlled_callable_arg() {
    let source = r#"
        operation ApplyOp(op : (Qubit[], Qubit) => Unit is Ctl, q : Qubit) : Unit {
            op([], q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(Controlled X, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(((Qubit)[], Qubit) => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Ctl)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), q : Qubit) : Unit {
                op([], q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(Controlled X, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : ((Qubit[], Qubit) => Unit is Adj + Ctl), q : Qubit) : Unit {
                op([], q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), q : Qubit) : Unit {
                op([], q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__Ctl_X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : ((Qubit[], Qubit) => Unit is Adj + Ctl), q : Qubit) : Unit {
                op([], q);
            }
            operation ApplyOp_AdjCtl__Ctl_X_(q : Qubit) : Unit {
                Controlled X([], q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_multiple_call_sites_same_hof() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
            ApplyOp(X, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(H, q);
                ApplyOp_AdjCtl_(X, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q);
                ApplyOp_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_single_assignment_local_traced() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myH = H;
            ApplyOp(myH, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
            lattice states:
              callable Main:
                2: Single(H:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let myH : (Qubit => Unit is Adj + Ctl) = H;
                ApplyOp_AdjCtl_(myH, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_dynamic_callable_detected() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            op = X;
            ApplyOp(op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)
            lattice states:
              callable Main:
                2: Single(X:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                op = X;
                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                op = X;
                ApplyOp_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn udt_field_single_level_direct() {
    let source = r#"
        struct Config { Apply : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Apply = H };
            ApplyOp(config.Apply, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Apply = H
                };
                ApplyOp_Empty_(config::Apply, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn udt_field_via_let_binding() {
    let source = r#"
        struct Config { Apply : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Apply = H };
            let f = config.Apply;
            ApplyOp(f, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)
            lattice states:
              callable Main:
                3: Single(H:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Apply = H
                };
                let f : (Qubit => Unit) = config::Apply;
                ApplyOp_Empty_(f, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn udt_field_in_hof_body() {
    let source = r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation RunWithConfig(config : Config, q : Qubit) : Unit {
            ApplyOp(config.Op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Op = H };
            RunWithConfig(config, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 2
              param: callable_id=<item 5 in package 2>, path=[0], ty=(Qubit => Unit)
              param: callable_id=<item 3 in package 2>, path=[0, 0], ty=(Qubit => Unit)
            call_sites: 2
              site: hof=RunWithConfig, arg=Global(H, Body)
              site: hof=ApplyOp<Empty>, arg=Dynamic"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Op = H
                };
                RunWithConfig(config, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunWithConfig_H_((), q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig_H_(config : Unit, q : Qubit) : Unit {
                ApplyOp_Empty__H_(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn udt_field_in_hof_body_defunctionalizes_end_to_end() {
    let source = r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation RunWithConfig(config : Config, q : Qubit) : Unit {
            ApplyOp(config.Op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Op = H };
            RunWithConfig(config, q);
        }
        "#;
    check(
        source,
        &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit
            RunWithConfig{H}: input_ty=(Unit, Qubit)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Op = H
                };
                RunWithConfig(config, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunWithConfig_H_((), q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig_H_(config : Unit, q : Qubit) : Unit {
                ApplyOp_Empty__H_(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn udt_field_in_hof_body_full_pipeline_invariants() {
    let source = r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation RunWithConfig(config : Config, q : Qubit) : Unit {
            ApplyOp(config.Op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let config = new Config { Op = H };
            RunWithConfig(config, q);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Op = H
                };
                RunWithConfig(config, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig(config : __UDT_Item_1__Package_2_, q : Qubit) : Unit {
                ApplyOp_Empty_(config::Op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunWithConfig_H_((), q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunWithConfig_H_(config : Unit, q : Qubit) : Unit {
                ApplyOp_Empty__H_(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn udt_field_nested_two_level() {
    let source = r#"
        struct InnerConfig { Apply : Qubit => Unit }
        struct OuterConfig { Inner : InnerConfig, Label : Int }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let outer = new OuterConfig {
                Inner = new InnerConfig { Apply = H },
                Label = 0,
            };
            ApplyOp(outer.Inner.Apply, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 5 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype InnerConfig = ((Qubit => Unit), );
            newtype OuterConfig = (__UDT_Item_1__Package_2_, Int);
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let outer : __UDT_Item_2__Package_2_ = new OuterConfig {
                    Inner = new InnerConfig {
                        Apply = H
                    },
                    Label = 0
                };
                ApplyOp_Empty_(outer::Inner::Apply, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            newtype InnerConfig = ((Qubit => Unit), );
            newtype OuterConfig = (__UDT_Item_1__Package_2_, Int);
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn udt_field_nested_two_level_defunctionalizes_end_to_end() {
    let source = r#"
        struct InnerConfig { Apply : Qubit => Unit }
        struct OuterConfig { Inner : InnerConfig, Label : Int }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let outer = new OuterConfig {
                Inner = new InnerConfig { Apply = H },
                Label = 0,
            };
            ApplyOp(outer.Inner.Apply, q);
        }
        "#;
    check(
        source,
        &expect![[r#"
            ApplyOp<Empty>{H}: input_ty=Qubit
            Main: input_ty=Unit"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype InnerConfig = ((Qubit => Unit), );
            newtype OuterConfig = (__UDT_Item_1__Package_2_, Int);
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let outer : __UDT_Item_2__Package_2_ = new OuterConfig {
                    Inner = new InnerConfig {
                        Apply = H
                    },
                    Label = 0
                };
                ApplyOp_Empty_(outer::Inner::Apply, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            newtype InnerConfig = ((Qubit => Unit), );
            newtype OuterConfig = (__UDT_Item_1__Package_2_, Int);
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn udt_field_closure_value() {
    let source = r#"
        struct Config { Op : Qubit => Unit }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            let config = new Config { Op = q1 => Rx(angle, q1) };
            ApplyOp(config.Op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 5 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=4, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle : Double = 1.;
                let config : __UDT_Item_1__Package_2_ = new Config {
                    Op = / * closure item = 4 captures = [angle] * / _lambda_4
                };
                ApplyOp_Empty_(config::Op, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_4(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            newtype Config = ((Qubit => Unit), );
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle : Double = 1.;
                ApplyOp_Empty__closure_(q, angle);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_4(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Double) : Unit {
                _lambda_4(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn udt_field_from_parameter() {
    let source = r#"
        struct Config { Op : Qubit => Unit }
        operation MakeConfig() : Config {
            new Config { Op = H }
        }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let c = MakeConfig();
            ApplyOp(c.Op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 5 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Config = ((Qubit => Unit), );
            operation MakeConfig() : __UDT_Item_1__Package_2_ {
                new Config {
                    Op = H
                }

            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let c : __UDT_Item_1__Package_2_ = MakeConfig();
                ApplyOp_Empty_(c::Op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            newtype Config = ((Qubit => Unit), );
            operation MakeConfig() : __UDT_Item_1__Package_2_ {
                new Config {
                    Op = H
                }

            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn constructor_and_factory_return_field_projection_resolve_distinctly() {
    let source = r#"
        newtype Config = (Count : Int, Apply : Qubit => Unit);

        function MakeConfig(angle : Double) : Config {
            Config(1, q => Rx(angle, q))
        }

        operation Main() : Unit {
            use q = Qubit();
            let direct = Config(0, H);
            direct::Apply(q);
            let factory = MakeConfig(0.25);
            factory::Apply(q);
        }
        "#;

    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(source);
    let result = super::run_prepass_and_analysis(&mut fir_store, fir_pkg_id);

    assert_eq!(
        result.direct_call_sites.len(),
        2,
        "both projected callable fields should be recorded"
    );
    assert!(
        result.direct_call_sites.iter().any(|site| {
            matches!(
                &site.callable,
                ConcreteCallable::Global { item_id, .. }
                    if resolve_item_name(&fir_store, item_id) == "H"
            )
        }),
        "the constructor field should resolve directly from its argument"
    );
    let factory_captures = result.direct_call_sites.iter().find_map(|site| {
        if let ConcreteCallable::Closure { captures, .. } = &site.callable {
            Some(captures)
        } else {
            None
        }
    });
    assert_eq!(
        factory_captures.map(Vec::len),
        Some(1),
        "the factory field should follow the function return and recover its angle capture"
    );
}

#[test]
fn identity_closure_over_global_callable_collapses() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(a => H(a), q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(a : Qubit, ) : Unit {
                H(a)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(a : Qubit, ) : Unit {
                H(a)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn identity_closure_wrapping_param() {
    let source = r#"
        operation Inner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Outer(action : Qubit => Unit, q : Qubit) : Unit {
            Inner(a => action(a), q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(H, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer(action : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(/ * closure item = 4 captures = [action] * / _lambda_4, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_4(action : (Qubit => Unit), a : Qubit) : Unit {
                action(a)
            }
            operation Inner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_AdjCtl_(action : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Inner_Empty_(/ * closure item = 7 captures = [action] * / _lambda_4, q);
            }
            operation _lambda_4(action : (Qubit => Unit is Adj + Ctl), a : Qubit) : Unit {
                action(a)
            }
            // entry
            Main()

            AFTER:
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer(action : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(/ * closure item = 4 captures = [action] * / _lambda_4, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_4(action : (Qubit => Unit), a : Qubit) : Unit {
                action(a)
            }
            operation Inner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_AdjCtl_(action : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Inner_Empty_(action, q);
            }
            operation _lambda_4(action : (Qubit => Unit is Adj + Ctl), a : Qubit) : Unit {
                action(a)
            }
            operation Outer_AdjCtl__H_(q : Qubit) : Unit {
                Inner_Empty__H_(q);
            }
            operation Inner_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn non_identity_closure_preserved() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(a => { H(a); X(a); }, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=3, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(a : Qubit, ) : Unit {
                {
                    H(a);
                    X(a);
                }

            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__closure_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(a : Qubit, ) : Unit {
                {
                    H(a);
                    X(a);
                }

            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit) : Unit {
                _lambda_3(q, );
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn identity_closure_tuple_args() {
    let source = r#"
        operation Pair(a : Qubit, b : Qubit) : Unit {
            H(a);
            H(b);
        }
        operation HOF2(op : (Qubit, Qubit) => Unit, q1 : Qubit, q2 : Qubit) : Unit {
            op(q1, q2);
        }
        operation Main() : Unit {
            use q1 = Qubit();
            use q2 = Qubit();
            HOF2((a, b) => Pair(a, b), q1, q2);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Pair(a : Qubit, b : Qubit) : Unit {
                H(a);
                H(b);
            }
            operation HOF2(op : ((Qubit, Qubit) => Unit), q1 : Qubit, q2 : Qubit) : Unit {
                op(q1, q2);
            }
            operation Main() : Unit {
                let q1 : Qubit = __quantum__rt__qubit_allocate();
                let q2 : Qubit = __quantum__rt__qubit_allocate();
                HOF2_Empty_(/ * closure item = 4 captures = [] * / _lambda_4, q1, q2);
                __quantum__rt__qubit_release(q2);
                __quantum__rt__qubit_release(q1);
            }
            operation _lambda_4((a : Qubit, b : Qubit), ) : Unit {
                Pair(a, b)
            }
            operation HOF2_Empty_(op : ((Qubit, Qubit) => Unit), q1 : Qubit, q2 : Qubit) : Unit {
                op(q1, q2);
            }
            // entry
            Main()

            AFTER:
            operation Pair(a : Qubit, b : Qubit) : Unit {
                H(a);
                H(b);
            }
            operation HOF2(op : ((Qubit, Qubit) => Unit), q1 : Qubit, q2 : Qubit) : Unit {
                op(q1, q2);
            }
            operation Main() : Unit {
                let q1 : Qubit = __quantum__rt__qubit_allocate();
                let q2 : Qubit = __quantum__rt__qubit_allocate();
                HOF2_Empty__Pair_(q1, q2);
                __quantum__rt__qubit_release(q2);
                __quantum__rt__qubit_release(q1);
            }
            operation _lambda_4((a : Qubit, b : Qubit), ) : Unit {
                Pair(a, b)
            }
            operation HOF2_Empty_(op : ((Qubit, Qubit) => Unit), q1 : Qubit, q2 : Qubit) : Unit {
                op(q1, q2);
            }
            operation HOF2_Empty__Pair_(q1 : Qubit, q2 : Qubit) : Unit {
                Pair(q1, q2);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn closure_with_captures_not_identity() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(a => Rx(angle, a), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=3, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle : Double = 1.;
                ApplyOp_Empty_(/ * closure item = 3 captures = [angle] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle : Double, a : Qubit) : Unit {
                Rx(angle, a)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle : Double = 1.;
                ApplyOp_Empty__closure_(q, angle);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle : Double, a : Qubit) : Unit {
                Rx(angle, a)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Double) : Unit {
                _lambda_3(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

/// A partial-application lambda argument (`register => Shifted(1, register)`)
/// is lifted to a closure and then defunctionalized end-to-end: the test
/// snapshots both the reachable-callable shape (`check`) and the full
/// before/after rewrite (`check_rewrite`), not just the analysis result.
#[test]
fn partial_application_lambda_defunctionalizes_end_to_end() {
    let source = r#"
        operation ApplyOp(op : Qubit[] => Unit, register : Qubit[]) : Unit {
            op(register);
        }
        operation Shifted(shift : Int, register : Qubit[]) : Unit {
            ApplyXorInPlace(shift, register);
        }
        operation Main() : Unit {
            use register = Qubit[2];
            ApplyOp(register => Shifted(1, register), register);
        }
        "#;
    check(
        source,
        &expect![
            ".lambda_4: input_ty=((Qubit)[],)\nApplyOp<Empty>{closure}: input_ty=(Qubit)[]\nMain: input_ty=Unit\nShifted: input_ty=(Int, (Qubit)[])"
        ],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit[] => Unit), register : Qubit[]) : Unit {
                op(register);
            }
            operation Shifted(shift : Int, register : Qubit[]) : Unit {
                ApplyXorInPlace(shift, register);
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(2);
                ApplyOp_Empty_(/ * closure item = 4 captures = [] * / _lambda_4, register);
                ReleaseQubitArray(register);
            }
            operation _lambda_4(register : Qubit[], ) : Unit {
                Shifted(1, register)
            }
            operation ApplyOp_Empty_(op : (Qubit[] => Unit), register : Qubit[]) : Unit {
                op(register);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit[] => Unit), register : Qubit[]) : Unit {
                op(register);
            }
            operation Shifted(shift : Int, register : Qubit[]) : Unit {
                ApplyXorInPlace(shift, register);
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(2);
                ApplyOp_Empty__closure_(register);
                ReleaseQubitArray(register);
            }
            operation _lambda_4(register : Qubit[], ) : Unit {
                Shifted(1, register)
            }
            operation ApplyOp_Empty_(op : (Qubit[] => Unit), register : Qubit[]) : Unit {
                op(register);
            }
            operation ApplyOp_Empty__closure_(register : Qubit[]) : Unit {
                _lambda_4(register, );
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn reaching_def_mutable_single_assign() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            ApplyOp(op, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                ApplyOp_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn reaching_def_conditional_both_known() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } else { X };
            ApplyOp(f, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let f : (Qubit => Unit is Adj + Ctl) = if true {
                    H
                } else {
                    X
                };
                ApplyOp_AdjCtl_(f, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                if true {
                    ApplyOp_AdjCtl__H_(q)
                } else {
                    ApplyOp_AdjCtl__X_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn reaching_def_mutable_multi_assign() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { set op = X; }
            ApplyOp(op, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                }

                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                }

                if true {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__H_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn reaching_def_mutable_both_branches() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { set op = X; } else { set op = S; }
            ApplyOp(op, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                } else {
                    op = S;
                }

                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                } else {
                    op = S;
                }

                if true {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__S_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_AdjCtl__S_(q : Qubit) : Unit {
                S(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn reaching_def_mutable_in_loop_dynamic() {
    check_errors(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            for _ in 0..3 { set op = X; }
            ApplyOp(op, q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

/// A conditional callable whose selecting guard reads a local that is
/// reassigned after the callable is bound must not be lowered to a guarded
/// dispatch: rewrite re-evaluates the guard at the *apply* site, but the guard
/// variable's value there differs from its value at the *binding* site, so the
/// dispatch would silently select the wrong callable. The analysis degrades
/// such a guard to `Dynamic`, surfacing a clear diagnostic instead of emitting
/// incorrect dispatch.
#[test]
fn reaching_def_conditional_callable_reassigned_guard_dynamic() {
    check_errors(
        r#"
        operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            use a = Qubit();
            X(a);
            let ra = MResetZ(a);
            mutable flag = ra == One;
            let op = if flag { X } else { Z };
            set flag = false;
            ApplyOp(op, q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

#[test]
fn analysis_closure_through_multiple_levels() {
    let source = r#"
        operation Inner(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Outer(op : Qubit => Unit, q : Qubit) : Unit { Inner(op, q); }
        operation Main() : Unit {
            use q = Qubit();
            Outer(q1 => H(q1), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 2
              param: callable_id=<item 5 in package 2>, path=[0], ty=(Qubit => Unit)
              param: callable_id=<item 6 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 2
              site: hof=Outer<Empty>, arg=Global(H, Body)
              site: hof=Inner<Empty>, arg=Dynamic"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_Empty_(/ * closure item = 4 captures = [] * / _lambda_4, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_4(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation Inner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(op, q);
            }
            // entry
            Main()

            AFTER:
            operation Inner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Outer_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_4(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation Inner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Outer_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                Inner_Empty_(op, q);
            }
            operation Outer_Empty__H_(q : Qubit) : Unit {
                Inner_Empty__H_(q);
            }
            operation Inner_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_callable_returned_from_function() {
    let source = r#"
        operation GetOp() : Qubit => Unit { H }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            let op = GetOp();
            ApplyOp(op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(H, Body)
            lattice states:
              callable Main:
                2: Single(H:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation GetOp() : (Qubit => Unit) {
                H
            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let op : (Qubit => Unit) = GetOp();
                ApplyOp_Empty_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation GetOp() : (Qubit => Unit) {
                H
            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_from_function_return_resolves_statically() {
    let source = r#"
        function GetOp() : (Qubit => Unit) { H }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(GetOp(), q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            function GetOp() : (Qubit => Unit) {
                H
            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(GetOp(), q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            function GetOp() : (Qubit => Unit) {
                H
            }
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_returning_partial_application_resolves_statically() {
    let source = r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
            if bits[0] {
                CNOT(register[0], target);
            }
        }

        operation MakeParity(bits : Bool[]) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(bits, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            let op = MakeParity([true]);
            ApplyOp(op, register, target);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation MakeParity(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Bool[] = bits;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let op : ((Qubit[], Qubit) => Unit) = MakeParity([true]);
                ApplyOp_Empty_(op, register, target);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_5(arg : Bool[], (hole : Qubit[], hole_1 : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation MakeParity(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Bool[] = bits;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                {
                    let __capture : Bool[] = [true];
                    ApplyOp_Empty__closure_(register, target, __capture)
                };
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_5(arg : Bool[], (hole : Qubit[], hole_1 : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyOp_Empty__closure_(register : Qubit[], target : Qubit, __capture_0 : Bool[]) : Unit {
                _lambda_5(__capture_0, (register, target));
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_callable_returning_partial_application_with_explicit_return() {
    let source = r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
            if bits[0] {
                CNOT(register[0], target);
            }
        }

        operation MakeParity(bits : Bool[]) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(bits, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            let op = MakeParity([true]);
            ApplyOp(op, register, target);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 6 in package 2>, path=[0], ty=(((Qubit)[], Qubit) => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=5, Body)
            lattice states:
              callable Main:
                3: Single(Closure(5):Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation MakeParity(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Bool[] = bits;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let op : ((Qubit[], Qubit) => Unit) = MakeParity([true]);
                ApplyOp_Empty_(op, register, target);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_5(arg : Bool[], (hole : Qubit[], hole_1 : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation MakeParity(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Bool[] = bits;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                {
                    let __capture : Bool[] = [true];
                    ApplyOp_Empty__closure_(register, target, __capture)
                };
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_5(arg : Bool[], (hole : Qubit[], hole_1 : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyOp_Empty__closure_(register : Qubit[], target : Qubit, __capture_0 : Bool[]) : Unit {
                _lambda_5(__capture_0, (register, target));
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_returning_partial_application_from_local_arg_preserves_capture_expr() {
    let source = r#"
        operation UseOracle(oracle : ((Qubit[], Qubit) => Unit), n : Int) : Unit {
            use register = Qubit[n];
            use target = Qubit();
            oracle(register, target);
            Reset(target);
            ResetAll(register);
        }

        operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
            if bits[0] {
                CNOT(register[0], target);
            }
        }

        operation Encode(bits : Bool[]) : (Qubit[], Qubit) => Unit {
            ApplyParityOperation(bits, _, _)
        }

        operation Main() : Unit {
            let bits = [true];
            let oracle = Encode(bits);
            UseOracle(oracle, Length(bits));
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation UseOracle(oracle : ((Qubit[], Qubit) => Unit), n : Int) : Unit {
                let register : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                oracle(register, target);
                Reset(target);
                ResetAll(register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation Encode(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                {
                    let arg : Bool[] = bits;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                }

            }
            operation Main() : Unit {
                let bits : Bool[] = [true];
                let oracle : ((Qubit[], Qubit) => Unit) = Encode(bits);
                UseOracle_Empty_(oracle, Length(bits));
            }
            operation _lambda_5(arg : Bool[], (hole : Qubit[], hole_1 : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation UseOracle_Empty_(oracle : ((Qubit[], Qubit) => Unit), n : Int) : Unit {
                let register : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                oracle(register, target);
                Reset(target);
                ResetAll(register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            // entry
            Main()

            AFTER:
            operation UseOracle(oracle : ((Qubit[], Qubit) => Unit), n : Int) : Unit {
                let register : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                oracle(register, target);
                Reset(target);
                ResetAll(register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation ApplyParityOperation(bits : Bool[], register : Qubit[], target : Qubit) : Unit {
                if bits[0] {
                    CNOT(register[0], target);
                }

            }
            operation Encode(bits : Bool[]) : ((Qubit[], Qubit) => Unit) {
                {
                    let arg : Bool[] = bits;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                }

            }
            operation Main() : Unit {
                let bits : Bool[] = [true];
                {
                    let __capture : Bool[] = bits;
                    UseOracle_Empty__closure_(Length(bits), __capture)
                };
            }
            operation _lambda_5(arg : Bool[], (hole : Qubit[], hole_1 : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation UseOracle_Empty_(oracle : ((Qubit[], Qubit) => Unit), n : Int) : Unit {
                let register : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                oracle(register, target);
                Reset(target);
                ResetAll(register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation UseOracle_Empty__closure_(n : Int, __capture_0 : Bool[]) : Unit {
                let register : Qubit[] = AllocateQubitArray(n);
                let target : Qubit = __quantum__rt__qubit_allocate();
                _lambda_5(__capture_0, (register, target));
                Reset(target);
                ResetAll(register);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_from_array_index_resolves_statically() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            let ops = [H, X];
            ApplyOp(ops[0], q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let ops : (Qubit => Unit is Adj + Ctl)[] = [H, X];
                ApplyOp_AdjCtl_(ops[0], q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let ops : (Qubit => Unit is Adj + Ctl)[] = [H, X];
                ApplyOp_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_returning_partial_application_from_function_resolves_statically() {
    let source = r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
            if value == 1 {
                CNOT(register[0], target);
            }
        }

        function Encode(value : Int) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(value, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            let value = 1;
            let oracle = Encode(value);
            ApplyOp(oracle, register, target);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
                if value == 1 {
                    CNOT(register[0], target);
                }

            }
            function Encode(value : Int) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Int = value;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let value : Int = 1;
                let oracle : ((Qubit[], Qubit) => Unit) = Encode(value);
                ApplyOp_Empty_(oracle, register, target);
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_5(arg : Int, (hole : Qubit[], hole_1 : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
                if value == 1 {
                    CNOT(register[0], target);
                }

            }
            function Encode(value : Int) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Int = value;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let value : Int = 1;
                {
                    let __capture : Int = value;
                    ApplyOp_Empty__closure_(register, target, __capture)
                };
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
            }
            operation _lambda_5(arg : Int, (hole : Qubit[], hole_1 : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyOp_Empty__closure_(register : Qubit[], target : Qubit, __capture_0 : Int) : Unit {
                _lambda_5(__capture_0, (register, target));
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_callable_from_constant_callable_array_loop() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let ops = [H, X];
            for op in ops {
                ApplyOp(op, q);
            }
        }
                "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)
            lattice states:
              callable Main:
                7: Dynamic"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let ops : (Qubit => Unit is Adj + Ctl)[] = [H, X];
                let _generated_ident_77 : Unit = {
                    let _array_id_44 : (Qubit => Unit is Adj + Ctl)[] = ops;
                    let _len_id_48 : Int = Length(_array_id_44);
                    mutable _index_id_53 : Int = 0;
                    while _index_id_53 < _len_id_48 {
                        let op : (Qubit => Unit is Adj + Ctl) = _array_id_44[_index_id_53];
                        ApplyOp_AdjCtl_(op, q);
                        _index_id_53 += 1;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_77
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let ops : (Qubit => Unit is Adj + Ctl)[] = [H, X];
                let _generated_ident_77 : Unit = {
                    let _array_id_44 : (Qubit => Unit is Adj + Ctl)[] = ops;
                    let _len_id_48 : Int = Length(_array_id_44);
                    mutable _index_id_53 : Int = 0;
                    while _index_id_53 < _len_id_48 {
                        let op : (Qubit => Unit is Adj + Ctl) = _array_id_44[_index_id_53];
                        {
                            [(), ()][_index_id_53];
                            if (_index_id_53 == 0) or (_index_id_53 == -2) {
                                ApplyOp_AdjCtl__H_(q)
                            } else {
                                ApplyOp_AdjCtl__X_(q)
                            }
                        };
                        _index_id_53 += 1;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_77
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn indexed_closure_callable_array_loop_dispatches_closures() {
    let source = r#"
        struct Config {
            Ops : ((Qubit, Qubit[]) => Unit)[],
            Count : Int
        }

        operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
            if value == 1 {
                Controlled X([control], register[0]);
            }
        }

        operation Run(config : Config) : Unit {
            use qs = Qubit[config.Count + 1];
            let controls = qs[0..config.Count - 1];
            let targets = qs[config.Count...];
            for idx in 0..config.Count - 1 {
                config.Ops[idx](controls[idx], targets);
            }
            ResetAll(qs);
        }

        operation Main() : Unit {
            let ops = [ApplyParityOperation(1, _, _), ApplyParityOperation(2, _, _)];
            Run(new Config { Ops = ops, Count = 2 });
        }
        "#;
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Config = (((Qubit, Qubit[]) => Unit)[], Int);
            operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
                if value == 1 {
                    Controlled X([control], register[0]);
                }

            }
            operation Run(config : __UDT_Item_1__Package_2_) : Unit {
                let qs : Qubit[] = AllocateQubitArray(config::Count + 1);
                let controls : Qubit[] = qs[0..config::Count - 1];
                let targets : Qubit[] = qs[config::Count...];
                {
                    let _range_id_165 : Range = 0..config::Count - 1;
                    mutable _index_id_168 : Int = _range_id_165.Start;
                    let _step_id_173 : Int = _range_id_165.Step;
                    let _end_id_178 : Int = _range_id_165.End;
                    while ((_step_id_173 > 0) and (_index_id_168 <= _end_id_178)) or ((_step_id_173 < 0) and (_index_id_168 >= _end_id_178)) {
                        let idx : Int = _index_id_168;
                        config::Ops[idx](controls[idx], targets);
                        _index_id_168 += _step_id_173;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation Main() : Unit {
                let ops : ((Qubit, Qubit[]) => Unit)[] = [{
                    let arg : Int = 1;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                }, {
                    let arg_1 : Int = 2;
                    / * closure item = 6 captures = [arg_1] * / _lambda_6
                }];
                Run(new Config {
                    Ops = ops,
                    Count = 2
                });
            }
            operation _lambda_5(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation _lambda_6(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            // entry
            Main()

            AFTER:
            newtype Config = (((Qubit, Qubit[]) => Unit)[], Int);
            operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
                if value == 1 {
                    Controlled X([control], register[0]);
                }

            }
            operation Run(config : __UDT_Item_1__Package_2_) : Unit {
                let qs : Qubit[] = AllocateQubitArray(config::Count + 1);
                let controls : Qubit[] = qs[0..config::Count - 1];
                let targets : Qubit[] = qs[config::Count...];
                {
                    let _range_id_165 : Range = 0..config::Count - 1;
                    mutable _index_id_168 : Int = _range_id_165.Start;
                    let _step_id_173 : Int = _range_id_165.Step;
                    let _end_id_178 : Int = _range_id_165.End;
                    while ((_step_id_173 > 0) and (_index_id_168 <= _end_id_178)) or ((_step_id_173 < 0) and (_index_id_168 >= _end_id_178)) {
                        let idx : Int = _index_id_168;
                        config::Ops[idx](controls[idx], targets);
                        _index_id_168 += _step_id_173;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation Main() : Unit {
                Run_closure__closure_(2, 1, 2);
            }
            operation _lambda_5(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation _lambda_6(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation Run_closure__closure_(config : Int, __capture_0 : Int, __capture_1 : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray(config + 1);
                let controls : Qubit[] = qs[0..config - 1];
                let targets : Qubit[] = qs[config...];
                {
                    let _range_id_165 : Range = 0..config - 1;
                    mutable _index_id_168 : Int = _range_id_165.Start;
                    let _step_id_173 : Int = _range_id_165.Step;
                    let _end_id_178 : Int = _range_id_165.End;
                    while ((_step_id_173 > 0) and (_index_id_168 <= _end_id_178)) or ((_step_id_173 < 0) and (_index_id_168 >= _end_id_178)) {
                        let idx : Int = _index_id_168;
                        {
                            [(), ()][idx];
                            if (idx == 0) or (idx == -2) {
                                _lambda_5(__capture_0, (controls[idx], targets))
                            } else {
                                _lambda_6(__capture_1, (controls[idx], targets))
                            }
                        };
                        _index_id_168 += _step_id_173;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()
        "#]],
    );
}

/// A closure callable-array forwarded through a struct-literal field and fully
/// consumed by an indexed dispatch inside the callee leaves the source-array
/// local dead in the reachable caller. The dead binding must be removed before
/// the `PostDefunc` invariant walk observes it; this exercises that walk over
/// the same shape as
/// `indexed_closure_callable_array_loop_dispatches_closures`.
#[test]
fn indexed_closure_callable_array_loop_passes_invariants() {
    let source = r#"
        struct Config {
            Ops : ((Qubit, Qubit[]) => Unit)[],
            Count : Int
        }

        operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
            if value == 1 {
                Controlled X([control], register[0]);
            }
        }

        operation Run(config : Config) : Unit {
            use qs = Qubit[config.Count + 1];
            let controls = qs[0..config.Count - 1];
            let targets = qs[config.Count...];
            for idx in 0..config.Count - 1 {
                config.Ops[idx](controls[idx], targets);
            }
            ResetAll(qs);
        }

        operation Main() : Unit {
            let ops = [ApplyParityOperation(1, _, _), ApplyParityOperation(2, _, _)];
            Run(new Config { Ops = ops, Count = 2 });
        }
        "#;
    check_invariants(source);
    check_pipeline(source);
}

#[test]
fn indexed_closure_callable_array_tuple_arg_loop_dispatches_closures() {
    let source = r#"
        operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
            if value == 1 {
                Controlled X([control], register[0]);
            }
        }

        operation Run(
            statePrep : Qubit[] => Unit,
            controlledUnitary : ((Qubit, Qubit[]) => Unit)[],
            numBits : Int,
            systems : Int[],
            phaseQubitPrep : Qubit[] => Unit,
            numAncillaQubits : Int
        ) : Unit {
            use qs = Qubit[numBits + Length(systems) + numAncillaQubits];
            let ancillas = qs[0..numBits - 1];
            let allTargets = qs[numBits...];

            statePrep(allTargets);
            phaseQubitPrep(ancillas);

            for ancillaIdx in 0..numBits - 1 {
                controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
            }

            ResetAll(qs);
        }

        operation PrepareSystems(systems : Qubit[]) : Unit {
            X(systems[0]);
        }

        operation PreparePhase(ancillas : Qubit[]) : Unit {
            for q in ancillas {
                H(q);
            }
        }

        operation Main() : Unit {
            let controlledUnitary = [
                ApplyParityOperation(1, _, _),
                ApplyParityOperation(2, _, _)
            ];
            Run(PrepareSystems, controlledUnitary, 2, [0, 1], PreparePhase, 0);
        }
        "#;
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
                if value == 1 {
                    Controlled X([control], register[0]);
                }

            }
            operation Run(statePrep : (Qubit[] => Unit), controlledUnitary : ((Qubit, Qubit[]) => Unit)[], numBits : Int, systems : Int[], phaseQubitPrep : (Qubit[] => Unit), numAncillaQubits : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray((numBits + Length(systems)) + numAncillaQubits);
                let ancillas : Qubit[] = qs[0..numBits - 1];
                let allTargets : Qubit[] = qs[numBits...];
                statePrep(allTargets);
                phaseQubitPrep(ancillas);
                {
                    let _range_id_214 : Range = 0..numBits - 1;
                    mutable _index_id_217 : Int = _range_id_214.Start;
                    let _step_id_222 : Int = _range_id_214.Step;
                    let _end_id_227 : Int = _range_id_214.End;
                    while ((_step_id_222 > 0) and (_index_id_217 <= _end_id_227)) or ((_step_id_222 < 0) and (_index_id_217 >= _end_id_227)) {
                        let ancillaIdx : Int = _index_id_217;
                        controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                        _index_id_217 += _step_id_222;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation PrepareSystems(systems : Qubit[]) : Unit {
                X(systems[0]);
            }
            operation PreparePhase(ancillas : Qubit[]) : Unit {
                {
                    let _array_id_257 : Qubit[] = ancillas;
                    let _len_id_261 : Int = Length(_array_id_257);
                    mutable _index_id_266 : Int = 0;
                    while _index_id_266 < _len_id_261 {
                        let q : Qubit = _array_id_257[_index_id_266];
                        H(q);
                        _index_id_266 += 1;
                    }

                }

            }
            operation Main() : Unit {
                let controlledUnitary : ((Qubit, Qubit[]) => Unit)[] = [{
                    let arg : Int = 1;
                    / * closure item = 6 captures = [arg] * / _lambda_6
                }, {
                    let arg_1 : Int = 2;
                    / * closure item = 7 captures = [arg_1] * / _lambda_7
                }];
                Run_Empty__Empty__Empty_(PrepareSystems, controlledUnitary, 2, [0, 1], PreparePhase, 0);
            }
            operation _lambda_6(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation _lambda_7(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation Run_Empty__Empty__Empty_(statePrep : (Qubit[] => Unit), controlledUnitary : ((Qubit, Qubit[]) => Unit)[], numBits : Int, systems : Int[], phaseQubitPrep : (Qubit[] => Unit), numAncillaQubits : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray((numBits + Length(systems)) + numAncillaQubits);
                let ancillas : Qubit[] = qs[0..numBits - 1];
                let allTargets : Qubit[] = qs[numBits...];
                statePrep(allTargets);
                phaseQubitPrep(ancillas);
                {
                    let _range_id_214 : Range = 0..numBits - 1;
                    mutable _index_id_217 : Int = _range_id_214.Start;
                    let _step_id_222 : Int = _range_id_214.Step;
                    let _end_id_227 : Int = _range_id_214.End;
                    while ((_step_id_222 > 0) and (_index_id_217 <= _end_id_227)) or ((_step_id_222 < 0) and (_index_id_217 >= _end_id_227)) {
                        let ancillaIdx : Int = _index_id_217;
                        controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                        _index_id_217 += _step_id_222;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()

            AFTER:
            operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
                if value == 1 {
                    Controlled X([control], register[0]);
                }

            }
            operation Run(statePrep : (Qubit[] => Unit), controlledUnitary : ((Qubit, Qubit[]) => Unit)[], numBits : Int, systems : Int[], phaseQubitPrep : (Qubit[] => Unit), numAncillaQubits : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray((numBits + Length(systems)) + numAncillaQubits);
                let ancillas : Qubit[] = qs[0..numBits - 1];
                let allTargets : Qubit[] = qs[numBits...];
                statePrep(allTargets);
                phaseQubitPrep(ancillas);
                {
                    let _range_id_214 : Range = 0..numBits - 1;
                    mutable _index_id_217 : Int = _range_id_214.Start;
                    let _step_id_222 : Int = _range_id_214.Step;
                    let _end_id_227 : Int = _range_id_214.End;
                    while ((_step_id_222 > 0) and (_index_id_217 <= _end_id_227)) or ((_step_id_222 < 0) and (_index_id_217 >= _end_id_227)) {
                        let ancillaIdx : Int = _index_id_217;
                        controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                        _index_id_217 += _step_id_222;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation PrepareSystems(systems : Qubit[]) : Unit {
                X(systems[0]);
            }
            operation PreparePhase(ancillas : Qubit[]) : Unit {
                {
                    let _array_id_257 : Qubit[] = ancillas;
                    let _len_id_261 : Int = Length(_array_id_257);
                    mutable _index_id_266 : Int = 0;
                    while _index_id_266 < _len_id_261 {
                        let q : Qubit = _array_id_257[_index_id_266];
                        H(q);
                        _index_id_266 += 1;
                    }

                }

            }
            operation Main() : Unit {
                Run_Empty__Empty__Empty__PrepareSystems__closure__closure__PreparePhase_(2, [0, 1], 0, 1, 2);
            }
            operation _lambda_6(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation _lambda_7(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation Run_Empty__Empty__Empty_(statePrep : (Qubit[] => Unit), controlledUnitary : ((Qubit, Qubit[]) => Unit)[], numBits : Int, systems : Int[], phaseQubitPrep : (Qubit[] => Unit), numAncillaQubits : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray((numBits + Length(systems)) + numAncillaQubits);
                let ancillas : Qubit[] = qs[0..numBits - 1];
                let allTargets : Qubit[] = qs[numBits...];
                statePrep(allTargets);
                phaseQubitPrep(ancillas);
                {
                    let _range_id_214 : Range = 0..numBits - 1;
                    mutable _index_id_217 : Int = _range_id_214.Start;
                    let _step_id_222 : Int = _range_id_214.Step;
                    let _end_id_227 : Int = _range_id_214.End;
                    while ((_step_id_222 > 0) and (_index_id_217 <= _end_id_227)) or ((_step_id_222 < 0) and (_index_id_217 >= _end_id_227)) {
                        let ancillaIdx : Int = _index_id_217;
                        controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                        _index_id_217 += _step_id_222;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation Run_Empty__Empty__Empty__PrepareSystems__closure__closure__PreparePhase_(numBits : Int, systems : Int[], numAncillaQubits : Int, __capture_0 : Int, __capture_1 : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray((numBits + Length(systems)) + numAncillaQubits);
                let ancillas : Qubit[] = qs[0..numBits - 1];
                let allTargets : Qubit[] = qs[numBits...];
                PrepareSystems(allTargets);
                PreparePhase(ancillas);
                {
                    let _range_id_214 : Range = 0..numBits - 1;
                    mutable _index_id_217 : Int = _range_id_214.Start;
                    let _step_id_222 : Int = _range_id_214.Step;
                    let _end_id_227 : Int = _range_id_214.End;
                    while ((_step_id_222 > 0) and (_index_id_217 <= _end_id_227)) or ((_step_id_222 < 0) and (_index_id_217 >= _end_id_227)) {
                        let ancillaIdx : Int = _index_id_217;
                        {
                            [(), ()][ancillaIdx];
                            if (ancillaIdx == 0) or (ancillaIdx == -2) {
                                _lambda_6(__capture_0, (ancillas[ancillaIdx], allTargets))
                            } else {
                                _lambda_7(__capture_1, (ancillas[ancillaIdx], allTargets))
                            }
                        };
                        _index_id_217 += _step_id_222;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn indexed_same_target_closure_callable_array_tuple_arg_dispatches_closures() {
    let source = r#"
        operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
            if value == 1 {
                Controlled X([control], register[0]);
            }
        }

        operation Run(
            statePrep : Qubit[] => Unit,
            controlledUnitary : ((Qubit, Qubit[]) => Unit)[],
            numBits : Int,
            systems : Int[],
            phaseQubitPrep : Qubit[] => Unit,
            numAncillaQubits : Int
        ) : Unit {
            use qs = Qubit[numBits + Length(systems) + numAncillaQubits];
            let ancillas = qs[0..numBits - 1];
            let allTargets = qs[numBits...];

            statePrep(allTargets);
            phaseQubitPrep(ancillas);

            for ancillaIdx in 0..numBits - 1 {
                controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
            }

            ResetAll(qs);
        }

        operation PrepareSystems(systems : Qubit[]) : Unit {
            X(systems[0]);
        }

        operation PreparePhase(ancillas : Qubit[]) : Unit {
            for q in ancillas {
                H(q);
            }
        }

        operation Main() : Unit {
            let first = 1;
            let second = 2;
            let controlledUnitary = [
                ApplyParityOperation(first, _, _),
                ApplyParityOperation(second, _, _)
            ];
            Run(PrepareSystems, controlledUnitary, 2, [0, 1], PreparePhase, 0);
        }
        "#;
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
                if value == 1 {
                    Controlled X([control], register[0]);
                }

            }
            operation Run(statePrep : (Qubit[] => Unit), controlledUnitary : ((Qubit, Qubit[]) => Unit)[], numBits : Int, systems : Int[], phaseQubitPrep : (Qubit[] => Unit), numAncillaQubits : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray((numBits + Length(systems)) + numAncillaQubits);
                let ancillas : Qubit[] = qs[0..numBits - 1];
                let allTargets : Qubit[] = qs[numBits...];
                statePrep(allTargets);
                phaseQubitPrep(ancillas);
                {
                    let _range_id_222 : Range = 0..numBits - 1;
                    mutable _index_id_225 : Int = _range_id_222.Start;
                    let _step_id_230 : Int = _range_id_222.Step;
                    let _end_id_235 : Int = _range_id_222.End;
                    while ((_step_id_230 > 0) and (_index_id_225 <= _end_id_235)) or ((_step_id_230 < 0) and (_index_id_225 >= _end_id_235)) {
                        let ancillaIdx : Int = _index_id_225;
                        controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                        _index_id_225 += _step_id_230;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation PrepareSystems(systems : Qubit[]) : Unit {
                X(systems[0]);
            }
            operation PreparePhase(ancillas : Qubit[]) : Unit {
                {
                    let _array_id_265 : Qubit[] = ancillas;
                    let _len_id_269 : Int = Length(_array_id_265);
                    mutable _index_id_274 : Int = 0;
                    while _index_id_274 < _len_id_269 {
                        let q : Qubit = _array_id_265[_index_id_274];
                        H(q);
                        _index_id_274 += 1;
                    }

                }

            }
            operation Main() : Unit {
                let first : Int = 1;
                let second : Int = 2;
                let controlledUnitary : ((Qubit, Qubit[]) => Unit)[] = [{
                    let arg : Int = first;
                    / * closure item = 6 captures = [arg] * / _lambda_6
                }, {
                    let arg_1 : Int = second;
                    / * closure item = 7 captures = [arg_1] * / _lambda_7
                }];
                Run_Empty__Empty__Empty_(PrepareSystems, controlledUnitary, 2, [0, 1], PreparePhase, 0);
            }
            operation _lambda_6(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation _lambda_7(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation Run_Empty__Empty__Empty_(statePrep : (Qubit[] => Unit), controlledUnitary : ((Qubit, Qubit[]) => Unit)[], numBits : Int, systems : Int[], phaseQubitPrep : (Qubit[] => Unit), numAncillaQubits : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray((numBits + Length(systems)) + numAncillaQubits);
                let ancillas : Qubit[] = qs[0..numBits - 1];
                let allTargets : Qubit[] = qs[numBits...];
                statePrep(allTargets);
                phaseQubitPrep(ancillas);
                {
                    let _range_id_222 : Range = 0..numBits - 1;
                    mutable _index_id_225 : Int = _range_id_222.Start;
                    let _step_id_230 : Int = _range_id_222.Step;
                    let _end_id_235 : Int = _range_id_222.End;
                    while ((_step_id_230 > 0) and (_index_id_225 <= _end_id_235)) or ((_step_id_230 < 0) and (_index_id_225 >= _end_id_235)) {
                        let ancillaIdx : Int = _index_id_225;
                        controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                        _index_id_225 += _step_id_230;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()

            AFTER:
            operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
                if value == 1 {
                    Controlled X([control], register[0]);
                }

            }
            operation Run(statePrep : (Qubit[] => Unit), controlledUnitary : ((Qubit, Qubit[]) => Unit)[], numBits : Int, systems : Int[], phaseQubitPrep : (Qubit[] => Unit), numAncillaQubits : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray((numBits + Length(systems)) + numAncillaQubits);
                let ancillas : Qubit[] = qs[0..numBits - 1];
                let allTargets : Qubit[] = qs[numBits...];
                statePrep(allTargets);
                phaseQubitPrep(ancillas);
                {
                    let _range_id_222 : Range = 0..numBits - 1;
                    mutable _index_id_225 : Int = _range_id_222.Start;
                    let _step_id_230 : Int = _range_id_222.Step;
                    let _end_id_235 : Int = _range_id_222.End;
                    while ((_step_id_230 > 0) and (_index_id_225 <= _end_id_235)) or ((_step_id_230 < 0) and (_index_id_225 >= _end_id_235)) {
                        let ancillaIdx : Int = _index_id_225;
                        controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                        _index_id_225 += _step_id_230;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation PrepareSystems(systems : Qubit[]) : Unit {
                X(systems[0]);
            }
            operation PreparePhase(ancillas : Qubit[]) : Unit {
                {
                    let _array_id_265 : Qubit[] = ancillas;
                    let _len_id_269 : Int = Length(_array_id_265);
                    mutable _index_id_274 : Int = 0;
                    while _index_id_274 < _len_id_269 {
                        let q : Qubit = _array_id_265[_index_id_274];
                        H(q);
                        _index_id_274 += 1;
                    }

                }

            }
            operation Main() : Unit {
                let first : Int = 1;
                let second : Int = 2;
                {
                    let __capture : Int = first;
                    let __capture_1 : Int = second;
                    Run_Empty__Empty__Empty__PrepareSystems__closure__closure__PreparePhase_(2, [0, 1], 0, __capture, __capture_1)
                };
            }
            operation _lambda_6(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation _lambda_7(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation Run_Empty__Empty__Empty_(statePrep : (Qubit[] => Unit), controlledUnitary : ((Qubit, Qubit[]) => Unit)[], numBits : Int, systems : Int[], phaseQubitPrep : (Qubit[] => Unit), numAncillaQubits : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray((numBits + Length(systems)) + numAncillaQubits);
                let ancillas : Qubit[] = qs[0..numBits - 1];
                let allTargets : Qubit[] = qs[numBits...];
                statePrep(allTargets);
                phaseQubitPrep(ancillas);
                {
                    let _range_id_222 : Range = 0..numBits - 1;
                    mutable _index_id_225 : Int = _range_id_222.Start;
                    let _step_id_230 : Int = _range_id_222.Step;
                    let _end_id_235 : Int = _range_id_222.End;
                    while ((_step_id_230 > 0) and (_index_id_225 <= _end_id_235)) or ((_step_id_230 < 0) and (_index_id_225 >= _end_id_235)) {
                        let ancillaIdx : Int = _index_id_225;
                        controlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                        _index_id_225 += _step_id_230;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation Run_Empty__Empty__Empty__PrepareSystems__closure__closure__PreparePhase_(numBits : Int, systems : Int[], numAncillaQubits : Int, __capture_0 : Int, __capture_1 : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray((numBits + Length(systems)) + numAncillaQubits);
                let ancillas : Qubit[] = qs[0..numBits - 1];
                let allTargets : Qubit[] = qs[numBits...];
                PrepareSystems(allTargets);
                PreparePhase(ancillas);
                {
                    let _range_id_222 : Range = 0..numBits - 1;
                    mutable _index_id_225 : Int = _range_id_222.Start;
                    let _step_id_230 : Int = _range_id_222.Step;
                    let _end_id_235 : Int = _range_id_222.End;
                    while ((_step_id_230 > 0) and (_index_id_225 <= _end_id_235)) or ((_step_id_230 < 0) and (_index_id_225 >= _end_id_235)) {
                        let ancillaIdx : Int = _index_id_225;
                        {
                            [(), ()][ancillaIdx];
                            if (ancillaIdx == 0) or (ancillaIdx == -2) {
                                _lambda_6(__capture_0, (ancillas[ancillaIdx], allTargets))
                            } else {
                                _lambda_7(__capture_1, (ancillas[ancillaIdx], allTargets))
                            }
                        };
                        _index_id_225 += _step_id_230;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn indexed_closure_callable_array_udt_with_callable_siblings_dispatches_closures() {
    let source = r#"
        struct Config {
            StatePrep : Qubit[] => Unit,
            ControlledUnitary : ((Qubit, Qubit[]) => Unit)[],
            PhaseQubitPrep : Qubit[] => Unit,
            NumBits : Int,
            Systems : Int[],
            NumAncillaQubits : Int
        }

        operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
            if value == 1 {
                Controlled X([control], register[0]);
            }
        }

        operation Run(config : Config) : Unit {
            use qs = Qubit[config.NumBits + Length(config.Systems) + config.NumAncillaQubits];
            let ancillas = qs[0..config.NumBits - 1];
            let allTargets = qs[config.NumBits...];

            config.StatePrep(allTargets);
            config.PhaseQubitPrep(ancillas);

            for ancillaIdx in 0..config.NumBits - 1 {
                config.ControlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
            }

            ResetAll(qs);
        }

        operation PrepareSystems(systems : Qubit[]) : Unit {
            X(systems[0]);
        }

        operation PreparePhase(ancillas : Qubit[]) : Unit {
            for q in ancillas {
                H(q);
            }
        }

        operation Main() : Unit {
            let controlledUnitary = [
                ApplyParityOperation(1, _, _),
                ApplyParityOperation(2, _, _)
            ];
            Run(new Config {
                StatePrep = PrepareSystems,
                ControlledUnitary = controlledUnitary,
                PhaseQubitPrep = PreparePhase,
                NumBits = 2,
                Systems = [0, 1],
                NumAncillaQubits = 0
            });
        }
        "#;
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Config = ((Qubit[] => Unit), ((Qubit, Qubit[]) => Unit)[], (Qubit[] => Unit), Int, Int[], Int);
            operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
                if value == 1 {
                    Controlled X([control], register[0]);
                }

            }
            operation Run(config : __UDT_Item_1__Package_2_) : Unit {
                let qs : Qubit[] = AllocateQubitArray((config::NumBits + Length(config::Systems)) + config::NumAncillaQubits);
                let ancillas : Qubit[] = qs[0..config::NumBits - 1];
                let allTargets : Qubit[] = qs[config::NumBits...];
                config::StatePrep(allTargets);
                config::PhaseQubitPrep(ancillas);
                {
                    let _range_id_219 : Range = 0..config::NumBits - 1;
                    mutable _index_id_222 : Int = _range_id_219.Start;
                    let _step_id_227 : Int = _range_id_219.Step;
                    let _end_id_232 : Int = _range_id_219.End;
                    while ((_step_id_227 > 0) and (_index_id_222 <= _end_id_232)) or ((_step_id_227 < 0) and (_index_id_222 >= _end_id_232)) {
                        let ancillaIdx : Int = _index_id_222;
                        config::ControlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                        _index_id_222 += _step_id_227;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation PrepareSystems(systems : Qubit[]) : Unit {
                X(systems[0]);
            }
            operation PreparePhase(ancillas : Qubit[]) : Unit {
                {
                    let _array_id_262 : Qubit[] = ancillas;
                    let _len_id_266 : Int = Length(_array_id_262);
                    mutable _index_id_271 : Int = 0;
                    while _index_id_271 < _len_id_266 {
                        let q : Qubit = _array_id_262[_index_id_271];
                        H(q);
                        _index_id_271 += 1;
                    }

                }

            }
            operation Main() : Unit {
                let controlledUnitary : ((Qubit, Qubit[]) => Unit)[] = [{
                    let arg : Int = 1;
                    / * closure item = 7 captures = [arg] * / _lambda_7
                }, {
                    let arg_1 : Int = 2;
                    / * closure item = 8 captures = [arg_1] * / _lambda_8
                }];
                Run(new Config {
                    StatePrep = PrepareSystems,
                    ControlledUnitary = controlledUnitary,
                    PhaseQubitPrep = PreparePhase,
                    NumBits = 2,
                    Systems = [0, 1],
                    NumAncillaQubits = 0
                });
            }
            operation _lambda_7(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation _lambda_8(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            // entry
            Main()

            AFTER:
            newtype Config = ((Qubit[] => Unit), ((Qubit, Qubit[]) => Unit)[], (Qubit[] => Unit), Int, Int[], Int);
            operation ApplyParityOperation(value : Int, control : Qubit, register : Qubit[]) : Unit {
                if value == 1 {
                    Controlled X([control], register[0]);
                }

            }
            operation Run(config : __UDT_Item_1__Package_2_) : Unit {
                let qs : Qubit[] = AllocateQubitArray((config::NumBits + Length(config::Systems)) + config::NumAncillaQubits);
                let ancillas : Qubit[] = qs[0..config::NumBits - 1];
                let allTargets : Qubit[] = qs[config::NumBits...];
                config::StatePrep(allTargets);
                config::PhaseQubitPrep(ancillas);
                {
                    let _range_id_219 : Range = 0..config::NumBits - 1;
                    mutable _index_id_222 : Int = _range_id_219.Start;
                    let _step_id_227 : Int = _range_id_219.Step;
                    let _end_id_232 : Int = _range_id_219.End;
                    while ((_step_id_227 > 0) and (_index_id_222 <= _end_id_232)) or ((_step_id_227 < 0) and (_index_id_222 >= _end_id_232)) {
                        let ancillaIdx : Int = _index_id_222;
                        config::ControlledUnitary[ancillaIdx](ancillas[ancillaIdx], allTargets);
                        _index_id_222 += _step_id_227;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            operation PrepareSystems(systems : Qubit[]) : Unit {
                X(systems[0]);
            }
            operation PreparePhase(ancillas : Qubit[]) : Unit {
                {
                    let _array_id_262 : Qubit[] = ancillas;
                    let _len_id_266 : Int = Length(_array_id_262);
                    mutable _index_id_271 : Int = 0;
                    while _index_id_271 < _len_id_266 {
                        let q : Qubit = _array_id_262[_index_id_271];
                        H(q);
                        _index_id_271 += 1;
                    }

                }

            }
            operation Main() : Unit {
                Run_PrepareSystems__closure__closure__PreparePhase_((2, [0, 1], 0), 1, 2);
            }
            operation _lambda_7(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation _lambda_8(arg : Int, (hole : Qubit, hole_1 : Qubit[])) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation Run_PrepareSystems__closure__closure__PreparePhase_(config : (Int, Int[], Int), __capture_0 : Int, __capture_1 : Int) : Unit {
                let qs : Qubit[] = AllocateQubitArray((config::Item < 0 > + Length(config::Item < 1 >)) + config::Item < 2 >);
                let ancillas : Qubit[] = qs[0..config::Item < 0 > - 1];
                let allTargets : Qubit[] = qs[config::Item < 0 > ...];
                PrepareSystems(allTargets);
                PreparePhase(ancillas);
                {
                    let _range_id_219 : Range = 0..config::Item < 0 > - 1;
                    mutable _index_id_222 : Int = _range_id_219.Start;
                    let _step_id_227 : Int = _range_id_219.Step;
                    let _end_id_232 : Int = _range_id_219.End;
                    while ((_step_id_227 > 0) and (_index_id_222 <= _end_id_232)) or ((_step_id_227 < 0) and (_index_id_222 >= _end_id_232)) {
                        let ancillaIdx : Int = _index_id_222;
                        {
                            [(), ()][ancillaIdx];
                            if (ancillaIdx == 0) or (ancillaIdx == -2) {
                                _lambda_7(__capture_0, (ancillas[ancillaIdx], allTargets))
                            } else {
                                _lambda_8(__capture_1, (ancillas[ancillaIdx], allTargets))
                            }
                        };
                        _index_id_222 += _step_id_227;
                    }

                }

                ResetAll(qs);
                ReleaseQubitArray(qs);
            }
            // entry
            Main()
        "#]],
    );
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn analysis_callable_returning_partial_application_from_function_in_loop() {
    let source = r#"
        operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
            op(register, target);
        }

        operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
            if value == 1 {
                CNOT(register[0], target);
            }
        }

        function Encode(value : Int) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(value, _, _);
        }

        operation Main() : Unit {
            use register = Qubit[1];
            use target = Qubit();
            for value in [1, 2] {
                let oracle = Encode(value);
                ApplyOp(oracle, register, target);
            }
        }
                "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 6 in package 2>, path=[0], ty=(((Qubit)[], Qubit) => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Closure(target=5, Body)
            lattice states:
              callable Main:
                8: Single(Closure(5):Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
                if value == 1 {
                    CNOT(register[0], target);
                }

            }
            function Encode(value : Int) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Int = value;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_156 : Unit = {
                    let _array_id_118 : Int[] = [1, 2];
                    let _len_id_122 : Int = Length(_array_id_118);
                    mutable _index_id_127 : Int = 0;
                    while _index_id_127 < _len_id_122 {
                        let value : Int = _array_id_118[_index_id_127];
                        let oracle : ((Qubit[], Qubit) => Unit) = Encode(value);
                        ApplyOp_Empty_(oracle, register, target);
                        _index_id_127 += 1;
                    }

                };
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
                _generated_ident_156
            }
            operation _lambda_5(arg : Int, (hole : Qubit[], hole_1 : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyParityOperation(value : Int, register : Qubit[], target : Qubit) : Unit {
                if value == 1 {
                    CNOT(register[0], target);
                }

            }
            function Encode(value : Int) : ((Qubit[], Qubit) => Unit) {
                return {
                    let arg : Int = value;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation Main() : Unit {
                let register : Qubit[] = AllocateQubitArray(1);
                let target : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_156 : Unit = {
                    let _array_id_118 : Int[] = [1, 2];
                    let _len_id_122 : Int = Length(_array_id_118);
                    mutable _index_id_127 : Int = 0;
                    while _index_id_127 < _len_id_122 {
                        let value : Int = _array_id_118[_index_id_127];
                        {
                            let __capture : Int = value;
                            ApplyOp_Empty__closure_(register, target, __capture)
                        };
                        _index_id_127 += 1;
                    }

                };
                __quantum__rt__qubit_release(target);
                ReleaseQubitArray(register);
                _generated_ident_156
            }
            operation _lambda_5(arg : Int, (hole : Qubit[], hole_1 : Qubit)) : Unit {
                ApplyParityOperation(arg, hole, hole_1)
            }
            operation ApplyOp_Empty_(op : ((Qubit[], Qubit) => Unit), register : Qubit[], target : Qubit) : Unit {
                op(register, target);
            }
            operation ApplyOp_Empty__closure_(register : Qubit[], target : Qubit, __capture_0 : Int) : Unit {
                _lambda_5(__capture_0, (register, target));
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn reaching_def_mutable_in_while_loop() {
    check_errors(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            mutable n = 3;
            while n > 0 {
                op = X;
                n -= 1;
            }
            ApplyOp(op, q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

#[test]
fn analysis_nested_callable_in_tuple_param() {
    let source = r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_((H, 42), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_(42, q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                op(q);
            }
            operation Wrapper_AdjCtl__H_(pair : Int, q : Qubit) : Unit {
                let _ : Int = pair;
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_nested_callable_second_element() {
    let source = r#"
        operation Wrapper(pair : (Int, Qubit => Unit), q : Qubit) : Unit {
            let (_, op) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((42, H), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : (Int, (Qubit => Unit)), q : Qubit) : Unit {
                let (_ : Int, op : (Qubit => Unit)) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_((42, H), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : (Int, (Qubit => Unit is Adj + Ctl)), q : Qubit) : Unit {
                let (_ : Int, op : (Qubit => Unit is Adj + Ctl)) = pair;
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : (Int, (Qubit => Unit)), q : Qubit) : Unit {
                let (_ : Int, op : (Qubit => Unit)) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_(42, q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : (Int, (Qubit => Unit is Adj + Ctl)), q : Qubit) : Unit {
                let (_ : Int, op : (Qubit => Unit is Adj + Ctl)) = pair;
                op(q);
            }
            operation Wrapper_AdjCtl__H_(pair : Int, q : Qubit) : Unit {
                let _ : Int = pair;
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_nested_callable_single_param_supported() {
    let source = r#"
        operation Wrapper(pair : (Qubit => Unit, Int)) : Unit {
            let (op, _) = pair;
            use q = Qubit();
            op(q);
        }
        operation Main() : Unit {
            Wrapper((H, 42));
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : ((Qubit => Unit), Int)) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                Wrapper_AdjCtl_(H, 42);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int)) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : ((Qubit => Unit), Int)) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                Wrapper_AdjCtl__H_(42);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int)) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl__H_(pair : Int) : Unit {
                let _ : Int = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                H(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_branch_split_nested_callable_in_tuple() {
    let source = r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } else { X };
            Wrapper((f, 42), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)
              site: hof=Wrapper<AdjCtl>, arg=Global(X, Body)
            lattice states:
              callable Main:
                2: Multi([H:Body, X:Body])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let f : (Qubit => Unit is Adj + Ctl) = if true {
                    H
                } else {
                    X
                };
                Wrapper_AdjCtl_((f, 42), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                if true {
                    Wrapper_AdjCtl__H_(42, q)
                } else {
                    Wrapper_AdjCtl__X_(42, q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                op(q);
            }
            operation Wrapper_AdjCtl__H_(pair : Int, q : Qubit) : Unit {
                let _ : Int = pair;
                H(q);
            }
            operation Wrapper_AdjCtl__X_(pair : Int, q : Qubit) : Unit {
                let _ : Int = pair;
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_nested_callable_single_param_second_element_supported() {
    let source = r#"
        operation Wrapper(pair : (Int, Qubit => Unit)) : Unit {
            let (_, op) = pair;
            use q = Qubit();
            op(q);
        }
        operation Main() : Unit {
            Wrapper((42, H));
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 1], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : (Int, (Qubit => Unit))) : Unit {
                let (_ : Int, op : (Qubit => Unit)) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                Wrapper_AdjCtl_(42, H);
            }
            operation Wrapper_AdjCtl_(pair : (Int, (Qubit => Unit is Adj + Ctl))) : Unit {
                let (_ : Int, op : (Qubit => Unit is Adj + Ctl)) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : (Int, (Qubit => Unit))) : Unit {
                let (_ : Int, op : (Qubit => Unit)) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                Wrapper_AdjCtl__H_(42);
            }
            operation Wrapper_AdjCtl_(pair : (Int, (Qubit => Unit is Adj + Ctl))) : Unit {
                let (_ : Int, op : (Qubit => Unit is Adj + Ctl)) = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl__H_(pair : Int) : Unit {
                let _ : Int = pair;
                let q : Qubit = __quantum__rt__qubit_allocate();
                H(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_nested_callable_single_param_recursive_supported() {
    let source = r#"
        operation Wrapper(bundle : (((Qubit => Unit, Int), Double), Qubit)) : Unit {
            let (((op, n), angle), q) = bundle;
            let _ = n;
            let _ = angle;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((((H, 42), 1.0), q));
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0, 0, 0, 0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=Wrapper<AdjCtl>, arg=Global(H, Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Wrapper(bundle : ((((Qubit => Unit), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit), n : Int), angle : Double), q : Qubit) = bundle;
                let _ : Int = n;
                let _ : Double = angle;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_(((H, 42), 1.), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(bundle : ((((Qubit => Unit is Adj + Ctl), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit is Adj + Ctl), n : Int), angle : Double), q : Qubit) = bundle;
                let _ : Int = n;
                let _ : Double = angle;
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(bundle : ((((Qubit => Unit), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit), n : Int), angle : Double), q : Qubit) = bundle;
                let _ : Int = n;
                let _ : Double = angle;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_((42, 1.), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(bundle : ((((Qubit => Unit is Adj + Ctl), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit is Adj + Ctl), n : Int), angle : Double), q : Qubit) = bundle;
                let _ : Int = n;
                let _ : Double = angle;
                op(q);
            }
            operation Wrapper_AdjCtl__H_(bundle : ((Int, Double), Qubit)) : Unit {
                let ((n : Int, angle : Double), q : Qubit) = bundle;
                let _ : Int = n;
                let _ : Double = angle;
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn identity_closure_adjoint_wrapped_collapses() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => Adjoint S(q1), q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 4 in package 2>, path=[0], ty=(Qubit => Unit)
            call_sites: 1
              site: hof=ApplyOp<Empty>, arg=Global(S, Adj)
            direct_call_sites: 1
              site: callee=S:Adj, default"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q1 : Qubit, ) : Unit {
                Adjoint S(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__Adj_S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q1 : Qubit, ) : Unit {
                Adjoint S(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__Adj_S_(q : Qubit) : Unit {
                Adjoint S(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_use_immutable_local_promoted() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = H;
            ApplyOp(op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
            lattice states:
              callable Main:
                2: Single(H:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let op : (Qubit => Unit is Adj + Ctl) = H;
                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn multi_use_immutable_local_not_promoted() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q1 = Qubit();
            use q2 = Qubit();
            let op = H;
            ApplyOp(op, q1);
            ApplyOp(op, q2);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 2
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyOp<AdjCtl>, arg=Global(H, Body)
            lattice states:
              callable Main:
                3: Single(H:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q1 : Qubit = __quantum__rt__qubit_allocate();
                let q2 : Qubit = __quantum__rt__qubit_allocate();
                let op : (Qubit => Unit is Adj + Ctl) = H;
                ApplyOp_AdjCtl_(op, q1);
                ApplyOp_AdjCtl_(op, q2);
                __quantum__rt__qubit_release(q2);
                __quantum__rt__qubit_release(q1);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q1 : Qubit = __quantum__rt__qubit_allocate();
                let q2 : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q1);
                ApplyOp_AdjCtl__H_(q2);
                __quantum__rt__qubit_release(q2);
                __quantum__rt__qubit_release(q1);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mutable_local_not_promoted() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            op = X;
            ApplyOp(op, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 1
              param: callable_id=<item 3 in package 2>, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 1
              site: hof=ApplyOp<AdjCtl>, arg=Global(X, Body)
            lattice states:
              callable Main:
                2: Single(X:Body)"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                op = X;
                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                op = X;
                ApplyOp_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_conditional_callable_binding_produces_multi_lattice() {
    let source = r#"
        operation ApplyConditional(power : Int, target : Qubit) : Unit {
            let u = if power >= 0 { S } else { Adjoint S };
            u(target);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyConditional(3, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 0
            call_sites: 0
            direct_call_sites: 2
              site: callee=S:Adj, default
              site: callee=S:Body, condition=ExprId(4)
            lattice states:
              callable ApplyConditional:
                3: Multi([S:Body, S:Adj])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyConditional(power : Int, target : Qubit) : Unit {
                let u : (Qubit => Unit is Adj + Ctl) = if power >= 0 {
                    S
                } else {
                    Adjoint S
                };
                u(target);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyConditional(3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyConditional(power : Int, target : Qubit) : Unit {
                if power >= 0 {
                    S(target)
                } else {
                    Adjoint S(target)
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyConditional(3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_from_nested_conditions() {
    let source = r#"
        operation ApplyNested(a : Int, b : Int, target : Qubit) : Unit {
            let u = if a >= 0 { if b >= 0 { S } else { T } } else { Adjoint S };
            u(target);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyNested(3, 4, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
        callable_params: 0
        call_sites: 0
        direct_call_sites: 3
          site: callee=S:Adj, default
          site: callee=S:Body, condition=ExprId(4) and ExprId(9)
          site: callee=T:Body, condition=ExprId(4)
        lattice states:
          callable ApplyNested:
            4: Multi([S:Body, T:Body, S:Adj])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyNested(a : Int, b : Int, target : Qubit) : Unit {
                let u : (Qubit => Unit is Adj + Ctl) = if a >= 0 {
                    if b >= 0 {
                        S
                    } else {
                        T
                    }

                } else {
                    Adjoint S
                };
                u(target);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyNested(3, 4, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyNested(a : Int, b : Int, target : Qubit) : Unit {
                if a >= 0 {
                    if b >= 0 {
                        S(target)
                    } else {
                        T(target)
                    }
                } else {
                    Adjoint S(target)
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyNested(3, 4, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_from_triple_nested_conditions() {
    let source = r#"
        operation ApplyTriple(a : Int, b : Int, c : Int, target : Qubit) : Unit {
            let u = if a >= 0 { if b >= 0 { if c >= 0 { S } else { T } } else { X } } else { Y };
            u(target);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyTriple(1, 2, 3, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 0
            call_sites: 0
            direct_call_sites: 4
              site: callee=S:Body, condition=ExprId(4) and ExprId(9) and ExprId(14)
              site: callee=T:Body, condition=ExprId(4) and ExprId(9)
              site: callee=X:Body, condition=ExprId(4)
              site: callee=Y:Body, default
            lattice states:
              callable ApplyTriple:
                5: Multi([S:Body, T:Body, X:Body, Y:Body])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyTriple(a : Int, b : Int, c : Int, target : Qubit) : Unit {
                let u : (Qubit => Unit is Adj + Ctl) = if a >= 0 {
                    if b >= 0 {
                        if c >= 0 {
                            S
                        } else {
                            T
                        }

                    } else {
                        X
                    }

                } else {
                    Y
                };
                u(target);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyTriple(1, 2, 3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyTriple(a : Int, b : Int, c : Int, target : Qubit) : Unit {
                if a >= 0 {
                    if b >= 0 {
                        if c >= 0 {
                            S(target)
                        } else {
                            T(target)
                        }
                    } else {
                        X(target)
                    }
                } else {
                    Y(target)
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyTriple(1, 2, 3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn callable_from_dual_nested_conditions() {
    let source = r#"
        operation ApplyDual(a : Int, b : Int, c : Int, target : Qubit) : Unit {
            let u = if a >= 0 { if b >= 0 { S } else { T } } else { if c >= 0 { X } else { Y } };
            u(target);
        }

        operation Main() : Unit {
            use q = Qubit();
            ApplyDual(1, 2, 3, q);
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
        callable_params: 0
        call_sites: 0
        direct_call_sites: 4
          site: callee=S:Body, condition=ExprId(4) and ExprId(9)
          site: callee=T:Body, condition=ExprId(4)
          site: callee=X:Body, condition=ExprId(18)
          site: callee=Y:Body, default
        lattice states:
          callable ApplyDual:
            5: Multi([S:Body, T:Body, X:Body, Y:Body])"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyDual(a : Int, b : Int, c : Int, target : Qubit) : Unit {
                let u : (Qubit => Unit is Adj + Ctl) = if a >= 0 {
                    if b >= 0 {
                        S
                    } else {
                        T
                    }

                } else {
                    if c >= 0 {
                        X
                    } else {
                        Y
                    }

                };
                u(target);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyDual(1, 2, 3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyDual(a : Int, b : Int, c : Int, target : Qubit) : Unit {
                if a >= 0 {
                    if b >= 0 {
                        S(target)
                    } else {
                        T(target)
                    }
                } else if c >= 0 {
                    X(target)
                } else {
                    Y(target)
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyDual(1, 2, 3, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn reaching_def_mutable_nested_branches() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { if false { set op = X; } else { set op = T; } } else { set op = S; }
            ApplyOp(op, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    if false {
                        op = X;
                    } else {
                        op = T;
                    }

                } else {
                    op = S;
                }

                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    if false {
                        op = X;
                    } else {
                        op = T;
                    }

                } else {
                    op = S;
                }

                if true {
                    if false {
                        ApplyOp_AdjCtl__X_(q)
                    } else {
                        ApplyOp_AdjCtl__T_(q)
                    }
                } else {
                    ApplyOp_AdjCtl__S_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_AdjCtl__T_(q : Qubit) : Unit {
                T(q);
            }
            operation ApplyOp_AdjCtl__S_(q : Qubit) : Unit {
                S(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// Defunctionalization must preserve a side-effecting selection even when run
/// without the earlier condition-normalization pass. Its guard snapshot runs
/// Y once; both the mutable assignment and subsequent dispatch read that value.
#[test]
fn callable_in_mutable_with_side_effects_in_if_expr() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if {Y(q); true} { set op = X; }
            ApplyOp(op, q);
        }
        "#;
    check_invariants(source);
    crate::test_utils::check_semantic_equivalence(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if {
                    Y(q);
                    true
                }
                {
                    op = X;
                }

                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                mutable __branch_guard : Bool = false;
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                {
                    __branch_guard = {
                        Y(q);
                        true
                    };
                    if __branch_guard {
                        op = X;
                    }

                }

                if __branch_guard {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__H_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// A HOF (`ApplyChoice`) takes a boolean parameter that selects a callable in a
/// mutable `set` reassignment's `if` condition. The mutable-flow `If` arm now
/// routes its branch condition through `remap_condition_expr` (matching the
/// immutable path) so a substituted condition local resolves to the caller's
/// expression. Here `flag` remains a live parameter of the specialized
/// operation, so it passes through unchanged; the test guards that the mutable
/// nested HOF dispatch preserves both branches (`X` and `Y`) and resolves to
/// `if flag { X(q) } else { Y(q) }` rather than dropping a branch.
#[test]
fn reaching_def_mutable_hof_param_substituted_condition() {
    let source = r#"
        operation ApplyChoice(inner : Qubit => Unit, flag : Bool, q : Qubit) : Unit {
            mutable op = inner;
            if flag { set op = X; } else { set op = Y; }
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyChoice(H, true, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyChoice(inner : (Qubit => Unit), flag : Bool, q : Qubit) : Unit {
                mutable op : (Qubit => Unit) = inner;
                if flag {
                    op = X;
                } else {
                    op = Y;
                }

                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyChoice_AdjCtl_(H, true, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyChoice_AdjCtl_(inner : (Qubit => Unit is Adj + Ctl), flag : Bool, q : Qubit) : Unit {
                mutable op : (Qubit => Unit is Adj + Ctl) = inner;
                if flag {
                    op = X;
                } else {
                    op = Y;
                }

                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyChoice(inner : (Qubit => Unit), flag : Bool, q : Qubit) : Unit {
                mutable op : (Qubit => Unit) = inner;
                if flag {
                    op = X;
                } else {
                    op = Y;
                }

                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyChoice_AdjCtl__H_(true, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyChoice_AdjCtl_(inner : (Qubit => Unit is Adj + Ctl), flag : Bool, q : Qubit) : Unit {
                mutable op : (Qubit => Unit is Adj + Ctl) = inner;
                if flag {
                    op = X;
                } else {
                    op = Y;
                }

                if flag {
                    X(q)
                } else {
                    Y(q)
                };
            }
            operation ApplyChoice_AdjCtl__H_(flag : Bool, q : Qubit) : Unit {
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if flag {
                    op = X;
                } else {
                    op = Y;
                }

                if flag {
                    X(q)
                } else {
                    Y(q)
                };
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn analysis_callable_from_tuple_destructured_array_iteration() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                let arr = [(S, PauliZ), (T, PauliX)];
                for (op, _basis) in arr {
                    use q = Qubit();
                    op(q);
                }
            }
        }
        "#;
    check_analysis(
        source,
        &expect![[r#"
            callable_params: 0
            call_sites: 0
            direct_call_sites: 2
              site: callee=S:Body, default
              site: callee=T:Body, default
            lattice states:
              callable Main:
                5: Dynamic"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let arr : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(S, PauliZ), (T, PauliX)];
                {
                    let _array_id_36 : ((Qubit => Unit is Adj + Ctl), Pauli)[] = arr;
                    let _len_id_40 : Int = Length(_array_id_36);
                    mutable _index_id_45 : Int = 0;
                    while _index_id_45 < _len_id_40 {
                        let (op : (Qubit => Unit is Adj + Ctl), _basis : Pauli) = _array_id_36[_index_id_45];
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        op(q);
                        _index_id_45 += 1;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let arr : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(S, PauliZ), (T, PauliX)];
                {
                    let _array_id_36 : ((Qubit => Unit is Adj + Ctl), Pauli)[] = arr;
                    let _len_id_40 : Int = Length(_array_id_36);
                    mutable _index_id_45 : Int = 0;
                    while _index_id_45 < _len_id_40 {
                        let (op : (Qubit => Unit is Adj + Ctl), _basis : Pauli) = _array_id_36[_index_id_45];
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        {
                            [(), ()][_index_id_45];
                            if (_index_id_45 == 0) or (_index_id_45 == -2) {
                                S(q)
                            } else {
                                T(q)
                            }
                        };
                        _index_id_45 += 1;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn resolve_captures_missing_binding_returns_none() {
    let package = Package {
        id: qsc_fir::fir::PackageId::default(),
        items: IndexMap::new(),
        entry: None,
        entry_exec_graph: qsc_fir::fir::ExecGraph::default(),
        blocks: IndexMap::new(),
        exprs: IndexMap::new(),
        pats: IndexMap::new(),
        stmts: IndexMap::new(),
    };
    let locals = LocalState::default();
    let missing_var = LocalVarId::from(99usize);

    let captures = resolve_captures(&package, &locals, &[missing_var], &FxHashSet::default());

    assert!(
        captures.is_none(),
        "missing capture bindings should degrade analysis instead of panicking"
    );
}

// ---------------------------------------------------------------------------
// Flow-analysis completeness tests.
//
// These cover operand-position and program-point sensitivity in the
// defunctionalize flow analysis (`analyze_expr_flow` /
// `collect_written_vars_expr` and program-point-sensitive call recording).
// Each test pairs a top-level `set` with the same reassignment nested in an
// operand-position child; both must specialize calls to the reaching
// definition.

/// A `set f = Bar` in a `BinOp` operand block is observed in evaluation order,
/// so the later `f(5)` specializes to the reaching definition `Bar` (it ran
/// before the call), matching the top-level-`set` case.
#[allow(clippy::too_many_lines)]
#[test]
fn operand_block_set_specializes_to_reaching_definition() {
    // Top-level `set f = Bar;` -> f(5) resolves to Bar.
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            f = Bar;
            let z = 1;
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                f = Bar;
                let z : Int = 1;
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                f = Bar;
                let z : Int = 1;
                Bar(5)
            }
            // entry
            Main()
        "#]],
    );

    // `set f = Bar` in the left operand block of `+ 1` -> f(5) resolves to Bar.
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            let z = { set f = Bar; 0 } + 1;
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let z : Int = {
                    f = Bar;
                    0
                } + 1;
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let z : Int = {
                    f = Bar;
                    0
                } + 1;
                Bar(5)
            }
            // entry
            Main()
        "#]],
    );
}

/// An operand-position `set f = Bar` inside a loop body forces `f` to
/// `Dynamic`, so the post-loop `f(5)` is not specialized. A `Dynamic` callable
/// consumed by a direct call leaves an unresolved value, surfacing an
/// actionable `DynamicCallable` error at the call site (the direct-path
/// mirror of the HOF diagnostic), exactly like the top-level-`set`-in-loop
/// case.
#[test]
fn loop_operand_block_set_forces_dynamic() {
    fn assert_forces_dynamic(context: &str, source: &str) {
        let (mut store, package_id) = compile_to_monomorphized_fir(source);
        let mut assigners = PackageAssigners::new(&store, package_id);
        let errors = defunctionalize(&mut store, package_id, &mut assigners).diagnostics;
        assert_eq!(
            errors.len(),
            1,
            "{context}: expected the loop-reassigned callable to be forced Dynamic \
             (one unresolved direct call), got: {}",
            format_defunctionalization_errors(&errors)
        );
        assert!(
            matches!(errors[0], super::super::Error::DynamicCallable(..)),
            "{context}: expected DynamicCallable error, got {:?}",
            errors[0]
        );
    }

    // Top-level `set` inside the loop body forces `f` Dynamic.
    assert_forces_dynamic(
        "top-level set in loop",
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            for i in 0..2 {
                f = Bar;
            }
            f(5)
        }
        "#,
    );

    // `set f = Bar` in an operand block inside the loop also forces `f` Dynamic,
    // producing the same error.
    assert_forces_dynamic(
        "operand-block set in loop",
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            for i in 0..2 {
                let z = { set f = Bar; 0 } + 1;
            }
            f(5)
        }
        "#,
    );
}

/// Straight-line call resolution is program-point-sensitive: the `f(1)` that
/// precedes `set f = Bar` specializes to `Foo`, while the later `f(2)`
/// specializes to `Bar`.
#[test]
fn straight_line_reassignment_is_position_sensitive() {
    // No reassignment: both calls resolve to `Foo`.
    check_rewrite(
        r#"
        operation Foo(x : Int) : Unit {}
        operation Bar(x : Int) : Unit {}
        operation Main() : Unit {
            mutable f = Foo;
            f(1);
            f(2);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Foo(x : Int) : Unit {}
            operation Bar(x : Int) : Unit {}
            operation Main() : Unit {
                mutable f : (Int => Unit) = Foo;
                f(1);
                f(2);
            }
            // entry
            Main()

            AFTER:
            operation Foo(x : Int) : Unit {}
            operation Bar(x : Int) : Unit {}
            operation Main() : Unit {
                mutable f : (Int => Unit) = Foo;
                Foo(1);
                Foo(2);
            }
            // entry
            Main()
        "#]],
    );

    // A call before and after a top-level `set f = Bar`.
    check_rewrite(
        r#"
        operation Foo(x : Int) : Unit {}
        operation Bar(x : Int) : Unit {}
        operation Main() : Unit {
            mutable f = Foo;
            f(1);
            f = Bar;
            f(2);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Foo(x : Int) : Unit {}
            operation Bar(x : Int) : Unit {}
            operation Main() : Unit {
                mutable f : (Int => Unit) = Foo;
                f(1);
                f = Bar;
                f(2);
            }
            // entry
            Main()

            AFTER:
            operation Foo(x : Int) : Unit {}
            operation Bar(x : Int) : Unit {}
            operation Main() : Unit {
                mutable f : (Int => Unit) = Foo;
                Foo(1);
                f = Bar;
                Bar(2);
            }
            // entry
            Main()
        "#]],
    );
}

/// Passing a mutable callable local to a higher-order function after an
/// operand-position `set` threads the reaching definition `Bar` into the
/// specialized `Apply` variant, matching the top-level-`set` case.
#[allow(clippy::too_many_lines)]
#[test]
fn hof_operand_block_set_specializes_reaching_definition() {
    // Top-level `set f = Bar;` -> specializes to `Apply_Bar_`.
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        function Apply(g : Int -> Int, x : Int) : Int { g(x) }
        operation Main() : Int {
            mutable f = Foo;
            f = Bar;
            let z = 1;
            Apply(f, 5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            function Apply(g : (Int -> Int), x : Int) : Int {
                g(x)
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                f = Bar;
                let z : Int = 1;
                Apply(f, 5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            function Apply(g : (Int -> Int), x : Int) : Int {
                g(x)
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                f = Bar;
                let z : Int = 1;
                Apply_Bar_(5)
            }
            function Apply_Bar_(x : Int) : Int {
                Bar(x)
            }
            // entry
            Main()
        "#]],
    );

    // `set f = Bar` in an operand block before the HOF call.
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        function Apply(g : Int -> Int, x : Int) : Int { g(x) }
        operation Main() : Int {
            mutable f = Foo;
            let z = { set f = Bar; 0 } + 1;
            Apply(f, 5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            function Apply(g : (Int -> Int), x : Int) : Int {
                g(x)
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let z : Int = {
                    f = Bar;
                    0
                } + 1;
                Apply(f, 5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            function Apply(g : (Int -> Int), x : Int) : Int {
                g(x)
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let z : Int = {
                    f = Bar;
                    0
                } + 1;
                Apply_Bar_(5)
            }
            function Apply_Bar_(x : Int) : Int {
                Bar(x)
            }
            // entry
            Main()
        "#]],
    );
}

/// An operand-position conditional `set` forms the `Multi` lattice so the call
/// emits multi-way dispatch (`if true { X } else { H }`), matching the
/// statement-level case.
#[allow(clippy::too_many_lines)]
/// A conditional `set op = X` (whether at statement level or inside an operand
/// block) leaves `op` with two reaching definitions, so the call site is
/// rewritten into a generated `if`-dispatch that selects the matching
/// specialization per branch.
#[test]
fn operand_block_conditional_set_generates_branch_dispatch() {
    // Statement-level conditional `set`: multi-way dispatch.
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { set op = X; }
            ApplyOp(op, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                }

                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                }

                if true {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__H_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );

    // The conditional `set` lives in an operand block of `+ 1`.
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            let z = (if true { set op = X; 0 } else { 0 }) + 1;
            ApplyOp(op, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                let z : Int = if true {
                    op = X;
                    0
                } else {
                    0
                } + 1;
                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                let z : Int = if true {
                    op = X;
                    0
                } else {
                    0
                } + 1;
                if true {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__H_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// When a branch contains a statement-level `set op = Y` followed by an
/// operand-position `set op = X`, the dispatch arm for that branch targets the
/// reaching definition `X`, matching the all-statement-level case.
#[allow(clippy::too_many_lines)]
#[test]
fn operand_block_set_in_branch_uses_correct_arm() {
    // Both sets at statement level: true arm dispatches X.
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true {
                set op = Y;
                set op = X;
            }
            ApplyOp(op, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = Y;
                    op = X;
                }

                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = Y;
                    op = X;
                }

                if true {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__H_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );

    // The second `set op = X` in an operand block.
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true {
                set op = Y;
                let z = ({ set op = X; 0 }) + 0;
            }
            ApplyOp(op, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = Y;
                    let z : Int = {
                        op = X;
                        0
                    } + 0;
                }

                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = Y;
                    let z : Int = {
                        op = X;
                        0
                    } + 0;
                }

                if true {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__H_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// An array-of-tuples index dispatch whose tuple-pattern binding
/// (`let (initializer, _basis) = ops[i]`) is nested inside an operand-position
/// block resolves its tuple field path, so the indexed call is rewritten into
/// an `if`/`else` index dispatch.
#[allow(clippy::too_many_lines)]
#[test]
fn operand_block_tuple_pattern_dispatch_resolves_field_path() {
    check_rewrite(
        r#"
        operation Main() : Unit {
            let ops = [(I, PauliZ), (X, PauliZ)];
            for i in 0..1 {
                use q = Qubit();
                let z = { let (initializer, _basis) = ops[i]; initializer(q); 0 } + 1;
            }
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let ops : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(I, PauliZ), (X, PauliZ)];
                {
                    let _range_id_53 : Range = 0..1;
                    mutable _index_id_56 : Int = _range_id_53.Start;
                    let _step_id_61 : Int = _range_id_53.Step;
                    let _end_id_66 : Int = _range_id_53.End;
                    while ((_step_id_61 > 0) and (_index_id_56 <= _end_id_66)) or ((_step_id_61 < 0) and (_index_id_56 >= _end_id_66)) {
                        let i : Int = _index_id_56;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let z : Int = {
                            let (initializer : (Qubit => Unit is Adj + Ctl), _basis : Pauli) = ops[i];
                            initializer(q);
                            0
                        } + 1;
                        _index_id_56 += _step_id_61;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let ops : ((Qubit => Unit is Adj + Ctl), Pauli)[] = [(I, PauliZ), (X, PauliZ)];
                {
                    let _range_id_53 : Range = 0..1;
                    mutable _index_id_56 : Int = _range_id_53.Start;
                    let _step_id_61 : Int = _range_id_53.Step;
                    let _end_id_66 : Int = _range_id_53.End;
                    while ((_step_id_61 > 0) and (_index_id_56 <= _end_id_66)) or ((_step_id_61 < 0) and (_index_id_56 >= _end_id_66)) {
                        let i : Int = _index_id_56;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let z : Int = {
                            let (initializer : (Qubit => Unit is Adj + Ctl), _basis : Pauli) = ops[i];
                            {
                                [(), ()][i];
                                if (i == 0) or (i == -2) {
                                    I(q)
                                } else {
                                    X(q)
                                }
                            };
                            0
                        } + 1;
                        _index_id_56 += _step_id_61;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

/// When the index selecting a callable from an array is pure arithmetic, the
/// synthesized dispatch can reuse the original expression in each guard. This
/// keeps "non-trivial" from being mistaken for "effectful"; the separate
/// side-effecting index tests cover the single-evaluation hoist path.
#[test]
fn pure_arithmetic_array_index_dispatch_reuses_index_expression() {
    let source = r#"
        operation Main() : Unit {
            let ops = [I, X, Y];
            for i in 0..1 {
                use q = Qubit();
                let op = ops[i + 1];
                op(q);
            }
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let ops : (Qubit => Unit is Adj + Ctl)[] = [I, X, Y];
                {
                    let _range_id_40 : Range = 0..1;
                    mutable _index_id_43 : Int = _range_id_40.Start;
                    let _step_id_48 : Int = _range_id_40.Step;
                    let _end_id_53 : Int = _range_id_40.End;
                    while ((_step_id_48 > 0) and (_index_id_43 <= _end_id_53)) or ((_step_id_48 < 0) and (_index_id_43 >= _end_id_53)) {
                        let i : Int = _index_id_43;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let op : (Qubit => Unit is Adj + Ctl) = ops[i + 1];
                        op(q);
                        _index_id_43 += _step_id_48;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let ops : (Qubit => Unit is Adj + Ctl)[] = [I, X, Y];
                {
                    let _range_id_40 : Range = 0..1;
                    mutable _index_id_43 : Int = _range_id_40.Start;
                    let _step_id_48 : Int = _range_id_40.Step;
                    let _end_id_53 : Int = _range_id_40.End;
                    while ((_step_id_48 > 0) and (_index_id_43 <= _end_id_53)) or ((_step_id_48 < 0) and (_index_id_43 >= _end_id_53)) {
                        let i : Int = _index_id_43;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let op : (Qubit => Unit is Adj + Ctl) = ops[i + 1];
                        {
                            [(), (), ()][i + 1];
                            if ((i + 1) == 0) or ((i + 1) == -3) {
                                I(q)
                            } else if ((i + 1) == 1) or ((i + 1) == -2) {
                                X(q)
                            } else {
                                Y(q)
                            }
                        };
                        _index_id_43 += _step_id_48;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

/// A pure, non-trivial index expression does not need the single-evaluation
/// hoist used for side-effecting indices. Reusing the original block
/// expression keeps index-dispatch cleanup aligned with the shared purity
/// predicate.
#[test]
fn pure_array_index_dispatch_reuses_index_expression() {
    let source = r#"
        operation Main() : Unit {
            let ops = [I, X, Y];
            for i in 0..1 {
                use q = Qubit();
                let op = ops[{ i }];
                op(q);
            }
        }
        "#;

    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners).diagnostics;
    assert_no_defunctionalization_errors("defunctionalization", &errors);

    let after = crate::pretty::write_package_qsharp_parseable(&fir_store, fir_pkg_id);
    assert!(
        after.contains("if (i == 0) or (i == -3)")
            && after.contains("else if (i == 1) or (i == -2)"),
        "pure index dispatch should reuse the original block index:\n{after}"
    );
    assert!(
        !after.contains("let index : Int ="),
        "pure index dispatch should not hoist a side-effect-free block:\n{after}"
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let ops : (Qubit => Unit is Adj + Ctl)[] = [I, X, Y];
                {
                    let _range_id_41 : Range = 0..1;
                    mutable _index_id_44 : Int = _range_id_41.Start;
                    let _step_id_49 : Int = _range_id_41.Step;
                    let _end_id_54 : Int = _range_id_41.End;
                    while ((_step_id_49 > 0) and (_index_id_44 <= _end_id_54)) or ((_step_id_49 < 0) and (_index_id_44 >= _end_id_54)) {
                        let i : Int = _index_id_44;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let op : (Qubit => Unit is Adj + Ctl) = ops[{
                            i
                        }];
                        op(q);
                        _index_id_44 += _step_id_49;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let ops : (Qubit => Unit is Adj + Ctl)[] = [I, X, Y];
                {
                    let _range_id_41 : Range = 0..1;
                    mutable _index_id_44 : Int = _range_id_41.Start;
                    let _step_id_49 : Int = _range_id_41.Step;
                    let _end_id_54 : Int = _range_id_41.End;
                    while ((_step_id_49 > 0) and (_index_id_44 <= _end_id_54)) or ((_step_id_49 < 0) and (_index_id_44 >= _end_id_54)) {
                        let i : Int = _index_id_44;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let op : (Qubit => Unit is Adj + Ctl) = ops[{
                            i
                        }];
                        {
                            [(), (), ()][{
                                i
                            }];
                            if (i == 0) or (i == -3) {
                                I(q)
                            } else if (i == 1) or (i == -2) {
                                X(q)
                            } else {
                                Y(q)
                            }
                        };
                        _index_id_44 += _step_id_49;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

/// Hoisting a side-effecting index into an `index` local prevents its `X(q)`
/// call from being repeated by each synthesized branch guard. This raw-pass
/// snapshot also retains the original `op` initializer and its index evaluation,
/// so it checks guard reuse, not single evaluation across the entire block.
#[test]
fn impure_array_index_dispatch_hoists_index_expression() {
    let source = r#"
        operation Main() : Unit {
            let ops = [I, X, Y];
            for i in 0..1 {
                use q = Qubit();
                let op = ops[{ X(q); i }];
                op(q);
            }
        }
        "#;

    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners).diagnostics;
    assert_no_defunctionalization_errors("defunctionalization", &errors);

    let after = crate::pretty::write_package_qsharp_parseable(&fir_store, fir_pkg_id);
    assert!(
        after.contains("let index : Int = {")
            && after.contains("if (index == 0) or (index == -3)")
            && after.contains("else if (index == 1) or (index == -2)"),
        "side-effecting index should be hoisted and reused by the dispatch:\n{after}"
    );
    assert!(
        !after.contains("if (i == 0)") && !after.contains("else if (i == 1)"),
        "dispatch guards should not re-evaluate the side-effecting index block:\n{after}"
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let ops : (Qubit => Unit is Adj + Ctl)[] = [I, X, Y];
                {
                    let _range_id_45 : Range = 0..1;
                    mutable _index_id_48 : Int = _range_id_45.Start;
                    let _step_id_53 : Int = _range_id_45.Step;
                    let _end_id_58 : Int = _range_id_45.End;
                    while ((_step_id_53 > 0) and (_index_id_48 <= _end_id_58)) or ((_step_id_53 < 0) and (_index_id_48 >= _end_id_58)) {
                        let i : Int = _index_id_48;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let op : (Qubit => Unit is Adj + Ctl) = ops[{
                            X(q);
                            i
                        }];
                        op(q);
                        _index_id_48 += _step_id_53;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let ops : (Qubit => Unit is Adj + Ctl)[] = [I, X, Y];
                {
                    let _range_id_45 : Range = 0..1;
                    mutable _index_id_48 : Int = _range_id_45.Start;
                    let _step_id_53 : Int = _range_id_45.Step;
                    let _end_id_58 : Int = _range_id_45.End;
                    while ((_step_id_53 > 0) and (_index_id_48 <= _end_id_58)) or ((_step_id_53 < 0) and (_index_id_48 >= _end_id_58)) {
                        let i : Int = _index_id_48;
                        let q : Qubit = __quantum__rt__qubit_allocate();
                        let index : Int = {
                            X(q);
                            i
                        };
                        let op : (Qubit => Unit is Adj + Ctl) = ops[index];
                        {
                            [(), (), ()][index];
                            if (index == 0) or (index == -3) {
                                I(q)
                            } else if (index == 1) or (index == -2) {
                                X(q)
                            } else {
                                Y(q)
                            }
                        };
                        _index_id_48 += _step_id_53;
                        __quantum__rt__qubit_release(q);
                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

/// A `set f = Bar` inside the short-circuited RHS of `false and { .. }` is not
/// executed at runtime, so the later `f(5)` must keep the reaching definition
/// `Foo`. The fork/join arm applies the RHS conditionally rather than
/// unconditionally overwriting the lattice.
#[test]
fn binop_andl_short_circuit_rhs_set_does_not_reach_call() {
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            let b = false and { set f = Bar; true };
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let b : Bool = false and {
                    f = Bar;
                    true
                };
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let b : Bool = false and {
                    f = Bar;
                    true
                };
                if false {
                    Bar(5)
                } else {
                    Foo(5)
                }
            }
            // entry
            Main()
        "#]],
    );
}

/// A `set f = Bar` inside the short-circuited RHS of `true or { .. }` is not
/// executed at runtime, so the later `f(5)` must keep the reaching definition
/// `Foo` (same fork/join arm as `and`).
#[test]
fn binop_orl_short_circuit_rhs_set_does_not_reach_call() {
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            let b = true or { set f = Bar; false };
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let b : Bool = true or {
                    f = Bar;
                    false
                };
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let b : Bool = true or {
                    f = Bar;
                    false
                };
                if true {
                    Foo(5)
                } else {
                    Bar(5)
                }
            }
            // entry
            Main()
        "#]],
    );
}

/// A `set f = Bar` inside the short-circuited RHS of a logical compound-assign
/// `set b and= { .. }` (a distinct `AssignOp` arm) is not executed when the LHS
/// short-circuits, so the later `f(5)` must keep the reaching definition `Foo`.
#[test]
fn assignop_andl_short_circuit_rhs_set_does_not_reach_call() {
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            mutable b = false;
            set b and= { set f = Bar; false };
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                mutable b : Bool = false;
                b and= {
                    f = Bar;
                    false
                };
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable __branch_guard : Bool = false;
                mutable f : (Int -> Int) = Foo;
                mutable b : Bool = false;
                {
                    __branch_guard = b;
                    b = __branch_guard and {
                        f = Bar;
                        false
                    }

                };
                if __branch_guard {
                    Bar(5)
                } else {
                    Foo(5)
                }
            }
            // entry
            Main()
        "#]],
    );
}

/// In `(new Rec { A = f(5), B = 0 }) w/ B <- { set f = Bar; 0 }`, runtime
/// evaluates the replace operand (`set f = Bar`) before the record operand
/// (which contains `f(5)`), so the call resolves to the reaching definition
/// `Bar`. The reordered `UpdateField` arm recurses replace-then-record.
#[test]
fn update_field_replace_then_record_order_reaches_call() {
    check_rewrite(
        r#"
        struct Rec { A : Int, B : Int }
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            let r = (new Rec { A = f(5), B = 0 }) w/ B <- { set f = Bar; 0 };
            r.A
        }
        "#,
        &expect![[r#"
            BEFORE:
            newtype Rec = (Int, Int);
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let r : __UDT_Item_1__Package_2_ = new Rec {
                    A = f(5),
                    B = 0
                }
                    w/::B <- {
                    f = Bar;
                    0
                };
                r::A
            }
            // entry
            Main()

            AFTER:
            newtype Rec = (Int, Int);
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let r : __UDT_Item_1__Package_2_ = new Rec {
                    A = Bar(5),
                    B = 0
                }
                    w/::B <- {
                    f = Bar;
                    0
                };
                r::A
            }
            // entry
            Main()
        "#]],
    );
}

/// In `[f(5), 0] w/ 1 <- { set f = Bar; 0 }`, runtime evaluates the index then
/// the replace operand (`set f = Bar`) before the container operand (which
/// contains `f(5)`), so the call resolves to the reaching definition `Bar`.
/// The reordered `UpdateIndex` arm recurses index-replace-container(last).
#[test]
fn update_index_container_last_order_reaches_call() {
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            let arr = [f(5), 0] w/ 1 <- { set f = Bar; 0 };
            arr[0]
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let arr : Int[] = [f(5), 0] w/ 1 <- {
                    f = Bar;
                    0
                };
                arr[0]
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                let arr : Int[] = [Bar(5), 0] w/ 1 <- {
                    f = Bar;
                    0
                };
                arr[0]
            }
            // entry
            Main()
        "#]],
    );
}

/// Guard: a non-logical compound-assign (`+=`) executes its RHS
/// unconditionally at runtime, so the `set f = Bar` in `set acc += { .. }` does
/// reach the later `f(5)` (resolving to `Bar`). This confirms the `AssignOp`
/// match split did not accidentally route non-logical operators through the
/// conditional fork/join arm.
#[test]
fn assignop_non_logical_rhs_set_reaches_call() {
    check_rewrite(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            mutable f = Foo;
            mutable acc = 0;
            set acc += { set f = Bar; 1 };
            f(5)
        }
        "#,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                mutable acc : Int = 0;
                acc += {
                    f = Bar;
                    1
                };
                f(5)
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                mutable f : (Int -> Int) = Foo;
                mutable acc : Int = 0;
                acc += {
                    f = Bar;
                    1
                };
                Bar(5)
            }
            // entry
            Main()
        "#]],
    );
}

/// With a runtime-dynamic `or` condition, the fork/join produces a
/// condition-tagged `Multi` lattice entry that flows through branch-split
/// dispatch. The `OrL` branches are ordered so dispatch is
/// `if cond { Foo(5) } else { Bar(5) }`: when the condition is true the `or`
/// short-circuits (RHS not run, `f` stays `Foo`); when false the RHS runs and
/// `f = Bar`. Confirms the `OrL` branch ordering end-to-end with no
/// `DynamicCallable`/`FixpointNotReached` regression.
#[test]
fn orl_runtime_dynamic_condition_branch_split_dispatch() {
    check_rewrite_with_capabilities(
        r#"
        function Foo(x : Int) : Int { x + 1 }
        function Bar(x : Int) : Int { x + 100 }
        operation Main() : Int {
            use q = Qubit();
            mutable f = Foo;
            let cond = MResetZ(q) == One;
            let b = cond or { set f = Bar; false };
            f(5)
        }
        "#,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
        &expect![[r#"
            BEFORE:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable f : (Int -> Int) = Foo;
                let cond : Bool = MResetZ(q) == One;
                let b : Bool = cond or {
                    f = Bar;
                    false
                };
                let _generated_ident_67 : Int = f(5);
                __quantum__rt__qubit_release(q);
                _generated_ident_67
            }
            // entry
            Main()

            AFTER:
            function Foo(x : Int) : Int {
                x + 1
            }
            function Bar(x : Int) : Int {
                x + 100
            }
            operation Main() : Int {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable f : (Int -> Int) = Foo;
                let cond : Bool = MResetZ(q) == One;
                let b : Bool = cond or {
                    f = Bar;
                    false
                };
                let _generated_ident_67 : Int = if cond {
                    Foo(5)
                } else {
                    Bar(5)
                };
                __quantum__rt__qubit_release(q);
                _generated_ident_67
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn another_callable_parameter_does_not_supply_capture_type() {
    let (store, package_id) = compile_to_monomorphized_fir(
        r#"
        function Other(value : Double) : Double { value }
        operation Main() : Double { Other(2.0) }
        "#,
    );
    let package = store.get(package_id);
    let input = package.get_pat(callable_decl(package, "Other").input);
    let PatKind::Bind(parameter) = &input.kind else {
        panic!("Other should bind its parameter");
    };
    assert_eq!(input.ty, Ty::Prim(Prim::Double));

    // The ID exists in the package, but not in the capture's scope.
    let locals = LocalState::default();
    let captures = resolve_captures(package, &locals, &[parameter.id], &FxHashSet::default());

    assert!(
        captures.is_none(),
        "another callable's parameter must not replace missing scoped type evidence",
    );
}

/// Specializing a call site deletes the callable argument expression from the
/// call, and the argument-removal family that performs the deletion carries no
/// purity guard of its own. An inline producer call is therefore classified by
/// `consumed_callable_expr_disposition` before the call site is accepted: here
/// `GetOp` applies `X` before returning the callable it produces, nothing
/// relocates or replays that `X`, and no binding exists to retain it, so the
/// disposition is `Retained` and the call site is declined.
///
/// Before that gate existed the argument was dropped outright and the `X`
/// silently disappeared, changing the program's measured result. Declining
/// converts a wrong answer into an actionable diagnostic.
#[test]
fn inline_effectful_producer_callable_argument_is_declined() {
    check_errors(
        r#"
        operation GetOp(q : Qubit) : (Qubit => Unit) {
            X(q);
            X
        }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(GetOp(q), q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

/// The same gate for a producer that is pure but *fallible*. `GetOp` is a
/// `function` with no effects, so it passes side-effect freedom, but `1 /
/// divisor` can fail and the discard proof requires totality. Dropping the
/// argument turned a program that failed with a division error into one that
/// succeeded.
#[test]
fn inline_fallible_factory_callable_argument_is_declined() {
    check_errors(
        r#"
        function GetOp(divisor : Int) : Qubit => Unit {
            let ignored = 1 / divisor;
            X
        }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(GetOp(0), q);
        }
        "#,
        &expect!["callable argument could not be resolved statically"],
    );
}

/// The gate must not decline a producer it can prove pure and total. `MakeOp`
/// has no effects and cannot fail, so its evaluation is `Discarded` and the
/// call site specializes exactly as before, with the inline argument removed.
#[test]
fn inline_total_factory_callable_argument_still_specializes() {
    check_errors(
        r#"
        function MakeOp() : Qubit => Unit {
            X
        }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(MakeOp(), q);
        }
        "#,
        &expect!["(no error)"],
    );
}

#[test]
fn capture_admissibility_assignment_store_order() {
    let sources = [
        (
            r#"
        operation Main() : Unit {
            use q = Qubit();
            mutable angle = 0.0;
            let op = Rx(angle, _);
            set angle = { op(q); 3.141592653589793 };
            op(q);
        }
        "#,
            2,
        ),
        (
            r#"
        operation Main() : Unit {
            use q = Qubit();
            mutable angle = 0.0;
            let op = Rx(angle, _);
            set angle += { op(q); 3.141592653589793 };
            op(q);
        }
        "#,
            2,
        ),
        (
            r#"
        newtype Config = (Angle : Double);
        operation Main() : Unit {
            use q = Qubit();
            mutable config = Config(0.0);
            let op = Rx(config::Angle, _);
            set config w/= Angle <- { op(q); 3.141592653589793 };
            op(q);
        }
        "#,
            2,
        ),
        (
            r#"
        operation Main() : Unit {
            use q = Qubit();
            mutable angles = [0.0];
            let op = Rx(angles[0], _);
            set angles w/= 0 <- { op(q); 3.141592653589793 };
            op(q);
        }
        "#,
            0,
        ),
    ];

    // Replaying a mutable operand is declined even before a write. The fallible
    // index read instead gets an immutable capture binding during the prepass,
    // so its stored value remains admissible on both sides of the assignment.
    for (source, expected_unresolved) in sources {
        let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(source);
        let result = super::run_prepass_and_analysis(&mut fir_store, fir_pkg_id);
        let package = fir_store.get(fir_pkg_id);
        let direct_op_calls = result
            .direct_call_sites
            .iter()
            .filter(|site| {
                let span = package.get_expr(site.call_expr_id).span;
                &source[span.lo as usize..span.hi as usize] == "op(q)"
            })
            .count();
        let unresolved_op_calls = result
            .unresolved_direct_call_sites
            .iter()
            .filter(|site| {
                let span = package.get_expr(site.expr).span;
                &source[span.lo as usize..span.hi as usize] == "op(q)"
            })
            .count();
        assert_eq!(
            unresolved_op_calls, expected_unresolved,
            "replayed mutable operands must remain unresolved"
        );
        assert_eq!(
            direct_op_calls,
            2 - expected_unresolved,
            "stored indexed captures must be recordable before and after the store"
        );
    }
}
