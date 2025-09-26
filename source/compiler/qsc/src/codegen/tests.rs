// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use qsc_data_structures::{language_features::LanguageFeatures, target::TargetCapabilityFlags};
use qsc_frontend::compile::SourceMap;

use crate::codegen::qir::get_qir;

fn compile_source_to_qir(source: &str, capabilities: TargetCapabilityFlags) -> String {
    let sources = SourceMap::new([("test.qs".into(), source.into())], None);
    let language_features = LanguageFeatures::default();

    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
    get_qir(
        sources,
        language_features,
        capabilities,
        store,
        &[(std_id, None)],
    )
    .expect("Failed to generate QIR")
}

#[test]
fn code_with_errors_returns_errors() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit()
                let pi_over_two = 4.0 / 2.0;
            }
        }";
    let sources = SourceMap::new([("test.qs".into(), source.into())], None);
    let language_features = LanguageFeatures::default();
    let capabilities = TargetCapabilityFlags::empty();
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);

    expect![[r#"
        Err(
            [
                Compile(
                    WithSource {
                        sources: [
                            Source {
                                name: "test.qs",
                                contents: "namespace Test {\n            @EntryPoint()\n            operation Main() : Unit {\n                use q = Qubit()\n                let pi_over_two = 4.0 / 2.0;\n            }\n        }",
                                offset: 0,
                            },
                        ],
                        error: Frontend(
                            Error(
                                Parse(
                                    Error(
                                        Token(
                                            Semi,
                                            Keyword(
                                                Let,
                                            ),
                                            Span {
                                                lo: 129,
                                                hi: 132,
                                            },
                                        ),
                                    ),
                                ),
                            ),
                        ),
                    },
                ),
            ],
        )
    "#]]
    .assert_debug_eq(&get_qir(sources, language_features, capabilities, store, &[(std_id, None)]));
}

#[test]
fn code_returning_struct_from_entry_point_generates_errors() {
    let source = "namespace Test {
            @EntryPoint()
            operation Main() : Std.Math.Complex {
                new Std.Math.Complex { Real = 0.0, Imag = 0.0 }
            }
        }";
    let sources = SourceMap::new([("test.qs".into(), source.into())], None);
    let language_features = LanguageFeatures::default();
    let capabilities = TargetCapabilityFlags::empty();
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);

    expect![[r#"
        Err(
            [
                Pass(
                    WithSource {
                        sources: [
                            Source {
                                name: "test.qs",
                                contents: "namespace Test {\n            @EntryPoint()\n            operation Main() : Std.Math.Complex {\n                new Std.Math.Complex { Real = 0.0, Imag = 0.0 }\n            }\n        }",
                                offset: 0,
                            },
                        ],
                        error: CapabilitiesCk(
                            UseOfAdvancedOutput(
                                Span {
                                    lo: 65,
                                    hi: 69,
                                },
                            ),
                        ),
                    },
                ),
            ],
        )
    "#]]
    .assert_debug_eq(&get_qir(sources, language_features, capabilities, store, &[(std_id, None)]));
}

#[test]
fn code_returning_struct_from_entry_expr_generates_errors() {
    let source = "";
    let entry = "new Std.Math.Complex { Real = 0.0, Imag = 0.0 }";
    let sources = SourceMap::new([("test.qs".into(), source.into())], Some(entry.into()));
    let language_features = LanguageFeatures::default();
    let capabilities = TargetCapabilityFlags::empty();
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);

    expect![[r#"
        Err(
            [
                Pass(
                    WithSource {
                        sources: [
                            Source {
                                name: "<entry>",
                                contents: "new Std.Math.Complex { Real = 0.0, Imag = 0.0 }",
                                offset: 0,
                            },
                        ],
                        error: CapabilitiesCk(
                            UseOfAdvancedOutput(
                                Span {
                                    lo: 0,
                                    hi: 47,
                                },
                            ),
                        ),
                    },
                ),
            ],
        )
    "#]]
    .assert_debug_eq(&get_qir(
        sources,
        language_features,
        capabilities,
        store,
        &[(std_id, None)],
    ));
}

#[test]
fn code_returning_struct_from_block_entry_expr_generates_errors() {
    let source = "";
    let entry = "{ new Std.Math.Complex { Real = 0.0, Imag = 0.0 } }";
    let sources = SourceMap::new([("test.qs".into(), source.into())], Some(entry.into()));
    let language_features = LanguageFeatures::default();
    let capabilities = TargetCapabilityFlags::empty();
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);

    expect![[r#"
        Err(
            [
                Pass(
                    WithSource {
                        sources: [
                            Source {
                                name: "<entry>",
                                contents: "{ new Std.Math.Complex { Real = 0.0, Imag = 0.0 } }",
                                offset: 0,
                            },
                        ],
                        error: CapabilitiesCk(
                            UseOfAdvancedOutput(
                                Span {
                                    lo: 0,
                                    hi: 51,
                                },
                            ),
                        ),
                    },
                ),
            ],
        )
    "#]]
    .assert_debug_eq(&get_qir(
        sources,
        language_features,
        capabilities,
        store,
        &[(std_id, None)],
    ));
}

#[test]
fn code_returning_struct_from_if_entry_expr_generates_errors() {
    let source = "";
    let entry = "if (true) { new Std.Math.Complex { Real = 0.0, Imag = 0.0 } } else { fail \"shouldn't get here\" }";
    let sources = SourceMap::new([("test.qs".into(), source.into())], Some(entry.into()));
    let language_features = LanguageFeatures::default();
    let capabilities = TargetCapabilityFlags::empty();
    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);

    expect![[r#"
        Err(
            [
                Pass(
                    WithSource {
                        sources: [
                            Source {
                                name: "<entry>",
                                contents: "if (true) { new Std.Math.Complex { Real = 0.0, Imag = 0.0 } } else { fail \"shouldn't get here\" }",
                                offset: 0,
                            },
                        ],
                        error: CapabilitiesCk(
                            UseOfAdvancedOutput(
                                Span {
                                    lo: 0,
                                    hi: 96,
                                },
                            ),
                        ),
                    },
                ),
            ],
        )
    "#]]
    .assert_debug_eq(&get_qir(
        sources,
        language_features,
        capabilities,
        store,
        &[(std_id, None)],
    ));
}

mod base_profile {
    use expect_test::expect;
    use qsc_data_structures::target::TargetCapabilityFlags;

    use super::compile_source_to_qir;
    static CAPABILITIES: std::sync::LazyLock<TargetCapabilityFlags> =
        std::sync::LazyLock::new(TargetCapabilityFlags::empty);

    #[test]
    fn simple() {
        let source = "namespace Test {
            import Std.Math.*;
            open QIR.Intrinsic;
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let pi_over_two = 4.0 / 2.0;
                __quantum__qis__rz__body(pi_over_two, q);
                mutable some_angle = ArcSin(0.0);
                __quantum__qis__rz__body(some_angle, q);
                set some_angle = ArcCos(-1.0) / PI();
                __quantum__qis__rz__body(some_angle, q);
                __quantum__qis__mresetz__body(q)
            }
        }";

        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__rz__body(double 2.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__rz__body(double 0.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__rz__body(double 1.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__rz__body(double, %Qubit*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]]
        .assert_eq(&qir);
    }

    #[test]
    fn qubit_reuse_triggers_reindexing() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                use q = Qubit();
                (MResetZ(q), MResetZ(q))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_measurements_get_deferred() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1) = (Qubit(), Qubit());
                X(q0);
                let r0 = MResetZ(q0);
                X(q1);
                let r1 = MResetZ(q1);
                [r0, r1]
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_a0r\00"
            @1 = internal constant [6 x i8] c"1_a1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__rt__array_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_id_swap_results_in_different_id_usage() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                use (q0, q1) = (Qubit(), Qubit());
                X(q0);
                Relabel([q0, q1], [q1, q0]);
                X(q1);
                (MResetZ(q0), MResetZ(q1))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_id_swap_across_reset_uses_updated_ids() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                {
                    use (q0, q1) = (Qubit(), Qubit());
                    X(q0);
                    Relabel([q0, q1], [q1, q0]);
                    X(q1);
                    Reset(q0);
                    Reset(q1);
                }
                use (q0, q1) = (Qubit(), Qubit());
                (MResetZ(q0), MResetZ(q1))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="4" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]].assert_eq(&qir);
    }
}

