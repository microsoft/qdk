// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

mod adaptive_profile;
mod adaptive_ri_profile;
mod adaptive_rif_profile;
mod base_profile;

use std::sync::Arc;

use std::rc::Rc;

use expect_test::expect;
use miette::Report;
use qsc_data_structures::{
    functors::FunctorApp,
    language_features::LanguageFeatures,
    source::SourceMap,
    target::{Profile, TargetCapabilityFlags},
};
use qsc_eval::val::Value;
use qsc_frontend::compile::parse_all;
use qsc_hir::hir::{ItemKind, PackageId};
use rustc_hash::FxHashMap;

use crate::codegen::qir::{
    CallableArgsBackend, get_qir, get_qir_from_ast, get_rir, prepare_codegen_fir_from_callable_args,
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

/// Compiles `lib_source` as a separate library package, then generates QIR for
/// `user_source` with that library as a dependency. The library's namespaces are
/// visible to the user program without an alias, so user code can reference them
/// directly (for example `import Lib.*;`). This exercises emission of foreign
/// (non-entry-package) callables as standalone IR functions.
fn compile_source_to_qir_with_library(
    lib_source: &str,
    user_source: &str,
    capabilities: TargetCapabilityFlags,
) -> String {
    match compile_source_to_qir_with_library_result(lib_source, user_source, capabilities) {
        Ok(qir) => qir,
        Err(errors) => panic!(
            "Failed to generate QIR for capabilities {capabilities:?}:\n{}",
            format_interpret_errors(errors)
        ),
    }
}

fn compile_source_to_qir_with_library_result(
    lib_source: &str,
    user_source: &str,
    capabilities: TargetCapabilityFlags,
) -> Result<String, Vec<crate::interpret::Error>> {
    let language_features = LanguageFeatures::default();
    let (std_id, mut store) = crate::compile::package_store_with_stdlib(capabilities);

    // Compile the library as its own package depending on core + std.
    let lib_sources = SourceMap::new([("lib.qs".into(), lib_source.into())], None);
    let lib_dependencies: Vec<(PackageId, Option<Arc<str>>)> = vec![(std_id, None)];
    let (lib_unit, lib_errors) = crate::compile::compile(
        &store,
        &lib_dependencies,
        lib_sources,
        qsc_passes::PackageType::Lib,
        capabilities,
        language_features,
    );
    assert!(
        lib_errors.is_empty(),
        "library compilation failed: {lib_errors:?}"
    );
    let lib_id = store.insert(lib_unit);

    // Generate QIR for the user program, which depends on core + std + library.
    let user_sources = source_map_from_source(user_source);
    let user_dependencies: Vec<(PackageId, Option<Arc<str>>)> =
        vec![(std_id, None), (lib_id, None)];
    get_qir(
        user_sources,
        language_features,
        capabilities,
        store,
        &user_dependencies,
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
fn excessive_specializations_warning_does_not_block_qir_generation() {
    let source = r#"
        namespace Test {
            operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }

            @EntryPoint()
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
        }
    "#;

    let qir = compile_source_to_qir(
        source,
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations,
    );

    assert!(
        qir.contains("__quantum__qis__rx__body"),
        "expected QIR generation to continue through warning-only FIR transforms"
    );
}

#[test]
fn defunctionalize_operand_block_set_reaches_qir() {
    // Both programs reassign `f` to `Bar` before `f(5)` is evaluated, so the
    // reaching definition is `Bar`: `Bar(5) = 105`, plus `z = 1` gives the
    // angle `106.0`. They differ only in where `set f = Bar` sits — in an
    // operand-position block versus at the top level — and both must specialize
    // `f(5)` to `Bar` and emit `rz__body(double 106.0`.
    let operand_block = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let z = { set f = Bar; 0 } + 1;
                Rz(IntAsDouble(f(5) + z), q);
            }
        }
    "#;
    let top_level = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                set f = Bar;
                let z = 1;
                Rz(IntAsDouble(f(5) + z), q);
            }
        }
    "#;

    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let operand_block_qir = compile_source_to_qir(operand_block, capabilities);
    let top_level_qir = compile_source_to_qir(top_level, capabilities);

    assert!(
        top_level_qir.contains("__quantum__qis__rz__body(double 106.0"),
        "top-level set should specialize f(5) to Bar -> 106.0; got:\n{top_level_qir}"
    );
    assert!(
        operand_block_qir.contains("__quantum__qis__rz__body(double 106.0"),
        "operand-position set should specialize f(5) to Bar -> 106.0; got:\n{operand_block_qir}"
    );
    assert!(
        !operand_block_qir.contains("__quantum__qis__rz__body(double 6.0"),
        "operand-position set must not specialize f(5) to Foo -> 6.0; got:\n{operand_block_qir}"
    );
}

