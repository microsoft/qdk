// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Showcase and multi-package integration tests for the FIR transform
//! pipeline.
//!
//! These tests exercise the whole-compilation scoping of the structural
//! passes: a separately-compiled *library* package defines generic,
//! higher-order, multi-return, and tuple constructs that the *user* (entry)
//! package reaches. After the full pipeline runs, the foreign library
//! callables must be transformed **in place** in their owning package.
//!
//! Because the entry-only renderers (e.g.
//! [`crate::test_utils::format_reachable_callable_summary`]) filter to the
//! entry package, foreign-package transforms are surfaced here by rendering
//! the owning library package directly via
//! [`crate::pretty::write_package_qsharp`].
//!
//! The two `showcase_renders_final_fir_*` tests additionally render the final
//! transformed multi-package FIR as Q# and as QIR. The QIR program is chosen
//! to be partial-evaluation-legal (a scalar early-return inside a
//! measurement-dependent branch) so that codegen succeeds end-to-end.

use crate::PipelineStage;
use crate::pretty::write_package_qsharp;
use crate::test_utils::compile_and_run_pipeline_to_with_library;
use expect_test::{Expect, expect};
use qsc_fir::fir::{ItemKind, PackageId, PackageStore};

/// Finds the FIR package id of the separately-compiled `TestLib` library
/// package (the package other than core/std/user that defines the `TestLib`
/// namespace).
fn library_package_id(store: &PackageStore, user_pkg: PackageId) -> PackageId {
    for (id, package) in store {
        if id == user_pkg {
            continue;
        }
        let has_testlib = package.items.values().any(|item| {
            matches!(&item.kind, ItemKind::Namespace(name, _) if name.name.as_ref() == "TestLib")
        });
        if has_testlib {
            return id;
        }
    }
    panic!("could not locate the TestLib library package in the store");
}

/// Runs the full pipeline on a `lib`/`user` fixture and returns the rendered
/// Q# of the (transformed-in-place) library package.
fn final_library_qsharp(lib_source: &str, user_source: &str) -> String {
    let (store, user_pkg) =
        compile_and_run_pipeline_to_with_library(lib_source, user_source, PipelineStage::Full);
    let lib_pkg = library_package_id(&store, user_pkg);
    write_package_qsharp(&store, lib_pkg)
}

/// Runs the full pipeline on a `lib`/`user` fixture and returns the rendered
/// Q# of the user (entry) package.
fn final_user_qsharp(lib_source: &str, user_source: &str) -> String {
    let (store, user_pkg) =
        compile_and_run_pipeline_to_with_library(lib_source, user_source, PipelineStage::Full);
    write_package_qsharp(&store, user_pkg)
}

#[track_caller]
fn check(actual: &str, expect: &Expect) {
    expect.assert_eq(actual);
}

// ============================================================================
// Step 8.1: Multi-package cross-package transform coverage
// ============================================================================
// Each test builds a core+std+lib+user fixture where the library defines a
// construct the user reaches, then asserts (via an expect snapshot of the
// rendered owning package) that the foreign callable was transformed.

