// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Behavioral equivalence before and after the FIR pipeline: values, failures,
//! quantum effects, and receiver output. Structural and QIR assertions live in
//! the specialization, cross-package, and codegen test modules.

use indoc::formatdoc;
#[cfg(feature = "slow-proptest-tests")]
use proptest::prelude::*;
use qsc_eval::val::{Result as MeasurementResult, Value};

use super::test_cases;

pub(super) fn mixed_dispatch_owned_argument_cases() -> Vec<(String, i64)> {
    let mut cases = Vec::new();
    for flag in [false, true] {
        for fields in [
            "F=if flag { Inc } else { Twice }, N=n, G=Make(offset)",
            "G=Make(offset), N=n, F=if flag { Inc } else { Twice }",
        ] {
            let payload = format!("new Payload {{ {fields} }}");
            for call in [
                format!("Read({payload})"),
                format!("let stored={payload}; Read(stored)"),
                format!("let stored={payload}; Read(new Payload {{ ...stored, G=Make(offset) }})"),
                format!("ReadPair((0, {payload}))"),
                format!("ReadOuter(new Outer {{ Value={payload} }})"),
                "ReadLegacy(Legacy(if flag { Inc } else { Twice }, n, Make(offset)))".to_string(),
            ] {
                let source = formatdoc! {r#"
                    struct Payload {{ F : Int -> Int, G : Int -> Int, N : Int }}
                    struct Outer {{ Value : Payload }}
                    newtype Legacy = (F : Int -> Int, N : Int, G : Int -> Int);
                    function Inc(x : Int) : Int {{ x+1 }}
                    function Twice(x : Int) : Int {{ 2*x }}
                    function Make(offset : Int) : Int -> Int {{ x -> x+offset }}
                    function Read(p : Payload) : Int {{ p.F(p.N)+p.G(p.N) }}
                    function ReadPair(pair : (Int, Payload)) : Int {{
                        let (prefix, p)=pair; prefix+p.F(p.N)+p.G(p.N)
                    }}
                    function ReadOuter(p : Outer) : Int {{
                        p.Value.F(p.Value.N)+p.Value.G(p.Value.N)
                    }}
                    function ReadLegacy(p : Legacy) : Int {{
                        let (f, n, g)=p!; f(n)+g(n)
                    }}
                    function Pick(flag : Bool, n : Int, offset : Int) : Int {{ {call} }}
                    @EntryPoint() operation Main() : Int {{ Pick({flag}, 3, 10) }}
                "#};
                cases.push((source, if flag { 17 } else { 19 }));
            }
        }
    }
    cases
}

#[test]
fn mixed_dispatch_owned_arguments_preserve_values_and_bindings() {
    for (source, expected) in mixed_dispatch_owned_argument_cases() {
        check_projected_callee_result(&source, expected);
    }
}

#[test]
fn mixed_dispatch_owned_arguments_preserve_capture_occurrences_and_field_order() {
    for flag in [false, true] {
        for fields in [
            r#"F=selected, N=Log("first", n), G=Make(offset), Tail=Log("last", 7)"#,
            r#"Tail=Log("first", 7), G=Make(offset), N=Log("last", n), F=selected"#,
        ] {
            let source = formatdoc! {r#"
                struct Payload {{ F : Int -> Int, G : Int -> Int, N : Int, Tail : Int }}
                function Inc(x : Int) : Int {{ x+1 }}
                function Twice(x : Int) : Int {{ 2*x }}
                function Make(offset : Int) : Int -> Int {{ x -> x+offset }}
                function Log(label : String, n : Int) : Int {{ Message(label); n }}
                function Read(p : Payload) : Int {{ p.F(p.N)+p.G(p.N)+p.Tail }}
                function Pick(flag : Bool, n : Int, offset : Int) : Int {{
                    let selected=if flag {{ Inc }} else {{ Twice }};
                    Read(new Payload {{ {fields} }})
                }}
                @EntryPoint() operation Main() : Int {{
                    Pick({flag}, 3, 10)+Pick(not {flag}, 4, 20)
                }}
            "#};
            check_rewritten_callable_result(&source, if flag { 63 } else { 62 });
        }
    }
}

#[test]
fn mixed_dispatch_owned_arguments_preserve_first_field_failure() {
    for flag in [false, true] {
        for fields in [
            r#"F=selected, N=Fail("first"), G=Make(offset), Tail=Fail("last")"#,
            r#"Tail=Fail("first"), G=Make(offset), N=Fail("last"), F=selected"#,
        ] {
            let source = formatdoc! {r#"
                struct Payload {{ F : Int -> Int, G : Int -> Int, N : Int, Tail : Int }}
                function Inc(x : Int) : Int {{ x+1 }}
                function Twice(x : Int) : Int {{ 2*x }}
                function Make(offset : Int) : Int -> Int {{ x -> x+offset }}
                function Fail(label : String) : Int {{ Message(label); fail label }}
                function Read(p : Payload) : Int {{ p.F(p.N)+p.G(p.N)+p.Tail }}
                function Pick(flag : Bool, offset : Int) : Int {{
                    let selected=if flag {{ Inc }} else {{ Twice }};
                    Read(new Payload {{ {fields} }})
                }}
                @EntryPoint() operation Main() : Int {{ Pick({flag}, 10) }}
            "#};
            let error = crate::test_utils::eval_qsharp_original(&source)
                .expect_err("the first field must fail");
            assert!(error.contains("first"), "{error}");
            crate::test_utils::check_semantic_equivalence(&source);
        }
    }
}

#[test]
fn mixed_dispatch_owned_arguments_preserve_controlled_traces() {
    for functor in ["Controlled", "Controlled Controlled", "Adjoint Controlled"] {
        for flag in [false, true] {
            for enabled in [false, true] {
                let payload = "new Payload { F=selected, N=Data(target), G=Make(7) }";
                let args = if functor == "Controlled Controlled" {
                    format!("[outer], ([inner], ({payload}, target))")
                } else {
                    format!("[outer], ({payload}, target)")
                };
                let source = formatdoc! {r#"
                    struct Payload {{
                        F : Qubit => Unit is Adj + Ctl,
                        G : Qubit => Unit is Adj + Ctl, N : Int
                    }}
                    operation Toggle(tag : Int, q : Qubit) : Unit is Adj + Ctl {{
                        if tag==7 {{ X(q); }}
                    }}
                    function Make(tag : Int) : Qubit => Unit is Adj + Ctl {{ Toggle(tag, _) }}
                    operation Data(q : Qubit) : Int {{ Message("field"); X(q); 3 }}
                    operation Run(p : Payload, q : Qubit) : Unit is Adj + Ctl {{
                        if p.N==3 {{ p.F(q); p.G(q); }}
                    }}
                    operation Pick(flag : Bool, outer : Qubit, inner : Qubit, target : Qubit) : Unit {{
                        let selected=if flag {{ Z }} else {{ X }};
                        {functor} Run({args});
                    }}
                    @EntryPoint() operation Main() : Int {{
                        use outer=Qubit();
                        use inner=Qubit();
                        use target=Qubit();
                        if {enabled} {{ X(outer); X(inner); }}
                        Pick({flag}, outer, inner, target);
                        Reset(outer); Reset(inner);
                        if MResetZ(target)==One {{ 1 }} else {{ 0 }}
                    }}
                "#};
                check_rewritten_callable_result(&source, i64::from(!enabled || !flag));
            }
        }
    }
}

pub(super) fn dispatch_operand_struct_cases() -> Vec<(String, i64)> {
    let mut cases = Vec::new();
    let head = r#"{ Message("head"); set selector = not selector; set order = 10*order+1; 3 }"#;
    let tail = r#"{ Message("tail"); set order = 10*order+2; 7 }"#;
    let prefix = r#"{ Message("prefix"); set order = 10*order+3; 9 }"#;
    let selected = "if selector { Inc } else { Twice }";
    let payload = format!("new Payload {{ Head={head}, F={selected}, Tail={tail} }}");
    for flag in [true, false] {
        for (call, order, extra) in [
            (format!("Read({payload})"), 12, 0),
            (
                format!("Read(new Payload {{ Tail={tail}, Head={head}, F={selected} }})"),
                21,
                0,
            ),
            (format!("ReadNested(({prefix}, {payload}))"), 312, 9),
            (
                format!("ReadOuter(new Outer {{ Prefix={prefix}, Value={payload} }})"),
                312,
                9,
            ),
            (
                format!(
                    r#"Read(new Payload {{
                        ...new Payload {{
                            F=Inc, Head=0,
                            Tail={{ Message("copy"); set order=10*order+3; 7 }}
                        }},
                        Head={head}, F={selected}
                    }})"#
                ),
                31,
                0,
            ),
            (
                format!("ReadLegacy(Legacy({head}, {selected}, {tail}))"),
                12,
                0,
            ),
            (format!("let stored={payload}; Read(stored)"), 12, 0),
        ] {
            let source = formatdoc! {r#"
                struct Payload {{ F : Int -> Int, Head : Int, Tail : Int }}
                struct Outer {{ Value : Payload, Prefix : Int }}
                newtype Legacy = (Head : Int, F : Int -> Int, Tail : Int);
                function Inc(x : Int) : Int {{ x+1 }}
                function Twice(x : Int) : Int {{ 2*x }}
                function Read(p : Payload) : Int {{ p.F(p.Head)+p.Tail }}
                function ReadNested(data : (Int, Payload)) : Int {{
                    let (prefix, p)=data; prefix+Read(p)
                }}
                function ReadOuter(p : Outer) : Int {{ p.Prefix+Read(p.Value) }}
                function ReadLegacy(p : Legacy) : Int {{
                    let (head, f, tail)=p!; f(head)+tail
                }}
                function Choose(flag : Bool) : Int {{
                    mutable selector=flag;
                    mutable order=0;
                    let value={{ {call} }};
                    1000*order+value
                }}
                @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
            "#};
            cases.push((source, 1000 * order + extra + if flag { 13 } else { 11 }));
        }
    }
    cases
}

#[test]
fn dispatch_operand_struct_preserves_source_order_and_selection() {
    for (source, expected) in dispatch_operand_struct_cases() {
        check_callable_result(&source, expected);
    }
}

pub(super) fn dispatch_operand_reassigned_index_cases() -> Vec<(String, i64)> {
    let mut cases = Vec::new();
    for (first, second) in [(0, 1), (1, 0), (-2, -1), (-1, -2), (0, -1), (-1, 0)] {
        let first_value = if first == 0 || first == -2 { 4 } else { 6 };
        let second_value = if second == 0 || second == -2 { 4 } else { 6 };
        for (body, expected) in [
            ("set f=fs[second]; f(3)", second_value),
            ("set f=fs[second]; Apply(f, 3)", second_value),
            ("set f=fs[second]; let alias=f; alias(3)", second_value),
            (
                "let before=f; set f=fs[second]; 100*before(3)+f(3)",
                100 * first_value + second_value,
            ),
            (
                "mutable tag=0; set (tag, f)=(7, fs[second]); 100*tag+f(3)",
                700 + second_value,
            ),
            ("set f=fs[second]; f({ set f=fs[first]; 3 })", second_value),
            (
                r#"set f=fs[{ Message("store"); second }];
                   Apply(f, { Message("argument"); 3 })"#,
                second_value,
            ),
        ] {
            let source = formatdoc! {r#"
                function Inc(x : Int) : Int {{ x+1 }}
                function Twice(x : Int) : Int {{ 2*x }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Choose(first : Int, second : Int) : Int {{
                    let fs=[Inc, Twice];
                    mutable f=fs[first];
                    {body}
                }}
                @EntryPoint() operation Main() : Int {{ Choose({first}, {second}) }}
            "#};
            cases.push((source, expected));
        }
    }
    cases
}

#[test]
fn dispatch_operand_reassigned_index_preserves_reaching_selection() {
    for (source, expected) in dispatch_operand_reassigned_index_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn dispatch_operand_unassigned_index_locals_remain_specializable() {
    for binding in ["let", "mutable"] {
        let source = formatdoc! {r#"
            function Inc(x : Int) : Int {{ x+1 }}
            function Twice(x : Int) : Int {{ 2*x }}
            function Choose(index : Int) : Int {{
                let fs=[Inc, Twice];
                {binding} f=fs[index];
                f(3)
            }}
            @EntryPoint() operation Main() : Int {{ Choose(-1) }}
        "#};
        let (store, package) =
            crate::test_utils::compile_and_run_pipeline_to(&source, crate::PipelineStage::Defunc);
        let rendered = crate::pretty::write_package_qsharp_parseable(&store, package);
        assert!(rendered.contains("Inc(3)"), "{rendered}");
        assert!(rendered.contains("Twice(3)"), "{rendered}");
        check_callable_result(&source, 6);
    }
}

#[test]
fn dispatch_operand_struct_prefix_stays_in_its_lazy_branch() {
    for flag in [true, false] {
        let source = formatdoc! {r#"
            struct Payload {{ F : Int -> Int, N : Int }}
            function Inc(x : Int) : Int {{ x+1 }}
            function Twice(x : Int) : Int {{ 2*x }}
            function Read(p : Payload) : Int {{ p.F(p.N) }}
            function Choose(enabled : Bool) : Int {{
                mutable selector=true;
                mutable visits=0;
                let value=if enabled {{
                    Read(new Payload {{
                        N={{ set visits+=1; set selector=false; 3 }},
                        F=if selector {{ Inc }} else {{ Twice }}
                    }})
                }} else {{ 0 }};
                100*visits+value
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
        check_callable_result(&source, if flag { 106 } else { 0 });
    }
}

#[test]
fn dispatch_operand_failures_preserve_earlier_evaluations() {
    let source = r#"
        struct Payload { F : Int -> Int, N : Int, Tail : Int }
        function Inc(x : Int) : Int { x+1 }
        function Twice(x : Int) : Int { 2*x }
        function Read(p : Payload, extra : Int) : Int { p.F(p.N)+p.Tail+extra }
        function Choose(flag : Bool) : Int {
            mutable selector=flag;
            Read(new Payload {
                N={ Message("head"); set selector=not selector; fail "first field" },
                F=if selector { Inc } else { Twice },
                Tail={ fail "later field" }
            }, { fail "later argument" })
        }
        @EntryPoint() operation Main() : Int { Choose(true) }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source)
        .expect_err("the first field must fail before later operands");
    assert!(error.contains("first field"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);

    for (first, second) in [(0, 2), (0, -3), (2, 1), (-3, 1)] {
        let source = formatdoc! {r#"
            function Inc(x : Int) : Int {{ x+1 }}
            function Twice(x : Int) : Int {{ 2*x }}
            function Choose(first : Int, second : Int) : Int {{
                let fs=[Inc, Twice];
                mutable f=fs[{{ Message("initial"); first }}];
                set f=fs[{{ Message("store"); second }}];
                f({{ fail "argument" }})
            }}
            @EntryPoint() operation Main() : Int {{ Choose({first}, {second}) }}
        "#};
        let error = crate::test_utils::eval_qsharp_original(&source)
            .expect_err("selection bounds must fail before invocation arguments");
        assert!(error.starts_with("IndexOutOfRange("), "{error}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn dispatch_operand_struct_and_reassigned_index_preserve_quantum_trace() {
    for flag in [true, false] {
        for body in [
            r#"
                mutable selector=flag;
                Run(new Payload {
                    N={ H(q); set selector=not selector; 0 },
                    F=if selector { X } else { Z },
                    Q={ H(q); q }
                });
            "#,
            r#"
                let ops=[X, Z];
                mutable f=ops[first];
                set f=ops[{ H(q); second }];
                f({ H(q); q });
            "#,
        ] {
            let source = formatdoc! {r#"
                struct Payload {{ F : Qubit => Unit, N : Int, Q : Qubit }}
                operation Run(p : Payload) : Unit {{ p.F(p.Q); }}
                operation Pick(first : Int, second : Int, flag : Bool, q : Qubit) : Unit {{
                    {body}
                }}
                @EntryPoint() operation Main() : Int {{
                    use q=Qubit();
                    Y(q);
                    Pick({first}, {second}, {flag}, q);
                    Reset(q);
                    0
                }}
            "#, first = i32::from(!flag), second = i32::from(flag)};
            let (store, package) = crate::test_utils::compile_to_fir(&source);
            let (_, trace) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package);
            let gates: Vec<_> = trace
                .iter()
                .filter_map(|operation| match operation {
                    crate::test_utils::TraceOp::Gate { name, .. } => Some(name.as_str()),
                    _ => None,
                })
                .collect();
            assert_eq!(gates, ["Y", "H", "H", if flag { "Z" } else { "X" }]);
            check_callable_result(&source, 0);
        }
    }
}

pub(super) const PROJECTED_CALLEE_EFFECTS: &str = r#"
    struct Choice { F : Int -> Int, Stamp : Int }
    function Inc(x : Int) : Int { x + 1 }
    function Twice(x : Int) : Int { 2 * x }
    function Pick(flag : Bool) : Int {
        mutable order = 0;
        let value = (new Choice {
            F = if flag { Inc } else { Twice },
            Stamp = { set order = 10 * order + 1; 0 }
        }).F({ set order = 10 * order + 2; 3 });
        100 * order + value
    }
    @EntryPoint()
    operation Main() : Int { Pick(true) + Pick(false) }
"#;

pub(super) const PROJECTED_HOF_CAPTURE: &str = r#"
    struct Payload { F : Int -> Int, N : Int }
    struct Dispatch { Run : Payload -> Int }
    function Add(offset : Int, x : Int) : Int { offset + x }
    function Apply(f : Int -> Int, x : Int) : Int { f(x) }
    function First(p : Payload) : Int { Apply(p.F, p.N) }
    function Second(p : Payload) : Int { 100 + Apply(p.F, p.N) }
    function Pick(flag : Bool) : Int {
        mutable visits = 0;
        let value = (new Dispatch {
            Run = if flag { First } else { Second }
        }).Run(new Payload {
            F = Add({ set visits += 1; 10 }, _),
            N = 3
        });
        1000 * visits + value
    }
    @EntryPoint()
    operation Main() : Int { Pick(true) + Pick(false) }
"#;

#[test]
fn projected_callee_preserves_complete_aggregate_evaluation() {
    check_projected_callee_result(PROJECTED_CALLEE_EFFECTS, 2410);
}

#[test]
fn projected_callee_hof_owns_inline_struct_captures() {
    check_projected_callee_result(PROJECTED_HOF_CAPTURE, 2126);
}

pub(super) fn projected_callee_snapshot_cases() -> Vec<(String, i64)> {
    let mut cases = Vec::new();
    for flag in [false, true] {
        for capturing in [false, true] {
            for nested in [false, true] {
                let (first, second, selected, prefix) = if capturing {
                    (
                        "Add({ set order = 10 * order + 4; offset }, _)",
                        "Add({ set order = 10 * order + 4; 2 * offset }, _)",
                        if flag { 13 } else { 23 },
                        4,
                    )
                } else {
                    ("Inc", "Twice", if flag { 4 } else { 6 }, 0)
                };
                let choice = formatdoc! {r#"
                    new Choice {{
                        F = if selector {{ {first} }} else {{ {second} }},
                        Stamp = {{
                            set selector = not selector;
                            set order = 10 * order + 1;
                            0
                        }}
                    }}
                "#};
                let callee = if nested {
                    formatdoc! {r#"
                        (new Outer {{
                            Inner = {choice},
                            Stamp = {{ set order = 10 * order + 2; 0 }}
                        }}).Inner.F
                    "#}
                } else {
                    format!("({choice}).F")
                };
                let order = if nested {
                    1000 * prefix + 123
                } else {
                    100 * prefix + 13
                };
                let source = formatdoc! {r#"
                    struct Choice {{ F : Int -> Int, Stamp : Int }}
                    struct Outer {{ Inner : Choice, Stamp : Int }}
                    function Inc(x : Int) : Int {{ x + 1 }}
                    function Twice(x : Int) : Int {{ 2 * x }}
                    function Add(offset : Int, x : Int) : Int {{ offset + x }}
                    function Pick(flag : Bool, offset : Int) : Int {{
                        mutable selector = flag;
                        mutable order = 0;
                        let value = ({callee})({{ set order = 10 * order + 3; 3 }});
                        100 * order + value
                    }}
                    @EntryPoint() operation Main() : Int {{ Pick({flag}, 10) }}
                "#};
                cases.push((source, 100 * order + selected));
            }
        }
    }
    cases
}

#[test]
fn projected_callee_preserves_selection_and_capture_snapshots() {
    for (source, expected) in projected_callee_snapshot_cases() {
        check_projected_callee_result(&source, expected);
    }
}

pub(super) fn projected_callee_hof_cases() -> Vec<(String, i64)> {
    let mut cases = Vec::new();
    for flag in [false, true] {
        for nested in [false, true] {
            let dispatch = r#"
                new Dispatch {
                    Run = if flag { First } else { Second },
                    Stamp = { set order = 10 * order + 1; 0 }
                }
            "#;
            let callee = if nested {
                formatdoc! {r#"
                    (new Outer {{
                        Inner = {dispatch},
                        Stamp = {{ set order = 10 * order + 2; 0 }}
                    }}).Inner.Run
                "#}
            } else {
                format!("({dispatch}).Run")
            };
            let source = formatdoc! {r#"
                struct Payload {{ F : Int -> Int, N : Int }}
                struct Dispatch {{ Run : Payload -> Int, Stamp : Int }}
                struct Outer {{ Inner : Dispatch, Stamp : Int }}
                function Add(offset : Int, x : Int) : Int {{ offset + x }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function First(p : Payload) : Int {{ Apply(p.F, p.N) }}
                function Second(p : Payload) : Int {{ 100 + Apply(p.F, p.N) }}
                function Pick(flag : Bool) : Int {{
                    mutable order = 0;
                    let value = ({callee})(new Payload {{
                        F = Add({{ set order = 10 * order + 3; 10 }}, _),
                        N = {{ set order = 10 * order + 4; 3 }}
                    }});
                    1000 * order + value
                }}
                @EntryPoint() operation Main() : Int {{ Pick({flag}) }}
            "#};
            let order = if nested { 1234 } else { 134 };
            cases.push((source, 1000 * order + if flag { 13 } else { 113 }));
        }
    }
    cases
}

#[test]
fn projected_callee_hof_preserves_nested_effects_and_capture_ownership() {
    for (source, expected) in projected_callee_hof_cases() {
        check_projected_callee_result(&source, expected);
    }
}

#[test]
fn projected_callee_preserves_failure_order() {
    for flag in [false, true] {
        for nested in [false, true] {
            for capture_fails in [false, true] {
                let (first, second, expected) = if capture_fails {
                    (
                        "Add(CaptureFailure(), _)",
                        "Add(CaptureFailure(), _)",
                        "capture first",
                    )
                } else {
                    ("Inc", "Twice", "callee first")
                };
                let choice = formatdoc! {r#"
                    new Choice {{
                        F = if flag {{ {first} }} else {{ {second} }},
                        Stamp = {{ fail "callee first" }}
                    }}
                "#};
                let callee = if nested {
                    format!(
                        "(new Outer {{ Inner = {choice}, Stamp = {{ fail \"outer first\" }} }}).Inner.F"
                    )
                } else {
                    format!("({choice}).F")
                };
                let source = formatdoc! {r#"
                    struct Choice {{ F : Int -> Int, Stamp : Int }}
                    struct Outer {{ Inner : Choice, Stamp : Int }}
                    function Inc(x : Int) : Int {{ x + 1 }}
                    function Twice(x : Int) : Int {{ 2 * x }}
                    function Add(offset : Int, x : Int) : Int {{ offset + x }}
                    function CaptureFailure() : Int {{ fail "capture first" }}
                    function Pick(flag : Bool) : Int {{
                        ({callee})({{ fail "argument first" }})
                    }}
                    @EntryPoint() operation Main() : Int {{ Pick({flag}) }}
                "#};
                let error = crate::test_utils::eval_qsharp_original(&source)
                    .expect_err("the callee must fail before the argument");
                assert!(error.contains(expected), "{error}");
                crate::test_utils::check_semantic_equivalence(&source);
            }
        }
    }
}

#[test]
fn projected_callee_hof_failure_precedes_argument_capture() {
    for flag in [false, true] {
        let source = formatdoc! {r#"
            struct Payload {{ F : Int -> Int }}
            struct Dispatch {{ Run : Payload -> Int, Stamp : Int }}
            struct Outer {{ Inner : Dispatch, Stamp : Int }}
            function Add(offset : Int, x : Int) : Int {{ offset + x }}
            function CaptureFailure() : Int {{ fail "argument capture" }}
            function First(p : Payload) : Int {{ p.F(3) }}
            function Second(p : Payload) : Int {{ 100 + p.F(3) }}
            function Pick(flag : Bool) : Int {{
                (new Outer {{
                    Inner = new Dispatch {{
                        Run = if flag {{ First }} else {{ Second }},
                        Stamp = {{ fail "callee first" }}
                    }},
                    Stamp = {{ fail "outer first" }}
                }}).Inner.Run(new Payload {{ F = Add(CaptureFailure(), _) }})
            }}
            @EntryPoint() operation Main() : Int {{ Pick({flag}) }}
        "#};
        let error = crate::test_utils::eval_qsharp_original(&source)
            .expect_err("callee construction must fail before argument capture");
        assert!(error.contains("callee first"), "{error}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

fn check_projected_callee_result(source: &str, expected: i64) {
    use qsc_fir::fir::{ItemKind, PackageLookup};

    let (store, package) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
    for owner in crate::reachability::collect_reachable_from_entry(&store, package) {
        let package = store.get(owner.package);
        if let ItemKind::Callable(decl) = &package.get_item(owner.item).kind {
            crate::invariants::check_local_var_consistency(package, decl);
        }
    }
    check_callable_result(source, expected);
}

pub(super) fn callable_array_partial_sibling_cases() -> Vec<(String, i64)> {
    let mut cases = Vec::new();
    for reversed in [false, true] {
        let parameters = if reversed {
            "pair : (Int -> Int, Int), ops : (Int -> Int)[]"
        } else {
            "ops : (Int -> Int)[], pair : (Int -> Int, Int)"
        };
        for (callable, expected) in [("Inc", 10), ("x -> x + offset", 19)] {
            for stored in [false, true] {
                let pair = if stored {
                    "pair".to_string()
                } else {
                    format!("({callable}, Data())")
                };
                let arguments = if reversed {
                    format!("{pair}, [Inc, Twice]")
                } else {
                    format!("[Inc, Twice], {pair}")
                };
                let binding = if stored {
                    format!("let pair = ({callable}, Data()); Message(\"stored pair\");")
                } else {
                    String::new()
                };
                let source = formatdoc! {r#"
                    function Inc(x : Int) : Int {{ x + 1 }}
                    function Twice(x : Int) : Int {{ 2 * x }}
                    function Data() : Int {{ Message("data"); 3 }}
                    function Run({parameters}) : Int {{
                        let (f, n) = pair;
                        ops[1](n) + f(n)
                    }}
                    function Pick(offset : Int) : Int {{
                        {binding}
                        Run({arguments})
                    }}
                    @EntryPoint() operation Main() : Int {{ Pick(10) }}
                "#};
                cases.push((source, expected));
            }
        }
    }
    cases
}

#[test]
fn callable_array_partial_sibling_preserves_data_and_capture_layout() {
    for (source, expected) in callable_array_partial_sibling_cases() {
        check_rewritten_callable_result(&source, expected);
    }
}

pub(super) fn indexed_array_effect_cases() -> impl Iterator<Item = (String, i64)> {
    [(-2, 4), (-1, 6), (0, 4), (1, 6)]
        .into_iter()
        .map(|(index, value)| {
            let source = formatdoc! {r#"
                function Inc(x : Int) : Int {{ x + 1 }}
                function Twice(x : Int) : Int {{ 2 * x }}
                function Pick(index : Int) : Int {{
                    mutable order = 0;
                    let result = ({{
                        Message("array");
                        set order = 10 * order + 1;
                        [Inc, Twice]
                    }})[{{
                        Message("index");
                        set order = 10 * order + 2;
                        index
                    }}]({{
                        Message("argument");
                        set order = 10 * order + 3;
                        3
                    }});
                    100 * order + result
                }}
                @EntryPoint() operation Main() : Int {{ Pick({index}) }}
            "#};
            (source, 12300 + value)
        })
}

#[test]
fn indexed_array_construction_preserves_value_and_effect_order() {
    for (source, expected) in indexed_array_effect_cases() {
        check_rewritten_callable_result(&source, expected);
    }
}

#[test]
fn indexed_array_control_flow_preserves_selected_construction() {
    for (flag, expected) in [(true, 124), (false, 226)] {
        let source = formatdoc! {r#"
            function Inc(x : Int) : Int {{ x + 1 }}
            function Twice(x : Int) : Int {{ 2 * x }}
            function Pick(flag : Bool, index : Int) : Int {{
                mutable order = 0;
                let result = (if flag {{
                    Message("first array");
                    set order = 1;
                    [Inc, Twice]
                }} else {{
                    Message("second array");
                    set order = 2;
                    [Twice, Inc]
                }})[{{ set order = 10 * order + 2; index }}](3);
                10 * order + result
            }}
            @EntryPoint() operation Main() : Int {{ Pick({flag}, 0) }}
        "#};
        check_rewritten_callable_result(&source, expected);
    }
}

#[test]
fn indexed_array_construction_precedes_bounds_and_argument_failures() {
    for index in [-3, 2] {
        let source = formatdoc! {r#"
            function Inc(x : Int) : Int {{ x + 1 }}
            function Twice(x : Int) : Int {{ 2 * x }}
            function Argument() : Int {{ fail "argument" }}
            function Pick(index : Int) : Int {{
                ({{ Message("array"); [Inc, Twice] }})[index](Argument())
            }}
            @EntryPoint() operation Main() : Int {{ Pick({index}) }}
        "#};
        let error = crate::test_utils::eval_qsharp_original(&source)
            .expect_err("bounds must fail before the argument");
        assert!(error.starts_with("IndexOutOfRange("), "{error}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
    let source = r#"
        function Inc(x : Int) : Int { x + 1 }
        function Twice(x : Int) : Int { 2 * x }
        function ArrayPrefix() : Unit { fail "array"; }
        function Pick(index : Int) : Int {
            ({ ArrayPrefix(); [Inc, Twice] })[index](3)
        }
        @EntryPoint() operation Main() : Int { Pick(2) }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source)
        .expect_err("array construction must fail before bounds");
    assert!(error.contains("array"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

pub(super) fn mixed_stored_argument_cases() -> Vec<(String, i64)> {
    let mut cases = Vec::new();
    for (flag, expected) in [(true, 17), (false, 19)] {
        for call in [
            "Run(args)",
            "let alias = args; Run(alias)",
            "Run(args) + Run(args)",
        ] {
            let source = formatdoc! {r#"
                function Inc(x : Int) : Int {{ x + 1 }}
                function Twice(x : Int) : Int {{ 2 * x }}
                function Data() : Int {{ Message("stored data"); 3 }}
                function Run(f : Int -> Int, g : Int -> Int, n : Int) : Int {{
                    f(n) + g(n)
                }}
                function Pick(flag : Bool, offset : Int) : Int {{
                    let chosen = if flag {{ Inc }} else {{ Twice }};
                    let other = x -> x + offset;
                    let args = (chosen, other, Data());
                    Message("stored");
                    {call}
                }}
                @EntryPoint() operation Main() : Int {{ Pick({flag}, 10) }}
            "#};
            cases.push((
                source,
                if call.contains('+') {
                    2 * expected
                } else {
                    expected
                },
            ));
        }
    }
    cases
}

#[test]
fn mixed_dispatch_projects_stored_argument_values_once() {
    for (source, expected) in mixed_stored_argument_cases() {
        check_rewritten_callable_result(&source, expected);
    }
}

#[test]
fn mixed_dispatch_projects_stored_controlled_arguments() {
    for functor in ["Controlled", "Controlled Controlled", "Adjoint Controlled"] {
        for (flag, expected) in [(true, 0), (false, 1)] {
            let args = if functor == "Controlled Controlled" {
                "([outer], ([inner], (chosen, other, target)))"
            } else {
                "([outer], (chosen, other, target))"
            };
            let source = formatdoc! {r#"
                operation FlipWhen(tag : Int, q : Qubit) : Unit is Adj + Ctl {{
                    if tag == 7 {{ X(q); }}
                }}
                operation Run(
                    f : Qubit => Unit is Adj + Ctl,
                    g : Qubit => Unit is Adj + Ctl, q : Qubit
                ) : Unit is Adj + Ctl {{
                    f(q); g(q);
                }}
                operation Pick(flag : Bool, tag : Int) : Int {{
                    use outer = Qubit();
                    use inner = Qubit();
                    use target = Qubit();
                    X(outer); X(inner);
                    let chosen = if flag {{ X }} else {{ I }};
                    let other = FlipWhen(tag, _);
                    let args = {args};
                    {functor} Run(args);
                    Reset(outer); Reset(inner);
                    if MResetZ(target) == One {{ 1 }} else {{ 0 }}
                }}
                @EntryPoint() operation Main() : Int {{ Pick({flag}, 7) }}
            "#};
            check_rewritten_callable_result(&source, expected);
        }
    }
}

pub(super) fn surviving_unit_payload_cases() -> Vec<(String, i64)> {
    [
        (
            "Run((x -> x + offset, ()))",
            "if unit == () { f(3) } else { 0 }",
            true,
            13,
        ),
        (
            "let args = (x -> x + offset, ()); Run(args)",
            "if unit == () { f(3) } else { 0 }",
            true,
            13,
        ),
        (
            "mutable marker = 0;
             let result = Run((x -> x + offset, { Message(\"unit\"); set marker = 1; () }));
             1000 * marker + result",
            "f(3)",
            true,
            1013,
        ),
        (
            "let chosen = if flag { x -> x + offset } else { x -> x - offset };
             Run((chosen, ()))",
            "if unit == () { f(3) } else { 0 }",
            true,
            13,
        ),
        (
            "let chosen = if flag { x -> x + offset } else { x -> x - offset };
             Run((chosen, ()))",
            "if unit == () { f(3) } else { 0 }",
            false,
            -7,
        ),
    ]
    .into_iter()
    .map(|(call, body, flag, expected)| {
        let source = formatdoc! {r#"
            function Run(pair : (Int -> Int, Unit)) : Int {{
                let (f, unit) = pair;
                {body}
            }}
            function Pick(flag : Bool, offset : Int) : Int {{ {call} }}
            @EntryPoint() operation Main() : Int {{ Pick({flag}, 10) }}
        "#};
        (source, expected)
    })
    .collect()
}

#[test]
fn single_slot_specialization_preserves_surviving_unit_payloads() {
    for (source, expected) in surviving_unit_payload_cases() {
        check_rewritten_callable_result(&source, expected);
    }
}

fn check_rewritten_callable_result(source: &str, expected: i64) {
    check_callable_result(source, expected);
    let (mut store, package) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
    crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package, &[]);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, package),
        Ok(qsc_eval::val::Value::Int(expected)),
        "{source}",
    );
}

#[test]
fn guarded_indexed_candidates_preserve_selected_values() {
    for flag in [false, true] {
        for (index, indexed_value) in [(-2, 4), (-1, 6), (0, 4), (1, 6)] {
            for (selection, selects_index) in [
                ("if flag { fs[index] } else { Inc }", flag),
                ("if flag { Inc } else { fs[index] }", !flag),
                (
                    "if flag { if inner { fs[index] } else { Twice } } else { Inc }",
                    flag,
                ),
            ] {
                for call in ["selected(3)", "Apply(selected, 3)"] {
                    let source = formatdoc! {r#"
                        function Inc(x : Int) : Int {{ x+1 }}
                        function Twice(x : Int) : Int {{ 2*x }}
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Pick(flag : Bool, inner : Bool, index : Int) : Int {{
                            let fs = [Inc, Twice];
                            let selected = {selection};
                            {call}
                        }}
                        @EntryPoint() operation Main() : Int {{ Pick({flag}, true, {index}) }}
                    "#};
                    check_callable_result(&source, if selects_index { indexed_value } else { 4 });
                }
            }
        }
    }
}

#[test]
fn guarded_indexed_candidates_preserve_bounds_and_effect_order() {
    for flag in [false, true] {
        for index in ["-3", "2", "-9223372036854775807", "9223372036854775807"] {
            for call in ["selected(Argument())", "Apply(selected, Argument())"] {
                let source = formatdoc! {r#"
                    function Inc(x : Int) : Int {{ x+1 }}
                    function Twice(x : Int) : Int {{ 2*x }}
                    function Index(index : Int) : Int {{ Message("index"); index }}
                    function Argument() : Int {{ Message("argument"); 3 }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    function Pick(flag : Bool, index : Int) : Int {{
                        let fs = [Inc, Twice];
                        let selected = if flag {{ fs[Index(index)] }} else {{ Inc }};
                        Message("selected");
                        {call}
                    }}
                    @EntryPoint() operation Main() : Int {{ Pick({flag}, {index}) }}
                "#};
                if flag {
                    let error = crate::test_utils::eval_qsharp_original(&source)
                        .expect_err("the selected array index must fail before the argument");
                    assert!(error.starts_with("IndexOutOfRange("), "{error}");
                    crate::test_utils::check_semantic_equivalence(&source);
                } else {
                    check_callable_result(&source, 4);
                }
            }
        }
    }
}

#[test]
fn stored_controlled_closure_layout_preserves_functor_alias() {
    check_stored_controlled_closure_layout(
        "let controlledValue = Controlled partial; controlledValue([], 42);",
    );
}

#[test]
fn stored_controlled_closure_layout_preserves_input_tuple() {
    check_stored_controlled_closure_layout("let args = ([], 42); Controlled partial(args);");
}

#[test]
fn stored_controlled_closure_layout_preserves_both_aliases() {
    check_stored_controlled_closure_layout(
        "let controlledValue = Controlled partial; let args = ([], 42); controlledValue(args);",
    );
}

fn check_stored_controlled_closure_layout(body: &str) {
    let source = formatdoc! {r#"
        operation Target(n : Int, x : Int) : Unit is Ctl {{
            body (...) {{ fail $"body:{{n}}:{{x}}"; }}
            controlled (controls, ...) {{ fail $"ctl:{{n}}:{{x}}"; }}
        }}
        @EntryPoint() operation Main() : Unit {{
            let partial = Target(7, _);
            {body}
        }}
    "#};
    let error = crate::test_utils::eval_qsharp_original(&source)
        .expect_err("the controlled specialization reports its inputs");
    assert!(error.contains("ctl:7:42"), "{error}");
    crate::test_utils::check_semantic_equivalence(&source);
}

#[test]
fn stored_controlled_closure_layout_preserves_gates_and_argument_effects() {
    for functor in ["Controlled", "Controlled Controlled", "Adjoint Controlled"] {
        let arguments = if functor == "Controlled Controlled" {
            "([outer], ([inner], target))"
        } else {
            "([outer], target)"
        };
        for body in [
            format!("let controlledValue = {functor} selected; controlledValue{arguments};"),
            format!("let args = {arguments}; {functor} selected(args);"),
            format!(
                "let controlledValue = {functor} selected; \
                 controlledValue({{ Message(\"arguments\"); {arguments} }});"
            ),
        ] {
            for first in [false, true] {
                for enabled in [false, true] {
                    let source = formatdoc! {r#"
                        function Angle() : Double {{ Message("capture"); 1.5707963267948966 }}
                        operation Rotate(angle : Double, q : Qubit) : Unit is Adj + Ctl {{
                            Ry(angle, q);
                        }}
                        operation Invoke(first : Bool, outer : Qubit, inner : Qubit, target : Qubit) : Unit {{
                            let angle = Angle();
                            let positive = Rotate(angle, _);
                            let negative = Rotate(-angle, _);
                            let selected = if first {{ positive }} else {{ negative }};
                            {body}
                        }}
                        @EntryPoint() operation Main() : Int {{
                            use outer = Qubit();
                            use inner = Qubit();
                            use target = Qubit();
                            if {enabled} {{ X(outer); X(inner); H(target); }}
                            Invoke({first}, outer, inner, target);
                            Reset(outer);
                            Reset(inner);
                            if MResetZ(target) == One {{ 1 }} else {{ 0 }}
                        }}
                    "#};
                    let expected = i64::from(enabled && (first != functor.contains("Adjoint")));
                    check_callable_result(&source, expected);
                    let (mut store, package) = crate::test_utils::compile_and_run_pipeline_to(
                        &source,
                        crate::PipelineStage::Defunc,
                    );
                    crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package, &[]);
                    assert_eq!(
                        crate::test_utils::try_eval_fir_entry(&store, package),
                        Ok(qsc_eval::val::Value::Int(expected)),
                        "{functor}: {body}, first={first}, enabled={enabled}",
                    );
                }
            }
        }
    }
}

#[test]
fn aliased_hof_callees_preserve_mutating_argument_order() {
    for flag in [false, true] {
        let source = formatdoc! {r#"
            function Add(n : Int, x : Int) : Int {{ n+x }}
            function First(n : Int, f : Int -> Int) : Int {{ 100*n+f(3) }}
            function Second(n : Int, f : Int -> Int) : Int {{ 200*n+f(3) }}
            function Pick(flag : Bool) : Int {{
                mutable order = 0;
                let selected = if flag {{ First }} else {{ Second }};
                let result = selected(
                    {{ set order = 10*order+1; 4 }},
                    Add({{ set order = 10*order+2; 2 }}, _));
                1000*order+result
            }}
            @EntryPoint() operation Main() : Int {{ Pick({flag}) }}
        "#};
        check_callable_result(&source, if flag { 12405 } else { 12805 });
    }
}

#[test]
fn conditional_hof_callees_preserve_branch_argument_ownership() {
    for (source, expected) in test_cases::conditional_hof_argument_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn conditional_hof_callees_preserve_first_argument_failure() {
    let source = r#"
        function Fail(label : String) : Int { fail label }
        function Add(n : Int, x : Int) : Int { n+x }
        function First(n : Int, f : Int -> Int) : Int { n+f(3) }
        function Second(n : Int, f : Int -> Int) : Int { n-f(3) }
        function Pick(flag : Bool) : Int {
            (if flag {First} else {Second})(Fail("first"),Add(Fail("capture"),_))
        }
        @EntryPoint() operation Main() : Int { Pick(false) }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source).expect_err("first argument fails");
    assert!(error.contains("first"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn conditional_controlled_hof_callees_preserve_arguments() {
    for functor in ["Controlled", "Controlled Controlled", "Adjoint Controlled"] {
        for first in [true, false] {
            for enabled in [true, false] {
                let args = if functor == "Controlled Controlled" {
                    "[outer], ([inner], (Head(), Ry(Angle(),_), target))"
                } else {
                    "[outer], (Head(), Ry(Angle(),_), target)"
                };
                let source = formatdoc! {r#"
                    function Head() : Int {{ Message("head"); 4 }}
                    function Angle() : Double {{ Message("capture"); 1.5707963267948966 }}
                    operation First(n : Int, f : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl {{
                        if n==4 {{ f(q); }}
                    }}
                    operation Second(n : Int, f : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl {{
                        if n==4 {{ Adjoint f(q); }}
                    }}
                    operation Pick(flag : Bool, outer : Qubit, inner : Qubit, target : Qubit) : Unit {{
                        {functor} (if flag {{First}} else {{Second}})({args});
                    }}
                    @EntryPoint() operation Main() : Int {{
                        use outer=Qubit();
                        use inner=Qubit();
                        use target=Qubit();
                        if {enabled} {{ X(outer); X(inner); H(target); }}
                        Pick({first},outer,inner,target);
                        Reset(outer);
                        Reset(inner);
                        if MResetZ(target)==One {{ 1 }} else {{ 0 }}
                    }}
                "#};
                check_callable_result(
                    &source,
                    i64::from(enabled && (first != functor.contains("Adjoint"))),
                );
            }
        }
    }
}

#[test]
fn nested_closures_preserve_producer_capture_values() {
    for (source, expected) in test_cases::nested_environment_capture_cases() {
        check_callable_result(&source, expected);
    }
}

/// The nested operation must retain its captured pi angle, not the caller's
/// zero-valued sentinel. Both rotation directions flip the qubit; empty controls
/// exercise functor nesting, and trace comparison also checks the applied gates.
#[test]
fn nested_operation_captures_preserve_rotation_effects_through_functors() {
    for functor in [
        "",
        "Adjoint",
        "Controlled",
        "Controlled Controlled",
        "Adjoint Controlled",
    ] {
        let args = match functor.matches("Controlled").count() {
            0 => "q",
            1 => "[], q",
            _ => "[], ([], q)",
        };
        let source = formatdoc! {r#"
            function Make(angle : Double) : Qubit => Unit is Adj + Ctl {{
                let inner : Qubit => Unit is Adj + Ctl = q=>Ry(angle,q);
                let ops=[inner];
                q=>{{ let op=ops[0]; op(q); }}
            }}
            @EntryPoint() operation Main() : Int {{
                let sentinel=0.0;
                let op=Make(3.141592653589793);
                use q=Qubit();
                {functor} op({args});
                if MResetZ(q) == One {{ 1 }} else {{ 0 }}
            }}
        "#};
        check_callable_result(&source, 1);
    }
}

#[test]
fn data_only_factory_uses_preserve_other_closure_captures() {
    for (source, expected) in test_cases::surviving_producer_capture_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn data_only_factory_uses_preserve_functor_wrapped_closure_captures() {
    for source in test_cases::surviving_producer_functor_cases() {
        check_callable_result(&source, 7);
    }
}

/// Copying the capture expression must preserve the free `outer` reference and
/// freshen `inner` without changing its binding: `3 + (1 + (3 + 1) * 2) = 12`.
#[test]
fn copied_capture_block_preserves_free_and_bound_locals() {
    check_callable_result(
        r#"
        function Make(offset : Int) : Int -> Int { x->x+offset }
        @EntryPoint() operation Main() : Int {
            let outer=3;
            let f=Make({let inner=outer+1;inner*2});
            outer+f(1)
        }
        "#,
        12,
    );
}

#[test]
fn indirect_factory_calls_preserve_returned_closure_captures() {
    for (source, expected) in test_cases::indirect_factory_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn capturing_indirect_factory_preserves_outer_and_returned_captures() {
    check_callable_result(test_cases::CAPTURING_INDIRECT_FACTORY, 108);
}

/// Exercise both reaching factory values. Previously consumed direct closures
/// contribute 120; the indirect result contributes 6 or 9 according to the branch.
#[test]
fn conditional_factory_assignment_preserves_both_returned_closure_behaviors() {
    for choose_second in [false, true] {
        let source = formatdoc! {r#"
            function Make(n : Int) : Int -> Int {{ x->x+n }}
            function Other(n : Int) : Int -> Int {{ x->x+2*n }}
            @EntryPoint() operation Main() : Int {{
                let first=Make(1);
                let second=Other(1);
                mutable factory=Make;
                for i in 0..0 {{ if {choose_second} {{ set factory=Other; }} }}
                let deferred=factory(3);
                100*first(0)+10*second(0)+deferred(3)
            }}
        "#};
        check_callable_result(&source, if choose_second { 129 } else { 126 });
    }
}

/// Relay's `make:3` message must occur once before `ready`, not be removed or
/// replayed when reconstructing the returned closure.
#[test]
fn indirect_factory_call_preserves_message_order_and_capture_values() {
    check_callable_result(
        r#"
        function Make(n : Int) : Int -> Int { x->x+n }
        function Relay(n : Int) : Int -> Int { Message($"make:{n}"); Make(n) }
        @EntryPoint() operation Main() : Int {
            let direct=Make(1);
            mutable factory=Relay;
            for i in 0..0 { set factory=Relay; }
            let deferred=factory(3);
            Message("ready");
            100*direct(0)+deferred(3)
        }
        "#,
        106,
    );
}

/// Reading only `data.N` must not authorize cleanup of the same factory's
/// callable-valued result used through `factory`: `100 * 1 + 9 + (3 + 3) = 115`.
#[test]
fn data_only_factory_use_preserves_indirectly_called_closure() {
    check_callable_result(
        r#"
        struct Box { F : Int -> Int, N : Int }
        function Make(n : Int) : Box { new Box { F=x->x+n, N=n } }
        @EntryPoint() operation Main() : Int {
            let direct=Make(1);
            let data=Make(9);
            mutable factory=Make;
            for i in 0..0 { set factory=Make; }
            let deferred=factory(3);
            100*direct.F(0)+data.N+deferred.F(deferred.N)
        }
        "#,
        115,
    );
}

/// Consuming Make's closure in the library's Direct function must not change
/// the capture returned by a later indirect call from the user package.
#[test]
fn indirect_foreign_factory_preserves_returned_closure_captures() {
    let library = r#"
        namespace Lib {
            function Make(n : Int) : Int -> Int { x->x+n }
            function Direct() : Int { let f=Make(1); f(0) }
            export Make, Direct;
        }
    "#;
    let source = r#"
        import Lib.*;
        @EntryPoint() operation Main() : Int {
            let direct=Direct();
            mutable factory=Make;
            for i in 0..0 { set factory=Make; }
            let deferred=factory(3);
            100*direct+deferred(3)
        }
    "#;
    assert_eq!(
        crate::test_utils::eval_qsharp_original_with_library(library, source),
        Ok(qsc_eval::val::Value::Int(106)),
    );
    crate::test_utils::check_semantic_equivalence_with_library(library, source);
}

#[test]
fn higher_order_consumers_preserve_transitive_factory_captures() {
    for (source, expected) in test_cases::protected_owner_producer_cases() {
        check_callable_result(&source, expected);
    }
}

/// Specializing Consumer for Inc must keep the deferred Relay -> Make chain.
/// The direct closure contributes 100; the deferred closure computes `(3 + 1) + 3`.
#[test]
fn specialized_consumer_preserves_deferred_factory_capture_values() {
    check_callable_result(
        r#"
        function Make(n : Int) : Int -> Int { x->x+n }
        function Relay(n : Int) : Int -> Int { Make(n) }
        function Inc(n : Int) : Int { n+1 }
        function Consumer(f : Int -> Int, n : Int) : Int {
            mutable value=Relay(0);
            for i in 0..0 { set value=Relay(n); }
            value(f(n))
        }
        @EntryPoint() operation Main() : Int {
            let direct=Make(1);
            100*direct(0)+Consumer(Inc,3)
        }
        "#,
        107,
    );
}

/// Consumer must retain both factory evaluations, producing `relay:0` then
/// `relay:3` exactly once each, while the final closure uses the updated capture.
#[test]
fn higher_order_consumer_preserves_factory_message_order_and_capture_values() {
    check_callable_result(
        r#"
        function Make(n : Int) : Int -> Int { x->x+n }
        function Relay(n : Int) : Int -> Int { Message($"relay:{n}"); Make(n) }
        function Consumer(n : Int) : Int {
            mutable value=Relay(0);
            for i in 0..0 { set value=Relay(n); }
            value(n)
        }
        function Apply(f : Int -> Int, n : Int) : Int { f(n) }
        @EntryPoint() operation Main() : Int {
            let direct=Make(1);
            100*direct(0)+Apply(Consumer,3)
        }
        "#,
        106,
    );
}

/// The user specializes Apply for a foreign Consumer. Cleanup must preserve
/// Consumer's library-local Relay -> Make dependency and its updated capture.
#[test]
fn foreign_higher_order_consumer_preserves_transitive_factory_captures() {
    let library = r#"
        namespace Lib {
            function Make(n : Int) : Int -> Int { x->x+n }
            function Relay(n : Int) : Int -> Int { Make(n) }
            function Direct() : Int { let f=Make(1); f(0) }
            function Consumer(n : Int) : Int {
                mutable value=Relay(0);
                for i in 0..0 { set value=Relay(n); }
                value(n)
            }
            export Direct, Consumer;
        }
    "#;
    let source = r#"
        import Lib.*;
        function Apply(f : Int -> Int, n : Int) : Int { f(n) }
        @EntryPoint() operation Main() : Int { 100*Direct()+Apply(Consumer,3) }
    "#;
    assert_eq!(
        crate::test_utils::eval_qsharp_original_with_library(library, source),
        Ok(qsc_eval::val::Value::Int(106)),
    );
    crate::test_utils::check_semantic_equivalence_with_library(library, source);
}

#[test]
fn recursive_specializations_preserve_capture_environments() {
    for (source, expected) in test_cases::recursive_capture_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn controlled_recursive_specializations_preserve_captures() {
    for functor in [
        "Controlled",
        "Controlled Controlled",
        "Adjoint Controlled",
        "Controlled Adjoint",
        "Adjoint Controlled Controlled",
    ] {
        for (source, expected) in test_cases::recursive_capture_control_cases(functor) {
            check_callable_result(&source, expected);
        }
    }
}

#[test]
fn recursive_specialization_does_not_replay_capture_effects() {
    check_callable_result(
        r#"
        function Logged(n : Int) : Int { Message("capture"); n }
        function Add(offset : Int, n : Int) : Int { offset+n }
        function Repeat(f : Int -> Int, n : Int) : Int {
            if n==0 { f(0) } else { f(n)+Repeat(f,n-1) }
        }
        @EntryPoint() operation Main() : Int {
            let f=Add(Logged(7),_);
            Message("ready");
            Repeat(f,3)
        }
        "#,
        34,
    );
}

#[test]
fn effectful_short_circuit_guards_are_evaluated_once() {
    for (source, expected) in test_cases::effectful_short_circuit_guard_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn mutating_short_circuit_guards_preserve_selected_callable() {
    for (source, expected) in test_cases::mutating_short_circuit_guard_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn compound_short_circuit_guards_preserve_pre_store_selection() {
    for (source, expected) in test_cases::compound_short_circuit_guard_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn measured_short_circuit_guard_is_not_repeated_for_dispatch() {
    check_callable_result(test_cases::MEASURED_SHORT_CIRCUIT_GUARD, 6);
}

#[test]
fn effectful_short_circuit_guard_refreshes_each_loop_iteration() {
    check_callable_result(
        r#"
        function Inc(n : Int) : Int { n+1 }
        function Twice(n : Int) : Int { 2*n }
        function Guard(flag : Bool) : Bool { Message("guard"); flag }
        @EntryPoint() operation Main() : Int {
            mutable total=0;
            for i in 0..2 {
                mutable f=Inc;
                let unused=Guard(i % 2 == 0) and { set f=Twice; true };
                Message("ready");
                set total=10*total+f(3);
            }
            total
        }
        "#,
        646,
    );
}

#[test]
fn effectful_short_circuit_guard_stays_in_its_enclosing_operand() {
    for enabled in [false, true] {
        let source = indoc::formatdoc! {r#"
            function Inc(n : Int) : Int {{ n+1 }}
            function Twice(n : Int) : Int {{ 2*n }}
            function Guard() : Bool {{ Message("guard"); true }}
            @EntryPoint() operation Main() : Int {{
                mutable f=Inc;
                let unused={enabled} and {{
                    Guard() and {{ set f=Twice; true }}
                }};
                Message("ready");
                f(3)
            }}
        "#};
        check_callable_result(&source, if enabled { 6 } else { 4 });
    }
}

#[test]
fn nested_inline_struct_captures_preserve_evaluation_order() {
    for (source, expected) in test_cases::nested_inline_struct_capture_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn nested_inline_struct_field_failure_precedes_capture_failure() {
    let source = r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function Log(n : Int) : Int { Message("outer"); n }
        function Fail(label : String) : Int { fail label }
        function Add(n : Int, x : Int) : Int { n+x }
        function Read(p : Payload) : Int { 100*p.Head+10*p.F(2)+p.Tail }
        function Sum(n : Int, p : Payload) : Int { n+Read(p) }
        @EntryPoint() operation Main() : Int {
            Sum(
                Read(new Payload { Head=Fail("head"), F=Add(Fail("capture"),_), Tail=3 }),
                new Payload { Head=4, F=Add(Log(5),_), Tail=6 }
            )
        }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source)
        .expect_err("the inner head should fail before the capture and outer operands");
    assert!(error.contains("head"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn inline_struct_captures_preserve_operand_evaluation() {
    for (source, expected) in test_cases::inline_struct_capture_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn inline_struct_field_failure_precedes_capture_failure() {
    let source = r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function Fail(label : String) : Int { fail label }
        function Add(n : Int, x : Int) : Int { n+x }
        function Read(p : Payload) : Int { 100*p.Head+10*p.F(2)+p.Tail }
        @EntryPoint() operation Main() : Int {
            Read(new Payload { Tail=Fail("tail"), F=Add(Fail("capture"),_), Head=4 })
        }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source)
        .expect_err("the earlier field should fail before closure creation");
    assert!(error.contains("tail"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn inline_struct_capture_stays_in_its_call_branch() {
    for flag in [false, true] {
        let source = indoc::formatdoc! {r#"
            struct Payload {{ F : Int -> Int, N : Int }}
            function Log(label : String, n : Int) : Int {{ Message(label); n }}
            function Add(n : Int, x : Int) : Int {{ n+x }}
            function Read(p : Payload) : Int {{ p.F(p.N) }}
            function Choose(flag : Bool) : Int {{
                if flag {{
                    Read(new Payload {{ N=Log("field",4), F=Add(Log("capture",3),_) }})
                }} else {{ 2 }}
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
        check_callable_result(&source, if flag { 7 } else { 2 });
    }
}

#[test]
fn inline_struct_captures_remain_distinct_in_a_loop() {
    check_callable_result(
        r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function Log(label : String, n : Int) : Int { Message(label); n }
        function Add(n : Int, x : Int) : Int { n+x }
        function Read(p : Payload) : Int { 100*p.Head+10*p.F(2)+p.Tail }
        @EntryPoint() operation Main() : Int {
            mutable result=0;
            for i in 1..3 {
                set result+=Read(new Payload {
                    Head=Log("head",i), F=Add(Log("capture",i),_), Tail=Log("tail",i)
                });
            }
            result
        }
        "#,
        726,
    );
}

#[test]
fn inline_struct_functors_preserve_capture_evaluation() {
    for functor in [
        "Controlled",
        "Controlled Controlled",
        "Adjoint Controlled",
        "Controlled Adjoint",
        "Adjoint Controlled Controlled",
    ] {
        for (source, expected) in test_cases::direct_struct_capture_control_cases(functor) {
            check_callable_result(&source, expected);
        }
    }
}

#[test]
fn closure_creation_preserves_nested_prefix_effects() {
    check_callable_result(
        r#"
        function Add(n : Int, x : Int) : Int { n+x }
        function Capture() : Int { Message("capture"); 3 }
        @EntryPoint() operation Main() : Int {
            let f={Message("creating"); Add(Capture(),_)};
            Message("ready");
            f(2)
        }
        "#,
        5,
    );
}

#[test]
fn partial_application_evaluates_capture_before_following_statements() {
    check_callable_result(test_cases::PARTIAL_APPLICATION_CAPTURE_TIMING, 18);
}

#[test]
fn partial_application_snapshots_mutating_capture_once() {
    check_callable_result(test_cases::PARTIAL_APPLICATION_MUTATING_CAPTURE, 134);
}

#[test]
fn partial_application_indexed_capture_survives_assignment_rhs() {
    check_callable_result(test_cases::PARTIAL_APPLICATION_INDEXED_CAPTURE, 3300);
}

#[test]
fn partial_application_indexed_angle_survives_assignment_rhs() {
    check_callable_result(
        r#"
        @EntryPoint() operation Main() : Int {
            use q=Qubit();
            mutable angles=[0.0];
            let op=Rx(angles[0],_);
            set angles w/= 0 <- { op(q); 3.141592653589793 };
            op(q);
            if MResetZ(q) == One { 1 } else { 0 }
        }
        "#,
        0,
    );
}

#[test]
fn partial_application_evaluates_captures_once_in_source_order() {
    check_callable_result(
        r#"
        function Logged(label : String, value : Int) : Int { Message(label); value }
        function Sum(a : Int, b : Int, x : Int) : Int { a+b+x }
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        @EntryPoint() operation Main() : Int {
            let f=Sum(Logged("first",2),Logged("second",5),_);
            Message("ready");
            100*Apply(f,1)+Apply(f,2)
        }
        "#,
        809,
    );
}

#[test]
fn partial_application_capture_failure_precedes_following_statements() {
    let source = r#"
        function Capture() : Int { fail "capture" }
        function Add(offset : Int, x : Int) : Int { offset+x }
        @EntryPoint() operation Main() : Int {
            let f=Add(Capture(),_);
            Message("must not run");
            f(1)
        }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source)
        .expect_err("the captured operand should fail");
    assert!(error.contains("capture"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn partial_application_capture_stays_inside_its_branch() {
    for flag in [true, false] {
        let source = indoc::formatdoc! {r#"
            function Logged(value : Int) : Int {{ Message("capture"); value }}
            function Add(offset : Int, x : Int) : Int {{ offset+x }}
            function Choose(flag : Bool) : Int {{
                if flag {{
                    let f=Add(Logged(17),_);
                    Message("ready");
                    f(1)
                }} else {{ 7 }}
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
        check_callable_result(&source, if flag { 18 } else { 7 });
    }
}

#[test]
fn partial_application_capture_runs_once_per_loop_iteration() {
    check_callable_result(
        r#"
        function Logged(value : Int) : Int { Message($"capture:{value}"); value }
        function Add(offset : Int, x : Int) : Int { offset+x }
        @EntryPoint() operation Main() : Int {
            mutable total=0;
            for i in 1..3 {
                let f=Add(Logged(i),_);
                Message("ready");
                set total+=f(1);
            }
            total
        }
        "#,
        9,
    );
}

#[test]
fn partial_application_functors_preserve_capture_evaluation() {
    for functor in [
        "Controlled",
        "Controlled Controlled",
        "Adjoint Controlled",
        "Controlled Adjoint",
        "Adjoint Controlled Controlled",
    ] {
        for enabled in [false, true] {
            let args = if functor.matches("Controlled").count() == 2 {
                "[outer], ([inner], target)"
            } else {
                "[outer], target"
            };
            let source = indoc::formatdoc! {r#"
                function Angle() : Double {{ Message("capture"); 1.5707963267948966 }}
                @EntryPoint() operation Main() : Int {{
                    use outer=Qubit();
                    use inner=Qubit();
                    use target=Qubit();
                    if {enabled} {{ X(outer); X(inner); H(target); }}
                    let op=Ry(Angle(),_);
                    Message("ready");
                    {functor} op({args});
                    Reset(outer);
                    Reset(inner);
                    if MResetZ(target) == One {{ 1 }} else {{ 0 }}
                }}
            "#};
            check_callable_result(&source, i64::from(enabled && !functor.contains("Adjoint")));
        }
    }
}

#[test]
fn direct_struct_fields_preserve_declaration_order() {
    for (source, expected) in test_cases::direct_struct_field_order_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn direct_struct_fields_preserve_initializer_evaluation_order() {
    check_callable_result(
        r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function Inc(x : Int) : Int { x+1 }
        function Log(label : String, n : Int) : Int { Message(label); n }
        function Read(p : Payload) : Int { 100*p.Head+p.F(p.Tail) }
        @EntryPoint() operation Main() : Int {
            Read(new Payload { Tail=Log("tail",7), F=Inc, Head=Log("head",4) })
        }
        "#,
        408,
    );
}

#[test]
fn singleton_newtype_payload_preserves_specialized_fields() {
    for (source, expected) in test_cases::singleton_newtype_payload_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn controlled_singleton_newtype_payloads_preserve_values_and_effects() {
    for functor in [
        "Controlled",
        "Controlled Controlled",
        "Adjoint Controlled",
        "Controlled Adjoint",
        "Adjoint Controlled Controlled",
    ] {
        for (source, expected) in test_cases::controlled_newtype_cases(functor) {
            check_callable_result(&source, expected);
        }
    }
}

#[test]
fn struct_copy_factory_preserves_returned_fields() {
    for (source, expected) in test_cases::struct_copy_factory_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn nested_struct_copy_factory_preserves_returned_fields() {
    for (source, expected) in test_cases::nested_struct_copy_factory_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn direct_struct_copy_factory_preserves_returned_fields() {
    check_callable_result(test_cases::DIRECT_STRUCT_COPY_FACTORY, 408);
}

#[test]
fn type_constructor_arguments_preserve_underlying_fields() {
    for (source, expected) in test_cases::type_constructor_argument_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn direct_struct_copy_factory_runs_once_before_overrides() {
    check_callable_result(
        r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function Inc(x : Int) : Int { x+1 }
        function Original(n : Int) : Payload {
            Message("copy");
            new Payload { Head=n+1, F=Inc, Tail=2*n+1 }
        }
        function Override() : Int { Message("override"); 4 }
        function Read(payload : Payload) : Int { 100*payload.Head+payload.F(payload.Tail) }
        @EntryPoint() operation Main() : Int {
            Read(new Payload { ...Original(3), F=Inc, Head=Override() })
        }
        "#,
        408,
    );
}

#[test]
fn struct_copy_factory_failure_precedes_override_failure() {
    let source = r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function Inc(x : Int) : Int { x+1 }
        function Original() : Payload { fail "copy" }
        function Override() : Int { fail "override" }
        function Read(payload : Payload) : Int { 100*payload.Head+payload.F(payload.Tail) }
        @EntryPoint() operation Main() : Int {
            Read(new Payload { ...Original(), F=Inc, Head=Override() })
        }
    "#;
    let error =
        crate::test_utils::eval_qsharp_original(source).expect_err("the copy source should fail");
    assert!(error.contains("copy"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn stored_aggregate_arguments_preserve_creation_time_values() {
    for (source, expected) in test_cases::stored_aggregate_snapshot_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn conditional_stored_aggregate_arguments_preserve_creation_time_values() {
    for (source, expected) in test_cases::conditional_stored_aggregate_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn struct_branch_arguments_preserve_fields_and_captures() {
    for (source, expected) in test_cases::struct_branch_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn nested_struct_branch_arguments_preserve_fields_and_captures() {
    for (source, expected) in test_cases::nested_struct_branch_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn struct_branch_fields_preserve_evaluation_order() {
    for flag in [true, false] {
        let source = formatdoc! {r#"
            struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
            function Log(label : String, n : Int) : Int {{ Message(label); n }}
            function Make(n : Int) : Int -> Int {{ x -> x+n }}
            function Read(payload : Payload) : Int {{
                1000*payload.Head+100*payload.F(3)+payload.Tail
            }}
            function Choose(flag : Bool) : Int {{
                let selected = if flag {{ Make(1) }} else {{ Make(3) }};
                Read(new Payload {{ Tail=Log("tail",7), F=selected, Head=Log("head",2) }})
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
        check_callable_result(&source, if flag { 2407 } else { 2607 });
    }
}

#[test]
fn struct_branch_copy_source_is_evaluated_once_before_overrides() {
    for flag in [true, false] {
        let source = formatdoc! {r#"
            struct Payload {{ Head : Int, F : Int -> Int, Tail : Int }}
            function Log(label : String, n : Int) : Int {{ Message(label); n }}
            function Make(n : Int) : Int -> Int {{ x -> x+n }}
            function Original() : Payload {{
                Message("copy");
                new Payload {{ Head=2, F=Make(0), Tail=7 }}
            }}
            function Read(payload : Payload) : Int {{
                1000*payload.Head+100*payload.F(3)+payload.Tail
            }}
            function Choose(flag : Bool) : Int {{
                let selected = if flag {{ Make(1) }} else {{ Make(3) }};
                Read(new Payload {{ ...Original(), F=selected, Head=Log("override",2) }})
            }}
            @EntryPoint() operation Main() : Int {{ Choose({flag}) }}
        "#};
        check_callable_result(&source, if flag { 2407 } else { 2607 });
    }
}

#[test]
fn struct_branch_copy_snapshot_precedes_mutating_override() {
    let source = r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function Inc(x : Int) : Int { x+1 }
        function Twice(x : Int) : Int { 2*x }
        function Read(payload : Payload) : Int {
            1000*payload.Head+100*payload.F(3)+payload.Tail
        }
        function Choose(flag : Bool) : Int {
            let selected = if flag { Inc } else { Twice };
            mutable original = new Payload { Head=2, F=Inc, Tail=7 };
            Read(new Payload {
                ...original,
                F=selected,
                Tail={ set original w/= Head <- 99; 7 }
            })
        }
        @EntryPoint() operation Main() : Int { Choose(true) }
    "#;
    check_callable_result(source, 2407);
}

#[test]
fn nested_local_struct_argument_keeps_initializer_effects_once() {
    let source = r#"
        struct Payload { F : Int -> Int, N : Int }
        function Inc(x : Int) : Int { x+1 }
        function Log(n : Int) : Int { Message("field"); n }
        function Read(pair : (Int, Payload)) : Int {
            let (prefix, payload) = pair;
            prefix+payload.F(payload.N)
        }
        @EntryPoint() operation Main() : Int {
            let pair = (9, new Payload { F=Inc, N=Log(3) });
            Message("ready");
            Read(pair)
        }
    "#;
    check_callable_result(source, 13);
}

#[test]
fn struct_branch_preserves_first_field_failure() {
    let source = r#"
        struct Payload { Head : Int, F : Int -> Int, Tail : Int }
        function FailField(label : String) : Int { fail label }
        function Inc(x : Int) : Int { x+1 }
        function Twice(x : Int) : Int { 2*x }
        function Read(payload : Payload) : Int { payload.F(payload.Head)+payload.Tail }
        function Choose(flag : Bool) : Int {
            let selected = if flag { Inc } else { Twice };
            Read(new Payload { Tail=FailField("tail"), F=selected, Head=FailField("head") })
        }
        @EntryPoint() operation Main() : Int { Choose(true) }
    "#;
    let error = crate::test_utils::eval_qsharp_original(source)
        .expect_err("the first field initializer should fail");
    assert!(error.contains("tail"), "{error}");
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn single_controlled_branches_preserve_captures_and_effects() {
    for (source, expected) in test_cases::controlled_branch_cases("Controlled") {
        check_callable_result(&source, expected);
    }
}

#[test]
fn double_controlled_branches_preserve_captures_and_effects() {
    for (source, expected) in test_cases::controlled_branch_cases("Controlled Controlled") {
        check_callable_result(&source, expected);
    }
}

#[test]
fn adjoint_controlled_branches_preserve_captures_and_effects() {
    for functor in [
        "Adjoint Controlled",
        "Controlled Adjoint",
        "Adjoint Controlled Controlled",
    ] {
        for (source, expected) in test_cases::controlled_branch_cases(functor) {
            check_callable_result(&source, expected);
        }
    }
}

#[test]
fn conditional_branches_preserve_independent_nested_payloads() {
    for (source, expected) in test_cases::nested_branch_payload_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn forwarded_compound_capture_preserves_each_return_boundary() {
    for (source, expected) in test_cases::forwarded_capture_environment_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn forwarded_compound_captures_keep_each_field_and_call_occurrence() {
    check_callable_result(test_cases::FORWARDED_MULTI_FIELD_CAPTURES, 441_901);
}

#[test]
fn forwarded_compound_capture_preserves_mutable_caller_snapshot() {
    check_callable_result(
        r#"
        function Make(n : Int) : Int -> Int {
            let values = [n];
            x -> values[0]+x
        }
        function Forward(n : Int) : Int -> Int { Make(n+1) }
        @EntryPoint() operation Main() : Int {
            mutable n = 2;
            let f = Forward(n);
            set n = 99;
            f(0)
        }
        "#,
        3,
    );
}

#[test]
fn producer_local_computation_survives_capture_rebinding() {
    check_callable_result(
        r#"
        function Make(n : Int) : Int -> Int {
            let adjusted = n+1;
            let values = [adjusted];
            x -> values[0]+x
        }
        function Forward(n : Int) : Int -> Int { Make(2*n) }
        @EntryPoint() operation Main() : Int {
            let f = Forward(3);
            f(0)
        }
        "#,
        7,
    );
}

#[test]
fn forwarded_compound_capture_keeps_effectful_operand_at_creation() {
    check_callable_result(
        r#"
        operation Next(q : Qubit) : Int { X(q); 2 }
        function Make(n : Int) : Int -> Int {
            let values = [n];
            x -> values[0]+x
        }
        function Forward(n : Int) : Int -> Int { Make(n+1) }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let f = Forward(Next(q));
            let first = f(0);
            let second = f(1);
            let flipped = MResetZ(q) == One;
            if flipped { 10*first+second } else { 0 }
        }
        "#,
        34,
    );
}

#[test]
fn conditional_hof_calls_preserve_embedded_and_runtime_captures() {
    for (source, expected) in test_cases::conditional_capture_layout_cases() {
        check_callable_result(&source, expected);
    }
}

/// Regression for controlled dispatch of a *capturing* closure passed to a
/// higher-order operation whose callable parameter is **not** the first
/// argument. The HOF applies `Controlled op(ctls, q)`, so rewrite must nest the
/// closure's captures inside the base input tuple beneath the control register
/// (`(ctls, (capture0, capture1, q))`) rather than appending them as trailing
/// top-level siblings of `(ctls, q)`. A mis-placed capture would either crash
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

/// Regression for higher-order argument cleanup. `GetOp(q)` performs `X(q)`
/// before returning the named callable `X`; specializing `ApplyOp(op, q)`
/// consumes `op`, but cleanup must retain the initializer's effect.
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
/// factory is a classical `function`, but that does not make its evaluation
/// safe to delete: its body can still fail. Cleanup must keep the
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

/// Generates syntactically valid Q# programs exercising defunctionalization's
/// key code paths: lambda arguments, partial application, and direct callable
/// references passed to higher-order functions.
#[cfg(feature = "slow-proptest-tests")]
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

/// Generates multi-capture closures whose two varying captures have distinct
/// values and are used in non-commutative operations, exercising capture order.
/// The three-capture case also includes a constant capture of `1`.
#[cfg(feature = "slow-proptest-tests")]
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

#[cfg(feature = "slow-proptest-tests")]
proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn proptest_defunctionalize_preserves_semantics(source in defunc_pattern_strategy()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[cfg(feature = "slow-proptest-tests")]
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

/// A conditional callable is bound from a mutable guard that is never reassigned.
/// Reading the guard at the apply site therefore selects the same callable as
/// at the binding site (`X`), preserving the result and quantum effect trace.
///
/// The reassigned-guard counterpart checks the raw pass's `DynamicCallable`
/// diagnostic in
/// `defunctionalize::tests::analysis::reaching_def_conditional_callable_reassigned_guard_dynamic`.
/// The full pipeline defers that diagnostic rather than rejecting it here.
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

#[test]
fn embedded_callable_identity_preserves_execution_results() {
    for (source, expected) in test_cases::embedded_callable_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn independently_created_equivalent_callable_captures_preserve_results() {
    let source = test_cases::embedded_callable_source("Inc", "Inc", "Apply(a, 3)*100+Apply(b, 3)");
    check_callable_result(&source, 505);
}

#[test]
fn factory_returned_partial_applications_preserve_operand_values_per_occurrence() {
    for call in [
        "first(0)*10000+second(0)",
        "Invoke(first, 0)*10000+Invoke(second, 0)",
    ] {
        let source = formatdoc! {r#"
            function Identity(x : Int) : Int {{ x }}
            function Compose(
                f : Int -> Int, a : Int[], b : Int[], c : Int[], d : Int[], x : Int
            ) : Int {{
                f(x)+1000*a[0]+100*b[0]+10*c[0]+d[0]
            }}
            function Make(
                f : Int -> Int, a : Int[], b : Int[], c : Int[], d : Int[]
            ) : Int -> Int {{
                Compose(f, a, b, c, d, _)
            }}
            function Invoke(f : Int -> Int, x : Int) : Int {{ f(x) }}
            @EntryPoint() operation Main() : Int {{
                let first = Make(Identity, [1], [2], [3], [4]);
                let second = Make(Identity, [5], [6], [7], [8]);
                {call}
            }}
        "#};
        check_callable_result(&source, 12_345_678);
    }
}

#[test]
fn foreign_nested_callable_captures_return_613() {
    let library = r#"
        namespace Lib {
            function Add(n : Int) : Int -> Int { x -> x+n }
            function Wrap(f : Int -> Int, n : Int) : Int -> Int { x -> f(x)+n }
            function Apply<'T,'U>(f : 'T -> 'U, x : 'T) : 'U { f(x) }
            export Add, Wrap, Apply;
        }
    "#;
    let source = r#"
        @EntryPoint() operation Main() : Int {
            let first = Lib.Wrap(Lib.Add(2), 3);
            let second = Lib.Wrap(Lib.Add(5), 7);
            Lib.Apply(first, 1)*100 + Lib.Apply(second, 1)
        }
    "#;
    assert_eq!(
        crate::test_utils::eval_qsharp_original_with_library(library, source),
        Ok(qsc_eval::val::Value::Int(613))
    );
    crate::test_utils::check_semantic_equivalence_with_library(library, source);
}

#[test]
fn flat_closure_preserves_direct_and_forwarded_results() {
    crate::test_utils::check_semantic_equivalence(
        r#"
        function Flat(offset : Int, scale : Int) : Int -> Int {
            value -> (value+offset)*scale
        }
        function Invoke(callable : Int -> Int, value : Int) : Int { callable(value) }
        @EntryPoint() operation Main() : (Int, Int) {
            let first = Flat(3, 2);
            (first(1), Invoke(first, 3))
        }
        "#,
    );
}

#[test]
fn nested_callable_capture_slots_can_be_removed_or_expanded() {
    for (first, second, argument, expected) in [
        ("Inc", "Twice", 3, 406),
        ("Make(2, 3)", "Inc", 3, 1104),
        ("Inc", "Make(1, 7)", 3, 422),
        ("Make(2, 3)", "Make(5, 7)", 1, 512),
    ] {
        let source = formatdoc! {r#"
            function Inc(x : Int) : Int {{ x+1 }}
            function Twice(x : Int) : Int {{ 2*x }}
            function Make(offset : Int, scale : Int) : Int -> Int {{
                x -> scale*x+offset
            }}
            function Combine(f : Int -> Int, g : Int -> Int, x : Int) : Int {{
                let combined = y -> 100*f(y)+g(y);
                combined(x)
            }}
            @EntryPoint() operation Main() : Int {{
                Combine({first}, {second}, {argument})
            }}
        "#};
        check_callable_result(&source, expected);
    }
}

#[test]
fn captured_lambda_functor_applications_preserve_quantum_effects() {
    for call in [
        "Combine(first, second, target)",
        "Adjoint Combine(first, second, target)",
        "Controlled Combine([control], (first, second, target))",
        "Controlled Adjoint Combine([control], (first, second, target))",
    ] {
        let source = formatdoc! {r#"
            operation Rotate(a : Double, b : Double, q : Qubit) : Unit is Adj + Ctl {{
                Rx(a, q);
                Rz(b, q);
            }}
            operation Combine(
                first : Qubit => Unit is Adj + Ctl,
                second : Qubit => Unit is Adj + Ctl,
                q : Qubit
            ) : Unit is Adj + Ctl {{
                let combined : Qubit => Unit is Adj + Ctl =
                    target => {{ first(target); second(target); }};
                combined(q);
            }}
            @EntryPoint() operation Main() : Int {{
                use control = Qubit();
                use target = Qubit();
                X(control);
                let first = Rotate(0.4, 0.2, _);
                let second = Rotate(0.7, 0.3, _);
                {call};
                Reset(target);
                Reset(control);
                0
            }}
        "#};
        check_callable_result(&source, 0);
    }
}

#[test]
fn embedded_callable_identity_distinguishes_adjoint_application() {
    check_callable_result(
        r#"
        function Wrap(op : Qubit => Unit is Adj + Ctl) : Qubit => Unit is Adj + Ctl {
            q => { op(q); Z(q); }
        }
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        @EntryPoint() operation Main() : Int {
            use a = Qubit();
            use b = Qubit();
            let forward = Wrap(S);
            let inverse = Wrap(Adjoint S);
            H(a);
            Apply(forward, a);
            Apply(forward, a);
            H(a);
            H(b);
            Apply(forward, b);
            Apply(inverse, b);
            H(b);
            let left = MResetZ(a);
            let right = MResetZ(b);
            if left == One and right == Zero { 10 } else { 0 }
        }
        "#,
        10,
    );
}

#[test]
fn branch_split_in_specialized_clone_preserves_both_capture_environments() {
    for (prepare, expected) in [("", 16), ("X(flag);", 13)] {
        for call in ["selected(10)", "Apply(selected, 10)"] {
            let source = formatdoc! {r#"
                function Inc(x : Int) : Int {{ x+1 }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Choose(seed : Int -> Int, flag : Bool) : Int {{
                    let first = seed(2);
                    let second = seed(5);
                    let selected = if flag {{
                        x -> x+first
                    }} else {{
                        x -> x+second
                    }};
                    {call}
                }}
                @EntryPoint() operation Main() : Int {{
                    use flag = Qubit();
                    {prepare}
                    Choose(Inc, MResetZ(flag) == One)
                }}
            "#};
            check_callable_result(&source, expected);
        }
    }
}

#[test]
fn returned_compound_capture_substitutions_survive_forwarding() {
    for (source, expected) in test_cases::compound_capture_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn forwarded_nested_tuple_preserves_callable_and_scalar_fields() {
    for (choose, expected) in [("true", 29), ("false", 39)] {
        let source = formatdoc! {r#"
            function Make(offset : Int) : Int -> Int {{ x -> x+offset }}
            function Read(pair : (Int, (Int -> Int, Int)), choose : Bool) : Int {{
                if choose {{
                    let (prefix, (f, x)) = pair;
                    prefix+f(x)
                }} else {{
                    let (prefix, (f, x)) = pair;
                    10+prefix+f(x)
                }}
            }}
            function Forward(pair : (Int, (Int -> Int, Int)), choose : Bool) : Int {{
                Read(pair, choose)
            }}
            @EntryPoint() operation Main() : Int {{
                Forward((9, (Make(17), 3)), {choose})
            }}
        "#};
        check_callable_result(&source, expected);
    }
}

#[test]
fn forwarded_struct_preserves_callable_and_scalar_fields() {
    check_callable_result(
        r#"
        struct Pair { Apply : Int -> Int, Value : Int }
        function Make(offset : Int) : Int -> Int { x -> x+offset }
        function Read(pair : Pair) : Int { pair.Apply(pair.Value) }
        function Forward(pair : Pair) : Int { Read(pair) }
        @EntryPoint() operation Main() : Int {
            Forward(new Pair { Apply = Make(17), Value = 3 })
        }
        "#,
        20,
    );
}

#[test]
fn callable_array_with_no_scalar_sibling_preserves_member_captures() {
    for (members, expected) in [("[]", 0), ("[Make(2)]", 3), ("[Make(2), Make(5)]", 36)] {
        let source = formatdoc! {r#"
            function Make(offset : Int) : Int -> Int {{ x -> x+offset }}
            function Read(functions : (Int -> Int)[]) : Int {{
                mutable result = 0;
                for f in functions {{
                    set result = 10*result+f(1);
                }}
                result
            }}
            @EntryPoint() operation Main() : Int {{ Read({members}) }}
        "#};
        check_callable_result(&source, expected);
    }
}

#[test]
fn foreign_forwarded_global_callable_retains_its_identity() {
    let library = r#"
        namespace Lib {
            function Forward<'T>(f : 'T -> 'T) : 'T -> 'T { f }
            export Forward;
        }
    "#;
    let source = r#"
        function Inc(x : Int) : Int { x+1 }
        function Twice(x : Int) : Int { 2*x }
        @EntryPoint() operation Main() : Int {
            let a = Lib.Forward(Inc);
            let b = Lib.Forward(Twice);
            a(3)*100+b(3)
        }
    "#;
    assert_eq!(
        crate::test_utils::eval_qsharp_original_with_library(library, source),
        Ok(qsc_eval::val::Value::Int(406))
    );
    crate::test_utils::check_semantic_equivalence_with_library(library, source);
}

#[test]
fn returned_record_copy_update_rebinds_factory_parameters() {
    check_callable_result(test_cases::RECORD_COPY_UPDATE, 1919);
}

#[test]
fn compound_capture_preserves_repeat_count_and_range_bounds() {
    check_callable_result(test_cases::COMPOUND_REPEAT_AND_RANGE, 31721);
}

#[test]
fn compound_capture_factory_preserves_array_repeat_failure() {
    let source = r#"
        function Make(count : Int) : Int -> Int {
            let values = [17, size = count];
            x -> values[0]+x
        }
        @EntryPoint() operation Main() : Int {
            let f = Make(-1);
            f(1)
        }
    "#;
    assert!(crate::test_utils::eval_qsharp_original(source).is_err());
    crate::test_utils::check_semantic_equivalence(source);
}

#[test]
fn entry_expression_captures_keep_their_own_scope() {
    let source = "namespace Test { function Apply(f : Int -> Int, x : Int) : Int { f(x) } }";
    let entry = "{ let offset = 17; let f = x -> x+offset; Test.Apply(f, 3) }";
    let (mut store, package) = crate::test_utils::compile_to_fir_with_entry(source, entry);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, package),
        Ok(qsc_eval::val::Value::Int(20))
    );
    crate::test_utils::assert_full_pipeline_succeeds("entry capture", &mut store, package);
    assert_eq!(
        crate::test_utils::try_eval_fir_entry(&store, package),
        Ok(qsc_eval::val::Value::Int(20))
    );
}

#[test]
fn effectful_factory_operand_is_evaluated_once_before_repeated_calls() {
    check_callable_result(
        r#"
        operation Next(q : Qubit) : Int { X(q); 1 }
        function Make(values : Int[]) : Int -> Int { x -> values[0]+x }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let f = Make([Next(q)]);
            let first = f(1);
            let second = f(2);
            let flipped = MResetZ(q) == One;
            if flipped { 100*first+second } else { 0 }
        }
        "#,
        203,
    );
}

#[test]
fn direct_factory_closure_preserves_nested_control_layers() {
    for call in [
        "Controlled Controlled op([outer], ([inner], target))",
        "Controlled Controlled Adjoint op([outer], ([inner], target))",
    ] {
        let source = formatdoc! {r#"
            operation Rotate(angle : Double, q : Qubit) : Unit is Adj + Ctl {{
                Rx(angle, q);
            }}
            function Make(angle : Double) : Qubit => Unit is Adj + Ctl {{
                Rotate(angle, _)
            }}
            @EntryPoint() operation Main() : Int {{
                use outer = Qubit();
                use inner = Qubit();
                use target = Qubit();
                X(outer);
                X(inner);
                let op = Make(3.141592653589793);
                {call};
                Reset(outer);
                Reset(inner);
                if MResetZ(target) == One {{ 1 }} else {{ 0 }}
            }}
        "#};
        check_callable_result(&source, 1);
    }
}

#[test]
fn forwarded_newtype_preserves_callable_and_payload() {
    check_callable_result(
        r#"
        newtype Pair = (Int -> Int, Int);
        function Make(offset : Int) : Int -> Int { x -> x+offset }
        function Read(pair : Pair) : Int {
            let (f, value) = pair!;
            f(value)
        }
        function Forward(pair : Pair) : Int { Read(pair) }
        @EntryPoint() operation Main() : Int { Forward(Pair(Make(17), 3)) }
        "#,
        20,
    );
}

#[test]
fn nested_factory_closure_rebinds_captures_across_two_return_boundaries() {
    check_callable_result(
        r#"
        function Make(offset : Int) : Int -> Int { x -> x+offset }
        function Wrap(f : Int -> Int) : Int -> Int { x -> f(x)+1 }
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        @EntryPoint() operation Main() : Int {
            let first = Wrap(Wrap(Make(2)));
            let second = Wrap(Wrap(Make(5)));
            100*Apply(first, 1)+Apply(second, 1)
        }
        "#,
        508,
    );
}

#[test]
fn closure_dispatch_preserves_tuple_argument_grouping() {
    for call in ["f(2, 3)", "Apply(f, (2, 3))"] {
        let source = formatdoc! {r#"
            function Make(offset : Int) : (Int, Int) -> Int {{
                (a, b) -> 100*offset+10*a+b
            }}
            function Apply(f : (Int, Int) -> Int, pair : (Int, Int)) : Int {{ f(pair) }}
            @EntryPoint() operation Main() : Int {{
                let f = Make(7);
                {call}
            }}
        "#};
        check_callable_result(&source, 723);
    }
}

#[test]
fn factory_capture_keeps_scalar_value_before_caller_mutation() {
    for (source, expected) in test_cases::mutable_scalar_capture_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn factory_capture_keeps_array_value_before_caller_copy_update() {
    check_callable_result(test_cases::MUTABLE_ARRAY_CAPTURE, 18);
}

#[test]
fn loop_factory_occurrences_keep_their_iteration_values() {
    check_callable_result(test_cases::LOOP_FACTORY_CAPTURES, 234);
}

#[test]
fn callable_factory_tuple_output_preserves_stronger_functors() {
    check_callable_result(
        r#"
        function Make() : ((Qubit => Unit is Adj + Ctl), Int) { (X, 7) }
        operation Consume(factory : Unit -> ((Qubit => Unit), Int), q : Qubit) : Int {
            let (op, tag) = factory();
            op(q);
            tag
        }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let tag = Consume(Make, q);
            if MResetZ(q) == One { tag } else { 0 }
        }
        "#,
        7,
    );
}

#[test]
fn nested_callable_factory_output_preserves_destructured_types() {
    check_callable_result(
        r#"
        function Make() : (Int, ((Qubit => Unit is Adj + Ctl)[], Int)) {
            (3, ([X, Z], 7))
        }
        operation Consume(factory : Unit -> (Int, ((Qubit => Unit)[], Int)), q : Qubit) : Int {
            let (prefix, (ops, suffix)) = factory();
            for op in ops { op(q); }
            10*prefix+suffix
        }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let tag = Consume(Make, q);
            if MResetZ(q) == One { tag } else { 0 }
        }
        "#,
        37,
    );
}

#[test]
fn copy_updated_capture_keeps_operation_effects_at_factory_time() {
    check_callable_result(
        r#"
        operation Next(q : Qubit) : Int { X(q); 7 }
        operation Make(q : Qubit) : Int -> Int {
            let values = [1, 2] w/ 1 <- Next(q);
            x -> 100*values[0]+10*values[1]+x
        }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let f = Make(q);
            let first = f(3);
            let second = f(4);
            if MResetZ(q) == One { 1000*first+second } else { 0 }
        }
        "#,
        173_174,
    );
}

#[test]
fn callable_factory_early_returns_keep_distinct_environments() {
    for (source, expected) in test_cases::early_return_capture_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn guarded_fallible_factory_is_not_evaluated_on_untaken_branch() {
    let source = r#"
        function Make(count : Int) : Int -> Int {
            let values = [17, size = count];
            x -> values[0]+x
        }
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        @EntryPoint() operation Main() : Int {
            use flag = Qubit();
            if MResetZ(flag) == One {
                Apply(Make(-1), 1)
            } else {
                Apply(Make(1), 1)
            }
        }
    "#;
    check_callable_result(source, 18);
}

#[test]
fn caller_shadowing_does_not_rebind_forwarded_closure() {
    check_callable_result(test_cases::SHADOWED_CAPTURE, 18116);
}

#[test]
fn immutable_capture_snapshot_preserves_value_after_caller_mutation() {
    check_callable_result(test_cases::IMMUTABLE_CAPTURE_SNAPSHOT, 18);
}

#[test]
fn branch_dispatch_does_not_replay_a_mutated_capture_operand() {
    for (prepare, expected) in [("", 6), ("X(flag);", 18)] {
        let source = formatdoc! {r#"
            function Make(offset : Int) : Int -> Int {{ x -> x+offset }}
            function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
            @EntryPoint() operation Main() : Int {{
                use flag = Qubit();
                {prepare}
                mutable offset = 17;
                let selected = if MResetZ(flag) == One {{ Make(offset) }} else {{ Make(5) }};
                set offset = 99;
                Apply(selected, 1)
            }}
        "#};
        check_callable_result(&source, expected);
    }
}

#[test]
fn foreign_callable_factory_output_retains_nested_functor_types() {
    let library = r#"
        namespace Lib {
            function Make() : (Int, ((Qubit => Unit is Adj + Ctl), Int)) {
                (3, (X, 7))
            }
            export Make;
        }
    "#;
    let source = r#"
        operation Consume(factory : Unit -> (Int, ((Qubit => Unit), Int)), q : Qubit) : Int {
            let (prefix, (op, suffix)) = factory();
            op(q);
            10*prefix+suffix
        }
        @EntryPoint() operation Main() : Int {
            use q = Qubit();
            let tag = Consume(Lib.Make, q);
            if MResetZ(q) == One { tag } else { 0 }
        }
    "#;
    assert_eq!(
        crate::test_utils::eval_qsharp_original_with_library(library, source),
        Ok(qsc_eval::val::Value::Int(37))
    );
    crate::test_utils::check_semantic_equivalence_with_library(library, source);
}

#[test]
fn invalid_copy_updated_capture_preserves_factory_failure() {
    let source = r#"
        function Make(index : Int) : Int -> Int {
            let values = [17] w/ index <- 99;
            x -> values[0]+x
        }
        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        @EntryPoint() operation Main() : Int { Apply(Make(2), 1) }
    "#;
    for (label, (store, package)) in [
        ("original", crate::test_utils::compile_to_fir(source)),
        (
            "transformed",
            crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Full),
        ),
    ] {
        let error = crate::test_utils::try_eval_fir_entry(&store, package)
            .expect_err("the factory's invalid copy-update must fail");
        // Rematerializing the index can change its source span, not its failure.
        assert!(error.starts_with("IndexOutOfRange(2,"), "{label}: {error}");
    }
}

#[test]
fn mixed_branch_dispatch_preserves_constant_sibling_capture_and_effects() {
    for (prepare, expected) in [("", 603), ("X(flag);", 403)] {
        let source = formatdoc! {r#"
            function Inc(x : Int) : Int {{ x+1 }}
            function Twice(x : Int) : Int {{ 2*x }}
            function Make(offset : Int) : Int -> Int {{
                Message("make");
                x -> x+offset
            }}
            function Both(first : Int -> Int, second : Int -> Int, x : Int) : Int {{
                100*first(x)+second(x)
            }}
            @EntryPoint() operation Main() : Int {{
                use flag = Qubit();
                {prepare}
                let chosen = if MResetZ(flag) == One {{ Inc }} else {{ Twice }};
                let captured = Make(0);
                Message("ready");
                Both(chosen, captured, 3)
            }}
        "#};
        check_callable_result(&source, expected);
    }
}

#[test]
fn capture_free_callable_array_members_keep_distinct_embedded_identities() {
    for (source, expected) in test_cases::callable_array_identity_cases() {
        check_callable_result(&source, expected);
    }
}

#[test]
fn closure_retained_in_returned_callable_array_remains_executable() {
    check_callable_result(test_cases::RETURNED_CALLABLE_ARRAY, 407);
}

fn check_callable_result(source: &str, expected: i64) {
    crate::test_utils::check_semantic_equivalence_with_expected(
        source,
        qsc_eval::val::Value::Int(expected),
    );
}

fn mixed_static_dispatch_source(prior: &str, reset: &str, run: &str) -> String {
    formatdoc! {r#"
        operation Run(first : Qubit => Unit is Ctl, second : Qubit => Unit is Ctl, target : Qubit) : Unit is Ctl {{
            first(target); second(target);
        }}
        @EntryPoint() operation Main() : Unit {{
            use q = Qubit(); use c = Qubit();
            let angle = 0.25; let second = target => Rz(angle, target);
            {prior}
            let ops = [H, X];
            for i in 0..1 {{ let first = ops[i]; {run} }}
            {reset}
        }}
    "#}
}

#[test]
fn mixed_static_dispatch_preserves_original_nonzero_release_failure() {
    let source = mixed_static_dispatch_source(
        "Controlled Run([c], (H, second, q));",
        "",
        "Run(first, second, q);",
    );
    let (store, package) = crate::test_utils::compile_to_fir(&source);
    let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package);
    assert!(
        result
            .expect_err("original live qubit must not be silently reset")
            .starts_with("ReleasedQubitNotZero(0, PackageSpan"),
        "{source}"
    );
    crate::test_utils::check_semantic_equivalence(&source);
}

#[test]
fn mixed_static_dispatch_reset_preserves_trace_and_controlled_input_shapes() {
    use qsc_fir::{
        fir::{ExprKind, ItemKind, PackageLookup, Res},
        ty::Ty,
    };
    for (prior, call) in [
        (
            "Controlled Run([c], (H, second, q));",
            "Run(first, second, q);",
        ),
        ("", "Controlled Run([c], (first, second, q));"),
        (
            "",
            "Controlled Controlled Run([], ([c], (first, second, q)));",
        ),
        ("", "Run(first, second, q);"),
        ("Run(H, second, q);", "Run(first, second, q);"),
        (
            "",
            "set angle = 0.25; Run(first, { let saved = angle; target => Rz(saved, target) }, { set angle = 0.75; q });",
        ),
        (
            "",
            "Run({ let saved = Capture(q); target => Rz(saved, target) }, first, q);",
        ),
    ] {
        let source = format!(
            "operation Capture(q : Qubit) : Double {{ Z(q); 0.25 }}\n{}",
            mixed_static_dispatch_source(prior, "ResetAll([q, c]);", call)
        );
        let source = if call.contains("set angle") {
            source.replace("let angle = 0.25; let second = target => Rz(angle, target);",
                "mutable angle = 0.25; let initial = angle; let second = target => Rz(initial, target);")
        } else {
            source
        };
        let (store, package) = crate::test_utils::compile_to_fir(&source);
        let (result, trace) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package);
        assert!(result.is_ok(), "{prior} {call}: {result:?}");
        if prior.starts_with("Controlled") {
            expect_test::expect![[r#"[QubitAllocate(0), QubitAllocate(1), Gate { name: "S", is_adjoint: false, targets: [0], controls: [], theta: None }, Gate { name: "H", is_adjoint: false, targets: [0], controls: [], theta: None }, Gate { name: "T", is_adjoint: false, targets: [0], controls: [], theta: None }, Gate { name: "X", is_adjoint: false, targets: [0], controls: [1], theta: None }, Gate { name: "T", is_adjoint: true, targets: [0], controls: [], theta: None }, Gate { name: "H", is_adjoint: false, targets: [0], controls: [], theta: None }, Gate { name: "S", is_adjoint: true, targets: [0], controls: [], theta: None }, Gate { name: "Rz", is_adjoint: false, targets: [0], controls: [], theta: Some(0.125) }, Gate { name: "X", is_adjoint: false, targets: [0], controls: [1], theta: None }, Gate { name: "Rz", is_adjoint: false, targets: [0], controls: [], theta: Some(-0.125) }, Gate { name: "X", is_adjoint: false, targets: [0], controls: [1], theta: None }, Gate { name: "H", is_adjoint: false, targets: [0], controls: [], theta: None }, Gate { name: "Rz", is_adjoint: false, targets: [0], controls: [], theta: Some(0.25) }, Gate { name: "X", is_adjoint: false, targets: [0], controls: [], theta: None }, Gate { name: "Rz", is_adjoint: false, targets: [0], controls: [], theta: Some(0.25) }, Reset(0), Reset(1), QubitRelease(1), QubitRelease(0)]"#]]
                .assert_eq(&format!("{trace:?}"));
        }
        crate::test_utils::check_semantic_equivalence(&source);
        let (store, package) =
            crate::test_utils::compile_and_run_pipeline_to(&source, crate::PipelineStage::Defunc);
        let package = store.get(package);
        let mut checked = 0;
        for expr in package.exprs.values() {
            let ExprKind::Call(callee, args) = expr.kind else {
                continue;
            };
            let (base, _) = super::peel_body_functors(package, callee);
            let ExprKind::Var(Res::Item(item), _) = package.get_expr(base).kind else {
                continue;
            };
            let owner = store.get(item.package);
            let ItemKind::Callable(decl) = &owner.get_item(item.item).kind else {
                continue;
            };
            if !decl.name.name.starts_with("Run") || !decl.name.name.contains('{') {
                continue;
            }
            let Ty::Arrow(base_arrow) = &package.get_expr(base).ty else {
                panic!("base callable type")
            };
            assert_eq!(
                base_arrow.input.as_ref(),
                &owner.get_pat(decl.input).ty,
                "callee reference must agree with its actual package-owned declaration"
            );
            let Ty::Arrow(arrow) = &package.get_expr(callee).ty else {
                panic!("callable input")
            };
            assert_eq!(
                arrow.input.as_ref(),
                &package.get_expr(args).ty,
                "{prior} {call}: specialized dispatch call must match its input"
            );
            checked += 1;
        }
        assert!(
            checked > 0,
            "source-generated specialized calls must be checked"
        );
    }
}

#[test]
fn composite_callee_before_argument_write_returns_three() {
    for call in [
        "({ f })({ set f = Times2; 2 })",
        "(if true { f } else { Times2 })({ set f = Times2; 2 })",
        "Identity(f)({ set f = Times2; 2 })",
        "(new Holder { Op = f }).Op({ set f = Times2; 2 })",
        "Apply(if true { f } else { Times2 }, { set f = Times2; 2 })",
        "[f][0]({ set f = Times2; 2 })",
    ] {
        check_callable_result(
            &formatdoc! {r#"
                namespace Test {{
                    struct Holder {{ Op : Int -> Int }}
                    function Add1(x : Int) : Int {{ x + 1 }}
                    function Times2(x : Int) : Int {{ x * 2 }}
                    function Identity(f : Int -> Int) : Int -> Int {{ f }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    @EntryPoint()
                    operation Main() : Int {{
                        mutable f = Add1;
                        {call}
                    }}
                }}
            "#},
            3,
        );
    }
}

#[test]
fn self_mutating_branch_guard_preserves_taken_callable_returning_four() {
    for branch in [
        "if flag { set flag = false; set f = Times2; }",
        "let unused = flag and { set flag = false; set f = Times2; true };",
        "let unused = not flag or { set flag = false; set f = Times2; false };",
    ] {
        check_callable_result(
            &formatdoc! {r#"
                namespace Test {{
                    function Add1(x : Int) : Int {{ x + 1 }}
                    function Times2(x : Int) : Int {{ x * 2 }}
                    @EntryPoint()
                    operation Main() : Int {{
                        mutable flag = true;
                        mutable f = Add1;
                        {branch}
                        f(2)
                    }}
                }}
            "#},
            4,
        );
    }
}

#[test]
fn repeated_branch_selection_refreshes_guard_and_skips_inactive_effects() {
    for branch in [
        "if flag { set flag = false; set f = Times2; }",
        "let unused = flag and { set flag = false; set f = Times2; true };",
    ] {
        check_callable_result(
            &formatdoc! {r#"
                namespace Test {{
                    function Add1(x : Int) : Int {{ x + 1 }}
                    function Times2(x : Int) : Int {{ x * 2 }}
                    @EntryPoint()
                    operation Main() : Int {{
                        mutable flag = true;
                        mutable result = 0;
                        for index in 0..1 {{
                            mutable f = Add1;
                            {branch}
                            set result = result * 10 + f(2);
                        }}
                        result
                    }}
                }}
            "#},
            43,
        );
    }
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "Keep the callee evaluation-order matrix in its existing source regression."
)]
fn direct_callee_effects_run_once_before_arguments_and_conditional_dispatch() {
    check_callable_result(
        indoc::indoc! {r#"
            @EntryPoint()
            operation Main() : Int {
                mutable count = 0;
                let answer = ({
                    set count += 1;
                    let n = { set count += 2; 1 };
                    x -> x + n
                })(2);
                count * 100 + answer
            }
        "#},
        303,
    );
    check_callable_result(
        indoc::indoc! {r#"
            @EntryPoint()
            operation Main() : Int {
                mutable count = 0;
                let n = 1;
                let answer = ({ set count += 1; x -> x + n })(2);
                count * 100 + answer
            }
        "#},
        103,
    );
    check_callable_result(
        indoc::indoc! {r#"
            function Add1(x : Int) : Int { x + 1 }
            @EntryPoint()
            operation Main() : Int {
                mutable order = 0;
                let answer = ({ set order = order * 10 + 1; Add1 })(
                    { set order = order * 10 + 2; 2 });
                answer * 100 + order
            }
        "#},
        312,
    );
    check_callable_result(
        indoc::indoc! {r#"
            function Add1(x : Int) : Int { x + 1 }
            @EntryPoint()
            operation Main() : Int {
                mutable count = 0;
                let answer = ({ set count += 1; Add1 })(2);
                count * 100 + answer
            }
        "#},
        103,
    );
    for flag in [true, false] {
        for (label, callee, expected) in [
            (
                "prefix",
                "{ set count += 1; flag ? Add1 | Times2 }",
                if flag { 103 } else { 104 },
            ),
            (
                "nested prefix",
                "{ { set count += 1; flag ? Add1 | Times2 } }",
                if flag { 103 } else { 104 },
            ),
            (
                "selected prefix",
                "if flag { set count += 1; Add1 } else { set count += 2; Times2 }",
                if flag { 103 } else { 204 },
            ),
            (
                "inactive failure",
                "if flag { set count += 1; Add1 } else { fail \"inactive callee\"; Times2 }",
                103,
            ),
            (
                "inactive then failure",
                "if flag { fail \"inactive callee\"; Add1 } else { set count += 1; Times2 }",
                104,
            ),
            (
                "nested selection",
                "{ set count += 1; if flag { if count == 1 { Add1 } else { fail \"inactive nested callee\"; Times2 } } else { Times2 } }",
                if flag { 103 } else { 104 },
            ),
            (
                "higher order control",
                "{ set count += 1; flag ? Add1 | Times2 }",
                if flag { 103 } else { 104 },
            ),
        ] {
            if (label == "inactive failure" && !flag) || (label == "inactive then failure" && flag)
            {
                continue;
            }
            let call = if label == "higher order control" {
                format!("Apply({callee}, 2)")
            } else {
                format!("({callee})(2)")
            };
            let source = formatdoc! {r#"
                function Add1(x : Int) : Int {{ x + 1 }}
                function Times2(x : Int) : Int {{ 2 * x }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                @EntryPoint() operation Main() : Int {{
                    mutable count = 0;
                    mutable flag = {flag};
                    let value = {call};
                    count * 100 + value
                }}
            "#};
            eprintln!("{label}, flag={flag}");
            check_callable_result(&source, expected);
            let (mut store, package) = crate::test_utils::compile_and_run_pipeline_to(
                &source,
                crate::PipelineStage::Defunc,
            );
            crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package, &[]);
            assert_eq!(
                crate::test_utils::try_eval_fir_entry(&store, package),
                Ok(qsc_eval::val::Value::Int(expected)),
                "{label}, flag={flag}",
            );
        }
    }
    for body in [
        "for i in 0..2 { set total += ({ set count += 1; flag ? Add1 | Times2 })({ set flag = not flag; 2 }); }",
        "mutable i = 0; while i < 3 { set total += ({ set count += 1; flag ? Add1 | Times2 })({ set flag = not flag; 2 }); set i += 1; }",
        "mutable i = 0; repeat { set total += ({ set count += 1; flag ? Add1 | Times2 })({ set flag = not flag; 2 }); set i += 1; } until i == 3;",
    ] {
        let source = formatdoc! {r#"
            function Add1(x : Int) : Int {{ x + 1 }}
            function Times2(x : Int) : Int {{ 2 * x }}
            @EntryPoint() operation Main() : Int {{
                mutable count = 0;
                mutable flag = true;
                mutable total = 0;
                {body}
                count * 100 + total
            }}
        "#};
        eprintln!("repeated dispatch: {body}");
        check_callable_result(&source, 310);
        let (mut store, package) =
            crate::test_utils::compile_and_run_pipeline_to(&source, crate::PipelineStage::Defunc);
        crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package, &[]);
        assert_eq!(
            crate::test_utils::try_eval_fir_entry(&store, package),
            Ok(qsc_eval::val::Value::Int(310)),
            "{body}",
        );
    }
    let library = r#"
        namespace Lib {
            export Invoke;
            function Times2(x : Int) : Int { 2 * x }
            function Invoke(flag : Bool) : Int {
                mutable count = 0;
                let value = ({
                    set count += 1;
                    if flag { let n = count; x -> x + n } else { Times2 }
                })({ set count += 10; 2 });
                count * 100 + value
            }
        }
    "#;
    for (flag, expected) in [(true, 1103), (false, 1104)] {
        let source = format!("@EntryPoint() operation Main() : Int {{ Lib.Invoke({flag}) }}");
        assert_eq!(
            crate::test_utils::eval_qsharp_original_with_library(library, &source),
            Ok(qsc_eval::val::Value::Int(expected)),
        );
        for stage in [crate::PipelineStage::Defunc, crate::PipelineStage::Full] {
            let (mut store, package) = crate::test_utils::compile_and_run_pipeline_to_with_library(
                library, &source, stage,
            );
            crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package, &[]);
            assert_eq!(
                crate::test_utils::try_eval_fir_entry(&store, package),
                Ok(qsc_eval::val::Value::Int(expected)),
                "foreign conditional callee, flag={flag}, {stage:?}",
            );
        }
    }
    for (flag, expected) in [(true, 4), (false, 8)] {
        let source = formatdoc! {r#"
            function Add1(x : Int) : Int {{ x + 1 }}
            function Times2(x : Int) : Int {{ 2 * x }}
            @EntryPoint() operation Main() : Int {{
                mutable flag = {flag};
                mutable f = Add1;
                (if flag {{ set f = Add1; Add1 }} else {{ set f = Times2; Times2 }})(
                    {{ let value = f(2); value }})
            }}
        "#};
        check_callable_result(&source, expected);
    }
    for (flag, expected) in [(true, 33), (false, 44)] {
        let source = formatdoc! {r#"
            function Add1(x : Int) : Int {{ x + 1 }}
            function Times2(x : Int) : Int {{ 2 * x }}
            @EntryPoint() operation Main() : Int {{
                mutable flag = {flag};
                mutable f = Add1;
                let value = (if flag {{ set f = Add1; Add1 }} else {{ set f = Times2; Times2 }})(
                    {{ set flag = not flag; 2 }});
                value * 10 + f(2)
            }}
        "#};
        check_callable_result(&source, expected);
    }
}

#[test]
fn failing_direct_callee_and_factory_preserve_failure_before_argument() {
    for callee in [
        "{ fail \"callee evaluated\"; Add1 }",
        "Make()",
        "{ fail \"callee evaluated\"; let n = 1; x -> x + n }",
        "{ fail \"callee evaluated\"; flag ? Add1 | Times2 }",
        "{ { fail \"callee evaluated\"; flag ? Add1 | Times2 } }",
        "if flag { fail \"callee evaluated\"; Add1 } else { Times2 }",
    ] {
        let source = formatdoc! {r#"
            function Add1(x : Int) : Int {{ x + 1 }}
            function Times2(x : Int) : Int {{ 2 * x }}
            function Make() : Int -> Int {{ fail "callee evaluated"; Add1 }}
            @EntryPoint()
            operation Main() : Int {{
                mutable flag = true;
                ({callee})({{ fail "argument evaluated"; 2 }})
            }}
        "#};
        let (store, package) = crate::test_utils::compile_to_fir(&source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package);
        let failure = result.expect_err("callee must fail before the argument");
        assert!(failure.contains("callee evaluated"), "{callee}: {failure}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
    for flag in [true, false] {
        for callee in [
            "{ X(q); fail \"callee prefix failed\"; flag ? Add1 | Times2 }",
            "{ { X(q); fail \"callee prefix failed\"; flag ? Add1 | Times2 } }",
            if flag {
                "if flag { X(q); fail \"callee prefix failed\"; Add1 } else { Times2 }"
            } else {
                "if flag { Add1 } else { X(q); fail \"callee prefix failed\"; Times2 }"
            },
        ] {
            let source = formatdoc! {r#"
                function Add1(x : Int) : Int {{ x + 1 }}
                function Times2(x : Int) : Int {{ 2 * x }}
                @EntryPoint() operation Main() : Int {{
                    use q = Qubit();
                    mutable flag = {flag};
                    ({callee})({{ Z(q); fail "argument failed"; 2 }})
                }}
            "#};
            let (store, package) = crate::test_utils::compile_to_fir(&source);
            let (expected, trace) =
                crate::test_utils::try_eval_fir_entry_with_trace(&store, package);
            assert!(
                expected
                    .as_ref()
                    .expect_err("callee must fail")
                    .contains("callee prefix failed"),
                "{callee}, flag={flag}: {expected:?}",
            );
            expect_test::expect![[r#"[QubitAllocate(0), Gate { name: "X", is_adjoint: false, targets: [0], controls: [], theta: None }]"#]]
                .assert_eq(&format!("{trace:?}"));
            for stage in [crate::PipelineStage::Defunc, crate::PipelineStage::Full] {
                let (mut store, package) =
                    crate::test_utils::compile_and_run_pipeline_to(&source, stage);
                crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package, &[]);
                let (actual, actual_trace) =
                    crate::test_utils::try_eval_fir_entry_with_trace(&store, package);
                assert_eq!(actual, expected, "{callee}, flag={flag}, {stage:?}");
                assert_eq!(actual_trace, trace, "{callee}, flag={flag}, {stage:?}");
            }
        }
    }
}

#[test]
fn later_callable_selection_observes_earlier_argument_write_returning_four() {
    for selection in ["flag ? Add1 | Times2", "[Times2, Add1][index]"] {
        for call in [
            format!("ApplyAfter({{ set flag = false; set index = 0; 2 }}, {selection})"),
            format!("ApplyNested(({{ set flag = false; set index = 0; 2 }}, {selection}))"),
        ] {
            check_callable_result(
                &formatdoc! {r#"
                    function Add1(x : Int) : Int {{ x + 1 }}
                    function Times2(x : Int) : Int {{ x * 2 }}
                    function ApplyAfter(x : Int, f : Int -> Int) : Int {{ f(x) }}
                    function ApplyNested(pair : (Int, Int -> Int)) : Int {{
                        let (x, f) = pair;
                        f(x)
                    }}
                    @EntryPoint()
                    operation Main() : Int {{
                        mutable flag = true;
                        mutable index = 1;
                        {call}
                    }}
                "#},
                4,
            );
        }
    }
}

#[test]
fn earlier_callable_capture_preserves_three_across_scalar_and_controlled_inputs() {
    for call in [
        "Apply({ let n = k; x -> x + n }, { set k = 7; 0 })",
        "ApplyNested(({ let n = k; x -> x + n }, { set k = 7; 0 }))",
        "({ let n = k; x -> x + n })({ set k = 7; 0 })",
        "Apply(if true { let n = k; x -> x + n } else { x -> x }, { set k = 7; 0 })",
    ] {
        check_callable_result(
            &formatdoc! {r#"
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function ApplyNested(pair : (Int -> Int, Int)) : Int {{
                    let (f, x) = pair;
                    f(x)
                }}
                @EntryPoint()
                operation Main() : Int {{
                    mutable k = 3;
                    {call}
                }}
            "#},
            3,
        );
    }
    for call in [
        "Apply({ let n = k; target => Target(n, target) }, { set k = 7; q });",
        "Controlled Apply([], ({ let n = k; target => Target(n, target) }, { set k = 7; q }));",
        "Controlled Adjoint Apply([], ({ let n = k; target => Target(n, target) }, { set k = 7; q }));",
    ] {
        let source = formatdoc! {r#"
            operation Target(n : Int, q : Qubit) : Unit is Adj + Ctl {{
                body (...) {{ fail $"captured {{n}}"; }}
                adjoint self;
                controlled (controls, ...) {{ fail $"captured {{n}}"; }}
                controlled adjoint self;
            }}
            operation Apply(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl {{
                op(q);
            }}
            @EntryPoint()
            operation Main() : Unit {{
                use q = Qubit();
                mutable k = 3;
                {call}
            }}
        "#};
        let (store, package) = crate::test_utils::compile_to_fir(&source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package);
        let failure = result.expect_err("selected specialization reports its captured value");
        assert!(failure.contains("captured 3"), "{call}: {failure}");
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn callable_capture_timing_preserves_mutation_values_and_quantum_effects() {
    for flag in [true, false] {
        for callee in [
            "{ set count += 1; if flag { let n = count; x -> x + n } else { Times2 } }",
            "{ { set count += 1; if flag { let n = count; x -> x + n } else { Times2 } } }",
            "{ let n = { set count += 1; count }; if flag { x -> x + n } else { Times2 } }",
            "if flag { let n = { set count += 1; count }; x -> x + n } else { set count += 1; Times2 }",
        ] {
            for (argument, count) in [
                ("2", 1),
                ("{ set count += 10; set flag = not flag; 2 }", 11),
            ] {
                let expected = count * 100 + if flag { 3 } else { 4 };
                let source = formatdoc! {r#"
                    function Times2(x : Int) : Int {{ 2 * x }}
                    @EntryPoint() operation Main() : Int {{
                        mutable count = 0;
                        mutable flag = {flag};
                        let value = ({callee})({argument});
                        count * 100 + value
                    }}
                "#};
                eprintln!("capture flag={flag}, callee={callee}, argument={argument}");
                check_callable_result(&source, expected);
                let (mut store, package) = crate::test_utils::compile_and_run_pipeline_to(
                    &source,
                    crate::PipelineStage::Defunc,
                );
                crate::exec_graph_rebuild::rebuild_exec_graphs(&mut store, package, &[]);
                assert_eq!(
                    crate::test_utils::try_eval_fir_entry(&store, package),
                    Ok(qsc_eval::val::Value::Int(expected)),
                    "capture flag={flag}, callee={callee}, argument={argument}",
                );
            }
        }
    }
    check_callable_result(
        indoc::indoc! {r#"
            function Apply(a : Int, f : Int -> Int, b : Int, g : Int -> Int, c : Int) : Int {
                a * 1000 + f(0) * 100 + b * 10 + g(c)
            }
            @EntryPoint()
            operation Main() : Int {
                mutable k = 3;
                Apply(
                    { set k = 2; 1 },
                    { let n = k; x -> x + n },
                    { set k = 4; 3 },
                    { let n = k; x -> x + n },
                    { set k = 9; 0 })
            }
        "#},
        1234,
    );
    for (body, expected) in [
        (
            "let result = Apply({ let n = Capture(q); x -> x + n }, { Z(q); 0 }); X(q); result",
            3,
        ),
        (
            "ApplyTwo({ let n = Capture(q); x -> x + n }, { Z(q); 0 }, { let n = Capture(q); x -> x + n })",
            33,
        ),
    ] {
        let source = formatdoc! {r#"
            operation Capture(q : Qubit) : Int {{ X(q); 3 }}
            function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
            function ApplyTwo(f : Int -> Int, x : Int, g : Int -> Int) : Int {{ f(x) * 10 + g(x) }}
            @EntryPoint()
            operation Main() : Int {{
                use q = Qubit();
                {body}
            }}
        "#};
        let (store, package) = crate::test_utils::compile_to_fir(&source);
        let (result, trace) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package);
        assert_eq!(result, Ok(qsc_eval::val::Value::Int(expected)));
        expect_test::expect![[r#"[QubitAllocate(0), Gate { name: "X", is_adjoint: false, targets: [0], controls: [], theta: None }, Gate { name: "Z", is_adjoint: false, targets: [0], controls: [], theta: None }, Gate { name: "X", is_adjoint: false, targets: [0], controls: [], theta: None }, QubitRelease(0)]"#]]
            .assert_eq(&format!("{trace:?}"));
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn tuple_loop_callable_writes_return_seven_for_direct_and_higher_order_calls() {
    for call in ["f(2)", "Apply(f, 2)"] {
        check_callable_result(
            &formatdoc! {r#"
                namespace Test {{
                    function Add1(x : Int) : Int {{ x + 1 }}
                    function Times2(x : Int) : Int {{ x * 2 }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    @EntryPoint()
                    operation Main() : Int {{
                        mutable f = Add1;
                        mutable index = 0;
                        mutable result = 0;
                        while index < 2 {{
                            set result += {call};
                            set (index, f) = (index + 1, Times2);
                        }}
                        result
                    }}
                }}
            "#},
            7,
        );
    }
}

#[test]
fn scalar_loop_callable_write_returns_seven() {
    check_callable_result(
        indoc::indoc! {r#"
            namespace Test {
                function Add1(x : Int) : Int { x + 1 }
                function Times2(x : Int) : Int { x * 2 }
                @EntryPoint()
                operation Main() : Int {
                    mutable f = Add1;
                    mutable index = 0;
                    mutable result = 0;
                    while index < 2 {
                        set result += f(2);
                        set index += 1;
                        set f = Times2;
                    }
                    result
                }
            }
        "#},
        7,
    );
}

#[test]
fn tuple_loop_guard_write_preserves_callable_snapshot_returning_six() {
    check_callable_result(
        indoc::indoc! {r#"
            namespace Test {
                function Add1(x : Int) : Int { x + 1 }
                function Times2(x : Int) : Int { x * 2 }
                @EntryPoint()
                operation Main() : Int {
                    mutable flag = true;
                    let f = flag ? Add1 | Times2;
                    mutable index = 0;
                    mutable result = 0;
                    while index < 2 {
                        set result += f(2);
                        set (index, flag) = (index + 1, false);
                    }
                    result
                }
            }
        "#},
        6,
    );
}

#[test]
fn direct_callee_before_argument_write_preserves_selected_value_and_failure() {
    check_callable_result(
        indoc::indoc! {r#"
            namespace Test {
                function Add1(x : Int) : Int { x + 1 }
                function Times2(x : Int) : Int { x * 2 }
                @EntryPoint()
                operation Main() : Int {
                    mutable f = Add1;
                    f({ set f = Times2; 2 })
                }
            }
        "#},
        3,
    );
    check_wrapped_callable_before_write(false);
}

#[test]
fn direct_callee_before_tuple_argument_write_returns_three() {
    check_callable_result(
        indoc::indoc! {r#"
            namespace Test {
                function Add1(x : Int) : Int { x + 1 }
                function Times2(x : Int) : Int { x * 2 }
                @EntryPoint()
                operation Main() : Int {
                    mutable f = Add1;
                    mutable n = 0;
                    f({ set (f, n) = (Times2, 9); 2 })
                }
            }
        "#},
        3,
    );
}

#[test]
fn higher_order_callable_before_later_write_preserves_selected_value_and_failure() {
    for assignment in ["set f = Times2;", "set (f, n) = (Times2, 9);"] {
        check_callable_result(
            &formatdoc! {r#"
                namespace Test {{
                    function Add1(x : Int) : Int {{ x + 1 }}
                    function Times2(x : Int) : Int {{ x * 2 }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    @EntryPoint()
                    operation Main() : Int {{
                        mutable f = Add1;
                        mutable n = 0;
                        Apply(f, {{ {assignment} 2 }})
                    }}
                }}
            "#},
            3,
        );
    }
    check_wrapped_callable_before_write(true);
}

fn check_wrapped_callable_before_write(higher_order: bool) {
    for (functor, input, argument, specialization) in [
        ("Adjoint", "Int", "value", "adjoint"),
        ("Controlled", "(Qubit[], Int)", "([], value)", "controlled"),
        (
            "Controlled Adjoint",
            "(Qubit[], Int)",
            "([], value)",
            "controlled adjoint",
        ),
        ("Adjoint Adjoint", "Int", "value", "body"),
    ] {
        let argument = argument.replace("value", "{ set calls += 1; set op = Second; calls }");
        let call = if higher_order {
            format!("Apply({functor} op, {argument});")
        } else {
            format!("{functor} op({argument});")
        };
        let source = formatdoc! {r#"
            namespace Test {{
                operation First(value : Int) : Unit is Adj + Ctl {{
                    body (...) {{ fail $"first body {{value}}"; }}
                    adjoint (...) {{ fail $"first adjoint {{value}}"; }}
                    controlled (controls, ...) {{ fail $"first controlled {{value}}"; }}
                    controlled adjoint (controls, ...) {{
                        fail $"first controlled adjoint {{value}}";
                    }}
                }}
                operation Second(value : Int) : Unit is Adj + Ctl {{
                    body (...) {{ fail "second"; }}
                    adjoint (...) {{ fail "second"; }}
                    controlled (controls, ...) {{ fail "second"; }}
                    controlled adjoint (controls, ...) {{ fail "second"; }}
                }}
                operation Apply(op : {input} => Unit, value : {input}) : Unit {{
                    op(value);
                }}
                @EntryPoint()
                operation Main() : Unit {{
                    mutable calls = 0;
                    mutable op = First;
                    {call}
                }}
            }}
        "#};
        let (store, package_id) = crate::test_utils::compile_to_fir(&source);
        let (result, _) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package_id);
        let error = result.expect_err("the originally selected specialization must fail");
        assert!(
            error.contains(&format!("first {specialization} 1")),
            "unexpected failure for {functor}: {error}"
        );
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

#[test]
fn indexed_call_after_earlier_operand_preserves_order_returning_1312() {
    check_callable_result(
        indoc::indoc! {r#"
            namespace Test {
                function Add1(x : Int) : Int { x + 1 }
                function Times2(x : Int) : Int { x * 2 }
                @EntryPoint()
                operation Main() : Int {
                    mutable order = 0;
                    let result = { set order = order * 10 + 1; 10 }
                        + [Add1, Times2][{ set order = order * 10 + 2; 0 }](2);
                    result * 100 + order
                }
            }
        "#},
        1312,
    );
}

#[test]
fn indexed_call_in_unselected_branch_does_not_evaluate_index() {
    check_callable_result(
        indoc::indoc! {r#"
            namespace Test {
                function Add1(x : Int) : Int { x + 1 }
                function Times2(x : Int) : Int { x * 2 }
                @EntryPoint()
                operation Main() : Int {
                    mutable selected = false;
                    mutable order = 0;
                    let result = selected
                        ? [Add1, Times2][{ set order += 1; 0 }](2)
                        | 9;
                    result * 10 + order
                }
            }
        "#},
        90,
    );
}

#[test]
fn identical_conditional_index_arms_return_four_for_direct_and_higher_order_calls() {
    for call in ["f(2)", "Apply(f, 2)"] {
        check_callable_result(
            &formatdoc! {r#"
                namespace Test {{
                    function Add1(x : Int) : Int {{ x + 1 }}
                    function Times2(x : Int) : Int {{ x * 2 }}
                    function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                    @EntryPoint()
                    operation Main() : Int {{
                        let fs = [Add1, Times2];
                        mutable index = 1;
                        mutable flag = true;
                        let f = flag ? fs[index] | fs[index];
                        {call}
                    }}
                }}
            "#},
            4,
        );
    }
}

mod callable_evaluation_order {
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

/// Branch dispatch may reuse the selection, not rerun its measurement. The
/// prepass stores the guard where the original binding evaluates it, and later
/// dispatch reads that snapshot. The trace catches an extra measurement even
/// when the final result happens to be unchanged.
#[test]
fn replayed_disposition_preserves_branch_selection_once() {
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

/// An intervening gate changes the selector qubit after selection. Both direct
/// and HOF calls must keep the originally chosen operation across repeated uses,
/// while the trace must contain only the original selecting measurement.
#[test]
fn measured_callable_selection_survives_intervening_effects_and_reuse() {
    for selected_x in [false, true] {
        for invocation in ["chosen(target);", "Apply(chosen, target);"] {
            for count in [1, 2] {
                let calls = invocation.repeat(count);
                let source = formatdoc! {r#"
                    operation Apply(op : Qubit => Unit, target : Qubit) : Unit {{op(target);}}
                    @EntryPoint() operation Main() : Int {{
                        use selector = Qubit();
                        use target = Qubit();
                        if {selected_x} {{X(selector);}}
                        let chosen = if MResetZ(selector) == One {{X}} else {{I}};
                        Message("selected");
                        X(selector);
                        {calls}
                        Reset(selector);
                        if MResetZ(target) == One {{1}} else {{0}}
                    }}
                "#};
                crate::test_utils::check_semantic_equivalence_with_expected(
                    &source,
                    qsc_eval::val::Value::Int(i64::from(selected_x && count == 1)),
                );
                crate::test_utils::generate_qir(&source);
            }
        }
    }
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

#[test]
fn stored_multi_parameter_alias_preserves_capture_layout() {
    check_aggregate_rewrite_result(
        r#"
        function Run(f : Int -> Int, a : Int, b : Int) : Int { 100 * a + f(b) }
        function Pick(offset : Int) : Int {
            let args = (x -> x + offset, 4, 3);
            let alias = args;
            Run(alias) + Run(args)
        }
        @EntryPoint() operation Main() : Int { Pick(10) }
        "#,
        826,
    );
}

#[test]
fn nested_projected_aggregate_argument_preserves_payload() {
    check_aggregate_rewrite_result(
        r#"
        struct Inner { Pair : (Int -> Int, Int) }
        struct Outer { Inner : Inner }
        function Inc(x : Int) : Int { x + 1 }
        function Run(pair : (Int -> Int, Int)) : Int {
            let (f, n) = pair;
            f(n)
        }
        @EntryPoint() operation Main() : Int {
            let outer = new Outer { Inner = new Inner { Pair = (Inc, 4) } };
            Run(outer.Inner.Pair)
        }
        "#,
        5,
    );
}

#[test]
fn stored_nested_callable_array_arguments_preserve_aliases() {
    check_aggregate_rewrite_result(
        r#"
        function Inc(x : Int) : Int { x + 1 }
        function Twice(x : Int) : Int { 2 * x }
        function Run(prefix : Int, pair : ((Int -> Int)[], Int)) : Int {
            let (ops, n) = pair;
            prefix + ops[1](n)
        }
        @EntryPoint() operation Main() : Int {
            let args = (10, ([Inc, Twice], 3));
            let alias = args;
            Run(alias) + Run(args)
        }
        "#,
        32,
    );
}

// Keep complete Q# fixtures together for the shared semantic and QIR checks.
#[allow(clippy::too_many_lines)]
pub(super) fn nonliteral_argument_rewrite_cases() -> Vec<(String, i64)> {
    [
        (
            r#"
            function Run(f : Int -> Int, a : Int, b : Int) : Int { 100 * a + f(b) }
            function Pick(offset : Int) : Int {
                let args = (x -> x + offset, 4, 3);
                Run(args)
            }
            @EntryPoint() operation Main() : Int { Pick(10) }
            "#,
            413,
        ),
        (
            r#"
            function Run(f : Int -> Int, a : Int, b : Int) : Int { 100 * a + f(b) }
            function Logged(label : String, value : Int) : Int { Message(label); value }
            function Pick(offset : Int) : Int {
                let args = (
                    x -> x + offset,
                    Logged("first", 4),
                    Logged("second", 3)
                );
                let alias = args;
                Message("stored");
                Run(alias) + Run(args)
            }
            @EntryPoint() operation Main() : Int { Pick(10) }
            "#,
            826,
        ),
        (
            r#"
            struct Outer { Pair : (Int -> Int, Int) }
            function Inc(x : Int) : Int { x + 1 }
            function Run(pair : (Int -> Int, Int)) : Int {
                let (f, n) = pair;
                f(n)
            }
            @EntryPoint() operation Main() : Int {
                let outer = new Outer { Pair = (Inc, 3) };
                Run(outer.Pair)
            }
            "#,
            4,
        ),
        (
            r#"
            struct Inner { Pair : (Int -> Int, Int) }
            struct Outer { Inner : Inner }
            function Inc(x : Int) : Int { x + 1 }
            function Logged(value : Int) : Int { Message("field"); value }
            function Run(pair : (Int -> Int, Int)) : Int {
                let (f, n) = pair;
                f(n)
            }
            @EntryPoint() operation Main() : Int {
                let outer = new Outer {
                    Inner = new Inner { Pair = (Inc, Logged(4)) }
                };
                Message("stored");
                Run(outer.Inner.Pair)
            }
            "#,
            5,
        ),
        (
            r#"
            function Inc(x : Int) : Int { x + 1 }
            function Twice(x : Int) : Int { 2 * x }
            function Run(prefix : Int, pair : ((Int -> Int)[], Int)) : Int {
                let (ops, n) = pair;
                prefix + ops[1](n)
            }
            @EntryPoint() operation Main() : Int {
                let args = (10, ([Inc, Twice], 3));
                Run(args)
            }
            "#,
            16,
        ),
        (
            r#"
            function Inc(x : Int) : Int { x + 1 }
            function Twice(x : Int) : Int { 2 * x }
            function Run(prefix : Int, pair : ((Int -> Int)[], Int)) : Int {
                let (ops, n) = pair;
                prefix + ops[1](n)
            }
            @EntryPoint() operation Main() : Int {
                let args = (
                    Logged("prefix", 10),
                    ([Inc, Twice], Logged("value", 3))
                );
                let alias = args;
                Message("stored");
                Run(alias) + Run(args)
            }
            function Logged(label : String, value : Int) : Int { Message(label); value }
            "#,
            32,
        ),
        (
            r#"
            function Run(prefix : Int, pair : ((Int -> Int)[], Int)) : Int {
                let (ops, n) = pair;
                prefix + ops[1](n)
            }
            function Pick(offset : Int) : Int {
                let args = (10, ([x -> x + offset, x -> x + offset + 1], 3));
                Run(args)
            }
            @EntryPoint() operation Main() : Int { Pick(10) }
            "#,
            24,
        ),
    ]
    .into_iter()
    .map(|(source, expected)| (source.to_string(), expected))
    .collect()
}

pub(super) fn deep_partial_argument_rewrite_cases() -> Vec<(String, i64)> {
    let mut cases = vec![(
        r#"
        function Inc(x : Int) : Int { x + 1 }
        function Twice(x : Int) : Int { 2 * x }
        function Run(ops : (Int -> Int)[], pair : ((Int -> Int, Int -> Int), Int)) : Int {
            let ((f, g), n) = pair;
            ops[1](n) + f(n) + g(n)
        }
        @EntryPoint() operation Main() : Int {
            Run([Inc, Twice], ((Inc, Twice), 3))
        }
        "#
        .to_string(),
        16,
    )];
    for stored in [false, true] {
        for (signature, unpack, argument, result, expected) in [
            (
                "((Int -> Int, Int -> Int), Int)",
                "let ((f, g), n) = pair;",
                "((Inc, Twice), { set order = 10 * order + 2; 3 })",
                "ops[1](n) + f(n) + g(n)",
                12316,
            ),
            (
                "((Int -> Int, Int -> Int, Int -> Int), Int)",
                "let ((f, g, h), n) = pair;",
                "((Inc, Twice, Inc), { set order = 10 * order + 2; 3 })",
                "ops[1](n) + f(n) + g(n) + h(n)",
                12320,
            ),
            (
                "((Int -> Int, Int -> Int), Unit, Int)",
                "let ((f, g), unit, n) = pair;",
                "((Inc, Twice), (), { set order = 10 * order + 2; 3 })",
                "if unit == () { ops[1](n) + f(n) + g(n) } else { 0 }",
                12316,
            ),
        ] {
            let arguments = format!("([Inc, Twice], {argument})");
            let invocation = if stored {
                format!("let args = {arguments}; let alias = args; Run(alias)")
            } else {
                format!("Run{arguments}")
            };
            cases.push((
                formatdoc! {r#"
                    function Inc(x : Int) : Int {{ x + 1 }}
                    function Twice(x : Int) : Int {{ 2 * x }}
                    function Run(ops : (Int -> Int)[], pair : {signature}) : Int {{
                        {unpack}
                        {result}
                    }}
                    @EntryPoint() operation Main() : Int {{
                        mutable order = 1;
                        let value = {{ {invocation} }};
                        set order = 10 * order + 3;
                        100 * order + value
                    }}
                "#},
                expected,
            ));
        }
    }
    cases
}

#[test]
fn nonliteral_argument_rewrites_preserve_values_effects_and_abi() {
    for (source, expected) in nonliteral_argument_rewrite_cases() {
        check_aggregate_rewrite_result(&source, expected);
    }
}

#[test]
fn deep_partial_argument_rewrites_preserve_values_effects_and_abi() {
    for (source, expected) in deep_partial_argument_rewrite_cases() {
        check_aggregate_rewrite_result(&source, expected);
    }
}

fn check_aggregate_rewrite_result(source: &str, expected: i64) {
    use qsc_fir::{
        fir::{ExprKind, ItemKind, PackageLookup, Res},
        ty::Ty,
    };

    let (store, package_id) =
        crate::test_utils::compile_and_run_pipeline_to(source, crate::PipelineStage::Defunc);
    let mut checked = 0;
    for item in super::collect_reachable_from_entry(&store, package_id) {
        let package = store.get(item.package);
        let ItemKind::Callable(decl) = &package.get_item(item.item).kind else {
            continue;
        };
        crate::walk_utils::for_each_expr_in_callable_impl(
            package,
            &decl.implementation,
            &mut |_, expr| {
                let ExprKind::Call(callee, args) = expr.kind else {
                    return;
                };
                let ExprKind::Var(Res::Item(target), _) = package.get_expr(callee).kind else {
                    return;
                };
                let owner = store.get(target.package);
                let ItemKind::Callable(target) = &owner.get_item(target.item).kind else {
                    return;
                };
                if !target.name.name.starts_with("Run") || !target.name.name.contains('{') {
                    return;
                }
                let expected_input = &owner.get_pat(target.input).ty;
                assert_eq!(&package.get_expr(args).ty, expected_input, "{source}");
                let Ty::Arrow(arrow) = &package.get_expr(callee).ty else {
                    panic!("specialized target must have an arrow type");
                };
                assert_eq!(arrow.input.as_ref(), expected_input, "{source}");
                checked += 1;
            },
        );
    }
    assert!(checked > 0, "expected a specialized Run call:\n{source}");
    check_callable_result(source, expected);
}

/// Shared lambda targets must not merge different captured scalars or nested
/// callable values, including captures changed by recursive factory calls.
/// The operation case additionally distinguishes Step from Adjoint Step by
/// their X versus Z trace, not just by the final returned value.
#[test]
fn wrapped_closures_preserve_distinct_captures_and_functor_effects() {
    for (declarations, call, expected) in [
        (
            "function Make(n : Int) : Int -> Int { x -> x+n }",
            "Apply(Make(2), 1)*100+Apply(Make(5), 1)",
            306,
        ),
        (
            "function Make(n : Int) : Int -> Int { x -> x*n }\n\
             function Wrap(f : Int -> Int, n : Int) : Int -> Int { x -> f(x)+n }",
            "Apply(Wrap(Make(2), 1), 5)*100+Apply(Wrap(Make(3), 4), 5)",
            1119,
        ),
        (
            "function Make(n : Int) : Int -> Int { x -> x+n }\n\
             function Rec(f : Int -> Int, n : Int) : Int {\n\
                 if n == 0 { f(1) } else { f(n)*100+Rec(Make(n), n-1) }\n\
             }",
            "Rec(Make(5), 2)",
            1002,
        ),
    ] {
        check_callable_result(
            &formatdoc! {r#"
            {declarations}
            function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
            @EntryPoint() operation Main() : Int {{ {call} }}
        "#},
            expected,
        );
    }
    let source = r#"
        operation Step(q : Qubit) : Unit is Adj {
            body ... { X(q); }
            adjoint ... { Z(q); }
        }
        function Wrap(f : Qubit => Unit is Adj) : Qubit => Unit is Adj { q => f(q) }
        operation Apply(f : Qubit => Unit is Adj, q : Qubit) : Unit { f(q); }
        @EntryPoint() operation Main() : Unit {
            use q = Qubit();
            let first = Wrap(Step); let second = Wrap(Adjoint Step);
            Apply(first, q); Apply(second, q); X(q);
        }
    "#;
    let (store, package) = crate::test_utils::compile_to_fir(source);
    let (result, trace) = crate::test_utils::try_eval_fir_entry_with_trace(&store, package);
    assert_eq!(result, Ok(Value::unit()));
    expect_test::expect![[r#"[QubitAllocate(0), Gate { name: "X", is_adjoint: false, targets: [0], controls: [], theta: None }, Gate { name: "Z", is_adjoint: false, targets: [0], controls: [], theta: None }, Gate { name: "X", is_adjoint: false, targets: [0], controls: [], theta: None }, QubitRelease(0)]"#]]
        .assert_eq(&format!("{trace:?}"));
    crate::test_utils::check_semantic_equivalence(source);
}

/// Exercise direct and transitive forwarding, with straight-line or loop
/// reassignment. The direct closure keeps capture 1; the deferred closure uses 3.
/// Removing the direct use must not remove the deferred factory's environment.
#[test]
fn forwarded_factories_preserve_independent_direct_and_deferred_captures() {
    for (forward, assignment, direct, expected) in [
        (
            "Make(n)",
            "for i in 0..0 { set deferred = Forward(3); }",
            "direct(0)*10+",
            13,
        ),
        (
            "Relay(n)",
            "for i in 0..0 { set deferred = Forward(3); }",
            "direct(0)*10+",
            13,
        ),
        ("Make(n)", "set deferred = Forward(3);", "direct(0)*10+", 13),
        (
            "Make(n)",
            "for i in 0..0 { set deferred = Forward(3); }",
            "",
            3,
        ),
    ] {
        check_callable_result(
            &formatdoc! {r#"
            function Make(n : Int) : Int -> Int {{ x -> x+n }}
            function Relay(n : Int) : Int -> Int {{ Make(n) }}
            function Forward(n : Int) : Int -> Int {{ {forward} }}
            @EntryPoint() operation Main() : Int {{
                let direct = Make(1);
                mutable deferred = Forward(0);
                {assignment}
                {direct}deferred(0)
            }}
        "#},
            expected,
        );
    }
}

/// Vary the factory's container, forwarding depth, and reassignment shape.
/// Retained closures must use the updated capture and retain any non-callable
/// fields. Omitting the direct closure use removes only its contribution of 10.
#[test]
fn callable_aggregate_factories_preserve_captures_and_noncallable_fields() {
    for (declarations, result, expected) in [
        (
            "function Make(n : Int) : (Int -> Int, Int) { (x -> x+n, n) }",
            "let (a, _) = direct; let (f, n) = deferred; a(0)*10+f(0)+n",
            16,
        ),
        (
            "struct Box { F : Int -> Int, N : Int }\n\
          function Make(n : Int) : Box { new Box { F = x -> x+n, N = n } }",
            "direct::F(0)*10+deferred::F(0)+deferred::N",
            16,
        ),
        (
            "function Make(n : Int) : (Int -> Int)[] { [x -> x+n] }",
            "direct[0](0)*10+deferred[0](0)",
            13,
        ),
        (
            "struct Box { F : Int -> Int, N : Int }\n\
             function Make(n : Int) : Box { new Box { F = x -> x+n, N = n } }",
            "let dataOnly = Make(9); direct::F(0)*10+deferred::F(0)+deferred::N+dataOnly::N",
            25,
        ),
        (
            "function Produce(n : Int) : (Int -> Int, Int) { (x -> x+n, n) }\n\
          function Relay(n : Int) : (Int -> Int, Int) { Produce(n) }\n\
          function Make(n : Int) : (Int -> Int, Int) { Relay(n) }",
            "let (a, _) = direct; let (f, n) = deferred; a(0)*10+f(0)+n",
            16,
        ),
    ] {
        for update in [
            "for i in 0..0 { set deferred = Make(3); }",
            "set deferred = Make(3);",
        ] {
            for consume_direct in [true, false] {
                let result = if consume_direct {
                    result.to_string()
                } else {
                    result
                        .replace("a(0)*10+", "")
                        .replace("direct::F(0)*10+", "")
                        .replace("direct[0](0)*10+", "")
                };
                check_callable_result(
                    &formatdoc! {r#"
                {declarations}
                @EntryPoint() operation Main() : Int {{
                    let direct = Make(1);
                    mutable deferred = Make(0);
                    {update}
                    {result}
                }}
            "#},
                    if consume_direct {
                        expected
                    } else {
                        expected - 10
                    },
                );
            }
        }
    }
}

/// Reassignment must replace either a named callable or an earlier factory
/// result, without overwriting the independent direct closure's capture.
/// The result combines the direct capture 1 and updated capture 3 as `10 + 3`.
#[test]
fn reassigned_factory_result_preserves_captures_with_either_initializer() {
    for initializer in ["Add1", "Make(0)"] {
        check_callable_result(
            &formatdoc! {r#"
                namespace Test {{
                    function Add1(x : Int) : Int {{ x + 1 }}
                    function Make(n : Int) : Int -> Int {{ x -> x + n }}
                    @EntryPoint()
                    operation Main() : Int {{
                        let direct = Make(1);
                        mutable deferred = {initializer};
                        for index in 0..0 {{ set deferred = Make(3); }}
                        direct(0) * 10 + deferred(0)
                    }}
                }}
            "#},
            13,
        );
    }
}

/// Alternate calls between closures with different nested offsets and scales.
/// Direct and higher-order dispatch must use each instance's own environment.
#[test]
fn distinct_nested_closures_preserve_direct_and_forwarded_results() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        indoc::indoc! {r#"
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
    "#},
        Value::Tuple(
            vec![
                Value::Int(8),
                Value::Int(95),
                Value::Int(12),
                Value::Int(105),
            ]
            .into(),
            None,
        ),
    );
}

#[test]
fn nested_closure_preserves_mixed_direct_and_forwarded_results() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        &nested_closure_source("let first = Wrap(Make(3), 2); (first(1), Invoke(first, 3))"),
        Value::Tuple(vec![Value::Int(8), Value::Int(12)].into(), None),
    );
}

fn nested_closure_source(body: &str) -> String {
    formatdoc! {r#"
        function Make(offset : Int) : Int -> Int {{ value -> value+offset }}
        function Wrap(inner : Int -> Int, scale : Int) : Int -> Int {{
            value -> inner(value)*scale
        }}
        function Invoke(callable : Int -> Int, value : Int) : Int {{ callable(value) }}
        @EntryPoint()
        operation Main() : (Int, Int) {{ {body} }}
    "#}
}

#[test]
fn nested_closure_preserves_direct_call_results() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        &nested_closure_source("let first = Wrap(Make(3), 2); (first(1), first(3))"),
        Value::Tuple(vec![Value::Int(8), Value::Int(12)].into(), None),
    );
}

#[test]
fn nested_closure_preserves_higher_order_call_results() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        &nested_closure_source(
            "let first = Wrap(Make(3), 2); (Invoke(first, 1), Invoke(first, 3))",
        ),
        Value::Tuple(vec![Value::Int(8), Value::Int(12)].into(), None),
    );
}

/// Identical conditional arms leave an indexed closure value live even when
/// per-candidate specializations exist. Cleanup must not replace it with a
/// stand-in. Zero-angle rotations keep both results Zero; trace comparison still
/// detects the wrong gate, a missing invocation, or duplicated effects.
#[test]
fn unresolved_indexed_dispatch_preserves_live_closure_effects() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        indoc::indoc! {r#"
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
    "#},
        Value::Tuple(
            vec![Value::Result(MeasurementResult::Val(false)); 2].into(),
            None,
        ),
    );
}

/// Specialization makes `MakeOp` unreachable without removing or replaying the
/// surrounding effects. The trace must retain X, H, Rx, H, Y in order; the
/// producer's orphaned closure can be removed later with its item.
#[test]
fn orphaned_producer_body_preserves_caller_evaluation_order() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        indoc::indoc! {r#"
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
    "#},
        Value::Result(MeasurementResult::Val(false)),
    );
}

/// Both `Choose` branches are specialized, but `Offset` reads keep their aggregate
/// values live. Replacing the unused callable fields must preserve the direct
/// calls' results and effects: X, three H, Z, four Y, then Reset.
/// The result `11 + 20 + 100 + 7 = 138` also checks the retained data fields.
#[test]
fn consumed_aggregate_fields_preserve_direct_dispatch_results_and_effects() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        indoc::indoc! {r#"
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
    "#},
        Value::Int(138),
    );
}

/// Table lookup exercises capturing closures inside library aggregate values.
/// Address |1> selects data[1] = [true], so the output must measure One.
/// Trace comparison also checks that cleanup preserves the library's gate sequence.
#[test]
fn table_lookup_preserves_result_and_effects_after_closure_cleanup() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        indoc::indoc! {r#"
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
    "#},
        Value::Result(MeasurementResult::Val(true)),
    );
}

/// `MakeRot` only constructs a partial application, so specialization may consume
/// its result. The captured pi rotation must still execute once, yielding One
/// with the same effect trace before and after transformation.
#[test]
fn pure_partial_application_factory_preserves_rotation_effects_after_specialization() {
    crate::test_utils::check_semantic_equivalence_with_expected(
        indoc::indoc! {r#"
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
    "#},
        Value::Result(MeasurementResult::Val(true)),
    );
}

/// The captured count stays 3 after the source changes to 2. Eleven repetitions
/// therefore flip only the actively controlled target; compare the complete trace.
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

/// The payload applies 33 S gates, equivalent to S. The prepared S phase is
/// canceled by the adjoint but doubled by the forward operation, so measurement
/// distinguishes the functor direction as well as both control layers.
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

/// An array remains one capture slot. Adding a direct callable sibling permits
/// tuple expansion, but must not flatten or reorder the array's contents.
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

/// Eligible outer and inner environments must expand together. The array-only
/// case stays opaque; both variants must keep the original seed after reassignment.
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

/// End-to-end compatibility checks. The isolated prepass suite asserts the
/// changed layouts directly, before other passes can normalize equivalent code.
mod nested_capture_snapshots {
    use super::check_callable_result;

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
        check_callable_result(source, 10_042_099);
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
        check_callable_result(source, 33_134);
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
        check_callable_result(source, 8_095_308);
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
        check_callable_result(source, 24_261);
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
        check_callable_result(source, 24);
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
        check_callable_result(source, 70_195);
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
        check_callable_result(source, 4_019);
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
        check_callable_result(source, 6_025);
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
        check_callable_result(source, 12_049_101);
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
        check_callable_result(source, 12_049_589);
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
    check_callable_result(source, 5_027_062);
}

#[test]
fn forwarded_callable_array_preserves_each_candidates_result() {
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
        check_callable_result(&source, expected);
    }
}

/// Scratch-array sizes expose both the captured widths and the two controls.
/// Controlled calls allocate 5 and 7 scratch qubits; controlled-adjoint calls
/// allocate 8 and 6. Including the controls, the trace must allocate/release 28.
#[test]
fn controlled_root_callable_array_preserves_control_register() {
    use crate::test_utils::TraceOp::{QubitAllocate, QubitRelease};

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
    assert_eq!(
        trace
            .iter()
            .filter(|event| matches!(event, QubitAllocate(_)))
            .count(),
        28,
        "{trace:?}"
    );
    assert_eq!(
        trace
            .iter()
            .filter(|event| matches!(event, QubitRelease(_)))
            .count(),
        28,
        "{trace:?}"
    );
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
    check_callable_result(source, 5757);
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
        check_callable_result(&source, 534);
    }
}

#[test]
fn forwarded_tuple_preserves_capture_snapshot_after_reassignment() {
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
    check_callable_result(source, 171_405);
}

#[test]
fn nested_wrapper_array_preserves_distinct_capture_results() {
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
    check_callable_result(source, 24_630);
}

/// Preserve existing forwarding and mutation semantics alongside environment
/// normalization; these cases need not require normalization to execute correctly.
mod aggregate_forwarding {
    use super::check_callable_result;

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
        check_callable_result(source, 1412);
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
        check_callable_result(source, 13_405_919);
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
        check_callable_result(source, 1405);
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
        check_callable_result(source, 1413);
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
        check_callable_result(source, 1413);
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
        check_callable_result(source, 7_052_339);
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
        check_callable_result(source, 7_052_339);
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
        check_callable_result(source, 141_313);
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
        check_callable_result(source, 14_130_906);
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
        check_callable_result(source, 14_130_906);
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