/// A `set f = Bar` inside the short-circuited RHS of `false and { .. }` is NOT
/// executed at runtime, so `f(5)` must keep the reaching definition `Foo`
/// (`Foo(5) + 1 = 6.0`).
#[test]
fn defunctionalize_binop_andl_short_circuit_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let b = false and { set f = Bar; true };
                Rz(IntAsDouble(f(5) + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 6.0"),
        "short-circuited `and` RHS must NOT reach f(5); expected Foo -> 6.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 106.0"),
        "short-circuited `and` RHS must not specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
}

/// A `set f = Bar` inside the short-circuited RHS of `true or { .. }` is NOT
/// executed at runtime, so `f(5)` must keep `Foo` -> 6.0 (same `BinOp` arm).
#[test]
fn defunctionalize_binop_orl_short_circuit_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let b = true or { set f = Bar; false };
                Rz(IntAsDouble(f(5) + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 6.0"),
        "short-circuited `or` RHS must NOT reach f(5); expected Foo -> 6.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 106.0"),
        "short-circuited `or` RHS must not specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
}

/// A `set f = Bar` inside the short-circuited RHS of a logical compound-assign
/// `set b and= { .. }` is NOT executed when the LHS short-circuits, so `f(5)`
/// must keep `Foo` -> 6.0 (distinct `AssignOp` arm, same fork/join fix).
#[test]
fn defunctionalize_assignop_andl_short_circuit_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                mutable b = false;
                set b and= { set f = Bar; false };
                Rz(IntAsDouble(f(5) + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 6.0"),
        "short-circuited `and=` RHS must NOT reach f(5); expected Foo -> 6.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 106.0"),
        "short-circuited `and=` RHS must not specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
}

/// A `set f = Bar` inside the short-circuited RHS of a logical compound-assign
/// `set b or= { .. }` is NOT executed when the LHS short-circuits, so `f(5)`
/// must keep `Foo` -> 6.0.
#[test]
fn defunctionalize_assignop_orl_short_circuit_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                mutable b = true;
                set b or= { set f = Bar; false };
                Rz(IntAsDouble(f(5) + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 6.0"),
        "short-circuited `or=` RHS must NOT reach f(5); expected Foo -> 6.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 106.0"),
        "short-circuited `or=` RHS must not specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
}

/// In `(new Rec { A = f(5), B = 0 }) w/ B <- { set f = Bar; 0 }`, runtime
/// evaluates the replace operand (`set f = Bar`) BEFORE the record operand
/// (containing `f(5)`), so `f(5)` specializes to `Bar` -> 106.0.
#[test]
fn defunctionalize_update_field_replace_first_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            struct Rec { A : Int, B : Int }
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let r = (new Rec { A = f(5), B = 0 }) w/ B <- { set f = Bar; 0 };
                Rz(IntAsDouble(r.A + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 106.0"),
        "UpdateField replace-then-record must specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 6.0"),
        "UpdateField must not specialize f(5) to Foo -> 6.0; got:\n{qir}"
    );
}

/// In `[f(5), 0] w/ 1 <- { set f = Bar; 0 }`, runtime evaluates the index then
/// the replace operand (`set f = Bar`) BEFORE the container operand (containing
/// `f(5)`), so `f(5)` specializes to `Bar` -> 106.0.
#[test]
fn defunctionalize_update_index_replace_first_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let arr = [f(5), 0] w/ 1 <- { set f = Bar; 0 };
                Rz(IntAsDouble(arr[0] + 1), q);
            }
        }
    "#;
    let capabilities =
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    assert!(
        qir.contains("__quantum__qis__rz__body(double 106.0"),
        "UpdateIndex index-replace-container must specialize f(5) to Bar -> 106.0; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 6.0"),
        "UpdateIndex must not specialize f(5) to Foo -> 6.0; got:\n{qir}"
    );
}

