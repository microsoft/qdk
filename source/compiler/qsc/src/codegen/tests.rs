// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use std::sync::Arc;

use std::rc::Rc;

use expect_test::expect;
use miette::Report;
use qsc_data_structures::{
    functors::FunctorApp, language_features::LanguageFeatures, source::SourceMap,
    target::TargetCapabilityFlags,
};
use qsc_eval::val::Value;
use qsc_frontend::compile::parse_all;
use qsc_hir::hir::{ItemKind, PackageId};
use rustc_hash::FxHashMap;

use crate::codegen::qir::{
    get_qir, get_qir_from_ast, get_rir, prepare_codegen_fir_from_callable_args,
};

fn format_interpret_errors(errors: Vec<crate::interpret::Error>) -> String {
    errors
        .into_iter()
        .map(|error| format!("{:?}", Report::new(error)))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn source_map_from_source(source: &str) -> SourceMap {
    SourceMap::new([("test.qs".into(), source.into())], None)
}

fn parse_source_to_ast(source: &str) -> (qsc_ast::ast::Package, SourceMap) {
    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (ast_package, errors) = parse_all(&sources, language_features);

    if errors.is_empty() {
        (ast_package, sources)
    } else {
        let diagnostics = errors
            .into_iter()
            .map(|error| format!("{:?}", Report::new(error)))
            .collect::<Vec<_>>()
            .join("\n\n");

        panic!("Failed to parse AST test source:\n{diagnostics}");
    }
}

fn compile_source_to_qir(source: &str, capabilities: TargetCapabilityFlags) -> String {
    match compile_source_to_qir_result(source, capabilities) {
        Ok(qir) => qir,
        Err(errors) => panic!(
            "Failed to generate QIR for capabilities {capabilities:?}:\n{}",
            format_interpret_errors(errors)
        ),
    }
}

fn compile_source_to_qir_result(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> Result<String, Vec<crate::interpret::Error>> {
    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();

    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
    get_qir(
        sources,
        language_features,
        capabilities,
        store,
        &[(std_id, None)],
    )
}

fn compile_source_to_qir_from_ast(source: &str, capabilities: TargetCapabilityFlags) -> String {
    match compile_source_to_qir_from_ast_result(source, capabilities) {
        Ok(qir) => qir,
        Err(errors) => panic!(
            "Failed to generate QIR from AST for capabilities {capabilities:?}:\n{}",
            format_interpret_errors(errors)
        ),
    }
}

fn compile_source_to_qir_from_ast_result(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> Result<String, Vec<crate::interpret::Error>> {
    let (ast_package, sources) = parse_source_to_ast(source);
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> =
        vec![(PackageId::CORE, None), (std_id, None)];

    get_qir_from_ast(
        &mut store,
        &dependencies,
        ast_package,
        sources,
        capabilities,
    )
}

fn compile_source_to_rir(source: &str, capabilities: TargetCapabilityFlags) -> Vec<String> {
    match compile_source_to_rir_result(source, capabilities) {
        Ok(rir) => rir,
        Err(errors) => panic!(
            "Failed to generate RIR for capabilities {capabilities:?}:\n{}",
            format_interpret_errors(errors)
        ),
    }
}

fn compile_source_to_rir_result(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> Result<Vec<String>, Vec<crate::interpret::Error>> {
    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();

    let (std_id, store) = crate::compile::package_store_with_stdlib(capabilities);
    get_rir(
        sources,
        language_features,
        capabilities,
        store,
        &[(std_id, None)],
    )
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
fn unsupported_profile_patterns_return_pass_errors() {
    let res = compile_source_to_qir_result(
        indoc::indoc! {r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Int {
                    use q = Qubit();
                    mutable x = 1;
                    if MResetZ(q) == One {
                        set x = 2;
                    }
                    x
                }
            }
        "#},
        TargetCapabilityFlags::Adaptive,
    );

    let errors = res.expect_err("expected capability error");
    assert!(!errors.is_empty(), "expected at least one error");
    assert!(
        errors
            .iter()
            .all(|error| matches!(error, crate::interpret::Error::Pass(_))),
        "expected pass-derived codegen readiness errors, got {errors:?}"
    );
    assert!(
        errors.iter().any(|error| error
            .to_string()
            .contains("cannot use a dynamic integer value")),
        "expected a dynamic integer capability diagnostic, got {errors:?}"
    );
}

#[test]
fn qir_generation_succeeds_for_struct_copy_update() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                struct Point3d { X : Double, Y : Double, Z : Double }

                let point = new Point3d { X = 1.0, Y = 2.0, Z = 3.0 };
                let point2 = new Point3d { ...point, Z = 4.0 };
                let x : Double = point2.X;
            }
        }
    "#;

    let qir = compile_source_to_qir(source, TargetCapabilityFlags::empty());
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_t\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__rt__tuple_record_output(i64 0, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__rt__tuple_record_output(i64, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="0" }
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
fn deutsch_jozsa_sample_shape_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Diagnostics.*;
            import Std.Math.*;
            import Std.Measurement.*;

            @EntryPoint()
            operation Main() : Bool[] {
                let functionsToTest = [
                    SimpleConstantBoolF,
                    SimpleBalancedBoolF,
                    ConstantBoolF,
                    BalancedBoolF
                ];

                mutable results = [];
                for fn in functionsToTest {
                    let isConstant = DeutschJozsa(fn, 3);
                    set results += [isConstant];
                }

                return results;
            }

            operation DeutschJozsa(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
                use queryRegister = Qubit[n];
                use target = Qubit();
                X(target);
                H(target);
                within {
                    for q in queryRegister {
                        H(q);
                    }
                } apply {
                    Uf(queryRegister, target);
                }

                mutable result = true;
                for q in queryRegister {
                    if MResetZ(q) == One {
                        set result = false;
                    }
                }

                Reset(target);
                return result;
            }

            operation SimpleConstantBoolF(args : Qubit[], target : Qubit) : Unit {
                X(target);
            }

            operation SimpleBalancedBoolF(args : Qubit[], target : Qubit) : Unit {
                CX(args[0], target);
            }

            operation ConstantBoolF(args : Qubit[], target : Qubit) : Unit {
                for i in 0..(2^Length(args)) - 1 {
                    ApplyControlledOnInt(i, X, args, target);
                }
            }

            operation BalancedBoolF(args : Qubit[], target : Qubit) : Unit {
                for i in 0..2..(2^Length(args)) - 1 {
                    ApplyControlledOnInt(i, X, args, target);
                }
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive
            | TargetCapabilityFlags::IntegerComputations
            | TargetCapabilityFlags::FloatingPointComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0b\00"
        @2 = internal constant [6 x i8] c"2_a1b\00"
        @3 = internal constant [6 x i8] c"3_a2b\00"
        @4 = internal constant [6 x i8] c"4_a3b\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          %var_6 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          br i1 %var_6, label %block_1, label %block_2
        block_1:
          br label %block_2
        block_2:
          %var_139 = phi i1 [true, %block_0], [false, %block_1]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          %var_8 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
          br i1 %var_8, label %block_3, label %block_4
        block_3:
          br label %block_4
        block_4:
          %var_140 = phi i1 [%var_139, %block_2], [false, %block_3]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
          %var_10 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
          br i1 %var_10, label %block_5, label %block_6
        block_5:
          br label %block_6
        block_6:
          %var_141 = phi i1 [%var_140, %block_4], [false, %block_5]
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 3 to %Result*))
          %var_19 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
          br i1 %var_19, label %block_7, label %block_8
        block_7:
          br label %block_8
        block_8:
          %var_142 = phi i1 [true, %block_6], [false, %block_7]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
          %var_21 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
          br i1 %var_21, label %block_9, label %block_10
        block_9:
          br label %block_10
        block_10:
          %var_143 = phi i1 [%var_142, %block_8], [false, %block_9]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
          %var_23 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
          br i1 %var_23, label %block_11, label %block_12
        block_11:
          br label %block_12
        block_12:
          %var_144 = phi i1 [%var_143, %block_10], [false, %block_11]
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 6 to %Result*))
          %var_89 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 6 to %Result*))
          br i1 %var_89, label %block_13, label %block_14
        block_13:
          br label %block_14
        block_14:
          %var_145 = phi i1 [true, %block_12], [false, %block_13]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
          %var_91 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 7 to %Result*))
          br i1 %var_91, label %block_15, label %block_16
        block_15:
          br label %block_16
        block_16:
          %var_146 = phi i1 [%var_145, %block_14], [false, %block_15]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 8 to %Result*))
          %var_93 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 8 to %Result*))
          br i1 %var_93, label %block_17, label %block_18
        block_17:
          br label %block_18
        block_18:
          %var_147 = phi i1 [%var_146, %block_16], [false, %block_17]
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*), %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__ccx__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 4 to %Qubit*))
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 9 to %Result*))
          %var_131 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 9 to %Result*))
          br i1 %var_131, label %block_19, label %block_20
        block_19:
          br label %block_20
        block_20:
          %var_148 = phi i1 [true, %block_18], [false, %block_19]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 10 to %Result*))
          %var_133 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 10 to %Result*))
          br i1 %var_133, label %block_21, label %block_22
        block_21:
          br label %block_22
        block_22:
          %var_149 = phi i1 [%var_148, %block_20], [false, %block_21]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 11 to %Result*))
          %var_135 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 11 to %Result*))
          br i1 %var_135, label %block_23, label %block_24
        block_23:
          br label %block_24
        block_24:
          %var_150 = phi i1 [%var_149, %block_22], [false, %block_23]
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__rt__array_record_output(i64 4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 %var_141, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 %var_144, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 %var_147, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 %var_150, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare i1 @__quantum__rt__read_result(%Result*)

        declare void @__quantum__qis__reset__body(%Qubit*) #1

        declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

        declare void @__quantum__qis__ccx__body(%Qubit*, %Qubit*, %Qubit*)

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__bool_record_output(i1, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="12" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
    "#]]
        .assert_eq(&qir);
}

#[test]
fn simple_phase_estimation_sample_shape_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Main() : Result[] {
                use state = Qubit();
                use phase = Qubit[3];

                X(state);

                let oracle = ApplyOperationPowerCA(_, qs => U(qs[0]), _);
                ApplyQPE(oracle, [state], phase);

                let results = MeasureEachZ(phase);

                Reset(state);
                ResetAll(phase);

                Std.Arrays.Reversed(results)
            }

            operation U(q : Qubit) : Unit is Ctl + Adj {
                Rz(Std.Math.PI() / 3.0, q);
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive
            | TargetCapabilityFlags::IntegerComputations
            | TargetCapabilityFlags::FloatingPointComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"
        @3 = internal constant [6 x i8] c"3_a2r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.5235987755982988, %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.7853981633974483, %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.7853981633974483, %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.39269908169872414, %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.39269908169872414, %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.39269908169872414, %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.7853981633974483, %Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__rz__body(double -0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__rz__body(double 0.7853981633974483, %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__cx__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 3 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 2 to %Qubit*))
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__rt__array_record_output(i64 3, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 2 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

        declare void @__quantum__qis__h__body(%Qubit*)

        declare void @__quantum__qis__rz__body(double, %Qubit*)

        declare void @__quantum__qis__cx__body(%Qubit*, %Qubit*)

        declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

        declare void @__quantum__qis__reset__body(%Qubit*) #1

        declare void @__quantum__rt__array_record_output(i64, i8*)

        declare void @__quantum__rt__result_record_output(%Result*, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="3" }
        attributes #1 = { "irreversible" }

        ; module flags

        !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

        !0 = !{i32 1, !"qir_major_version", i32 1}
        !1 = !{i32 7, !"qir_minor_version", i32 0}
        !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
        !3 = !{i32 1, !"dynamic_result_management", i1 false}
        !4 = !{i32 5, !"int_computations", !{!"i64"}}
        !5 = !{i32 5, !"float_computations", !{!"double"}}
    "#]]
        .assert_eq(&qir);
}

#[test]
fn explicit_return_tuple_keeps_dynamic_integer_output() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            @EntryPoint()
            operation Main() : (Int, Bool) {
                use q = Qubit();
                mutable a = 0;
                if MResetZ(q) == Zero {
                    set a = 1;
                } else {
                    set a = 2;
                }

                use p = Qubit();
                return (a, MResetZ(p) == One);
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_t\00"
        @1 = internal constant [6 x i8] c"1_t0i\00"
        @2 = internal constant [6 x i8] c"2_t1b\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          %var_1 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          %var_2 = icmp eq i1 %var_1, false
          br i1 %var_2, label %block_1, label %block_2
        block_1:
          br label %block_3
        block_2:
          br label %block_3
        block_3:
          %var_5 = phi i64 [1, %block_1], [2, %block_2]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          %var_3 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__int_record_output(i64 %var_5, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 %var_3, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

        declare i1 @__quantum__rt__read_result(%Result*)

        declare void @__quantum__rt__tuple_record_output(i64, i8*)

        declare void @__quantum__rt__int_record_output(i64, i8*)

        declare void @__quantum__rt__bool_record_output(i1, i8*)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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
fn result_array_helper_return_survives_adaptive_codegen_prep() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            @EntryPoint()
            operation Main() : Result[] {
                use register = Qubit[2];
                return MResetZ2Register(register);
            }

            operation MResetZ2Register(register : Qubit[]) : Result[] {
                return [MResetZ(register[0]), MResetZ(register[1])];
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

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
    "#]]
        .assert_eq(&qir);
}

#[test]
fn higher_order_closure_captures_are_threaded_into_specialized_calls() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Canon.*;
            import Std.Measurement.*;

            operation ApplyOp(op : (Qubit[] => Unit), register : Qubit[]) : Result[] {
                op(register);
                return MResetEachZ(register);
            }

            @EntryPoint()
            operation Main() : Result[] {
                use register = Qubit[2];
                return ApplyOp(register => Shifted(1, register), register);
            }

            operation Shifted(shift : Int, register : Qubit[]) : Unit {
                ApplyXorInPlace(shift, register);
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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
    "#]]
        .assert_eq(&qir);
}

#[test]
fn two_callable_hof_closure_preserves_array_arg_threading() {
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Arrays.*;
            import Std.Canon.*;
            import Std.Convert.*;
            import Std.Measurement.*;

            operation Outer(Ufstar : (Qubit[] => Unit), Ug : (Qubit[] => Unit), n : Int) : Result[] {
                use qubits = Qubit[n];
                Ug(qubits);
                return MResetEachZ(qubits);
            }

            operation Empty(register : Qubit[]) : Unit {
            }

            operation ShiftedSimple(shift : Int, register : Qubit[]) : Unit {
                ApplyXorInPlace(shift, register);
            }

            @EntryPoint()
            operation Main() : Result[] {
                let bits = [true, false];
                let shift = BoolArrayAsInt(bits);
                let n = Length(bits);
                return Outer(Empty, register => ShiftedSimple(shift, register), n);
            }
        }
    "#};

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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
    "#]]
        .assert_eq(&qir);
}

#[test]
fn callable_args_with_arrow_input_survives_dce() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit) : Result {
                use q = Qubit();
                op(q);
                MResetZ(q)
            }
            operation MyOp(q : Qubit) : Unit { H(q); }
        }
    "#};

    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;

    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];

    let (unit, errors) = crate::compile::compile(
        &store,
        &dependencies,
        sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(errors.is_empty(), "compilation failed: {errors:?}");
    let package_id = store.insert(unit);

    // Find ApplyOp and MyOp by name in the HIR package.
    let hir_package = &store.get(package_id).expect("package should exist").package;
    let mut apply_op_local = None;
    let mut my_op_local = None;
    for (local_id, item) in hir_package.items.iter() {
        if let ItemKind::Callable(decl) = &item.kind {
            if decl.name.name.as_ref() == "ApplyOp" {
                apply_op_local = Some(local_id);
            } else if decl.name.name.as_ref() == "MyOp" {
                my_op_local = Some(local_id);
            }
        }
    }
    let apply_op_local = apply_op_local.expect("ApplyOp should exist in HIR");
    let my_op_local = my_op_local.expect("MyOp should exist in HIR");

    let apply_op_hir_id = qsc_hir::hir::ItemId {
        package: package_id,
        item: apply_op_local,
    };

    // Construct Value::Global for MyOp using FIR StoreItemId.
    let my_op_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(my_op_local),
    };
    let my_op_value = Value::Global(my_op_fir_id, FunctorApp::default());

    // The synthetic Call path makes ApplyOp entry-reachable. Defunc specializes
    // it to ApplyOp{MyOp}, and the pipeline transforms it fully. The original
    // ApplyOp is pinned for DCE survival so fir_to_qir_from_callable can use
    // the original ID with the original-shaped args.
    let codegen_fir =
        prepare_codegen_fir_from_callable_args(&store, apply_op_hir_id, &my_op_value, capabilities)
            .unwrap_or_else(|errors| {
                panic!(
                    "callable-args with arrow-input should survive DCE, got: {}",
                    format_interpret_errors(errors)
                )
            });

    let backend_callable = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(apply_op_hir_id.package),
        item: qsc_lowerer::map_hir_local_item_to_fir(apply_op_hir_id.item),
    };

    let qir = qsc_codegen::qir::fir_to_qir_from_callable(
        &codegen_fir.fir_store,
        capabilities,
        &codegen_fir.compute_properties,
        backend_callable,
        my_op_value,
    )
    .unwrap_or_else(|e| panic!("QIR generation from arrow-input callable should succeed: {e:?}"));

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__h__body(%Qubit*)

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
        !5 = !{i32 5, !"float_computations", !{!"double"}}
    "#]]
    .assert_eq(&qir);
}

