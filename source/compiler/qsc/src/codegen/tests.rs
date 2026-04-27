// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

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

use crate::codegen::qir::{
    get_qir, get_qir_from_ast, get_rir, prepare_backend_fir_from_callable_args,
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
        "expected pass-derived backend readiness errors, got {errors:?}"
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
    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()"),
        "expected entry point in generated QIR, got:\n{qir}"
    );
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
                    let isConstant = DeutschJozsa(fn, 5);
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

    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()"),
        "expected entry point in generated QIR, got:\n{qir}"
    );
    assert!(
        qir.contains("call void @__quantum__rt__bool_record_output"),
        "expected bool output recording in generated QIR, got:\n{qir}"
    );
}

#[test]
fn simple_phase_estimation_sample_shape_generates_qir() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation Main() : Result[] {
                use state = Qubit();
                use phase = Qubit[6];

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

    assert!(
        qir.contains("define i64 @ENTRYPOINT__main()"),
        "expected entry point in generated QIR, got:\n{qir}"
    );
    assert!(
        qir.contains("call void @__quantum__rt__result_record_output"),
        "expected result output recording in generated QIR, got:\n{qir}"
    );
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

    assert!(
        qir.contains("call void @__quantum__rt__int_record_output(i64 %var_"),
        "expected explicit return tuple to preserve a dynamic integer SSA value, got:\n{qir}"
    );
    assert!(
        !qir.contains("call void @__quantum__rt__int_record_output(i64 0,"),
        "expected explicit return tuple to avoid recording a stale literal, got:\n{qir}"
    );
}

#[test]
fn result_array_helper_return_survives_adaptive_backend_prep() {
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

    assert_eq!(
        qir.matches("call void @__quantum__qis__mresetz__body")
            .count(),
        2,
        "expected helper return lowering to preserve both measurement-reset calls, got:\n{qir}"
    );
    assert!(
        qir.contains("call void @__quantum__rt__result_record_output"),
        "expected helper return lowering to preserve result output recording, got:\n{qir}"
    );
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

    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        1,
        "expected the captured integer to drive exactly one X application, got:\n{qir}"
    );
    assert_eq!(
        qir.matches("call void @__quantum__qis__mresetz__body")
            .count(),
        2,
        "expected the specialized HOF call to preserve both reset measurements, got:\n{qir}"
    );
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

    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        1,
        "expected the captured integer to drive exactly one X application, got:\n{qir}"
    );
    assert_eq!(
        qir.matches("call void @__quantum__qis__mresetz__body")
            .count(),
        2,
        "expected the specialized two-callable HOF to preserve both qubit resets, got:\n{qir}"
    );
}

#[test]
fn callable_args_with_arrow_input_survives_dce() {
    let source = indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
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

    // The callable-args path pins ApplyOp (arrow-input target) and seeds
    // MyOp into the entry, then runs the full pipeline including DCE.
    // If pinned items are not threaded through, DCE removes ApplyOp and
    // the pipeline panics.
    let result =
        prepare_backend_fir_from_callable_args(&store, apply_op_hir_id, &my_op_value, capabilities);
    match result {
        Ok(_) => {}
        Err(errors) => panic!(
            "callable-args with arrow-input should survive DCE, got: {}",
            format_interpret_errors(errors)
        ),
    }
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
        prepare_backend_fir_from_callable_args(&store, apply_hir_id, &config_value, capabilities);
    match result {
        Ok(_) => {}
        Err(errors) => panic!(
            "callable-args with UDT-wrapped arrow should survive DCE, got: {}",
            format_interpret_errors(errors)
        ),
    }
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

        assert!(qir.contains("define i64 @ENTRYPOINT__main()"));
        assert!(qir.contains("__quantum__rt__bool_record_output"));
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
    use super::compile_source_to_qir_result;
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
        let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("nested for-loop over qubit slice should compile");
        assert!(qir.contains("@ENTRYPOINT__main"));
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
        let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("ConstantFolding pattern should compile");
        assert!(qir.contains("@ENTRYPOINT__main"));
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
        let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("ThreeQubitRepetitionCode pattern should compile");
        assert!(qir.contains("@ENTRYPOINT__main"));
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
        let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("for-loop over qubit slice inside dynamic while should compile");
        assert!(qir.contains("@ENTRYPOINT__main"));
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
        compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("Result array indexing in loop should compile");
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
        let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("Result[] while-loop dynamic indexing should compile");
        assert!(qir.contains("@ENTRYPOINT__main"));
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
        let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("for-loop over qubits with ResetAll should compile");
        assert!(qir.contains("@ENTRYPOINT__main"));
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
        let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("MeasureEachZ on static qubits should compile");
        assert!(qir.contains("@ENTRYPOINT__main"));
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
        let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("static while inside emit-while should compile");
        assert!(qir.contains("@ENTRYPOINT__main"));
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
        let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("nested emit-while loops should compile");
        assert!(qir.contains("@ENTRYPOINT__main"));
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
        let qir = compile_source_to_qir_result(source, *CAPABILITIES)
            .expect("for-loop over qubits with dynamic exit should compile");
        assert!(qir.contains("@ENTRYPOINT__main"));
    }
}