/// `f` is reassigned to `Bar` inside the right-hand side of a short-circuiting
/// `and` whose condition is a measurement outcome, so whether the reassignment
/// happens is only known at runtime:
///
/// ```qsharp
/// mutable f = Foo;
/// let cond = MResetZ(q) == Zero;
/// let b = cond and { set f = Bar; true };  // RHS runs only when cond is true
/// Rz(IntAsDouble(f(5) + 1), q);
/// ```
///
/// Because `and` only evaluates its right-hand side when the left side is true,
/// the later `f(5)` must call:
///   * `Bar` when `cond` is true  -> `Bar(5) + 1 = 106`,
///   * `Foo` when `cond` is false -> `Foo(5) + 1 = 6`.
///
/// The two outcomes cannot be folded to a single constant, so the generated QIR
/// keeps both branches and merges them with a `phi i64 [105, ...], [6, ...]`
/// (the `+ 1` is added after the merge), then passes that runtime value to `Rz`
/// as `double %...`. This verifies the call is dispatched against the correct
/// per-branch definition of `f` and that the measurement-dependent condition
/// still produces valid QIR rather than failing codegen.
#[test]
fn defunctionalize_binop_andl_runtime_dynamic_branch_split_reaches_qir() {
    let source = r#"
        namespace Test {
            import Std.Convert.IntAsDouble;
            function Foo(x : Int) : Int { x + 1 }
            function Bar(x : Int) : Int { x + 100 }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable f = Foo;
                let cond = MResetZ(q) == Zero;
                let b = cond and { set f = Bar; true };
                Rz(IntAsDouble(f(5) + 1), q);
            }
        }
    "#;
    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    // cond-true arm runs the RHS -> f = Bar -> Bar(5) = 105.
    assert!(
        qir.contains("phi i64 [105,"),
        "cond-true arm must resolve f(5) to Bar (Bar(5) = 105); got:\n{qir}"
    );
    // cond-false arm short-circuits past the RHS -> f stays Foo -> Foo(5) = 6.
    assert!(
        qir.contains(", [6,"),
        "cond-false arm must resolve f(5) to Foo (Foo(5) = 6); got:\n{qir}"
    );
    // The Rz angle is the runtime branch-split result, never a folded constant.
    assert!(
        qir.contains("__quantum__qis__rz__body(double %"),
        "Rz angle must be the runtime phi value, not a folded constant; got:\n{qir}"
    );
    assert!(
        !qir.contains("__quantum__qis__rz__body(double 6.0")
            && !qir.contains("__quantum__qis__rz__body(double 106.0"),
        "runtime-dynamic dispatch must not constant-fold the Rz angle; got:\n{qir}"
    );
}

