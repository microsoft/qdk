// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Many tests pair a primary assertion with a `check_rewrite` before/after
// snapshot, so the generated Q# pushes function bodies past the line limit.
#![allow(clippy::too_many_lines)]

use crate::{
    defunctionalize::specialize::CAPTURE_NAME_PREFIX, package_assigners::PackageAssigners,
};

use super::*;
use expect_test::expect;

#[test]
fn nested_factory_captures_refresh_after_inner_rewrites_introduce_locals() {
    for (body, expected) in [
        ("Apply(Compose(h::G, Twice(Adder(3))), Mark(2))", [12, 12]),
        ("Twice(Compose(h::G, Adder(3)))(Mark(2))", [16, 16]),
        (
            "let f = if flag { Compose(h::G, Adder(3)) } else { Adder(8) }; Apply(f, 2)",
            [10, 9],
        ),
        (
            "let holder = new Holder { F = Compose(h::G, Adder(3)), Tag = 5 }; Apply(holder.F, 2)",
            [9, 9],
        ),
        ("let f = Compose(h::G, Adder(3)); Apply(f, f(2))", [16, 16]),
        (
            "mutable f = h::G; if flag { set f = Compose(h::G, Adder(3)); } else { set f = Adder(8); } f(2)",
            [10, 9],
        ),
    ] {
        for (flag, expected) in [(false, expected[0]), (true, expected[1])] {
            for padding in ["", "function Unused(x : Int) : (Int, Int) { (x+1,x+2) }"] {
                let source = format!(
                    r#"
                    {padding}
                    function Add(a : Int, b : Int) : Int {{ a + b }}
                    function Adder(k : Int) : Int -> Int {{ x -> k + x }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    function Twice(f : Int -> Int) : Int -> Int {{ x -> f(f(x)) }}
                    function Compose(f : Int -> Int, g : Int -> Int) : Int -> Int {{ x -> f(g(x)) }}
                    function Mark(x : Int) : Int {{ Message($"argument {{x}}"); x }}
                    newtype Box = (G : Int -> Int, Tag : Int);
                    struct Holder {{ F : Int -> Int, Tag : Int }}
                    function Run(flag : Bool) : Int {{ let h = Box(Add(4, _), 7); {body} }}
                    @EntryPoint() operation Main() : Int {{ Run({flag}) }}
                "#
                );
                // An inner Compose rewrite saves h::G in a new local. The outer
                // use must refresh capture facts even if its own callee is an alias.
                crate::test_utils::check_semantic_equivalence_with_expected(
                    &source,
                    qsc_eval::val::Value::Int(expected),
                );
                assert_specialization_call_abis(&source);
                assert_specialized_int_qir(&source, expected);
            }
        }
    }
}

#[test]
fn nested_higher_order_arguments_preserve_inner_layouts_and_captures() {
    for (source, expected) in [
        (
            r#"
            function Add(a : Int, b : Int) : Int { a + b }
            function Dbl(x : Int) : Int { 2 * x }
            operation Main() : Int {
                Std.Arrays.Fold(Add, 0, Std.Arrays.Mapped(Dbl, [1, 1, 2]))
            }
        "#,
            8,
        ),
        (
            r#"
            newtype T = (G : Int -> Int, M : Int);
            function ApplyTwice(f : Int -> Int, x : Int) : Int { f(f(x)) }
            function Dec(x : Int) : Int { x - 2 }
            function CapT(t : T) : (Int -> Int) { x -> t::G(x) }
            operation Main() : Int { ApplyTwice(CapT(T(Dec, 2)), 0) }
        "#,
            -4,
        ),
        (
            r#"
            function Apply(f : Int -> Int, x : Int) : Int { f(x) }
            function Adder(k : Int) : (Int -> Int) { x -> x + k }
            operation Main() : Int { Apply(Adder(1), Adder(2)(3)) }
        "#,
            6,
        ),
        (
            r#"
            function Mul(a : Int, b : Int) : Int { a * b }
            operation Main() : Int {
                let c4 = 4;
                let pr3 = x -> x * c4;
                mutable f5 = Mul(2, _);
                pr3(f5(1))
            }
        "#,
            8,
        ),
    ] {
        for padding in ["", "function Unused(x : Int) : Int { (x + 1) * 3 }"] {
            let source = format!("{padding}\n{source}");
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(expected),
            );
            assert_specialization_call_abis(&source);
            assert_specialized_int_qir(&source, expected);
        }
    }
}

#[test]
fn nested_quantum_higher_order_arguments_preserve_mapped_qubits_and_sorted_comparator() {
    for (source, expected) in [
        (
            r#"
            import Std.Arrays.*;
            operation Main() : Result[] {
                use qs = Qubit[3];
                ApplyToEach(X, Mapped(q -> q, qs));
                MResetEachZ(qs)
            }
        "#,
            qsc_eval::val::Value::Array(
                vec![qsc_eval::val::Value::Result(qsc_eval::val::Result::Val(true)); 3].into(),
            ),
        ),
        (
            r#"
            function ByRemainder(remainders : Double[]) : Int[] {
                Std.Arrays.Sorted(
                    (i, j) -> remainders[i] > remainders[j] or
                        (remainders[i] == remainders[j] and i <= j),
                    Std.Arrays.MappedOverRange(i -> i, 0..Length(remainders) - 1)
                )
            }
            operation Main() : Result {
                use q = Qubit();
                let order = ByRemainder([0.25, 0.75, 0.5]);
                if order[0] == 1 and order[1] == 2 and order[2] == 0 { X(q); }
                MResetZ(q)
            }
        "#,
            qsc_eval::val::Value::Result(qsc_eval::val::Result::Val(true)),
        ),
    ] {
        crate::test_utils::check_semantic_equivalence_with_expected(source, expected);
        assert_specialization_call_abis(source);
    }
}

#[test]
fn nested_higher_order_rewrites_preserve_effects_and_controlled_preparation() {
    for (body, expected, output) in [
        (
            "Apply(Adder(1), Adder(2)(Mark(3)))",
            6,
            "argument 3\napply\n",
        ),
        (
            "Apply(Adder(1), Apply(Adder(2), Apply(Adder(3), Mark(4))))",
            10,
            "argument 4\napply\napply\napply\n",
        ),
    ] {
        let source = format!(
            r#"
            function Apply(f : Int -> Int, x : Int) : Int {{ Message("apply"); f(x) }}
            function Adder(k : Int) : (Int -> Int) {{ x -> x + k }}
            function Mark(k : Int) : Int {{ Message($"argument {{k}}"); k }}
            operation Main() : Int {{ {body} }}
        "#
        );
        crate::test_utils::check_semantic_equivalence_with_expected_output(
            &source,
            qsc_eval::val::Value::Int(expected),
            output,
        );
        assert_specialization_call_abis(&source);
        assert_specialized_int_qir(&source, expected);
    }
    let source = r#"
        operation UserPrep(register : Qubit[]) : Unit is Adj + Ctl {
            body ... { Std.Canon.ApplyToEach(X, Std.Arrays.Mapped(q -> q, register)); }
            adjoint self;
            controlled (controls, ...) {
                for q in Std.Arrays.Mapped(q -> q, register) { Controlled X(controls, q); }
            }
            controlled adjoint self;
        }
        @EntryPoint() operation Main() : Result[] {
            use qs = Qubit[2];
            use control = Qubit();
            UserPrep(qs);
            Adjoint UserPrep(qs);
            X(control);
            Controlled UserPrep([control], qs);
            Reset(control);
            MResetEachZ(qs)
        }
    "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Array(
            vec![qsc_eval::val::Value::Result(qsc_eval::val::Result::Val(true)); 2].into(),
        ),
    );
    assert_specialization_call_abis(source);
}

#[test]
fn call_rewrite_postorder_visits_shared_block_children_before_lower_numbered_parents() {
    use crate::fir_builder::{alloc_block, alloc_expr, alloc_expr_stmt, alloc_unit_expr};
    use qsc_fir::{assigner::Assigner, fir::ExprKind, ty::Ty};

    let mut package = fir::Package::default();
    let mut assigner = Assigner::new();
    let span = package.synthetic_span();
    let parent = alloc_unit_expr(&mut package, &mut assigner, span);
    let child = alloc_unit_expr(&mut package, &mut assigner, span);
    let stmt = alloc_expr_stmt(&mut package, &mut assigner, child, span);
    let block = alloc_block(&mut package, &mut assigner, vec![stmt], Ty::UNIT, span);
    let nested = alloc_expr(
        &mut package,
        &mut assigner,
        Ty::UNIT,
        ExprKind::Block(block),
        span,
    );
    package.exprs.get_mut(parent).expect("parent").kind = ExprKind::Tuple(vec![nested, child]);
    let ordered = crate::walk_utils::expressions_in_postorder(&package);
    assert_eq!(ordered, vec![child, nested, parent]);
}

#[test]
fn church_numeral_composition_preserves_values_and_messages() {
    // Church numerals encode repeated function application. Keep the complete
    // composition and assert its numeric results through exact messages even
    // though Main itself returns Unit.
    crate::test_utils::check_semantic_equivalence_with_expected_output(
        r#"
        function ChurchZero() : (Int -> Int) -> (Int -> Int) {
            f -> (x -> x)
        }
        function Succ(n : (Int -> Int) -> (Int -> Int)) : (Int -> Int) -> (Int -> Int) {
            f -> (x -> f(n(f)(x)))
        }
        function Add(m : (Int -> Int) -> (Int -> Int), n : (Int -> Int) -> (Int -> Int)) : (Int -> Int) -> (Int -> Int) {
            f -> (x -> m(f)(n(f)(x)))
        }
        function Mult(m : (Int -> Int) -> (Int -> Int), n : (Int -> Int) -> (Int -> Int)) : (Int -> Int) -> (Int -> Int) {
            f -> m(n(f))
        }
        function ToInt(n : (Int -> Int) -> (Int -> Int)) : Int { (n(i -> i + 1))(0) }
        @EntryPoint()
        function Main() : Unit {
            let two = Succ(Succ(ChurchZero()));
            let three = Succ(two);
            let five = Add(two, three);
            let six = Mult(two, three);
            Message($"two   = {ToInt(two)}");
            Message($"three = {ToInt(three)}");
            Message($"2 + 3 = {ToInt(five)}");
            Message($"2 * 3 = {ToInt(six)}");
            Message($"three(double)(1) = {(three(x -> x * 2))(1)}");
        }
    "#,
        qsc_eval::val::Value::unit(),
        "two   = 2\nthree = 3\n2 + 3 = 5\n2 * 3 = 6\nthree(double)(1) = 8\n",
    );
}

#[test]
fn live_factory_closure_remains_invocable_after_another_use_is_specialized() {
    let source = r#"
            function Neg(x : Int) : Int { -x }
            function Twice(f : Int -> Int) : (Int -> Int) { x -> f(f(x)) }
            function Ap2(f : (Int, Int) -> Int, x : Int) : Int { f(x, 1) }
            operation Main() : Result {
                let g = Neg;
                use q = Qubit();
                if Ap2((a, b) -> Twice(g)(a) + b, 5) == 6 { X(q); }
                MResetZ(q)
            }
        "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Result(qsc_eval::val::Result::Val(true)),
    );
    assert_specialization_call_abis(source);
}

#[test]
fn composed_partial_captures_preserve_caller_scope_and_return_negative_one() {
    let source = r#"
            function Add(a : Int, b : Int) : Int { a + b }
            function Twice(f : Int -> Int) : (Int -> Int) { x -> f(f(x)) }
            function Compose(f : Int -> Int, g : Int -> Int) : (Int -> Int) { x -> f(g(x)) }
            function MakeAdder(k : Int) : (Int -> Int) { x -> x + k }
            newtype NBox = (G : Int -> Int, J : Int);
            operation Main() : Int {
                let bx1 = NBox(Add(4, _), 5);
                Compose(bx1::G, Twice(MakeAdder(-3)))(1)
            }
        "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(-1),
    );
    assert_specialization_call_abis(source);
    assert_specialized_int_qir(source, -1);
}

#[test]
fn branch_local_callable_guard_preserves_scope_and_selection() {
    for n in [0, 1, 3] {
        let source = format!(
            r#"
            operation Apply(n : Int, q : Qubit) : Unit {{
                mutable op = X;
                if n > 0 {{
                    let b = n > 2;
                    if b {{ set op = H; }}
                }}
                op(q);
            }}
            @EntryPoint() operation Main() : Result {{
                use q = Qubit();
                Apply({n}, q);
                {undo}
                MResetZ(q)
            }}
        "#,
            undo = if n > 2 { "H(q);" } else { "X(q);" }
        );
        // Undo the selected gate so the expected result is deterministic even
        // for the original H branch; the trace still checks the actual choice.
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Result(qsc_eval::val::Result::Val(false)),
        );
        let qir = crate::test_utils::generate_qir(&source);
        let gate = if n > 2 {
            "__quantum__qis__h__body"
        } else {
            "__quantum__qis__x__body"
        };
        assert_eq!(
            qir.matches(&format!("call void @{gate}")).count(),
            2,
            "{qir}"
        );
    }
}

#[test]
fn foreign_factory_callees_keep_live_closures_and_branch_guard_scope() {
    let library = r#"
        namespace Lib {
            function Neg(x : Int) : Int { -x }
            function Twice(f : Int -> Int) : (Int -> Int) { x -> f(f(x)) }
            function Ap2(f : (Int, Int) -> Int, x : Int) : Int { f(x, 1) }
            function Compute() : Int {
                let g = Neg;
                Ap2((a, b) -> Twice(g)(a) + b, 5)
            }
            operation Apply(n : Int, q : Qubit) : Unit {
                mutable selected = X;
                if n > 0 {
                    let enabled = n > 2;
                    if enabled { set selected = H; }
                }
                selected(q);
            }
            export Compute, Apply;
        }
    "#;
    let source = r#"
        @EntryPoint() operation Main() : Result {
            use q = Qubit();
            if Lib.Compute() != 6 { fail "lost factory capture"; }
            Lib.Apply(3, q);
            H(q);
            Lib.Apply(0, q);
            X(q);
            MResetZ(q)
        }
    "#;
    crate::test_utils::check_semantic_equivalence_with_library(library, source);
    let (store, package) = crate::test_utils::compile_and_run_pipeline_to_with_library(
        library,
        source,
        crate::PipelineStage::Full,
    );
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, package),
        Ok(qsc_eval::val::Value::Result(qsc_eval::val::Result::Val(
            false
        ))),
    );
}

#[test]
fn nested_branch_guards_keep_each_iterations_selection_without_escaping_locals() {
    let source = r#"
        operation Apply(q : Qubit) : Unit {
            for n in [0, 1, 3, 1, 0] {
                mutable op = X;
                if n > 0 {
                    let (enabled, unrelated) = (n > 2, 11);
                    if enabled { set op = H; }
                }
                op(q);
                if n > 2 { H(q); } else { X(q); }
            }
        }
        @EntryPoint() operation Main() : Result {
            use q = Qubit();
            Apply(q);
            MResetZ(q)
        }
    "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Result(qsc_eval::val::Result::Val(false)),
    );
    let qir = crate::test_utils::generate_qir(source);
    assert_eq!(
        qir.matches("call void @__quantum__qis__h__body").count(),
        2,
        "{qir}"
    );
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        8,
        "{qir}"
    );
}

#[test]
fn captured_operation_array_preserves_loop_uncomputation_and_unused_declaration_layout() {
    let source = r#"
        struct QP { cu : ((Qubit, Qubit[]) => Unit is Adj)[] }
        operation RunQ(p : QP, qs : Qubit[]) : Unit is Adj {
            for i in 0..Length(p.cu) - 1 { p.cu[i](qs[i], qs[2..3]); }
        }
        function MkQ(cu : ((Qubit, Qubit[]) => Unit is Adj)[]) : (Qubit[] => Unit is Adj) {
            RunQ(new QP { cu = cu }, _)
        }
        operation Mark(qpe : Qubit[] => Unit is Adj, system : Qubit[], target : Qubit) : Unit is Adj {
            use phase = Qubit[2];
            within { qpe(phase + system); } apply { CNOT(phase[0], target); }
        }
        function MarkOp(qpe : Qubit[] => Unit is Adj) : ((Qubit[], Qubit) => Unit is Adj) {
            Mark(qpe, _, _)
        }
        operation AARun(oracle : (Qubit[], Qubit) => Unit is Adj) : Result[] {
            use reg = Qubit[2];
            for _ in 1..1 {
                use flag = Qubit();
                oracle(reg, flag); Adjoint oracle(reg, flag);
            }
            MResetEachZ(reg)
        }
        operation PrepI(k : Int, qs : Qubit[]) : Unit is Adj + Ctl { X(qs[0]); }
        function MkPrep(k : Int) : (Qubit[] => Unit is Adj + Ctl) { PrepI(k, _) }
        operation CUI(k : Int, c : Qubit, t : Qubit[]) : Unit is Adj + Ctl { CNOT(c, t[0]); }
        function MkCU(k : Int) : ((Qubit, Qubit[]) => Unit is Adj + Ctl) { CUI(k, _, _) }
        operation Main() : Result[] { AARun(MarkOp(MkQ([MkCU(1)]))) }
    "#;
    // Keep PrepI/MkPrep: these unused declarations perturb lifted item IDs in
    // the original failing composition. The oracle and adjoint must cancel.
    // This whole-source case is a compatibility control; the original failure
    // was on the incremental entry path.
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Array(
            vec![qsc_eval::val::Value::Result(qsc_eval::val::Result::Val(false)); 2].into(),
        ),
    );
    assert_specialization_call_abis(source);
    let qir = crate::test_utils::generate_qir(source);
    assert_eq!(
        qir.matches("call void @__quantum__rt__result_record_output")
            .count(),
        2,
        "{qir}"
    );
}

#[test]
fn specialization_capability_compatible_array_capture_is_packed_once() {
    let source = r#"
        operation Target(value : Int) : Unit is Adj + Ctl {}
        function Make() : ((Int => Unit is Adj + Ctl), Int) {
            (Target, 7)
        }
        operation Run(ops : (Int => Unit)[]) : Unit {
            ops[0](5);
        }
        operation Consume(factory : Unit -> ((Int => Unit), Int)) : Unit {
            let (op, tag) = factory();
            let f = value => {
                if tag != 7 { fail "wrong tag"; }
                op(value);
            };
            Run([f]);
        }
        @EntryPoint()
        operation Main() : Int {
            Consume(Make);
            42
        }
    "#;
    check_capability_dispatch(source);
}

#[test]
fn specialization_capability_capture_orders_and_control_layers_preserve_abi() {
    for (requirement, invocation) in [
        ("", "ops[0](5)"),
        ("is Adj", "Adjoint ops[0](5)"),
        ("is Ctl", "Controlled ops[0]([], 5)"),
        ("is Ctl", "Controlled Controlled ops[0]([], ([], 5))"),
    ] {
        for tag_first in [false, true] {
            let (output, required, values, bindings) = if tag_first {
                (
                    "(Int, (Int => Unit is Adj + Ctl))".to_string(),
                    format!("(Int, (Int => Unit {requirement}))"),
                    "(7, Target)",
                    "(tag, op)",
                )
            } else {
                (
                    "((Int => Unit is Adj + Ctl), Int)".to_string(),
                    format!("((Int => Unit {requirement}), Int)"),
                    "(Target, 7)",
                    "(op, tag)",
                )
            };
            for entries in ["[f]", "[f, f]"] {
                let source = indoc::formatdoc! {r#"
                    operation Target(value : Int) : Unit is Adj + Ctl {{}}
                    function Make() : {output} {{ {values} }}
                    operation Run(ops : (Int => Unit {requirement})[]) : Unit {{
                        {invocation};
                    }}
                    operation Consume(factory : Unit -> {required}) : Unit {{
                        let {bindings} = factory();
                        let f = value => {{
                            if tag != 7 {{ fail "wrong tag"; }}
                            op(value);
                        }};
                        Run({entries});
                    }}
                    @EntryPoint() operation Main() : Int {{
                        Consume(Make);
                        42
                    }}
                "#};
                check_capability_dispatch(&source);
            }
        }
    }
}

fn check_capability_dispatch(source: &str) {
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(42),
    );
    assert_specialization_call_abis(source);
    assert_specialized_int_qir(source, 42);
}

#[test]
fn specialization_dispatch_copy_watermark_preserves_nested_capture_values() {
    let source = dispatch_copy_source(
        "ops[0]({ let n = 1; ops[1](n) })",
        "Run([Make(10), Make(100)])",
    );
    check_dispatch_copy_watermark(&source, 111);
}

#[test]
fn specialization_dispatch_copy_watermark_preserves_deeper_and_repeated_calls() {
    for (body, entry, expected) in [
        (
            "ops[0]({ let n = 1; ops[1]({ let m = n + 1; ops[0](m) }) })",
            "Run([Make(10), Make(100)])",
            122,
        ),
        (
            "ops[0]({ let n = 1; ops[1](n) })",
            "1000 * Run([Make(10), Make(100)]) + Run([Make(20), Make(200)])",
            111_221,
        ),
        (
            "ops[0]({ let n = 1; ops[1](n) }) + ops[1]({ let n = 2; ops[0](n) })",
            "Run([Make(10), Make(100)])",
            223,
        ),
        (
            "ops[0]({ mutable n = 1; set n += 1; ops[1](n) })",
            "Run([Make(10), Make(100)])",
            112,
        ),
    ] {
        check_dispatch_copy_watermark(&dispatch_copy_source(body, entry), expected);
    }
}

#[test]
fn specialization_dispatch_copy_watermark_preserves_single_specialization_captures() {
    let source = r#"
        function Make(offset : Int, scale : Int) : Int -> Int {
            x -> offset + scale * x
        }
        function Run(ops : (Int -> Int)[]) : Int {
            let bias = 0;
            ops[0]({ let n = bias + 1; ops[0](n) })
        }
        @EntryPoint() operation Main() : Int {
            Run([Make(10, 100)])
        }
    "#;
    check_dispatch_copy_watermark(source, 11010);
}

fn dispatch_copy_source(body: &str, entry: &str) -> String {
    indoc::formatdoc! {r#"
        function Make(offset : Int) : Int -> Int {{ x -> offset + x }}
        function Run(ops : (Int -> Int)[]) : Int {{ {body} }}
        @EntryPoint() operation Main() : Int {{ {entry} }}
    "#}
}

fn check_dispatch_copy_watermark(source: &str, expected: i64) {
    use crate::walk_utils::{CallableNode, for_each_node_in_callable};
    use rustc_hash::FxHashSet;

    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(expected),
    );
    let (store, package_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
    let mut specializations = 0;
    for owner in collect_reachable_from_entry(&store, package_id) {
        let package = store.get(owner.package);
        let ItemKind::Callable(decl) = &package.get_item(owner.item).kind else {
            continue;
        };
        if decl.name.name.starts_with("Run") && decl.name.name.contains('{') {
            specializations += 1;
        }
        let mut patterns = FxHashSet::default();
        let mut bindings = FxHashSet::default();
        for_each_node_in_callable(package, decl, &mut |node| {
            if let CallableNode::Pat(id) = node
                && patterns.insert(id)
                && let fir::PatKind::Bind(binding) = &package.get_pat(id).kind
            {
                assert!(
                    bindings.insert(binding.id),
                    "{} reuses local {} for binding {}:\n{source}",
                    decl.name.name,
                    binding.id,
                    binding.name,
                );
            }
        });
    }
    assert_eq!(
        specializations, 1,
        "Run must specialize and repeated calls must share its declaration:\n{source}"
    );
    assert_specialization_call_abis(source);
    assert_specialized_int_qir(source, expected);
}

#[test]
fn specialization_layout_snapshot_and_embedded_capture_identity_record_507() {
    let source = r#"
        function Inc(x : Int) : Int { x + 1 }
        function Twice(x : Int) : Int { x * 2 }
        function Wrap(f : Int -> Int) : Int -> Int { x -> f(x) + 1 }
        @EntryPoint()
        operation Main() : Int {
            let first = Wrap(Inc);
            let second = Wrap(Twice);
            mutable selected = first;
            let earlier = ({ selected })({ set selected = second; 3 });
            earlier * 100 + selected(3)
        }
    "#;
    let mut mismatches = Vec::new();
    for stage in [crate::PipelineStage::Defunc, crate::PipelineStage::Full] {
        let (store, package_id) = crate::test_utils::compile_and_run_pipeline_to(source, stage);
        let mut closures = 0;
        for owner in collect_reachable_from_entry(&store, package_id) {
            let package = store.get(owner.package);
            let ItemKind::Callable(decl) = &package.get_item(owner.item).kind else {
                continue;
            };
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |_, expr| {
                    let fir::ExprKind::Closure(captures, target) = &expr.kind else {
                        return;
                    };
                    if !captures.is_empty() {
                        return;
                    }
                    closures += 1;
                    let qsc_fir::ty::Ty::Arrow(arrow) = &expr.ty else {
                        panic!("closure must have an arrow type");
                    };
                    let ItemKind::Callable(target) = &package.get_item(*target).kind else {
                        panic!("closure target must be callable");
                    };
                    let expected = qsc_fir::ty::Ty::Tuple(vec![arrow.input.as_ref().clone()]);
                    let actual = &package.get_pat(target.input).ty;
                    if *actual != expected {
                        mismatches.push(format!(
                            "{stage:?}: {} returns closure {} with public input {:?}, \
                             runtime argument {expected:?}, declared input {actual:?}",
                            decl.name.name, target.name.name, arrow.input
                        ));
                    }
                },
            );
        }
        assert_eq!(
            closures, 2,
            "both returned closures must survive at {stage:?}"
        );
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    assert_specialization_call_abis(source);
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(507),
    );
    assert_specialized_int_qir(source, 507);
}