#[test]
fn callable_args_with_udt_wrapped_arrow_survives_dce() {
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Config = (Op: Qubit => Unit, Data: Int);
            operation Apply(cfg: Config) : Unit {
                use q = Qubit();
                cfg::Op(q);
            }
            operation MyOp(q: Qubit) : Unit { H(q); }
        }
    "#};

    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;

    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];

    let (unit, errors) = crate::compile::compile(
        &store,
        &dependencies,
        sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(errors.is_empty(), "compilation failed: {errors:?}");
    let package_id = store.insert(unit);

    let hir_package = &store.get(package_id).expect("package should exist").package;
    let mut apply_local = None;
    let mut my_op_local = None;
    let mut config_udt_local = None;
    for (local_id, item) in hir_package.items.iter() {
        match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Apply" => {
                apply_local = Some(local_id);
            }
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "MyOp" => {
                my_op_local = Some(local_id);
            }
            ItemKind::Ty(name, _) if name.name.as_ref() == "Config" => {
                config_udt_local = Some(local_id);
            }
            _ => {}
        }
    }
    let apply_local = apply_local.expect("Apply should exist in HIR");
    let my_op_local = my_op_local.expect("MyOp should exist in HIR");

    let apply_hir_id = qsc_hir::hir::ItemId {
        package: package_id,
        item: apply_local,
    };

    let my_op_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(my_op_local),
    };
    let my_op_value = Value::Global(my_op_fir_id, FunctorApp::default());

    // Build a Config UDT value: Config(MyOp, 42)
    // UDT values are Value::Tuple(Rc<[Value]>, Option<Rc<StoreItemId>>)
    let config_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(
            config_udt_local.expect("Config UDT should exist"),
        ),
    };
    let config_value = Value::Tuple(
        vec![my_op_value, Value::Int(42)].into(),
        Some(Rc::new(config_fir_id)),
    );

    let result =
        prepare_codegen_fir_from_callable_args(&store, apply_hir_id, &config_value, capabilities);
    match result {
        Ok(_) => {}
        Err(errors) => panic!(
            "callable-args with UDT-wrapped arrow should survive DCE, got: {}",
            format_interpret_errors(errors)
        ),
    }
}