/// A nested conditional that selects between callables must reference each
/// source branch condition exactly once. The outer condition here is a
/// side-effecting measurement:
///
/// ```qsharp
/// let u = if MResetZ(ctl) == One { if flag { Foo } else { Bar } } else { Baz };
/// u(target);
/// ```
///
/// The previous flat left-associated `AndL` dispatch fold referenced the outer
/// condition twice (once in the `outer and inner` guard for `Foo` and once in
/// the standalone `outer` guard for `Bar`), so the measurement was emitted
/// twice. The recursive nested-`if` tree references it exactly once, so the
/// program must contain exactly one `mresetz` measurement instruction.
#[test]
fn defunctionalize_nested_condition_dispatch_evaluates_measurement_once() {
    let source = r#"
        namespace Test {
            operation Foo(q : Qubit) : Unit { Rx(1.0, q); }
            operation Bar(q : Qubit) : Unit { Rx(2.0, q); }
            operation Baz(q : Qubit) : Unit { Rx(3.0, q); }
            @EntryPoint()
            operation Main() : Unit {
                use ctl = Qubit();
                use target = Qubit();
                let flag = true;
                let u = if MResetZ(ctl) == One { if flag { Foo } else { Bar } } else { Baz };
                u(target);
            }
        }
    "#;
    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;
    let qir = compile_source_to_qir(source, capabilities);
    let measurement_count = qir
        .matches("call void @__quantum__qis__mresetz__body")
        .count();
    assert_eq!(
        measurement_count, 1,
        "side-effecting outer condition must be measured exactly once; got:\n{qir}"
    );
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

// Regression test: a callable-typed local used ONLY inside a
// live struct field must survive the defunctionalize pass. Before the
// walk_utils `Struct` recursion fix, defunctionalize's dead-callable-local
// prune skipped recursing into `Struct` field initializers, so `f` (whose
// only use is `new Holder { Cb = f }`) appeared dead and was removed, leaving
// a dangling `Var(Res::Local)` that crashed the downstream codegen pipeline.
// This exercises the full pipeline (FIR transforms -> partial eval -> QIR) and
// observes that QIR generation succeeds and produces the expected classical
// result (`h.Cb(3)` == 4).
#[test]
fn callable_local_in_struct_field_generates_qir() {
    let source = r#"
        namespace Test {
            struct Holder { Cb : (Int => Int) }
            operation Pick(arr : (Int => Int)[]) : Holder {
                let f = arr[0];
                new Holder { Cb = f }
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let ops : (Int => Int)[] = [AddOne];
                let h = Pick(ops);
                if h.Cb(3) == 4 {
                    X(q);
                }
                MResetZ(q)
            }
            operation AddOne(x : Int) : Int { x + 1 }
        }
    "#;

    let qir = compile_source_to_qir(source, TargetCapabilityFlags::empty());
    expect![[r#"
        %Result = type opaque
        %Qubit = type opaque

        @0 = internal constant [4 x i8] c"0_r\00"

        define i64 @ENTRYPOINT__main() #0 {
        block_0:
          call void @__quantum__rt__initialize(i8* null)
          call void @__quantum__qis__x__body(%Qubit* inttoptr (i64 0 to %Qubit*))
          call void @__quantum__qis__m__body(%Qubit* inttoptr (i64 0 to %Qubit*), %Result* inttoptr (i64 0 to %Result*))
          call void @__quantum__rt__result_record_output(%Result* inttoptr (i64 0 to %Result*), i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          ret i64 0
        }

        declare void @__quantum__rt__initialize(i8*)

        declare void @__quantum__qis__x__body(%Qubit*)

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
          %var_9 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          br i1 %var_9, label %block_1, label %block_2
        block_1:
          br label %block_2
        block_2:
          %var_259 = phi i1 [true, %block_0], [false, %block_1]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          %var_11 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
          br i1 %var_11, label %block_3, label %block_4
        block_3:
          br label %block_4
        block_4:
          %var_260 = phi i1 [%var_259, %block_2], [false, %block_3]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 2 to %Result*))
          %var_13 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 2 to %Result*))
          br i1 %var_13, label %block_5, label %block_6
        block_5:
          br label %block_6
        block_6:
          %var_261 = phi i1 [%var_260, %block_4], [false, %block_5]
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
          %var_25 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 3 to %Result*))
          br i1 %var_25, label %block_7, label %block_8
        block_7:
          br label %block_8
        block_8:
          %var_262 = phi i1 [true, %block_6], [false, %block_7]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 4 to %Result*))
          %var_27 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 4 to %Result*))
          br i1 %var_27, label %block_9, label %block_10
        block_9:
          br label %block_10
        block_10:
          %var_263 = phi i1 [%var_262, %block_8], [false, %block_9]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 5 to %Result*))
          %var_29 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 5 to %Result*))
          br i1 %var_29, label %block_11, label %block_12
        block_11:
          br label %block_12
        block_12:
          %var_264 = phi i1 [%var_263, %block_10], [false, %block_11]
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
          %var_170 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 6 to %Result*))
          br i1 %var_170, label %block_13, label %block_14
        block_13:
          br label %block_14
        block_14:
          %var_265 = phi i1 [true, %block_12], [false, %block_13]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 7 to %Result*))
          %var_172 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 7 to %Result*))
          br i1 %var_172, label %block_15, label %block_16
        block_15:
          br label %block_16
        block_16:
          %var_266 = phi i1 [%var_265, %block_14], [false, %block_15]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 8 to %Result*))
          %var_174 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 8 to %Result*))
          br i1 %var_174, label %block_17, label %block_18
        block_17:
          br label %block_18
        block_18:
          %var_267 = phi i1 [%var_266, %block_16], [false, %block_17]
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
          %var_251 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 9 to %Result*))
          br i1 %var_251, label %block_19, label %block_20
        block_19:
          br label %block_20
        block_20:
          %var_268 = phi i1 [true, %block_18], [false, %block_19]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 10 to %Result*))
          %var_253 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 10 to %Result*))
          br i1 %var_253, label %block_21, label %block_22
        block_21:
          br label %block_22
        block_22:
          %var_269 = phi i1 [%var_268, %block_20], [false, %block_21]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 2 to %Qubit*), %Result* inttoptr (i64 11 to %Result*))
          %var_255 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 11 to %Result*))
          br i1 %var_255, label %block_23, label %block_24
        block_23:
          br label %block_24
        block_24:
          %var_270 = phi i1 [%var_269, %block_22], [false, %block_23]
          call void @__quantum__qis__reset__body(%Qubit* inttoptr (i64 3 to %Qubit*))
          call void @__quantum__rt__array_record_output(i64 4, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 %var_261, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 %var_264, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 %var_267, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @3, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 %var_270, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @4, i64 0, i64 0))
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
                    a = 1;
                } else {
                    a = 2;
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
          %var_2 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 0 to %Result*))
          %var_3 = icmp eq i1 %var_2, false
          br i1 %var_3, label %block_1, label %block_2
        block_1:
          br label %block_3
        block_2:
          br label %block_3
        block_3:
          %var_9 = phi i64 [1, %block_1], [2, %block_2]
          call void @__quantum__qis__mresetz__body(%Qubit* inttoptr (i64 1 to %Qubit*), %Result* inttoptr (i64 1 to %Result*))
          %var_5 = call i1 @__quantum__rt__read_result(%Result* inttoptr (i64 1 to %Result*))
          call void @__quantum__rt__tuple_record_output(i64 2, i8* getelementptr inbounds ([4 x i8], [4 x i8]* @0, i64 0, i64 0))
          call void @__quantum__rt__int_record_output(i64 %var_9, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @1, i64 0, i64 0))
          call void @__quantum__rt__bool_record_output(i1 %var_5, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @2, i64 0, i64 0))
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
    // it to ApplyOp{MyOp}, and the pipeline transforms it fully into a
    // self-contained entry that is consumed directly.
    let (codegen_fir, _backend) =
        prepare_codegen_fir_from_callable_args(&store, apply_op_hir_id, &my_op_value, capabilities)
            .unwrap_or_else(|errors| {
                panic!(
                    "callable-args with arrow-input should survive DCE, got: {}",
                    format_interpret_errors(errors)
                )
            });

    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        capabilities,
        &codegen_fir.compute_properties,
        &entry,
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

    let (codegen_fir, _backend) =
        prepare_codegen_fir_from_callable_args(&store, apply_hir_id, &config_value, capabilities)
            .unwrap_or_else(|errors| {
                panic!(
                    "callable-args with UDT-wrapped arrow should produce CodegenFir, got: {}",
                    format_interpret_errors(errors)
                )
            });

    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        capabilities,
        &codegen_fir.compute_properties,
        &entry,
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

    let (codegen_fir, _backend) = prepare_codegen_fir_from_callable_args(
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

    let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
    let qir = qsc_codegen::qir::fir_to_qir(
        &codegen_fir.fir_store,
        capabilities,
        &codegen_fir.compute_properties,
        &entry,
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

/// Matches a callable's actual name against a requested lookup name. The `<lambda>` sentinel
/// matches any lifted lambda, whose real name carries a volatile `<lambda>_<item-id>` suffix.
fn callable_name_matches(actual: &str, requested: &str) -> bool {
    if requested == "<lambda>" {
        actual.starts_with("<lambda>")
    } else {
        actual == requested
    }
}

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
                    if is_callable && callable_name_matches(decl.name.name.as_ref(), name) {
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
    let (codegen_fir, backend) =
        prepare_codegen_fir_from_callable_args(store, target_hir, args, capabilities)
            .unwrap_or_else(|errors| {
                panic!(
                    "prepare_codegen_fir_from_callable_args failed: {}",
                    format_interpret_errors(errors)
                )
            });
    match backend {
        CallableArgsBackend::SyntheticEntry => {
            let entry = crate::codegen::qir::entry_from_codegen_fir(&codegen_fir);
            qsc_codegen::qir::fir_to_qir(
                &codegen_fir.fir_store,
                capabilities,
                &codegen_fir.compute_properties,
                &entry,
            )
            .unwrap_or_else(|e| panic!("fir_to_qir failed: {e:?}"))
        }
        CallableArgsBackend::ReinvokeOriginal { callable, args } => {
            qsc_codegen::qir::fir_to_qir_from_callable(
                &codegen_fir.fir_store,
                capabilities,
                &codegen_fir.compute_properties,
                callable,
                args,
            )
            .unwrap_or_else(|e| panic!("fir_to_qir_from_callable failed: {e:?}"))
        }
    }
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let (codegen_fir, _backend) =
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
fn synthetic_path_classical_capture_closure_generates_qir() {
    // A closure capturing a classical value (Int shift) flows through the
    // self-contained synthetic entry: the capture is materialized as a local and
    // the closure is reconstructed for partial evaluation, which specializes the
    // lifted callable and emits the corresponding gates. The target also takes a
    // plain Int argument so the synthetic entry exercises mixed closure/scalar
    // argument lowering.
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Canon.*;
            import Std.Measurement.*;

            operation RunOp(op : (Qubit[] => Unit), reps : Int) : Result[] {
                use register = Qubit[2];
                for _ in 1..reps {
                    op(register);
                }
                return MResetEachZ(register);
            }

            operation Shifted(shift : Int, register : Qubit[]) : Unit {
                ApplyXorInPlace(shift, register);
            }

            operation MakeShift(shift : Int) : (Qubit[] => Unit) {
                return register => Shifted(shift, register);
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("RunOp", true), ("Shifted", true), ("<lambda>", true)],
        caps,
    );

    // The lifted lambda's input is `(shift, register)`; capturing `shift = 1`
    // leaves the explicit `register` slot for the synthetic entry to supply.
    let shifted_closure = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Int(1)].into(),
        id: fir_id_for(pkg, items["<lambda>"]),
        functor: FunctorApp::default(),
    }));
    let args = Value::Tuple(vec![shifted_closure, Value::Int(1)].into(), None);

    // A classical capture must route through the synthetic entry, not the pin path.
    let target_hir = hir_id_for(pkg, items["RunOp"]);
    let (_codegen_fir, backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &args, caps,
    )
    .unwrap_or_else(|errors| {
        panic!(
            "classical-capture closure should produce CodegenFir, got: {}",
            format_interpret_errors(errors)
        )
    });
    assert!(
        matches!(backend, CallableArgsBackend::SyntheticEntry),
        "classical-capture closure should route to the synthetic entry"
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunOp"], &args, caps);
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate from the captured shift in QIR:\n{qir}"
    );
}

