// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::fmt::Write;

use expect_test::expect;
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};

use super::compile_source_to_qir;
use super::compile_source_to_qir_with_library;
use super::compile_source_to_rir;

static CAPABILITIES: std::sync::LazyLock<TargetCapabilityFlags> =
    std::sync::LazyLock::new(|| TargetCapabilityFlags::from(Profile::Adaptive));

/// With the internal `DynamicQubitAllocation` flag enabled, qubit-allocating
/// callables may be emitted as IR functions instead of inlining.
static CAPABILITIES_DYNAMIC_QUBIT_ALLOC: std::sync::LazyLock<TargetCapabilityFlags> =
    std::sync::LazyLock::new(|| {
        TargetCapabilityFlags::from(Profile::Adaptive)
            | TargetCapabilityFlags::DynamicQubitAllocation
    });

fn check_recursive_native_boundary(
    source: String,
    limit: Option<&str>,
    gates: usize,
    measurements: usize,
    profile: Profile,
    emitted: (usize, usize),
) {
    const SOURCE_ENV: &str = "QDK_RECURSIVE_INLINE_BOUNDARY_SOURCE";
    const LIMIT_ENV: &str = "QDK_RECURSIVE_INLINE_BOUNDARY_LIMIT";
    const OUTPUT_ENV: &str = "QDK_RECURSIVE_INLINE_BOUNDARY_OUTPUT";
    const PROFILE_ENV: &str = "QDK_RECURSIVE_INLINE_BOUNDARY_PROFILE";
    if let Ok(source) = std::env::var(SOURCE_ENV) {
        let capabilities = std::env::var(PROFILE_ENV)
            .expect("parent supplies profile")
            .parse::<Profile>()
            .expect("valid boundary profile")
            .into();
        let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
        let result = crate::codegen::qir::get_qir(
            crate::SourceMap::new([("test.qs".into(), source.into())], None),
            crate::LanguageFeatures::default(),
            capabilities,
            store,
            &[(std_id, None)],
        );
        let limit = std::env::var(LIMIT_ENV).expect("parent supplies expected outcome");
        if limit.is_empty() {
            let qir = result.expect("supported boundary must compile");
            assert!(qir.contains("define i64 @ENTRYPOINT__main()"));
            assert!(qir.contains("call void @__quantum__rt__result_record_output"));
            let output = std::env::var(OUTPUT_ENV).expect("parent supplies expected output");
            let [gates, measurements, functions, calls]: [usize; 4] = output
                .split(',')
                .map(|count| count.parse().expect("numeric output count"))
                .collect::<Vec<_>>()
                .try_into()
                .expect("gate, measurement, emitted-function and call counts");
            assert_eq!(
                qir.matches("call void @__quantum__qis__x__body(").count(),
                gates
            );
            assert_eq!(
                qir.matches("call void @__quantum__qis__m__body(").count(),
                measurements
            );
            assert_eq!(
                qir.lines()
                    .filter(|line| line.starts_with("define internal void @Step"))
                    .count(),
                functions
            );
            assert_eq!(qir.matches("call void @Step").count(), calls);
            let recorded = qir
                .lines()
                .find(|line| line.contains("call void @__quantum__rt__result_record_output("))
                .expect("recorded result");
            let operand = recorded.split_once(',').expect("result operand").0;
            assert!(operand.contains(&format!("inttoptr (i64 {measurements} to ")));
        } else {
            let errors = result.expect_err("over-limit compilation must not yield QIR");
            let diagnostic = format!("{errors:?}");
            assert!(
                diagnostic.contains("RecursiveExpansionLimitExceeded"),
                "{diagnostic}"
            );
            assert!(diagnostic.contains(&format!("\"{limit}\"")), "{diagnostic}");
        }
        return;
    }

    // A separate process keeps the 120-second deadline responsive to stack
    // overflow or a compiler loop, including in an optimized test binary.
    let thread = std::thread::current();
    let mut child = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", thread.name().expect("named boundary test")])
        .env(SOURCE_ENV, source)
        .env(LIMIT_ENV, limit.unwrap_or(""))
        .env(
            OUTPUT_ENV,
            format!("{gates},{measurements},{},{}", emitted.0, emitted.1),
        )
        .env(PROFILE_ENV, profile.to_str())
        .spawn()
        .expect("start isolated native compiler");
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("check compiler process") {
            assert!(status.success(), "native boundary process failed: {status}");
            break;
        }
        if started.elapsed() >= std::time::Duration::from_mins(2) {
            child.kill().expect("terminate timed-out compiler");
            child.wait().expect("reap timed-out compiler");
            panic!("native recursive boundary exceeded 120 seconds");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn recursive_inline_production_depth_limit_counts_inline_and_emitted_bodies() {
    if std::env::var_os("QDK_RECURSIVE_INLINE_BOUNDARY_SOURCE").is_some() {
        check_recursive_native_boundary(String::new(), None, 0, 0, Profile::AdaptiveRI, (0, 0));
        return;
    }
    let depth = qsc_partial_eval::MAX_RECURSIVE_INLINE_DEPTH;
    for helper in [false, true] {
        let divisor = if helper { 2 } else { 1 };
        assert_eq!((depth - 2) % divisor, 0, "fixture must land on the limit");
        let n = (depth - 2) / divisor;
        for above in [false, true] {
            let descend = if helper {
                "Adjoint Descend(n - 1, qs);"
            } else {
                "Rec(n - 1, qs);"
            };
            let base = if above { "Step(qs);" } else { "" };
            let source = format!(
                "
                operation Main() : Result {{
                    use qs = Qubit[1];
                    Rec({n}, qs);
                    MResetZ(qs[0])
                }}
                operation Rec(n : Int, qs : Qubit[]) : Unit {{
                    if n > 0 {{
                        if M(qs[0]) == Zero {{ {descend} }}
                    }} else {{ {base} }}
                }}
                operation Descend(n : Int, qs : Qubit[]) : Unit is Adj {{
                    body (...) {{}}
                    adjoint (...) {{ Rec(n, qs); }}
                }}
                operation Step(qs : Qubit[]) : Unit {{
                    QIR.Intrinsic.__quantum__qis__x__body(qs[0]);
                }}"
            );
            check_recursive_native_boundary(
                source,
                above.then_some("depth"),
                0,
                n,
                Profile::AdaptiveRI,
                (0, 0),
            );
        }
    }
    for helpers in [depth - 2, depth - 1] {
        let mut source = String::from(
            "
            operation Main() : Result {
                use qs = Qubit[1];
                Rec(1, qs);
                MResetZ(qs[0])
            }
            operation Rec(n : Int, qs : Qubit[]) : Unit {
                if n > 0 {
                    if M(qs[0]) == Zero { Rec(n - 1, qs); Step0(qs[0]); }
                }
            }",
        );
        for i in 0..helpers {
            let body = if i + 1 < helpers {
                format!("Step{}(q);", i + 1)
            } else {
                "QIR.Intrinsic.__quantum__qis__x__body(q);".into()
            };
            writeln!(source, "operation Step{i}(q : Qubit) : Unit {{ {body} }}")
                .expect("write helper declaration");
        }
        check_recursive_native_boundary(
            source,
            (helpers > depth - 2).then_some("depth"),
            1,
            1,
            Profile::Adaptive,
            (helpers, helpers),
        );
    }
}

#[test]
fn recursive_inline_production_work_limit_counts_helpers_but_not_emitted_calls() {
    if std::env::var_os("QDK_RECURSIVE_INLINE_BOUNDARY_SOURCE").is_some() {
        check_recursive_native_boundary(String::new(), None, 0, 0, Profile::AdaptiveRI, (0, 0));
        return;
    }
    let work = qsc_partial_eval::MAX_RECURSIVE_INLINE_WORK;
    for helper in [false, true] {
        for above in [false, true] {
            let descend = if helper {
                "Adjoint Descend(n - 1, qs);"
            } else {
                "Rec(n - 1, qs);"
            };
            let padding = work - if helper { 4 } else { 3 } + usize::from(above);
            let source = format!(
                "
                operation Main() : Result {{
                    use qs = Qubit[1];
                    Rec(1, qs);
                    MResetZ(qs[0])
                }}
                operation Rec(n : Int, qs : Qubit[]) : Unit {{
                    if n > 0 {{
                        if M(qs[0]) == Zero {{
                            {descend}
                            for _ in 1..{padding} {{ Step(qs); }}
                        }}
                    }}
                }}
                operation Descend(n : Int, qs : Qubit[]) : Unit is Adj {{
                    body (...) {{}}
                    adjoint (...) {{ Rec(n, qs); }}
                }}
                operation Step(qs : Qubit[]) : Unit {{
                    QIR.Intrinsic.__quantum__qis__x__body(qs[0]);
                }}"
            );
            check_recursive_native_boundary(
                source,
                above.then_some("work"),
                padding,
                1,
                Profile::AdaptiveRI,
                (0, 0),
            );
        }
    }
    let calls = work + 1;
    check_recursive_native_boundary(
        format!(
            "
            operation Main() : Result {{
                use qs = Qubit[1];
                Rec(1, qs);
                MResetZ(qs[0])
            }}
            operation Rec(n : Int, qs : Qubit[]) : Unit {{
                if n > 0 {{
                    if M(qs[0]) == Zero {{
                        Rec(n - 1, qs);
                        {}
                    }}
                }}
            }}
            operation Step0(q : Qubit) : Unit {{
                QIR.Intrinsic.__quantum__qis__x__body(q);
            }}",
            "Step0(qs[0]);".repeat(calls)
        ),
        None,
        1,
        1,
        Profile::Adaptive,
        (1, calls),
    );
}

#[test]
fn recursive_inline_static_depth_three_preserves_measurement_guarded_target_order() {
    let source = r#"
        operation Rec(n : Int, qs : Qubit[], r : Qubit) : Unit {
            if n > 0 {
                if MResetZ(r) == Zero {
                    X(qs[n - 1]);
                    Rec(n - 1, qs, r);
                }
            }
        }
        operation Main() : Result[] {
            use qs = Qubit[3];
            use r = Qubit();
            Rec(3, qs, r);
            MResetEachZ(qs)
        }
    "#;
    let expected = [
        expect![[r#"
        block_0:
        call void @__quantum__rt__initialize(ptr null)
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 0 to ptr))
        %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        %var_3 = icmp eq i1 %var_2, false
        br i1 %var_3, label %block_1, label %block_6
        block_1:
        call void @X(ptr inttoptr (i64 2 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 1 to ptr))
        %var_6 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
        %var_7 = icmp eq i1 %var_6, false
        br i1 %var_7, label %block_2, label %block_5
        block_2:
        call void @X(ptr inttoptr (i64 1 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
        %var_9 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
        %var_10 = icmp eq i1 %var_9, false
        br i1 %var_10, label %block_3, label %block_4
        block_3:
        call void @X(ptr inttoptr (i64 0 to ptr))
        br label %block_4
        block_4:
        br label %block_5
        block_5:
        br label %block_6
        block_6:
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 5 to ptr))
        call void @__quantum__rt__array_record_output(i64 3, ptr @0)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @1)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @2)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @3)
        ret i64 0
    "#]],
        expect![[r#"
        block_0:
        call void @__quantum__rt__initialize(ptr null)
        call void @X(ptr inttoptr (i64 3 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 0 to ptr))
        %var_3 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        %var_4 = icmp eq i1 %var_3, false
        br i1 %var_4, label %block_1, label %block_6
        block_1:
        call void @X(ptr inttoptr (i64 2 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 1 to ptr))
        %var_6 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
        %var_7 = icmp eq i1 %var_6, false
        br i1 %var_7, label %block_2, label %block_5
        block_2:
        call void @X(ptr inttoptr (i64 1 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
        %var_9 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
        %var_10 = icmp eq i1 %var_9, false
        br i1 %var_10, label %block_3, label %block_4
        block_3:
        call void @X(ptr inttoptr (i64 0 to ptr))
        br label %block_4
        block_4:
        br label %block_5
        block_5:
        br label %block_6
        block_6:
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 5 to ptr))
        call void @__quantum__rt__array_record_output(i64 3, ptr @0)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @1)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @2)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @3)
        ret i64 0
    "#]],
    ];
    for (prepared, expected) in [false, true].into_iter().zip(expected) {
        let source = if prepared {
            source.replace("Rec(3, qs, r);", "X(r); Rec(3, qs, r);")
        } else {
            source.to_owned()
        };
        let qir = compile_source_to_qir(&source, *CAPABILITIES);
        assert_inlined(&qir, "Rec");
        expected.assert_eq(&recursive_entry_dataflow(&qir));
    }
}

#[test]
fn recursive_static_budget_three_preserves_retry_and_failure_paths() {
    let source = r#"
        operation Rus(budget : Int, q : Qubit, out : Qubit) : Unit {
            if budget > 0 {
                if MResetZ(q) == Zero { X(q); Rus(budget - 1, q, out); } else { X(out); }
            }
        }
        @EntryPoint()
        operation StaticBudget() : Result {
            use q = Qubit();
            use out = Qubit();
            Rus(3, q, out);
            MResetZ(out)
        }
    "#;
    let expected = [
        expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            call void @__quantum__rt__initialize(ptr null)
            call void @Rus(i64 3, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @0)
            ret i64 0
            }
            define internal void @Rus(i64 %var_0, ptr %var_1, ptr %var_2) {
            block_1:
            %var_3 = alloca i1
            %var_4 = alloca i64
            %var_9 = alloca i64
            %var_12 = alloca ptr
            %var_13 = alloca ptr
            store i1 false, ptr %var_3
            store i64 %var_0, ptr %var_4
            %var_16 = load i64, ptr %var_4
            %var_5 = icmp sgt i64 %var_16, 0
            br i1 %var_5, label %block_2, label %block_6
            block_2:
            call void @__quantum__qis__mresetz__body(ptr %var_1, ptr inttoptr (i64 0 to ptr))
            %var_6 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            %var_7 = icmp eq i1 %var_6, false
            store i1 %var_7, ptr %var_3
            %var_18 = load i1, ptr %var_3
            br i1 %var_18, label %block_3, label %block_4
            block_3:
            call void @X(ptr %var_1)
            store i64 %var_0, ptr %var_9
            %var_20 = load i64, ptr %var_9
            %var_10 = sub i64 %var_20, 1
            store ptr %var_1, ptr %var_12
            store ptr %var_2, ptr %var_13
            %var_23 = load ptr, ptr %var_12
            %var_24 = load ptr, ptr %var_13
            call void @Rus(i64 %var_10, ptr %var_23, ptr %var_24)
            br label %block_5
            block_4:
            call void @X(ptr %var_2)
            br label %block_5
            block_5:
            br label %block_6
            block_6:
            ret void
            }
        "#]],
        expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            call void @__quantum__rt__initialize(ptr null)
            call void @X(ptr inttoptr (i64 0 to ptr))
            call void @Rus(i64 3, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @0)
            ret i64 0
            }
            define internal void @Rus(i64 %var_1, ptr %var_2, ptr %var_3) {
            block_2:
            %var_4 = alloca i1
            %var_5 = alloca i64
            %var_9 = alloca i64
            %var_12 = alloca ptr
            %var_13 = alloca ptr
            store i1 false, ptr %var_4
            store i64 %var_1, ptr %var_5
            %var_16 = load i64, ptr %var_5
            %var_6 = icmp sgt i64 %var_16, 0
            br i1 %var_6, label %block_3, label %block_7
            block_3:
            call void @__quantum__qis__mresetz__body(ptr %var_2, ptr inttoptr (i64 0 to ptr))
            %var_7 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            %var_8 = icmp eq i1 %var_7, false
            store i1 %var_8, ptr %var_4
            %var_18 = load i1, ptr %var_4
            br i1 %var_18, label %block_4, label %block_5
            block_4:
            call void @X(ptr %var_2)
            store i64 %var_1, ptr %var_9
            %var_20 = load i64, ptr %var_9
            %var_10 = sub i64 %var_20, 1
            store ptr %var_2, ptr %var_12
            store ptr %var_3, ptr %var_13
            %var_23 = load ptr, ptr %var_12
            %var_24 = load ptr, ptr %var_13
            call void @Rus(i64 %var_10, ptr %var_23, ptr %var_24)
            br label %block_6
            block_5:
            call void @X(ptr %var_3)
            br label %block_6
            block_6:
            br label %block_7
            block_7:
            ret void
            }
        "#]],
    ];
    for (prepared, expected) in [false, true].into_iter().zip(expected) {
        let source = if prepared {
            source.replace("Rus(3, q, out);", "X(q); Rus(3, q, out);")
        } else {
            source.to_owned()
        };
        let qir = compile_source_to_qir(&source, *CAPABILITIES);
        expected.assert_eq(&recursive_emitted_dataflow(
            &qir,
            &["ENTRYPOINT__main", "Rus"],
        ));
    }
}