#[test]
fn callable_with_udt_wrapped_arrow_generates_qir_via_callable_args() {
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Config = (Op: Qubit => Unit, Data: Int);
            operation Apply(cfg: Config) : Result {
                use q = Qubit();
                cfg::Op(q);
                MResetZ(q)
            }
            operation MyOp(q: Qubit) : Unit { H(q); }
        }
    "#};

    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;

    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];

    let (unit, errors) = crate::compile::compile(
        &store,
        &dependencies,
        sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(errors.is_empty(), "compilation failed: {errors:?}");
    let package_id = store.insert(unit);

    let hir_package = &store.get(package_id).expect("package should exist").package;
    let mut apply_local = None;
    let mut my_op_local = None;
    let mut config_udt_local = None;
    for (local_id, item) in hir_package.items.iter() {
        match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Apply" => {
                apply_local = Some(local_id);
            }
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "MyOp" => {
                my_op_local = Some(local_id);
            }
            ItemKind::Ty(name, _) if name.name.as_ref() == "Config" => {
                config_udt_local = Some(local_id);
            }
            _ => {}
        }
    }
    let apply_local = apply_local.expect("Apply should exist in HIR");
    let my_op_local = my_op_local.expect("MyOp should exist in HIR");

    let apply_hir_id = qsc_hir::hir::ItemId {
        package: package_id,
        item: apply_local,
    };

    let my_op_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(my_op_local),
    };
    let my_op_value = Value::Global(my_op_fir_id, FunctorApp::default());

    let config_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(
            config_udt_local.expect("Config UDT should exist"),
        ),
    };
    let config_value = Value::Tuple(
        vec![my_op_value, Value::Int(42)].into(),
        Some(Rc::new(config_fir_id)),
    );

    let codegen_fir =
        prepare_codegen_fir_from_callable_args(&store, apply_hir_id, &config_value, capabilities)
            .unwrap_or_else(|errors| {
                panic!(
                    "callable-args with UDT-wrapped arrow should produce CodegenFir, got: {}",
                    format_interpret_errors(errors)
                )
            });

    let backend_callable = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(apply_hir_id.package),
        item: qsc_lowerer::map_hir_local_item_to_fir(apply_hir_id.item),
    };

    let qir = qsc_codegen::qir::fir_to_qir_from_callable(
        &codegen_fir.fir_store,
        capabilities,
        &codegen_fir.compute_properties,
        backend_callable,
        config_value,
    )
    .unwrap_or_else(|e| panic!("QIR generation from UDT-wrapped arrow should succeed: {e:?}"));

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__h__body(%Qubit*)

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
        !5 = !{i32 5, !"float_computations", !{!"double"}}
    "#]]
        .assert_eq(&qir);
}

#[test]
fn callable_with_nested_udt_wrapped_arrow_generates_qir_via_callable_args() {
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype OpWrapper = (Op: Qubit => Unit);
            newtype Config = (Inner: OpWrapper, Count: Int);
            operation Apply(cfg: Config) : Result {
                use q = Qubit();
                cfg::Inner::Op(q);
                MResetZ(q)
            }
            operation MyOp(q: Qubit) : Unit { X(q); }
        }
    "#};

    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;

    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];

    let (unit, errors) = crate::compile::compile(
        &store,
        &dependencies,
        sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(errors.is_empty(), "compilation failed: {errors:?}");
    let package_id = store.insert(unit);

    let hir_package = &store.get(package_id).expect("package should exist").package;
    let mut apply_local = None;
    let mut my_op_local = None;
    let mut config_udt_local = None;
    for (local_id, item) in hir_package.items.iter() {
        match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Apply" => {
                apply_local = Some(local_id);
            }
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "MyOp" => {
                my_op_local = Some(local_id);
            }
            ItemKind::Ty(name, _) if name.name.as_ref() == "Config" => {
                config_udt_local = Some(local_id);
            }
            _ => {}
        }
    }
    let apply_local = apply_local.expect("Apply should exist in HIR");
    let my_op_local = my_op_local.expect("MyOp should exist in HIR");

    let apply_hir_id = qsc_hir::hir::ItemId {
        package: package_id,
        item: apply_local,
    };

    let my_op_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(my_op_local),
    };
    let my_op_value = Value::Global(my_op_fir_id, FunctorApp::default());

    let config_fir_id = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(
            config_udt_local.expect("Config UDT should exist"),
        ),
    };
    let config_value = Value::Tuple(
        vec![my_op_value, Value::Int(5)].into(),
        Some(Rc::new(config_fir_id)),
    );

    let codegen_fir = prepare_codegen_fir_from_callable_args(
        &store,
        apply_hir_id,
        &config_value,
        capabilities,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "callable-args with nested UDT-wrapped arrow should produce CodegenFir, got: {}",
            format_interpret_errors(errors)
        )
    });

    let backend_callable = qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(apply_hir_id.package),
        item: qsc_lowerer::map_hir_local_item_to_fir(apply_hir_id.item),
    };

    let qir = qsc_codegen::qir::fir_to_qir_from_callable(
        &codegen_fir.fir_store,
        capabilities,
        &codegen_fir.compute_properties,
        backend_callable,
        config_value,
    )
    .unwrap_or_else(|e| {
        panic!("QIR generation from nested UDT-wrapped arrow should succeed: {e:?}")
    });

    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

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
        !5 = !{i32 5, !"float_computations", !{!"double"}}
    "#]].assert_eq(&qir);
}

// ---------------------------------------------------------------------------
// Synthetic-path and fallback-path coverage for callable-args codegen
// ---------------------------------------------------------------------------

/// Helper: compile a lib package, locate named items, and return (`store`, `package_id`, `items_map`).
/// `item_names` maps display names to a bool: true = Callable, false = Ty (UDT).
fn compile_and_locate_items(
    source: &str,
    item_names: &[(&str, bool)],
    capabilities: TargetCapabilityFlags,
) -> (
    crate::PackageStore,
    PackageId,
    FxHashMap<String, qsc_hir::hir::LocalItemId>,
) {
    let sources = source_map_from_source(source);
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);
    let dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];
    let (unit, errors) = crate::compile::compile(
        &store,
        &dependencies,
        sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(errors.is_empty(), "compilation failed: {errors:?}");
    let package_id = store.insert(unit);

    let hir_package = &store.get(package_id).expect("package should exist").package;
    let mut found = FxHashMap::default();
    for (local_id, item) in hir_package.items.iter() {
        match &item.kind {
            ItemKind::Callable(decl) => {
                for &(name, is_callable) in item_names {
                    if is_callable && decl.name.name.as_ref() == name {
                        found.insert(name.to_string(), local_id);
                    }
                }
            }
            ItemKind::Ty(name, _) => {
                for &(item_name, is_callable) in item_names {
                    if !is_callable && name.name.as_ref() == item_name {
                        found.insert(item_name.to_string(), local_id);
                    }
                }
            }
            _ => {}
        }
    }
    for &(name, _) in item_names {
        assert!(
            found.contains_key(name),
            "{name} should exist in HIR package"
        );
    }
    (store, package_id, found)
}