#[test]
fn specialization_layout_preserves_nominal_data() {
    for stored in [false, true] {
        for (input, body, value, expected) in [
            (
                "(Int -> Int, Data)",
                "let (f, data) = p; f(data.N) + data.Tail",
                "(selected, new Data { N = 3, Tail = 10 })",
                30,
            ),
            (
                "Payload",
                "p.F(p.D.N) + p.Values[0].Tail",
                "new Payload { F = selected, D = new Data { N = 3, Tail = 0 }, Values = [new Data { N = 0, Tail = 10 }] }",
                30,
            ),
            (
                "ScalarPayload",
                "p.F(p.D.N)",
                "new ScalarPayload { F = selected, D = new ScalarData { N = 3 } }",
                10,
            ),
        ] {
            let invocation = if stored {
                format!("let held = {value}; Read(held)")
            } else {
                format!("Read({value})")
            };
            let source = indoc::formatdoc! {r#"
                struct Data {{ N : Int, Tail : Int }}
                struct Payload {{ F : Int -> Int, D : Data, Values : Data[] }}
                struct ScalarData {{ N : Int }}
                struct ScalarPayload {{ F : Int -> Int, D : ScalarData }}
                function Inc(x : Int) : Int {{ x + 1 }}
                function Twice(x : Int) : Int {{ 2 * x }}
                function Read(p : {input}) : Int {{ {body} }}
                function Pick(flag : Bool) : Int {{
                    let selected = if flag {{ Inc }} else {{ Twice }};
                    {invocation}
                }}
                @EntryPoint() operation Main() : Int {{ Pick(true) + Pick(false) }}
            "#};
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(expected),
            );
            assert_specialization_call_abis(&source);
            assert_specialized_int_qir(&source, expected);
        }
    }
}

#[test]
fn specialization_layout_empty_path_extraction_preserves_direct_values() {
    check_empty_path_nominal_extraction(false);
}

#[test]
fn specialization_layout_empty_path_extraction_preserves_scalar_captures() {
    check_empty_path_nominal_extraction(true);
}

fn check_empty_path_nominal_extraction(capturing: bool) {
    for (definition, value, extract) in [
        ("newtype Data = (N : Int);", "Data(10)", "let n = data::N;"),
        (
            "newtype Inner = (N : Int); newtype Data = (Value : Inner);",
            "Data(Inner(10))",
            "let n = (data::Value)::N;",
        ),
        (
            "newtype Data = (Value : (Int, Int));",
            "Data((10, 2))",
            "let (n, _) = data::Value;",
        ),
        (
            "newtype Data = (Value : (Int,));",
            "Data((10,))",
            "let (n,) = data::Value;",
        ),
    ] {
        for keep_tail in [false, true] {
            for held in [false, true] {
                let extra_field = if keep_tail { ", Tail : Int" } else { "" };
                let extra_value = if keep_tail { ", Tail = 0" } else { "" };
                let extra_result = if keep_tail { " + p.Tail" } else { "" };
                let extract = if held {
                    format!("let data = p.D; {extract}")
                } else {
                    extract.replace("data", "(p.D)")
                };
                let (body, expected) = if capturing {
                    ("let g = x -> n + x; Apply(g, 3) + p.F(0)", 14)
                } else {
                    ("p.F(n)", 11)
                };
                let source = indoc::formatdoc! {r#"
                    {definition}
                    struct Payload {{ F : Int -> Int, D : Data{extra_field} }}
                    function Inc(x : Int) : Int {{ x + 1 }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    function Read(p : Payload) : Int {{ {extract} {body}{extra_result} }}
                    @EntryPoint() operation Main() : Int {{
                        Read(new Payload {{ F = Inc, D = {value}{extra_value} }})
                    }}
                "#};
                crate::test_utils::check_semantic_equivalence_with_expected(
                    &source,
                    qsc_eval::val::Value::Int(expected),
                );
                assert_specialization_call_abis(&source);
                assert_nominal_extraction_types(&source, keep_tail);
                assert_specialized_int_qir(&source, expected);
            }
        }
    }
}

fn assert_nominal_extraction_types(source: &str, keep_tail: bool) {
    use qsc_fir::ty::{Prim, Ty};

    let (store, package_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
    let mut checked = 0;
    for owner in collect_reachable_from_entry(&store, package_id) {
        let package = store.get(owner.package);
        let ItemKind::Callable(decl) = &package.get_item(owner.item).kind else {
            continue;
        };
        if !decl.name.name.starts_with("Read{") {
            continue;
        }
        checked += 1;
        let input = &package.get_pat(decl.input).ty;
        let nominal = if keep_tail {
            let Ty::Tuple(fields) = input else {
                panic!("retained data and tail need a tuple: {input:?}");
            };
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[1], Ty::Prim(Prim::Int));
            &fields[0]
        } else {
            input
        };
        let Ty::Udt(fir::Res::Item(data)) = nominal else {
            panic!("the retained Data must remain nominal: {nominal:?}");
        };
        let ItemKind::Ty(_, udt) = &store.get(data.package).get_item(data.item).kind else {
            panic!("Data must be a type");
        };
        assert_eq!(udt.name.as_ref(), "Data");

        let mut extractions = 0;
        crate::walk_utils::for_each_expr_in_callable_impl(
            package,
            &decl.implementation,
            &mut |_, expr| {
                if let fir::ExprKind::Field(base, fir::Field::Path(path)) = &expr.kind
                    && path.indices.is_empty()
                {
                    extractions += 1;
                    let Ty::Udt(fir::Res::Item(item)) = &package.get_expr(*base).ty else {
                        panic!("empty-path extraction must read a nominal value: {expr:?}");
                    };
                    let ItemKind::Ty(_, udt) = &store.get(item.package).get_item(item.item).kind
                    else {
                        panic!("projection base must have a type declaration");
                    };
                    assert_eq!(expr.ty, udt.get_pure_ty(), "{source}");
                }
                if let fir::ExprKind::Call(callee, args) = expr.kind
                    && let fir::ExprKind::Var(fir::Res::Item(item), _) =
                        package.get_expr(callee).kind
                    && let ItemKind::Callable(target) =
                        &store.get(item.package).get_item(item.item).kind
                {
                    assert_eq!(
                        package.get_expr(args).ty,
                        store.get(item.package).get_pat(target.input).ty,
                        "arguments must match the actual declaration: {source}",
                    );
                }
            },
        );
        assert!(
            extractions > 0,
            "Read must retain nominal extraction: {source}"
        );
    }
    assert_eq!(checked, 1, "Read must actually be specialized: {source}");
}

#[test]
fn specialization_layout_preserves_consumed_payload_effects() {
    for (call, expected) in [
        ("Run(Inc, payload, 3)", 708),
        ("RunWithData(marker, Inc, payload, 3, marker)", 1408),
        ("Run(if flag { Inc } else { Twice }, payload, 3)", 710),
        ("Run(Add(2, _), payload, 3)", 709),
    ] {
        let payload = r#"new Only {
            ...({ Message("copy"); set marker = 7; new Only { F = Inc } }),
            F = Inc
        }"#;
        let call = call.replace("payload", payload);
        let source = indoc::formatdoc! {r#"
            struct Only {{ F : Int -> Int }}
            function Inc(x : Int) : Int {{ x + 1 }}
            function Twice(x : Int) : Int {{ 2 * x }}
            function Add(offset : Int, x : Int) : Int {{ offset + x }}
            function Run(g : Int -> Int, p : Only, n : Int) : Int {{ g(n) + p.F(n) }}
            function RunWithData(before : Int, g : Int -> Int, p : Only, n : Int, after : Int) : Int {{
                1000 * before + g(n) + p.F(n) + 100 * after
            }}
            function Pick(flag : Bool) : Int {{
                mutable marker = 0;
                Message("before");
                let value = {call};
                Message("after");
                100 * marker + value
            }}
            @EntryPoint() operation Main() : Int {{ Pick(false) }}
        "#};
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Int(expected),
        );
        assert_specialization_call_abis(&source);
        assert_specialized_int_qir(&source, expected);
    }
}

#[test]
fn specialization_layout_preserves_consumed_payload_quantum_trace() {
    let source = r#"
        struct Only { F : Int -> Int }
        function Inc(x : Int) : Int { x + 1 }
        function Run(g : Int -> Int, p : Only, n : Int) : Int { g(n) + p.F(n) }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let value = Run(Inc, new Only {
                ...({ X(q); Message("copy"); new Only { F = Inc } }), F = Inc
            }, 3);
            X(q);
            Reset(q);
            value
        }
    "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(8),
    );
    assert_specialization_call_abis(source);
}