#[test]
fn classical_capture_closure_routes_to_synthetic_entry_qubit_capture_does_not() {
    // A closure capturing a runtime qubit identity cannot be lowered to a FIR
    // literal, so it must keep the pin-based `ReinvokeOriginal` route.
    let source = indoc::indoc! {r#"
        namespace Test {
            import Std.Measurement.*;

            operation RunOp(op : (Qubit => Unit)) : Result {
                use q = Qubit();
                op(q);
                return MResetZ(q);
            }

            operation Entangle(control : Qubit, target : Qubit) : Unit is Adj + Ctl {
                CNOT(control, target);
            }

            operation MakeEntangler(control : Qubit) : (Qubit => Unit) {
                return target => Entangle(control, target);
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) = compile_and_locate_items(
        source,
        &[("RunOp", true), ("Entangle", true), ("<lambda>", true)],
        caps,
    );

    // Keep the captured qubit alive for the duration of the closure value.
    let captured_qubit = Rc::new(qsc_eval::val::Qubit(0));
    let qubit_closure = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Qubit((&captured_qubit).into())].into(),
        id: fir_id_for(pkg, items["<lambda>"]),
        functor: FunctorApp::default(),
    }));

    let target_hir = hir_id_for(pkg, items["RunOp"]);
    let (_codegen_fir, backend) =
        prepare_codegen_fir_from_callable_args(&store, target_hir, &qubit_closure, caps)
            .unwrap_or_else(|errors| {
                panic!(
                    "qubit-capture closure should still produce CodegenFir, got: {}",
                    format_interpret_errors(errors)
                )
            });
    assert!(
        matches!(backend, CallableArgsBackend::ReinvokeOriginal { .. }),
        "qubit-capture closure should keep the pin-based ReinvokeOriginal route"
    );
}