/// A generic library function reached by the user is monomorphized into a
/// concrete specialization in its owning (library) package.
#[test]
fn multipackage_library_generic_specialized_in_place() {
    let lib_source = r#"
        namespace TestLib {
            function Pick<'T>(cond : Bool, a : 'T, b : 'T) : 'T {
                if cond { a } else { b }
            }
            export Pick;
        }
    "#;
    let user_source = r#"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Int {
            Pick(true, 10, 20)
        }
    "#;

    check(
        &final_library_qsharp(lib_source, user_source),
        &expect![[r#"
        // namespace TestLib
        function Pick<Int>(cond : Bool, a : Int, b : Int) : Int {
            body {
                if cond {
                    a
                } else {
                    b
                }

            }
        }
    "#]],
    );
}

/// A multi-return library operation reached by the user has its early returns
/// unified in place in its owning (library) package.
#[test]
fn multipackage_library_multi_return_unified_in_place() {
    let lib_source = r#"
        namespace TestLib {
            function Classify(x : Int) : Int {
                if x > 0 {
                    return 1;
                }
                return 0;
            }
            export Classify;
        }
    "#;
    let user_source = r#"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Int {
            Classify(5)
        }
    "#;

    check(
        &final_library_qsharp(lib_source, user_source),
        &expect![[r#"
        // namespace TestLib
        function Classify(x : Int) : Int {
            body {
                mutable __has_returned : Bool = false;
                mutable __ret_val : Int = 0;
                if x > 0 {
                    {
                        __ret_val = 1;
                        __has_returned = true;
                    };
                }

                if not __has_returned {
                    {
                        __ret_val = 0;
                        __has_returned = true;
                    };
                };
                __ret_val
            }
        }
    "#]],
    );
}

/// A library function that binds a tuple pattern, reached by the user, has the
/// tuple bind decomposed in place in its owning (library) package.
#[test]
fn multipackage_library_tuple_bind_decomposed_in_place() {
    let lib_source = r#"
        namespace TestLib {
            function SumPair(p : (Int, Int)) : Int {
                let (a, b) = p;
                a + b
            }
            export SumPair;
        }
    "#;
    let user_source = r#"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Int {
            SumPair((3, 4))
        }
    "#;

    check(
        &final_library_qsharp(lib_source, user_source),
        &expect![[r#"
        // namespace TestLib
        function SumPair(p_0 : Int, p_1 : Int) : Int {
            body {
                let a : Int = p_0;
                let b : Int = p_1;
                a + b
            }
        }
    "#]],
    );
}

/// A higher-order library operation reached by the user is defunctionalized:
/// the arrow-typed parameter is eliminated by a concrete specialization. The
/// user (entry) package render surfaces the resulting arrow-free call.
#[test]
fn multipackage_library_hof_specialized() {
    let lib_source = r#"
        namespace TestLib {
            operation ApplyTwice(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
                op(q);
            }
            export ApplyTwice;
        }
    "#;
    let user_source = r#"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            ApplyTwice(H, q);
            Reset(q);
        }
    "#;

    check(
        &final_user_qsharp(lib_source, user_source),
        &expect![[r#"
        // namespace test
        operation Main() : Unit {
            body {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyTwice < AdjCtl > { H }
                (q);
                Reset(q);
                __quantum__rt__qubit_release(q);
            }
        }
        operation ApplyTwice<AdjCtl> { H }
        (q : Qubit) : Unit {
            body {
                H(q);
                H(q);
            }
        }
        // entry
        Main()
    "#]],
    );
}

// ============================================================================
// Step 8.2: Showcase — final multi-package FIR rendered as Q# and QIR
// ============================================================================

/// The showcase fixture: a library operation with a scalar early-return inside
/// a measurement-dependent (dynamic) branch — a partial-evaluation-legal shape
/// — reached from the user entry point.
const SHOWCASE_LIB: &str = r#"
    namespace TestLib {
        operation MeasureAndScore(q : Qubit) : Int {
            if MResetZ(q) == One {
                return 1;
            }
            return 0;
        }
        export MeasureAndScore;
    }
"#;

const SHOWCASE_USER: &str = r#"
    import TestLib.*;
    @EntryPoint()
    operation Main() : Int {
        use q = Qubit();
        H(q);
        MeasureAndScore(q)
    }
"#;

/// Generates QIR from a fully-transformed multi-package FIR store, following
/// the canonical RCA-analyze → `ProgramEntry` → `fir_to_qir` pattern.
fn generate_qir_with_library(lib_source: &str, user_source: &str) -> String {
    use qsc_codegen::qir::fir_to_qir;
    use qsc_data_structures::target::TargetCapabilityFlags;
    use qsc_partial_eval::ProgramEntry;

    let capabilities = TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations;
    let (store, pkg_id) =
        compile_and_run_pipeline_to_with_library(lib_source, user_source, PipelineStage::Full);
    let package = store.get(pkg_id);
    let entry = ProgramEntry {
        exec_graph: package.entry_exec_graph.clone(),
        expr: (
            pkg_id,
            package
                .entry
                .expect("package must have an entry expression"),
        )
            .into(),
    };
    let compute_properties = qsc_rca::Analyzer::init(&store, capabilities).analyze_all();
    fir_to_qir(&store, capabilities, &compute_properties, &entry).expect("QIR generation failed")
}

/// The final transformed multi-package FIR renders as valid QIR. The program
/// reaches library code, so the multi-package pipeline is exercised end to
/// end before codegen.
#[test]
fn showcase_renders_final_fir_as_qir() {
    let qir = generate_qir_with_library(SHOWCASE_LIB, SHOWCASE_USER);
    check(
        &qir,
        &expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_i\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          %var_2 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          br i1 %var_2, label %block_1, label %block_2
        block_1:
          br label %block_2
        block_2:
          %var_7 = phi i64 [0, %block_0], [1, %block_1]
          %var_6 = phi i1 [false, %block_0], [true, %block_1]
          %var_4 = xor i1 %var_6, true
          br i1 %var_4, label %block_3, label %block_4
        block_3:
          br label %block_4
        block_4:
          %var_9 = phi i64 [%var_7, %block_2], [0, %block_3]
          %var_8 = phi i1 [%var_6, %block_2], [true, %block_3]
          call void @__quantum__rt__int_record_output(i64 %var_9, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
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
    "#]],
    );
}

/// The final transformed multi-package FIR renders back to Q# (entry package).
#[test]
fn showcase_renders_final_fir_as_qsharp() {
    let qsharp = final_user_qsharp(SHOWCASE_LIB, SHOWCASE_USER);
    check(
        &qsharp,
        &expect![[r#"
        // namespace test
        operation Main() : Int {
            body {
                let q : Qubit = __quantum__rt__qubit_allocate();
                H(q);
                let
                @generated_ident_26 : Int = MeasureAndScore(q);
                __quantum__rt__qubit_release(q);
                @generated_ident_26
            }
        }
        // entry
        Main()
    "#]],
    );
}

// ============================================================================
// Step 3.1: Second legal showcase shape — top-level Result[] update via library
// ============================================================================

/// A second showcase fixture, distinct from the scalar-early-return shape: a
/// library operation that measures a qubit and returns its `Result`, reached
/// from the user entry point.
///
/// The user builds and updates a top-level `Result[]` (`mutable arr = [...]`
/// then `set arr w/= ...`). Crucially, every aggregate (`Result[]`) write
/// happens at the top level — never inside a measurement-dependent (dynamic)
/// branch — so it stays partial-evaluation-legal under
/// `Adaptive | IntegerComputations` (only `Bool`/`Int`/`Double` get backing RIR
/// registers, so an in-branch `Result[]` reassign would be rejected). This
/// exercises the whole-closure multi-package pipeline before codegen while
/// covering a different control/data shape than `SHOWCASE_LIB`.
const SHOWCASE_RESULT_LIB: &str = r#"
    namespace TestLib {
        operation MeasureZ(q : Qubit) : Result {
            return MResetZ(q);
        }
        export MeasureZ;
    }
"#;

const SHOWCASE_RESULT_USER: &str = r#"
    import TestLib.*;
    @EntryPoint()
    operation Main() : Result[] {
        use (q0, q1) = (Qubit(), Qubit());
        H(q0);
        H(q1);
        mutable arr = [MeasureZ(q0), Zero];
        set arr w/= 1 <- MeasureZ(q1);
        arr
    }
"#;

/// The second multi-package shape (top-level `Result[]` update reaching a
/// library callable) also renders as valid QIR, broadening codegen coverage of
/// whole-closure-transformed multi-package FIR.
#[test]
fn showcase_renders_second_shape_as_qir() {
    let qir = generate_qir_with_library(SHOWCASE_RESULT_LIB, SHOWCASE_RESULT_USER);
    check(
        &qir,
        &expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_a\00"
        @1 = internal constant [6 x i8] c"1_a0r\00"
        @2 = internal constant [6 x i8] c"2_a1r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__h__body(%Qubit* inttoptr (i64 1 to %Qubit*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__array_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 1 to %Result*), i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__h__body(%Qubit*)

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
    "#]],
    );
}