/// Returns the target capabilities used by callable-args synthetic path tests.
fn adaptive_capabilities() -> TargetCapabilityFlags {
    TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations
}

/// Maps a HIR local item ID in the test package to its corresponding FIR item ID.
fn fir_id_for(
    package_id: PackageId,
    local_id: qsc_hir::hir::LocalItemId,
) -> qsc_fir::fir::StoreItemId {
    qsc_fir::fir::StoreItemId {
        package: qsc_lowerer::map_hir_package_to_fir(package_id),
        item: qsc_lowerer::map_hir_local_item_to_fir(local_id),
    }
}

/// Builds a HIR item ID from a test package ID and local item ID.
fn hir_id_for(package_id: PackageId, local_id: qsc_hir::hir::LocalItemId) -> qsc_hir::hir::ItemId {
    qsc_hir::hir::ItemId {
        package: package_id,
        item: local_id,
    }
}

/// Runs `prepare_codegen_fir_from_callable_args` and then `fir_to_qir_from_callable`,
/// returning the QIR string.
fn callable_args_to_qir(
    store: &crate::PackageStore,
    package_id: PackageId,
    target_local: qsc_hir::hir::LocalItemId,
    args: &Value,
    capabilities: TargetCapabilityFlags,
) -> String {
    let target_hir = hir_id_for(package_id, target_local);
    let codegen_fir = prepare_codegen_fir_from_callable_args(store, target_hir, args, capabilities)
        .unwrap_or_else(|errors| {
            panic!(
                "prepare_codegen_fir_from_callable_args failed: {}",
                format_interpret_errors(errors)
            )
        });
    let backend_callable = fir_id_for(package_id, target_local);
    qsc_codegen::qir::fir_to_qir_from_callable(
        &codegen_fir.fir_store,
        capabilities,
        &codegen_fir.compute_properties,
        backend_callable,
        args.clone(),
    )
    .unwrap_or_else(|e| panic!("fir_to_qir_from_callable failed: {e:?}"))
}

// ---- Synthetic path: arrow + non-callable params (tuple input) ----

#[test]
fn synthetic_path_arrow_and_int_tuple_generates_qir() {
    // Target takes (op: Qubit => Unit, count: Int). Only the callable flows
    // through `args`; count is provided as a plain Int value.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation RunOp(op : Qubit => Unit, count : Int) : Result {
                use q = Qubit();
                for _ in 0..count - 1 {
                    op(q);
                }
                MResetZ(q)
            }
            operation MyH(q : Qubit) : Unit { H(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunOp", true), ("MyH", true)], caps);

    let my_h = Value::Global(fir_id_for(pkg, items["MyH"]), FunctorApp::default());
    let args = Value::Tuple(vec![my_h, Value::Int(3)].into(), None);

    let qir = callable_args_to_qir(&store, pkg, items["RunOp"], &args, caps);
    // The QIR must contain an h__body call from the loop body.
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected h gate in QIR:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__mresetz__body"),
        "expected mresetz in QIR:\n{qir}"
    );
}

// ---- Synthetic path: two callable args in a tuple ----