#[test]
fn recursive_static_budget_zero_skips_retry_measurements_and_gates() {
    let source = r#"
        operation Rus(budget : Int, q : Qubit, out : Qubit) : Unit {
            if budget > 0 {
                if MResetZ(q) == Zero { X(q); Rus(budget - 1, q, out); } else { X(out); }
            }
        }
        operation Main() : Result {
            use q = Qubit();
            use out = Qubit();
            Rus(0, q, out);
            Reset(q);
            MResetZ(out)
        }
    "#;
    let expected = [
        expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            call void @__quantum__rt__initialize(ptr null)
            call void @Rus(i64 0, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @Reset(ptr inttoptr (i64 0 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @0)
            ret i64 0
            }
            define internal void @Rus(i64 %var_0, ptr %var_1, ptr %var_2) {
            block_1:
            %var_3 = alloca i1
            %var_4 = alloca i64
            %var_9 = alloca i64
            %var_12 = alloca ptr
            %var_13 = alloca ptr
            store i1 false, ptr %var_3
            store i64 %var_0, ptr %var_4
            %var_16 = load i64, ptr %var_4
            %var_5 = icmp sgt i64 %var_16, 0
            br i1 %var_5, label %block_2, label %block_6
            block_2:
            call void @__quantum__qis__mresetz__body(ptr %var_1, ptr inttoptr (i64 0 to ptr))
            %var_6 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            %var_7 = icmp eq i1 %var_6, false
            store i1 %var_7, ptr %var_3
            %var_18 = load i1, ptr %var_3
            br i1 %var_18, label %block_3, label %block_4
            block_3:
            call void @X(ptr %var_1)
            store i64 %var_0, ptr %var_9
            %var_20 = load i64, ptr %var_9
            %var_10 = sub i64 %var_20, 1
            store ptr %var_1, ptr %var_12
            store ptr %var_2, ptr %var_13
            %var_23 = load ptr, ptr %var_12
            %var_24 = load ptr, ptr %var_13
            call void @Rus(i64 %var_10, ptr %var_23, ptr %var_24)
            br label %block_5
            block_4:
            call void @X(ptr %var_2)
            br label %block_5
            block_5:
            br label %block_6
            block_6:
            ret void
            }
        "#]],
        expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            call void @__quantum__rt__initialize(ptr null)
            call void @X(ptr inttoptr (i64 0 to ptr))
            call void @Rus(i64 0, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @Reset(ptr inttoptr (i64 0 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @0)
            ret i64 0
            }
            define internal void @Rus(i64 %var_1, ptr %var_2, ptr %var_3) {
            block_2:
            %var_4 = alloca i1
            %var_5 = alloca i64
            %var_9 = alloca i64
            %var_12 = alloca ptr
            %var_13 = alloca ptr
            store i1 false, ptr %var_4
            store i64 %var_1, ptr %var_5
            %var_16 = load i64, ptr %var_5
            %var_6 = icmp sgt i64 %var_16, 0
            br i1 %var_6, label %block_3, label %block_7
            block_3:
            call void @__quantum__qis__mresetz__body(ptr %var_2, ptr inttoptr (i64 0 to ptr))
            %var_7 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            %var_8 = icmp eq i1 %var_7, false
            store i1 %var_8, ptr %var_4
            %var_18 = load i1, ptr %var_4
            br i1 %var_18, label %block_4, label %block_5
            block_4:
            call void @X(ptr %var_2)
            store i64 %var_1, ptr %var_9
            %var_20 = load i64, ptr %var_9
            %var_10 = sub i64 %var_20, 1
            store ptr %var_2, ptr %var_12
            store ptr %var_3, ptr %var_13
            %var_23 = load ptr, ptr %var_12
            %var_24 = load ptr, ptr %var_13
            call void @Rus(i64 %var_10, ptr %var_23, ptr %var_24)
            br label %block_6
            block_5:
            call void @X(ptr %var_3)
            br label %block_6
            block_6:
            br label %block_7
            block_7:
            ret void
            }
        "#]],
    ];
    for (prepared, expected) in [false, true].into_iter().zip(expected) {
        let source = if prepared {
            source.replace("Rus(0, q, out);", "X(q); Rus(0, q, out);")
        } else {
            source.to_owned()
        };
        let qir = compile_source_to_qir(&source, *CAPABILITIES);
        expected.assert_eq(&recursive_emitted_dataflow(
            &qir,
            &["ENTRYPOINT__main", "Rus"],
        ));
    }
}

#[test]
fn recursive_static_budget_one_stops_after_first_attempt() {
    let source = r#"
        operation Rus(budget : Int, q : Qubit, out : Qubit) : Unit {
            if budget > 0 {
                if MResetZ(q) == Zero { X(q); Rus(budget - 1, q, out); } else { X(out); }
            }
        }
        operation Main() : Result {
            use q = Qubit();
            use out = Qubit();
            Rus(1, q, out);
            Reset(q);
            MResetZ(out)
        }
    "#;
    let expected = [
        expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            call void @__quantum__rt__initialize(ptr null)
            call void @Rus(i64 1, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @Reset(ptr inttoptr (i64 0 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @0)
            ret i64 0
            }
            define internal void @Rus(i64 %var_0, ptr %var_1, ptr %var_2) {
            block_1:
            %var_3 = alloca i1
            %var_4 = alloca i64
            %var_9 = alloca i64
            %var_12 = alloca ptr
            %var_13 = alloca ptr
            store i1 false, ptr %var_3
            store i64 %var_0, ptr %var_4
            %var_16 = load i64, ptr %var_4
            %var_5 = icmp sgt i64 %var_16, 0
            br i1 %var_5, label %block_2, label %block_6
            block_2:
            call void @__quantum__qis__mresetz__body(ptr %var_1, ptr inttoptr (i64 0 to ptr))
            %var_6 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            %var_7 = icmp eq i1 %var_6, false
            store i1 %var_7, ptr %var_3
            %var_18 = load i1, ptr %var_3
            br i1 %var_18, label %block_3, label %block_4
            block_3:
            call void @X(ptr %var_1)
            store i64 %var_0, ptr %var_9
            %var_20 = load i64, ptr %var_9
            %var_10 = sub i64 %var_20, 1
            store ptr %var_1, ptr %var_12
            store ptr %var_2, ptr %var_13
            %var_23 = load ptr, ptr %var_12
            %var_24 = load ptr, ptr %var_13
            call void @Rus(i64 %var_10, ptr %var_23, ptr %var_24)
            br label %block_5
            block_4:
            call void @X(ptr %var_2)
            br label %block_5
            block_5:
            br label %block_6
            block_6:
            ret void
            }
        "#]],
        expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            call void @__quantum__rt__initialize(ptr null)
            call void @X(ptr inttoptr (i64 0 to ptr))
            call void @Rus(i64 1, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @Reset(ptr inttoptr (i64 0 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @0)
            ret i64 0
            }
            define internal void @Rus(i64 %var_1, ptr %var_2, ptr %var_3) {
            block_2:
            %var_4 = alloca i1
            %var_5 = alloca i64
            %var_9 = alloca i64
            %var_12 = alloca ptr
            %var_13 = alloca ptr
            store i1 false, ptr %var_4
            store i64 %var_1, ptr %var_5
            %var_16 = load i64, ptr %var_5
            %var_6 = icmp sgt i64 %var_16, 0
            br i1 %var_6, label %block_3, label %block_7
            block_3:
            call void @__quantum__qis__mresetz__body(ptr %var_2, ptr inttoptr (i64 0 to ptr))
            %var_7 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            %var_8 = icmp eq i1 %var_7, false
            store i1 %var_8, ptr %var_4
            %var_18 = load i1, ptr %var_4
            br i1 %var_18, label %block_4, label %block_5
            block_4:
            call void @X(ptr %var_2)
            store i64 %var_1, ptr %var_9
            %var_20 = load i64, ptr %var_9
            %var_10 = sub i64 %var_20, 1
            store ptr %var_2, ptr %var_12
            store ptr %var_3, ptr %var_13
            %var_23 = load ptr, ptr %var_12
            %var_24 = load ptr, ptr %var_13
            call void @Rus(i64 %var_10, ptr %var_23, ptr %var_24)
            br label %block_6
            block_5:
            call void @X(ptr %var_3)
            br label %block_6
            block_6:
            br label %block_7
            block_7:
            ret void
            }
        "#]],
    ];
    for (prepared, expected) in [false, true].into_iter().zip(expected) {
        let source = if prepared {
            source.replace("Rus(1, q, out);", "X(q); Rus(1, q, out);")
        } else {
            source.to_owned()
        };
        let qir = compile_source_to_qir(&source, *CAPABILITIES);
        expected.assert_eq(&recursive_emitted_dataflow(
            &qir,
            &["ENTRYPOINT__main", "Rus"],
        ));
    }
}

#[test]
fn nested_for_over_qubit_slice_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Unit {
                use qs = Qubit[3];
                X(qs[0]);
                for _ in 1..2 {
                    for q in qs[1...] {
                        CNOT(qs[0], q);
                    }
                }
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"
        @array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          %var_3 = alloca i64
          %var_6 = alloca i1
          %var_7 = alloca i64
          %var_8 = alloca i64
          %var_17 = alloca i64
          %var_19 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @X(ptr inttoptr (i64 0 to ptr))
          store i64 1, ptr %var_2
          br label %block_1
        block_1:
          %var_22 = load i64, ptr %var_2
          store i64 %var_22, ptr %var_3
          %var_24 = load i64, ptr %var_3
          %var_4 = icmp sle i64 %var_24, 2
          store i1 true, ptr %var_6
          br i1 %var_4, label %block_2, label %block_3
        block_2:
          %var_27 = load i1, ptr %var_6
          br i1 %var_27, label %block_4, label %block_5
        block_3:
          store i1 false, ptr %var_6
          br label %block_2
        block_4:
          store i64 0, ptr %var_7
          br label %block_6
        block_5:
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        block_6:
          %var_29 = load i64, ptr %var_7
          store i64 %var_29, ptr %var_8
          %var_31 = load i64, ptr %var_8
          %var_9 = icmp slt i64 %var_31, 2
          br i1 %var_9, label %block_7, label %block_8
        block_7:
          %var_36 = load i64, ptr %var_7
          %var_37_offset_chk = icmp slt i64 %var_36, 0
          %var_37_offset = select i1 %var_37_offset_chk, i64 1, i64 0
          %var_37 = getelementptr [2 x ptr], ptr @array0, i64 %var_37_offset, i64 %var_36
          %var_10 = load ptr, ptr %var_37
          call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr %var_10)
          store i64 %var_36, ptr %var_17
          %var_39 = load i64, ptr %var_17
          %var_18 = add i64 %var_39, 1
          store i64 %var_18, ptr %var_7
          br label %block_6
        block_8:
          %var_32 = load i64, ptr %var_2
          store i64 %var_32, ptr %var_19
          %var_34 = load i64, ptr %var_19
          %var_20 = add i64 %var_34, 1
          store i64 %var_20, ptr %var_2
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @X(ptr %var_1) {
        block_9:
          call void @__quantum__qis__x__body(ptr %var_1)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        define internal void @CNOT(ptr %var_13, ptr %var_14) {
        block_10:
          call void @__quantum__qis__cx__body(ptr %var_13, ptr %var_14)
          ret void
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn constant_folding_pattern_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Result[] {
                use qs = Qubit[3];
                let iterations = 2;
                X(qs[0]);
                for _ in 1..iterations {
                    for q in qs[1...] {
                        CNOT(qs[0], q);
                    }
                }
                MResetEachZ(qs)
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"
        @3 = internal constant [6 x i8] c"3_a2r\00"
        @array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          %var_3 = alloca i64
          %var_6 = alloca i1
          %var_7 = alloca i64
          %var_8 = alloca i64
          %var_17 = alloca i64
          %var_19 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @X(ptr inttoptr (i64 0 to ptr))
          store i64 1, ptr %var_2
          br label %block_1
        block_1:
          %var_22 = load i64, ptr %var_2
          store i64 %var_22, ptr %var_3
          %var_24 = load i64, ptr %var_3
          %var_4 = icmp sle i64 %var_24, 2
          store i1 true, ptr %var_6
          br i1 %var_4, label %block_2, label %block_3
        block_2:
          %var_27 = load i1, ptr %var_6
          br i1 %var_27, label %block_4, label %block_5
        block_3:
          store i1 false, ptr %var_6
          br label %block_2
        block_4:
          store i64 0, ptr %var_7
          br label %block_6
        block_5:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 3, ptr @0)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
          ret i64 0
        block_6:
          %var_29 = load i64, ptr %var_7
          store i64 %var_29, ptr %var_8
          %var_31 = load i64, ptr %var_8
          %var_9 = icmp slt i64 %var_31, 2
          br i1 %var_9, label %block_7, label %block_8
        block_7:
          %var_36 = load i64, ptr %var_7
          %var_37_offset_chk = icmp slt i64 %var_36, 0
          %var_37_offset = select i1 %var_37_offset_chk, i64 1, i64 0
          %var_37 = getelementptr [2 x ptr], ptr @array0, i64 %var_37_offset, i64 %var_36
          %var_10 = load ptr, ptr %var_37
          call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr %var_10)
          store i64 %var_36, ptr %var_17
          %var_39 = load i64, ptr %var_17
          %var_18 = add i64 %var_39, 1
          store i64 %var_18, ptr %var_7
          br label %block_6
        block_8:
          %var_32 = load i64, ptr %var_2
          store i64 %var_32, ptr %var_19
          %var_34 = load i64, ptr %var_19
          %var_20 = add i64 %var_34, 1
          store i64 %var_20, ptr %var_2
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @X(ptr %var_1) {
        block_9:
          call void @__quantum__qis__x__body(ptr %var_1)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        define internal void @CNOT(ptr %var_13, ptr %var_14) {
        block_10:
          call void @__quantum__qis__cx__body(ptr %var_13, ptr %var_14)
          ret void
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn three_qubit_repetition_code_pattern_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            operation ApplyRotationalIdentity(register : Qubit[]) : Unit {
                let theta = 2.0 * 3.14159265;
                for qubit in register {
                    Rx(theta, qubit);
                }
            }
            @EntryPoint()
            operation Main() : Result[] {
                use qs = Qubit[3];
                X(qs[0]);
                let iterations = 2;
                for _ in 1..iterations {
                    for q in qs[1...] {
                        CNOT(qs[0], q);
                    }
                    ApplyRotationalIdentity(qs);
                }
                MResetEachZ(qs)
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"
        @3 = internal constant [6 x i8] c"3_a2r\00"
        @array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]
        @array1 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          %var_3 = alloca i64
          %var_6 = alloca i1
          %var_7 = alloca i64
          %var_8 = alloca i64
          %var_17 = alloca i64
          %var_19 = alloca i64
          %var_20 = alloca i64
          %var_29 = alloca i64
          %var_31 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @X(ptr inttoptr (i64 0 to ptr))
          store i64 1, ptr %var_2
          br label %block_1
        block_1:
          %var_34 = load i64, ptr %var_2
          store i64 %var_34, ptr %var_3
          %var_36 = load i64, ptr %var_3
          %var_4 = icmp sle i64 %var_36, 2
          store i1 true, ptr %var_6
          br i1 %var_4, label %block_2, label %block_3
        block_2:
          %var_39 = load i1, ptr %var_6
          br i1 %var_39, label %block_4, label %block_5
        block_3:
          store i1 false, ptr %var_6
          br label %block_2
        block_4:
          store i64 0, ptr %var_7
          br label %block_6
        block_5:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 3, ptr @0)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
          ret i64 0
        block_6:
          %var_41 = load i64, ptr %var_7
          store i64 %var_41, ptr %var_8
          %var_43 = load i64, ptr %var_8
          %var_9 = icmp slt i64 %var_43, 2
          br i1 %var_9, label %block_7, label %block_8
        block_7:
          %var_57 = load i64, ptr %var_7
          %var_58_offset_chk = icmp slt i64 %var_57, 0
          %var_58_offset = select i1 %var_58_offset_chk, i64 1, i64 0
          %var_58 = getelementptr [2 x ptr], ptr @array0, i64 %var_58_offset, i64 %var_57
          %var_10 = load ptr, ptr %var_58
          call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr %var_10)
          store i64 %var_57, ptr %var_17
          %var_60 = load i64, ptr %var_17
          %var_18 = add i64 %var_60, 1
          store i64 %var_18, ptr %var_7
          br label %block_6
        block_8:
          store i64 0, ptr %var_19
          br label %block_9
        block_9:
          %var_45 = load i64, ptr %var_19
          store i64 %var_45, ptr %var_20
          %var_47 = load i64, ptr %var_20
          %var_21 = icmp slt i64 %var_47, 3
          br i1 %var_21, label %block_10, label %block_11
        block_10:
          %var_52 = load i64, ptr %var_19
          %var_53_offset_chk = icmp slt i64 %var_52, 0
          %var_53_offset = select i1 %var_53_offset_chk, i64 1, i64 0
          %var_53 = getelementptr [3 x ptr], ptr @array1, i64 %var_53_offset, i64 %var_52
          %var_22 = load ptr, ptr %var_53
          call void @Rx(double 6.2831853, ptr %var_22)
          store i64 %var_52, ptr %var_29
          %var_55 = load i64, ptr %var_29
          %var_30 = add i64 %var_55, 1
          store i64 %var_30, ptr %var_19
          br label %block_9
        block_11:
          %var_48 = load i64, ptr %var_2
          store i64 %var_48, ptr %var_31
          %var_50 = load i64, ptr %var_31
          %var_32 = add i64 %var_50, 1
          store i64 %var_32, ptr %var_2
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @X(ptr %var_1) {
        block_12:
          call void @__quantum__qis__x__body(ptr %var_1)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        define internal void @CNOT(ptr %var_13, ptr %var_14) {
        block_13:
          call void @__quantum__qis__cx__body(ptr %var_13, ptr %var_14)
          ret void
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)

        define internal void @Rx(double %var_25, ptr %var_26) {
        block_14:
          call void @__quantum__qis__rx__body(double %var_25, ptr %var_26)
          ret void
        }

        declare void @__quantum__qis__rx__body(double, ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn for_over_qubit_slice_inside_dynamic_while_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Unit {
                use qs = Qubit[3];
                mutable done = false;
                while not done {
                    for q in qs[1...] {
                        CNOT(qs[0], q);
                    }
                    set done = MResetZ(qs[0]) == One;
                }
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"
        @array0 = internal constant [2 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca i1
          %var_3 = alloca i64
          %var_4 = alloca i64
          %var_13 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i1 false, ptr %var_1
          br label %block_1
        block_1:
          %var_17 = load i1, ptr %var_1
          %var_2 = xor i1 %var_17, true
          br i1 %var_2, label %block_2, label %block_3
        block_2:
          store i64 0, ptr %var_3
          br label %block_4
        block_3:
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        block_4:
          %var_19 = load i64, ptr %var_3
          store i64 %var_19, ptr %var_4
          %var_21 = load i64, ptr %var_4
          %var_5 = icmp slt i64 %var_21, 2
          br i1 %var_5, label %block_5, label %block_6
        block_5:
          %var_23 = load i64, ptr %var_3
          %var_24_offset_chk = icmp slt i64 %var_23, 0
          %var_24_offset = select i1 %var_24_offset_chk, i64 1, i64 0
          %var_24 = getelementptr [2 x ptr], ptr @array0, i64 %var_24_offset, i64 %var_23
          %var_6 = load ptr, ptr %var_24
          call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr %var_6)
          store i64 %var_23, ptr %var_13
          %var_26 = load i64, ptr %var_13
          %var_14 = add i64 %var_26, 1
          store i64 %var_14, ptr %var_3
          br label %block_4
        block_6:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_15 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          store i1 %var_15, ptr %var_1
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @CNOT(ptr %var_9, ptr %var_10) {
        block_7:
          call void @__quantum__qis__cx__body(ptr %var_9, ptr %var_10)
          ret void
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn result_array_dynamic_index_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Int {
                use qs = Qubit[4];
                let results = MResetEachZ(qs);
                mutable count = 0;
                for i in 0..3 {
                    if results[i] == One {
                        set count += 1;
                    }
                }
                count
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"
        @array0 = internal constant [4 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          %var_3 = alloca i64
          %var_4 = alloca i64
          %var_7 = alloca i1
          %var_8 = alloca i64
          %var_13 = alloca i64
          %var_15 = alloca i64
          %var_17 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
          store i64 0, ptr %var_2
          store i64 0, ptr %var_3
          br label %block_1
        block_1:
          %var_20 = load i64, ptr %var_3
          store i64 %var_20, ptr %var_4
          %var_22 = load i64, ptr %var_4
          %var_5 = icmp sle i64 %var_22, 3
          store i1 true, ptr %var_7
          br i1 %var_5, label %block_2, label %block_3
        block_2:
          %var_25 = load i1, ptr %var_7
          br i1 %var_25, label %block_4, label %block_5
        block_3:
          store i1 false, ptr %var_7
          br label %block_2
        block_4:
          %var_29 = load i64, ptr %var_3
          store i64 %var_29, ptr %var_8
          %var_31 = load i64, ptr %var_8
          %var_32_offset_chk = icmp slt i64 %var_31, 0
          %var_32_offset = select i1 %var_32_offset_chk, i64 1, i64 0
          %var_32 = getelementptr [4 x ptr], ptr @array0, i64 %var_32_offset, i64 %var_31
          %var_9 = load ptr, ptr %var_32
          %var_11 = call zeroext i1 @__quantum__rt__read_result(ptr %var_9)
          br i1 %var_11, label %block_6, label %block_7
        block_5:
          %var_26 = load i64, ptr %var_2
          store i64 %var_26, ptr %var_17
          %var_28 = load i64, ptr %var_17
          call void @__quantum__rt__int_record_output(i64 %var_28, ptr @0)
          ret i64 0
        block_6:
          %var_37 = load i64, ptr %var_2
          store i64 %var_37, ptr %var_13
          %var_39 = load i64, ptr %var_13
          %var_14 = add i64 %var_39, 1
          store i64 %var_14, ptr %var_2
          br label %block_7
        block_7:
          %var_33 = load i64, ptr %var_3
          store i64 %var_33, ptr %var_15
          %var_35 = load i64, ptr %var_15
          %var_16 = add i64 %var_35, 1
          store i64 %var_16, ptr %var_3
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn result_array_while_loop_dynamic_index_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Int {
                use qs = Qubit[4];
                H(qs[0]);
                H(qs[1]);
                H(qs[2]);
                H(qs[3]);
                let r0 = MResetZ(qs[0]);
                let r1 = MResetZ(qs[1]);
                let r2 = MResetZ(qs[2]);
                let r3 = MResetZ(qs[3]);
                let results = [r0, r1, r2, r3];
                mutable count = 0;
                mutable i = 0;
                while i < 4 {
                    if results[i] == One { set count += 1; }
                    set i += 1;
                }
                count
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"
        @array0 = internal constant [4 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_2 = alloca i64
          %var_3 = alloca i64
          %var_4 = alloca i64
          %var_10 = alloca i64
          %var_12 = alloca i64
          %var_14 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          call void @H(ptr inttoptr (i64 0 to ptr))
          call void @H(ptr inttoptr (i64 1 to ptr))
          call void @H(ptr inttoptr (i64 2 to ptr))
          call void @H(ptr inttoptr (i64 3 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
          store i64 0, ptr %var_2
          store i64 0, ptr %var_3
          br label %block_1
        block_1:
          %var_17 = load i64, ptr %var_3
          store i64 %var_17, ptr %var_4
          %var_19 = load i64, ptr %var_4
          %var_5 = icmp slt i64 %var_19, 4
          br i1 %var_5, label %block_2, label %block_3
        block_2:
          %var_23 = load i64, ptr %var_3
          %var_24_offset_chk = icmp slt i64 %var_23, 0
          %var_24_offset = select i1 %var_24_offset_chk, i64 1, i64 0
          %var_24 = getelementptr [4 x ptr], ptr @array0, i64 %var_24_offset, i64 %var_23
          %var_6 = load ptr, ptr %var_24
          %var_8 = call zeroext i1 @__quantum__rt__read_result(ptr %var_6)
          br i1 %var_8, label %block_4, label %block_5
        block_3:
          %var_20 = load i64, ptr %var_2
          store i64 %var_20, ptr %var_14
          %var_22 = load i64, ptr %var_14
          call void @__quantum__rt__int_record_output(i64 %var_22, ptr @0)
          ret i64 0
        block_4:
          %var_29 = load i64, ptr %var_2
          store i64 %var_29, ptr %var_10
          %var_31 = load i64, ptr %var_10
          %var_11 = add i64 %var_31, 1
          store i64 %var_11, ptr %var_2
          br label %block_5
        block_5:
          %var_25 = load i64, ptr %var_3
          store i64 %var_25, ptr %var_12
          %var_27 = load i64, ptr %var_12
          %var_13 = add i64 %var_27, 1
          store i64 %var_13, ptr %var_3
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @H(ptr %var_1) {
        block_6:
          call void @__quantum__qis__h__body(ptr %var_1)
          ret void
        }

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn mutable_result_variable_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                H(q);
                mutable r = M(q);
                if r == One {
                    X(q);
                    set r = M(q);
                }
                r
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca ptr
          %var_5 = alloca ptr
          call void @__quantum__rt__initialize(ptr null)
          call void @H(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          store ptr inttoptr (i64 0 to ptr), ptr %var_1
          %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_2, label %block_1, label %block_2
        block_1:
          call void @X(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          store ptr inttoptr (i64 1 to ptr), ptr %var_1
          br label %block_2
        block_2:
          %var_7 = load ptr, ptr %var_1
          store ptr %var_7, ptr %var_5
          %var_9 = load ptr, ptr %var_5
          call void @__quantum__rt__result_record_output(ptr %var_9, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @H(ptr %var_0) {
        block_3:
          call void @__quantum__qis__h__body(ptr %var_0)
          ret void
        }

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__qis__m__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        define internal void @X(ptr %var_4) {
        block_4:
          call void @__quantum__qis__x__body(ptr %var_4)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="2" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn result_variables_with_default_values_succeeds() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                use q = Qubit();
                mutable r1 = Zero;
                mutable r2 = One;
                if M(q) == One {
                    r1 = One;
                    r2 = Zero;
                }
                Reset(q);
                (r1, r2)
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"
        @1 = internal constant [6 x i8] c"1_t0r\00"
        @2 = internal constant [6 x i8] c"2_t1r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_0 = alloca ptr
          %var_1 = alloca ptr
          %var_6 = alloca ptr
          %var_7 = alloca ptr
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__rt__write_result(i1 zeroext false, ptr inttoptr (i64 1 to ptr))
          call void @__quantum__rt__write_result(i1 zeroext true, ptr inttoptr (i64 2 to ptr))
          store ptr inttoptr (i64 1 to ptr), ptr %var_0
          store ptr inttoptr (i64 2 to ptr), ptr %var_1
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_2, label %block_1, label %block_2
        block_1:
          store ptr inttoptr (i64 2 to ptr), ptr %var_0
          store ptr inttoptr (i64 1 to ptr), ptr %var_1
          br label %block_2
        block_2:
          call void @Reset(ptr inttoptr (i64 0 to ptr))
          %var_10 = load ptr, ptr %var_0
          store ptr %var_10, ptr %var_6
          %var_12 = load ptr, ptr %var_1
          store ptr %var_12, ptr %var_7
          call void @__quantum__rt__tuple_record_output(i64 2, ptr @0)
          %var_14 = load ptr, ptr %var_6
          call void @__quantum__rt__result_record_output(ptr %var_14, ptr @1)
          %var_15 = load ptr, ptr %var_7
          call void @__quantum__rt__result_record_output(ptr %var_15, ptr @2)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__m__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        define internal void @Reset(ptr %var_5) {
        block_3:
          call void @__quantum__qis__reset__body(ptr %var_5)
          ret void
        }

        declare void @__quantum__qis__reset__body(ptr) #1

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        declare void @__quantum__rt__write_result(i1 zeroext, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="3" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn for_loop_over_qubits_with_reset_all_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Result {
                use qs = Qubit[4];
                for q in qs {
                    H(q);
                }
                let r = MResetZ(qs[0]);
                ResetAll(qs[1..3]);
                r
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_r\00"
        @array0 = internal constant [4 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]
        @array1 = internal constant [3 x ptr] [ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca i64
          %var_2 = alloca i64
          %var_7 = alloca i64
          %var_9 = alloca i64
          %var_10 = alloca i64
          %var_15 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i64 0, ptr %var_1
          br label %block_1
        block_1:
          %var_18 = load i64, ptr %var_1
          store i64 %var_18, ptr %var_2
          %var_20 = load i64, ptr %var_2
          %var_3 = icmp slt i64 %var_20, 4
          br i1 %var_3, label %block_2, label %block_3
        block_2:
          %var_30 = load i64, ptr %var_1
          %var_31_offset_chk = icmp slt i64 %var_30, 0
          %var_31_offset = select i1 %var_31_offset_chk, i64 1, i64 0
          %var_31 = getelementptr [4 x ptr], ptr @array0, i64 %var_31_offset, i64 %var_30
          %var_4 = load ptr, ptr %var_31
          call void @H(ptr %var_4)
          store i64 %var_30, ptr %var_7
          %var_33 = load i64, ptr %var_7
          %var_8 = add i64 %var_33, 1
          store i64 %var_8, ptr %var_1
          br label %block_1
        block_3:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          store i64 0, ptr %var_9
          br label %block_4
        block_4:
          %var_22 = load i64, ptr %var_9
          store i64 %var_22, ptr %var_10
          %var_24 = load i64, ptr %var_10
          %var_11 = icmp slt i64 %var_24, 3
          br i1 %var_11, label %block_5, label %block_6
        block_5:
          %var_25 = load i64, ptr %var_9
          %var_26_offset_chk = icmp slt i64 %var_25, 0
          %var_26_offset = select i1 %var_26_offset_chk, i64 1, i64 0
          %var_26 = getelementptr [3 x ptr], ptr @array1, i64 %var_26_offset, i64 %var_25
          %var_12 = load ptr, ptr %var_26
          call void @Reset(ptr %var_12)
          store i64 %var_25, ptr %var_15
          %var_28 = load i64, ptr %var_15
          %var_16 = add i64 %var_28, 1
          store i64 %var_16, ptr %var_9
          br label %block_4
        block_6:
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @H(ptr %var_6) {
        block_7:
          call void @__quantum__qis__h__body(ptr %var_6)
          ret void
        }

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        define internal void @Reset(ptr %var_14) {
        block_8:
          call void @__quantum__qis__reset__body(ptr %var_14)
          ret void
        }

        declare void @__quantum__qis__reset__body(ptr) #1

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn measure_each_z_static_qubits_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Result[] {
                use qs = Qubit[3];
                X(qs[0]);
                H(qs[1]);
                MResetEachZ(qs)
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"
        @3 = internal constant [6 x i8] c"3_a2r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @X(ptr inttoptr (i64 0 to ptr))
          call void @H(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 3, ptr @0)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @3)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @X(ptr %var_1) {
        block_1:
          call void @__quantum__qis__x__body(ptr %var_1)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        define internal void @H(ptr %var_2) {
        block_2:
          call void @__quantum__qis__h__body(ptr %var_2)
          ret void
        }

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn static_while_inside_emit_while_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable total = 0;
                while MResetZ(q) == One {
                    mutable idx = 0;
                    while idx < 3 {
                        set total += 1;
                        set idx += 1;
                    }
                }
                total
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_0 = alloca i64
          %var_3 = alloca i64
          %var_4 = alloca i64
          %var_6 = alloca i64
          %var_8 = alloca i64
          %var_10 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i64 0, ptr %var_0
          br label %block_1
        block_1:
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_1, label %block_2, label %block_3
        block_2:
          store i64 0, ptr %var_3
          br label %block_4
        block_3:
          %var_12 = load i64, ptr %var_0
          store i64 %var_12, ptr %var_10
          %var_14 = load i64, ptr %var_10
          call void @__quantum__rt__int_record_output(i64 %var_14, ptr @0)
          ret i64 0
        block_4:
          %var_16 = load i64, ptr %var_3
          store i64 %var_16, ptr %var_4
          %var_18 = load i64, ptr %var_4
          %var_5 = icmp slt i64 %var_18, 3
          br i1 %var_5, label %block_5, label %block_6
        block_5:
          %var_19 = load i64, ptr %var_0
          store i64 %var_19, ptr %var_6
          %var_21 = load i64, ptr %var_6
          %var_7 = add i64 %var_21, 1
          store i64 %var_7, ptr %var_0
          %var_23 = load i64, ptr %var_3
          store i64 %var_23, ptr %var_8
          %var_25 = load i64, ptr %var_8
          %var_9 = add i64 %var_25, 1
          store i64 %var_9, ptr %var_3
          br label %block_4
        block_6:
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn nested_emit_while_loops_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Int {
                use qs = Qubit[2];
                mutable outer = 0;
                while outer < 3 {
                    H(qs[0]);
                    mutable inner = 0;
                    while inner < 2 {
                        H(qs[1]);
                        set inner += 1;
                    }
                    set outer += 1;
                }
                outer
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_1 = alloca i64
          %var_2 = alloca i64
          %var_5 = alloca i64
          %var_6 = alloca i64
          %var_8 = alloca i64
          %var_10 = alloca i64
          %var_12 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i64 0, ptr %var_1
          br label %block_1
        block_1:
          %var_14 = load i64, ptr %var_1
          store i64 %var_14, ptr %var_2
          %var_16 = load i64, ptr %var_2
          %var_3 = icmp slt i64 %var_16, 3
          br i1 %var_3, label %block_2, label %block_3
        block_2:
          call void @H(ptr inttoptr (i64 0 to ptr))
          store i64 0, ptr %var_5
          br label %block_4
        block_3:
          %var_17 = load i64, ptr %var_1
          store i64 %var_17, ptr %var_12
          %var_19 = load i64, ptr %var_12
          call void @__quantum__rt__int_record_output(i64 %var_19, ptr @0)
          ret i64 0
        block_4:
          %var_21 = load i64, ptr %var_5
          store i64 %var_21, ptr %var_6
          %var_23 = load i64, ptr %var_6
          %var_7 = icmp slt i64 %var_23, 2
          br i1 %var_7, label %block_5, label %block_6
        block_5:
          call void @H(ptr inttoptr (i64 1 to ptr))
          %var_28 = load i64, ptr %var_5
          store i64 %var_28, ptr %var_8
          %var_30 = load i64, ptr %var_8
          %var_9 = add i64 %var_30, 1
          store i64 %var_9, ptr %var_5
          br label %block_4
        block_6:
          %var_24 = load i64, ptr %var_1
          store i64 %var_24, ptr %var_10
          %var_26 = load i64, ptr %var_10
          %var_11 = add i64 %var_26, 1
          store i64 %var_11, ptr %var_1
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @H(ptr %var_4) {
        block_7:
          call void @__quantum__qis__h__body(ptr %var_4)
          ret void
        }

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
            .assert_eq(&qir);
}

#[test]
fn for_loop_over_qubits_with_dynamic_exit_succeeds() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            @EntryPoint()
            operation Main() : Bool {
                use qs = Qubit[3];
                mutable found = false;
                for q in qs {
                    H(q);
                    if MResetZ(q) == One {
                        found = true;
                    }
                }
                found
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_b\00"
        @array0 = internal constant [3 x ptr] [ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr)]

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_0 = alloca i1
          %var_2 = alloca i1
          %var_3 = alloca i64
          %var_4 = alloca i64
          %var_11 = alloca i64
          %var_13 = alloca i1
          call void @__quantum__rt__initialize(ptr null)
          store i1 false, ptr %var_0
          store i1 false, ptr %var_2
          store i64 0, ptr %var_3
          br label %block_1
        block_1:
          %var_17 = load i64, ptr %var_3
          store i64 %var_17, ptr %var_4
          %var_19 = load i64, ptr %var_4
          %var_5 = icmp slt i64 %var_19, 3
          br i1 %var_5, label %block_2, label %block_3
        block_2:
          %var_23 = load i64, ptr %var_3
          %var_24_offset_chk = icmp slt i64 %var_23, 0
          %var_24_offset = select i1 %var_24_offset_chk, i64 1, i64 0
          %var_24 = getelementptr [3 x ptr], ptr @array0, i64 %var_24_offset, i64 %var_23
          %var_6 = load ptr, ptr %var_24
          call void @H(ptr %var_6)
          call void @__quantum__qis__mresetz__body(ptr %var_6, ptr inttoptr (i64 0 to ptr))
          %var_9 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          store i1 %var_9, ptr %var_0
          %var_26 = load i1, ptr %var_0
          br i1 %var_26, label %block_4, label %block_5
        block_3:
          %var_20 = load i1, ptr %var_2
          store i1 %var_20, ptr %var_13
          %var_22 = load i1, ptr %var_13
          call void @__quantum__rt__bool_record_output(i1 zeroext %var_22, ptr @0)
          ret i64 0
        block_4:
          store i1 true, ptr %var_2
          br label %block_5
        block_5:
          %var_27 = load i64, ptr %var_3
          store i64 %var_27, ptr %var_11
          %var_29 = load i64, ptr %var_11
          %var_12 = add i64 %var_29, 1
          store i64 %var_12, ptr %var_3
          br label %block_1
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @H(ptr %var_8) {
        block_6:
          call void @__quantum__qis__h__body(ptr %var_8)
          ret void
        }

        declare void @__quantum__qis__h__body(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__bool_record_output(i1 zeroext, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
            .assert_eq(&qir);
}

/// Regression test for a defunctionalization capture-resolution bug where a
/// partial-application closure returned across a function boundary threaded
/// the wrong captured value into the specialized callable. This mirrors the
/// Bernstein-Vazirani sample shape: `MakeParity` returns
/// `ApplyParity(secret, _, _)` (capturing `secret`), which is then invoked
/// through the `Apply` higher-order operation. The captured `secret` (5 =
/// 0b101) must drive which `CNOT`s fire — controls on query qubits 0 and 2,
/// each targeting the shared ancilla. Before the fix, the capture was
/// resolved to a caller-scope qubit, corrupting the CNOT operands.
#[test]
fn cross_function_partial_application_capture_threads_correct_value() {
    let source = "namespace Test {
            import Std.Intrinsic.*;
            operation ApplyParity(secret : Int, query : Qubit[], target : Qubit) : Unit {
                if (secret &&& 1) != 0 {
                    CNOT(query[0], target);
                }
                if (secret &&& 2) != 0 {
                    CNOT(query[1], target);
                }
                if (secret &&& 4) != 0 {
                    CNOT(query[2], target);
                }
            }
            function MakeParity(secret : Int) : ((Qubit[], Qubit) => Unit) {
                return ApplyParity(secret, _, _);
            }
            operation Apply(f : ((Qubit[], Qubit) => Unit), query : Qubit[], target : Qubit) : Unit {
                f(query, target);
            }
            @EntryPoint()
            operation Main() : Unit {
                use query = Qubit[3];
                use target = Qubit();
                let parity = MakeParity(5);
                Apply(parity, query, target);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    // secret = 5 (0b101) folds at compile time -> CNOT(query[0], target) and
    // CNOT(query[2], target). query qubits are 0,1,2 and target is qubit 3, so
    // both CNOTs use constant operands and share target qubit 3. The cross-package
    // `CNOT` operation is emitted as its own IR function, so the constant operands
    // appear at the `call void @CNOT(...)` site. Before the fix the captured
    // `secret` resolved to a caller-scope qubit, corrupting the operands (and the
    // bit selection).
    assert!(
        qir.contains("call void @CNOT(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))"),
        "expected CNOT(query[0]=0, target=3), got:\n{qir}"
    );
    assert!(
        qir.contains("call void @CNOT(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))"),
        "expected CNOT(query[2]=2, target=3), got:\n{qir}"
    );
    assert!(
        !qir.contains("call void @CNOT(ptr inttoptr (i64 1 to ptr),"),
        "secret 0b101 must not fire CNOT on query[1], got:\n{qir}"
    );
}

/// A simple void user operation called from the entry point emits an
/// `ir_functions` module flag, a `define void @ApplyX`, and a `call void`.
#[test]
fn simple_void_operation_emits_ir_function() {
    let source = "namespace Test {
            operation ApplyX(q : Qubit) : Unit {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyX(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @ApplyX(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @ApplyX(ptr %var_0) {
        block_1:
          call void @X(ptr %var_0)
          ret void
        }

        define internal void @X(ptr %var_1) {
        block_2:
          call void @__quantum__qis__x__body(ptr %var_1)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

/// Two call sites of the same operation share one `define` and emit two
/// `call void`s.
#[test]
fn two_call_sites_share_one_ir_function() {
    let source = "namespace Test {
            operation ApplyX(q : Qubit) : Unit {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyX(q);
                ApplyX(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    // Exactly one definition shared by two call sites.
    assert_eq!(
        qir.matches("define internal void @ApplyX(").count(),
        1,
        "expected a single shared IR function definition; got:\n{qir}"
    );
    assert_eq!(
        qir.matches("call void @ApplyX(").count(),
        2,
        "expected two call sites to the shared IR function; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @ApplyX(ptr inttoptr (i64 0 to ptr))
          call void @ApplyX(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @ApplyX(ptr %var_0) {
        block_1:
          call void @X(ptr %var_0)
          ret void
        }

        define internal void @X(ptr %var_1) {
        block_2:
          call void @__quantum__qis__x__body(ptr %var_1)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

/// `body` + `Adjoint` calls emit distinct functions named by the
/// `FunctorSetValue` mangle (`Op` and `Op__Adj`).
#[test]
fn body_and_adjoint_emit_distinct_ir_functions() {
    let source = "namespace Test {
            operation Op(q : Qubit) : Unit is Adj {
                Rx(1.0, q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Op(q);
                Adjoint Op(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define internal void @Op("),
        "expected a body IR function named `Op`; got:\n{qir}"
    );
    assert!(
        qir.contains("define internal void @Op__Adj("),
        "expected an adjoint IR function named `Op__Adj`; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @Op(ptr inttoptr (i64 0 to ptr))
          call void @Op__Adj(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @Op(ptr %var_0) {
        block_1:
          call void @Rx(double 1.0, ptr %var_0)
          ret void
        }

        define internal void @Rx(double %var_2, ptr %var_3) {
        block_2:
          call void @__quantum__qis__rx__body(double %var_2, ptr %var_3)
          ret void
        }

        declare void @__quantum__qis__rx__body(double, ptr)

        define internal void @Op__Adj(ptr %var_6) {
        block_3:
          call void @Rx__Adj(double 1.0, ptr %var_6)
          ret void
        }

        define internal void @Rx__Adj(double %var_8, ptr %var_9) {
        block_4:
          %var_10 = fmul double -1.0, %var_8
          call void @Rx(double %var_10, ptr %var_9)
          ret void
        }

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

/// A generic higher-order helper should still emit as an IR function after
/// monomorphization (`'T` -> `Qubit`) and defunctionalization (operation
/// parameter lowered away), and the entry point should call that emitted
/// helper rather than inlining it.
#[test]
fn defunctionalized_monomorphized_helper_emits_ir_function() {
    let source = "namespace Test {
            operation ApplyGeneric<'T>(op : ('T => Unit), x : 'T) : Unit {
                op(x);
            }

            operation UseGeneric(q : Qubit) : Unit {
                ApplyGeneric(X, q);
            }

            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                UseGeneric(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);

    assert!(
        qir.contains("define internal void @UseGeneric("),
        "expected emitted helper function for the specialized call path; got:\n{qir}"
    );
    assert!(
        qir.contains("call void @UseGeneric("),
        "expected entry point to call emitted specialized helper; got:\n{qir}"
    );
    assert!(
        qir.contains("define internal void @\"ApplyGeneric"),
        "expected specialized ApplyGeneric IR function definition; got:\n{qir}"
    );
    assert!(
        qir.contains("call void @\"ApplyGeneric"),
        "expected call into specialized ApplyGeneric IR function; got:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected lowered intrinsic call in specialized helper body; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @UseGeneric(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @UseGeneric(ptr %var_0) {
        block_1:
          call void @"ApplyGeneric<Qubit, AdjCtl>{X}"(ptr %var_0)
          ret void
        }

        define internal void @"ApplyGeneric<Qubit, AdjCtl>{X}"(ptr %var_1) {
        block_2:
          call void @X(ptr %var_1)
          ret void
        }

        define internal void @X(ptr %var_2) {
        block_3:
          call void @__quantum__qis__x__body(ptr %var_2)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

/// With the internal `DynamicQubitAllocation` flag ON, a qubit-allocating
/// callable may emit as an IR function rather than inlining.
#[test]
fn qubit_allocating_callable_emits_ir_function_when_dynamic_alloc_enabled() {
    let source = "namespace Test {
            operation AllocAndX() : Unit {
                use a = Qubit();
                X(a);
            }
            @EntryPoint()
            operation Main() : Unit {
                AllocAndX();
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES_DYNAMIC_QUBIT_ALLOC);
    assert!(
        qir.contains("define internal void @AllocAndX("),
        "expected a qubit-allocating IR function when DynamicQubitAllocation is enabled; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @AllocAndX()
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @AllocAndX() {
        block_1:
          %var_0 = call ptr @__quantum__rt__qubit_allocate()
          call void @X(ptr %var_0)
          call void @__quantum__rt__qubit_release(ptr %var_0)
          ret void
        }

        declare ptr @__quantum__rt__qubit_allocate()

        define internal void @X(ptr %var_2) {
        block_2:
          call void @__quantum__qis__x__body(ptr %var_2)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__qubit_release(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 true}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

/// With the internal `DynamicQubitAllocation` flag ON, a callable that
/// allocates a qubit array may emit as an IR function rather than inlining.
#[test]
fn qubit_array_allocating_callable_emits_ir_function_when_dynamic_alloc_enabled() {
    let source = "namespace Test {
            operation AllocArrayAndX() : Unit {
                use qs = Qubit[2];
                X(qs[0]);
                X(qs[1]);
            }
            @EntryPoint()
            operation Main() : Unit {
                AllocArrayAndX();
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES_DYNAMIC_QUBIT_ALLOC);
    assert!(
        qir.contains("define internal void @AllocArrayAndX("),
        "expected a qubit-array-allocating IR function when DynamicQubitAllocation is enabled; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    assert!(
        qir.contains("qubit_allocate"),
        "expected qubit allocation in the emitted IR function; got:\n{qir}"
    );
    assert!(
        qir.contains("qubit_release"),
        "expected qubit release in the emitted IR function; got:\n{qir}"
    );
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @AllocArrayAndX()
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @AllocArrayAndX() {
        block_1:
          %var_1 = call ptr @__quantum__rt__qubit_allocate()
          %var_3 = call ptr @__quantum__rt__qubit_allocate()
          call void @X(ptr %var_1)
          call void @X(ptr %var_3)
          call void @__quantum__rt__qubit_release(ptr %var_1)
          call void @__quantum__rt__qubit_release(ptr %var_3)
          ret void
        }

        declare ptr @__quantum__rt__qubit_allocate()

        define internal void @X(ptr %var_5) {
        block_2:
          call void @__quantum__qis__x__body(ptr %var_5)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__qubit_release(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 true}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

// ---- Negative cases: callables that must inline (no IR function) ----

fn assert_inlined(qir: &str, callable_name: &str) {
    assert!(
        !qir.contains(&format!("define internal void @{callable_name}(")),
        "expected `{callable_name}` to inline (no IR function definition); got:\n{qir}"
    );
}

/// An operation with a composite-leaf parameter (an array, which has no
/// flattenable scalar representation) must inline.
#[test]
fn composite_signature_operation_inlines() {
    let source = "namespace Test {
            operation ApplyAll(qs : Qubit[]) : Unit {
                for q in qs {
                    X(q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use qs = Qubit[2];
                ApplyAll(qs);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "ApplyAll");
}

/// A tuple parameter whose leaves are all scalars/qubits is FLATTENED into
/// individual scalar/qubit parameters and emitted as an IR function (the
/// eligibility predicate rejects composite LEAVES, e.g. arrays, not
/// flattenable tuples-of-scalars). This pins the implemented flattening
/// behavior so a regression is caught.
#[test]
fn tuple_of_scalars_parameter_flattens_to_ir_function() {
    let source = "namespace Test {
            operation ApplyPair(qs : (Qubit, Qubit)) : Unit {
                let (a, b) = qs;
                X(a);
                X(b);
            }
            @EntryPoint()
            operation Main() : Unit {
                use a = Qubit();
                use b = Qubit();
                ApplyPair((a, b));
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define internal void @ApplyPair("),
        "expected a flattened tuple-of-qubits IR function; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @ApplyPair(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @ApplyPair(ptr %var_0, ptr %var_1) {
        block_1:
          call void @X(ptr %var_0)
          call void @X(ptr %var_1)
          ret void
        }

        define internal void @X(ptr %var_4) {
        block_2:
          call void @__quantum__qis__x__body(ptr %var_4)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

/// A qubit-allocating callable inlines with `DynamicQubitAllocation` OFF
/// (the default for `Profile::Adaptive`); ancillas fold into
/// `required_num_qubits`.
#[test]
fn qubit_allocating_callable_inlines_when_dynamic_alloc_disabled() {
    let source = "namespace Test {
            operation AllocAndX() : Unit {
                use a = Qubit();
                X(a);
            }
            @EntryPoint()
            operation Main() : Unit {
                AllocAndX();
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "AllocAndX");
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected the allocated-qubit body to inline into the entry point; got:\n{qir}"
    );
}

/// A `Controlled` call inlines: the controlled specialization takes a
/// synthesized dynamic-length `Qubit[]` control register that has no
/// base-phase RIR representation, so it is never emitted as an IR function.
#[test]
fn controlled_specialization_inlines() {
    let source = "namespace Test {
            operation Op(q : Qubit) : Unit is Ctl {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use ctl = Qubit();
                use target = Qubit();
                Controlled Op([ctl], target);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        !qir.contains("define internal void @Op__Ctl("),
        "expected the controlled specialization to inline; got:\n{qir}"
    );
    assert!(
        !qir.contains("define internal void @Op("),
        "expected no IR function for the uncalled body specialization; got:\n{qir}"
    );
}

#[test]
fn recursive_static_allocation_inlines() {
    let source = r#"
        operation Rec(n : Int) : Unit {
            use q = Qubit();
            H(q);
            if n > 0 { Rec(n - 1); }
            Reset(q);
        }
        @EntryPoint() operation Main() : Unit { Rec(2); }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "Rec");
    assert!(qir.contains("\"required_num_qubits\"=\"3\""), "{qir}");

    let dynamic = compile_source_to_qir(source, *CAPABILITIES_DYNAMIC_QUBIT_ALLOC);
    assert!(
        dynamic.contains("define internal void @Rec(")
            && dynamic.contains("call ptr @__quantum__rt__qubit_allocate()"),
        "{dynamic}"
    );
}

#[test]
fn recursive_static_array_allocation_inlines() {
    let source = r#"
        operation Rec(n : Int) : Unit {
            use qs = Qubit[2];
            H(qs[0]);
            if n > 0 { Rec(n - 1); }
            Reset(qs[0]);
        }
        @EntryPoint() operation Main() : Unit { Rec(2); }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "Rec");
    assert!(qir.contains("\"required_num_qubits\"=\"6\""), "{qir}");
}

#[test]
fn mutual_recursion_preserves_live_caller_qubit() {
    let source = r#"
        operation A(n : Int) : Unit {
            use q = Qubit();
            H(q);
            Reset(q);
            if n > 0 { B(n - 1); }
        }
        operation B(n : Int) : Unit { A(n); }
        @EntryPoint() operation Main() : Result {
            B(0);
            use r = Qubit();
            X(r);
            B(0);
            MResetZ(r)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "A");
    assert_inlined(&qir, "B");
    assert!(qir.contains("\"required_num_qubits\"=\"2\""), "{qir}");
    assert!(
        qir.contains("call void @Reset(ptr inttoptr (i64 1 to ptr))"),
        "{qir}"
    );
}

#[test]
fn curried_allocation_preserves_live_caller_qubit() {
    let source = r#"
        operation Inc(x : Int) : Int {
            use q = Qubit(); H(q); Reset(q); x + 1
        }
        function Curried(f : Int => Int) : (Int -> (Int => Int)) {
            a -> (b => f(a + 2 * b))
        }
        @EntryPoint() operation Main() : (Int, Int, Result) {
            mutable a = 0;
            mutable b = 0;
            set a = Curried(Inc)(a)(2);
            use r = Qubit();
            X(r);
            set b = Curried(Inc)(b)(2);
            (a, b, MResetZ(r))
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(qir.contains("\"required_num_qubits\"=\"2\""), "{qir}");
    assert_eq!(
        qir.matches("call void @__quantum__rt__int_record_output(i64 5,")
            .count(),
        2,
        "{qir}"
    );
}

#[test]
fn runtime_recursive_static_allocation_reports_error() {
    let source = r#"
        operation Rec(n : Int) : Unit {
            use q = Qubit(); H(q); Reset(q);
            if n > 0 { Rec(n - 1); }
        }
        @EntryPoint() operation Main() : Unit {
            use flag = Qubit();
            Rec(if MResetZ(flag) == Zero { 2 } else { 0 });
        }
    "#;
    let errors = super::compile_source_to_qir_result(source, *CAPABILITIES)
        .expect_err("runtime recursion cannot inline static allocation");
    assert!(
        super::format_interpret_errors(errors)
            .contains("Qdk.Qsc.PartialEval.UnsupportedRecursiveInlining")
    );
}

#[test]
fn nonallocating_mutual_recursion_remains_ir_functions() {
    let source = r#"
        operation A(n : Int, q : Qubit) : Unit {
            if n > 0 { X(q); B(n - 1, q); }
        }
        operation B(n : Int, q : Qubit) : Unit { A(n, q); }
        @EntryPoint() operation Main() : Unit {
            use q = Qubit(); B(2, q);
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(qir.contains("define internal void @A("), "{qir}");
    assert!(qir.contains("define internal void @B("), "{qir}");
}

#[test]
fn uncalled_allocating_body_does_not_block_adjoint_ir_function() {
    let source = r#"
        operation Act(q : Qubit) : Unit is Adj {
            body (...) { use a = Qubit(); let _ = MResetZ(a); }
            adjoint (...) { X(q); }
        }
        @EntryPoint() operation Main() : Unit {
            use q = Qubit(); Adjoint Act(q);
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(qir.contains("define internal void @Act__Adj("), "{qir}");
    assert!(qir.contains("\"required_num_qubits\"=\"1\""), "{qir}");
}

#[test]
fn controlled_adjoint_resource_check_uses_selected_specialization() {
    let source = r#"
        operation Act(q : Qubit) : Unit is Adj + Ctl {
            body (...) { use unused = Qubit(); }
            adjoint (...) { use unused = Qubit(); }
            controlled (cs, ...) { use unused = Qubit(); }
            controlled adjoint (cs, ...) { use a = Qubit(); H(a); Reset(a); }
        }
        operation Invoke(c : Qubit, q : Qubit) : Unit {
            Controlled Adjoint Act([c], q);
        }
        @EntryPoint() operation Main() : Unit {
            use (c, q) = (Qubit(), Qubit()); Invoke(c, q);
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "Invoke");
    assert!(qir.contains("\"required_num_qubits\"=\"3\""), "{qir}");

    let safe = source.replace("use a = Qubit(); H(a); Reset(a);", "X(q);");
    let qir = compile_source_to_qir(&safe, *CAPABILITIES);
    assert!(qir.contains("define internal void @Invoke("), "{qir}");
    assert!(qir.contains("\"required_num_qubits\"=\"2\""), "{qir}");
}

/// The outer frame saves `Zero`, then the inner frame saves `One` after the auxiliary
/// qubit is flipped. Unwinding must read each frame's own result: only the inner
/// frame flips the target. Reusing its result for the outer read would flip the
/// target twice and incorrectly return `Zero` instead of `One`.
#[test]
fn recursive_measurement_preserves_caller_result() {
    let source = r#"
        operation R(n : Int, q : Qubit, a : Qubit) : Unit {
            if n > 0 {
                let r = M(a);
                X(a);
                R(n - 1, q, a);
                if r == One { X(q); }
            }
        }
        @EntryPoint() operation Main() : Result {
            use (q, a) = (Qubit(), Qubit());
            R(2, q, a);
            Reset(a);
            MResetZ(q)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "R");
    assert!(qir.contains("\"required_num_results\"=\"3\""), "{qir}");
    expect![[r#"
        block_0:
        call void @__quantum__rt__initialize(ptr null)
        call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
        call void @X(ptr inttoptr (i64 1 to ptr))
        call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
        call void @X(ptr inttoptr (i64 1 to ptr))
        %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
        br i1 %var_1, label %block_1, label %block_2
        block_1:
        call void @X(ptr inttoptr (i64 0 to ptr))
        br label %block_2
        block_2:
        %var_3 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        br i1 %var_3, label %block_3, label %block_4
        block_3:
        call void @X(ptr inttoptr (i64 0 to ptr))
        br label %block_4
        block_4:
        call void @Reset(ptr inttoptr (i64 1 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
        ret i64 0
    "#]].assert_eq(&recursive_entry_dataflow(&qir));
}

/// The measurement occurs in an acyclic descendant, but its result must survive
/// the positive-depth `A`/`B` recursion just like a measurement directly in `A`.
#[test]
fn mutual_recursion_preserves_measuring_descendant_result() {
    let source = r#"
        operation Observe(q : Qubit) : Result { M(q) }
        operation A(n : Int, target : Qubit, auxiliary : Qubit) : Unit {
            if n > 0 {
                let saved = Observe(auxiliary);
                X(auxiliary);
                B(n - 1, target, auxiliary);
                if saved == One { X(target); }
            }
        }
        operation B(n : Int, target : Qubit, auxiliary : Qubit) : Unit {
            A(n, target, auxiliary);
        }
        @EntryPoint() operation Main() : Result {
            use (target, auxiliary) = (Qubit(), Qubit());
            B(2, target, auxiliary);
            Reset(auxiliary);
            MResetZ(target)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "A");
    assert_inlined(&qir, "B");
    expect![[r#"
        block_0:
        call void @__quantum__rt__initialize(ptr null)
        call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
        call void @X(ptr inttoptr (i64 1 to ptr))
        call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
        call void @X(ptr inttoptr (i64 1 to ptr))
        %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
        br i1 %var_1, label %block_1, label %block_2
        block_1:
        call void @X(ptr inttoptr (i64 0 to ptr))
        br label %block_2
        block_2:
        %var_3 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        br i1 %var_3, label %block_3, label %block_4
        block_3:
        call void @X(ptr inttoptr (i64 0 to ptr))
        br label %block_4
        block_4:
        call void @Reset(ptr inttoptr (i64 1 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
        ret i64 0
    "#]].assert_eq(&recursive_entry_dataflow(&qir));
}

/// Each live `A` frame holds a fresh qubit in `One` until its child returns.
/// Measuring and resetting the deepest frame must not reset either caller's
/// qubit: all three frames should flip their distinct target, producing
/// `[One, One, One]`. `B` enters the mutual recursion at positive depth.
#[test]
fn mutual_recursion_preserves_positive_depth_live_frames() {
    let source = r#"
        operation A(n : Int, targets : Qubit[]) : Unit {
            use fresh = Qubit();
            X(fresh);
            if n > 0 { B(n - 1, targets); }
            if MResetZ(fresh) == One { X(targets[n]); }
        }
        operation B(n : Int, targets : Qubit[]) : Unit { A(n, targets); }
        @EntryPoint() operation Main() : Result[] {
            use targets = Qubit[3];
            B(2, targets);
            MResetEachZ(targets)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "A");
    assert_inlined(&qir, "B");
    assert!(qir.contains("\"required_num_qubits\"=\"6\""), "{qir}");
    expect![[r#"
        block_0:
        call void @__quantum__rt__initialize(ptr null)
        call void @X(ptr inttoptr (i64 3 to ptr))
        call void @X(ptr inttoptr (i64 4 to ptr))
        call void @X(ptr inttoptr (i64 5 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 0 to ptr))
        %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        br i1 %var_2, label %block_1, label %block_2
        block_1:
        call void @X(ptr inttoptr (i64 0 to ptr))
        br label %block_2
        block_2:
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 1 to ptr))
        %var_4 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
        br i1 %var_4, label %block_3, label %block_4
        block_3:
        call void @X(ptr inttoptr (i64 1 to ptr))
        br label %block_4
        block_4:
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
        %var_6 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
        br i1 %var_6, label %block_5, label %block_6
        block_5:
        call void @X(ptr inttoptr (i64 2 to ptr))
        br label %block_6
        block_6:
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 4 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 5 to ptr))
        call void @__quantum__rt__array_record_output(i64 3, ptr @0)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @1)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 4 to ptr), ptr @2)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 5 to ptr), ptr @3)
        ret i64 0
    "#]].assert_eq(&recursive_entry_dataflow(&qir));
}

fn recursive_entry_dataflow(qir: &str) -> String {
    super::qir_entry_body(qir)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn recursive_emitted_dataflow(qir: &str, names: &[&str]) -> String {
    let mut result = String::new();
    for name in names {
        let definition = qir
            .lines()
            .find(|line| line.starts_with("define ") && line.contains(&format!(" @{name}(")))
            .expect("expected emitted callable");
        let body = qir
            .split_once(definition)
            .expect("definition exists")
            .1
            .split_once("\n}")
            .expect("function body terminates")
            .0;
        writeln!(result, "{definition}").expect("write definition");
        for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
            writeln!(result, "{line}").expect("write body");
        }
        writeln!(result, "}}").expect("write closing brace");
    }
    result
}

#[test]
fn mutual_measurement_recursion_inlines_from_either_member() {
    let source = r#"
        operation A(n : Int, q : Qubit) : Unit {
            if n > 0 {
                let r = M(q);
                B(n - 1, q);
                if r == One { X(q); }
            }
        }
        operation B(n : Int, q : Qubit) : Unit { A(n, q); }
        @EntryPoint() operation Main() : Unit {
            use q = Qubit();
            A(2, q);
            B(2, q);
            Reset(q);
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "A");
    assert_inlined(&qir, "B");
    assert!(qir.contains("\"required_num_results\"=\"4\""), "{qir}");
}

#[test]
fn recursive_custom_measurement_helper_and_result_alias_inline() {
    let source = r#"
        @Measurement()
        operation Read(q : Qubit) : Result { body intrinsic; }
        operation Observe(q : Qubit) : Result { Read(q) }
        operation R(n : Int, q : Qubit) : Unit {
            if n > 0 {
                let r = Observe(q);
                let saved = r;
                R(n - 1, q);
                if saved == One { X(q); }
            }
        }
        @EntryPoint() operation Main() : Unit {
            use q = Qubit(); R(2, q); Reset(q);
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_inlined(&qir, "R");
    assert!(qir.contains("\"required_num_results\"=\"2\""), "{qir}");
}

#[test]
fn recursive_measurement_through_acyclic_bool_helper_emits() {
    let source = r#"
        operation Observe(q : Qubit) : Bool { M(q) == One }
        operation R(n : Int, q : Qubit) : Unit {
            if n > 0 {
                let r = Observe(q);
                R(n - 1, q);
                if r { X(q); }
            }
        }
        @EntryPoint() operation Main() : Unit {
            use q = Qubit(); R(2, q); Reset(q);
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(qir.contains("define internal void @R("), "{qir}");
    assert!(
        qir.contains("define internal zeroext i1 @Observe("),
        "{qir}"
    );
}

#[test]
fn runtime_depth_recursive_measurement_reads_before_reentry() {
    let source = r#"
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {
            if n > 0 {
                if MResetZ(q) == One { X(target); }
                Rec(n - 1, q, target);
            }
        }
        @EntryPoint()
        operation Main() : Result {
            use (flag, q, target) = (Qubit(), Qubit(), Qubit());
            X(flag);
            X(q);
            let n = if MResetZ(flag) == One { 2 } else { 0 };
            Rec(n, q, target);
            Reset(q);
            MResetZ(target)
        }
    "#;
    let recursive_body = expect![[r#"
        define internal void @Rec(i64 %var_6, ptr %var_7, ptr %var_8) {
        block_5:
        %var_9 = alloca i1
        %var_10 = alloca i64
        %var_14 = alloca i64
        %var_17 = alloca ptr
        %var_18 = alloca ptr
        store i1 false, ptr %var_9
        store i64 %var_6, ptr %var_10
        %var_26 = load i64, ptr %var_10
        %var_11 = icmp sgt i64 %var_26, 0
        br i1 %var_11, label %block_6, label %block_9
        block_6:
        call void @__quantum__qis__mresetz__body(ptr %var_7, ptr inttoptr (i64 1 to ptr))
        %var_12 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
        store i1 %var_12, ptr %var_9
        %var_28 = load i1, ptr %var_9
        br i1 %var_28, label %block_7, label %block_8
        block_7:
        call void @X(ptr %var_8)
        br label %block_8
        block_8:
        store i64 %var_6, ptr %var_14
        %var_30 = load i64, ptr %var_14
        %var_15 = sub i64 %var_30, 1
        store ptr %var_7, ptr %var_17
        store ptr %var_8, ptr %var_18
        %var_33 = load ptr, ptr %var_17
        %var_34 = load ptr, ptr %var_18
        call void @Rec(i64 %var_15, ptr %var_33, ptr %var_34)
        br label %block_9
        block_9:
        ret void
        }
    "#]];
    for (prepared, expected_entry) in [false, true]
        .into_iter()
        .zip([expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            %var_3 = alloca i64
            %var_4 = alloca i64
            call void @__quantum__rt__initialize(ptr null)
            call void @X(ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
            %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            br i1 %var_1, label %block_1, label %block_2
            block_1:
            store i64 2, ptr %var_3
            br label %block_3
            block_2:
            store i64 0, ptr %var_3
            br label %block_3
            block_3:
            %var_20 = load i64, ptr %var_3
            store i64 %var_20, ptr %var_4
            %var_22 = load i64, ptr %var_4
            call void @Rec(i64 %var_22, ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @Reset(ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
            ret i64 0
            }
        "#]], expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            %var_3 = alloca i64
            %var_4 = alloca i64
            call void @__quantum__rt__initialize(ptr null)
            call void @X(ptr inttoptr (i64 0 to ptr))
            call void @X(ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
            %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            br i1 %var_1, label %block_1, label %block_2
            block_1:
            store i64 2, ptr %var_3
            br label %block_3
            block_2:
            store i64 0, ptr %var_3
            br label %block_3
            block_3:
            %var_20 = load i64, ptr %var_3
            store i64 %var_20, ptr %var_4
            %var_22 = load i64, ptr %var_4
            call void @Rec(i64 %var_22, ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @Reset(ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
            ret i64 0
            }
        "#]]])
    {
        let source = if prepared {
            source.to_owned()
        } else {
            source.replace("X(flag);", "")
        };
        let qir = compile_source_to_qir(&source, *CAPABILITIES);
        expected_entry.assert_eq(&recursive_emitted_dataflow(&qir, &["ENTRYPOINT__main"]));
        recursive_body.assert_eq(&recursive_emitted_dataflow(&qir, &["Rec"]));
        assert_runtime_measurement_recursion_contract(&source, &qir);
    }
}

#[test]
fn runtime_budget_recursive_retry_preserves_success_and_exhaustion() {
    let source = r#"
        operation Rus(budget : Int, q : Qubit, out : Qubit) : Unit {
            if budget > 0 {
                if MResetZ(q) == Zero { X(q); Rus(budget - 1, q, out); } else { X(out); }
            }
        }
        @EntryPoint()
        operation Main() : Result {
            use (flag, q, out) = (Qubit(), Qubit(), Qubit());
            X(flag);
            let budget = if MResetZ(flag) == One { 2 } else { 1 };
            Rus(budget, q, out);
            Reset(q);
            MResetZ(out)
        }
    "#;
    for (prepared, expected) in [false, true]
        .into_iter()
        .zip([expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            %var_2 = alloca i64
            %var_3 = alloca i64
            call void @__quantum__rt__initialize(ptr null)
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
            %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            br i1 %var_0, label %block_1, label %block_2
            block_1:
            store i64 2, ptr %var_2
            br label %block_3
            block_2:
            store i64 1, ptr %var_2
            br label %block_3
            block_3:
            %var_20 = load i64, ptr %var_2
            store i64 %var_20, ptr %var_3
            %var_22 = load i64, ptr %var_3
            call void @Rus(i64 %var_22, ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @Reset(ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
            ret i64 0
            }
            define internal void @Rus(i64 %var_5, ptr %var_6, ptr %var_7) {
            block_4:
            %var_8 = alloca i1
            %var_9 = alloca i64
            %var_14 = alloca i64
            %var_17 = alloca ptr
            %var_18 = alloca ptr
            store i1 false, ptr %var_8
            store i64 %var_5, ptr %var_9
            %var_26 = load i64, ptr %var_9
            %var_10 = icmp sgt i64 %var_26, 0
            br i1 %var_10, label %block_5, label %block_9
            block_5:
            call void @__quantum__qis__mresetz__body(ptr %var_6, ptr inttoptr (i64 1 to ptr))
            %var_11 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
            %var_12 = icmp eq i1 %var_11, false
            store i1 %var_12, ptr %var_8
            %var_28 = load i1, ptr %var_8
            br i1 %var_28, label %block_6, label %block_7
            block_6:
            call void @X(ptr %var_6)
            store i64 %var_5, ptr %var_14
            %var_30 = load i64, ptr %var_14
            %var_15 = sub i64 %var_30, 1
            store ptr %var_6, ptr %var_17
            store ptr %var_7, ptr %var_18
            %var_33 = load ptr, ptr %var_17
            %var_34 = load ptr, ptr %var_18
            call void @Rus(i64 %var_15, ptr %var_33, ptr %var_34)
            br label %block_8
            block_7:
            call void @X(ptr %var_7)
            br label %block_8
            block_8:
            br label %block_9
            block_9:
            ret void
            }
        "#]], expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            %var_3 = alloca i64
            %var_4 = alloca i64
            call void @__quantum__rt__initialize(ptr null)
            call void @X(ptr inttoptr (i64 0 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
            %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            br i1 %var_1, label %block_1, label %block_2
            block_1:
            store i64 2, ptr %var_3
            br label %block_3
            block_2:
            store i64 1, ptr %var_3
            br label %block_3
            block_3:
            %var_20 = load i64, ptr %var_3
            store i64 %var_20, ptr %var_4
            %var_22 = load i64, ptr %var_4
            call void @Rus(i64 %var_22, ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @Reset(ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
            ret i64 0
            }
            define internal void @Rus(i64 %var_6, ptr %var_7, ptr %var_8) {
            block_5:
            %var_9 = alloca i1
            %var_10 = alloca i64
            %var_14 = alloca i64
            %var_17 = alloca ptr
            %var_18 = alloca ptr
            store i1 false, ptr %var_9
            store i64 %var_6, ptr %var_10
            %var_26 = load i64, ptr %var_10
            %var_11 = icmp sgt i64 %var_26, 0
            br i1 %var_11, label %block_6, label %block_10
            block_6:
            call void @__quantum__qis__mresetz__body(ptr %var_7, ptr inttoptr (i64 1 to ptr))
            %var_12 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
            %var_13 = icmp eq i1 %var_12, false
            store i1 %var_13, ptr %var_9
            %var_28 = load i1, ptr %var_9
            br i1 %var_28, label %block_7, label %block_8
            block_7:
            call void @X(ptr %var_7)
            store i64 %var_6, ptr %var_14
            %var_30 = load i64, ptr %var_14
            %var_15 = sub i64 %var_30, 1
            store ptr %var_7, ptr %var_17
            store ptr %var_8, ptr %var_18
            %var_33 = load ptr, ptr %var_17
            %var_34 = load ptr, ptr %var_18
            call void @Rus(i64 %var_15, ptr %var_33, ptr %var_34)
            br label %block_9
            block_8:
            call void @X(ptr %var_8)
            br label %block_9
            block_9:
            br label %block_10
            block_10:
            ret void
            }
        "#]]])
    {
        let source = if prepared {
            source.to_owned()
        } else {
            source.replace("X(flag);", "")
        };
        let qir = compile_source_to_qir(&source, *CAPABILITIES);
        expected.assert_eq(&recursive_emitted_dataflow(&qir, &["ENTRYPOINT__main", "Rus"]));
        assert_runtime_measurement_recursion_contract(&source, &qir);
    }
}

/// Alternating the measured qubit gives the two live frames different saved Booleans.
/// Reading one reused raw slot on unwind instead would flip the target twice.
#[test]
fn runtime_depth_recursive_boolean_helper_preserves_saved_frames() {
    let source = r#"
        operation Observe(q : Qubit) : Bool { M(q) == One }
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {
            if n > 0 {
                let saved = Observe(q);
                X(q);
                Rec(n - 1, q, target);
                if saved { X(target); }
            }
        }
        @EntryPoint()
        operation Main() : Result {
            use (flag, q, target) = (Qubit(), Qubit(), Qubit());
            X(flag);
            let n = if MResetZ(flag) == One { 2 } else { 1 };
            Rec(n, q, target);
            Reset(q);
            MResetZ(target)
        }
    "#;
    for (prepared, expected) in [false, true]
        .into_iter()
        .zip([expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            %var_2 = alloca i64
            %var_3 = alloca i64
            call void @__quantum__rt__initialize(ptr null)
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
            %var_0 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            br i1 %var_0, label %block_1, label %block_2
            block_1:
            store i64 2, ptr %var_2
            br label %block_3
            block_2:
            store i64 1, ptr %var_2
            br label %block_3
            block_3:
            %var_22 = load i64, ptr %var_2
            store i64 %var_22, ptr %var_3
            %var_24 = load i64, ptr %var_3
            call void @Rec(i64 %var_24, ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @Reset(ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
            ret i64 0
            }
            define internal void @Rec(i64 %var_5, ptr %var_6, ptr %var_7) {
            block_4:
            %var_8 = alloca i64
            %var_16 = alloca i64
            %var_20 = alloca ptr
            store i64 %var_5, ptr %var_8
            %var_27 = load i64, ptr %var_8
            %var_9 = icmp sgt i64 %var_27, 0
            br i1 %var_9, label %block_5, label %block_8
            block_5:
            %var_13 = call zeroext i1 @Observe(ptr %var_6)
            call void @X(ptr %var_6)
            store i64 %var_5, ptr %var_16
            %var_29 = load i64, ptr %var_16
            %var_17 = sub i64 %var_29, 1
            store ptr %var_7, ptr %var_20
            %var_31 = load ptr, ptr %var_20
            call void @Rec(i64 %var_17, ptr %var_6, ptr %var_31)
            br i1 %var_13, label %block_6, label %block_7
            block_6:
            call void @X(ptr %var_7)
            br label %block_7
            block_7:
            br label %block_8
            block_8:
            ret void
            }
            define internal zeroext i1 @Observe(ptr %var_10) {
            block_9:
            call void @__quantum__qis__m__body(ptr %var_10, ptr inttoptr (i64 1 to ptr))
            %var_11 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
            ret i1 %var_11
            }
        "#]], expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
            block_0:
            %var_3 = alloca i64
            %var_4 = alloca i64
            call void @__quantum__rt__initialize(ptr null)
            call void @X(ptr inttoptr (i64 0 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
            %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
            br i1 %var_1, label %block_1, label %block_2
            block_1:
            store i64 2, ptr %var_3
            br label %block_3
            block_2:
            store i64 1, ptr %var_3
            br label %block_3
            block_3:
            %var_22 = load i64, ptr %var_3
            store i64 %var_22, ptr %var_4
            %var_24 = load i64, ptr %var_4
            call void @Rec(i64 %var_24, ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @Reset(ptr inttoptr (i64 1 to ptr))
            call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
            call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
            ret i64 0
            }
            define internal void @Rec(i64 %var_6, ptr %var_7, ptr %var_8) {
            block_5:
            %var_9 = alloca i64
            %var_16 = alloca i64
            %var_20 = alloca ptr
            store i64 %var_6, ptr %var_9
            %var_27 = load i64, ptr %var_9
            %var_10 = icmp sgt i64 %var_27, 0
            br i1 %var_10, label %block_6, label %block_9
            block_6:
            %var_14 = call zeroext i1 @Observe(ptr %var_7)
            call void @X(ptr %var_7)
            store i64 %var_6, ptr %var_16
            %var_29 = load i64, ptr %var_16
            %var_17 = sub i64 %var_29, 1
            store ptr %var_8, ptr %var_20
            %var_31 = load ptr, ptr %var_20
            call void @Rec(i64 %var_17, ptr %var_7, ptr %var_31)
            br i1 %var_14, label %block_7, label %block_8
            block_7:
            call void @X(ptr %var_8)
            br label %block_8
            block_8:
            br label %block_9
            block_9:
            ret void
            }
            define internal zeroext i1 @Observe(ptr %var_11) {
            block_10:
            call void @__quantum__qis__m__body(ptr %var_11, ptr inttoptr (i64 1 to ptr))
            %var_12 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
            ret i1 %var_12
            }
        "#]]])
    {
        let source = if prepared {
            source.to_owned()
        } else {
            source.replace("X(flag);", "")
        };
        let qir = compile_source_to_qir(&source, *CAPABILITIES);
        expected.assert_eq(&recursive_emitted_dataflow(
            &qir,
            &["ENTRYPOINT__main", "Rec", "Observe"],
        ));
        assert_runtime_measurement_recursion_contract(&source, &qir);
    }
}

#[test]
fn finite_recursion_in_measurement_branch_keeps_repeated_array_length_known() {
    let source = r#"
        operation Rec(n : Int, target : Qubit) : Unit {
            { use q = Qubit(); X(q); X(q); }
            if n > 0 {
                if MResetZ(target) == Zero { Rec(n - 1, target); }
            }
            let values = [true, size = n];
            if Length(values) == 2 { X(target); }
        }
        @EntryPoint()
        operation Main() : Result {
            use target = Qubit();
            Rec(2, target);
            MResetZ(target)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_static_resource_management(&qir, 2, 3);
    assert_inlined(&qir, "Rec");
    expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
        block_0:
        call void @__quantum__rt__initialize(ptr null)
        call void @X(ptr inttoptr (i64 1 to ptr))
        call void @X(ptr inttoptr (i64 1 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
        %var_2 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        %var_3 = icmp eq i1 %var_2, false
        br i1 %var_3, label %block_1, label %block_4
        block_1:
        call void @X(ptr inttoptr (i64 1 to ptr))
        call void @X(ptr inttoptr (i64 1 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
        %var_5 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
        %var_6 = icmp eq i1 %var_5, false
        br i1 %var_6, label %block_2, label %block_3
        block_2:
        call void @X(ptr inttoptr (i64 1 to ptr))
        call void @X(ptr inttoptr (i64 1 to ptr))
        br label %block_3
        block_3:
        br label %block_4
        block_4:
        call void @X(ptr inttoptr (i64 0 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
        ret i64 0
        }
        define internal void @X(ptr %var_1) {
        block_5:
        call void @__quantum__qis__x__body(ptr %var_1)
        ret void
        }
    "#]].assert_eq(&recursive_emitted_dataflow(&qir, &["ENTRYPOINT__main", "X"]));
}

#[test]
fn finite_recursion_keeps_repeated_array_length_known() {
    let source = r#"
        operation Rec(n : Int, target : Qubit) : Unit {
            { use q = Qubit(); X(q); X(q); }
            let values = [true, size = n];
            if Length(values) == 2 { X(target); }
            if n > 0 { Rec(n - 1, target); }
        }
        @EntryPoint()
        operation Main() : Result {
            use target = Qubit();
            Rec(2, target);
            MResetZ(target)
        }
    "#;
    let unrolled = r#"
        @EntryPoint()
        operation Main() : Result {
            use target = Qubit();
            { use q = Qubit(); X(q); X(q); }
            X(target);
            { use q = Qubit(); X(q); X(q); }
            { use q = Qubit(); X(q); X(q); }
            MResetZ(target)
        }
    "#;
    let expected = compile_source_to_qir(unrolled, *CAPABILITIES);
    assert_static_resource_management(&expected, 2, 1);
    assert_eq!(compile_source_to_qir(source, *CAPABILITIES), expected);
}

#[test]
fn finite_recursion_keeps_range_bounds_known() {
    let source = r#"
        operation Rec(n : Int, target : Qubit) : Unit {
            { use q = Qubit(); X(q); X(q); }
            for _ in 1..n { X(target); }
            if n > 0 { Rec(n - 1, target); }
        }
        @EntryPoint()
        operation Main() : Result {
            use target = Qubit();
            Rec(2, target);
            MResetZ(target)
        }
    "#;
    let unrolled = r#"
        @EntryPoint()
        operation Main() : Result {
            use target = Qubit();
            { use q = Qubit(); X(q); X(q); }
            for _ in 1..2 { X(target); }
            { use q = Qubit(); X(q); X(q); }
            for _ in 1..1 { X(target); }
            { use q = Qubit(); X(q); X(q); }
            for _ in 1..0 { X(target); }
            MResetZ(target)
        }
    "#;
    let expected = compile_source_to_qir(unrolled, *CAPABILITIES);
    assert_static_resource_management(&expected, 2, 1);
    assert_eq!(compile_source_to_qir(source, *CAPABILITIES), expected);
}

#[test]
fn finite_recursion_does_not_evaluate_unreachable_fail() {
    let source = r#"
        operation Rec(n : Int, target : Qubit) : Unit {
            { use q = Qubit(); X(q); X(q); }
            if n < 0 { fail "negative depth"; }
            X(target);
            if n > 0 { Rec(n - 1, target); }
        }
        @EntryPoint()
        operation Main() : Result {
            use target = Qubit();
            Rec(2, target);
            MResetZ(target)
        }
    "#;
    let unrolled = r#"
        @EntryPoint()
        operation Main() : Result {
            use target = Qubit();
            { use q = Qubit(); X(q); X(q); } X(target);
            { use q = Qubit(); X(q); X(q); } X(target);
            { use q = Qubit(); X(q); X(q); } X(target);
            MResetZ(target)
        }
    "#;
    let expected = compile_source_to_qir(unrolled, *CAPABILITIES);
    assert_static_resource_management(&expected, 2, 1);
    assert_eq!(compile_source_to_qir(source, *CAPABILITIES), expected);
}

#[test]
fn finite_recursion_keeps_array_size_with_unused_runtime_integer() {
    let source = r#"
        operation Size(n : Int, unused : Int) : Int {
            { use q = Qubit(); X(q); X(q); }
            if n > 0 { let ignored = Size(n - 1, unused); }
            2
        }
        @EntryPoint()
        operation Main() : Result {
            use flag = Qubit(); X(flag);
            let unused = if MResetZ(flag) == One { 1 } else { 0 };
            let size = Size(2, unused);
            use qs = Qubit[size];
            MResetZ(qs[0])
        }
    "#;
    let unrolled = r#"
        operation Unrolled(_ : Int, unused : Int) : Unit {
            { use q = Qubit(); X(q); X(q); }
            { use q = Qubit(); X(q); X(q); }
            { use q = Qubit(); X(q); X(q); }
        }
        @EntryPoint()
        operation Main() : Result {
            use flag = Qubit(); X(flag);
            let unused = if MResetZ(flag) == One { 1 } else { 0 };
            Unrolled(2, unused);
            use qs = Qubit[2];
            MResetZ(qs[0])
        }
    "#;
    let expected = compile_source_to_qir(unrolled, *CAPABILITIES);
    assert_static_resource_management(&expected, 3, 2);
    assert_eq!(compile_source_to_qir(source, *CAPABILITIES), expected);
}

#[test]
fn finite_recursion_keeps_array_size_with_unused_runtime_boolean() {
    let source = r#"
        operation Size(n : Int, unused : Bool) : Int {
            { use q = Qubit(); X(q); X(q); }
            if n > 0 { let ignored = Size(n - 1, unused); }
            2
        }
        @EntryPoint()
        operation Main() : Result {
            use flag = Qubit(); X(flag);
            let unused = MResetZ(flag) == One;
            let size = Size(2, unused);
            use qs = Qubit[size];
            MResetZ(qs[0])
        }
    "#;
    let unrolled = r#"
        @EntryPoint()
        operation Main() : Result {
            use flag = Qubit(); X(flag);
            let unused = MResetZ(flag) == One;
            { use q = Qubit(); X(q); X(q); }
            { use q = Qubit(); X(q); X(q); }
            { use q = Qubit(); X(q); X(q); }
            use qs = Qubit[2];
            MResetZ(qs[0])
        }
    "#;
    let expected = compile_source_to_qir(unrolled, *CAPABILITIES);
    assert_static_resource_management(&expected, 3, 2);
    assert_eq!(compile_source_to_qir(source, *CAPABILITIES), expected);
}

#[test]
fn finite_recursion_with_released_local_qubit_keeps_array_size_constant() {
    let source = r#"
        operation Size(n : Int) : Int {
            { use q = Qubit(); X(q); X(q); }
            if n > 0 { let ignored = Size(n - 1); }
            2
        }
        @EntryPoint()
        operation Main() : Result {
            let size = Size(2);
            use qs = Qubit[size];
            MResetZ(qs[0])
        }
    "#;
    let unrolled = r#"
        @EntryPoint()
        operation Main() : Result {
            { use q = Qubit(); X(q); X(q); }
            { use q = Qubit(); X(q); X(q); }
            { use q = Qubit(); X(q); X(q); }
            use qs = Qubit[2];
            MResetZ(qs[0])
        }
    "#;
    let expected = compile_source_to_qir(unrolled, *CAPABILITIES);
    assert_static_resource_management(&expected, 2, 1);
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_eq!(qir, expected);
}

#[test]
fn finite_recursion_with_released_local_qubit_keeps_computed_integer_constant() {
    let source = r#"
        operation Count(n : Int, target : Qubit) : Int {
            { use q = Qubit(); X(q); X(q); }
            X(target);
            if n > 0 { Count(n - 1, target) + 1 } else { 0 }
        }
        @EntryPoint()
        operation Main() : Result {
            use target = Qubit();
            let count = Count(2, target);
            if count == 2 { X(target); }
            MResetZ(target)
        }
    "#;
    let unrolled = r#"
        @EntryPoint()
        operation Main() : Result {
            use target = Qubit();
            { use q = Qubit(); X(q); X(q); }
            X(target);
            { use q = Qubit(); X(q); X(q); }
            X(target);
            { use q = Qubit(); X(q); X(q); }
            X(target);
            X(target);
            MResetZ(target)
        }
    "#;
    let expected = compile_source_to_qir(unrolled, *CAPABILITIES);
    assert_static_resource_management(&expected, 2, 1);
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_eq!(qir, expected);
}

#[test]
fn finite_integer_recursion_with_qubit_parameter_inside_runtime_recursion_is_inlined() {
    let source = r#"
        operation Count(n : Int, target : Qubit) : Int {
            { use q = Qubit(); X(q); X(q); }
            X(target);
            if n > 0 { Count(n - 1, target) + 1 } else { 0 }
        }
        operation Parent(n : Int, target : Qubit) : Unit {
            let count = Count(2, target);
            if count == 2 { X(target); }
            if n > 0 { Parent(n - 1, target); }
        }
        @EntryPoint()
        operation Main() : Result {
            use (flag, target) = (Qubit(), Qubit()); X(flag);
            let depth = if MResetZ(flag) == One { 2 } else { 0 };
            Parent(depth, target);
            MResetZ(target)
        }
    "#;
    let unrolled = r#"
        operation Last(n : Int, target : Qubit) : Int {
            { use q = Qubit(); X(q); X(q); }
            X(target);
            0
        }
        operation Middle(n : Int, target : Qubit) : Int {
            { use q = Qubit(); X(q); X(q); }
            X(target);
            Last(n - 1, target) + 1
        }
        operation First(n : Int, target : Qubit) : Int {
            { use q = Qubit(); X(q); X(q); }
            X(target);
            Middle(n - 1, target) + 1
        }
        operation Parent(n : Int, target : Qubit) : Unit {
            let count = First(2, target);
            if count == 2 { X(target); }
            if n > 0 { Parent(n - 1, target); }
        }
        @EntryPoint()
        operation Main() : Result {
            use (flag, target) = (Qubit(), Qubit()); X(flag);
            let depth = if MResetZ(flag) == One { 2 } else { 0 };
            Parent(depth, target);
            MResetZ(target)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_static_resource_management(&qir, 3, 2);
    assert!(qir.contains("define internal void @Parent("), "{qir}");
    assert!(!qir.contains("@Count("), "{qir}");
    assert_eq!(qir, compile_source_to_qir(unrolled, *CAPABILITIES));
}

#[test]
fn finite_recursion_with_unused_result_argument_keeps_computed_integer_constant() {
    let source = r#"
        operation Count(n : Int, unused : Result, target : Qubit) : Int {
            { use q = Qubit(); X(q); X(q); }
            X(target);
            if n > 0 { Count(n - 1, unused, target) + 1 } else { 0 }
        }
        @EntryPoint()
        operation Main() : Result {
            use (flag, target) = (Qubit(), Qubit()); X(flag);
            let saved = MResetZ(flag);
            let count = Count(2, saved, target);
            if count == 2 { X(target); }
            MResetZ(target)
        }
    "#;
    let unrolled = r#"
        @EntryPoint()
        operation Main() : Result {
            use (flag, target) = (Qubit(), Qubit()); X(flag);
            let saved = MResetZ(flag);
            { use q = Qubit(); X(q); X(q); } X(target);
            { use q = Qubit(); X(q); X(q); } X(target);
            { use q = Qubit(); X(q); X(q); } X(target);
            X(target);
            MResetZ(target)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_static_resource_management(&qir, 3, 2);
    assert!(!qir.contains("@Count("), "{qir}");
    assert_eq!(qir, compile_source_to_qir(unrolled, *CAPABILITIES));
}

#[test]
fn recursive_local_qubit_storage_preserves_known_array_size_requirement() {
    assert_recursive_local_qubit_storage_preserves_static_requirement(
        "let values = [true, size = known]; if Length(values) == 2 { X(target); }",
    );
}

#[test]
fn recursive_local_qubit_storage_preserves_known_string_requirement() {
    assert_recursive_local_qubit_storage_preserves_static_requirement(
        r#"let label = $"{known}"; if label == "2" { X(target); }"#,
    );
}

fn assert_recursive_local_qubit_storage_preserves_static_requirement(statement: &str) {
    let source = format!(
        r#"
        operation Rec(depth : Int, known : Int, target : Qubit) : Unit {{
            {{ use local = Qubit(); X(local); X(local); }}
            {statement}
            if depth > 0 {{ Rec(depth - 1, known, target); }}
        }}
        @EntryPoint()
        operation Main() : Result {{
            use (flag, target) = (Qubit(), Qubit());
            let depth = if MResetZ(flag) == One {{ 1 }} else {{ 0 }};
            Rec(depth, 2, target);
            MResetZ(target)
        }}
        "#
    );
    let static_source = source.replace("if MResetZ(flag) == One { 1 } else { 0 }", "1");
    let unrolled_source = static_source.replace(
        "Rec(depth, 2, target);",
        "{ use local = Qubit(); X(local); X(local); } X(target);
         { use local = Qubit(); X(local); X(local); } X(target);",
    );
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
        let errors = super::compile_source_to_qir_result(&source, profile.into()).expect_err(
            "local-qubit storage cannot make a required constant into a runtime parameter",
        );
        let diagnostics = super::format_interpret_errors(errors);
        assert!(
            diagnostics.contains("Qdk.Qsc.PartialEval.UnsupportedRecursiveInlining"),
            "{profile:?}: {diagnostics}"
        );
        assert_eq!(
            compile_source_to_qir(&static_source, profile.into()),
            compile_source_to_qir(&unrolled_source, profile.into()),
            "{profile:?}: known recursion depth must retain the finite inline fallback"
        );
    }
}

#[test]
fn runtime_recursion_with_known_integer_argument_keeps_unit_calls_emitted() {
    let source = r#"
        operation Rec(n : Int, unused : Int, target : Qubit) : Unit {
            { use q = Qubit(); X(q); X(q); }
            X(target);
            if n > 0 { Rec(n - 1, unused, target); }
        }
        @EntryPoint()
        operation Main() : Result {
            use (flag, target) = (Qubit(), Qubit()); X(flag);
            let depth = if MResetZ(flag) == One { 2 } else { 0 };
            Rec(depth, 7, target);
            MResetZ(target)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_static_resource_management(&qir, 4, 2);
    assert!(qir.contains("define internal void @Rec("), "{qir}");
    assert_eq!(qir.matches("call void @Rec(").count(), 2, "{qir}");
    expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
        block_0:
        %var_3 = alloca i64
        %var_4 = alloca i64
        %var_5 = alloca i64
        %var_6 = alloca i64
        %var_8 = alloca i64
        call void @__quantum__rt__initialize(ptr null)
        call void @X(ptr inttoptr (i64 0 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
        %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        br i1 %var_1, label %block_1, label %block_2
        block_1:
        store i64 2, ptr %var_3
        br label %block_3
        block_2:
        store i64 0, ptr %var_3
        br label %block_3
        block_3:
        %var_22 = load i64, ptr %var_3
        store i64 %var_22, ptr %var_4
        %var_24 = load i64, ptr %var_4
        store i64 %var_24, ptr %var_5
        call void @X(ptr inttoptr (i64 2 to ptr))
        call void @X(ptr inttoptr (i64 2 to ptr))
        call void @X(ptr inttoptr (i64 1 to ptr))
        %var_26 = load i64, ptr %var_5
        store i64 %var_26, ptr %var_6
        %var_28 = load i64, ptr %var_6
        %var_7 = icmp sgt i64 %var_28, 0
        br i1 %var_7, label %block_4, label %block_5
        block_4:
        %var_29 = load i64, ptr %var_5
        store i64 %var_29, ptr %var_8
        %var_31 = load i64, ptr %var_8
        %var_9 = sub i64 %var_31, 1
        call void @Rec(i64 %var_9, i64 7, ptr inttoptr (i64 1 to ptr))
        br label %block_5
        block_5:
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @0)
        ret i64 0
        }
        define internal void @Rec(i64 %var_11, i64 %var_12, ptr %var_13) {
        block_7:
        %var_14 = alloca i64
        %var_16 = alloca i64
        %var_20 = alloca ptr
        call void @X(ptr inttoptr (i64 3 to ptr))
        call void @X(ptr inttoptr (i64 3 to ptr))
        call void @X(ptr %var_13)
        store i64 %var_11, ptr %var_14
        %var_34 = load i64, ptr %var_14
        %var_15 = icmp sgt i64 %var_34, 0
        br i1 %var_15, label %block_8, label %block_9
        block_8:
        store i64 %var_11, ptr %var_16
        %var_36 = load i64, ptr %var_16
        %var_17 = sub i64 %var_36, 1
        store ptr %var_13, ptr %var_20
        %var_38 = load ptr, ptr %var_20
        call void @Rec(i64 %var_17, i64 %var_12, ptr %var_38)
        br label %block_9
        block_9:
        ret void
        }
        define internal void @X(ptr %var_0) {
        block_6:
        call void @__quantum__qis__x__body(ptr %var_0)
        ret void
        }
    "#]].assert_eq(&recursive_emitted_dataflow(
        &qir,
        &["ENTRYPOINT__main", "Rec", "X"],
    ));
}

#[test]
fn runtime_recursion_with_known_integer_argument_emits_constant_integer_return() {
    let source = r#"
        operation Rec(n : Int, unused : Int) : Int {
            { use q = Qubit(); X(q); X(q); }
            if n > 0 { let ignored = Rec(n - 1, unused); }
            2
        }
        @EntryPoint()
        operation Main() : Int {
            use flag = Qubit(); X(flag);
            let depth = if MResetZ(flag) == One { 2 } else { 0 };
            Rec(depth, 7)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_static_resource_management(&qir, 3, 1);
    let body = recursive_emitted_dataflow(&qir, &["Rec"]);
    assert!(body.contains("define internal i64 @Rec("), "{body}");
    assert!(body.contains("ret i64 2"), "{body}");
    assert_eq!(qir.matches("call i64 @Rec(").count(), 2, "{qir}");
    let entry = super::qir_entry_body(&qir);
    assert!(
        entry.contains("call void @__quantum__rt__int_record_output(i64 2,"),
        "{entry}"
    );
}

#[test]
fn finite_recursion_preserves_child_integer_return_and_parent_value() {
    let source = r#"
        operation Rec(depth : Int) : Int {
            { use local = Qubit(); X(local); X(local); }
            let parentDigit = depth + 1;
            if depth > 0 {
                let child = Rec(depth - 1);
                child * 10 + parentDigit
            } else { parentDigit }
        }
        @EntryPoint()
        operation Main() : Int { Rec(DEPTH) }
    "#;
    // Unwinding appends each parent's digit to its child's result: 1 -> 12 -> 123 -> 1234.
    for (depth, expected) in [(0, 1), (3, 1234)] {
        let source = source.replace("DEPTH", &depth.to_string());
        for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF, Profile::Adaptive] {
            let qir = compile_source_to_qir(&source, profile.into());
            assert_inlined(&qir, "Rec");
            let entry = super::qir_entry_body(&qir);
            assert!(
                entry.contains(&format!(
                    "call void @__quantum__rt__int_record_output(i64 {expected},"
                )),
                "{profile:?}, depth {depth}: expected integer {expected}\n{entry}"
            );
        }
    }
}

#[test]
fn runtime_recursion_with_child_dependent_integer_return_is_rejected() {
    let source = r#"
        operation Rec(depth : Int) : Int {
            { use local = Qubit(); X(local); X(local); }
            let parentDigit = depth + 1;
            if depth > 0 {
                let child = Rec(depth - 1);
                child * 10 + parentDigit
            } else { parentDigit }
        }
        @EntryPoint()
        operation Main() : Int {
            use flag = Qubit();
            let depth = if MResetZ(flag) == One { 3 } else { 0 };
            Rec(depth)
        }
    "#;
    let errors = super::compile_source_to_qir_result(source, *CAPABILITIES)
        .expect_err("runtime-dependent recursive return values are not supported yet");
    let diagnostics = super::format_interpret_errors(errors);
    assert!(
        diagnostics.contains("Qdk.Qsc.PartialEval.UnexpectedDynamicValue"),
        "{diagnostics}"
    );
}

#[test]
fn measurement_controlled_recursion_with_known_argument_keeps_runtime_calls() {
    let source = r#"
        operation Rec(unused : Int, stop : Qubit, target : Qubit) : Unit {
            { use q = Qubit(); X(q); X(q); }
            let again = MResetZ(stop) == One;
            if again { Rec(unused, stop, target); }
            X(target);
        }
        @EntryPoint()
        operation Main() : Result {
            use (stop, target) = (Qubit(), Qubit()); X(stop);
            Rec(7, stop, target);
            MResetZ(target)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_static_resource_management(&qir, 4, 3);
    assert!(qir.contains("define internal void @Rec("), "{qir}");
    assert_eq!(qir.matches("call void @Rec(").count(), 2, "{qir}");
    expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
        block_0:
        call void @__quantum__rt__initialize(ptr null)
        call void @X(ptr inttoptr (i64 0 to ptr))
        call void @X(ptr inttoptr (i64 2 to ptr))
        call void @X(ptr inttoptr (i64 2 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
        %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        br i1 %var_1, label %block_1, label %block_2
        block_1:
        call void @Rec(i64 7, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
        br label %block_2
        block_2:
        call void @X(ptr inttoptr (i64 1 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @0)
        ret i64 0
        }
        define internal void @Rec(i64 %var_4, ptr %var_5, ptr %var_6) {
        block_4:
        %var_11 = alloca ptr
        %var_12 = alloca ptr
        call void @X(ptr inttoptr (i64 3 to ptr))
        call void @X(ptr inttoptr (i64 3 to ptr))
        call void @__quantum__qis__mresetz__body(ptr %var_5, ptr inttoptr (i64 1 to ptr))
        %var_7 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
        br i1 %var_7, label %block_5, label %block_6
        block_5:
        store ptr %var_5, ptr %var_11
        store ptr %var_6, ptr %var_12
        %var_15 = load ptr, ptr %var_11
        %var_16 = load ptr, ptr %var_12
        call void @Rec(i64 %var_4, ptr %var_15, ptr %var_16)
        br label %block_6
        block_6:
        call void @X(ptr %var_6)
        ret void
        }
        define internal void @X(ptr %var_0) {
        block_3:
        call void @__quantum__qis__x__body(ptr %var_0)
        ret void
        }
    "#]].assert_eq(&recursive_emitted_dataflow(
        &qir,
        &["ENTRYPOINT__main", "Rec", "X"],
    ));
}

#[test]
fn runtime_recursion_with_released_local_qubit_emits_constant_integer_return() {
    let source = r#"
        operation Rec(n : Int) : Int {
            { use fresh = Qubit(); X(fresh); X(fresh); }
            if n > 0 { let ignored = Rec(n - 1); }
            2
        }
        @EntryPoint()
        operation Main() : Int {
            use flag = Qubit(); X(flag);
            let depth = if MResetZ(flag) == One { 2 } else { 0 };
            Rec(depth)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_static_resource_management(&qir, 2, 1);
    let body = recursive_emitted_dataflow(&qir, &["Rec"]);
    assert!(body.contains("define internal i64 @Rec("), "{body}");
    assert!(body.contains("ret i64 2"), "{body}");
    assert!(qir.contains("call i64 @Rec("), "{qir}");
    let entry = super::qir_entry_body(&qir);
    let result = entry
        .lines()
        .find_map(|line| line.trim().split_once(" = call i64 @Rec("))
        .expect("entry must capture the recursive call's return value")
        .0;
    assert!(
        entry.contains(&format!(
            "call void @__quantum__rt__int_record_output(i64 {result},"
        )),
        "{entry}"
    );
}

#[test]
fn runtime_recursion_reuses_released_local_qubit_without_dynamic_management() {
    let source = r#"
        operation Rec(n : Int) : Unit {
            { use fresh = Qubit(); X(fresh); X(fresh); }
            if n > 0 { Rec(n - 1); }
        }
        @EntryPoint()
        operation Main() : Result {
            use flag = Qubit();
            X(flag);
            let n = if MResetZ(flag) == One { 2 } else { 0 };
            Rec(n);
            MResetZ(flag)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_static_resource_management(&qir, 2, 2);
    expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
        block_0:
        %var_3 = alloca i64
        %var_4 = alloca i64
        call void @__quantum__rt__initialize(ptr null)
        call void @X(ptr inttoptr (i64 0 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
        %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        br i1 %var_1, label %block_1, label %block_2
        block_1:
        store i64 2, ptr %var_3
        br label %block_3
        block_2:
        store i64 0, ptr %var_3
        br label %block_3
        block_3:
        %var_11 = load i64, ptr %var_3
        store i64 %var_11, ptr %var_4
        %var_13 = load i64, ptr %var_4
        call void @Rec(i64 %var_13)
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @0)
        ret i64 0
        }
        define internal void @Rec(i64 %var_5) {
        block_5:
        %var_6 = alloca i64
        %var_8 = alloca i64
        call void @X(ptr inttoptr (i64 1 to ptr))
        call void @X(ptr inttoptr (i64 1 to ptr))
        store i64 %var_5, ptr %var_6
        %var_16 = load i64, ptr %var_6
        %var_7 = icmp sgt i64 %var_16, 0
        br i1 %var_7, label %block_6, label %block_7
        block_6:
        store i64 %var_5, ptr %var_8
        %var_18 = load i64, ptr %var_8
        %var_9 = sub i64 %var_18, 1
        call void @Rec(i64 %var_9)
        br label %block_7
        block_7:
        ret void
        }
        define internal void @X(ptr %var_0) {
        block_4:
        call void @__quantum__qis__x__body(ptr %var_0)
        ret void
        }
    "#]].assert_eq(&recursive_emitted_dataflow(&qir, &["ENTRYPOINT__main", "Rec", "X"]));
}

#[test]
fn recursive_local_qubit_is_isolated_from_caller_qubits_before_between_and_after_calls() {
    let source = r#"
        operation Rec(n : Int) : Unit {
            { use fresh = Qubit(); X(fresh); X(fresh); }
            if n > 0 { Rec(n - 1); }
        }
        @EntryPoint()
        operation Main() : Result[] {
            use flag = Qubit(); X(flag);
            let n = if MResetZ(flag) == One { 2 } else { 0 };
            use before = Qubit(); X(before);
            Rec(n);
            use between = Qubit(); X(between);
            Rec(n);
            use after = Qubit(); X(after);
            Rec(n);
            [MResetZ(before), MResetZ(between), MResetZ(after)]
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_static_resource_management(&qir, 5, 4);
    expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
        block_0:
        %var_3 = alloca i64
        %var_4 = alloca i64
        call void @__quantum__rt__initialize(ptr null)
        call void @X(ptr inttoptr (i64 0 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
        %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        br i1 %var_1, label %block_1, label %block_2
        block_1:
        store i64 2, ptr %var_3
        br label %block_3
        block_2:
        store i64 0, ptr %var_3
        br label %block_3
        block_3:
        %var_11 = load i64, ptr %var_3
        store i64 %var_11, ptr %var_4
        call void @X(ptr inttoptr (i64 1 to ptr))
        %var_13 = load i64, ptr %var_4
        call void @Rec(i64 %var_13)
        call void @X(ptr inttoptr (i64 3 to ptr))
        call void @Rec(i64 %var_13)
        call void @X(ptr inttoptr (i64 4 to ptr))
        call void @Rec(i64 %var_13)
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 2 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 3 to ptr))
        call void @__quantum__rt__array_record_output(i64 3, ptr @0)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @1)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr @2)
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr @3)
        ret i64 0
        }
        define internal void @Rec(i64 %var_5) {
        block_5:
        %var_6 = alloca i64
        %var_8 = alloca i64
        call void @X(ptr inttoptr (i64 2 to ptr))
        call void @X(ptr inttoptr (i64 2 to ptr))
        store i64 %var_5, ptr %var_6
        %var_16 = load i64, ptr %var_6
        %var_7 = icmp sgt i64 %var_16, 0
        br i1 %var_7, label %block_6, label %block_7
        block_6:
        store i64 %var_5, ptr %var_8
        %var_18 = load i64, ptr %var_8
        %var_9 = sub i64 %var_18, 1
        call void @Rec(i64 %var_9)
        br label %block_7
        block_7:
        ret void
        }
        define internal void @X(ptr %var_0) {
        block_4:
        call void @__quantum__qis__x__body(ptr %var_0)
        ret void
        }
    "#]].assert_eq(&recursive_emitted_dataflow(&qir, &["ENTRYPOINT__main", "Rec", "X"]));
}

#[test]
fn runtime_depth_mutual_recursion_reuses_released_fixed_arrays_without_caller_aliases() {
    let source = r#"
        operation A(n : Int) : Unit {
            { use qs = Qubit[2]; X(qs[0]); X(qs[0]); }
            if n > 0 { B(n - 1); }
        }
        operation B(n : Int) : Unit {
            { use qs = Qubit[2]; X(qs[1]); X(qs[1]); }
            if n > 0 { A(n - 1); }
        }
        @EntryPoint()
        operation Main() : Result {
            use flag = Qubit(); X(flag);
            let n = if MResetZ(flag) == One { 2 } else { 0 };
            use caller = Qubit(); X(caller);
            A(n);
            B(n);
            MResetZ(caller)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_static_resource_management(&qir, 6, 2);
    expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
        block_0:
        %var_3 = alloca i64
        %var_4 = alloca i64
        call void @__quantum__rt__initialize(ptr null)
        call void @X(ptr inttoptr (i64 0 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
        %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        br i1 %var_1, label %block_1, label %block_2
        block_1:
        store i64 2, ptr %var_3
        br label %block_3
        block_2:
        store i64 0, ptr %var_3
        br label %block_3
        block_3:
        %var_18 = load i64, ptr %var_3
        store i64 %var_18, ptr %var_4
        call void @X(ptr inttoptr (i64 1 to ptr))
        %var_20 = load i64, ptr %var_4
        call void @A(i64 %var_20)
        call void @B(i64 %var_20)
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @0)
        ret i64 0
        }
        define internal void @A(i64 %var_5) {
        block_5:
        %var_7 = alloca i64
        %var_9 = alloca i64
        call void @X(ptr inttoptr (i64 2 to ptr))
        call void @X(ptr inttoptr (i64 2 to ptr))
        store i64 %var_5, ptr %var_7
        %var_23 = load i64, ptr %var_7
        %var_8 = icmp sgt i64 %var_23, 0
        br i1 %var_8, label %block_6, label %block_7
        block_6:
        store i64 %var_5, ptr %var_9
        %var_25 = load i64, ptr %var_9
        %var_10 = sub i64 %var_25, 1
        call void @B(i64 %var_10)
        br label %block_7
        block_7:
        ret void
        }
        define internal void @B(i64 %var_11) {
        block_8:
        %var_13 = alloca i64
        %var_15 = alloca i64
        call void @X(ptr inttoptr (i64 5 to ptr))
        call void @X(ptr inttoptr (i64 5 to ptr))
        store i64 %var_11, ptr %var_13
        %var_27 = load i64, ptr %var_13
        %var_14 = icmp sgt i64 %var_27, 0
        br i1 %var_14, label %block_9, label %block_10
        block_9:
        store i64 %var_11, ptr %var_15
        %var_29 = load i64, ptr %var_15
        %var_16 = sub i64 %var_29, 1
        call void @A(i64 %var_16)
        br label %block_10
        block_10:
        ret void
        }
        define internal void @X(ptr %var_0) {
        block_4:
        call void @__quantum__qis__x__body(ptr %var_0)
        ret void
        }
    "#]].assert_eq(&recursive_emitted_dataflow(&qir, &["ENTRYPOINT__main", "A", "B", "X"]));
}

#[test]
fn runtime_recursion_with_inlined_qubit_allocating_helper_preserves_target_updates() {
    let source = r#"
        operation Scratch(qs : Qubit[]) : Unit {
            use own = Qubit(); X(own); X(own); X(qs[0]);
        }
        operation Rec(n : Int, target : Qubit) : Unit {
            { use qs = Qubit[2]; X(qs[0]); X(qs[0]); }
            Scratch([target]);
            if n > 0 { Rec(n - 1, target); }
        }
        @EntryPoint()
        operation Main() : Result {
            use (flag, target) = (Qubit(), Qubit()); X(flag);
            let n = if MResetZ(flag) == One { 2 } else { 0 };
            Rec(n, target);
            MResetZ(target)
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert_static_resource_management(&qir, 4, 2);
    assert_inlined(&qir, "Scratch");
    expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
        block_0:
        %var_3 = alloca i64
        %var_4 = alloca i64
        call void @__quantum__rt__initialize(ptr null)
        call void @X(ptr inttoptr (i64 0 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
        %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
        br i1 %var_1, label %block_1, label %block_2
        block_1:
        store i64 2, ptr %var_3
        br label %block_3
        block_2:
        store i64 0, ptr %var_3
        br label %block_3
        block_3:
        %var_18 = load i64, ptr %var_3
        store i64 %var_18, ptr %var_4
        %var_20 = load i64, ptr %var_4
        call void @Rec(i64 %var_20, ptr inttoptr (i64 1 to ptr))
        call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
        call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @0)
        ret i64 0
        }
        define internal void @Rec(i64 %var_6, ptr %var_7) {
        block_5:
        %var_9 = alloca ptr
        %var_11 = alloca i64
        %var_13 = alloca i64
        %var_16 = alloca ptr
        call void @X(ptr inttoptr (i64 2 to ptr))
        call void @X(ptr inttoptr (i64 2 to ptr))
        store ptr %var_7, ptr %var_9
        call void @X(ptr inttoptr (i64 2 to ptr))
        call void @X(ptr inttoptr (i64 2 to ptr))
        %var_23 = load ptr, ptr %var_9
        call void @X(ptr %var_23)
        store i64 %var_6, ptr %var_11
        %var_25 = load i64, ptr %var_11
        %var_12 = icmp sgt i64 %var_25, 0
        br i1 %var_12, label %block_6, label %block_7
        block_6:
        store i64 %var_6, ptr %var_13
        %var_27 = load i64, ptr %var_13
        %var_14 = sub i64 %var_27, 1
        store ptr %var_7, ptr %var_16
        %var_29 = load ptr, ptr %var_16
        call void @Rec(i64 %var_14, ptr %var_29)
        br label %block_7
        block_7:
        ret void
        }
        define internal void @X(ptr %var_0) {
        block_4:
        call void @__quantum__qis__x__body(ptr %var_0)
        ret void
        }
    "#]].assert_eq(&recursive_emitted_dataflow(&qir, &["ENTRYPOINT__main", "Rec", "X"]));
}

fn assert_static_resource_management(qir: &str, qubits: usize, results: usize) {
    assert!(
        qir.contains(&format!("\"required_num_qubits\"=\"{qubits}\"")),
        "{qir}"
    );
    assert!(
        qir.contains(&format!("\"required_num_results\"=\"{results}\"")),
        "{qir}"
    );
    for flag in ["dynamic_qubit_management", "dynamic_result_management"] {
        assert!(qir.contains(&format!("!\"{flag}\", i1 false")), "{qir}");
    }
    assert!(
        !qir.contains("call ptr @__quantum__rt__qubit_allocate("),
        "{qir}"
    );
}

#[test]
fn runtime_recursion_with_qubit_release_skipped_by_logical_operators_is_rejected() {
    let release = "{ Reset(fresh); QIR.Runtime.__quantum__rt__qubit_release(fresh); true }";
    for (name, statement) in [
        ("and", "let ignored = false and RELEASE;"),
        ("or", "let ignored = true or RELEASE;"),
        (
            "and assignment",
            "mutable ignored = false; set ignored and= RELEASE;",
        ),
        (
            "or assignment",
            "mutable ignored = true; set ignored or= RELEASE;",
        ),
    ] {
        let statement = statement.replace("RELEASE", release);
        for preparation in ["", "X(flag);"] {
            let source = format!(
                r#"
                operation Rec(n : Int, target : Qubit) : Unit {{
                    let fresh = QIR.Runtime.__quantum__rt__qubit_allocate();
                    if MResetZ(fresh) == One {{ X(target); }}
                    X(fresh);
                    {statement}
                    if n > 0 {{ Rec(n - 1, target); }}
                }}
                @EntryPoint()
                operation Main() : Result {{
                    use (flag, target) = (Qubit(), Qubit());
                    {preparation}
                    let depth = if MResetZ(flag) == One {{ 1 }} else {{ 0 }};
                    Rec(depth, target);
                    Rec(depth, target);
                    MResetZ(target)
                }}
                "#
            );
            let errors = super::compile_source_to_qir_result(&source, *CAPABILITIES)
                .expect_err("a skipped release cannot justify recursive reuse of a local qubit");
            let errors = super::format_interpret_errors(errors);
            assert!(
                errors.contains("Qdk.Qsc.PartialEval.UnsupportedRecursiveInlining"),
                "{name}, preparation {preparation:?}: {errors}"
            );
        }
    }
}

#[test]
fn qubit_release_in_logical_left_operand_remains_unconditional() {
    let source = r#"
        operation Rec(n : Int) : Unit {
            let fresh = QIR.Runtime.__quantum__rt__qubit_allocate();
            X(fresh); X(fresh);
            RELEASE
            if n > 0 { Rec(n - 1); }
        }
        @EntryPoint()
        operation Main() : Result {
            use flag = Qubit(); X(flag);
            let depth = if MResetZ(flag) == One { 2 } else { 0 };
            Rec(depth);
            MResetZ(flag)
        }
    "#;
    let expected = compile_source_to_qir(
        &source.replace(
            "RELEASE",
            "QIR.Runtime.__quantum__rt__qubit_release(fresh);",
        ),
        *CAPABILITIES,
    );
    assert_static_resource_management(&expected, 2, 2);
    for statement in [
        "let ignored = { QIR.Runtime.__quantum__rt__qubit_release(fresh); true } and false;",
        "let ignored = { QIR.Runtime.__quantum__rt__qubit_release(fresh); false } or true;",
    ] {
        let qir = compile_source_to_qir(&source.replace("RELEASE", statement), *CAPABILITIES);
        assert_eq!(qir, expected, "{statement}");
    }
}

#[test]
fn runtime_recursion_in_array_update_index_before_local_qubit_release_is_rejected() {
    assert_array_update_reentry_before_release_is_rejected(
        "QIR.Runtime.__quantum__rt__qubit_allocate()",
        "fresh",
        r#"let ignored = { Reset(fresh); QIR.Runtime.__quantum__rt__qubit_release(fresh); [false] }
            w/ { if n > 0 { Rec(n - 1, target); } 0 } <- true;"#,
    );
}

#[test]
fn runtime_recursion_in_array_update_replacement_before_local_qubit_release_is_rejected() {
    assert_array_update_reentry_before_release_is_rejected(
        "QIR.Runtime.__quantum__rt__qubit_allocate()",
        "fresh",
        r#"let ignored = { Reset(fresh); QIR.Runtime.__quantum__rt__qubit_release(fresh); [false] }
            w/ 0 <- { if n > 0 { Rec(n - 1, target); } true };"#,
    );
}

#[test]
fn runtime_recursion_in_array_update_index_before_local_qubit_array_release_is_rejected() {
    assert_array_update_reentry_before_release_is_rejected(
        "QIR.Runtime.AllocateQubitArray(1)",
        "fresh[0]",
        r#"let ignored = { Reset(fresh[0]); QIR.Runtime.ReleaseQubitArray(fresh); [false] }
            w/ { if n > 0 { Rec(n - 1, target); } 0 } <- true;"#,
    );
}

#[test]
fn runtime_recursion_in_array_update_replacement_before_local_qubit_array_release_is_rejected() {
    assert_array_update_reentry_before_release_is_rejected(
        "QIR.Runtime.AllocateQubitArray(1)",
        "fresh[0]",
        r#"let ignored = { Reset(fresh[0]); QIR.Runtime.ReleaseQubitArray(fresh); [false] }
            w/ 0 <- { if n > 0 { Rec(n - 1, target); } true };"#,
    );
}

fn assert_array_update_reentry_before_release_is_rejected(
    allocation: &str,
    qubit: &str,
    statement: &str,
) {
    for preparation in ["", "X(flag);"] {
        let source = format!(
            r#"
            operation Rec(n : Int, target : Qubit) : Unit {{
                let fresh = {allocation};
                if MResetZ({qubit}) == One {{ X(target); }}
                X({qubit});
                {statement}
            }}
            @EntryPoint()
            operation Main() : Result {{
                use (flag, target) = (Qubit(), Qubit());
                {preparation}
                let depth = if MResetZ(flag) == One {{ 1 }} else {{ 0 }};
                Rec(depth, target);
                MResetZ(target)
            }}
            "#
        );
        let errors = super::compile_source_to_qir_result(&source, *CAPABILITIES)
            .expect_err("a local qubit must be released before recursive reentry");
        let errors = super::format_interpret_errors(errors);
        assert!(
            errors.contains("Qdk.Qsc.PartialEval.UnsupportedRecursiveInlining"),
            "{statement}, preparation {preparation:?}: {errors}"
        );
    }
}

#[test]
fn qubit_release_in_array_update_index_precedes_recursive_replacement() {
    assert_array_update_release_preserves_direct_release_qir(
        r#"let ignored = [false]
            w/ { QIR.Runtime.__quantum__rt__qubit_release(fresh); 0 }
            <- { X(target); if n > 0 { Rec(n - 1, target); } true };"#,
    );
}

#[test]
fn qubit_release_in_array_update_replacement_precedes_recursive_container() {
    assert_array_update_release_preserves_direct_release_qir(
        r#"let ignored = { X(target); if n > 0 { Rec(n - 1, target); } [false] }
            w/ 0 <- { QIR.Runtime.__quantum__rt__qubit_release(fresh); true };"#,
    );
}

fn assert_array_update_release_preserves_direct_release_qir(statement: &str) {
    let source = r#"
        operation Rec(n : Int, target : Qubit) : Unit {
            let fresh = QIR.Runtime.__quantum__rt__qubit_allocate();
            X(fresh); X(fresh);
            STATEMENT
        }
        @EntryPoint()
        operation Main() : Result {
            use (flag, target) = (Qubit(), Qubit()); X(flag);
            let depth = if MResetZ(flag) == One { 2 } else { 0 };
            Rec(depth, target);
            MResetZ(target)
        }
    "#;
    let expected = compile_source_to_qir(
        &source.replace(
            "STATEMENT",
            "QIR.Runtime.__quantum__rt__qubit_release(fresh);
             X(target); if n > 0 { Rec(n - 1, target); }",
        ),
        *CAPABILITIES,
    );
    assert_static_resource_management(&expected, 3, 2);
    let qir = compile_source_to_qir(&source.replace("STATEMENT", statement), *CAPABILITIES);
    assert_eq!(qir, expected, "{statement}");
}

#[test]
fn releasing_local_qubits_does_not_allow_results_to_outlive_recursive_measurement() {
    assert_unsafe_measurement_recursion_rejected(
        r#"
        operation Rec(n : Int, q : Qubit, target : Qubit) : Unit {
            { use scratch = Qubit(); X(scratch); X(scratch); }
            if n > 0 {
                let saved = M(q);
                X(q);
                Rec(n - 1, q, target);
                if saved == One { X(target); }
            }
        }
        operation Main() : Unit {
            use (flag, q, target) = (Qubit(), Qubit(), Qubit()); X(flag);
            let n = if MResetZ(flag) == One { 2 } else { 0 };
            Rec(n, q, target);
            Reset(q); Reset(target);
        }
        "#,
    );
}

fn assert_runtime_measurement_recursion_contract(source: &str, qir: &str) {
    assert!(qir.contains("\"required_num_qubits\"=\"3\""), "{qir}");
    assert!(qir.contains("\"required_num_results\"=\"3\""), "{qir}");
    for flag in ["dynamic_qubit_management", "dynamic_result_management"] {
        assert!(qir.contains(&format!("!\"{flag}\", i1 false")), "{qir}");
    }
    for profile in [Profile::AdaptiveRI, Profile::AdaptiveRIF] {
        let errors = super::compile_source_to_qir_result(source, profile.into())
            .expect_err("these profiles cannot emit runtime recursive calls");
        assert!(
            super::format_interpret_errors(errors)
                .contains("Qdk.Qsc.PartialEval.UnsupportedRecursiveInlining")
        );
    }
}

fn assert_unsafe_measurement_recursion_rejected(source: &str) {
    for capabilities in [*CAPABILITIES, *CAPABILITIES_DYNAMIC_QUBIT_ALLOC] {
        let errors = super::compile_source_to_qir_result(source, capabilities)
            .expect_err("unproved result lifetimes must not emit recursive functions");
        assert!(
            super::format_interpret_errors(errors)
                .contains("Qdk.Qsc.PartialEval.UnsupportedRecursiveInlining")
        );
    }
}

#[test]
fn runtime_recursive_result_alias_remains_rejected() {
    assert_unsafe_measurement_recursion_rejected(
        r#"
        operation Rec(n : Int, q : Qubit) : Unit {
            if n > 0 {
                let measured = M(q);
                let saved = measured;
                Rec(n - 1, q);
                if saved == One { X(q); }
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            Rec(if MResetZ(q) == One { 2 } else { 1 }, q);
            Reset(q);
        }
        "#,
    );
}

#[test]
fn runtime_recursive_result_tuple_remains_rejected() {
    assert_unsafe_measurement_recursion_rejected(
        r#"
        operation Rec(n : Int, q : Qubit) : Unit {
            if n > 0 {
                let pair = (M(q), false);
                Rec(n - 1, q);
                let (saved, _) = pair;
                if saved == One { X(q); }
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            Rec(if MResetZ(q) == One { 2 } else { 1 }, q);
            Reset(q);
        }
        "#,
    );
}

#[test]
fn runtime_recursive_result_array_remains_rejected() {
    assert_unsafe_measurement_recursion_rejected(
        r#"
        operation Rec(n : Int, q : Qubit) : Unit {
            if n > 0 {
                let saved = [M(q)];
                Rec(n - 1, q);
                if saved[0] == One { X(q); }
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            Rec(if MResetZ(q) == One { 2 } else { 1 }, q);
            Reset(q);
        }
        "#,
    );
}

#[test]
fn runtime_recursive_measuring_descendant_remains_rejected() {
    assert_unsafe_measurement_recursion_rejected(
        r#"
        operation Observe(q : Qubit) : Result { M(q) }
        operation A(n : Int, q : Qubit) : Unit {
            if n > 0 {
                let saved = Observe(q);
                B(n - 1, q);
                if saved == One { X(q); }
            }
        }
        operation B(n : Int, q : Qubit) : Unit { A(n, q); }
        operation Main() : Unit {
            use q = Qubit();
            B(if MResetZ(q) == One { 2 } else { 1 }, q);
            Reset(q);
        }
        "#,
    );
}

#[test]
fn runtime_recursive_custom_measurement_remains_unproved() {
    assert_unsafe_measurement_recursion_rejected(
        r#"
        @Measurement()
        operation Read(q : Qubit) : Result { body intrinsic; }
        operation Rec(n : Int, q : Qubit) : Unit {
            if n > 0 {
                if Read(q) == One { X(q); }
                Rec(n - 1, q);
            }
        }
        operation Main() : Unit {
            use q = Qubit();
            Rec(if MResetZ(q) == One { 2 } else { 1 }, q);
            Reset(q);
        }
        "#,
    );
}

#[test]
fn runtime_recursive_measurement_reports_error_even_with_dynamic_qubits() {
    let source = r#"
        operation R(n : Int, q : Qubit) : Unit {
            if n > 0 {
                let r = M(q);
                R(n - 1, q);
                if r == One { X(q); }
            }
        }
        @EntryPoint() operation Main() : Unit {
            use q = Qubit();
            R(if M(q) == One { 2 } else { 1 }, q);
            Reset(q);
        }
    "#;
    for capabilities in [*CAPABILITIES, *CAPABILITIES_DYNAMIC_QUBIT_ALLOC] {
        let errors = super::compile_source_to_qir_result(source, capabilities)
            .expect_err("runtime recursion cannot reuse static result slots");
        assert!(
            super::format_interpret_errors(errors)
                .contains("Qdk.Qsc.PartialEval.UnsupportedRecursiveInlining")
        );
    }
}

#[test]
fn acyclic_measurement_with_nonmeasuring_recursion_remains_ir_function() {
    let source = r#"
        operation Flip(n : Int, q : Qubit) : Unit {
            if n > 0 { X(q); Flip(n - 1, q); }
        }
        operation Observe(q : Qubit) : Bool {
            let r = M(q);
            Flip(2, q);
            X(q);
            X(q);
            r == One
        }
        @EntryPoint() operation Main() : Bool {
            use q = Qubit();
            let r = Observe(q);
            Reset(q);
            r
        }
    "#;
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define internal zeroext i1 @Observe("),
        "{qir}"
    );
    assert!(qir.contains("define internal void @Flip("), "{qir}");
}

/// A recursive operation can be emitted into an IR function.
#[test]
fn recursive_operation_emits_to_ir_function() {
    let source = "namespace Test {
            operation Recurse(n : Int, q : Qubit) : Unit {
                if n > 0 {
                    X(q);
                    Recurse(n - 1, q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Recurse(3, q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @Recurse(i64 3, ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @Recurse(i64 %var_0, ptr %var_1) {
        block_1:
          %var_2 = alloca i64
          %var_5 = alloca i64
          store i64 %var_0, ptr %var_2
          %var_8 = load i64, ptr %var_2
          %var_3 = icmp sgt i64 %var_8, 0
          br i1 %var_3, label %block_2, label %block_3
        block_2:
          call void @X(ptr %var_1)
          store i64 %var_0, ptr %var_5
          %var_10 = load i64, ptr %var_5
          %var_6 = sub i64 %var_10, 1
          call void @Recurse(i64 %var_6, ptr %var_1)
          br label %block_3
        block_3:
          ret void
        }

        define internal void @X(ptr %var_4) {
        block_4:
          call void @__quantum__qis__x__body(ptr %var_4)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

/// A call into a stdlib/library operation now emits as a standalone IR function. The
/// cross-package `X` operation is rendered as its own `define void @X` whose body calls the
/// `__quantum__qis__x__body` intrinsic, and the entry point calls that emitted function rather
/// than inlining the intrinsic directly.
#[test]
fn cross_package_operation_emits() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                X(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define internal void @X("),
        "expected the cross-package `X` operation to emit a standalone IR function; got:\n{qir}"
    );
    assert!(
        qir.contains("call void @X("),
        "expected the entry point to call the emitted `X` IR function; got:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected the emitted `X` wrapper body to call its intrinsic; got:\n{qir}"
    );
}

/// A non-Unit-returning operation inlines in the base (void) phase.
#[test]
fn non_unit_returning_operation_inlines() {
    let source = "namespace Test {
            operation MeasureIt(q : Qubit) : Result {
                return MResetZ(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                return MeasureIt(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        !qir.contains("@MeasureIt"),
        "expected the non-Unit-returning operation to inline; got:\n{qir}"
    );
}

/// A value-returning IR function whose scalar return value is produced by a
/// dynamic store (a `set` inside a measurement-conditioned branch) must emit
/// a `define i64 @Foo` whose `ret i64` operand is a value defined within the
/// function. This guards against the non-SSA RIR passes ignoring the
/// `Return` operand, which would prune the defining `Store` and skip
/// inserting the load, producing an invalid `ret i64` that references an
/// undefined variable.
#[test]
fn value_returning_ir_function_with_dynamic_store_return_is_defined() {
    let source = "namespace Test {
            operation Foo(q : Qubit) : Int {
                mutable x = 1;
                if MResetZ(q) == One {
                    set x = 2;
                }
                x
            }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return Foo(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    assert!(
        qir.contains("define internal i64 @Foo("),
        "expected a value-returning IR function named `Foo`; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          %var_7 = call i64 @Foo(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__int_record_output(i64 %var_7, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal i64 @Foo(ptr %var_2) {
        block_1:
          %var_3 = alloca i64
          store i64 1, ptr %var_3
          call void @__quantum__qis__mresetz__body(ptr %var_2, ptr inttoptr (i64 0 to ptr))
          %var_4 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_4, label %block_2, label %block_3
        block_2:
          store i64 2, ptr %var_3
          br label %block_3
        block_3:
          %var_9 = load i64, ptr %var_3
          ret i64 %var_9
        }

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

/// A value-returning IR function whose returned mutable is read, updated,
/// and stored again in the same merge block must reload the freshly stored
/// value before `ret`. The Q# `set x = x + 1; x` sequence reads `x`, adds
/// one, stores it, then returns it from the same block; the rendered QIR
/// must `load` the alloca after that `store` and feed the fresh value into
/// `ret i64`, not the pre-increment load.
#[test]
fn value_returning_ir_function_reloads_after_same_block_store() {
    let source = "namespace Test {
            operation Foo(q : Qubit) : Int {
                mutable x = 0;
                if MResetZ(q) == One {
                    set x = 5;
                }
                set x = x + 1;
                x
            }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return Foo(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          %var_9 = call i64 @Foo(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__int_record_output(i64 %var_9, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal i64 @Foo(ptr %var_2) {
        block_1:
          %var_3 = alloca i64
          %var_7 = alloca i64
          store i64 0, ptr %var_3
          call void @__quantum__qis__mresetz__body(ptr %var_2, ptr inttoptr (i64 0 to ptr))
          %var_4 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_4, label %block_2, label %block_3
        block_2:
          store i64 5, ptr %var_3
          br label %block_3
        block_3:
          %var_11 = load i64, ptr %var_3
          store i64 %var_11, ptr %var_7
          %var_13 = load i64, ptr %var_7
          %var_8 = add i64 %var_13, 1
          store i64 %var_8, ptr %var_3
          %var_15 = load i64, ptr %var_3
          ret i64 %var_15
        }

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
        .assert_eq(&qir);
}

/// The post-transform (`ssa`) RIR for the same store-backed value-returning
/// body must contain a `Load` of the returned variable that follows its
/// final `Store` in the block that ends in `Return`, proving the returned
/// value is reloaded after the same-block update.
#[test]
fn value_returning_ir_function_rir_reloads_after_same_block_store() {
    let source = "namespace Test {
            operation Foo(q : Qubit) : Int {
                mutable x = 0;
                if MResetZ(q) == One {
                    set x = 5;
                }
                set x = x + 1;
                x
            }
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                return Foo(q);
            }
        }";
    let rir = compile_source_to_rir(source, *CAPABILITIES);
    let [_raw, ssa] = rir.as_slice() else {
        panic!("expected raw and transformed RIR programs");
    };
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: __quantum__rt__initialize
                    call_type: Regular
                    input_type:
                        [0]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: Foo
                    call_type: Regular
                    input_type:
                        [0]: Qubit
                    input_vars:
                        [0]: 2
                    output_type: Integer
                    body: 1
                Callable 3: Callable:
                    name: __quantum__qis__mresetz__body
                    call_type: Measurement
                    input_type:
                        [0]: Qubit
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
                Callable 4: Callable:
                    name: __quantum__rt__read_result
                    call_type: Readout
                    input_type:
                        [0]: Result
                    output_type: Boolean
                    body: <NONE>
                Callable 5: Callable:
                    name: __quantum__rt__int_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Integer
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( Pointer, )
                    Variable(9, Integer) = Call id(2), args( Qubit(0), ) !dbg dbg_location=1
                    Call id(5), args( Variable(9, Integer), Tag(0, 3), )
                    Return Integer(0)
                Block 1: Block:
                    Variable(3, Integer) = Alloca
                    Variable(7, Integer) = Alloca
                    Variable(3, Integer) = Store Integer(0)
                    Call id(3), args( Variable(2, Qubit), Result(0), ) !dbg dbg_location=3
                    Variable(4, Boolean) = Call id(4), args( Result(0), ) !dbg dbg_location=2
                    Branch Variable(4, Boolean), 2, 3 !dbg dbg_location=4
                Block 2: Block:
                    Variable(3, Integer) = Store Integer(5)
                    Jump(3)
                Block 3: Block:
                    Variable(11, Integer) = Load Variable(3, Integer)
                    Variable(7, Integer) = Store Variable(11, Integer)
                    Variable(13, Integer) = Load Variable(7, Integer)
                    Variable(8, Integer) = Add Variable(13, Integer), Integer(1)
                    Variable(3, Integer) = Store Variable(8, Integer)
                    Variable(15, Integer) = Load Variable(3, Integer)
                    Return Variable(15, Integer)
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | StaticSizedArrays | CallSupport)
            num_qubits: 1
            num_results: 1

            dbg_scopes:
                0 = SubProgram name=Main location=(2-282)
                1 = SubProgram name=Foo location=(2-29)
                2 = SubProgram name=MResetZ location=(1-182413)
            dbg_locations:
                [1]: scope=0 location=(2-363)
                [2]: scope=1 location=(2-112) inlined_at=1
                [3]: scope=2 location=(1-182462) inlined_at=2
                [4]: scope=1 location=(2-109) inlined_at=1
            tags:
                [0]: 0_i
    "#]]
        .assert_eq(ssa);
}

#[test]
fn preparepurestated_cyclic_library_calls_generate_correct_qir() {
    let source = "
    operation Main() : Result[] {
        use qs = Qubit[2];
        Std.StatePreparation.PreparePureStateD([0.5, 0.5, 0.5, 0.5], qs);
        MResetEachZ(qs)
    }
    ";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @S__Adj(ptr inttoptr (i64 0 to ptr))
          call void @H(ptr inttoptr (i64 0 to ptr))
          call void @Rz(double 1.5707963267948966, ptr inttoptr (i64 0 to ptr))
          call void @H__Adj(ptr inttoptr (i64 0 to ptr))
          call void @S(ptr inttoptr (i64 0 to ptr))
          call void @S__Adj(ptr inttoptr (i64 1 to ptr))
          call void @H(ptr inttoptr (i64 1 to ptr))
          call void @Rz(double 1.5707963267948966, ptr inttoptr (i64 1 to ptr))
          call void @CNOT__Adj(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @CNOT__Adj(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @H__Adj(ptr inttoptr (i64 1 to ptr))
          call void @S(ptr inttoptr (i64 1 to ptr))
          call void @CNOT__Adj(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @CNOT__Adj(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__rt__array_record_output(i64 2, ptr @0)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @1)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr @2)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @S__Adj(ptr %var_3) {
        block_1:
          call void @__quantum__qis__s__adj(ptr %var_3)
          ret void
        }

        declare void @__quantum__qis__s__adj(ptr)

        define internal void @H(ptr %var_4) {
        block_2:
          call void @__quantum__qis__h__body(ptr %var_4)
          ret void
        }

        declare void @__quantum__qis__h__body(ptr)

        define internal void @Rz(double %var_7, ptr %var_8) {
        block_3:
          call void @__quantum__qis__rz__body(double %var_7, ptr %var_8)
          ret void
        }

        declare void @__quantum__qis__rz__body(double, ptr)

        define internal void @H__Adj(ptr %var_11) {
        block_4:
          call void @__quantum__qis__h__body(ptr %var_11)
          ret void
        }

        define internal void @S(ptr %var_12) {
        block_5:
          call void @__quantum__qis__s__body(ptr %var_12)
          ret void
        }

        declare void @__quantum__qis__s__body(ptr)

        define internal void @CNOT__Adj(ptr %var_17, ptr %var_18) {
        block_6:
          call void @__quantum__qis__cx__body(ptr %var_17, ptr %var_18)
          ret void
        }

        declare void @__quantum__qis__cx__body(ptr, ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare void @__quantum__rt__array_record_output(i64, ptr)

        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]]
    .assert_eq(&qir);
}

#[test]
fn point_in_time_copy_of_mutable_variable_maintains_value_after_original_mutable_updated() {
    let source = "
    operation Main() : Int {
        use q0 = Qubit();
        mutable n = 0;
        if MResetZ(q0) == One { n = 2; }
        let h = n;
        n = 7;
        h
    }
    ";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_0 = alloca i64
          %var_4 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i64 0, ptr %var_0
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_1, label %block_1, label %block_2
        block_1:
          store i64 2, ptr %var_0
          br label %block_2
        block_2:
          %var_6 = load i64, ptr %var_0
          store i64 %var_6, ptr %var_4
          store i64 7, ptr %var_0
          %var_9 = load i64, ptr %var_4
          call void @__quantum__rt__int_record_output(i64 %var_9, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn point_in_time_copy_of_mutable_variable_maintains_value_after_original_mutable_updated_in_dynamic_branch()
 {
    let source = "
    operation Main() : Int {
        use q0 = Qubit();
        mutable n = 0;
        if MResetZ(q0) == One { n = 2; }
        let h = n;
        if MResetZ(q0) == One { n = 7; }
        h
    }
    ";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_0 = alloca i64
          %var_4 = alloca i64
          %var_8 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i64 0, ptr %var_0
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          br i1 %var_1, label %block_1, label %block_2
        block_1:
          store i64 2, ptr %var_0
          br label %block_2
        block_2:
          %var_10 = load i64, ptr %var_0
          store i64 %var_10, ptr %var_4
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          %var_5 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          br i1 %var_5, label %block_3, label %block_4
        block_3:
          store i64 7, ptr %var_0
          br label %block_4
        block_4:
          %var_12 = load i64, ptr %var_4
          store i64 %var_12, ptr %var_8
          %var_14 = load i64, ptr %var_8
          call void @__quantum__rt__int_record_output(i64 %var_14, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="2" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

#[test]
fn point_in_time_copy_of_mutable_variable_copied_across_multiple_dynamic_branch_updates() {
    let source = "
        operation Main() : Int {
            use q = Qubit();
            mutable n = 0;
            if MResetZ(q) == Zero { n = 2; }
            mutable x = 0;
            x = n;
            let a = x;
            mutable r = 0;
            if MResetZ(q) == Zero { r = a; }
            r
        }
    ";

    let qir = compile_source_to_qir(source, *CAPABILITIES);
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          %var_0 = alloca i64
          %var_4 = alloca i64
          %var_5 = alloca i64
          %var_6 = alloca i64
          %var_10 = alloca i64
          call void @__quantum__rt__initialize(ptr null)
          store i64 0, ptr %var_0
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %var_1 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          %var_2 = icmp eq i1 %var_1, false
          br i1 %var_2, label %block_1, label %block_2
        block_1:
          store i64 2, ptr %var_0
          br label %block_2
        block_2:
          %var_12 = load i64, ptr %var_0
          store i64 %var_12, ptr %var_4
          %var_14 = load i64, ptr %var_4
          store i64 %var_14, ptr %var_5
          store i64 0, ptr %var_6
          call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          %var_7 = call zeroext i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          %var_8 = icmp eq i1 %var_7, false
          br i1 %var_8, label %block_3, label %block_4
        block_3:
          %var_20 = load i64, ptr %var_5
          store i64 %var_20, ptr %var_6
          br label %block_4
        block_4:
          %var_17 = load i64, ptr %var_6
          store i64 %var_17, ptr %var_10
          %var_19 = load i64, ptr %var_10
          call void @__quantum__rt__int_record_output(i64 %var_19, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

        declare zeroext i1 @__quantum__rt__read_result(ptr) #2

        declare void @__quantum__rt__int_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="2" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

// ---- Cross-package (foreign) IR-function emission ----

/// A reachable, eligible operation that lives in a separate library package
/// (not the entry package) is emitted as its own standalone IR function rather
/// than being inlined into the entry point. The library callable keeps its bare
/// name because nothing else competes for it, and the entry point reaches it
/// through a `call`.
#[test]
fn cross_package_library_callable_emits_standalone_define() {
    let lib = "namespace Lib {
            operation ApplyX(q : Qubit) : Unit {
                X(q);
            }
            export ApplyX;
        }";
    let user = "namespace Test {
            import Lib.*;
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ApplyX(q);
            }
        }";
    let qir = compile_source_to_qir_with_library(lib, user, *CAPABILITIES);
    // The foreign library callable is emitted exactly once as a standalone
    // definition under its bare name, proving it is not inlined.
    assert_eq!(
        qir.matches("define internal void @ApplyX(").count(),
        1,
        "expected exactly one standalone IR function for the foreign callable; got:\n{qir}"
    );
    // The entry point reaches the foreign callable through a call instruction.
    assert!(
        qir.contains("call void @ApplyX("),
        "expected the entry point to call the foreign IR function; got:\n{qir}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
    expect![[r#"
        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(ptr null)
          call void @ApplyX(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
          ret i64 0
        }

        declare void @__quantum__rt__initialize(ptr)

        define internal void @ApplyX(ptr %var_0) {
        block_1:
          call void @X(ptr %var_0)
          ret void
        }

        define internal void @X(ptr %var_1) {
        block_2:
          call void @__quantum__qis__x__body(ptr %var_1)
          ret void
        }

        declare void @__quantum__qis__x__body(ptr)

        declare void @__quantum__rt__tuple_record_output(i64, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
        attributes #1 = { "irreversible" }
        attributes #2 = { nofree nosync nounwind willreturn memory(argmem: read) }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7, !8, !9}

        !0 = !{i32 1, !"qir_major_version", i32 2}
        !1 = !{i32 7, !"qir_minor_version", i32 1}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
        !6 = !{i32 7, !"backwards_branching", i2 3}
        !7 = !{i32 1, !"arrays", i1 true}
        !8 = !{i32 1, !"ir_functions", i1 true}
        !9 = !{i32 1, !"writable_results", i1 true}
    "#]].assert_eq(&qir);
}

/// When a library package and the user (entry) package each declare a callable
/// with the same bare name, the name registry keeps the bare name for the
/// first-emitted callable and applies a package discriminator (`__p<pkg>`) to
/// the colliding one. Exactly one bare `@Foo` and one suffixed `@Foo__p...`
/// definition are produced, and the QIR renders without a duplicate `define`.
#[test]
fn cross_package_same_name_callables_get_discriminated() {
    let lib = "namespace Lib {
            operation Foo(q : Qubit) : Unit {
                X(q);
            }
            export Foo;
        }";
    let user = "namespace Test {
            operation Foo(q : Qubit) : Unit {
                Y(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Foo(q);
                Lib.Foo(q);
            }
        }";
    let qir = compile_source_to_qir_with_library(lib, user, *CAPABILITIES);
    // Exactly one callable keeps the bare `@Foo` name.
    assert_eq!(
        qir.matches("define internal void @Foo(").count(),
        1,
        "expected exactly one bare `@Foo` definition; got:\n{qir}"
    );
    // The colliding callable is emitted under a package-discriminated name.
    assert_eq!(
        qir.matches("define internal void @Foo__p").count(),
        1,
        "expected exactly one package-discriminated `@Foo__p...` definition; got:\n{qir}"
    );
    // Both distinct functions are reached through calls.
    assert_eq!(
        qir.matches("call void @Foo(").count(),
        1,
        "expected one call to the bare `@Foo`; got:\n{qir}"
    );
    assert_eq!(
        qir.matches("call void @Foo__p").count(),
        1,
        "expected one call to the discriminated `@Foo__p...`; got:\n{qir}"
    );
}

/// A user operation whose Q# identifier is literally `ENTRYPOINT__main` collides
/// with the reserved codegen entry-point symbol. The reserved-symbol guard
/// forces a discriminator on the emitted IR function so it never shadows the
/// real entry point: the entry `define i64 @ENTRYPOINT__main()` stays intact and
/// the user operation emits as a suffixed `@ENTRYPOINT__main__p...`.
#[test]
fn user_callable_named_like_entry_point_is_discriminated() {
    let source = "namespace Test {
            operation ENTRYPOINT__main(q : Qubit) : Unit {
                X(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ENTRYPOINT__main(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    // The real codegen entry point is untouched (i64 return, entry attributes).
    assert_eq!(
        qir.matches("define i64 @ENTRYPOINT__main()").count(),
        1,
        "expected the reserved entry-point definition to be intact; got:\n{qir}"
    );
    // The user operation is emitted under a discriminated name, never shadowing
    // the reserved entry symbol.
    assert_eq!(
        qir.matches("define internal void @ENTRYPOINT__main__p")
            .count(),
        1,
        "expected the user callable to be discriminated away from the reserved \
         entry symbol; got:\n{qir}"
    );
    assert!(
        !qir.contains("define internal void @ENTRYPOINT__main("),
        "the user callable must not be emitted under the reserved entry symbol; got:\n{qir}"
    );
    assert!(
        qir.contains("call void @ENTRYPOINT__main__p"),
        "expected the entry point to call the discriminated user callable; got:\n{qir}"
    );
}

/// Two distinct directly-invoked lambdas in a single user package are each
/// lifted into their own callable and emitted as separate IR functions. The
/// lifted names embed the defining item id (`.lambda_<item>`), so the two
/// emitted symbols are distinct and there is no duplicate `define`.
#[test]
fn distinct_lambdas_emit_distinct_ir_functions() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let f = qq => { X(qq); Y(qq); };
                let g = qq => { Y(qq); X(qq); };
                f(q);
                g(q);
            }
        }";
    let qir = compile_source_to_qir(source, *CAPABILITIES);
    // Each lambda is lifted and emitted as its own IR function. Lifted names
    // contain special characters and therefore render as quoted globals.
    assert_eq!(
        qir.matches("define internal void @.lambda").count(),
        2,
        "expected two distinct lifted-lambda IR functions; got:\n{qir}"
    );
    // The two emitted lambda definitions must have different names.
    let names: Vec<&str> = qir
        .match_indices("define internal void @.lambda")
        .map(|(idx, _)| {
            let rest = &qir[idx + "define internal void @".len()..];
            let end = rest.find('(').expect("lambda name should be terminated");
            &rest[..end]
        })
        .collect();
    assert_eq!(names.len(), 2, "expected two lifted-lambda names");
    assert_ne!(
        names[0], names[1],
        "expected the two lifted lambdas to have distinct names; got: {names:?}"
    );
    assert!(
        qir.contains("ir_functions"),
        "expected the ir_functions module flag; got:\n{qir}"
    );
}

/// A foreign (library) callable that returns a non-Unit value (`Result`) is not
/// eligible for IR-function emission and continues to inline into the entry
/// point; no standalone `define` is produced for it.
#[test]
fn cross_package_result_returning_callable_inlines() {
    let lib = "namespace Lib {
            import Std.Measurement.*;
            operation Measure(q : Qubit) : Result {
                MResetZ(q)
            }
            export Measure;
        }";
    let user = "namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Lib.Measure(q)
            }
        }";
    let qir = compile_source_to_qir_with_library(lib, user, *CAPABILITIES);
    assert!(
        !qir.contains("@Measure"),
        "expected the Result-returning foreign callable to inline; got:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__mresetz__body"),
        "expected the foreign callable body to inline to its intrinsic; got:\n{qir}"
    );
}

/// A foreign (library) controlled call still inlines: the controlled
/// specialization takes a synthesized dynamic-length control register with no
/// base-phase RIR representation, so no standalone controlled `define` is
/// emitted even across packages. The body specialization that does get emitted
/// preserves the same intrinsic gate it would when inlined, demonstrating
/// behavior preservation structurally.
#[test]
fn cross_package_controlled_call_inlines() {
    let lib = "namespace Lib {
            operation Op(q : Qubit) : Unit is Ctl {
                X(q);
            }
            export Op;
        }";
    let user = "namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use ctl = Qubit();
                use target = Qubit();
                Controlled Lib.Op([ctl], target);
            }
        }";
    let qir = compile_source_to_qir_with_library(lib, user, *CAPABILITIES);
    assert!(
        !qir.contains("define internal void @Op__Ctl("),
        "expected the foreign controlled specialization to inline; got:\n{qir}"
    );
    // The controlled lowering still performs the same controlled intrinsic it
    // would when inlined, preserving behavior.
    assert!(
        qir.contains("__quantum__qis__cx__body") || qir.contains("__quantum__qis__cnot__body"),
        "expected the controlled call to lower to a controlled-X intrinsic; got:\n{qir}"
    );
}