#[test]
fn specialization_layout_preserves_consumed_payload_failure_order() {
    let source = r#"
        struct Only { F : Int -> Int }
        function Inc(x : Int) : Int { x + 1 }
        function Run(g : Int -> Int, p : Only, n : Int) : Int { g(n) + p.F(n) }
        @EntryPoint() operation Main() : Int {
            Run(Inc, new Only { ...({ fail "copy first" }), F = Inc },
                { fail "argument first" })
        }
    "#;
    let error =
        crate::test_utils::eval_qsharp_original(source).expect_err("the copy source must fail");
    assert!(error.contains("copy first"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

/// Forwarded values carry their creation-site controls in their arrow input,
/// even when the concrete global's declaration has a scalar input.
#[test]
fn specialization_preserves_forwarded_controlled_values() {
    use std::fmt::Write as _;

    for (functor, layers) in [
        ("Controlled", 1),
        ("Controlled Controlled", 2),
        ("Adjoint Controlled", 1),
    ] {
        for array in [false, true] {
            let mut input = "Int".to_string();
            let mut argument = "7".to_string();
            for _ in 0..layers {
                input = format!("(Qubit[], {input})");
                argument = format!("([], {argument})");
            }
            let mut unpack = String::new();
            let mut payload = "value".to_string();
            for layer in 0..layers {
                write!(unpack, "let (controls{layer}, value{layer}) = {payload};")
                    .expect("writing to a String is infallible");
                payload = format!("value{layer}");
            }
            for layer in (0..layers).rev() {
                payload = format!("(controls{layer}, {payload})");
            }
            let (parameter, call, forwarded) = if array {
                (
                    format!("ops : ({input} => Unit is Adj + Ctl)[]"),
                    format!("ops[1]{payload}"),
                    "[f, f]",
                )
            } else {
                (
                    format!("f : {input} => Unit is Adj + Ctl"),
                    format!("f{payload}"),
                    "f",
                )
            };
            let source = indoc::formatdoc! {r#"
                operation Target(value : Int) : Unit is Adj + Ctl {{
                    body (...) {{ if value != 7 {{ fail "body"; }} }}
                    adjoint (...) {{ if value != 7 {{ fail "adjoint"; }} }}
                    controlled (controls, ...) {{ if value != 7 {{ fail "controlled"; }} }}
                    controlled adjoint (controls, ...) {{ if value != 7 {{ fail "controlled adjoint"; }} }}
                }}
                operation Consume({parameter}, value : {input}) : Unit {{ {unpack} {call}; }}
                operation Relay(f : {input} => Unit is Adj + Ctl, value : {input}) : Unit {{
                    Consume({forwarded}, value);
                }}
                @EntryPoint() operation Main() : Int {{
                    Relay({functor} Target, {argument});
                    42
                }}
            "#};
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(42),
            );
            assert_specialization_call_abis(&source);
            assert_specialized_int_qir(&source, 42);
        }
    }
}

#[test]
fn specialization_layout_capture_free_arrays_match_declared_inputs() {
    for (input, entries, argument, values) in [
        ("Int", "[x -> x + 1, x -> 2 * x]", "3", [4, 6]),
        (
            "(Int, Int)",
            "[(x, y) -> x + y, (x, y) -> 2 * x + 2 * y]",
            "(3, 4)",
            [7, 14],
        ),
    ] {
        for (index, expected) in [
            (0, values[0]),
            (1, values[1]),
            (-2, values[0]),
            (-1, values[1]),
        ] {
            let source = indoc::formatdoc! {r#"
                function Run(ops : ({input} -> Int)[], index : Int) : Int {{
                    ops[index]({argument})
                }}
                @EntryPoint() operation Main() : Int {{ Run({entries}, {index}) }}
            "#};
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(expected),
            );
            assert_specialization_call_abis(&source);
            assert_specialized_int_qir(&source, expected);
        }
    }
}

#[test]
fn specialization_layout_deep_arrays_preserve_data_ancestors() {
    for capturing in [false, true] {
        for stored in [false, true] {
            let entries = if capturing {
                "[Add(first, _), Add(second, _)]"
            } else {
                "[Inc, Twice]"
            };
            for (input, unpack, argument, result, expected) in [
                (
                    "(((Int -> Int)[], Int), Int)",
                    "let ((ops, n), tail) = pair;",
                    format!("(({entries}, 3), 100)"),
                    "ops[1](n) + tail",
                    106,
                ),
                (
                    "(Int, ((Int -> Int)[], Int))",
                    "let (tail, (ops, n)) = pair;",
                    format!("(100, ({entries}, 3))"),
                    "ops[1](n) + tail",
                    106,
                ),
                (
                    "((Int, ((Int -> Int)[], Int)), Unit)",
                    "let ((tail, (ops, n)), unit) = pair;",
                    format!("((100, ({entries}, 3)), ())"),
                    "if unit == () { ops[1](n) + tail } else { 0 }",
                    106,
                ),
                (
                    "(((Int -> Int)[], Int), (Int -> Int, Int))",
                    "let ((ops, n), (f, m)) = pair;",
                    format!("(({entries}, 3), (Inc, 5))"),
                    "ops[1](n) + f(m)",
                    12,
                ),
            ] {
                let call = if stored {
                    format!("let args = {argument}; let alias = args; Run(alias)")
                } else {
                    format!("Run({argument})")
                };
                let source = indoc::formatdoc! {r#"
                    function Inc(x : Int) : Int {{ x + 1 }}
                    function Twice(x : Int) : Int {{ 2 * x }}
                    function Add(offset : Int, x : Int) : Int {{ offset + x }}
                    function Run(pair : {input}) : Int {{ {unpack} {result} }}
                    function Pick(first : Int, second : Int) : Int {{ {call} }}
                    @EntryPoint() operation Main() : Int {{ Pick(1, 3) }}
                "#};
                crate::test_utils::check_semantic_equivalence_with_expected(
                    &source,
                    qsc_eval::val::Value::Int(expected),
                );
                assert_specialization_call_abis(&source);
                assert_specialized_int_qir(&source, expected);
            }
        }
    }
}

#[test]
fn specialization_layout_preserves_existing_packing_routes() {
    for (source, expected, require_specialization) in [
        (
            r#"
            struct Leaf { F : Int -> Int }
            struct Root { Leaf : Leaf }
            function Run(p : Root) : Int { p.Leaf.F(3) }
            function Pick(offset : Int) : Int {
                Run(new Root { Leaf = new Leaf { F = x -> x + offset } })
            }
            @EntryPoint() operation Main() : Int { Pick(10) }
        "#,
            13,
            false,
        ),
        (
            r#"
            function Inc(x : Int) : Int { x + 1 }
            function Run(f : Int -> Int, g : Int -> Int) : Int { f(1) + g(2) }
            function Pick(offset : Int) : Int {
                let args = (x -> x + offset, Inc);
                Run(args)
            }
            @EntryPoint() operation Main() : Int { Pick(10) }
        "#,
            14,
            true,
        ),
        (
            r#"
            struct Payload { Ops : (Int -> Int)[], Head : Int, Tail : Int }
            function Inc(x : Int) : Int { x + 1 }
            function Run(p : Payload) : Int {
                100 * p.Head + p.Ops[0](p.Tail) + p.Ops[1](p.Tail)
            }
            function Pick(offset : Int) : Int {
                Run(new Payload { Ops = [x -> x + offset, Inc], Head = 4, Tail = 3 })
            }
            @EntryPoint() operation Main() : Int { Pick(10) }
        "#,
            417,
            true,
        ),
    ] {
        crate::test_utils::check_semantic_equivalence_with_expected(
            source,
            qsc_eval::val::Value::Int(expected),
        );
        // The nominal factory probe is a Full-pipeline guard, not evidence
        // that its callable argument was specialized at the Defunc boundary.
        if require_specialization {
            assert_specialization_call_abis(source);
        }
        assert_specialized_int_qir(source, expected);
    }
}

#[test]
fn specialization_layout_controls_keep_tuple_parameter_and_capture() {
    let source = r#"
        operation Flip(tag : Int, q : Qubit) : Unit is Adj + Ctl {
            if tag == 7 { X(q); }
        }
        operation Run(f : Qubit => Unit is Adj + Ctl, pair : (Int, Qubit)) : Unit is Adj + Ctl {
            let (tag, q) = pair;
            if tag == 4 { f(q); }
        }
        operation Pick(offset : Int, control : Qubit, target : Qubit) : Unit {
            Controlled Run([control], (Flip(offset, _), (4, target)));
        }
        @EntryPoint() operation Main() : Int {
            use control = Qubit();
            use target = Qubit();
            X(control);
            Pick(7, control, target);
            Reset(control);
            if MResetZ(target) == One { 1 } else { 0 }
        }
    "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(1),
    );
    assert_specialization_call_abis(source);
}

#[test]
fn specialization_layout_callee_effects_precede_control_argument_prefix() {
    let source = r#"
        operation Check(tag : Int, value : Int) : Unit is Ctl {
            body (...) { if tag + value != 13 { fail "body payload"; } }
            controlled (controls, ...) {
                if tag + value != 13 { fail "controlled payload"; }
            }
        }
        function Make(tag : Int) : Int => Unit is Ctl {
            Message("callee");
            Check(tag, _)
        }
        @EntryPoint() operation Main() : Int {
            mutable order = 0;
            let args : (Qubit[], Int) = ([], 3);
            Controlled (Make({ set order = 10 * order + 1; 10 }))(
                { Message("argument"); set order = 10 * order + 2; args }
            );
            order
        }
    "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(12),
    );
    assert_specialized_int_qir(source, 12);
}

#[test]
fn stored_selection_cleanup_preserves_guard_effects() {
    for flag in [false, true] {
        for other in ["A", "B"] {
            for call in [
                "selected({ set visits = 10 * visits + 2; 0 })",
                "Apply(selected, { set visits = 10 * visits + 2; 0 })",
            ] {
                let source = indoc::formatdoc! {r#"
                    function A(x : Int) : Int {{ x + 1 }}
                    function B(x : Int) : Int {{ x + 2 }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    @EntryPoint() operation Main() : Int {{
                        mutable visits = 0;
                        let selected = if {{ set visits = 10 * visits + 1; {flag} }} {{
                            A
                        }} else {{ {other} }};
                        let result = {call};
                        100 * visits + result
                    }}
                "#};
                let expected = if flag || other == "A" { 1201 } else { 1202 };
                crate::test_utils::check_semantic_equivalence_with_expected(
                    &source,
                    qsc_eval::val::Value::Int(expected),
                );
                assert_specialized_int_qir(&source, expected);
            }
        }
    }
}

#[test]
fn stored_selection_cleanup_preserves_guard_failure() {
    for call in [
        "selected({ fail \"argument ran first\" })",
        "Apply(selected, { fail \"argument ran first\" })",
    ] {
        let source = indoc::formatdoc! {r#"
            function A(x : Int) : Int {{ x + 1 }}
            function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
            function Guard() : Bool {{ fail "guard must run" }}
            @EntryPoint() operation Main() : Int {{
                let selected = if Guard() {{ A }} else {{ A }};
                {call}
            }}
        "#};
        let error = crate::test_utils::eval_qsharp_original(&source)
            .expect_err("the selection guard must fail before the call argument");
        assert!(error.contains("guard must run"), "{error}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

/// A stored argument is a value snapshot, not permission to replay its initializer.
/// The marker also detects repeated evaluation of computed control tuples.
#[test]
fn specialization_preserves_stored_controlled_array_arguments() {
    for layers in [1, 2] {
        for tuple_payload in [false, true] {
            for computed in [false, true] {
                let (payload_ty, payload, check) = if tuple_payload {
                    (
                        "(Int, Int)",
                        "(value, 0)",
                        "let (left, right) = value; tag + left + right",
                    )
                } else {
                    ("Int", "value", "tag + value")
                };
                let mut input = payload_ty.to_string();
                let mut argument = payload.to_string();
                for _ in 0..layers {
                    input = format!("(Qubit[], {input})");
                    argument = format!("([], {argument})");
                }
                let functor = "Controlled ".repeat(layers);
                let invoke_args = if computed {
                    "{ set marker = 10 * marker + 2; args }"
                } else {
                    "args"
                };
                let index = if computed { -1 } else { 1 };
                let expected = if computed { 1242 } else { 142 };
                let source = indoc::formatdoc! {r#"
                    operation Check(tag : Int, value : {payload_ty}) : Unit is Ctl {{
                        body (...) {{ let sum = {{ {check} }}; if sum != 13 {{ fail "body payload"; }} }}
                        controlled (controls, ...) {{
                            let sum = {{ {check} }};
                            if sum != 13 {{ fail "controlled payload"; }}
                        }}
                    }}
                    operation Run(ops : ({payload_ty} => Unit is Ctl)[], index : Int) : Int {{
                        mutable marker = 0;
                        mutable value = 3;
                        let args : {input} = {{ set marker = 1; {argument} }};
                        set value = 99;
                        {functor}ops[index]({invoke_args});
                        100 * marker + 42
                    }}
                    operation Pick(first : Int, second : Int) : Int {{
                        Run([Check(first, _), Check(second, _)], {index})
                    }}
                    @EntryPoint() operation Main() : Int {{ Pick(20, 10) }}
                "#};
                crate::test_utils::check_semantic_equivalence_with_expected(
                    &source,
                    qsc_eval::val::Value::Int(expected),
                );
                assert_specialization_call_abis(&source);
                assert_specialized_int_qir(&source, expected);
            }
        }
    }
}

/// Materializing an inner control shell must not move its effects ahead of an
/// outer control expression. Each operand appends one digit to the marker.
#[test]
fn specialization_preserves_control_shell_evaluation_order() {
    let source = r#"
        operation Check(tag : Int, value : Int) : Unit is Ctl {
            body (...) { if tag + value != 13 { fail "body payload"; } }
            controlled (controls, ...) {
                if tag + value != 13 { fail "controlled payload"; }
            }
        }
        operation Run(ops : (Int => Unit is Ctl)[]) : Int {
            mutable marker = 0;
            let inner : (Qubit[], Int) = ([], 3);
            Controlled Controlled ops[1](
                { set marker = 10 * marker + 1; [] },
                { set marker = 10 * marker + 2; inner }
            );
            100 * marker + 42
        }
        operation Pick(first : Int, second : Int) : Int {
            Run([Check(first, _), Check(second, _)])
        }
        @EntryPoint() operation Main() : Int { Pick(20, 10) }
    "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(1242),
    );
    assert_specialization_call_abis(source);
    assert_specialized_int_qir(source, 1242);
}

/// Mutable destructuring initializes a runtime binding. Assignments must not
/// rewrite that binding's store target or freeze later calls at its initial value.
#[test]
fn specialization_preserves_mutable_destructured_callables() {
    for capturing in [false, true] {
        for (update, last) in [
            ("set f = Twice;", 6),
            ("set (f, n) = (Twice, 4);", 8),
            ("if flag { set f = Twice; }", 6),
            ("for _ in 0..1 { set f = Twice; }", 6),
        ] {
            let entry = if capturing { "x -> x + offset" } else { "Inc" };
            let first = if capturing { 13 } else { 4 };
            let expected = 100 * first + last;
            let source = indoc::formatdoc! {r#"
                function Inc(x : Int) : Int {{ x + 1 }}
                function Twice(x : Int) : Int {{ 2 * x }}
                function Run(pair : (Int -> Int, Int), flag : Bool) : Int {{
                    mutable (f, n) = pair;
                    let first = f(n);
                    {update}
                    100 * first + f(n)
                }}
                function Pick(offset : Int) : Int {{ Run(({entry}, 3), true) }}
                @EntryPoint() operation Main() : Int {{ Pick(10) }}
            "#};
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(expected),
            );
            assert_specialization_call_abis(&source);
            assert_specialized_int_qir(&source, expected);
        }
    }
}

/// Both callable leaves of the inner tuple disappear, but the outer payload
/// survives. Its original paths must be consumed before the inner tuple collapses.
#[test]
fn specialization_preserves_partial_nested_removal_batches() {
    for reversed in [false, true] {
        for unit in [false, true] {
            let (pair_ty, unpack, pair, value) = if unit {
                (
                    "((Int -> Int, Int -> Int), Unit)",
                    "let ((f, g), unit) = pair;",
                    "((Inc, Twice), ())",
                    "if unit == () { 3 } else { 9 }",
                )
            } else {
                (
                    "((Int -> Int, Int -> Int), Int)",
                    "let ((f, g), n) = pair;",
                    "((Inc, Twice), 3)",
                    "n",
                )
            };
            let (signature, arguments) = if reversed {
                (
                    format!("pair : {pair_ty}, ops : (Int -> Int)[]"),
                    format!("{pair}, [Inc, Twice]"),
                )
            } else {
                (
                    format!("ops : (Int -> Int)[], pair : {pair_ty}"),
                    format!("[Inc, Twice], {pair}"),
                )
            };
            let source = indoc::formatdoc! {r#"
                function Inc(x : Int) : Int {{ x + 1 }}
                function Twice(x : Int) : Int {{ 2 * x }}
                function Run({signature}) : Int {{
                    {unpack}
                    let value = {value};
                    ops[0](value) + f(value) + g(value)
                }}
                @EntryPoint() operation Main() : Int {{ Run({arguments}) }}
            "#};
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(14),
            );
            assert_specialization_call_abis(&source);
            assert_specialized_int_qir(&source, 14);
        }
    }
}

/// The nested-array caller retains a Unit payload when all its fields disappear,
/// followed by any captured operands. Both sides must agree on that grouping.
#[test]
fn specialization_preserves_consumed_nested_array_payload() {
    for capturing in [false, true] {
        let array = if capturing {
            "[Add(first, _), Add(second, _)]"
        } else {
            "[Inc, Twice]"
        };
        let source = indoc::formatdoc! {r#"
            function Inc(x : Int) : Int {{ x + 1 }}
            function Twice(x : Int) : Int {{ 2 * x }}
            function Add(offset : Int, x : Int) : Int {{ offset + x }}
            function Run(pair : ((Int -> Int)[], Int -> Int), n : Int) : Int {{
                let (ops, f) = pair;
                ops[1](n) + f(n)
            }}
            function Pick(first : Int, second : Int) : Int {{
                Run(({array}, Inc), 3)
            }}
            @EntryPoint() operation Main() : Int {{ Pick(1, 3) }}
        "#};
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Int(10),
        );
        assert_specialization_call_abis(&source);
        assert_specialized_int_qir(&source, 10);
    }
}

/// Check declarations, not just the callee metadata that a rewrite also edits.
fn assert_specialization_call_abis(source: &str) {
    let (store, package_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
    let mut checked = 0;
    for item in collect_reachable_from_entry(&store, package_id) {
        let package = store.get(item.package);
        let ItemKind::Callable(decl) = &package.get_item(item.item).kind else {
            continue;
        };
        crate::invariants::check_local_var_consistency(package, decl);
        crate::walk_utils::for_each_expr_in_callable_impl(
            package,
            &decl.implementation,
            &mut |_, expression| {
                let fir::ExprKind::Call(callee, args) = expression.kind else {
                    return;
                };
                let (base, functor) =
                    crate::defunctionalize::types::peel_body_functors(package, callee);
                let target = match package.get_expr(base).kind {
                    fir::ExprKind::Var(fir::Res::Item(target), _) => target,
                    fir::ExprKind::Closure(ref captures, target) if captures.is_empty() => {
                        fir::ItemId {
                            package: item.package,
                            item: target,
                        }
                    }
                    _ => return,
                };
                let target_package = store.get(target.package);
                let ItemKind::Callable(target) = &target_package.get_item(target.item).kind else {
                    return;
                };
                if !target.name.name.contains('{') && !target.name.name.starts_with(".lambda") {
                    return;
                }
                let mut expected = target_package.get_pat(target.input).ty.clone();
                for _ in 0..functor.controlled {
                    expected = qsc_fir::ty::Ty::Tuple(vec![
                        qsc_fir::ty::Ty::Array(Box::new(qsc_fir::ty::Ty::Prim(
                            qsc_fir::ty::Prim::Qubit,
                        ))),
                        expected,
                    ]);
                }
                let qsc_fir::ty::Ty::Arrow(arrow) = &package.get_expr(callee).ty else {
                    panic!("specialized callee must be arrow-typed");
                };
                assert_eq!(*arrow.input, expected, "callee metadata:\n{source}");
                assert_eq!(
                    package.get_expr(args).ty,
                    expected,
                    "call arguments:\n{source}"
                );
                checked += 1;
            },
        );
    }
    assert!(
        checked > 0,
        "source must exercise a specialized call:\n{source}"
    );
}

#[test]
fn specialization_preserves_controlled_array_environments() {
    for (functor, layers) in [
        ("Controlled", 1),
        ("Controlled Controlled", 2),
        ("Adjoint Controlled", 1),
    ] {
        for same_target in [false, true] {
            for enabled in [false, true] {
                for index in [0, 1, -1, -2] {
                    let entries = if same_target {
                        "Make(first), Make(second)"
                    } else {
                        "FlipWhen(first, _), FlipWhen(second, _)"
                    };
                    let args = if layers == 2 {
                        "[controls[0]], ([controls[1]], q)"
                    } else {
                        "[controls[0]], q"
                    };
                    let source = indoc::formatdoc! {r#"
                        operation FlipWhen(tag : Int, q : Qubit) : Unit is Adj + Ctl {{
                            if tag == 1 {{ X(q); }}
                        }}
                        function Make(tag : Int) : Qubit => Unit is Adj + Ctl {{
                            FlipWhen(tag, _)
                        }}
                        operation Run(
                            ops : (Qubit => Unit is Adj + Ctl)[],
                            index : Int, controls : Qubit[], q : Qubit
                        ) : Unit {{
                            {functor} ops[index]({args});
                        }}
                        operation Pick(first : Int, second : Int) : Int {{
                            use controls = Qubit[{layers}];
                            use q = Qubit();
                            if {enabled} {{ for control in controls {{ X(control); }} }}
                            Run([{entries}], {index}, controls, q);
                            for control in controls {{ let _ = MResetZ(control); }}
                            if MResetZ(q) == One {{ 1 }} else {{ 0 }}
                        }}
                        @EntryPoint() operation Main() : Int {{ Pick(0, 1) }}
                    "#};
                    crate::test_utils::check_semantic_equivalence_with_expected(
                        &source,
                        qsc_eval::val::Value::Int(i64::from(
                            enabled && (index == 1 || index == -1),
                        )),
                    );
                }
            }
        }
    }
}

#[test]
fn specialization_preserves_nested_index_evaluation() {
    for capturing in [false, true] {
        for depth in 1..=3 {
            let mut index = "0".to_string();
            for _ in 0..depth {
                index = format!("ops[{index}](-1)");
            }
            let entry = if capturing {
                "value -> Logged(value + offset - 1)"
            } else {
                "Logged"
            };
            let source = indoc::formatdoc! {r#"
                function Logged(value : Int) : Int {{ Message($"call:{{value}}"); value + 1 }}
                function Run(ops : (Int -> Int)[]) : Int {{
                    mutable marker = 0;
                    let result = ops[{{ Message("index"); set marker = 1; {index} }}](
                        {{ Message("argument"); set marker = 10 * marker + 2; 41 }}
                    );
                    100 * marker + result
                }}
                function Pick(offset : Int) : Int {{ Run([{entry}]) }}
                @EntryPoint() operation Main() : Int {{ Pick(1) }}
            "#};
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(1242),
            );
        }
    }
}

#[test]
fn specialization_preserves_array_sibling_payloads() {
    for reversed in [false, true] {
        for capturing in [false, true] {
            for unit in [false, true] {
                let (signature, unpack, pair) = if unit {
                    (
                        "(Int -> Int, Unit)",
                        "let (f, unit) = pair; let n = if unit == () { 3 } else { 9 };",
                        "(Inc, ())",
                    )
                } else {
                    ("(Int -> Int, Int)", "let (f, n) = pair;", "(Inc, 3)")
                };
                let entries = if capturing {
                    "[Add(first, _), Add(second, _)]"
                } else {
                    "[Inc, Twice]"
                };
                let (params, args) = if reversed {
                    (
                        format!("pair : {signature}, ops : (Int -> Int)[]"),
                        format!("{pair}, {entries}"),
                    )
                } else {
                    (
                        format!("ops : (Int -> Int)[], pair : {signature}"),
                        format!("{entries}, {pair}"),
                    )
                };
                let source = indoc::formatdoc! {r#"
                    function Inc(x : Int) : Int {{ x + 1 }}
                    function Twice(x : Int) : Int {{ 2 * x }}
                    function Add(offset : Int, x : Int) : Int {{ offset + x }}
                    function Run({params}) : Int {{ {unpack} ops[1](n) + f(n) }}
                    function Pick(first : Int, second : Int) : Int {{ Run({args}) }}
                    @EntryPoint() operation Main() : Int {{ Pick(1, 3) }}
                "#};
                crate::test_utils::check_semantic_equivalence_with_expected(
                    &source,
                    qsc_eval::val::Value::Int(10),
                );
                assert_specialized_int_qir(&source, 10);
            }
        }
    }
}

#[test]
fn specialization_preserves_recursive_nested_removal_paths() {
    for (signature, unpack, argument, result) in [
        (
            "(Int -> Int, Int -> Int)",
            "let (f, g) = pair;",
            "(f, g)",
            "f(1) + g(1)",
        ),
        (
            "(Int -> Int, Int -> Int, Int -> Int)",
            "let (f, g, h) = pair;",
            "(f, g, h)",
            "f(1) + g(1) + h(1)",
        ),
        (
            "(Int -> Int, Int -> Int, Unit)",
            "let (f, g, unit) = pair;",
            "(f, g, unit)",
            "if unit == () { f(1) + g(1) } else { 0 }",
        ),
    ] {
        let entry = if signature.ends_with("Unit)") {
            "(Inc, Twice, ())"
        } else if signature.matches("Int -> Int").count() == 3 {
            "(Inc, Twice, Inc)"
        } else {
            "(Inc, Twice)"
        };
        let expected = if entry == "(Inc, Twice, Inc)" { 6 } else { 4 };
        let source = indoc::formatdoc! {r#"
            function Inc(x : Int) : Int {{ x + 1 }}
            function Twice(x : Int) : Int {{ 2 * x }}
            function Recur(pair : {signature}, n : Int) : Int {{
                {unpack}
                if n == 0 {{ {result} }} else {{ Recur({argument}, n - 1) }}
            }}
            @EntryPoint() operation Main() : Int {{ Recur({entry}, 2) }}
        "#};
        let (store, package_id) =
            crate::test_utils::compile_and_run_pipeline_to(&source, crate::PipelineStage::Defunc);
        for item in collect_reachable_from_entry(&store, package_id) {
            let package = store.get(item.package);
            let ItemKind::Callable(decl) = &package.get_item(item.item).kind else {
                continue;
            };
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |_, expr| {
                    let fir::ExprKind::Call(callee, args) = expr.kind else {
                        return;
                    };
                    let fir::ExprKind::Var(fir::Res::Item(target), _) =
                        package.get_expr(callee).kind
                    else {
                        return;
                    };
                    let target_package = store.get(target.package);
                    let ItemKind::Callable(target) = &target_package.get_item(target.item).kind
                    else {
                        return;
                    };
                    if target.name.name.starts_with("Recur") && target.name.name.contains('{') {
                        assert_eq!(
                            package.get_expr(args).ty,
                            target_package.get_pat(target.input).ty,
                            "{source}"
                        );
                    }
                },
            );
        }
        crate::test_utils::check_semantic_equivalence_with_expected(
            &source,
            qsc_eval::val::Value::Int(expected),
        );
        assert_specialized_int_qir(&source, expected);
    }
}

#[test]
fn specialization_preserves_foreign_udt_layouts() {
    use std::fmt::Write as _;

    let library = r#"
        namespace Lib {
            struct Payload { F : Int -> Int, G : Int -> Int, N : Int }
            function Read(p : Payload) : Int { 100 * p.F(p.N) + p.G(p.N) }
            export Payload, Read;
        }
    "#;
    for padding in 0..8 {
        let mut declarations = String::new();
        for index in 0..padding {
            writeln!(
                declarations,
                "function Padding{index}() : Int {{ {index} }}"
            )
            .expect("writing to a String is infallible");
        }
        for reversed in [false, true] {
            let different = "struct Different { F : Int -> Int, G : Int -> Int }";
            let functions = "function Inc(n : Int) : Int { n + 1 }
                function Twice(n : Int) : Int { 2 * n }";
            let items = if reversed {
                format!("{functions}\n{different}")
            } else {
                format!("{different}\n{functions}")
            };
            for call in ["Lib.Read(p)", "Relay(p)"] {
                let source = indoc::formatdoc! {r#"
                    {declarations}
                    {items}
                    function Relay(p : Lib.Payload) : Int {{ Lib.Read(p) }}
                    @EntryPoint() operation Main() : Int {{
                        let p = new Lib.Payload {{ F = Inc, G = Twice, N = 3 }};
                        {call}
                    }}
                "#};
                assert_eq!(
                    crate::test_utils::eval_qsharp_original_with_library(library, &source),
                    Ok(qsc_eval::val::Value::Int(406)),
                );
                let (store, package_id) =
                    crate::test_utils::compile_and_run_pipeline_to_with_library(
                        library,
                        &source,
                        crate::PipelineStage::Defunc,
                    );
                for item in collect_reachable_from_entry(&store, package_id) {
                    let package = store.get(item.package);
                    if let ItemKind::Callable(decl) = &package.get_item(item.item).kind {
                        crate::invariants::check_local_var_consistency(package, decl);
                    }
                }
                crate::test_utils::check_semantic_equivalence_with_library(library, &source);
            }
        }
    }
}

#[test]
fn specialization_preserves_single_slot_unit_data() {
    let source = r#"
        function Run(pair : (Int -> Int, Unit)) : Int {
            let (f, unit) = pair;
            if unit == () { f(3) } else { 0 }
        }
        function Pick(offset : Int) : Int { Run((x -> x + offset, ())) }
        @EntryPoint() operation Main() : Int { Pick(10) }
    "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(13),
    );
    assert_specialized_int_qir(source, 13);
}

#[test]
fn specialization_preserves_single_slot_unit_effects() {
    let source = r#"
        function Run(pair : (Int -> Int, Unit)) : Int {
            let (f, _) = pair;
            f(3)
        }
        function Pick(offset : Int) : Int {
            mutable marker = 0;
            let result = Run((x -> x + offset, { set marker = 1; () }));
            1000 * marker + result
        }
        @EntryPoint() operation Main() : Int { Pick(10) }
    "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(1013),
    );
    assert_specialized_int_qir(source, 1013);
}

/// Indexed dispatch must preserve functors applied both when a callable enters
/// the array (`creation`) and when the HOF invokes it (`body`). Adjoint flags
/// cancel in pairs; control layers add. Previously, a generated dispatch branch
/// could call the bare operation while retaining its controlled argument shape.
///
/// Index 0 always selects an S gate, either directly or through captured Phase
/// arguments. Prepare its inverse between two H gates so correct execution
/// returns 0. Disabled controls instead require no phase preparation and no gate.
/// The semantic helper also compares quantum traces, not just the measured bit.
#[test]
fn specialization_preserves_indexed_functors() {
    // Each row names creation functors, invocation functors, and total controls.
    for (creation, body, controls) in [
        ("", "", 0),
        ("", "Adjoint", 0),
        ("Adjoint", "", 0),
        ("Adjoint", "Adjoint", 0),
        ("", "Controlled", 1),
        ("", "Adjoint Controlled", 1),
        ("", "Controlled Controlled", 2),
        ("Controlled", "", 1),
        ("Controlled", "Adjoint", 1),
        ("Controlled", "Controlled", 2),
        ("Adjoint Controlled", "Controlled", 2),
        ("Controlled", "Adjoint Controlled Controlled", 3),
    ] {
        for capturing in [false, true] {
            for enabled in [false, true] {
                // With no controls there is no distinct disabled case.
                if controls == 0 && !enabled {
                    continue;
                }
                // A stored Controlled operation already accepts (controls, q);
                // invocation-side controls add further wrappers around that input.
                let input = if creation.contains("Controlled") {
                    "(Qubit[], Qubit)"
                } else {
                    "Qubit"
                };
                let mut args = "q".to_string();
                // Build outer-to-inner control tuples, e.g. ([c0], ([c1], q)).
                for index in (0..controls).rev() {
                    args = format!("[controls[{index}]], ({args})");
                }
                // Equal captured tags select S; unequal tags select T. Keeping
                // both entries exercises array dispatch and capture forwarding.
                let (first, second) = if capturing {
                    ("Phase(7, 7, _)", "Phase(2, 3, _)")
                } else {
                    ("S", "T")
                };
                // An effective Adjoint S needs S as its inverse preparation;
                // an effective S needs Adjoint S. Two Adjoints cancel.
                let prepare = if controls != 0 && !enabled {
                    ""
                } else if creation.contains("Adjoint") != body.contains("Adjoint") {
                    "S(target);"
                } else {
                    "Adjoint S(target);"
                };
                let source = indoc::formatdoc! {r#"
                    operation Phase(a : Int, b : Int, q : Qubit) : Unit is Adj + Ctl {{
                        if a == b {{ S(q); }} else {{ T(q); }}
                    }}
                    operation Run(
                        ops : ({input} => Unit is Adj + Ctl)[],
                        index : Int, controls : Qubit[], q : Qubit
                    ) : Unit {{
                        {body} ops[index]({args});
                    }}
                    @EntryPoint() operation Main() : Int {{
                        use controls = Qubit[{controls}];
                        use target = Qubit();
                        if {enabled} {{ for control in controls {{ X(control); }} }}
                        H(target);
                        {prepare}
                        Run([{creation} ({first}), {creation} ({second})], 0, controls, target);
                        H(target);
                        for control in controls {{ let _ = MResetZ(control); }}
                        if MResetZ(target) == Zero {{ 0 }} else {{ 1 }}
                    }}
                "#};
                crate::test_utils::check_semantic_equivalence_with_expected(
                    &source,
                    qsc_eval::val::Value::Int(0),
                );
            }
        }
    }
}

/// The same selected callable occupies f in one Compose call and g in another.
/// Those positions need different specialization keys: f(g(x)) is not generally
/// equal to g(f(x)). Test both top-level parameters and fields of one tuple.
///
/// Reversing discovery order must not change cache reuse or either answer.
/// With Twice selected, the results are 8 and 7, encoded as 807; with Inc
/// selected, both are 5, encoded as 505.
#[test]
fn specialization_preserves_dispatched_parameter_positions() {
    for nested in [false, true] {
        for reversed in [false, true] {
            for flag in [false, true] {
                let (signature, unpack, first, second) = if nested {
                    (
                        "pair : (Int -> Int, Int -> Int), x : Int",
                        "let (f, g) = pair;",
                        "Compose((selected, Inc), 3)",
                        "Compose((Inc, selected), 3)",
                    )
                } else {
                    (
                        "f : Int -> Int, g : Int -> Int, x : Int",
                        "",
                        "Compose(selected, Inc, 3)",
                        "Compose(Inc, selected, 3)",
                    )
                };
                let calls = if reversed {
                    format!("let second = {second}; let first = {first};")
                } else {
                    format!("let first = {first}; let second = {second};")
                };
                // Measurement keeps the branch dynamic to compilation, while
                // preparing the qubit makes each simulated outcome deterministic.
                let source = indoc::formatdoc! {r#"
                    function Inc(x : Int) : Int {{ x + 1 }}
                    function Twice(x : Int) : Int {{ 2 * x }}
                    function Compose({signature}) : Int {{ {unpack} f(g(x)) }}
                    @EntryPoint() operation Main() : Int {{
                        use flagQubit = Qubit();
                        if {flag} {{ X(flagQubit); }}
                        let flag = MResetZ(flagQubit) == One;
                        let selected = if flag {{ Inc }} else {{ Twice }};
                        {calls}
                        100 * first + second
                    }}
                "#};
                crate::test_utils::check_semantic_equivalence_with_expected(
                    &source,
                    qsc_eval::val::Value::Int(if flag { 505 } else { 807 }),
                );
            }
        }
    }
}

/// Substitution must visit a parallel expression's body and optional limit,
/// including closures nested inside the body. Otherwise specialization removes
/// f from the input but leaves an unbound reference beneath the Parallel node.
///
/// The global callable returns 4 at x=3; the captured callable returns 10.
/// A limit-only use must still be rewritten even though the body returns 42.
#[test]
fn specialization_preserves_parallel_parameter_uses() {
    for (callable, result) in [("Inc", 4), ("value -> value + offset", 10)] {
        for (body, expected) in [
            // Body only; limit only; both; then a nested closure capturing f.
            ("parallel { f(x) }", result),
            ("parallel within f(x) { 42 }", 42),
            ("parallel within f(x) { f(x) + 1 }", result + 1),
            ("parallel { let again = y -> f(y); again(x) }", result),
        ] {
            let source = indoc::formatdoc! {r#"
                function Inc(x : Int) : Int {{ x + 1 }}
                operation ApplyParallel(f : Int -> Int, x : Int) : Int {{ {body} }}
                @EntryPoint() operation Main() : Int {{
                    let offset = 7;
                    ApplyParallel({callable}, 3)
                }}
            "#};
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(expected),
            );
        }
    }
}

/// Capture threading must not mistake a foreign function for a local lifted
/// closure with the same numeric item ID. The complete identity includes the
/// package; otherwise the closure's offset can be prepended to Foreign's input.
///
/// Vary unused library declarations to exercise different item-number layouts,
/// and place the foreign call before and after f. Both orders must return
/// 1000 * Foreign(3) + (3 + 7) = 1000 * 30 + 10 = 30010.
#[test]
fn specialization_preserves_foreign_callee_identity() {
    use std::fmt::Write as _;

    for padding in 0..8 {
        let mut declarations = String::new();
        for index in 0..padding {
            writeln!(
                declarations,
                "function Padding{index}() : Int {{ {index} }}"
            )
            .expect("writing to a String is infallible");
        }
        let library = indoc::formatdoc! {r#"
            namespace Lib {{
                {declarations}
                function Foreign(x : Int) : Int {{ 10 * x }}
                export Foreign;
            }}
        "#};
        for body in [
            "1000 * Lib.Foreign(x) + f(x)",
            "let value = f(x); 1000 * Lib.Foreign(x) + value",
        ] {
            let source = indoc::formatdoc! {r#"
                function Use(f : Int -> Int, x : Int) : Int {{ {body} }}
                @EntryPoint() operation Main() : Int {{
                    let offset = 7;
                    Use(x -> x + offset, 3)
                }}
            "#};
            assert_eq!(
                crate::test_utils::eval_qsharp_original_with_library(&library, &source),
                Ok(qsc_eval::val::Value::Int(30010)),
            );
            crate::test_utils::check_semantic_equivalence_with_library(&library, &source);
        }
    }
}

/// A mixed dispatch combines a conditionally selected callable inside a tuple
/// with a separate capturing callable. Removing the callable fields must keep
/// the tuple's data and append the captured offset without losing their grouping.
///
/// The matrix varies tuple depth, the top-level parameter order, and the branch.
/// At n=3, other(n)=13: Inc gives 17 and Twice gives 19. Nested cases must also
/// preserve the prefix 100, giving 117 or 119.
#[test]
fn specialization_preserves_mixed_partial_inputs() {
    for nested in [false, true] {
        for top_first in [false, true] {
            for flag in [false, true] {
                let (pair_type, pair, unpack, prefix) = if nested {
                    (
                        "(Int, (Int -> Int, Int))",
                        "(100, (chosen, 3))",
                        "let (prefix, (f, n)) = pair;",
                        "prefix + ",
                    )
                } else {
                    ("(Int -> Int, Int)", "(chosen, 3)", "let (f, n) = pair;", "")
                };
                let (signature, args) = if top_first {
                    (
                        format!("other : Int -> Int, pair : {pair_type}"),
                        format!("x -> x + offset, {pair}"),
                    )
                } else {
                    (
                        format!("pair : {pair_type}, other : Int -> Int"),
                        format!("{pair}, x -> x + offset"),
                    )
                };
                let source = indoc::formatdoc! {r#"
                    function Inc(x : Int) : Int {{ x + 1 }}
                    function Twice(x : Int) : Int {{ 2 * x }}
                    function Run({signature}) : Int {{ {unpack} {prefix}f(n) + other(n) }}
                    function Pick(flag : Bool) : Int {{
                        let offset = 10;
                        let chosen = if flag {{ Inc }} else {{ Twice }};
                        Run({args})
                    }}
                    @EntryPoint() operation Main() : Int {{ Pick({flag}) }}
                "#};
                let expected = (if flag { 17 } else { 19 }) + if nested { 100 } else { 0 };
                crate::test_utils::check_semantic_equivalence_with_expected(
                    &source,
                    qsc_eval::val::Value::Int(expected),
                );
                assert_specialized_int_qir(&source, expected);
            }
        }
    }
}

/// Exercise the same partial-tuple removal beneath one or two control wrappers,
/// including an adjoint invocation. The tuple's n=3 and the partial application's
/// captured tag=7 must both survive; they enable the two gates in Run.
///
/// With enabled controls, choosing X gives X followed by X (result 0), while
/// choosing I leaves one X (result 1). Disabled controls always give 0. These
/// gates are self-adjoint, so the adjoint variant has the same result.
#[test]
fn specialization_preserves_controlled_mixed_partial_inputs() {
    for functor in ["Controlled", "Controlled Controlled", "Adjoint Controlled"] {
        for enabled in [false, true] {
            for flag in [false, true] {
                let args = if functor == "Controlled Controlled" {
                    "[outer], ([inner], ((chosen, 3), FlipWhen(7, _), target))"
                } else {
                    "[outer], ((chosen, 3), FlipWhen(7, _), target)"
                };
                let source = indoc::formatdoc! {r#"
                    operation FlipWhen(tag : Int, q : Qubit) : Unit is Adj + Ctl {{
                        if tag == 7 {{ X(q); }}
                    }}
                    operation Run(
                        pair : (Qubit => Unit is Adj + Ctl, Int),
                        other : Qubit => Unit is Adj + Ctl, q : Qubit
                    ) : Unit is Adj + Ctl {{
                        let (op, n) = pair;
                        if n == 3 {{ op(q); other(q); }}
                    }}
                    operation Pick(flag : Bool) : Int {{
                        use outer = Qubit();
                        use inner = Qubit();
                        use target = Qubit();
                        if {enabled} {{ X(outer); X(inner); }}
                        let chosen = if flag {{ X }} else {{ I }};
                        {functor} Run({args});
                        let _ = MResetZ(outer);
                        let _ = MResetZ(inner);
                        if MResetZ(target) == One {{ 1 }} else {{ 0 }}
                    }}
                    @EntryPoint() operation Main() : Int {{ Pick({flag}) }}
                "#};
                crate::test_utils::check_semantic_equivalence_with_expected(
                    &source,
                    qsc_eval::val::Value::Int(i64::from(enabled && !flag)),
                );
                // This is an output-shape smoke check, not a QIR value oracle:
                // measurement remains dynamic. The helper above checks values
                // and quantum traces before and after the FIR pipeline.
                let qir = crate::test_utils::generate_qir(&source);
                assert_eq!(
                    qir.lines()
                        .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
                        .count(),
                    1,
                    "{source}\n{qir}",
                );
            }
        }
    }
}

/// Mixed specialization must distinguish a partially consumed input from a
/// fully consumed input, even when either lowers to a Unit-shaped value.
/// Surviving tuple fields stay grouped; real Unit data stays present; fully
/// removed tuples and UDT wrappers must not leave phantom Unit arguments.
///
/// Each row supplies a Run signature/body, its call, and the expected results
/// for Twice and Inc respectively. The other closure adds offset=10.
#[test]
fn specialization_preserves_mixed_payload_grouping_and_unit_fields() {
    for (declaration, call, false_result, true_result) in [
        // Keep n=3 and m=4 together in the single surviving bundle:
        // 300 + chosen(3) + 14 gives 320 or 318.
        (
            "function Run(bundle : (Int -> Int, Int, Int -> Int, Int)) : Int {
                let (f, n, g, m) = bundle;
                100 * n + f(n) + g(m)
            }",
            "Run((chosen, 3, x -> x + offset, 4))",
            320,
            318,
        ),
        // Both bundle fields disappear; only the closure capture remains:
        // 100 * chosen(3) + 14 gives 614 or 414.
        (
            "function Run(bundle : (Int -> Int, Int -> Int)) : Int {
                let (f, g) = bundle;
                100 * f(3) + g(4)
            }",
            "Run((chosen, x -> x + offset))",
            614,
            414,
        ),
        // The original Unit field is real data, so pair is not fully removed.
        // It selects n=3: chosen(3) + 13 gives 19 or 17.
        (
            "function Run(pair : (Int -> Int, Unit), other : Int -> Int) : Int {
                let (f, unit) = pair;
                let n = if unit == () { 3 } else { 9 };
                f(n) + other(n)
            }",
            "Run((chosen, ()), x -> x + offset)",
            19,
            17,
        ),
        // Keep pair's n=3, but remove the all-callable sibling tuple:
        // chosen(3) + 13 + Inc(3) gives 23 or 21.
        (
            "function Run(pair : (Int -> Int, Int), only : (Int -> Int, Int -> Int)) : Int {
                let (f, n) = pair;
                let (g, h) = only;
                f(n) + g(n) + h(n)
            }",
            "Run((chosen, 3), (x -> x + offset, Inc))",
            23,
            21,
        ),
        // The same whole-slot rule must remove a single-field UDT containing
        // a global callable, while the separate closure still passes its capture.
        (
            "struct Holder { Apply : Int -> Int }
            function Run(pair : (Int -> Int, Int), holder : Holder, other : Int -> Int) : Int {
                let (f, n) = pair;
                f(n) + holder.Apply(n) + other(n)
            }",
            "Run((chosen, 3), new Holder { Apply = Inc }, x -> x + offset)",
            23,
            21,
        ),
    ] {
        for (flag, expected) in [(false, false_result), (true, true_result)] {
            let source = indoc::formatdoc! {r#"
                function Inc(x : Int) : Int {{ x + 1 }}
                function Twice(x : Int) : Int {{ 2 * x }}
                {declaration}
                function Pick(flag : Bool) : Int {{
                    let offset = 10;
                    let chosen = if flag {{ Inc }} else {{ Twice }};
                    {call}
                }}
                @EntryPoint() operation Main() : Int {{ Pick({flag}) }}
            "#};
            // Check immediately after Defunc, before later tuple/argument passes
            // can hide a mismatch. Compare each generated Run call's arguments
            // with its declaration, not merely with the callee expression's type.
            let (store, package_id) = crate::test_utils::compile_and_run_pipeline_to(
                &source,
                crate::PipelineStage::Defunc,
            );
            let reachable = collect_reachable_from_entry(&store, package_id);
            for item in reachable {
                let package = store.get(item.package);
                let ItemKind::Callable(decl) = &package.get_item(item.item).kind else {
                    continue;
                };
                crate::walk_utils::for_each_expr_in_callable_impl(
                    package,
                    &decl.implementation,
                    &mut |_, expr| {
                        let fir::ExprKind::Call(callee, args) = expr.kind else {
                            return;
                        };
                        let fir::ExprKind::Var(fir::Res::Item(target), _) =
                            package.get_expr(callee).kind
                        else {
                            return;
                        };
                        let target_package = store.get(target.package);
                        let ItemKind::Callable(target) = &target_package.get_item(target.item).kind
                        else {
                            return;
                        };
                        if target.name.name.starts_with("Run") && target.name.name.contains('{') {
                            assert_eq!(
                                package.get_expr(args).ty,
                                target_package.get_pat(target.input).ty,
                                "mixed payload call arguments must match the specialized input:\n{source}"
                            );
                        }
                    },
                );
            }
            crate::test_utils::check_semantic_equivalence_with_expected(
                &source,
                qsc_eval::val::Value::Int(expected),
            );
            assert_specialized_int_qir(&source, expected);
        }
    }
}

/// These classical fixtures fully evaluate during QIR generation. Require one
/// integer output call with the expected literal value, not just any output call.
fn assert_specialized_int_qir(source: &str, expected: i64) {
    let qir = crate::test_utils::generate_qir(source);
    let records: Vec<_> = qir
        .lines()
        .filter(|line| line.contains("call void @__quantum__rt__int_record_output"))
        .collect();
    assert_eq!(records.len(), 1, "{source}\n{qir}");
    assert!(
        records[0].contains(&format!("i64 {expected},")),
        "{source}\n{qir}",
    );
}

#[test]
fn recursive_capture_calls_match_specialized_signatures() {
    for (source, _) in crate::defunctionalize::test_cases::recursive_capture_cases() {
        let (store, pkg_id) =
            crate::test_utils::compile_and_run_pipeline_to(&source, crate::PipelineStage::Defunc);
        let reachable = collect_reachable_from_entry(&store, pkg_id);
        let mut checked = 0;
        for item in reachable.iter().filter(|item| item.package == pkg_id) {
            let package = store.get(pkg_id);
            let ItemKind::Callable(decl) = &package.get_item(item.item).kind else {
                continue;
            };
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |_, expr| {
                    let fir::ExprKind::Call(callee, args) = expr.kind else {
                        return;
                    };
                    let fir::ExprKind::Var(fir::Res::Item(target), _) =
                        package.get_expr(callee).kind
                    else {
                        return;
                    };
                    let target_pkg = store.get(target.package);
                    let ItemKind::Callable(target) = &target_pkg.get_item(target.item).kind else {
                        return;
                    };
                    if !target.name.name.starts_with("Repeat") || !target.name.name.contains('{') {
                        return;
                    }
                    let qsc_fir::ty::Ty::Arrow(arrow) = &package.get_expr(callee).ty else {
                        panic!("specialized callee should be arrow-typed");
                    };
                    let input = &target_pkg.get_pat(target.input).ty;
                    assert_eq!(&*arrow.input, input, "callee metadata:\n{source}");
                    assert_eq!(
                        &package.get_expr(args).ty,
                        input,
                        "call arguments:\n{source}"
                    );
                    checked += 1;
                },
            );
        }
        assert!(
            checked >= 2,
            "entry and recursive calls should specialize:\n{source}"
        );
    }
}

#[test]
fn inline_struct_capture_arguments_are_specialized() {
    for (source, _) in crate::defunctionalize::test_cases::inline_struct_capture_cases() {
        let (store, pkg_id) =
            crate::test_utils::compile_and_run_pipeline_to(&source, crate::PipelineStage::Defunc);
        let reachable = collect_reachable_from_entry(&store, pkg_id);
        assert!(
            reachable.iter().any(|item| {
                matches!(&store.get(item.package).get_item(item.item).kind,
                    ItemKind::Callable(decl) if decl.name.name.starts_with("Read")
                        && decl.name.name.contains('{'))
            }),
            "the inline captured field should specialize Read, not merely defer:\n{source}"
        );
    }
}

#[test]
fn partial_application_capture_is_bound_before_rewritten_call() {
    let source = crate::defunctionalize::test_cases::PARTIAL_APPLICATION_CAPTURE_TIMING;
    let (store, pkg_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
    let main = crate::test_utils::callable_id_by_name(store.get(pkg_id), "Main");
    expect![[r#"
        operation Main() : Int {
            let arg : Int = Logged(17);
            Message($"ready");
            _lambda_4(arg, 1)
        }
    "#]]
    .assert_eq(&crate::pretty::write_item_qsharp_parseable(
        &store, pkg_id, main,
    ));
}

#[test]
fn pure_struct_copy_factory_is_evaluated_once() {
    let (store, pkg_id) = crate::test_utils::compile_and_run_pipeline_to(
        crate::defunctionalize::test_cases::DIRECT_STRUCT_COPY_FACTORY,
        crate::PipelineStage::Defunc,
    );
    let package = store.get(pkg_id);
    let main = crate::test_utils::find_callable(package, "Main");
    let original = crate::test_utils::callable_id_by_name(package, "Original");
    let mut calls = 0;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &main.implementation,
        &mut |_, expr| {
            if let fir::ExprKind::Call(callee, _) = expr.kind
                && let fir::ExprKind::Var(fir::Res::Item(target), _) = package.get_expr(callee).kind
                && target.package == pkg_id
                && target.item == original
            {
                calls += 1;
            }
        },
    );
    assert_eq!(calls, 1, "copied fields must share one factory evaluation");
}

#[test]
fn controlled_branch_arguments_match_declared_signatures() {
    use qsc_fir::ty::{Prim, Ty};
    for functor in [
        "Controlled",
        "Controlled Controlled",
        "Adjoint Controlled",
        "Controlled Adjoint",
        "Adjoint Controlled Controlled",
    ] {
        for (source, _) in crate::defunctionalize::test_cases::controlled_branch_cases(functor) {
            let (mut store, pkg_id) = crate::test_utils::compile_and_run_pipeline_to(
                &source,
                crate::PipelineStage::ReturnUnify,
            );
            let mut assigners = PackageAssigners::new(&store, pkg_id);
            let outcome = defunctionalize(&mut store, pkg_id, &mut assigners);
            assert_no_defunctionalization_errors(&source, &outcome.diagnostics);
            let package = store.get(pkg_id);
            let reachable = collect_reachable_from_entry(&store, pkg_id);
            let mut checked = 0;
            for item in reachable.iter().filter(|item| item.package == pkg_id) {
                let ItemKind::Callable(decl) = &package.get_item(item.item).kind else {
                    continue;
                };
                crate::walk_utils::for_each_expr_in_callable_impl(
                    package,
                    &decl.implementation,
                    &mut |_, expr| {
                        let fir::ExprKind::Call(callee, arg) = expr.kind else {
                            return;
                        };
                        let (base, applied) =
                            crate::defunctionalize::types::peel_body_functors(package, callee);
                        let fir::ExprKind::Var(fir::Res::Item(target), _) =
                            package.get_expr(base).kind
                        else {
                            return;
                        };
                        let target_package = store.get(target.package);
                        let ItemKind::Callable(target) = &target_package.get_item(target.item).kind
                        else {
                            return;
                        };
                        if !target.name.name.starts_with("Apply") || !target.name.name.contains('{')
                        {
                            return;
                        }
                        let mut expected = target_package.get_pat(target.input).ty.clone();
                        for _ in 0..applied.controlled {
                            expected = Ty::Tuple(vec![
                                Ty::Array(Box::new(Ty::Prim(Prim::Qubit))),
                                expected,
                            ]);
                        }
                        let Ty::Arrow(arrow) = &package.get_expr(callee).ty else {
                            panic!("specialized callee should be an arrow");
                        };
                        assert_eq!(&*arrow.input, &expected, "callee signature for {functor}");
                        assert_eq!(
                            &package.get_expr(arg).ty,
                            &expected,
                            "branch arguments for {functor}"
                        );
                        checked += 1;
                    },
                );
            }
            assert_eq!(checked, 2, "both branches should call a specialized Apply");
            fir_invariants::check(&store, pkg_id, InvariantLevel::PostDefunc);
        }
    }
}

#[test]
fn conditional_capture_calls_match_specialized_input_types() {
    for (source, _) in crate::defunctionalize::test_cases::conditional_capture_layout_cases()
        .chain(crate::defunctionalize::test_cases::struct_branch_cases())
        .chain(crate::defunctionalize::test_cases::nested_struct_branch_cases())
        .chain(crate::defunctionalize::test_cases::struct_copy_factory_cases())
        .chain(crate::defunctionalize::test_cases::nested_struct_copy_factory_cases())
        .chain(crate::defunctionalize::test_cases::type_constructor_argument_cases())
    {
        let (store, pkg_id) =
            crate::test_utils::compile_and_run_pipeline_to(&source, crate::PipelineStage::Defunc);
        let package = store.get(pkg_id);
        let reachable = collect_reachable_from_entry(&store, pkg_id);
        let mut checked = 0;
        for item in reachable.iter().filter(|item| item.package == pkg_id) {
            let ItemKind::Callable(decl) = &package.get_item(item.item).kind else {
                continue;
            };
            crate::walk_utils::for_each_expr_in_callable_impl(
                package,
                &decl.implementation,
                &mut |_, expr| {
                    let fir::ExprKind::Call(callee_id, args_id) = expr.kind else {
                        return;
                    };
                    let callee = package.get_expr(callee_id);
                    let fir::ExprKind::Var(fir::Res::Item(target), _) = callee.kind else {
                        return;
                    };
                    let ItemKind::Callable(target) =
                        &store.get(target.package).get_item(target.item).kind
                    else {
                        return;
                    };
                    if !target.name.name.starts_with("Apply{")
                        && !target.name.name.starts_with("Use{")
                        && !target.name.name.starts_with("Read{")
                        && !target.name.name.starts_with("ReadNested{")
                    {
                        return;
                    }
                    let qsc_fir::ty::Ty::Arrow(arrow) = &callee.ty else {
                        panic!("specialized callee should have an arrow type");
                    };
                    let input = &package.get_pat(target.input).ty;
                    assert_eq!(
                        &*arrow.input, input,
                        "callee metadata: {}",
                        target.name.name
                    );
                    assert_eq!(
                        &package.get_expr(args_id).ty,
                        input,
                        "call arguments: {}",
                        target.name.name
                    );
                    checked += 1;
                },
            );
        }
        assert_eq!(
            checked, 2,
            "both conditional branches should call specialized targets"
        );
    }
}

#[test]
fn independently_created_equivalent_callable_captures_share_specialization() {
    let source = crate::defunctionalize::test_cases::embedded_callable_source(
        "Inc",
        "Inc",
        "Apply(a, 3)*100+Apply(b, 3)",
    );
    let (store, package) =
        crate::test_utils::compile_and_run_pipeline_to(&source, crate::PipelineStage::Defunc);
    let count = store
        .get(package)
        .items
        .values()
        .filter(|item| {
            matches!(&item.kind, ItemKind::Callable(decl)
                if decl.name.name.starts_with("Apply{"))
        })
        .count();
    assert_eq!(
        count, 1,
        "equivalent embedded callable identities deduplicate"
    );
}

#[test]
fn embedded_callable_specializations_are_deterministic() {
    let source = crate::defunctionalize::test_cases::embedded_callable_source(
        "Inc",
        "Twice",
        "Apply(a, 3)*100+Apply(b, 3)",
    );
    let render = || {
        let (store, package) =
            crate::test_utils::compile_and_run_pipeline_to(&source, crate::PipelineStage::Defunc);
        crate::pretty::write_package_qsharp_parseable(&store, package)
    };
    assert_eq!(render(), render(), "specialization must be deterministic");
}

#[test]
fn immutable_capture_snapshot_remains_specializable_after_caller_mutation() {
    check_invariants(crate::defunctionalize::test_cases::IMMUTABLE_CAPTURE_SNAPSHOT);
}

#[test]
fn specialize_single_global_callable() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
        }
        "#,
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
fn specialize_two_different_callables() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
            ApplyOp(X, q);
        }
        "#,
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
fn specialize_same_callable_reuse() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
            ApplyOp(H, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(H, q);
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

/// A program with no higher-order functions is a no-op for the pass: the
/// before/after snapshots are identical because there is nothing to specialize.
#[test]
fn specialize_no_hof_unchanged() {
    check_rewrite(
        r#"
        operation Foo(q : Qubit) : Unit {
            H(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Foo(q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Foo(q : Qubit) : Unit {
                H(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Foo(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Foo(q : Qubit) : Unit {
                H(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Foo(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_closure_no_captures() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#,
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
fn specialize_closure_with_captures() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#,
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
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
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
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
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

#[test]
fn closure_callable_capture_specializations_keep_distinct_callees() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        function Wrap(inner : Qubit => Unit) : Qubit => Unit {
            q => inner(q)
        }

        operation Main() : Unit {
            use a = Qubit();
            use b = Qubit();
            ApplyOp(Wrap(H), a);
            ApplyOp(Wrap(X), b);
        }
        "#;
    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let after = crate::pretty::write_package_qsharp_parseable(&fir_store, fir_pkg_id);

    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            function Wrap(inner : (Qubit => Unit)) : (Qubit => Unit) {
                / * closure item = 4 captures = [inner] * / _lambda_4
            }
            operation Main() : Unit {
                let a : Qubit = __quantum__rt__qubit_allocate();
                let b : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(Wrap_AdjCtl_(H), a);
                ApplyOp_Empty_(Wrap_AdjCtl_(X), b);
                __quantum__rt__qubit_release(b);
                __quantum__rt__qubit_release(a);
            }
            operation _lambda_4(inner : (Qubit => Unit), q : Qubit) : Unit {
                inner(q)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            function Wrap_AdjCtl_(inner : (Qubit => Unit is Adj + Ctl)) : (Qubit => Unit) {
                / * closure item = 7 captures = [inner] * / _lambda_4
            }
            operation _lambda_4(inner : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                inner(q)
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            function Wrap(inner : (Qubit => Unit)) : (Qubit => Unit) {
                / * closure item = 4 captures = [inner] * / _lambda_4
            }
            operation Main() : Unit {
                let a : Qubit = __quantum__rt__qubit_allocate();
                let b : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(a);
                ApplyOp_Empty__X_(b);
                __quantum__rt__qubit_release(b);
                __quantum__rt__qubit_release(a);
            }
            operation _lambda_4(inner : (Qubit => Unit), q : Qubit) : Unit {
                inner(q)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            function Wrap_AdjCtl_(inner : (Qubit => Unit is Adj + Ctl)) : (Qubit => Unit) {
                inner
            }
            operation _lambda_4(inner : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                inner(q)
            }
            function Wrap_AdjCtl__H_() : (Qubit => Unit) {
                H
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            function Wrap_AdjCtl__X_() : (Qubit => Unit) {
                X
            }
            operation ApplyOp_Empty__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );

    assert!(
        after.contains("H(q);"),
        "Wrap(H) should specialize to a concrete H call:\n{after}"
    );
    assert!(
        after.contains("X(q);"),
        "Wrap(X) should specialize to a concrete X call distinct from Wrap(H):\n{after}"
    );
}

#[test]
fn specialize_closure_capture_types_preserved() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let n = 3;
            ApplyOp(q1 => { for _ in 0..n { H(q1); } }, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 3;
                ApplyOp_Empty_(/ * closure item = 3 captures = [n] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(n : Int, q1 : Qubit) : Unit {
                {
                    {
                        let _range_id_59 : Range = 0..n;
                        mutable _index_id_62 : Int = _range_id_59.Start;
                        let _step_id_67 : Int = _range_id_59.Step;
                        let _end_id_72 : Int = _range_id_59.End;
                        while ((_step_id_67 > 0) and (_index_id_62 <= _end_id_72)) or ((_step_id_67 < 0) and (_index_id_62 >= _end_id_72)) {
                            let _ : Int = _index_id_62;
                            H(q1);
                            _index_id_62 += _step_id_67;
                        }

                    }

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
                let n : Int = 3;
                ApplyOp_Empty__closure_(q, n);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(n : Int, q1 : Qubit) : Unit {
                {
                    {
                        let _range_id_59 : Range = 0..n;
                        mutable _index_id_62 : Int = _range_id_59.Start;
                        let _step_id_67 : Int = _range_id_59.Step;
                        let _end_id_72 : Int = _range_id_59.End;
                        while ((_step_id_67 > 0) and (_index_id_62 <= _end_id_72)) or ((_step_id_67 < 0) and (_index_id_62 >= _end_id_72)) {
                            let _ : Int = _index_id_62;
                            H(q1);
                            _index_id_62 += _step_id_67;
                        }

                    }

                }

            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Int) : Unit {
                _lambda_3(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

/// Adjoint applied only at the *creation site*: `Adjoint S` is passed to a HOF
/// whose body calls `op(q)` plainly, so the specialization bakes in
/// `Adjoint S(q)`. Contrast with `specialize_body_side_adjoint` (adjoint on the
/// body call) and `specialize_double_adjoint_cancels` (both, which cancel).
#[test]
fn specialize_creation_site_adjoint() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(Adjoint S, q);
        }
        "#,
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

/// Adjoint applied only on the *body call*: plain `S` is passed to a HOF whose
/// body calls `Adjoint op(q)`, so the specialization bakes in `Adjoint S(q)`.
/// Contrast with `specialize_creation_site_adjoint` (adjoint at the argument)
/// and `specialize_double_adjoint_cancels` (both, which cancel).
#[test]
fn specialize_body_side_adjoint() {
    check_rewrite(
        r#"
        operation ApplyAdj(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Adjoint op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyAdj(S, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyAdj(op : (Qubit => Unit), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyAdj_AdjCtl_(S, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Adjoint op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyAdj(op : (Qubit => Unit), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyAdj_AdjCtl__S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation ApplyAdj_AdjCtl__S_(q : Qubit) : Unit {
                Adjoint S(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// Adjoint applied at *both* the creation site (`Adjoint S`) and the body call
/// (`Adjoint op(q)`): functor composition cancels the two adjoints, so the
/// specialization bakes in plain `S(q)`. Contrast with the single-adjoint
/// siblings `specialize_creation_site_adjoint` and `specialize_body_side_adjoint`.
#[test]
fn specialize_double_adjoint_cancels() {
    check_rewrite(
        r#"
        operation ApplyAdj(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Adjoint op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyAdj(Adjoint S, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyAdj(op : (Qubit => Unit), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyAdj_AdjCtl_(Adjoint S, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Adjoint op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyAdj(op : (Qubit => Unit), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyAdj_AdjCtl__Adj_S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation ApplyAdj_AdjCtl__Adj_S_(q : Qubit) : Unit {
                S(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_body_side_controlled() {
    check_rewrite(
        r#"
        operation ApplyCtl(op : Qubit => Unit is Ctl, ctl : Qubit, q : Qubit) : Unit {
            Controlled op([ctl], q);
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyCtl(X, ctl, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyCtl(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_44 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_46 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_44, _generated_ident_46);
                ApplyCtl_AdjCtl_(X, ctl, q);
                __quantum__rt__qubit_release(_generated_ident_46);
                __quantum__rt__qubit_release(_generated_ident_44);
            }
            operation ApplyCtl_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyCtl(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_44 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_46 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_44, _generated_ident_46);
                ApplyCtl_AdjCtl__X_(ctl, q);
                __quantum__rt__qubit_release(_generated_ident_46);
                __quantum__rt__qubit_release(_generated_ident_44);
            }
            operation ApplyCtl_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation ApplyCtl_AdjCtl__X_(ctl : Qubit, q : Qubit) : Unit {
                Controlled X([ctl], q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_body_controlled_adjoint_nested() {
    check_rewrite(
        r#"
        operation ApplyCtlAdj(op : Qubit => Unit is Adj + Ctl, ctl : Qubit, q : Qubit) : Unit {
            Controlled Adjoint op([ctl], q);
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyCtlAdj(S, ctl, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyCtlAdj(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_45 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_47 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_45, _generated_ident_47);
                ApplyCtlAdj_AdjCtl_(S, ctl, q);
                __quantum__rt__qubit_release(_generated_ident_47);
                __quantum__rt__qubit_release(_generated_ident_45);
            }
            operation ApplyCtlAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint op([ctl], q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyCtlAdj(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_45 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_47 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_45, _generated_ident_47);
                ApplyCtlAdj_AdjCtl__S_(ctl, q);
                __quantum__rt__qubit_release(_generated_ident_47);
                __quantum__rt__qubit_release(_generated_ident_45);
            }
            operation ApplyCtlAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint op([ctl], q);
            }
            operation ApplyCtlAdj_AdjCtl__S_(ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint S([ctl], q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_creation_adjoint_body_controlled() {
    check_rewrite(
        r#"
        operation ApplyCtl(op : Qubit => Unit is Adj + Ctl, ctl : Qubit, q : Qubit) : Unit {
            Controlled op([ctl], q);
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyCtl(Adjoint S, ctl, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyCtl(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_45 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_47 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_45, _generated_ident_47);
                ApplyCtl_AdjCtl_(Adjoint S, ctl, q);
                __quantum__rt__qubit_release(_generated_ident_47);
                __quantum__rt__qubit_release(_generated_ident_45);
            }
            operation ApplyCtl_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyCtl(op : (Qubit => Unit), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation Main() : Unit {
                let _generated_ident_45 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_47 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_45, _generated_ident_47);
                ApplyCtl_AdjCtl__Adj_S_(ctl, q);
                __quantum__rt__qubit_release(_generated_ident_47);
                __quantum__rt__qubit_release(_generated_ident_45);
            }
            operation ApplyCtl_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), ctl : Qubit, q : Qubit) : Unit {
                Controlled op([ctl], q);
            }
            operation ApplyCtl_AdjCtl__Adj_S_(ctl : Qubit, q : Qubit) : Unit {
                Controlled Adjoint S([ctl], q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_hof_with_adj_autogen() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
            body ... { op(q); }
            adjoint auto;
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(S, q);
            Adjoint ApplyOp(S, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(S, q);
                Adjoint ApplyOp_AdjCtl_(S, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__S_(q);
                Adjoint ApplyOp_AdjCtl__S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
            }
            operation ApplyOp_AdjCtl__S_(q : Qubit) : Unit is Adj {
                body ... {
                    S(q);
                }
                adjoint ... {
                    Adjoint S(q);
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_hof_with_ctl_autogen() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit is Ctl, q : Qubit) : Unit is Ctl {
            body ... { op(q); }
            controlled auto;
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(X, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Ctl {
                body ... {
                    op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(X, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Ctl {
                body ... {
                    op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Ctl {
                body ... {
                    op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Ctl {
                body ... {
                    op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit is Ctl {
                body ... {
                    X(q);
                }
                controlled (ctls, ...) {
                    Controlled X(ctls, q);
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_hof_with_adj_ctl_autogen() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl {
            body ... { op(q); }
            adjoint auto;
            controlled auto;
            controlled adjoint auto;
        }
        operation Main() : Unit {
            use (ctl, q) = (Qubit(), Qubit());
            ApplyOp(S, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            operation Main() : Unit {
                let _generated_ident_73 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_75 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_73, _generated_ident_75);
                ApplyOp_AdjCtl_(S, q);
                __quantum__rt__qubit_release(_generated_ident_75);
                __quantum__rt__qubit_release(_generated_ident_73);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            operation Main() : Unit {
                let _generated_ident_73 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_75 : Qubit = __quantum__rt__qubit_allocate();
                let (ctl : Qubit, q : Qubit) = (_generated_ident_73, _generated_ident_75);
                ApplyOp_AdjCtl__S_(q);
                __quantum__rt__qubit_release(_generated_ident_75);
                __quantum__rt__qubit_release(_generated_ident_73);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            operation ApplyOp_AdjCtl__S_(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    S(q);
                }
                adjoint ... {
                    Adjoint S(q);
                }
                controlled (ctls, ...) {
                    Controlled S(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint S(ctls, q);
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_single_assignment_local() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let myH = H;
            ApplyOp(myH, q);
        }
        "#,
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
fn defunctionalized_call_site_drops_callable_argument() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
        }
        "#;
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
    assert_eq!(
        call_arg_tuple_lengths_after_defunc(source, "ApplyOp<AdjCtl>{H}"),
        vec![1],
        "defunctionalized ApplyOp call should pass only the qubit argument"
    );
}

#[test]
fn rewrite_closure_capture_args_inserted() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#;
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
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
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
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
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
    assert_eq!(
        call_arg_tuple_lengths_after_defunc(source, "ApplyOp<Empty>{closure}"),
        vec![2],
        "rewritten closure call should pass the qubit and captured angle"
    );
}

#[test]
fn multiple_callable_parameters_specialize_independently() {
    check_rewrite(
        r#"
        operation ApplyTwo(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit {
            f(q);
            g(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyTwo(H, X, q);
        }
        "#,
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

/// Three statically-resolved callable fields in a single tuple-typed parameter
/// are removed together in one combined specialization.
///
/// When every field of the tuple-valued parameter is a concrete callable, the
/// group is combine-eligible per `super::is_combined_eligible`, so the whole
/// `ops` slot is dropped in a single pass rather than removed one field at a
/// time across iterations. The snapshot pins the single collapsed
/// specialization `RunOps_AdjCtl__AdjCtl__AdjCtl__H__X__Y_(q)` with the fields
/// inlined in order `first -> H`, `second -> X`, `third -> Y`; a field-index
/// mix-up would inline the gates out of order or dispatch the wrong callable.
///
/// The per-field `reindex_sibling_field_access` path, which shifts surviving
/// siblings down as each is removed, is exercised only when the group is not
/// combine-eligible, for example when the tuple's fields are only partially
/// covered by concrete callables.
#[test]
fn three_callable_field_tuple_param_combines_into_one_spec() {
    check_rewrite(
        r#"
        operation RunOps(ops : (Qubit => Unit, Qubit => Unit, Qubit => Unit), q : Qubit) : Unit {
            let (first, second, third) = ops;
            first(q);
            second(q);
            third(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            RunOps((H, X, Y), q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation RunOps(ops : ((Qubit => Unit), (Qubit => Unit), (Qubit => Unit)), q : Qubit) : Unit {
                let (first : (Qubit => Unit), second : (Qubit => Unit), third : (Qubit => Unit)) = ops;
                first(q);
                second(q);
                third(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunOps_AdjCtl__AdjCtl__AdjCtl_((H, X, Y), q);
                __quantum__rt__qubit_release(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl_(ops : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl)), q : Qubit) : Unit {
                let (first : (Qubit => Unit is Adj + Ctl), second : (Qubit => Unit is Adj + Ctl), third : (Qubit => Unit is Adj + Ctl)) = ops;
                first(q);
                second(q);
                third(q);
            }
            // entry
            Main()

            AFTER:
            operation RunOps(ops : ((Qubit => Unit), (Qubit => Unit), (Qubit => Unit)), q : Qubit) : Unit {
                let (first : (Qubit => Unit), second : (Qubit => Unit), third : (Qubit => Unit)) = ops;
                first(q);
                second(q);
                third(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunOps_AdjCtl__AdjCtl__AdjCtl__H__X__Y_(q);
                __quantum__rt__qubit_release(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl_(ops : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl)), q : Qubit) : Unit {
                let (first : (Qubit => Unit is Adj + Ctl), second : (Qubit => Unit is Adj + Ctl), third : (Qubit => Unit is Adj + Ctl)) = ops;
                first(q);
                second(q);
                third(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl__H__X__Y_(q : Qubit) : Unit {
                H(q);
                X(q);
                Y(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// The combined rewrite reduces a non-inline argument: a single tuple-valued
/// parameter HOF called with a pre-bound tuple local such as `let ops = (H, X,
/// Y); RunOps(ops)` rather than an inline tuple literal.
///
/// Because the argument is `Var(ops)`, the rewrite cannot drop tuple slots in
/// place; retained fields are projected from the held value unless initializer
/// replay is separately proven stable and unobservable. Here
/// every field is a global callable removed together, so the reduced call takes
/// no arguments, the now-dead `let ops` binding is pruned, and the collapsed
/// specialization inlines `H, X, Y` in order. A projection error would leave the
/// arrow-typed `let ops` binding behind or pass a stale full-arity argument.
#[test]
fn bound_tuple_arg_combines_into_one_spec() {
    check_rewrite(
        r#"
        operation RunOps(ops : (Qubit => Unit, Qubit => Unit, Qubit => Unit)) : Unit {
            use q = Qubit();
            let (first, second, third) = ops;
            first(q);
            second(q);
            third(q);
        }
        operation Main() : Unit {
            let ops = (H, X, Y);
            RunOps(ops);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation RunOps(ops : ((Qubit => Unit), (Qubit => Unit), (Qubit => Unit))) : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let (first : (Qubit => Unit), second : (Qubit => Unit), third : (Qubit => Unit)) = ops;
                first(q);
                second(q);
                third(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                let ops : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl)) = (H, X, Y);
                RunOps_AdjCtl__AdjCtl__AdjCtl_(ops);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl_(ops : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl))) : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let (first : (Qubit => Unit is Adj + Ctl), second : (Qubit => Unit is Adj + Ctl), third : (Qubit => Unit is Adj + Ctl)) = ops;
                first(q);
                second(q);
                third(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation RunOps(ops : ((Qubit => Unit), (Qubit => Unit), (Qubit => Unit))) : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let (first : (Qubit => Unit), second : (Qubit => Unit), third : (Qubit => Unit)) = ops;
                first(q);
                second(q);
                third(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                RunOps_AdjCtl__AdjCtl__AdjCtl__H__X__Y_();
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl_(ops : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl), (Qubit => Unit is Adj + Ctl))) : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let (first : (Qubit => Unit is Adj + Ctl), second : (Qubit => Unit is Adj + Ctl), third : (Qubit => Unit is Adj + Ctl)) = ops;
                first(q);
                second(q);
                third(q);
                __quantum__rt__qubit_release(q);
            }
            operation RunOps_AdjCtl__AdjCtl__AdjCtl__H__X__Y_() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                H(q);
                X(q);
                Y(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn indexed_callable_array_preserves_duplicate_global_positions() {
    let source = r#"
        operation ApplyAt(ops : (Qubit => Unit)[], idx : Int, q : Qubit) : Unit {
            ops[idx](q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyAt([H, H, X], 1, q);
        }
        "#;

    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let package = fir_store.get(fir_pkg_id);
    let mut matching_targets = Vec::new();
    for item in package.items.values() {
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        if !decl.name.name.starts_with("ApplyAt") || decl.name.name.as_ref() == "ApplyAt" {
            continue;
        }
        let mut targets = Vec::new();
        crate::walk_utils::for_each_expr_in_callable_impl(
            package,
            &decl.implementation,
            &mut |_expr_id, expr| {
                if let fir::ExprKind::Call(callee_id, _) = &expr.kind
                    && let Some(target) = call_target_name(&fir_store, package, *callee_id)
                    && matches!(target.as_str(), "H" | "X")
                {
                    targets.push(target);
                }
            },
        );
        targets.sort();
        matching_targets.push((decl.name.name.to_string(), targets));
    }

    assert!(
        matching_targets
            .iter()
            .any(|(_, targets)| targets == &["H", "H", "X"]),
        "expected one ApplyAt specialization to dispatch [H, H, X], got {matching_targets:?}"
    );
}

#[test]
fn struct_copy_surviving_field_is_forwarded_after_callable_field_removal() {
    let source = r#"
        struct Config { Op : Qubit => Unit, Data : Int }
        operation Run(config : Config, q : Qubit) : Unit {
            if config.Data == 1 {
                config.Op(q);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            let base = new Config { Op = X, Data = 1 };
            Run(new Config { ...base, Op = H }, q);
        }
        "#;

    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let package = fir_store.get(fir_pkg_id);
    let mut found = false;
    for expr in package.exprs.values() {
        let fir::ExprKind::Call(callee_id, args_id) = &expr.kind else {
            continue;
        };
        let Some(target) = call_target_name(&fir_store, package, *callee_id) else {
            continue;
        };
        if !target.starts_with("Run") || target == "Run" {
            continue;
        }

        let args = package.get_expr(*args_id);
        let fir::ExprKind::Tuple(elements) = &args.kind else {
            panic!("specialized Run call should receive Data and q, got {args:?}");
        };
        assert_eq!(
            elements.len(),
            2,
            "specialized Run call must keep copied Data and q"
        );
        assert!(
            matches!(
                package.get_expr(elements[0]).ty,
                qsc_fir::ty::Ty::Prim(qsc_fir::ty::Prim::Int)
            ),
            "first specialized Run argument must be the surviving copied Int field"
        );
        found = true;
    }
    assert!(found, "expected a specialized Run call for H");
}

#[test]
fn capture_local_ids_are_reasonable() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#,
    );
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners).diagnostics;
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    let mut capture_binding_count = 0;
    for (_, pat) in &package.pats {
        if let fir::PatKind::Bind(ident) = &pat.kind {
            let id: u32 = ident.id.into();
            assert!(
                id < 10_000,
                "LocalVarId {id} is unreasonably large -- capture IDs should be sequential, not u32::MAX-based"
            );
            if ident.name.starts_with(CAPTURE_NAME_PREFIX) {
                capture_binding_count += 1;
            }
        }
    }
    assert_eq!(
        capture_binding_count, 1,
        "the `angle` capture should produce exactly one capture binding, proving the \
         capture-threading path actually ran rather than vacuously passing with no captures"
    );
}

#[test]
fn pipeline_with_captures_no_tuple_decompose_panic() {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (_store, _pkg_id) = compile_and_run_pipeline_to(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let pair = (1.0, 2.0);
            let (a, b) = pair;
            ApplyOp(q1 => Rx(a + b, q1), q);
        }
        "#,
        PipelineStage::Full,
    );
}

#[test]
fn multiple_captures_sequential_ids() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let a = 1.0;
            let b = 2.0;
            let c = 3.0;
            ApplyOp(q1 => { Rx(a, q1); Ry(b, q1); Rz(c, q1); }, q);
        }
        "#,
    );
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners).diagnostics;
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    let mut capture_ids: Vec<u32> = Vec::new();
    for (_, pat) in &package.pats {
        if let fir::PatKind::Bind(ident) = &pat.kind
            && ident.name.starts_with(CAPTURE_NAME_PREFIX)
        {
            let id: u32 = ident.id.into();
            capture_ids.push(id);
        }
    }

    assert!(
        capture_ids.len() >= 3,
        "expected at least 3 capture bindings, found {}",
        capture_ids.len()
    );

    for &id in &capture_ids {
        assert!(id < 10_000, "capture LocalVarId {id} is unreasonably large");
    }

    capture_ids.sort_unstable();
    for window in capture_ids.windows(2) {
        assert_eq!(
            window[1] - window[0],
            1,
            "capture IDs should be sequential, got {} and {}",
            window[0],
            window[1]
        );
    }
}

#[test]
fn specialize_closure_capturing_immutable_variable() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#,
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
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
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
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
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

#[test]
fn specialize_closure_in_while_loop_body() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            mutable n = 3;
            while n > 0 {
                ApplyOp(q1 => H(q1), q);
                n -= 1;
            }
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable n : Int = 3;
                let _generated_ident_62 : Unit = while n > 0 {
                    ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_3, q);
                    n -= 1;
                };
                __quantum__rt__qubit_release(q);
                _generated_ident_62
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
                mutable n : Int = 3;
                let _generated_ident_62 : Unit = while n > 0 {
                    ApplyOp_Empty__H_(q);
                    n -= 1;
                };
                __quantum__rt__qubit_release(q);
                _generated_ident_62
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
fn specialize_multiple_closures_same_signature() {
    check_rewrite(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
            ApplyOp(q1 => X(q1), q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_3, q);
                ApplyOp_Empty_(/ * closure item = 4 captures = [] * / _lambda_4, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation _lambda_4(q1 : Qubit, ) : Unit {
                X(q1)
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
                ApplyOp_Empty__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation _lambda_4(q1 : Qubit, ) : Unit {
                X(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            operation ApplyOp_Empty__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn branch_split_two_callees() {
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
fn branch_split_three_callees() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } elif false { X } else { S };
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
                } else if false {
                    X
                } else {
                    S
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
                } else if false {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__S_(q)
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
            operation ApplyOp_AdjCtl__S_(q : Qubit) : Unit {
                S(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn branch_split_mutable_conditional() {
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
fn branch_split_nested_callable_in_tuple() {
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
    check_invariants(source);
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
fn branch_split_nested_callable_in_tuple_args_consistency() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } else { X };
            Wrapper((f, 42), q);
        }
        "#,
    );
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners).diagnostics;
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    let mut mismatches = Vec::new();
    for (expr_id, expr) in &package.exprs {
        if let fir::ExprKind::Call(_callee_id, args_id) = &expr.kind {
            let args_expr = package.get_expr(*args_id);
            if let fir::ExprKind::Tuple(elements) = &args_expr.kind
                && let qsc_fir::ty::Ty::Tuple(type_elems) = &args_expr.ty
            {
                if elements.len() != type_elems.len() {
                    mismatches.push(format!(
                        "Call expr {expr_id}: args tuple has {} elements but type has {} elements",
                        elements.len(),
                        type_elems.len()
                    ));
                }
                for (i, (&elem_id, ty_elem)) in elements.iter().zip(type_elems.iter()).enumerate() {
                    let elem_expr = package.get_expr(elem_id);
                    let elem_is_tuple = matches!(elem_expr.kind, fir::ExprKind::Tuple(_));
                    let ty_is_tuple = matches!(ty_elem, qsc_fir::ty::Ty::Tuple(_));
                    if elem_is_tuple != ty_is_tuple {
                        mismatches.push(format!(
                            "Call expr {expr_id}: args[{i}] is_tuple={elem_is_tuple} but type is_tuple={ty_is_tuple} (elem_ty={}, type_elem={ty_elem})",
                            elem_expr.ty,
                        ));
                    }
                }
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "Type/value mismatches in branch-split args:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn branch_split_nested_callable_full_pipeline() {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (_store, _pkg_id) = compile_and_run_pipeline_to(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let f = if true { H } else { X };
            Wrapper((f, 42), q);
        }
        "#,
        PipelineStage::Full,
    );
}

#[test]
fn specialize_nested_callable_first_element() {
    check_rewrite(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
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
fn specialize_nested_callable_second_element() {
    check_rewrite(
        r#"
        operation Wrapper(pair : (Int, Qubit => Unit), q : Qubit) : Unit {
            let (_, op) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((42, H), q);
        }
        "#,
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
fn specialize_nested_callable_both_fields_used() {
    check_rewrite(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, n) = pair;
            op(q);
            let _ = n;
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), n : Int) = pair;
                op(q);
                let _ : Int = n;
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_((H, 42), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), n : Int) = pair;
                op(q);
                let _ : Int = n;
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), n : Int) = pair;
                op(q);
                let _ : Int = n;
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_(42, q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), n : Int) = pair;
                op(q);
                let _ : Int = n;
            }
            operation Wrapper_AdjCtl__H_(pair : Int, q : Qubit) : Unit {
                let n : Int = pair;
                H(q);
                let _ : Int = n;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_nested_callable_transitive_alias() {
    check_rewrite(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            let f = op;
            f(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                let f : (Qubit => Unit) = op;
                f(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_((H, 42), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                let f : (Qubit => Unit is Adj + Ctl) = op;
                f(q);
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), _ : Int) = pair;
                let f : (Qubit => Unit) = op;
                f(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_(42, q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), _ : Int) = pair;
                let f : (Qubit => Unit is Adj + Ctl) = op;
                f(q);
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
fn specialize_nested_callable_through_aggregate_alias() {
    check_rewrite(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let alias = pair;
            let (op, n) = alias;
            op(q);
            let _ = n;
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let alias : ((Qubit => Unit), Int) = pair;
                let (op : (Qubit => Unit), n : Int) = alias;
                op(q);
                let _ : Int = n;
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_((H, 42), q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let alias : ((Qubit => Unit is Adj + Ctl), Int) = pair;
                let (op : (Qubit => Unit is Adj + Ctl), n : Int) = alias;
                op(q);
                let _ : Int = n;
            }
            // entry
            Main()

            AFTER:
            operation Wrapper(pair : ((Qubit => Unit), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit), n : Int) = pair;
                op(q);
                let _ : Int = n;
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_(42, q);
                __quantum__rt__qubit_release(q);
            }
            operation Wrapper_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Int), q : Qubit) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), n : Int) = pair;
                op(q);
                let _ : Int = n;
            }
            operation Wrapper_AdjCtl__H_(pair : Int, q : Qubit) : Unit {
                let n : Int = pair;
                H(q);
                let _ : Int = n;
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn specialize_nested_callable_invariants() {
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
    check_invariants(source);
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
fn specialize_nested_callable_full_pipeline() {
    use crate::test_utils::{PipelineStage, compile_and_run_pipeline_to};

    let (_store, _pkg_id) = compile_and_run_pipeline_to(
        r#"
        operation Wrapper(pair : (Qubit => Unit, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((H, 42), q);
        }
        "#,
        PipelineStage::Full,
    );
}

#[test]
fn branch_split_nested_callable_adj_ctl_args_consistency() {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation Op1(q : Qubit) : Unit is Adj + Ctl { H(q); }
        operation Op2(q : Qubit) : Unit is Adj + Ctl { X(q); }
        operation Wrapper(pair : (Qubit => Unit is Adj + Ctl, Int), q : Qubit) : Unit {
            let (op, _) = pair;
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let b = true;
            let f = if b { Op1 } else { Op2 };
            Wrapper((f, 42), q);
        }
        "#,
    );
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners).diagnostics;
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let package = fir_store.get(fir_pkg_id);

    let mut mismatches = Vec::new();
    for (expr_id, expr) in &package.exprs {
        if let fir::ExprKind::Call(_callee_id, args_id) = &expr.kind {
            let args_expr = package.get_expr(*args_id);
            if let fir::ExprKind::Tuple(elements) = &args_expr.kind
                && let qsc_fir::ty::Ty::Tuple(type_elems) = &args_expr.ty
                && elements.len() != type_elems.len()
            {
                mismatches.push(format!(
                    "Call expr {expr_id}: args tuple has {} elements but type has {} elements",
                    elements.len(),
                    type_elems.len()
                ));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "Type/value mismatches in branch-split args:\n{}",
        mismatches.join("\n")
    );
}

#[test]
fn closure_with_multiple_captures_threads_all_captures() {
    check_rewrite(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let angle1 = 1.0;
            let angle2 = 2.0;
            let myOp = (q) => { Rx(angle1, q); Ry(angle2, q); };
            Apply(myOp, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle1 : Double = 1.;
                let angle2 : Double = 2.;
                let myOp : (Qubit => Unit) = / * closure item = 3 captures = [angle1, angle2] * / _lambda_3;
                Apply_Empty_(myOp, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle1 : Double, angle2 : Double, q : Qubit) : Unit {
                {
                    Rx(angle1, q);
                    Ry(angle2, q);
                }

            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle1 : Double = 1.;
                let angle2 : Double = 2.;
                Apply_Empty__closure_(q, angle1, angle2);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle1 : Double, angle2 : Double, q : Qubit) : Unit {
                {
                    Rx(angle1, q);
                    Ry(angle2, q);
                }

            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_Empty__closure_(q : Qubit, __capture_0 : Double, __capture_1 : Double) : Unit {
                _lambda_3(__capture_0, __capture_1, q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_param_tuple_containing_arrow_specializes_end_to_end() {
    check_rewrite(
        r#"
        operation Apply(pair : (Qubit => Unit, Qubit)) : Unit {
            let (op, q) = pair;
            op(q);
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Apply((H, q));
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Apply(pair : ((Qubit => Unit), Qubit)) : Unit {
                let (op : (Qubit => Unit), q : Qubit) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Apply_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation Apply_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Qubit)) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), q : Qubit) = pair;
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(pair : ((Qubit => Unit), Qubit)) : Unit {
                let (op : (Qubit => Unit), q : Qubit) = pair;
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Apply_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation Apply_AdjCtl_(pair : ((Qubit => Unit is Adj + Ctl), Qubit)) : Unit {
                let (op : (Qubit => Unit is Adj + Ctl), q : Qubit) = pair;
                op(q);
            }
            operation Apply_AdjCtl__H_(pair : Qubit) : Unit {
                let q : Qubit = pair;
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn single_param_tuple_second_element_specializes_end_to_end() {
    check_rewrite(
        r#"
        operation Wrapper(pair : (Int, Qubit => Unit)) : Unit {
            let (_, op) = pair;
            use q = Qubit();
            op(q);
        }
        operation Main() : Unit {
            Wrapper((42, H));
        }
        "#,
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
fn single_param_recursive_tuple_callable_specializes_end_to_end() {
    check_rewrite(
        r#"
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
        "#,
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
fn recursive_hof_function_specialization_remaps_self_call_to_specialization() {
    let source = r#"
        function Repeat(op : Int -> Int, n : Int, q : Qubit) : Unit {
            if n > 0 {
                let i : Int = op(n);
                Repeat(Id, i - 1, q);
            }
        }
        function Id(x : Int) : Int { x }
        operation Main() : Unit {
            use q = Qubit();
            Repeat(Id, 2, q);
        }
        "#;

    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let package = fir_store.get(fir_pkg_id);
    let repeat_names = package
        .items
        .values()
        .filter_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.starts_with("Repeat") => {
                Some(decl.name.name.to_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let repeat_specializations = repeat_names
        .iter()
        .filter(|name| name.contains("{Id}"))
        .collect::<Vec<_>>();
    assert_eq!(
        repeat_specializations.len(),
        1,
        "expected one H-specialized Repeat callable, got {repeat_specializations:?} from {repeat_names:?}"
    );

    let repeat_specialization = repeat_specializations[0];
    let targets = callable_call_targets_after_defunc(source, repeat_specialization);
    assert!(
        targets.contains(repeat_specialization),
        "recursive specialization {repeat_specialization} should call itself, got targets {targets:?}"
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            function Repeat(op : (Int -> Int), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    let i : Int = op(n);
                    Repeat(Id, i - 1, q);
                }

            }
            function Id(x : Int) : Int {
                x
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Repeat(Id, 2, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            function Repeat(op : (Int -> Int), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    let i : Int = op(n);
                    Repeat_Id_(i - 1, q);
                }

            }
            function Id(x : Int) : Int {
                x
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Repeat_Id_(2, q);
                __quantum__rt__qubit_release(q);
            }
            function Repeat_Id_(n : Int, q : Qubit) : Unit {
                if n > 0 {
                    let i : Int = Id(n);
                    Repeat_Id_(i - 1, q);
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn recursive_hof_specialization_remaps_self_call_to_specialization() {
    let source = r#"
        operation Repeat(op : Qubit => Unit, n : Int, q : Qubit) : Unit {
            if n > 0 {
                op(q);
                Repeat(H, n - 1, q);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            Repeat(H, 2, q);
        }
        "#;

    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let package = fir_store.get(fir_pkg_id);
    let repeat_names = package
        .items
        .values()
        .filter_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.starts_with("Repeat") => {
                Some(decl.name.name.to_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let repeat_specializations = repeat_names
        .iter()
        .filter(|name| name.contains("{H}"))
        .collect::<Vec<_>>();
    assert_eq!(
        repeat_specializations.len(),
        1,
        "expected one H-specialized Repeat callable, got {repeat_specializations:?} from {repeat_names:?}"
    );

    let repeat_specialization = repeat_specializations[0];
    let targets = callable_call_targets_after_defunc(source, repeat_specialization);
    assert!(
        targets.contains(repeat_specialization),
        "recursive specialization {repeat_specialization} should call itself, got targets {targets:?}"
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Repeat(op : (Qubit => Unit), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_AdjCtl_(H, n - 1, q);
                }

            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Repeat_AdjCtl_(H, 2, q);
                __quantum__rt__qubit_release(q);
            }
            operation Repeat_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_AdjCtl_(H, n - 1, q);
                }

            }
            // entry
            Main()

            AFTER:
            operation Repeat(op : (Qubit => Unit), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_AdjCtl_(H, n - 1, q);
                }

            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Repeat_AdjCtl__H_(2, q);
                __quantum__rt__qubit_release(q);
            }
            operation Repeat_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_AdjCtl__H_(n - 1, q);
                }

            }
            operation Repeat_AdjCtl__H_(n : Int, q : Qubit) : Unit {
                if n > 0 {
                    H(q);
                    Repeat_AdjCtl__H_(n - 1, q);
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn recursive_multi_param_hof_specialization_remaps_self_call_to_specialization() {
    let source = r#"
        operation RepeatPair(n : Int, first : Qubit => Unit, second : Qubit => Unit, q : Qubit) : Unit {
            if n > 0 {
                first(q);
                second(q);
                RepeatPair(n - 1, H, X, q);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            RepeatPair(2, H, X, q);
        }
        "#;

    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let package = fir_store.get(fir_pkg_id);
    let repeat_names = package
        .items
        .values()
        .filter_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.starts_with("RepeatPair") => {
                Some(decl.name.name.to_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let repeat_specializations = repeat_names
        .iter()
        .filter(|name| name.contains("{H}") && name.contains("{X}"))
        .collect::<Vec<_>>();
    assert_eq!(
        repeat_specializations.len(),
        1,
        "expected one H/X-specialized RepeatPair callable, got {repeat_specializations:?} from {repeat_names:?}"
    );

    let repeat_specialization = repeat_specializations[0];
    let targets = callable_call_targets_after_defunc(source, repeat_specialization);
    assert!(
        targets.contains(repeat_specialization),
        "recursive specialization {repeat_specialization} should call itself, got targets {targets:?}"
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation RepeatPair(n : Int, first : (Qubit => Unit), second : (Qubit => Unit), q : Qubit) : Unit {
                if n > 0 {
                    first(q);
                    second(q);
                    RepeatPair_AdjCtl__AdjCtl_(n - 1, H, X, q);
                }

            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RepeatPair_AdjCtl__AdjCtl_(2, H, X, q);
                __quantum__rt__qubit_release(q);
            }
            operation RepeatPair_AdjCtl__AdjCtl_(n : Int, first : (Qubit => Unit is Adj + Ctl), second : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                if n > 0 {
                    first(q);
                    second(q);
                    RepeatPair_AdjCtl__AdjCtl_(n - 1, H, X, q);
                }

            }
            // entry
            Main()

            AFTER:
            operation RepeatPair(n : Int, first : (Qubit => Unit), second : (Qubit => Unit), q : Qubit) : Unit {
                if n > 0 {
                    first(q);
                    second(q);
                    RepeatPair_AdjCtl__AdjCtl_(n - 1, H, X, q);
                }

            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RepeatPair_AdjCtl__AdjCtl__H__X_(2, q);
                __quantum__rt__qubit_release(q);
            }
            operation RepeatPair_AdjCtl__AdjCtl_(n : Int, first : (Qubit => Unit is Adj + Ctl), second : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                if n > 0 {
                    first(q);
                    second(q);
                    RepeatPair_AdjCtl__AdjCtl__H__X_(n - 1, q);
                }

            }
            operation RepeatPair_AdjCtl__AdjCtl__H__X_(n : Int, q : Qubit) : Unit {
                if n > 0 {
                    H(q);
                    X(q);
                    RepeatPair_AdjCtl__AdjCtl__H__X_(n - 1, q);
                }

            }
            // entry
            Main()
        "#]],
    );
}

// Regression for the soundness of the conditional, key-matched recursive
// self-call remap. When a used-parameter recursive HOF forwards a *different*
// global callable on its self-call than the one it was entered with, the
// self-call must be routed to the sibling specialization for that forwarded
// callable rather than folded into a self-loop on the current specialization.
//
// Here `Repeat` is entered with `H` but its recursive self-call forwards `X`.
// The correct lowering emits the sequence `H, X`: the `H` specialization runs
// `H(q)` and then calls the `X` sibling specialization, which runs `X(q)` and
// recurses on itself. An unconditional remap (the pre-fix behavior) would
// instead retarget the self-call back into the `H` specialization, silently
// emitting `H, H` and leaving the `X` specialization orphaned.
//
// Note: the sibling-routing path currently mints the `X` specialization more
// than once (one copy is reachable via the `H` specialization and one is dead
// code reached only through the now-unreachable generic HOF). This is a
// redundant-specialization inefficiency, not a miscompile -- the reachable
// gate sequence is still `H, X` -- so the assertions below key off the
// specialization *names* rather than a specialization count.
#[test]
fn recursive_hof_self_call_with_different_callable_routes_to_sibling_specialization() {
    let source = r#"
        operation Repeat(op : Qubit => Unit, n : Int, q : Qubit) : Unit {
            if n > 0 {
                op(q);
                Repeat(X, n - 1, q);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            Repeat(H, 2, q);
        }
        "#;

    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let package = fir_store.get(fir_pkg_id);
    let repeat_names = package
        .items
        .values()
        .filter_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.starts_with("Repeat") => {
                Some(decl.name.name.to_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let h_specializations = repeat_names
        .iter()
        .filter(|name| name.contains("{H}"))
        .collect::<Vec<_>>();
    let x_specializations = repeat_names
        .iter()
        .filter(|name| name.contains("{X}"))
        .collect::<Vec<_>>();
    assert_eq!(
        h_specializations.len(),
        1,
        "expected one H-specialized Repeat callable, got {h_specializations:?} from {repeat_names:?}"
    );
    assert!(
        !x_specializations.is_empty(),
        "expected at least one X-specialized Repeat callable, got {x_specializations:?} from {repeat_names:?}"
    );

    let h_specialization = h_specializations[0];
    let x_specialization = x_specializations[0];

    // The H specialization forwards `X` on its self-call, so it must call the
    // X sibling specialization and must not self-loop (a self-loop would emit
    // the incorrect sequence H, H instead of the correct H, X).
    let h_targets = callable_call_targets_after_defunc(source, h_specialization);
    assert!(
        h_targets.contains(x_specialization),
        "H specialization {h_specialization} should call the X sibling specialization \
         {x_specialization}, got targets {h_targets:?}"
    );
    assert!(
        !h_targets.contains(h_specialization),
        "H specialization {h_specialization} must not self-loop (that would emit H, H), \
         got targets {h_targets:?}"
    );

    // The X specialization forwards `X` on its self-call, which matches its own
    // key, so it self-loops.
    let x_targets = callable_call_targets_after_defunc(source, x_specialization);
    assert!(
        x_targets.contains(x_specialization),
        "X specialization {x_specialization} should self-loop, got targets {x_targets:?}"
    );

    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Repeat(op : (Qubit => Unit), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_AdjCtl_(X, n - 1, q);
                }

            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Repeat_AdjCtl_(H, 2, q);
                __quantum__rt__qubit_release(q);
            }
            operation Repeat_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_AdjCtl_(X, n - 1, q);
                }

            }
            // entry
            Main()

            AFTER:
            operation Repeat(op : (Qubit => Unit), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_AdjCtl_(X, n - 1, q);
                }

            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Repeat_AdjCtl__H_(2, q);
                __quantum__rt__qubit_release(q);
            }
            operation Repeat_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat_AdjCtl__X_(n - 1, q);
                }

            }
            operation Repeat_AdjCtl__H_(n : Int, q : Qubit) : Unit {
                if n > 0 {
                    H(q);
                    Repeat_AdjCtl__X_(n - 1, q);
                }

            }
            operation Repeat_AdjCtl__X_(n : Int, q : Qubit) : Unit {
                if n > 0 {
                    X(q);
                    Repeat_AdjCtl__X_(n - 1, q);
                }

            }
            operation Repeat_AdjCtl__X_(n : Int, q : Qubit) : Unit {
                if n > 0 {
                    X(q);
                    Repeat_AdjCtl__X_(n - 1, q);
                }

            }
            // entry
            Main()
        "#]],
    );
}

// The exact repro reported for degenerate recursive defunctionalization. `F`'s
// callable parameter is unused, so every specialization collapses to the same
// body once the callable slot is stripped, and the fixpoint loop converges on a
// small bounded specialization set. This must complete with no diagnostics --
// no fixpoint-not-reached, no dynamic-callable, and no excessive-specialization
// warning.
#[test]
fn recursive_unused_param_identity_lambda_converges_clean() {
    let source = r#"
        operation Main() : Unit {
            F(x => x)
        }
        operation F(f : () => ()) : Unit {
            F(() => F(x => x));
        }
        "#;

    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners).diagnostics;
    assert!(
        errors.is_empty(),
        "expected clean convergence with no diagnostics, got:\n{}",
        format_defunctionalization_errors(&errors)
    );
}

#[test]
fn single_param_recursive_tuple_callable_closure_capture_invariants() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Wrapper(bundle : (((Qubit => Unit, Int), Double), Qubit)) : Unit {
            let (((op, n), angle), q) = bundle;
            ApplyOp(
                q1 => {
                    if n == 0 {
                        Rx(angle, q1);
                    }
                    op(q1);
                },
                q
            );
        }
        operation Main() : Unit {
            use q = Qubit();
            Wrapper((((H, 0), 1.0), q));
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
            operation Wrapper(bundle : ((((Qubit => Unit), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit), n : Int), angle : Double), q : Qubit) = bundle;
                ApplyOp_Empty_(/ * closure item = 4 captures = [op, n, angle] * / _lambda_4, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl_(((H, 0), 1.), q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_4(op : (Qubit => Unit), n : Int, angle : Double, q1 : Qubit) : Unit {
                {
                    if n == 0 {
                        Rx(angle, q1);
                    }

                    op(q1);
                }

            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Wrapper_AdjCtl_(bundle : ((((Qubit => Unit is Adj + Ctl), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit is Adj + Ctl), n : Int), angle : Double), q : Qubit) = bundle;
                ApplyOp_Empty_(/ * closure item = 7 captures = [op, n, angle] * / _lambda_4, q);
            }
            operation _lambda_4(op : (Qubit => Unit is Adj + Ctl), n : Int, angle : Double, q1 : Qubit) : Unit {
                {
                    if n == 0 {
                        Rx(angle, q1);
                    }

                    op(q1);
                }

            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Wrapper(bundle : ((((Qubit => Unit), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit), n : Int), angle : Double), q : Qubit) = bundle;
                ApplyOp_Empty_(/ * closure item = 4 captures = [op, n, angle] * / _lambda_4, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Wrapper_AdjCtl__H_((0, 1.), q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_4(op : (Qubit => Unit), n : Int, angle : Double, q1 : Qubit) : Unit {
                {
                    if n == 0 {
                        Rx(angle, q1);
                    }

                    op(q1);
                }

            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Wrapper_AdjCtl_(bundle : ((((Qubit => Unit is Adj + Ctl), Int), Double), Qubit)) : Unit {
                let (((op : (Qubit => Unit is Adj + Ctl), n : Int), angle : Double), q : Qubit) = bundle;
                ApplyOp_Empty__closure_(q, op, n, angle);
            }
            operation _lambda_4(op : (Qubit => Unit is Adj + Ctl), n : Int, angle : Double, q1 : Qubit) : Unit {
                {
                    if n == 0 {
                        Rx(angle, q1);
                    }

                    op(q1);
                }

            }
            operation Wrapper_AdjCtl__H_(bundle : ((Int, Double), Qubit)) : Unit {
                let ((n : Int, angle : Double), q : Qubit) = bundle;
                ApplyOp_Empty__closure_(q, n, angle);
            }
            operation _lambda_4(n : Int, angle : Double, q1 : Qubit) : Unit {
                {
                    if n == 0 {
                        Rx(angle, q1);
                    }

                    H(q1);
                }

            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : (Qubit => Unit is Adj + Ctl), __capture_1 : Int, __capture_2 : Double) : Unit {
                _lambda_4(__capture_0, __capture_1, __capture_2, q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Int, __capture_1 : Double) : Unit {
                _lambda_4(__capture_0, __capture_1, q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn three_branch_conditional_callable_generates_branch_split() {
    let source = r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let n = 2;
            mutable op = H;
            if n == 0 {
                op = X;
            } elif n == 1 {
                op = Y;
            } else {
                op = Z;
            }
            Apply(op, q);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 2;
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if n == 0 {
                    op = X;
                } else if n == 1 {
                    op = Y;
                } else {
                    op = Z;
                }

                Apply_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation Apply_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 2;
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if n == 0 {
                    op = X;
                } else if n == 1 {
                    op = Y;
                } else {
                    op = Z;
                }

                if n == 0 {
                    Apply_AdjCtl__X_(q)
                } else if n == 1 {
                    Apply_AdjCtl__Y_(q)
                } else {
                    Apply_AdjCtl__Z_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation Apply_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation Apply_AdjCtl__Y_(q : Qubit) : Unit {
                Y(q);
            }
            operation Apply_AdjCtl__Z_(q : Qubit) : Unit {
                Z(q);
            }
            // entry
            Main()
        "#]],
    );
    let targets = callable_call_targets_after_defunc(source, "Main");
    assert!(
        targets.contains(&"Apply<AdjCtl>{X}".to_string())
            && targets.contains(&"Apply<AdjCtl>{Y}".to_string())
            && targets.contains(&"Apply<AdjCtl>{Z}".to_string()),
        "branch split should call X, Y, and Z specializations, got {targets:?}"
    );
}

#[test]
fn identity_closure_peephole_replaces_wrapper() {
    check_rewrite(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let wrapper = q => H(q);
            Apply(wrapper, q);
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let wrapper : (Qubit => Unit) = / * closure item = 3 captures = [] * / _lambda_3;
                Apply_Empty_(wrapper, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q : Qubit, ) : Unit {
                H(q)
            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Apply_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q : Qubit, ) : Unit {
                H(q)
            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn excessive_specializations_warning_emitted() {
    // A HOF called with > 10 different concrete closures triggers the
    // ExcessiveSpecializations warning. Each explicit `q1 => Rx(angle, q1)`
    // lambda has a distinct lifted target, and all closures are passed to the
    // same functorless Apply<Empty> variant.
    check_errors(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            Apply(q1 => Rx(1.0, q1), q);
            Apply(q1 => Rx(2.0, q1), q);
            Apply(q1 => Rx(3.0, q1), q);
            Apply(q1 => Rx(4.0, q1), q);
            Apply(q1 => Rx(5.0, q1), q);
            Apply(q1 => Rx(6.0, q1), q);
            Apply(q1 => Rx(7.0, q1), q);
            Apply(q1 => Rx(8.0, q1), q);
            Apply(q1 => Rx(9.0, q1), q);
            Apply(q1 => Rx(10.0, q1), q);
            Apply(q1 => Rx(11.0, q1), q);
        }
        "#,
        &expect![[r#"
            higher-order function `Apply<Empty>` generated 11 specializations, exceeding the warning threshold"#]],
    );
}

#[test]
fn below_threshold_no_excessive_specializations_warning() {
    // Ten call sites split across the AdjCtl and Empty functor variants should
    // not trigger the per-variant specialization warning.
    let source = r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            Apply(H, q);
            Apply(X, q);
            Apply(Y, q);
            Apply(Z, q);
            Apply(S, q);
            Apply(T, q);
            Apply(I, q);
            Apply(q1 => Rx(1.0, q1), q);
            Apply(q1 => Rx(2.0, q1), q);
            Apply(q1 => Rx(3.0, q1), q);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Apply_AdjCtl_(H, q);
                Apply_AdjCtl_(X, q);
                Apply_AdjCtl_(Y, q);
                Apply_AdjCtl_(Z, q);
                Apply_AdjCtl_(S, q);
                Apply_AdjCtl_(T, q);
                Apply_AdjCtl_(I, q);
                Apply_Empty_(/ * closure item = 3 captures = [] * / _lambda_3, q);
                Apply_Empty_(/ * closure item = 4 captures = [] * / _lambda_4, q);
                Apply_Empty_(/ * closure item = 5 captures = [] * / _lambda_5, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q1 : Qubit, ) : Unit {
                Rx(1., q1)
            }
            operation _lambda_4(q1 : Qubit, ) : Unit {
                Rx(2., q1)
            }
            operation _lambda_5(q1 : Qubit, ) : Unit {
                Rx(3., q1)
            }
            operation Apply_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                Apply_AdjCtl__H_(q);
                Apply_AdjCtl__X_(q);
                Apply_AdjCtl__Y_(q);
                Apply_AdjCtl__Z_(q);
                Apply_AdjCtl__S_(q);
                Apply_AdjCtl__T_(q);
                Apply_AdjCtl__I_(q);
                Apply_Empty__closure_(q);
                Apply_Empty__closure_(q);
                Apply_Empty__closure_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q1 : Qubit, ) : Unit {
                Rx(1., q1)
            }
            operation _lambda_4(q1 : Qubit, ) : Unit {
                Rx(2., q1)
            }
            operation _lambda_5(q1 : Qubit, ) : Unit {
                Rx(3., q1)
            }
            operation Apply_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            operation Apply_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation Apply_AdjCtl__Y_(q : Qubit) : Unit {
                Y(q);
            }
            operation Apply_AdjCtl__Z_(q : Qubit) : Unit {
                Z(q);
            }
            operation Apply_AdjCtl__S_(q : Qubit) : Unit {
                S(q);
            }
            operation Apply_AdjCtl__T_(q : Qubit) : Unit {
                T(q);
            }
            operation Apply_AdjCtl__I_(q : Qubit) : Unit {
                I(q);
            }
            operation Apply_Empty__closure_(q : Qubit) : Unit {
                _lambda_3(q, );
            }
            operation Apply_Empty__closure_(q : Qubit) : Unit {
                _lambda_4(q, );
            }
            operation Apply_Empty__closure_(q : Qubit) : Unit {
                _lambda_5(q, );
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn excessive_specializations_warning_does_not_block_compilation() {
    // A program that triggers ExcessiveSpecializations should still compile
    // successfully — the warning is non-fatal. We verify by running the
    // full defunctionalization and checking PostDefunc invariants hold.
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            Apply(q1 => Rx(1.0, q1), q);
            Apply(q1 => Rx(2.0, q1), q);
            Apply(q1 => Rx(3.0, q1), q);
            Apply(q1 => Rx(4.0, q1), q);
            Apply(q1 => Rx(5.0, q1), q);
            Apply(q1 => Rx(6.0, q1), q);
            Apply(q1 => Rx(7.0, q1), q);
            Apply(q1 => Rx(8.0, q1), q);
            Apply(q1 => Rx(9.0, q1), q);
            Apply(q1 => Rx(10.0, q1), q);
            Apply(q1 => Rx(11.0, q1), q);
        }
        "#,
    );
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners).diagnostics;

    // Should have exactly one warning, no fatal errors.
    let warnings: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, super::super::Error::ExcessiveSpecializations(..)))
        .collect();
    let fatal: Vec<_> = errors
        .iter()
        .filter(|e| !matches!(e, super::super::Error::ExcessiveSpecializations(..)))
        .collect();
    assert_eq!(warnings.len(), 1, "expected exactly one warning");
    assert!(fatal.is_empty(), "expected no fatal errors, got: {fatal:?}");

    // PostDefunc invariants must still hold.
    fir_invariants::check(&fir_store, fir_pkg_id, InvariantLevel::PostDefunc);
}

#[test]
fn cumulative_specialization_cap_fails_closed_with_fatal_error() {
    use std::fmt::Write as _;

    // A single HOF forwarded one more distinct global callable than the
    // cumulative cap allows must fail closed with a fatal
    // `RecursiveSpecialization` diagnostic rather than looping or panicking.
    // Each `OpN` is a distinct global, so each `Apply(OpN, q)` call site yields
    // a distinct specialization key for `Apply`; `cap + 1` of them pushes the
    // HOF's cumulative distinct-specialization count past the budget.
    let cap = crate::defunctionalize::specialize::CUMULATIVE_SPECIALIZATION_CAP;
    let num_ops = cap + 1;

    let mut ops = String::new();
    let mut calls = String::new();
    for i in 0..num_ops {
        writeln!(ops, "        operation Op{i}(q : Qubit) : Unit {{}}")
            .expect("writing to a String cannot fail");
        writeln!(calls, "            Apply(Op{i}, q);").expect("writing to a String cannot fail");
    }
    let source = format!(
        "\n        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {{ op(q); }}\n{ops}        operation Main() : Unit {{\n            use q = Qubit();\n{calls}        }}\n"
    );

    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(&source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    // The loop must terminate (fail closed); if the guard were absent this
    // could otherwise run to the iteration cap. Reaching this assertion at all
    // proves there was no panic or hang.
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners).diagnostics;

    let recursive: Vec<_> = errors
        .iter()
        .filter(|e| matches!(e, super::super::Error::RecursiveSpecialization(..)))
        .collect();
    assert_eq!(
        recursive.len(),
        1,
        "expected exactly one RecursiveSpecialization diagnostic, got: {errors:?}"
    );
    // The fatal diagnostic must not be classified as a warning.
    assert!(
        !recursive[0].is_warning(),
        "RecursiveSpecialization must be fatal, not a warning"
    );
    // The reported cumulative count must be over the cap.
    if let super::super::Error::RecursiveSpecialization(_, count, _) = recursive[0] {
        assert!(
            *count > cap,
            "reported count {count} should exceed the cap {cap}"
        );
    }
}

#[test]
fn primary_fix_regressions_stay_under_cumulative_cap() {
    // The key-matched self-call remap keeps each recursive HOF's specialization
    // set bounded, so neither the used-parameter variant nor the exact
    // unused-parameter repro should ever trip the cumulative cap. Both must
    // defunctionalize without emitting a `RecursiveSpecialization` diagnostic.
    let used_param = r#"
        operation Repeat(op : Qubit => Unit, n : Int, q : Qubit) : Unit {
            if n > 0 {
                op(q);
                Repeat(X, n - 1, q);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            Repeat(H, 2, q);
        }
        "#;
    let unused_param = r#"
        operation Main() : Unit {
            F(x => x)
        }
        operation F(f : () => ()) : Unit {
            F(() => F(x => x));
        }
        "#;

    for source in [used_param, unused_param] {
        let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(source);
        let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
        let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners).diagnostics;
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e, super::super::Error::RecursiveSpecialization(..))),
            "expected no RecursiveSpecialization under the cap, got: {errors:?}"
        );
    }
}

#[test]
fn zero_capture_conditional_alias_dispatches_correctly() {
    let source = r#"
        operation ZeroCaptureConditionalAlias(q : Qubit, useAdj : Bool) : Unit {
            let u = if useAdj { Adjoint S } else { S };
            u(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ZeroCaptureConditionalAlias(q, true);
        }
        "#;
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ZeroCaptureConditionalAlias(q : Qubit, useAdj : Bool) : Unit {
                let u : (Qubit => Unit is Adj + Ctl) = if useAdj {
                    Adjoint S
                } else {
                    S
                };
                u(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ZeroCaptureConditionalAlias(q, true);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation ZeroCaptureConditionalAlias(q : Qubit, useAdj : Bool) : Unit {
                if useAdj {
                    Adjoint S(q)
                } else {
                    S(q)
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ZeroCaptureConditionalAlias(q, true);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
    let targets = callable_call_targets_after_defunc(source, "ZeroCaptureConditionalAlias");
    assert!(
        targets.contains(&"Adjoint S".to_string()) && targets.contains(&"S".to_string()),
        "conditional alias should preserve both S and Adjoint S dispatch targets, got {targets:?}"
    );
}

/// When an identity closure `q => H(q)` is eta-reduced and its direct call is
/// rewritten, the surviving direct `Call` expr must carry the original lambda
/// body span (`H(q)`), not the discarded `f(q)` call-site span, so circuit
/// instructions point at the lambda body, matching un-optimized evaluation.
#[test]
fn direct_call_preserves_lambda_body_span() {
    let source = "
        operation Main() : Unit {
            use q = Qubit();
            let f = q => H(q);
            f(q);
        }
    ";
    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let package = fir_store.get(fir_pkg_id);
    let decl = callable_decl(package, "Main");

    let mut call_span = None;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &decl.implementation,
        &mut |_expr_id, expr| {
            if let fir::ExprKind::Call(callee_id, _) = &expr.kind
                && call_target_name(&fir_store, package, *callee_id).as_deref() == Some("H")
            {
                call_span = Some(expr.span);
            }
        },
    );

    let span = call_span.expect("expected a surviving direct call to H in Main");
    let slice = &source[span.lo as usize..span.hi as usize];
    assert_eq!(
        slice, "H(q)",
        "surviving direct call should carry the lambda body span, got {slice:?}"
    );
}

/// `MakeRotation` returns a partial application capturing its `base` parameter.
/// `Main` binds it to `rotation` and passes it to the higher-order operation
/// `Apply`. The rewrite must substitute the caller's `amount` for the
/// producer's `base`, rather than thread a local from the wrong callable.
#[test]
fn cross_function_closure_capture_threads_correct_value() {
    let source = r#"
        import Std.Convert.*;
        operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
            f(q);
        }

        operation ApplyRotation(base : Int, q : Qubit) : Unit {
            Rx(IntAsDouble(base), q);
        }

        function MakeRotation(base : Int) : (Qubit => Unit) {
            return ApplyRotation(base, _);
        }

        operation Main() : Unit {
            use q = Qubit();
            let amount = 5;
            let rotation = MakeRotation(amount);
            Apply(rotation, q);
        }
        "#;
    check_rewrite_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            BEFORE:
            operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation ApplyRotation(base : Int, q : Qubit) : Unit {
                Rx(IntAsDouble(base), q);
            }
            function MakeRotation(base : Int) : (Qubit => Unit) {
                return {
                    let arg : Int = base;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let amount : Int = 5;
                let rotation : (Qubit => Unit) = MakeRotation(amount);
                Apply_Empty_(rotation, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_5(arg : Int, hole : Qubit) : Unit {
                ApplyRotation(arg, hole)
            }
            operation Apply_Empty_(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation ApplyRotation(base : Int, q : Qubit) : Unit {
                Rx(IntAsDouble(base), q);
            }
            function MakeRotation(base : Int) : (Qubit => Unit) {
                return {
                    let arg : Int = base;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let amount : Int = 5;
                {
                    let __capture : Int = amount;
                    Apply_Empty__closure_(q, __capture)
                };
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_5(arg : Int, hole : Qubit) : Unit {
                ApplyRotation(arg, hole)
            }
            operation Apply_Empty_(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation Apply_Empty__closure_(q : Qubit, __capture_0 : Int) : Unit {
                _lambda_5(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

/// The closure is created inline in the same block as the HOF call.
#[test]
fn inline_closure_capture_threads_correct_value() {
    let source = r#"
        import Std.Convert.*;
        operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
            f(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let amount = 5;
            Apply(qubit => Rx(IntAsDouble(amount), qubit), q);
        }
        "#;
    check_rewrite_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            BEFORE:
            operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let amount : Int = 5;
                Apply_Empty_(/ * closure item = 3 captures = [amount] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(amount : Int, qubit : Qubit) : Unit {
                Rx(IntAsDouble(amount), qubit)
            }
            operation Apply_Empty_(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let amount : Int = 5;
                Apply_Empty__closure_(q, amount);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(amount : Int, qubit : Qubit) : Unit {
                Rx(IntAsDouble(amount), qubit)
            }
            operation Apply_Empty_(f : (Qubit => Unit), q : Qubit) : Unit {
                f(q);
            }
            operation Apply_Empty__closure_(q : Qubit, __capture_0 : Int) : Unit {
                _lambda_3(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

/// A struct-capturing closure invoked through a `Controlled` dispatch must
/// thread its capture all the way to the controlled call. Here the closure
/// captures a `StatePreparationParams` struct (a partial application of
/// `ApplyStatePreparation`) and is forwarded into an inner closure that issues
/// `Controlled prepareOp([control], systems)` under a loop.
///
/// The snapshot retargets that controlled call to the lifted partial-application
/// wrapper while threading the captured struct, i.e.
/// `Controlled _lambda_8([control], (__capture_0, systems))`. This
/// guards against a silent re-drop where the control layer wraps the base input
/// and the capture is lost, leaving `Controlled _lambda_8([control], systems)`.
#[test]
fn struct_capture_closure_threads_capture_through_controlled_dispatch() {
    let source = r#"
        struct StatePreparationParams {
            rowMap : Int[],
            stateVector : Double[],
            expansionOps : Int[][],
            numQubits : Int
        }

        operation ApplyStatePreparation(params : StatePreparationParams, qs : Qubit[]) : Unit is Adj + Ctl {
            if Length(params.expansionOps) != 0 {
                X(qs[0]);
            }
        }

        operation SelectIdentity(systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {}

        function MakeControlledPrepSelPrepOp(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : (Qubit, Qubit[]) => Unit {
            (control, allQubits) => {
                let systems = allQubits[0..numSystemQubits - 1];
                let ancilla = allQubits[numSystemQubits...];
                for _ in 0..power - 1 {
                    Controlled prepareOp([control], systems);
                    Controlled selectOp([control], (systems, ancilla));
                }
            }
        }

        operation MakeControlledPrepSelPrepCircuit(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : Unit {
            use control = Qubit();
            use systems = Qubit[numSystemQubits + 1];
            let op = MakeControlledPrepSelPrepOp(prepareOp, selectOp, numSystemQubits, power);
            op(control, systems);
        }

        operation Main() : Unit {
            let params = new StatePreparationParams {
                rowMap = [0],
                stateVector = [1.0, 0.0],
                expansionOps = [],
                numQubits = 1
            };
            let prep = ApplyStatePreparation(params, _);
            MakeControlledPrepSelPrepCircuit(prep, SelectIdentity, 1, 1);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype StatePreparationParams = (Int[], Double[], Int[][], Int);
            operation ApplyStatePreparation(params : __UDT_Item_1__Package_2_, qs : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    if Length(params::expansionOps) != 0 {
                        X(qs[0]);
                    }

                }
                adjoint ... {
                    if Length(params::expansionOps) != 0 {
                        Adjoint X(qs[0]);
                    }

                }
                controlled (ctls, ...) {
                    if Length(params::expansionOps) != 0 {
                        Controlled X(ctls, qs[0]);
                    }

                }
                controlled adjoint (ctls, ...) {
                    if Length(params::expansionOps) != 0 {
                        Controlled Adjoint X(ctls, qs[0]);
                    }

                }
            }
            operation SelectIdentity(systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
                body ... {}
                adjoint ... {}
                controlled (ctls, ...) {}
                controlled adjoint (ctls, ...) {}
            }
            function MakeControlledPrepSelPrepOp(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 7 captures = [prepareOp, selectOp, numSystemQubits, power] * / _lambda_7
            }
            operation MakeControlledPrepSelPrepCircuit(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                let op : ((Qubit, Qubit[]) => Unit) = MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp, selectOp, numSystemQubits, power);
                op(control, systems);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            operation Main() : Unit {
                let params : __UDT_Item_1__Package_2_ = new StatePreparationParams {
                    rowMap = [0],
                    stateVector = [1., 0.],
                    expansionOps = [],
                    numQubits = 1
                };
                let prep : (Qubit[] => Unit is Adj + Ctl) = {
                    let arg : __UDT_Item_1__Package_2_ = params;
                    / * closure item = 8 captures = [arg] * / _lambda_8
                };
                MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl_(prep, SelectIdentity, 1, 1);
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_341 : Range = 0..power - 1;
                        mutable _index_id_344 : Int = _range_id_341.Start;
                        let _step_id_349 : Int = _range_id_341.Step;
                        let _end_id_354 : Int = _range_id_341.End;
                        while ((_step_id_349 > 0) and (_index_id_344 <= _end_id_354)) or ((_step_id_349 < 0) and (_index_id_344 >= _end_id_354)) {
                            let _ : Int = _index_id_344;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_344 += _step_id_349;
                        }

                    }

                }

            }
            operation _lambda_8(arg : __UDT_Item_1__Package_2_, hole : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    ApplyStatePreparation(arg, hole)
                }
                adjoint ... {
                    Adjoint ApplyStatePreparation(arg, hole)
                }
                controlled (ctls, ...) {
                    Controlled ApplyStatePreparation(ctls, (arg, hole))
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplyStatePreparation(ctls, (arg, hole))
                }
            }
            function MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 10 captures = [prepareOp, selectOp, numSystemQubits, power] * / _lambda_7
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_341 : Range = 0..power - 1;
                        mutable _index_id_344 : Int = _range_id_341.Start;
                        let _step_id_349 : Int = _range_id_341.Step;
                        let _end_id_354 : Int = _range_id_341.End;
                        while ((_step_id_349 > 0) and (_index_id_344 <= _end_id_354)) or ((_step_id_349 < 0) and (_index_id_344 >= _end_id_354)) {
                            let _ : Int = _index_id_344;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_344 += _step_id_349;
                        }

                    }

                }

            }
            operation MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                let op : ((Qubit, Qubit[]) => Unit) = MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp, selectOp, numSystemQubits, power);
                op(control, systems);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            // entry
            Main()

            AFTER:
            newtype StatePreparationParams = (Int[], Double[], Int[][], Int);
            operation ApplyStatePreparation(params : __UDT_Item_1__Package_2_, qs : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    if Length(params::expansionOps) != 0 {
                        X(qs[0]);
                    }

                }
                adjoint ... {
                    if Length(params::expansionOps) != 0 {
                        Adjoint X(qs[0]);
                    }

                }
                controlled (ctls, ...) {
                    if Length(params::expansionOps) != 0 {
                        Controlled X(ctls, qs[0]);
                    }

                }
                controlled adjoint (ctls, ...) {
                    if Length(params::expansionOps) != 0 {
                        Controlled Adjoint X(ctls, qs[0]);
                    }

                }
            }
            operation SelectIdentity(systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
                body ... {}
                adjoint ... {}
                controlled (ctls, ...) {}
                controlled adjoint (ctls, ...) {}
            }
            function MakeControlledPrepSelPrepOp(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 7 captures = [prepareOp, selectOp, numSystemQubits, power] * / _lambda_7
            }
            operation MakeControlledPrepSelPrepCircuit(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                let op : ((Qubit, Qubit[]) => Unit) = MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp, selectOp, numSystemQubits, power);
                op(control, systems);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            operation Main() : Unit {
                let params : __UDT_Item_1__Package_2_ = new StatePreparationParams {
                    rowMap = [0],
                    stateVector = [1., 0.],
                    expansionOps = [],
                    numQubits = 1
                };
                {
                    let __capture : __UDT_Item_1__Package_2_ = params;
                    MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl__closure__SelectIdentity_(1, 1, __capture)
                };
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_341 : Range = 0..power - 1;
                        mutable _index_id_344 : Int = _range_id_341.Start;
                        let _step_id_349 : Int = _range_id_341.Step;
                        let _end_id_354 : Int = _range_id_341.End;
                        while ((_step_id_349 > 0) and (_index_id_344 <= _end_id_354)) or ((_step_id_349 < 0) and (_index_id_344 >= _end_id_354)) {
                            let _ : Int = _index_id_344;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_344 += _step_id_349;
                        }

                    }

                }

            }
            operation _lambda_8(arg : __UDT_Item_1__Package_2_, hole : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    ApplyStatePreparation(arg, hole)
                }
                adjoint ... {
                    Adjoint ApplyStatePreparation(arg, hole)
                }
                controlled (ctls, ...) {
                    Controlled ApplyStatePreparation(ctls, (arg, hole))
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplyStatePreparation(ctls, (arg, hole))
                }
            }
            function MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 10 captures = [prepareOp, selectOp, numSystemQubits, power] * / _lambda_7
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_341 : Range = 0..power - 1;
                        mutable _index_id_344 : Int = _range_id_341.Start;
                        let _step_id_349 : Int = _range_id_341.Step;
                        let _end_id_354 : Int = _range_id_341.End;
                        while ((_step_id_349 > 0) and (_index_id_344 <= _end_id_354)) or ((_step_id_349 < 0) and (_index_id_344 >= _end_id_354)) {
                            let _ : Int = _index_id_344;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_344 += _step_id_349;
                        }

                    }

                }

            }
            operation MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                _lambda_7(prepareOp, selectOp, numSystemQubits, power, (control, systems));
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            operation MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl__closure__SelectIdentity_(numSystemQubits : Int, power : Int, __capture_0 : __UDT_Item_1__Package_2_) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                _lambda_7_closure__SelectIdentity_(numSystemQubits, power, (control, systems), __capture_0);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            function MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl__closure__SelectIdentity_(numSystemQubits : Int, power : Int, __capture_0 : __UDT_Item_1__Package_2_) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 14 captures = [__capture_0, numSystemQubits, power] * / _lambda_7
            }
            operation _lambda_7(__capture_0 : __UDT_Item_1__Package_2_, numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_341 : Range = 0..power - 1;
                        mutable _index_id_344 : Int = _range_id_341.Start;
                        let _step_id_349 : Int = _range_id_341.Step;
                        let _end_id_354 : Int = _range_id_341.End;
                        while ((_step_id_349 > 0) and (_index_id_344 <= _end_id_354)) or ((_step_id_349 < 0) and (_index_id_344 >= _end_id_354)) {
                            let _ : Int = _index_id_344;
                            Controlled _lambda_8([control], (__capture_0, systems));
                            Controlled SelectIdentity([control], (systems, ancilla));
                            _index_id_344 += _step_id_349;
                        }

                    }

                }

            }
            operation _lambda_7_closure__SelectIdentity_(numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[]), __capture_0 : __UDT_Item_1__Package_2_) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_341 : Range = 0..power - 1;
                        mutable _index_id_344 : Int = _range_id_341.Start;
                        let _step_id_349 : Int = _range_id_341.Step;
                        let _end_id_354 : Int = _range_id_341.End;
                        while ((_step_id_349 > 0) and (_index_id_344 <= _end_id_354)) or ((_step_id_349 < 0) and (_index_id_344 >= _end_id_354)) {
                            let _ : Int = _index_id_344;
                            Controlled _lambda_8([control], (__capture_0, systems));
                            Controlled SelectIdentity([control], (systems, ancilla));
                            _index_id_344 += _step_id_349;
                        }

                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

/// A closure that captures a struct built from a factory function's own
/// parameters must be rebuilt from caller-scope values when it is specialized
/// into a different callable.
///
/// `Main` calls the factory `MakeStatePreparationOp`, which builds a
/// `StatePreparationParams` struct from its parameters and returns a
/// partial-application closure capturing that struct. The closure is forwarded
/// through the `MakeControlledPrepSelPrepCircuit` wrapper.
///
/// Because the captured struct references the factory's parameters, it cannot
/// be copied as-is into `Main`, which does not bind those parameters.
/// Specialization must rebind each struct field to the argument the factory was
/// called with — `[0]`, `[1.0, 0.0]`, `[]`, and `1` — so the struct is rooted
/// entirely in caller-scope values.
///
/// The test expects no errors and passing `PostDefunc` invariants.
#[test]
fn producer_scope_struct_capture_reconstructed_in_caller() {
    let source = r#"
        struct StatePreparationParams {
            rowMap : Int[],
            stateVector : Double[],
            expansionOps : Int[][],
            numQubits : Int
        }

        operation ApplyStatePreparation(params : StatePreparationParams, qs : Qubit[]) : Unit is Adj + Ctl {
            if Length(params.expansionOps) != 0 {
                X(qs[0]);
            }
        }

        operation SelectIdentity(systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {}

        function MakeStatePreparationOp(
            rowMap : Int[],
            stateVector : Double[],
            expansionOps : Int[][],
            numQubits : Int
        ) : Qubit[] => Unit is Adj + Ctl {
            let params = new StatePreparationParams {
                rowMap = rowMap,
                stateVector = stateVector,
                expansionOps = expansionOps,
                numQubits = numQubits
            };
            ApplyStatePreparation(params, _)
        }

        function MakeControlledPrepSelPrepOp(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : (Qubit, Qubit[]) => Unit {
            (control, allQubits) => {
                let systems = allQubits[0..numSystemQubits - 1];
                let ancilla = allQubits[numSystemQubits...];
                for _ in 0..power - 1 {
                    Controlled prepareOp([control], systems);
                    Controlled selectOp([control], (systems, ancilla));
                }
            }
        }

        operation MakeControlledPrepSelPrepCircuit(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : Unit {
            use control = Qubit();
            use systems = Qubit[numSystemQubits + 1];
            let op = MakeControlledPrepSelPrepOp(prepareOp, selectOp, numSystemQubits, power);
            op(control, systems);
        }

        operation Main() : Unit {
            let prep = MakeStatePreparationOp([0], [1.0, 0.0], [], 1);
            MakeControlledPrepSelPrepCircuit(prep, SelectIdentity, 1, 1);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    // The invariant check is the point of this test: the captured struct must be
    // rebuilt from caller-scope values, or the `PostDefunc` local-variable
    // consistency check fails.
    check_invariants(source);
}

/// A captured struct whose field is a computed value referencing the factory's
/// parameters through pure function calls and operators must still specialize.
///
/// Here `numQubits` is `Length(stateVector) + Length(rowMap)`. Rebuilding the
/// captured struct in `Main` requires rebinding the parameter references inside
/// the computed field to the caller-scope arguments `[1.0, 0.0]` and `[0]`.
/// Because `Length` is a pure function and `+` has no side effects, the field
/// can be safely reconstructed from caller values.
///
/// The test expects genuine specialization: no errors and passing `PostDefunc`
/// invariants. A decline to a dynamic call would instead emit a
/// `DynamicCallable` error and fail the assertion.
#[test]
fn producer_scope_struct_capture_computed_field_specializes() {
    let source = r#"
        struct StatePreparationParams {
            rowMap : Int[],
            stateVector : Double[],
            expansionOps : Int[][],
            numQubits : Int
        }

        operation ApplyStatePreparation(params : StatePreparationParams, qs : Qubit[]) : Unit is Adj + Ctl {
            if params.numQubits != 0 {
                X(qs[0]);
            }
        }

        operation SelectIdentity(systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {}

        function MakeStatePreparationOp(
            rowMap : Int[],
            stateVector : Double[],
            expansionOps : Int[][]
        ) : Qubit[] => Unit is Adj + Ctl {
            let params = new StatePreparationParams {
                rowMap = rowMap,
                stateVector = stateVector,
                expansionOps = expansionOps,
                numQubits = Length(stateVector) + Length(rowMap)
            };
            ApplyStatePreparation(params, _)
        }

        function MakeControlledPrepSelPrepOp(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : (Qubit, Qubit[]) => Unit {
            (control, allQubits) => {
                let systems = allQubits[0..numSystemQubits - 1];
                let ancilla = allQubits[numSystemQubits...];
                for _ in 0..power - 1 {
                    Controlled prepareOp([control], systems);
                    Controlled selectOp([control], (systems, ancilla));
                }
            }
        }

        operation MakeControlledPrepSelPrepCircuit(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : Unit {
            use control = Qubit();
            use systems = Qubit[numSystemQubits + 1];
            let op = MakeControlledPrepSelPrepOp(prepareOp, selectOp, numSystemQubits, power);
            op(control, systems);
        }

        operation Main() : Unit {
            let prep = MakeStatePreparationOp([0], [1.0, 0.0], []);
            MakeControlledPrepSelPrepCircuit(prep, SelectIdentity, 1, 1);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    // Passing `check_invariants` proves genuine specialization. It runs
    // defunctionalization, asserts there are no defunctionalization errors, and
    // checks `PostDefunc` consistency; a decline to dynamic would emit a
    // `DynamicCallable` error and fail the assertion.
    check_invariants(source);
}

/// A captured struct whose field is computed by an operation call must not be
/// rebuilt in the caller; specialization declines to a dynamic call instead.
///
/// Here `numQubits` is `CountQubits(rowMap)`, where `CountQubits` is an
/// operation. Relocating an operation call into caller-scope argument
/// construction could change observable ordering or duplicate the call, so
/// specialization refuses to reconstruct the field. Instead it declines the
/// closure to a dynamic call site and emits the recoverable `DynamicCallable`
/// diagnostic.
///
/// This confirms the purity check does not over-decline: the pure-function
/// computed field above still specializes, while this operation-valued field
/// declines cleanly rather than panicking.
#[test]
fn producer_scope_struct_capture_operation_field_declines_to_dynamic() {
    let source = r#"
        struct StatePreparationParams {
            rowMap : Int[],
            numQubits : Int
        }

        operation CountQubits(arr : Int[]) : Int {
            return Length(arr);
        }

        operation ApplyStatePreparation(params : StatePreparationParams, qs : Qubit[]) : Unit is Adj + Ctl {
            if params.numQubits != 0 {
                X(qs[0]);
            }
        }

        operation SelectIdentity(systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {}

        operation MakeStatePreparationOp(rowMap : Int[]) : Qubit[] => Unit is Adj + Ctl {
            let params = new StatePreparationParams {
                rowMap = rowMap,
                numQubits = CountQubits(rowMap)
            };
            ApplyStatePreparation(params, _)
        }

        function MakeControlledPrepSelPrepOp(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : (Qubit, Qubit[]) => Unit {
            (control, allQubits) => {
                let systems = allQubits[0..numSystemQubits - 1];
                let ancilla = allQubits[numSystemQubits...];
                for _ in 0..power - 1 {
                    Controlled prepareOp([control], systems);
                    Controlled selectOp([control], (systems, ancilla));
                }
            }
        }

        operation MakeControlledPrepSelPrepCircuit(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : Unit {
            use control = Qubit();
            use systems = Qubit[numSystemQubits + 1];
            let op = MakeControlledPrepSelPrepOp(prepareOp, selectOp, numSystemQubits, power);
            op(control, systems);
        }

        operation Main() : Unit {
            let prep = MakeStatePreparationOp([0]);
            MakeControlledPrepSelPrepCircuit(prep, SelectIdentity, 1, 1);
        }
        "#;
    // The operation-call computed field cannot be safely rebuilt in the caller,
    // so the closure declines to a dynamic call site and emits the recoverable
    // `DynamicCallable` diagnostic rather than panicking.
    check_errors(
        source,
        &expect![[r#"
            callable argument could not be resolved statically
            callable argument could not be resolved statically"#]],
    );
}

/// A mixed branch-split call can combine a dispatched callable field with a
/// single-valued inline capturing closure while leaving another field of the same tuple
/// parameter live.
///
/// The combined specialization must remove only the callable fields from
/// `pair`; dropping the whole tuple parameter deletes the destructuring that
/// binds `target` and leaves the inlined calls using an unbound local. The
/// snapshot pins the surviving `target` binding in both dispatch leaves.
#[test]
fn mixed_branch_split_partial_tuple_field_coverage_preserves_surviving_field() {
    check_rewrite(
        r#"
        operation Run(pair : (Qubit => Unit, Qubit => Unit, Qubit)) : Unit {
            let (first, second, target) = pair;
            first(target);
            second(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let choose = M(q) == One;
            let first = if choose { H } else { X };
            let angle = 0.25;
            Run((first, target => Rz(angle, target), q));
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Run(pair : ((Qubit => Unit), (Qubit => Unit), Qubit)) : Unit {
                let (first : (Qubit => Unit), second : (Qubit => Unit), target : Qubit) = pair;
                first(target);
                second(target);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let choose : Bool = M(q) == One;
                let first : (Qubit => Unit is Adj + Ctl) = if choose {
                    H
                } else {
                    X
                };
                let angle : Double = 0.25;
                Run_AdjCtl__Empty_(first, / * closure item = 3 captures = [angle] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle : Double, target : Qubit) : Unit {
                Rz(angle, target)
            }
            operation Run_AdjCtl__Empty_(pair : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit), Qubit)) : Unit {
                let (first : (Qubit => Unit is Adj + Ctl), second : (Qubit => Unit), target : Qubit) = pair;
                first(target);
                second(target);
            }
            // entry
            Main()

            AFTER:
            operation Run(pair : ((Qubit => Unit), (Qubit => Unit), Qubit)) : Unit {
                let (first : (Qubit => Unit), second : (Qubit => Unit), target : Qubit) = pair;
                first(target);
                second(target);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let choose : Bool = M(q) == One;
                let angle : Double = 0.25;
                if choose {
                    Run_AdjCtl__Empty__H__closure_(q, angle)
                } else {
                    Run_AdjCtl__Empty__X__closure_(q, angle)
                };
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle : Double, target : Qubit) : Unit {
                Rz(angle, target)
            }
            operation Run_AdjCtl__Empty_(pair : ((Qubit => Unit is Adj + Ctl), (Qubit => Unit), Qubit)) : Unit {
                let (first : (Qubit => Unit is Adj + Ctl), second : (Qubit => Unit), target : Qubit) = pair;
                first(target);
                second(target);
            }
            operation Run_AdjCtl__Empty__H__closure_(pair : Qubit, __capture_0 : Double) : Unit {
                let target : Qubit = pair;
                H(target);
                _lambda_3(__capture_0, target);
            }
            operation Run_AdjCtl__Empty__X__closure_(pair : Qubit, __capture_0 : Double) : Unit {
                let target : Qubit = pair;
                X(target);
                _lambda_3(__capture_0, target);
            }
            // entry
            Main()
        "#]],
    );
}

/// A mixed branch-split group with a `Dynamic` sibling is intentionally not a
/// combined per-candidate specialization.
///
/// The unresolved `third` argument must surface as `DynamicCallable`. The
/// capturing closure sibling may still get a transient per-row spec while that
/// diagnostic is collected, but tracking it as consumed could clear a live closure or
/// trip the internal consistency panic. This test uses more than `MULTI_CAP`
/// array elements to force the sibling to `Dynamic` and asserts the pass returns
/// diagnostics instead of panicking.
#[test]
fn mixed_branch_split_dynamic_sibling_reports_error_without_panic() {
    use std::fmt::Write as _;

    const ELEMENTS: usize = 1001;

    let mut defs = String::new();
    let mut elems = String::new();
    for i in 0..ELEMENTS {
        writeln!(defs, "        operation Op{i}(q : Qubit) : Unit {{}}").expect("write succeeds");
        if i > 0 {
            elems.push_str(", ");
        }
        write!(elems, "Op{i}").expect("write succeeds");
    }

    let source = format!(
        r#"
{defs}
        operation Run(
            first : Qubit => Unit,
            second : Qubit => Unit,
            third : Qubit => Unit,
            q : Qubit
        ) : Unit {{
            first(q);
            second(q);
            third(q);
        }}
        operation Main() : Unit {{
            use q = Qubit();
            let choose = M(q) == One;
            let first = if choose {{ Op0 }} else {{ Op1 }};
            let angle = 0.25;
            let ops = [{elems}];
            for third in ops {{
                Run(first, target => Rz(angle, target), third, q);
            }}
        }}
        "#
    );

    check_errors(
        &source,
        &expect![[r#"
            callable argument could not be resolved statically
            callable argument could not be resolved statically"#]],
    );
}

#[test]
fn single_element_callable_array_into_struct_field_survives_as_array() {
    // A single-element callable array threaded into a callee that indexes it
    // (`arr[0]`) and stores the result in a struct field must survive
    // specialization as a one-element array literal. Collapsing the forwarded
    // array to the scalar callable would leave `arr[0]` indexing a non-array
    // value, so the specialized body keeps `[AddOne][0]`.
    let source = r#"
        struct Holder { Cb : (Int => Int) }
        operation Pick(arr : (Int => Int)[]) : Holder {
            let f = arr[0];
            new Holder { Cb = f }
        }
        operation Main() : Int {
            let ops : (Int => Int)[] = [AddOne];
            let h = Pick(ops);
            h.Cb(3)
        }
        operation AddOne(x : Int) : Int { x + 1 }
        "#;
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(4),
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Holder = ((Int => Int), );
            operation Pick(arr : (Int => Int)[]) : __UDT_Item_1__Package_2_ {
                let f : (Int => Int) = arr[0];
                new Holder {
                    Cb = f
                }

            }
            operation Main() : Int {
                let ops : (Int => Int)[] = [AddOne];
                let h : __UDT_Item_1__Package_2_ = Pick_Empty_(ops);
                h::Cb(3)
            }
            operation AddOne(x : Int) : Int {
                x + 1
            }
            operation Pick_Empty_(arr : (Int => Int)[]) : __UDT_Item_1__Package_2_ {
                let f : (Int => Int) = arr[0];
                new Holder {
                    Cb = f
                }

            }
            // entry
            Main()

            AFTER:
            newtype Holder = ((Int => Int), );
            operation Pick(arr : (Int => Int)[]) : __UDT_Item_1__Package_2_ {
                let f : (Int => Int) = arr[0];
                new Holder {
                    Cb = f
                }

            }
            operation Main() : Int {
                let ops : (Int => Int)[] = [AddOne];
                let h : __UDT_Item_1__Package_2_ = Pick_Empty__AddOne_();
                AddOne(3)
            }
            operation AddOne(x : Int) : Int {
                x + 1
            }
            operation Pick_Empty_(arr : (Int => Int)[]) : __UDT_Item_1__Package_2_ {
                let f : (Int => Int) = arr[0];
                new Holder {
                    Cb = f
                }

            }
            operation Pick_Empty__AddOne_() : __UDT_Item_1__Package_2_ {
                let f : (Int => Int) = [AddOne][0];
                new Holder {
                    Cb = f
                }

            }
            // entry
            Main()
        "#]],
    );
}

/// A single-element tuple parameter `(Qubit => Unit,)` whose only field is a
/// capturing producer closure routes through the per-row singular path. Removing
/// the consumed field empties the parameter's tuple, so the specialized input
/// drops the emptied slot and keeps only the threaded capture. The rebuilt call
/// argument must likewise supply only the capture and never prepend the emptied
/// slot, so the specialized input pattern and the call argument stay arity
/// matched.
#[test]
fn single_element_producer_tuple_param_drops_slot_and_threads_capture() {
    check_rewrite(
        r#"
        operation Rotate(angle : Double, q : Qubit) : Unit { Rx(angle, q); }
        function Make(angle : Double) : (Qubit => Unit) { return Rotate(angle, _); }
        operation ApplyTup(ops : (Qubit => Unit,)) : Unit {
            use q = Qubit();
            let (a,) = ops;
            a(q);
            a(q);
        }
        @EntryPoint()
        operation Main() : Unit {
            ApplyTup((Make(0.5),));
        }
        "#,
        &expect![[r#"
            BEFORE:
            operation Rotate(angle : Double, q : Qubit) : Unit {
                Rx(angle, q);
            }
            function Make(angle : Double) : (Qubit => Unit) {
                return {
                    let arg : Double = angle;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation ApplyTup(ops : ((Qubit => Unit), )) : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let (a : (Qubit => Unit), ) = ops;
                a(q);
                a(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                ApplyTup_Empty_(Make(0.5), );
            }
            operation _lambda_5(arg : Double, hole : Qubit) : Unit {
                Rotate(arg, hole)
            }
            operation ApplyTup_Empty_(ops : ((Qubit => Unit), )) : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let (a : (Qubit => Unit), ) = ops;
                a(q);
                a(q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Rotate(angle : Double, q : Qubit) : Unit {
                Rx(angle, q);
            }
            function Make(angle : Double) : (Qubit => Unit) {
                return {
                    let arg : Double = angle;
                    / * closure item = 5 captures = [arg] * / _lambda_5
                };
            }
            operation ApplyTup(ops : ((Qubit => Unit), )) : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let (a : (Qubit => Unit), ) = ops;
                a(q);
                a(q);
                __quantum__rt__qubit_release(q);
            }
            operation Main() : Unit {
                ApplyTup_Empty__closure_(0.5, );
            }
            operation _lambda_5(arg : Double, hole : Qubit) : Unit {
                Rotate(arg, hole)
            }
            operation ApplyTup_Empty_(ops : ((Qubit => Unit), )) : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let (a : (Qubit => Unit), ) = ops;
                a(q);
                a(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyTup_Empty__closure_(__capture_0 : Double, ) : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                _lambda_5(__capture_0, q);
                _lambda_5(__capture_0, q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// A `for` loop over a callable array desugars to a `Length(array)` call and an
/// indexed read of the array. `Length` is an intrinsic that consumes its
/// argument as data and never invokes it, so it must not be treated as a
/// higher-order function: the array argument has to survive so `Length` and the
/// indexed read stay well-formed. Here the loop element `op` (candidates
/// `[H, X]`) is dispatched into the two-parameter `ApplyTwo` alongside a
/// single-valued global sibling `Y` in a different slot. The rewrite keeps both
/// dispatch candidates, threads the sibling into each specialized leaf, and
/// leaves the `Length(_array_id)` call unspecialized.
#[test]
fn callable_array_loop_dispatch_with_global_sibling_preserves_length_call() {
    let source = r#"
        operation ApplyTwo(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit {
            f(q);
            g(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let ops = [H, X];
            for op in ops {
                ApplyTwo(op, Y, q);
            }
        }
        "#;
    check_invariants(source);
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
                let ops : (Qubit => Unit is Adj + Ctl)[] = [H, X];
                let _generated_ident_84 : Unit = {
                    let _array_id_51 : (Qubit => Unit is Adj + Ctl)[] = ops;
                    let _len_id_55 : Int = Length(_array_id_51);
                    mutable _index_id_60 : Int = 0;
                    while _index_id_60 < _len_id_55 {
                        let op : (Qubit => Unit is Adj + Ctl) = _array_id_51[_index_id_60];
                        ApplyTwo_AdjCtl__AdjCtl_(op, Y, q);
                        _index_id_60 += 1;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_84
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
                let ops : (Qubit => Unit is Adj + Ctl)[] = [H, X];
                let _generated_ident_84 : Unit = {
                    let _array_id_51 : (Qubit => Unit is Adj + Ctl)[] = ops;
                    let _len_id_55 : Int = Length(_array_id_51);
                    mutable _index_id_60 : Int = 0;
                    while _index_id_60 < _len_id_55 {
                        let op : (Qubit => Unit is Adj + Ctl) = _array_id_51[_index_id_60];
                        {
                            [(), ()][_index_id_60];
                            if (_index_id_60 == 0) or (_index_id_60 == -2) {
                                ApplyTwo_AdjCtl__AdjCtl__H__Y_(q)
                            } else {
                                ApplyTwo_AdjCtl__AdjCtl__X__Y_(q)
                            }
                        };
                        _index_id_60 += 1;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_84
            }
            operation ApplyTwo_AdjCtl__AdjCtl_(f : (Qubit => Unit is Adj + Ctl), g : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                f(q);
                g(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl__H_(g : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                H(q);
                g(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl__X_(g : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                X(q);
                g(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl__Y_(f : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                f(q);
                Y(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl__H__Y_(q : Qubit) : Unit {
                H(q);
                Y(q);
            }
            operation ApplyTwo_AdjCtl__AdjCtl__X__Y_(q : Qubit) : Unit {
                X(q);
                Y(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// Specializing a HOF that indexes directly into a callable-array parameter
/// must not duplicate a side-effecting index expression across synthesized
/// branch guards. The index operation applies `X(q)`, so running it once versus
/// once per dispatch arm is an observable semantic difference.
#[test]
fn indexed_callable_array_param_hoists_side_effecting_index_once() {
    let source = r#"
        operation ChooseIndex(q : Qubit) : Int {
            X(q);
            1
        }
        operation RunAt(ops : (Qubit => Unit)[], q : Qubit) : Unit {
            ops[ChooseIndex(q)](q);
        }
        operation Main() : Unit {
            use q = Qubit();
            RunAt([I, X, Y], q);
        }
        "#;

    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners).diagnostics;
    assert_no_defunctionalization_errors("defunctionalization", &errors);

    let after = crate::pretty::write_package_qsharp_parseable(&fir_store, fir_pkg_id);
    assert!(
        after.contains("let index : Int = ChooseIndex(q);"),
        "side-effecting index must be hoisted into a single local after specialization:\n{after}"
    );
    assert!(
        !after.contains("if (ChooseIndex(q)") && !after.contains("else if (ChooseIndex(q)"),
        "specialized dispatch guards must not re-evaluate the side-effecting index:\n{after}"
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ChooseIndex(q : Qubit) : Int {
                X(q);
                1
            }
            operation RunAt(ops : (Qubit => Unit)[], q : Qubit) : Unit {
                ops[ChooseIndex(q)](q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunAt_AdjCtl_([I, X, Y], q);
                __quantum__rt__qubit_release(q);
            }
            operation RunAt_AdjCtl_(ops : (Qubit => Unit is Adj + Ctl)[], q : Qubit) : Unit {
                ops[ChooseIndex(q)](q);
            }
            // entry
            Main()

            AFTER:
            operation ChooseIndex(q : Qubit) : Int {
                X(q);
                1
            }
            operation RunAt(ops : (Qubit => Unit)[], q : Qubit) : Unit {
                ops[ChooseIndex(q)](q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunAt_AdjCtl__I__X__Y_(q);
                __quantum__rt__qubit_release(q);
            }
            operation RunAt_AdjCtl_(ops : (Qubit => Unit is Adj + Ctl)[], q : Qubit) : Unit {
                ops[ChooseIndex(q)](q);
            }
            operation RunAt_AdjCtl__I__X__Y_(q : Qubit) : Unit {
                {
                    let index : Int = ChooseIndex(q);
                    [(), (), ()][index];
                    if (index == 0) or (index == -3) {
                        I(q)
                    } else if (index == 1) or (index == -2) {
                        X(q)
                    } else {
                        Y(q)
                    }
                };
            }
            // entry
            Main()
        "#]],
    );
}

/// A callable array is forwarded into a higher-order function that receives it
/// as a plain array parameter and iterates over it, mirroring the shape of the
/// Deutsch-Jozsa sample where a list of oracles is looped over and each is run
/// through a driver operation. The forwarding operation `RunEach` takes the
/// callable array by value and its own `for` loop desugars to a `Length` call
/// plus an indexed read. The array must survive intact so `Length` and the
/// index stay well-formed, and the inner `Run(op, q)` dispatch is specialized
/// per candidate.
#[test]
fn callable_array_forwarded_to_iterating_hof_preserves_length_call() {
    let source = r#"
        operation Run(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation RunEach(ops : (Qubit => Unit)[], q : Qubit) : Unit {
            for op in ops {
                Run(op, q);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            RunEach([H, X], q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Run(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunEach(ops : (Qubit => Unit)[], q : Qubit) : Unit {
                {
                    let _array_id_55 : (Qubit => Unit)[] = ops;
                    let _len_id_59 : Int = Length(_array_id_55);
                    mutable _index_id_64 : Int = 0;
                    while _index_id_64 < _len_id_59 {
                        let op : (Qubit => Unit) = _array_id_55[_index_id_64];
                        Run_Empty_(op, q);
                        _index_id_64 += 1;
                    }

                }

            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunEach_AdjCtl_([H, X], q);
                __quantum__rt__qubit_release(q);
            }
            operation Run_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunEach_AdjCtl_(ops : (Qubit => Unit is Adj + Ctl)[], q : Qubit) : Unit {
                {
                    let _array_id_55 : (Qubit => Unit is Adj + Ctl)[] = ops;
                    let _len_id_59 : Int = Length(_array_id_55);
                    mutable _index_id_64 : Int = 0;
                    while _index_id_64 < _len_id_59 {
                        let op : (Qubit => Unit is Adj + Ctl) = _array_id_55[_index_id_64];
                        Run_Empty_(op, q);
                        _index_id_64 += 1;
                    }

                }

            }
            // entry
            Main()

            AFTER:
            operation Run(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunEach(ops : (Qubit => Unit)[], q : Qubit) : Unit {
                {
                    let _array_id_55 : (Qubit => Unit)[] = ops;
                    let _len_id_59 : Int = Length(_array_id_55);
                    mutable _index_id_64 : Int = 0;
                    while _index_id_64 < _len_id_59 {
                        let op : (Qubit => Unit) = _array_id_55[_index_id_64];
                        Run_Empty_(op, q);
                        _index_id_64 += 1;
                    }

                }

            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                RunEach_AdjCtl__H__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation Run_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation RunEach_AdjCtl_(ops : (Qubit => Unit is Adj + Ctl)[], q : Qubit) : Unit {
                {
                    let _array_id_55 : (Qubit => Unit is Adj + Ctl)[] = ops;
                    let _len_id_59 : Int = Length(_array_id_55);
                    mutable _index_id_64 : Int = 0;
                    while _index_id_64 < _len_id_59 {
                        let op : (Qubit => Unit is Adj + Ctl) = _array_id_55[_index_id_64];
                        Run_Empty_(op, q);
                        _index_id_64 += 1;
                    }

                }

            }
            operation RunEach_AdjCtl__H__X_(q : Qubit) : Unit {
                {
                    let _array_id_55 : (Qubit => Unit is Adj + Ctl)[] = [H, X];
                    let _len_id_59 : Int = Length(_array_id_55);
                    mutable _index_id_64 : Int = 0;
                    while _index_id_64 < _len_id_59 {
                        let op : (Qubit => Unit is Adj + Ctl) = _array_id_55[_index_id_64];
                        {
                            [(), ()][_index_id_64];
                            if (_index_id_64 == 0) or (_index_id_64 == -2) {
                                Run_Empty__H_(q)
                            } else {
                                Run_Empty__X_(q)
                            }
                        };
                        _index_id_64 += 1;
                    }

                }

            }
            operation Run_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            operation Run_Empty__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}