#[test]
fn synthetic_path_two_arrow_args_generates_qir() {
    // Target takes (op1: Qubit => Unit, op2: Qubit => Unit). Both are
    // Global values — the synthetic Call must place both at their respective
    // tuple positions.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation ApplyBoth(op1 : Qubit => Unit, op2 : Qubit => Unit) : Result {
                use q = Qubit();
                op1(q);
                op2(q);
                MResetZ(q)
            }
            operation DoH(q : Qubit) : Unit { H(q); }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("ApplyBoth", true), ("DoH", true), ("DoX", true)],
        caps,
    );

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let args = Value::Tuple(vec![do_h, do_x].into(), None);

    let qir = callable_args_to_qir(&store, pkg, items["ApplyBoth"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: arrow sandwiched between non-callable params ----

#[test]
fn synthetic_path_int_arrow_bool_tuple_generates_qir() {
    // Target takes (n: Int, op: Qubit => Unit, flag: Bool). The callable is
    // in the middle of the tuple — exercises the element-wise matching logic
    // in `build_synthetic_args`.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Middle(n : Int, op : Qubit => Unit, flag : Bool) : Result {
                use q = Qubit();
                if flag {
                    for _ in 0..n - 1 { op(q); }
                }
                MResetZ(q)
            }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Middle", true), ("DoX", true)], caps);

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let args = Value::Tuple(vec![Value::Int(2), do_x, Value::Bool(true)].into(), None);

    let qir = callable_args_to_qir(&store, pkg, items["Middle"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: no callable args (pure values) ----

#[test]
fn no_callable_args_takes_early_return_path() {
    // When args contain no callable values, `prepare_codegen_fir_from_callable_args`
    // takes the `concrete_callables.is_empty()` early return to `prepare_codegen_fir_from_callable`.
    // This exercises that branch.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Simple(n : Int) : Result {
                use q = Qubit();
                for _ in 0..n - 1 { H(q); }
                MResetZ(q)
            }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) = compile_and_locate_items(source, &[("Simple", true)], caps);

    let args = Value::Int(3);
    let target_hir = hir_id_for(pkg, items["Simple"]);

    // Should succeed without error — takes the no-callable early path.
    let result = prepare_codegen_fir_from_callable_args(&store, target_hir, &args, caps);
    assert!(
        result.is_ok(),
        "no-callable args should succeed: {:?}",
        result.err().map(format_interpret_errors)
    );
}

// ---- Synthetic path: struct with callable and non-callable fields ----

#[test]
fn synthetic_path_struct_with_callable_field_generates_qir() {
    // `Config` is a newtype wrapping (Op: Qubit => Unit, Data: Int).
    // The synthetic Call builder resolves the UDT's pure tuple shape so defunc
    // can discover and specialize the callable field.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Config = (Op: Qubit => Unit, Data: Int);
            operation Apply(cfg: Config) : Result {
                use q = Qubit();
                cfg::Op(q);
                MResetZ(q)
            }
            operation DoH(q: Qubit) : Unit { H(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("Apply", true), ("DoH", true), ("Config", false)],
        caps,
    );

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let config = Value::Tuple(
        vec![do_h, Value::Int(42)].into(),
        Some(Rc::new(fir_id_for(pkg, items["Config"]))),
    );

    let qir = callable_args_to_qir(&store, pkg, items["Apply"], &config, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

// ---- No-callable path: struct with only non-callable fields ----

#[test]
fn struct_with_no_callable_fields_takes_early_return_path() {
    // A UDT that contains no callable fields takes the `concrete_callables.is_empty()`
    // early return.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Pair = (First: Int, Second: Int);
            operation Sum(p: Pair) : Result {
                use q = Qubit();
                let total = p::First + p::Second;
                if total > 0 { H(q); }
                MResetZ(q)
            }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Sum", true), ("Pair", false)], caps);

    let pair = Value::Tuple(
        vec![Value::Int(3), Value::Int(5)].into(),
        Some(Rc::new(fir_id_for(pkg, items["Pair"]))),
    );
    let target_hir = hir_id_for(pkg, items["Sum"]);

    let result = prepare_codegen_fir_from_callable_args(&store, target_hir, &pair, caps);
    assert!(
        result.is_ok(),
        "struct with no callable fields should succeed: {:?}",
        result.err().map(format_interpret_errors)
    );
}

// ---- Synthetic path: single Global arg (not in a tuple) ----

#[test]
fn synthetic_path_single_global_arg_generates_qir() {
    // The simplest synthetic path: a single callable arg, not wrapped in a tuple.
    // `build_synthetic_args` hits the `Ty::Arrow` branch directly.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Invoke(op : Qubit => Unit) : Result {
                use q = Qubit();
                op(q);
                MResetZ(q)
            }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Invoke", true), ("DoX", true)], caps);

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());

    let qir = callable_args_to_qir(&store, pkg, items["Invoke"], &do_x, caps);
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_captureless_closure_adjoint_preserves_functor() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Invoke(op : Qubit => Unit is Adj) : Result {
                use q = Qubit();
                op(q);
                MResetZ(q)
            }
            operation DoS(q : Qubit) : Unit is Adj { S(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Invoke", true), ("DoS", true)], caps);

    let adjoint_do_s = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: Vec::<Value>::new().into(),
        id: fir_id_for(pkg, items["DoS"]),
        functor: FunctorApp {
            adjoint: true,
            controlled: 0,
        },
    }));

    let target_hir = hir_id_for(pkg, items["Invoke"]);
    let codegen_fir =
        prepare_codegen_fir_from_callable_args(&store, target_hir, &adjoint_do_s, caps)
            .unwrap_or_else(|errors| {
                panic!(
                    "adjoint captureless closure should produce CodegenFir, got: {}",
                    format_interpret_errors(errors)
                )
            });
    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        caps,
        &codegen_fir.compute_properties,
        &entry,
    )
    .unwrap_or_else(|e| panic!("synthetic entry QIR generation should succeed: {e:?}"));
    assert!(
        qir.contains("__quantum__qis__s__adj"),
        "expected adjoint S gate in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_udt_wrapped_controlled_callable_preserves_functor() {
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype CtlBox = (Op: ((Qubit[], Qubit) => Unit), Tag: Int);
            operation Invoke(b : CtlBox) : Result {
                use (control, target) = (Qubit(), Qubit());
                b::Op([control], target);
                Reset(control);
                MResetZ(target)
            }
            operation DoX(q : Qubit) : Unit is Ctl { X(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("Invoke", true), ("DoX", true), ("CtlBox", false)],
        caps,
    );

    let controlled_do_x = Value::Global(
        fir_id_for(pkg, items["DoX"]),
        FunctorApp {
            adjoint: false,
            controlled: 1,
        },
    );
    let boxed = Value::Tuple(
        vec![controlled_do_x, Value::Int(0)].into(),
        Some(Rc::new(fir_id_for(pkg, items["CtlBox"]))),
    );

    let target_hir = hir_id_for(pkg, items["Invoke"]);
    let codegen_fir = prepare_codegen_fir_from_callable_args(&store, target_hir, &boxed, caps)
        .unwrap_or_else(|errors| {
            panic!(
                "controlled UDT-wrapped callable should produce CodegenFir, got: {}",
                format_interpret_errors(errors)
            )
        });
    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        caps,
        &codegen_fir.compute_properties,
        &entry,
    )
    .unwrap_or_else(|e| panic!("synthetic entry QIR generation should succeed: {e:?}"));
    assert!(
        qir.contains("__quantum__qis__cx__body"),
        "expected controlled X gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: struct wrapping a callable field ----

#[test]
fn synthetic_path_single_field_struct_wrapping_callable_generates_qir() {
    // Single-field UDT constructors are transparent in Value form: OpBox(DoH)
    // is represented as the bare Global callable value.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype OpBox = (Op: Qubit => Unit);
            operation RunBoxed(b: OpBox) : Result {
                use q = Qubit();
                b::Op(q);
                MResetZ(q)
            }
            operation DoH(q: Qubit) : Unit { H(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunBoxed", true), ("DoH", true)], caps);

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());

    let qir = callable_args_to_qir(&store, pkg, items["RunBoxed"], &do_h, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_struct_wrapping_callable_and_tag_generates_qir() {
    // A newtype that wraps a callable and a non-callable field.
    // This keeps tuple structure in the runtime Value while still exercising
    // UDT pure-type discovery.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype OpBox = (Op: Qubit => Unit, Tag: Int);
            operation RunBoxed(b: OpBox) : Result {
                use q = Qubit();
                b::Op(q);
                MResetZ(q)
            }
            operation DoH(q: Qubit) : Unit { H(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("RunBoxed", true), ("DoH", true), ("OpBox", false)],
        caps,
    );

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let boxed = Value::Tuple(
        vec![do_h, Value::Int(0)].into(),
        Some(Rc::new(fir_id_for(pkg, items["OpBox"]))),
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunBoxed"], &boxed, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_udt_wrapped_adjoint_callable_preserves_functor() {
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype OpBox = (Op: Qubit => Unit is Adj, Tag: Int);
            operation RunBoxed(b: OpBox) : Result {
                use q = Qubit();
                b::Op(q);
                MResetZ(q)
            }
            operation DoS(q: Qubit) : Unit is Adj { S(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("RunBoxed", true), ("DoS", true), ("OpBox", false)],
        caps,
    );

    let adjoint_do_s = Value::Global(
        fir_id_for(pkg, items["DoS"]),
        FunctorApp {
            adjoint: true,
            controlled: 0,
        },
    );
    let boxed = Value::Tuple(
        vec![adjoint_do_s, Value::Int(0)].into(),
        Some(Rc::new(fir_id_for(pkg, items["OpBox"]))),
    );

    let target_hir = hir_id_for(pkg, items["RunBoxed"]);
    let codegen_fir = prepare_codegen_fir_from_callable_args(&store, target_hir, &boxed, caps)
        .unwrap_or_else(|errors| {
            panic!(
                "adjoint UDT-wrapped callable should produce CodegenFir, got: {}",
                format_interpret_errors(errors)
            )
        });
    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        caps,
        &codegen_fir.compute_properties,
        &entry,
    )
    .unwrap_or_else(|e| panic!("synthetic entry QIR generation should succeed: {e:?}"));
    assert!(
        qir.contains("__quantum__qis__s__adj"),
        "expected adjoint S gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: callable arg with additional non-callable tuple values ----

#[test]
fn synthetic_path_callable_with_double_and_string_generates_qir() {
    // Target takes (factor: Double, op: Qubit => Unit, label: String).
    // All three value types exercise different branches in `lower_value_to_expr`.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Tagged(factor : Double, op : Qubit => Unit, label : String) : Result {
                use q = Qubit();
                op(q);
                MResetZ(q)
            }
            operation DoH(q : Qubit) : Unit { H(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Tagged", true), ("DoH", true)], caps);

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let args = Value::Tuple(
        vec![Value::Double(1.5), do_h, Value::String("test".into())].into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["Tagged"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: nested struct (UDT inside UDT) with callable ----

#[test]
fn synthetic_path_nested_struct_with_callable_generates_qir() {
    // Two levels of UDT wrapping: Config(Inner: OpBox, N: Int) where
    // OpBox(Op: Qubit => Unit, Id: Int). This exercises UDT pure-type descent
    // and nested field-chain replacement in defunctionalization.
    // Inner UDTs need 2+ fields to avoid the single-field-UDT unwrap issue
    // where the Value::Tuple shape misaligns with the erased type.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype OpBox = (Op: Qubit => Unit, Id: Int);
            newtype Config = (Inner: OpBox, N: Int);
            operation RunConfig(cfg: Config) : Result {
                use q = Qubit();
                cfg::Inner::Op(q);
                MResetZ(q)
            }
            operation DoX(q: Qubit) : Unit { X(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("RunConfig", true),
            ("DoX", true),
            ("Config", false),
            ("OpBox", false),
        ],
        caps,
    );

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let inner = Value::Tuple(
        vec![do_x, Value::Int(1)].into(),
        Some(Rc::new(fir_id_for(pkg, items["OpBox"]))),
    );
    let config = Value::Tuple(
        vec![inner, Value::Int(5)].into(),
        Some(Rc::new(fir_id_for(pkg, items["Config"]))),
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunConfig"], &config, caps);
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_callable_field_taking_udt_with_callable_generates_qir() {
    // Outer wraps a callable whose input is Inner, and Inner itself wraps a
    // callable. This exercises UDT expansion through arrow input types, not
    // just nested UDT fields that directly contain callable values.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Inner = (NestedOp: Qubit => Unit, Id: Int);
            newtype Outer = (ApplyInner: Inner => Result, Id: Int);

            operation Invoke(outer: Outer) : Result {
                let inner = Inner(DoH, 2);
                outer::ApplyInner(inner)
            }

            operation UseInner(inner: Inner) : Result {
                use q = Qubit();
                inner::NestedOp(q);
                MResetZ(q)
            }

            operation DoH(q: Qubit) : Unit { H(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("Invoke", true),
            ("UseInner", true),
            ("DoH", true),
            ("Inner", false),
            ("Outer", false),
        ],
        caps,
    );

    let use_inner = Value::Global(fir_id_for(pkg, items["UseInner"]), FunctorApp::default());
    let outer = Value::Tuple(
        vec![use_inner, Value::Int(1)].into(),
        Some(Rc::new(fir_id_for(pkg, items["Outer"]))),
    );

    let target_hir = hir_id_for(pkg, items["Invoke"]);
    let codegen_fir = prepare_codegen_fir_from_callable_args(&store, target_hir, &outer, caps)
        .unwrap_or_else(|errors| {
            panic!(
                "callable field taking a UDT with a callable should produce CodegenFir, got: {}",
                format_interpret_errors(errors)
            )
        });
    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        caps,
        &codegen_fir.compute_properties,
        &entry,
    )
    .unwrap_or_else(|e| panic!("synthetic entry QIR generation should succeed: {e:?}"));
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: tuple arg where only one element is callable ----

#[test]
fn synthetic_path_tuple_with_one_callable_among_many_scalars() {
    // (Int, Int, Qubit => Unit, Bool, Int) — callable buried deep in a wide tuple.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Wide(a : Int, b : Int, op : Qubit => Unit, flag : Bool, c : Int) : Result {
                use q = Qubit();
                if flag { op(q); }
                MResetZ(q)
            }
            operation DoH(q : Qubit) : Unit { H(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Wide", true), ("DoH", true)], caps);

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let args = Value::Tuple(
        vec![
            Value::Int(1),
            Value::Int(2),
            do_h,
            Value::Bool(true),
            Value::Int(4),
        ]
        .into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["Wide"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: plain tuple with callable ----

#[test]
fn plain_tuple_with_callable_takes_synthetic_path() {
    // A plain `Value::Tuple(_, None)` (no UDT tag) containing a callable takes
    // the same synthetic path as UDT values.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation RunPair(op : Qubit => Unit, n : Int) : Result {
                use q = Qubit();
                for _ in 0..n - 1 { op(q); }
                MResetZ(q)
            }
            operation DoH(q : Qubit) : Unit { H(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunPair", true), ("DoH", true)], caps);

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    // Plain tuple — no UDT tag.
    let args = Value::Tuple(vec![do_h, Value::Int(2)].into(), None);

    let qir = callable_args_to_qir(&store, pkg, items["RunPair"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: struct with two callable fields ----

#[test]
fn synthetic_path_struct_with_two_callable_fields_generates_qir() {
    // A newtype with two arrow fields. Both are wrapped in the UDT.
    let source = indoc::indoc! {r#"
        namespace Test {
            newtype Ops = (First: Qubit => Unit, Second: Qubit => Unit);
            operation RunOps(ops: Ops) : Result {
                use q = Qubit();
                ops::First(q);
                ops::Second(q);
                MResetZ(q)
            }
            operation DoH(q: Qubit) : Unit { H(q); }
            operation DoX(q: Qubit) : Unit { X(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[
            ("RunOps", true),
            ("DoH", true),
            ("DoX", true),
            ("Ops", false),
        ],
        caps,
    );

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let ops = Value::Tuple(
        vec![do_h, do_x].into(),
        Some(Rc::new(fir_id_for(pkg, items["Ops"]))),
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunOps"], &ops, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate in QIR:\n{qir}"
    );
}

// ---- Synthetic path: callable with Pauli and Result args ----

#[test]
fn synthetic_path_callable_with_pauli_and_result_values() {
    // Exercises the Pauli and Result branches of `lower_value_to_expr`.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Measure(op : Qubit => Unit, basis : Pauli) : Result {
                use q = Qubit();
                op(q);
                MResetZ(q)
            }
            operation DoH(q : Qubit) : Unit { H(q); }
        }
    "#};
    let caps = adaptive_capabilities();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("Measure", true), ("DoH", true)], caps);

    let do_h = Value::Global(fir_id_for(pkg, items["DoH"]), FunctorApp::default());
    let args = Value::Tuple(
        vec![do_h, Value::Pauli(qsc_fir::fir::Pauli::Z)].into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["Measure"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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

            @0 = internal constant [4 x i8] c"0_a\00"
            @1 = internal constant [6 x i8] c"1_a0r\00"
            @2 = internal constant [6 x i8] c"2_a1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__x__body(%Qubit*)

            declare void @__quantum__rt__tuple_record_output(i64, i8*)

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="3" "required_num_results"="2" }
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
    fn noise_intrinsic_generates_correct_qir() {
        let source = "namespace Test {
            operation Main() : Result {
                use q = Qubit();
                test_noise_intrinsic(q);
                MResetZ(q)
            }

            @NoiseIntrinsic()
            operation test_noise_intrinsic(target: Qubit) : Unit {
                body intrinsic;
            }
        }";

        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @test_noise_intrinsic(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @test_noise_intrinsic(%Qubit*) #2

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="base_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }
            attributes #2 = { "qdk_noise" }

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

            !llvm.module.flags = !{!0, !1, !2, !3}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
        "#]]
        .assert_eq(&qir);
    }

    #[test]
    fn noise_intrinsic_generates_correct_qir() {
        let source = "namespace Test {
            operation Main() : Result {
                use q = Qubit();
                test_noise_intrinsic(q);
                MResetZ(q)
            }

            @NoiseIntrinsic()
            operation test_noise_intrinsic(target: Qubit) : Unit {
                body intrinsic;
            }
        }";

        let qir = compile_source_to_qir(source, *CAPABILITIES);
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @test_noise_intrinsic(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @test_noise_intrinsic(%Qubit*) #2

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare void @__quantum__rt__result_record_output(%Result*, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }
            attributes #2 = { "qdk_noise" }

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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mzz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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

            @0 = internal constant [4 x i8] c"0_a\00"
            @1 = internal constant [6 x i8] c"1_a0r\00"
            @2 = internal constant [6 x i8] c"2_a1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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

    use super::{compile_source_to_qir, compile_source_to_qir_from_ast, compile_source_to_rir};
    static CAPABILITIES: std::sync::LazyLock<TargetCapabilityFlags> =
        std::sync::LazyLock::new(|| {
            TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations
        });

    fn terminal_result_return_with_qubit_cleanup_source() -> &'static str {
        indoc::indoc! {r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Result {
                    use q = Qubit();
                    let r = M(q);
                    Reset(q);
                    return r;
                }
            }
        "#}
    }

    fn assert_terminal_result_return_with_qubit_cleanup_qir(qir: &str) {
        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__m__body(%Qubit*, %Result*) #1

            declare void @__quantum__qis__reset__body(%Qubit*) #1

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
        .assert_eq(qir);
    }

    fn assert_terminal_result_return_with_qubit_cleanup_rir(program: &str, form: &str) {
        assert!(
            program.contains("name: __quantum__qis__m__body"),
            "{form} RIR should include the measurement callable"
        );
        assert!(
            program.contains("name: __quantum__qis__reset__body"),
            "{form} RIR should include the cleanup reset callable"
        );
        assert!(
            program.contains("name: __quantum__rt__result_record_output"),
            "{form} RIR should include result output recording"
        );
        assert!(
            program.contains("num_qubits: 1"),
            "{form} RIR should keep a single allocated qubit"
        );
        assert!(
            program.contains("num_results: 1"),
            "{form} RIR should keep a single returned result"
        );

        let measurement_call = program
            .find("args( Qubit(0), Result(0), )")
            .unwrap_or_else(|| panic!("{form} RIR should contain the measurement call"));
        let reset_call = program
            .find("args( Qubit(0), )")
            .unwrap_or_else(|| panic!("{form} RIR should contain the cleanup reset call"));
        let output_call = program
            .find("args( Result(0), Tag(")
            .unwrap_or_else(|| panic!("{form} RIR should record the returned result"));

        assert!(
            measurement_call < reset_call && reset_call < output_call,
            "{form} RIR should measure, reset, and then record the returned result"
        );
    }

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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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

            @0 = internal constant [4 x i8] c"0_a\00"
            @1 = internal constant [6 x i8] c"1_a0r\00"
            @2 = internal constant [6 x i8] c"2_a1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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

    #[test]
    fn terminal_result_return_with_qubit_cleanup_generates_correct_qir() {
        let qir = compile_source_to_qir(
            terminal_result_return_with_qubit_cleanup_source(),
            *CAPABILITIES,
        );
        assert_terminal_result_return_with_qubit_cleanup_qir(&qir);
    }

    #[test]
    fn terminal_result_return_with_qubit_cleanup_generates_correct_qir_from_ast() {
        let qir = compile_source_to_qir_from_ast(
            terminal_result_return_with_qubit_cleanup_source(),
            *CAPABILITIES,
        );
        assert_terminal_result_return_with_qubit_cleanup_qir(&qir);
    }

    #[test]
    fn terminal_result_return_with_qubit_cleanup_generates_rir() {
        let rir = compile_source_to_rir(
            terminal_result_return_with_qubit_cleanup_source(),
            *CAPABILITIES,
        );
        let [raw, ssa] = rir.as_slice() else {
            panic!("expected raw and SSA RIR programs");
        };

        assert_terminal_result_return_with_qubit_cleanup_rir(raw, "raw");
        assert_terminal_result_return_with_qubit_cleanup_rir(ssa, "ssa");
    }
}

mod adaptive_rif_profile {
    use super::compile_source_to_qir;
    use expect_test::expect;
    use qsc_data_structures::target::TargetCapabilityFlags;
    static CAPABILITIES: std::sync::LazyLock<TargetCapabilityFlags> =
        std::sync::LazyLock::new(|| {
            TargetCapabilityFlags::Adaptive
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
            !5 = !{i32 5, !"float_computations", !{!"double"}}
        "#]]
        .assert_eq(&qir);
    }

    #[test]
    fn tuple_comparison_generates_qir_after_pipeline() {
        let qir = compile_source_to_qir(
            indoc::indoc! {r#"
                namespace Test {
                    @EntryPoint()
                    operation Main() : Bool {
                        use (q0, q1) = (Qubit(), Qubit());
                        let lhs = (MResetZ(q0), MResetZ(q1));
                        lhs == (Zero, Zero)
                    }
                }
            "#},
            *CAPABILITIES,
        );

        expect![[r#"
            %Result = type opaque
            %Qubit = type opaque

            @0 = internal constant [4 x i8] c"0_b\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              %var_0 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
              %var_1 = icmp eq i1 %var_0, false
              br i1 %var_1, label %block_1, label %block_2
            block_1:
              %var_3 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
              %var_4 = icmp eq i1 %var_3, false
              br label %block_2
            block_2:
              %var_6 = phi i1 [false, %block_0], [%var_4, %block_1]
              call void @__quantum__rt__bool_record_output(i1 %var_6, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              ret i64 0
            }

            declare void @__quantum__rt__initialize(i8*)

            declare void @__quantum__qis__mresetz__body(%Qubit*, %Result*) #1

            declare i1 @__quantum__rt__read_result(%Result*)

            declare void @__quantum__rt__bool_record_output(i1, i8*)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5}

            !0 = !{i32 1, !"qir_major_version", i32 1}
            !1 = !{i32 7, !"qir_minor_version", i32 0}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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
            !5 = !{i32 5, !"float_computations", !{!"double"}}
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

            @0 = internal constant [4 x i8] c"0_a\00"
            @1 = internal constant [6 x i8] c"1_a0r\00"
            @2 = internal constant [6 x i8] c"2_a1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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
            !5 = !{i32 5, !"float_computations", !{!"double"}}
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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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
            !5 = !{i32 5, !"float_computations", !{!"double"}}
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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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
            !5 = !{i32 5, !"float_computations", !{!"double"}}
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

            @0 = internal constant [4 x i8] c"0_t\00"
            @1 = internal constant [6 x i8] c"1_t0r\00"
            @2 = internal constant [6 x i8] c"2_t1r\00"

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              call void @__quantum__rt__initialize(i8* null)
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 2 to %Qubit*))
              call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 1 to %Qubit*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
              call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
              call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
              call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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
            !5 = !{i32 5, !"float_computations", !{!"double"}}
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
            !5 = !{i32 5, !"float_computations", !{!"double"}}
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
            !5 = !{i32 5, !"float_computations", !{!"double"}}
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
            !5 = !{i32 5, !"float_computations", !{!"double"}}
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
            !5 = !{i32 5, !"float_computations", !{!"double"}}
        "#]].assert_eq(&qir);
    }
}

mod adaptive_rifla_profile {
    use super::compile_source_to_qir;
    use super::compile_source_to_qir_result;
    use expect_test::expect;
    use qsc_data_structures::target::TargetCapabilityFlags;

    static CAPABILITIES: std::sync::LazyLock<TargetCapabilityFlags> =
        std::sync::LazyLock::new(|| {
            TargetCapabilityFlags::Adaptive
                | TargetCapabilityFlags::IntegerComputations
                | TargetCapabilityFlags::FloatingPointComputations
                | TargetCapabilityFlags::BackwardsBranching
                | TargetCapabilityFlags::StaticSizedArrays
        });

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
              %var_1 = alloca i64
              %var_3 = alloca i1
              %var_4 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
              store i64 1, ptr %var_1
              br label %block_1
            block_1:
              %var_11 = load i64, ptr %var_1
              %var_2 = icmp sle i64 %var_11, 2
              store i1 true, ptr %var_3
              br i1 %var_2, label %block_2, label %block_3
            block_2:
              %var_14 = load i1, ptr %var_3
              br i1 %var_14, label %block_4, label %block_5
            block_3:
              store i1 false, ptr %var_3
              br label %block_2
            block_4:
              store i64 0, ptr %var_4
              br label %block_6
            block_5:
              call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
              ret i64 0
            block_6:
              %var_16 = load i64, ptr %var_4
              %var_5 = icmp slt i64 %var_16, 2
              br i1 %var_5, label %block_7, label %block_8
            block_7:
              %var_19 = load i64, ptr %var_4
              %var_6 = getelementptr ptr, ptr @array0, i64 %var_19
              %var_20 = load ptr, ptr %var_6
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr %var_20)
              %var_8 = add i64 %var_19, 1
              store i64 %var_8, ptr %var_4
              br label %block_6
            block_8:
              %var_17 = load i64, ptr %var_1
              %var_9 = add i64 %var_17, 1
              store i64 %var_9, ptr %var_1
              br label %block_1
            }

            declare void @__quantum__rt__initialize(ptr)

            declare void @__quantum__qis__x__body(ptr)

            declare void @__quantum__qis__cx__body(ptr, ptr)

            declare void @__quantum__rt__tuple_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
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
              %var_1 = alloca i64
              %var_3 = alloca i1
              %var_4 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
              store i64 1, ptr %var_1
              br label %block_1
            block_1:
              %var_11 = load i64, ptr %var_1
              %var_2 = icmp sle i64 %var_11, 2
              store i1 true, ptr %var_3
              br i1 %var_2, label %block_2, label %block_3
            block_2:
              %var_14 = load i1, ptr %var_3
              br i1 %var_14, label %block_4, label %block_5
            block_3:
              store i1 false, ptr %var_3
              br label %block_2
            block_4:
              store i64 0, ptr %var_4
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
              %var_16 = load i64, ptr %var_4
              %var_5 = icmp slt i64 %var_16, 2
              br i1 %var_5, label %block_7, label %block_8
            block_7:
              %var_19 = load i64, ptr %var_4
              %var_6 = getelementptr ptr, ptr @array0, i64 %var_19
              %var_20 = load ptr, ptr %var_6
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr %var_20)
              %var_8 = add i64 %var_19, 1
              store i64 %var_8, ptr %var_4
              br label %block_6
            block_8:
              %var_17 = load i64, ptr %var_1
              %var_9 = add i64 %var_17, 1
              store i64 %var_9, ptr %var_1
              br label %block_1
            }

            declare void @__quantum__rt__initialize(ptr)

            declare void @__quantum__qis__x__body(ptr)

            declare void @__quantum__qis__cx__body(ptr, ptr)

            declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

            declare void @__quantum__rt__array_record_output(i64, ptr)

            declare void @__quantum__rt__result_record_output(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
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
              %var_1 = alloca i64
              %var_3 = alloca i1
              %var_4 = alloca i64
              %var_9 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
              store i64 1, ptr %var_1
              br label %block_1
            block_1:
              %var_16 = load i64, ptr %var_1
              %var_2 = icmp sle i64 %var_16, 2
              store i1 true, ptr %var_3
              br i1 %var_2, label %block_2, label %block_3
            block_2:
              %var_19 = load i1, ptr %var_3
              br i1 %var_19, label %block_4, label %block_5
            block_3:
              store i1 false, ptr %var_3
              br label %block_2
            block_4:
              store i64 0, ptr %var_4
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
              %var_21 = load i64, ptr %var_4
              %var_5 = icmp slt i64 %var_21, 2
              br i1 %var_5, label %block_7, label %block_8
            block_7:
              %var_29 = load i64, ptr %var_4
              %var_6 = getelementptr ptr, ptr @array0, i64 %var_29
              %var_30 = load ptr, ptr %var_6
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr %var_30)
              %var_8 = add i64 %var_29, 1
              store i64 %var_8, ptr %var_4
              br label %block_6
            block_8:
              store i64 0, ptr %var_9
              br label %block_9
            block_9:
              %var_23 = load i64, ptr %var_9
              %var_10 = icmp slt i64 %var_23, 3
              br i1 %var_10, label %block_10, label %block_11
            block_10:
              %var_26 = load i64, ptr %var_9
              %var_11 = getelementptr ptr, ptr @array1, i64 %var_26
              %var_27 = load ptr, ptr %var_11
              call void @__quantum__qis__rx__body(double 6.2831853, ptr %var_27)
              %var_13 = add i64 %var_26, 1
              store i64 %var_13, ptr %var_9
              br label %block_9
            block_11:
              %var_24 = load i64, ptr %var_1
              %var_14 = add i64 %var_24, 1
              store i64 %var_14, ptr %var_1
              br label %block_1
            }

            declare void @__quantum__rt__initialize(ptr)

            declare void @__quantum__qis__x__body(ptr)

            declare void @__quantum__qis__cx__body(ptr, ptr)

            declare void @__quantum__qis__rx__body(double, ptr)

            declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

            declare void @__quantum__rt__array_record_output(i64, ptr)

            declare void @__quantum__rt__result_record_output(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
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
              call void @__quantum__rt__initialize(ptr null)
              store i1 false, ptr %var_1
              br label %block_1
            block_1:
              %var_10 = load i1, ptr %var_1
              %var_2 = xor i1 %var_10, true
              br i1 %var_2, label %block_2, label %block_3
            block_2:
              store i64 0, ptr %var_3
              br label %block_4
            block_3:
              call void @__quantum__rt__tuple_record_output(i64 0, ptr @0)
              ret i64 0
            block_4:
              %var_12 = load i64, ptr %var_3
              %var_4 = icmp slt i64 %var_12, 2
              br i1 %var_4, label %block_5, label %block_6
            block_5:
              %var_14 = load i64, ptr %var_3
              %var_5 = getelementptr ptr, ptr @array0, i64 %var_14
              %var_15 = load ptr, ptr %var_5
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr %var_15)
              %var_7 = add i64 %var_14, 1
              store i64 %var_7, ptr %var_3
              br label %block_4
            block_6:
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_8 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              store i1 %var_8, ptr %var_1
              br label %block_1
            }

            declare void @__quantum__rt__initialize(ptr)

            declare void @__quantum__qis__cx__body(ptr, ptr)

            declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

            declare i1 @__quantum__rt__read_result(ptr)

            declare void @__quantum__rt__tuple_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
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

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              %var_2 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
              store i64 0, ptr %var_2
              %var_4 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %var_4, label %block_1, label %block_2
            block_1:
              %var_24 = load i64, ptr %var_2
              %var_6 = add i64 %var_24, 1
              store i64 %var_6, ptr %var_2
              br label %block_2
            block_2:
              %var_7 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              br i1 %var_7, label %block_3, label %block_4
            block_3:
              %var_22 = load i64, ptr %var_2
              %var_9 = add i64 %var_22, 1
              store i64 %var_9, ptr %var_2
              br label %block_4
            block_4:
              %var_10 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
              br i1 %var_10, label %block_5, label %block_6
            block_5:
              %var_20 = load i64, ptr %var_2
              %var_12 = add i64 %var_20, 1
              store i64 %var_12, ptr %var_2
              br label %block_6
            block_6:
              %var_13 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
              br i1 %var_13, label %block_7, label %block_8
            block_7:
              %var_18 = load i64, ptr %var_2
              %var_15 = add i64 %var_18, 1
              store i64 %var_15, ptr %var_2
              br label %block_8
            block_8:
              %var_17 = load i64, ptr %var_2
              call void @__quantum__rt__int_record_output(i64 %var_17, ptr @0)
              ret i64 0
            }

            declare void @__quantum__rt__initialize(ptr)

            declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

            declare i1 @__quantum__rt__read_result(ptr)

            declare void @__quantum__rt__int_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
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

            define i64 @ENTRYPOINT__main() #0 {
            block_0:
              %var_1 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 2 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
              store i64 0, ptr %var_1
              %var_3 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %var_3, label %block_1, label %block_2
            block_1:
              %var_23 = load i64, ptr %var_1
              %var_5 = add i64 %var_23, 1
              store i64 %var_5, ptr %var_1
              br label %block_2
            block_2:
              %var_6 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              br i1 %var_6, label %block_3, label %block_4
            block_3:
              %var_21 = load i64, ptr %var_1
              %var_8 = add i64 %var_21, 1
              store i64 %var_8, ptr %var_1
              br label %block_4
            block_4:
              %var_9 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
              br i1 %var_9, label %block_5, label %block_6
            block_5:
              %var_19 = load i64, ptr %var_1
              %var_11 = add i64 %var_19, 1
              store i64 %var_11, ptr %var_1
              br label %block_6
            block_6:
              %var_12 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
              br i1 %var_12, label %block_7, label %block_8
            block_7:
              %var_17 = load i64, ptr %var_1
              %var_14 = add i64 %var_17, 1
              store i64 %var_14, ptr %var_1
              br label %block_8
            block_8:
              %var_16 = load i64, ptr %var_1
              call void @__quantum__rt__int_record_output(i64 %var_16, ptr @0)
              ret i64 0
            }

            declare void @__quantum__rt__initialize(ptr)

            declare void @__quantum__qis__h__body(ptr)

            declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

            declare i1 @__quantum__rt__read_result(ptr)

            declare void @__quantum__rt__int_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
        "#]]
            .assert_eq(&qir);
    }

    #[test]
    #[ignore = "CapabilitiesCk(UseOfDynamicResult) — mutable Result re-measurement requires UseOfDynamicResult, not in RIFLA profile"]
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
        let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("mutable Result variable should compile");
        assert!(qir.contains("@ENTRYPOINT__main"));
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
              %var_6 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              store i64 0, ptr %var_1
              br label %block_1
            block_1:
              %var_12 = load i64, ptr %var_1
              %var_2 = icmp slt i64 %var_12, 4
              br i1 %var_2, label %block_2, label %block_3
            block_2:
              %var_18 = load i64, ptr %var_1
              %var_3 = getelementptr ptr, ptr @array0, i64 %var_18
              %var_19 = load ptr, ptr %var_3
              call void @__quantum__qis__h__body(ptr %var_19)
              %var_5 = add i64 %var_18, 1
              store i64 %var_5, ptr %var_1
              br label %block_1
            block_3:
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              store i64 0, ptr %var_6
              br label %block_4
            block_4:
              %var_14 = load i64, ptr %var_6
              %var_7 = icmp slt i64 %var_14, 3
              br i1 %var_7, label %block_5, label %block_6
            block_5:
              %var_15 = load i64, ptr %var_6
              %var_8 = getelementptr ptr, ptr @array1, i64 %var_15
              %var_16 = load ptr, ptr %var_8
              call void @__quantum__qis__reset__body(ptr %var_16)
              %var_10 = add i64 %var_15, 1
              store i64 %var_10, ptr %var_6
              br label %block_4
            block_6:
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr @0)
              ret i64 0
            }

            declare void @__quantum__rt__initialize(ptr)

            declare void @__quantum__qis__h__body(ptr)

            declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

            declare void @__quantum__qis__reset__body(ptr) #1

            declare void @__quantum__rt__result_record_output(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
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
              call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
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

            declare void @__quantum__qis__x__body(ptr)

            declare void @__quantum__qis__h__body(ptr)

            declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

            declare void @__quantum__rt__array_record_output(i64, ptr)

            declare void @__quantum__rt__result_record_output(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
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
              call void @__quantum__rt__initialize(ptr null)
              store i64 0, ptr %var_0
              br label %block_1
            block_1:
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %var_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %var_1, label %block_2, label %block_3
            block_2:
              store i64 0, ptr %var_3
              br label %block_4
            block_3:
              %var_8 = load i64, ptr %var_0
              call void @__quantum__rt__int_record_output(i64 %var_8, ptr @0)
              ret i64 0
            block_4:
              %var_10 = load i64, ptr %var_3
              %var_4 = icmp slt i64 %var_10, 3
              br i1 %var_4, label %block_5, label %block_6
            block_5:
              %var_11 = load i64, ptr %var_0
              %var_5 = add i64 %var_11, 1
              store i64 %var_5, ptr %var_0
              %var_13 = load i64, ptr %var_3
              %var_6 = add i64 %var_13, 1
              store i64 %var_6, ptr %var_3
              br label %block_4
            block_6:
              br label %block_1
            }

            declare void @__quantum__rt__initialize(ptr)

            declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

            declare i1 @__quantum__rt__read_result(ptr)

            declare void @__quantum__rt__int_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
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
              %var_3 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              store i64 0, ptr %var_1
              br label %block_1
            block_1:
              %var_8 = load i64, ptr %var_1
              %var_2 = icmp slt i64 %var_8, 3
              br i1 %var_2, label %block_2, label %block_3
            block_2:
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              store i64 0, ptr %var_3
              br label %block_4
            block_3:
              %var_9 = load i64, ptr %var_1
              call void @__quantum__rt__int_record_output(i64 %var_9, ptr @0)
              ret i64 0
            block_4:
              %var_11 = load i64, ptr %var_3
              %var_4 = icmp slt i64 %var_11, 2
              br i1 %var_4, label %block_5, label %block_6
            block_5:
              call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
              %var_14 = load i64, ptr %var_3
              %var_5 = add i64 %var_14, 1
              store i64 %var_5, ptr %var_3
              br label %block_4
            block_6:
              %var_12 = load i64, ptr %var_1
              %var_6 = add i64 %var_12, 1
              store i64 %var_6, ptr %var_1
              br label %block_1
            }

            declare void @__quantum__rt__initialize(ptr)

            declare void @__quantum__qis__h__body(ptr)

            declare void @__quantum__rt__int_record_output(i64, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
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
                        set found = true;
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
              %var_1 = alloca i1
              %var_2 = alloca i64
              call void @__quantum__rt__initialize(ptr null)
              store i1 false, ptr %var_1
              store i64 0, ptr %var_2
              br label %block_1
            block_1:
              %var_11 = load i64, ptr %var_2
              %var_3 = icmp slt i64 %var_11, 3
              br i1 %var_3, label %block_2, label %block_3
            block_2:
              %var_13 = load i64, ptr %var_2
              %var_4 = getelementptr ptr, ptr @array0, i64 %var_13
              %var_14 = load ptr, ptr %var_4
              call void @__quantum__qis__h__body(ptr %var_14)
              call void @__quantum__qis__mresetz__body(ptr %var_14, ptr inttoptr (i64 0 to ptr))
              %var_6 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              br i1 %var_6, label %block_4, label %block_5
            block_3:
              %var_12 = load i1, ptr %var_1
              call void @__quantum__rt__bool_record_output(i1 %var_12, ptr @0)
              ret i64 0
            block_4:
              store i1 true, ptr %var_1
              br label %block_5
            block_5:
              %var_15 = load i64, ptr %var_2
              %var_8 = add i64 %var_15, 1
              store i64 %var_8, ptr %var_2
              br label %block_1
            }

            declare void @__quantum__rt__initialize(ptr)

            declare void @__quantum__qis__h__body(ptr)

            declare void @__quantum__qis__mresetz__body(ptr, ptr) #1

            declare i1 @__quantum__rt__read_result(ptr)

            declare void @__quantum__rt__bool_record_output(i1, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="1" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
        "#]]
            .assert_eq(&qir);
    }
}