#[test]
fn synthetic_path_array_arg_preserves_element_values() {
    // A `Value::Array` argument must survive materialization on the synthetic
    // path with its element VALUES intact (not just its length). Each nonzero
    // element drives one `op(q)` call, so the gate count proves the concrete
    // contents `[1, 0, 1]` were threaded through `build_synthetic_args`.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation RunWith(op : Qubit => Unit, data : Int[]) : Result {
                use q = Qubit();
                for x in data {
                    if x != 0 {
                        op(q);
                    }
                }
                MResetZ(q)
            }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunWith", true), ("DoX", true)], caps);

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let args = Value::Tuple(
        vec![
            do_x,
            Value::Array(vec![Value::Int(1), Value::Int(0), Value::Int(1)].into()),
        ]
        .into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunWith"], &args, caps);
    // Count call sites only (the bare symbol also appears in the `declare` line).
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        2,
        "expected exactly 2 X gates (one per nonzero element) in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_empty_array_arg_does_not_panic() {
    // Regression: an empty `Value::Array` argument previously lowered to
    // `Ty::Array(Ty::Err)`, which panicked at `PostAll` in release builds.
    // The element-type hint fix lets the empty array carry its real element
    // type, so codegen succeeds and emits no `op(q)` calls.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation RunWith(op : Qubit => Unit, data : Int[]) : Result {
                use q = Qubit();
                for x in data {
                    if x != 0 {
                        op(q);
                    }
                }
                MResetZ(q)
            }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunWith", true), ("DoX", true)], caps);

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let args = Value::Tuple(vec![do_x, Value::Array(vec![].into())].into(), None);

    let qir = callable_args_to_qir(&store, pkg, items["RunWith"], &args, caps);
    // Count call sites only (the bare symbol also appears in the `declare` line).
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        0,
        "expected no X gates for an empty array argument in QIR:\n{qir}"
    );
}

#[test]
fn synthetic_path_nested_empty_array_arg_does_not_panic() {
    // Regression: a nested array `[[]] : Int[][]` whose inner array is empty
    // previously poisoned the OUTER element type with `Ty::Err`, panicking at
    // `PostAll`. The element-type hint must recurse so the outer array keeps
    // its `Int[]` element type even when an inner array is empty.
    let source = indoc::indoc! {r#"
        namespace Test {
            operation RunNested(op : Qubit => Unit, data : Int[][]) : Result {
                use q = Qubit();
                for inner in data {
                    for x in inner {
                        if x != 0 {
                            op(q);
                        }
                    }
                }
                MResetZ(q)
            }
            operation DoX(q : Qubit) : Unit { X(q); }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("RunNested", true), ("DoX", true)], caps);

    let do_x = Value::Global(fir_id_for(pkg, items["DoX"]), FunctorApp::default());
    let args = Value::Tuple(
        vec![do_x, Value::Array(vec![Value::Array(vec![].into())].into())].into(),
        None,
    );

    let qir = callable_args_to_qir(&store, pkg, items["RunNested"], &args, caps);
    // Count call sites only (the bare symbol also appears in the `declare` line).
    assert_eq!(
        qir.matches("call void @__quantum__qis__x__body").count(),
        0,
        "expected no X gates for a nested empty array argument in QIR:\n{qir}"
    );
}

// ---- SyntheticEntry vs ReinvokeOriginal early-return-in-dynamic-branch parity ----

/// Target body shared by the early-return parity tests below. Its early `return` sits
/// inside a measurement-dependent (`MResetZ`) branch with statements after the branch,
/// which is exactly the `ReturnWithinDynamicScope` shape that the entry-reachable
/// `return_unify` pass rewrites.
const EARLY_RETURN_TARGET_BODY: &str = r#"
    operation RunOp(op : (Qubit => Unit)) : Int {
        let r = {
            use q = Qubit();
            op(q);
            MResetZ(q)
        };
        if r == One {
            return 1;
        }
        return 2;
    }
"#;