mod adaptive_profile {
    use super::compile_source_to_qir;
    use expect_test::expect;
    use qsc_data_structures::target::TargetCapabilityFlags;
    static CAPABILITIES: std::sync::LazyLock<TargetCapabilityFlags> =
        std::sync::LazyLock::new(|| TargetCapabilityFlags::Adaptive);

    #[test]
    fn simple() {
        let source = "namespace Test {
            import Std.Math.*;
            open QIR.Intrinsic;
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let pi_over_two = 4.0 / 2.0;
                __quantum__qis__rz__body(pi_over_two, q);
                mutable some_angle = ArcSin(0.0);
                __quantum__qis__rz__body(some_angle, q);
                set some_angle = ArcCos(-1.0) / PI();
                __quantum__qis__rz__body(some_angle, q);
                __quantum__qis__mresetz__body(q)
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__rz__body(double 2.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__rz__body(double 0.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__rz__body(double 1.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__rz__body(double, %Qubit*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]]
        .assert_eq(&qir);
    }

    #[test]
    fn qubit_reuse_triggers_reindexing() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                use q = Qubit();
                (MResetZ(q), MResetZ(q))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn custom_measurement_generates_correct_qir() {
        let source = "namespace Test {
            operation Main() : Result {
                use q = Qubit();
                H(q);
                __quantum__qis__mx__body(q)
            }

            @Measurement()
            operation __quantum__qis__mx__body(target: Qubit) : Result {
                body intrinsic;
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__h__body(%Qubit*)

            declare void @__quantum__qis__mx__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn custom_joint_measurement_generates_correct_qir() {
        let source = "namespace Test {
            operation Main() : (Result, Result) {
                use q1 = Qubit();
                use q2 = Qubit();
                H(q1);
                H(q2);
                __quantum__qis__mzz__body(q1, q2)
            }

            @Measurement()
            operation __quantum__qis__mzz__body(q1: Qubit, q2: Qubit) : (Result, Result) {
                body intrinsic;
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mzz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__h__body(%Qubit*)

            declare void @__quantum__qis__mzz__body(%Qubit*, %Qubit*, %Result*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_measurements_not_deferred() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1) = (Qubit(), Qubit());
                X(q0);
                let r0 = MResetZ(q0);
                X(q1);
                let r1 = MResetZ(q1);
                [r0, r1]
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_a0r\00"
            @1 = internal constant [6 x i8] c"1_a1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__rt__array_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]].assert_eq(&qir);
    }
}

mod adaptive_ri_profile {

    use expect_test::expect;
    use qsc_data_structures::target::TargetCapabilityFlags;

    use super::compile_source_to_qir;
    static CAPABILITIES: std::sync::LazyLock<TargetCapabilityFlags> =
        std::sync::LazyLock::new(|| {
            TargetCapabilityFlags::Adaptive
                | TargetCapabilityFlags::QubitReset
                | TargetCapabilityFlags::IntegerComputations
        });

    #[test]
    fn simple() {
        let source = "namespace Test {
            import Std.Math.*;
            open QIR.Intrinsic;
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let pi_over_two = 4.0 / 2.0;
                __quantum__qis__rz__body(pi_over_two, q);
                mutable some_angle = ArcSin(0.0);
                __quantum__qis__rz__body(some_angle, q);
                set some_angle = ArcCos(-1.0) / PI();
                __quantum__qis__rz__body(some_angle, q);
                __quantum__qis__mresetz__body(q)
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__rz__body(double 2.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__rz__body(double 0.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__rz__body(double 1.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__rz__body(double, %Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]]
        .assert_eq(&qir);
    }

    #[test]
    fn qubit_reuse_allowed() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                use q = Qubit();
                (MResetZ(q), MResetZ(q))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_measurements_not_deferred() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1) = (Qubit(), Qubit());
                X(q0);
                let r0 = MResetZ(q0);
                X(q1);
                let r1 = MResetZ(q1);
                [r0, r1]
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_a0r\00"
            @1 = internal constant [6 x i8] c"1_a1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__array_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_id_swap_results_in_different_id_usage() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                use (q0, q1) = (Qubit(), Qubit());
                X(q0);
                Relabel([q0, q1], [q1, q0]);
                X(q1);
                (MResetZ(q0), MResetZ(q1))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_id_swap_across_reset_uses_updated_ids() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                {
                    use (q0, q1) = (Qubit(), Qubit());
                    X(q0);
                    Relabel([q0, q1], [q1, q0]);
                    X(q1);
                    Reset(q0);
                    Reset(q1);
                }
                use (q0, q1) = (Qubit(), Qubit());
                (MResetZ(q0), MResetZ(q1))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__reset__body(%Qubit*) #1

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_id_swap_with_out_of_order_release_uses_correct_ids() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                let q0 = QIR.Runtime.__quantum__rt__qubit_allocate();
                let q1 = QIR.Runtime.__quantum__rt__qubit_allocate();
                let q2 = QIR.Runtime.__quantum__rt__qubit_allocate();
                X(q0);
                X(q1);
                X(q2);
                Relabel([q0, q1], [q1, q0]);
                QIR.Runtime.__quantum__rt__qubit_release(q0);
                let q3 = QIR.Runtime.__quantum__rt__qubit_allocate();
                X(q3);
                (MResetZ(q3), MResetZ(q1))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn dynamic_integer_with_branch_and_phi_supported() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                H(q);
                MResetZ(q) == Zero ? 0 | 1
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [4 x i8] c"0_i\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_3
            block_2:
              br label %block_3
            block_3:
              %var_4 = phi i64 [0, %block_1], [1, %block_2]
              call void @__quantum__rt__int_record_output(i64 %var_4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__h__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare i1 @__quantum__rt__read_result(%Result*)

            declare void @__quantum__rt__int_record_output(i64, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn custom_reset_generates_correct_qir() {
        let source = "namespace Test {
            operation Main() : Result {
                use q = Qubit();
                __quantum__qis__custom_reset__body(q);
                M(q)
            }

            @Reset()
            operation __quantum__qis__custom_reset__body(target: Qubit) : Unit {
                body intrinsic;
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__custom_reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__custom_reset__body(%Qubit*) #1

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
        "#]]
        .assert_eq(&qir);
    }
}

mod adaptive_rif_profile {
    use super::compile_source_to_qir;
    use expect_test::expect;
    use qsc_data_structures::target::TargetCapabilityFlags;
    static CAPABILITIES: std::sync::LazyLock<TargetCapabilityFlags> =
        std::sync::LazyLock::new(|| {
            TargetCapabilityFlags::Adaptive
                | TargetCapabilityFlags::QubitReset
                | TargetCapabilityFlags::IntegerComputations
                | TargetCapabilityFlags::FloatingPointComputations
        });

    #[test]
    fn simple() {
        let source = "namespace Test {
            import Std.Math.*;
            open QIR.Intrinsic;
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let pi_over_two = 4.0 / 2.0;
                __quantum__qis__rz__body(pi_over_two, q);
                mutable some_angle = ArcSin(0.0);
                __quantum__qis__rz__body(some_angle, q);
                set some_angle = ArcCos(-1.0) / PI();
                __quantum__qis__rz__body(some_angle, q);
                __quantum__qis__mresetz__body(q)
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__rz__body(double 2.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__rz__body(double 0.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__rz__body(double 1.0, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__rz__body(double, %Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"f64"}}
        "#]]
        .assert_eq(&qir);
    }

    #[test]
    fn qubit_reuse_allowed() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                use q = Qubit();
                (MResetZ(q), MResetZ(q))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"f64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_measurements_not_deferred() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1) = (Qubit(), Qubit());
                X(q0);
                let r0 = MResetZ(q0);
                X(q1);
                let r1 = MResetZ(q1);
                [r0, r1]
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_a0r\00"
            @1 = internal constant [6 x i8] c"1_a1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__array_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"f64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_id_swap_results_in_different_id_usage() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                use (q0, q1) = (Qubit(), Qubit());
                X(q0);
                Relabel([q0, q1], [q1, q0]);
                X(q1);
                (MResetZ(q0), MResetZ(q1))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"f64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_id_swap_across_reset_uses_updated_ids() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                {
                    use (q0, q1) = (Qubit(), Qubit());
                    X(q0);
                    Relabel([q0, q1], [q1, q0]);
                    X(q1);
                    Reset(q0);
                    Reset(q1);
                }
                use (q0, q1) = (Qubit(), Qubit());
                (MResetZ(q0), MResetZ(q1))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__reset__body(%Qubit*) #1

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"f64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn qubit_id_swap_with_out_of_order_release_uses_correct_ids() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                let q0 = QIR.Runtime.__quantum__rt__qubit_allocate();
                let q1 = QIR.Runtime.__quantum__rt__qubit_allocate();
                let q2 = QIR.Runtime.__quantum__rt__qubit_allocate();
                X(q0);
                X(q1);
                X(q2);
                Relabel([q0, q1], [q1, q0]);
                QIR.Runtime.__quantum__rt__qubit_release(q0);
                let q3 = QIR.Runtime.__quantum__rt__qubit_allocate();
                X(q3);
                (MResetZ(q3), MResetZ(q1))
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [6 x i8] c"0_t0r\00"
            @1 = internal constant [6 x i8] c"1_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([1 x i8], [1 x i8]* @empty_tag, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"f64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn dynamic_integer_with_branch_and_phi_supported() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                H(q);
                MResetZ(q) == Zero ? 0 | 1
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [4 x i8] c"0_i\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_3
            block_2:
              br label %block_3
            block_3:
              %var_4 = phi i64 [0, %block_1], [1, %block_2]
              call void @__quantum__rt__int_record_output(i64 %var_4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__h__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare i1 @__quantum__rt__read_result(%Result*)

            declare void @__quantum__rt__int_record_output(i64, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"f64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn dynamic_double_with_branch_and_phi_supported() {
        let source = "namespace Test {
            @EntryPoint()
            operation Main() : Double {
                use q = Qubit();
                H(q);
                MResetZ(q) == Zero ? 0.0 | 1.0
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [4 x i8] c"0_d\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_3
            block_2:
              br label %block_3
            block_3:
              %var_4 = phi double [0.0, %block_1], [1.0, %block_2]
              call void @__quantum__rt__double_record_output(double %var_4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__h__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare i1 @__quantum__rt__read_result(%Result*)

            declare void @__quantum__rt__double_record_output(double, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"f64"}}
        "#]].assert_eq(&qir);
    }

    #[test]
    fn custom_reset_generates_correct_qir() {
        let source = "namespace Test {
            operation Main() : Result {
                use q = Qubit();
                __quantum__qis__custom_reset__body(q);
                M(q)
            }

            @Reset()
            operation __quantum__qis__custom_reset__body(target: Qubit) : Unit {
                body intrinsic;
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__custom_reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__custom_reset__body(%Qubit*) #1

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"f64"}}
        "#]]
        .assert_eq(&qir);
    }

    #[test]
    fn dynamic_double_intrinsic() {
        let source = "namespace Test {
            operation OpA(theta: Double, q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Double {
                use q = Qubit();
                H(q);
                let theta = MResetZ(q) == Zero ? 0.0 | 1.0;
                OpA(1.0 + theta, q);
                Rx(2.0 * theta, q);
                Ry(theta / 3.0, q);
                Rz(theta - 4.0, q);
                OpA(theta, q);
                Rx(theta, q);
                theta
            }
        }";
        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @empty_tag = internal constant [1 x i8] c"\00"
            @0 = internal constant [4 x i8] c"0_d\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              %var_0 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              br label %block_3
            block_2:
              br label %block_3
            block_3:
              %var_9 = phi double [0.0, %block_1], [1.0, %block_2]
              %var_4 = fadd double 1.0, %var_9
              call void @OpA(double %var_4, %Qubit* inttoptr (i64 0 to %Qubit*))
              %var_5 = fmul double 2.0, %var_9
              call void @__quantum__qis__rx__body(double %var_5, %Qubit* inttoptr (i64 0 to %Qubit*))
              %var_6 = fdiv double %var_9, 3.0
              call void @__quantum__qis__ry__body(double %var_6, %Qubit* inttoptr (i64 0 to %Qubit*))
              %var_7 = fsub double %var_9, 4.0
              call void @__quantum__qis__rz__body(double %var_7, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @OpA(double %var_9, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__rx__body(double %var_9, %Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__rt__double_record_output(double %var_9, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__h__body(%Qubit*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare i1 @__quantum__rt__read_result(%Result*)

            declare void @OpA(double, %Qubit*)

            declare void @__quantum__qis__rx__body(double, %Qubit*)

            declare void @__quantum__qis__ry__body(double, %Qubit*)

            declare void @__quantum__qis__rz__body(double, %Qubit*)

            declare void @__quantum__rt__double_record_output(double, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"f64"}}
        "#]].assert_eq(&qir);
    }
}