/// Source whose closure arg captures a classical value, so the arg is FIR-lowerable
/// and routes through the self-contained `SyntheticEntry` backend.
fn early_return_synthetic_entry_source() -> String {
    format!(
        r#"
namespace Test {{
    import Std.Measurement.*;
{EARLY_RETURN_TARGET_BODY}
    operation Rotate(reps : Int, target : Qubit) : Unit {{
        for _ in 1..reps {{
            X(target);
        }}
    }}

    operation MakeRotation(reps : Int) : (Qubit => Unit) {{
        return target => Rotate(reps, target);
    }}
}}
"#
    )
}

/// Source whose closure arg captures an allocated qubit (a runtime identity that is
/// NOT FIR-lowerable), forcing the pin-based `ReinvokeOriginal` backend.
fn early_return_reinvoke_original_source() -> String {
    format!(
        r#"
namespace Test {{
    import Std.Measurement.*;
{EARLY_RETURN_TARGET_BODY}
    operation Entangle(control : Qubit, target : Qubit) : Unit is Adj + Ctl {{
        CNOT(control, target);
    }}

    operation MakeEntangler(control : Qubit) : (Qubit => Unit) {{
        return target => Entangle(control, target);
    }}
}}
"#
    )
}

/// Parity regression for the `SyntheticEntry`-vs-`ReinvokeOriginal` capability behavior
/// on early-return-in-dynamic-branch closures.
///
/// Both variants pass the SAME target operation (`RunOp`), whose body early-returns
/// inside a measurement-dependent branch. They differ ONLY in the closure capture:
///
/// * Classical capture -> FIR-lowerable -> `SyntheticEntry`. The target body becomes
///   entry-reachable through the synthetic `Call`, so `return_unify` rewrites the early
///   return and the program compiles to QIR under an Adaptive profile.
/// * Qubit capture -> NOT FIR-lowerable -> `ReinvokeOriginal`. The target body is pinned
///   (not entry-reachable), so the main pipeline never return-unifies it. The body-only
///   signature-preserving sub-pipeline runs on the pinned body, so the early return is
///   rewritten into flag-guarded forward control flow and the program compiles to QIR
///   under an Adaptive profile with parity to the `SyntheticEntry` variant.
///
/// This test asserts both routes compile.
#[test]
fn early_return_in_dynamic_branch_synthetic_and_reinvoke_both_compile_parity() {
    let caps = Profile::AdaptiveRIF.into();

    // --- SyntheticEntry variant: classical capture compiles to QIR. ---
    let synthetic_source = early_return_synthetic_entry_source();
    let (store, pkg, items) = compile_and_locate_items(
        &synthetic_source,
        &[("RunOp", true), ("Rotate", true), ("<lambda>", true)],
        caps,
    );

    // The lifted lambda's input is `(reps, target)`; capturing `reps = 1` leaves the
    // explicit `target` slot for the closure invocation to supply.
    let rotation_closure = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Int(1)].into(),
        id: fir_id_for(pkg, items["<lambda>"]),
        functor: FunctorApp::default(),
    }));

    let target_hir = hir_id_for(pkg, items["RunOp"]);
    let (_codegen_fir, backend) =
        prepare_codegen_fir_from_callable_args(&store, target_hir, &rotation_closure, caps)
            .unwrap_or_else(|errors| {
                panic!(
                    "classical-capture early-return closure should produce CodegenFir, got: {}",
                    format_interpret_errors(errors)
                )
            });
    assert!(
        matches!(backend, CallableArgsBackend::SyntheticEntry),
        "classical-capture closure should route to the synthetic entry"
    );
    let qir = callable_args_to_qir(&store, pkg, items["RunOp"], &rotation_closure, caps);
    assert!(
        qir.contains("__quantum__qis__x__body"),
        "expected X gate from the SyntheticEntry early-return body in QIR:\n{qir}"
    );

    // --- ReinvokeOriginal variant: qubit capture compiles to QIR. ---
    let reinvoke_source = early_return_reinvoke_original_source();
    let (store, pkg, items) = compile_and_locate_items(
        &reinvoke_source,
        &[("RunOp", true), ("Entangle", true), ("<lambda>", true)],
        caps,
    );

    // Keep the captured qubit alive for the duration of the closure value.
    let captured_qubit = Rc::new(qsc_eval::val::Qubit(0));
    let qubit_closure = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Qubit((&captured_qubit).into())].into(),
        id: fir_id_for(pkg, items["<lambda>"]),
        functor: FunctorApp::default(),
    }));

    let target_hir = hir_id_for(pkg, items["RunOp"]);
    let (_codegen_fir, backend) =
        prepare_codegen_fir_from_callable_args(&store, target_hir, &qubit_closure, caps)
            .unwrap_or_else(|errors| {
                panic!(
                    "qubit-capture early-return closure should compile \
                     (ReinvokeOriginal sub-pipeline return-unifies the pinned body), got: {}",
                    format_interpret_errors(errors)
                )
            });
    assert!(
        matches!(backend, CallableArgsBackend::ReinvokeOriginal { .. }),
        "qubit-capture closure should keep the pin-based ReinvokeOriginal route"
    );
    let qir = callable_args_to_qir(&store, pkg, items["RunOp"], &qubit_closure, caps);
    assert!(
        qir.contains("__quantum__qis__cnot__body") || qir.contains("__quantum__qis__cx__body"),
        "expected the entangler CNOT from the ReinvokeOriginal early-return body in QIR:\n{qir}"
    );
}

/// Parity check: the `ReinvokeOriginal` early-return-in-dynamic-branch body compiles to
/// QIR with parity to the `SyntheticEntry` variant now that the body-only
/// signature-preserving sub-pipeline runs `return_unify` on the pinned target body.
///
/// This is the focused twin of
/// `early_return_in_dynamic_branch_synthetic_and_reinvoke_both_compile_parity`: it isolates
/// the `ReinvokeOriginal` route and asserts the entangler CNOT appears in the QIR.
#[test]
fn early_return_in_dynamic_branch_reinvoke_original_compiles_parity_target() {
    let caps = Profile::AdaptiveRIF.into();

    let reinvoke_source = early_return_reinvoke_original_source();
    let (store, pkg, items) = compile_and_locate_items(
        &reinvoke_source,
        &[("RunOp", true), ("Entangle", true), ("<lambda>", true)],
        caps,
    );

    // Keep the captured qubit alive for the duration of the closure value.
    let captured_qubit = Rc::new(qsc_eval::val::Qubit(0));
    let qubit_closure = Value::Closure(Box::new(qsc_eval::val::Closure {
        fixed_args: vec![Value::Qubit((&captured_qubit).into())].into(),
        id: fir_id_for(pkg, items["<lambda>"]),
        functor: FunctorApp::default(),
    }));

    let target_hir = hir_id_for(pkg, items["RunOp"]);
    let (_codegen_fir, backend) =
        prepare_codegen_fir_from_callable_args(&store, target_hir, &qubit_closure, caps)
            .unwrap_or_else(|errors| {
                panic!(
                    "qubit-capture early-return closure should compile, got: {}",
                    format_interpret_errors(errors)
                )
            });
    assert!(
        matches!(backend, CallableArgsBackend::ReinvokeOriginal { .. }),
        "qubit-capture closure should keep the pin-based ReinvokeOriginal route"
    );
    let qir = callable_args_to_qir(&store, pkg, items["RunOp"], &qubit_closure, caps);
    assert!(
        qir.contains("__quantum__qis__cnot__body") || qir.contains("__quantum__qis__cx__body"),
        "expected the entangler CNOT from the ReinvokeOriginal early-return body in QIR:\n{qir}"
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
    let caps = Profile::AdaptiveRIF.into();
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
    let (codegen_fir, _backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &boxed, caps,
    )
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let (codegen_fir, _backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &boxed, caps,
    )
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let (codegen_fir, _backend) = prepare_codegen_fir_from_callable_args(
        &store, target_hir, &outer, caps,
    )
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    let caps = Profile::AdaptiveRIF.into();
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
    // `First` maps to `DoH` and `Second` to `DoX`. Confirm the per-field
    // dispatch resolves in that order, guarding against a field-index mix-up
    // where the second callable field collapses onto the first.
    let h_pos = qir.find("__quantum__qis__h__body").expect("H gate present");
    let x_pos = qir.find("__quantum__qis__x__body").expect("X gate present");
    assert!(
        h_pos < x_pos,
        "expected First (H) to be emitted before Second (X):\n{qir}"
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
    let caps = Profile::AdaptiveRIF.into();
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

// ---- Synthetic path: callable whose RETURN type is a closure ----

#[test]
fn callable_returning_closure_arg_generates_qir() {
    // `MakeOp` returns a closure (`() => H(First(qs))`). Passing it to `DoOp`
    // previously panicked with "global not present" because codegen re-invoked
    // the original target after the producer closure had been erased. The
    // synthetic-entry route must generate QIR containing the H gate instead.
    let source = indoc::indoc! {r#"
        namespace Test {
            function First<'T>(arr : 'T[]) : 'T { arr[0] }
            function MakeOp(qs : Qubit[]) : Unit => Unit is Adj + Ctl {
                () => H(First(qs))
            }
            operation DoOp(make : Qubit[] -> Unit => Unit is Adj + Ctl) : Unit is Adj + Ctl {
                use qs = Qubit[1];
                let op = make(qs);
                op();
            }
        }
    "#};
    let caps = Profile::AdaptiveRIF.into();
    let (store, pkg, items) =
        compile_and_locate_items(source, &[("DoOp", true), ("MakeOp", true)], caps);

    let make_op = Value::Global(fir_id_for(pkg, items["MakeOp"]), FunctorApp::default());
    let qir = callable_args_to_qir(&store, pkg, items["DoOp"], &make_op, caps);
    assert!(
        qir.contains("__quantum__qis__h__body"),
        "expected H gate in QIR:\n{qir}"
    );
}
