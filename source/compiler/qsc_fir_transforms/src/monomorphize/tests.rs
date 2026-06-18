// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::{Expect, expect};
use indoc::indoc;

/// Compiles Q# source, runs monomorphization, and snapshots all callables
/// in the user package showing name, generic-param count, input type, and
/// output type. Sorted for determinism.
fn check(source: &str, expect: &Expect) {
    let (store, pkg_id) = compile_and_monomorphize(source);

    let package = store.get(pkg_id);
    let mut lines: Vec<String> = Vec::new();
    for (_, item) in &package.items {
        if let ItemKind::Callable(decl) = &item.kind {
            let pat = package.get_pat(decl.input);
            lines.push(format!(
                "{}: generics={}, input={}, output={}",
                decl.name.name,
                decl.generics.len(),
                pat.ty,
                decl.output,
            ));
        }
    }
    lines.sort();
    expect.assert_eq(&lines.join("\n"));
}

fn check_details(source: &str, expect: &Expect) {
    let (store, pkg_id) = crate::test_utils::compile_and_run_pipeline_to(
        source,
        crate::test_utils::PipelineStage::Mono,
    );
    expect.assert_eq(&crate::test_utils::extract_reachable_callable_details(
        &store, pkg_id,
    ));
}

fn compile_and_monomorphize(source: &str) -> (qsc_fir::fir::PackageStore, qsc_fir::fir::PackageId) {
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(source);
    let mut assigners = crate::package_assigners::PackageAssigners::entry(&store, pkg_id);
    monomorphize(&mut store, pkg_id, &mut assigners);
    (store, pkg_id)
}

fn compile_entry_and_monomorphize(
    source: &str,
    entry: &str,
) -> (qsc_fir::fir::PackageStore, qsc_fir::fir::PackageId) {
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir_with_entry(source, entry);
    let mut assigners = crate::package_assigners::PackageAssigners::entry(&store, pkg_id);
    monomorphize(&mut store, pkg_id, &mut assigners);
    (store, pkg_id)
}

fn entry_callee_name_and_generic_arg_count(package: &qsc_fir::fir::Package) -> (String, usize) {
    let entry_id = package
        .entry
        .expect("package should have an entry expression");
    let ExprKind::Call(callee_id, _) = package.get_expr(entry_id).kind else {
        panic!("entry expression should remain a call")
    };
    let ExprKind::Var(Res::Item(item_id), ref generic_args) = package.get_expr(callee_id).kind
    else {
        panic!("entry callee should be a callable reference")
    };
    let ItemKind::Callable(decl) = &package.get_item(item_id.item).kind else {
        panic!("entry callee should resolve to a callable item")
    };
    (decl.name.name.to_string(), generic_args.len())
}

/// Compiles Q# source, runs monomorphization, and asserts no
/// `ExprKind::Var` in the user package still carries generic args.
fn assert_no_generic_args(source: &str) {
    let (store, pkg_id) = compile_and_monomorphize(source);

    let package = store.get(pkg_id);
    for (id, expr) in &package.exprs {
        if let ExprKind::Var(_, ref args) = expr.kind {
            assert!(
                args.is_empty(),
                "Expr {id} still has non-empty generic args after monomorphization"
            );
        }
    }
}

fn reachable_parametric_callable_details(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) -> Vec<String> {
    let reachable = crate::reachability::collect_reachable_from_entry(store, pkg_id);
    let package = store.get(pkg_id);
    package
        .items
        .iter()
        .filter(|(item_id, _)| {
            reachable.contains(&qsc_fir::fir::StoreItemId {
                package: pkg_id,
                item: *item_id,
            })
        })
        .filter_map(|(_, item)| {
            let ItemKind::Callable(decl) = &item.kind else {
                return None;
            };

            let input_ty = &package.get_pat(decl.input).ty;
            let output_has_param = super::ty_contains_param(&decl.output);
            let input_has_param = super::ty_contains_param(input_ty);
            let functor_param = matches!(
                input_ty,
                qsc_fir::ty::Ty::Arrow(arrow)
                    if matches!(arrow.functors, qsc_fir::ty::FunctorSet::Param(_))
            );

            (output_has_param || input_has_param || functor_param).then(|| {
                format!(
                    "{}: generics={}, input={}, output={}",
                    decl.name.name,
                    decl.generics.len(),
                    input_ty,
                    decl.output,
                )
            })
        })
        .collect()
}

#[test]
fn mono_explicit_entry_expression_rewritten() {
    let (store, pkg_id) = compile_entry_and_monomorphize(
        indoc! {r#"
                namespace Test {
                    function Identity<'T>(x : 'T) : 'T { x }
                }
            "#},
        "Test.Identity(42)",
    );
    assert_eq!(
        entry_callee_name_and_generic_arg_count(store.get(pkg_id)),
        ("Identity<Int>".to_string(), 0),
    );
}

#[test]
fn mono_identity_int() {
    let source = indoc! {r#"
                operation Identity<'T>(x : 'T) : 'T { x }
                operation Main() : Int { Identity(42) }
            "#};
    check(
        source,
        &expect![[r#"
            Identity: generics=1, input=Param<0>, output=Param<0>
            Identity<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Main() : Int {
                Identity < Int > (42)
            }
            // entry
            Main()

            AFTER:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Main() : Int {
                Identity_Int_(42)
            }
            operation Identity_Int_(x : Int) : Int {
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_identity_qubit() {
    let source = indoc! {r#"
                operation Identity<'T>(x : 'T) : 'T { x }
                operation Main() : Unit {
                    use q = Qubit();
                    let _ = Identity(q);
                }
            "#};
    check(
        source,
        &expect![[r#"
            Identity: generics=1, input=Param<0>, output=Param<0>
            Identity<Qubit>: generics=0, input=Qubit, output=Qubit
            Main: generics=0, input=Unit, output=Unit"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _ : Qubit = Identity < Qubit > (q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _ : Qubit = Identity_Qubit_(q);
                __quantum__rt__qubit_release(q);
            }
            operation Identity_Qubit_(x : Qubit) : Qubit {
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_two_instantiations() {
    let source = indoc! {r#"
                operation Identity<'T>(x : 'T) : 'T { x }
                operation Main() : Unit {
                    let _ = Identity(42);
                    use q = Qubit();
                    let _ = Identity(q);
                }
            "#};
    check(
        source,
        &expect![[r#"
            Identity: generics=1, input=Param<0>, output=Param<0>
            Identity<Int>: generics=0, input=Int, output=Int
            Identity<Qubit>: generics=0, input=Qubit, output=Qubit
            Main: generics=0, input=Unit, output=Unit"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Main() : Unit {
                let _ : Int = Identity < Int > (42);
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _ : Qubit = Identity < Qubit > (q);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Main() : Unit {
                let _ : Int = Identity_Int_(42);
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _ : Qubit = Identity_Qubit_(q);
                __quantum__rt__qubit_release(q);
            }
            operation Identity_Int_(x : Int) : Int {
                x
            }
            operation Identity_Qubit_(x : Qubit) : Qubit {
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_no_generic_args() {
    let source = "operation Main() : Int { 42 }";
    check(source, &expect!["Main: generics=0, input=Unit, output=Int"]);
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Int {
                42
            }
            // entry
            Main()

            AFTER:
            operation Main() : Int {
                42
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_multiple_call_sites_same_args() {
    // Two call sites with Identity<Int> should produce only one
    // specialization.
    let source = indoc! {r#"
                operation Identity<'T>(x : 'T) : 'T { x }
                operation Main() : Unit {
                    let _ = Identity(1);
                    let _ = Identity(2);
                }
            "#};
    check(
        source,
        &expect![[r#"
            Identity: generics=1, input=Param<0>, output=Param<0>
            Identity<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Unit"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Main() : Unit {
                let _ : Int = Identity < Int > (1);
                let _ : Int = Identity < Int > (2);
            }
            // entry
            Main()

            AFTER:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Main() : Unit {
                let _ : Int = Identity_Int_(1);
                let _ : Int = Identity_Int_(2);
            }
            operation Identity_Int_(x : Int) : Int {
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_generic_args_cleared_after_mono() {
    assert_no_generic_args(indoc! {r#"
            operation Identity<'T>(x : 'T) : 'T { x }
            operation Main() : Unit {
                let _ = Identity(42);
                use q = Qubit();
                let _ = Identity(q);
            }
        "#});
}

#[test]
fn mono_nested_generic_call() {
    // Outer<'T> calls Identity<'T> — both should be specialized.
    let source = indoc! {r#"
                operation Identity<'T>(x : 'T) : 'T { x }
                operation Outer<'T>(x : 'T) : 'T { Identity(x) }
                operation Main() : Int { Outer(42) }
            "#};
    check(
        source,
        &expect![[r#"
            Identity: generics=1, input=Param<0>, output=Param<0>
            Identity<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int
            Outer: generics=1, input=Param<0>, output=Param<0>
            Outer<Int>: generics=0, input=Int, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Outer(x : 'T0) : 'T0 {
                Identity < 'T0 > (x)
            }
            operation Main() : Int {
                Outer < Int > (42)
            }
            // entry
            Main()

            AFTER:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Outer(x : 'T0) : 'T0 {
                Identity(x)
            }
            operation Main() : Int {
                Outer_Int_(42)
            }
            operation Outer_Int_(x : Int) : Int {
                Identity_Int_(x)
            }
            operation Identity_Int_(x : Int) : Int {
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_nested_generic_body_retargets_specialized_callee() {
    check_details(
        indoc! {r#"
                function Inner<'T>(x : 'T) : 'T { x }
                function Outer<'T>(x : 'T) : 'T {
                    let first = Inner(x);
                    Inner(first)
                }
                function Main() : Int { Outer(42) }
            "#},
        &expect![[r#"
            callable Inner<Int>: input_ty=Int, output_ty=Int
              body: block_ty=Int
                [0] Expr ty=Int Var
            callable Main: input_ty=Unit, output_ty=Int
              body: block_ty=Int
                [0] Expr ty=Int Call(Outer<Int>, arg_ty=Int)
            callable Outer<Int>: input_ty=Int, output_ty=Int
              body: block_ty=Int
                [0] Local pat_ty=Int init_ty=Int Call(Inner<Int>, arg_ty=Int)
                [1] Expr ty=Int Call(Inner<Int>, arg_ty=Int)"#]],
    );
}

#[test]
fn mono_partial_application_skips_non_concrete_stdlib_generics() {
    let source = indoc! {r#"
        namespace Test {
            import Std.Arrays.*;
            import Std.Convert.*;
            import Std.Diagnostics.*;
            import Std.Intrinsic.*;
            import Std.Math.*;
            import Std.Measurement.*;

            @EntryPoint()
            operation Main() : Result[] {
                let secretBitString = SecretBitStringAsBoolArray();
                let parityOperation = EncodeBitStringAsParityOperation(secretBitString);
                let decodedBitString = BernsteinVazirani(
                    parityOperation,
                    Length(secretBitString)
                );

                return decodedBitString;
            }

            operation BernsteinVazirani(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
                use queryRegister = Qubit[n];
                use target = Qubit();
                X(target);
                within {
                    ApplyToEachA(H, queryRegister);
                } apply {
                    H(target);
                    Uf(queryRegister, target);
                }
                let resultArray = MResetEachZ(queryRegister);
                Reset(target);
                return resultArray;
            }

            operation ApplyParityOperation(
                bitStringAsBoolArray : Bool[],
                xRegister : Qubit[],
                yQubit : Qubit
            ) : Unit {
                let requiredBits = Length(bitStringAsBoolArray);
                let availableQubits = Length(xRegister);
                Fact(
                    availableQubits >= requiredBits,
                    $"The bitstring has {requiredBits} bits but the quantum register " + $"only has {availableQubits} qubits"
                );
                for (index, bit) in Enumerated(bitStringAsBoolArray) {
                    if bit {
                        CNOT(xRegister[index], yQubit);
                    }
                }
            }

            operation EncodeBitStringAsParityOperation(bitStringAsBoolArray : Bool[]) : (Qubit[], Qubit) => Unit {
                return ApplyParityOperation(bitStringAsBoolArray, _, _);
            }

            function SecretBitStringAsBoolArray() : Bool[] {
                return [true, false, true, false, true];
            }
        }
    "#};

    let (store, pkg_id) = compile_and_monomorphize(source);
    let offenders = reachable_parametric_callable_details(&store, pkg_id);
    assert!(
        offenders.is_empty(),
        "offending callables after mono:\n{}",
        offenders.join("\n")
    );
    crate::invariants::check(&store, pkg_id, crate::invariants::InvariantLevel::PostMono);
}

#[test]
fn mono_nested_depth_2() {
    // A→B→C chain of generic calls.
    let source = indoc! {r#"
                operation C<'T>(x : 'T) : 'T { x }
                operation B<'T>(x : 'T) : 'T { C(x) }
                operation A<'T>(x : 'T) : 'T { B(x) }
                operation Main() : Int { A(42) }
            "#};
    check(
        source,
        &expect![[r#"
            A: generics=1, input=Param<0>, output=Param<0>
            A<Int>: generics=0, input=Int, output=Int
            B: generics=1, input=Param<0>, output=Param<0>
            B<Int>: generics=0, input=Int, output=Int
            C: generics=1, input=Param<0>, output=Param<0>
            C<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation C(x : 'T0) : 'T0 {
                x
            }
            operation B(x : 'T0) : 'T0 {
                C < 'T0 > (x)
            }
            operation A(x : 'T0) : 'T0 {
                B < 'T0 > (x)
            }
            operation Main() : Int {
                A < Int > (42)
            }
            // entry
            Main()

            AFTER:
            operation C(x : 'T0) : 'T0 {
                x
            }
            operation B(x : 'T0) : 'T0 {
                C(x)
            }
            operation A(x : 'T0) : 'T0 {
                B(x)
            }
            operation Main() : Int {
                A_Int_(42)
            }
            operation A_Int_(x : Int) : Int {
                B_Int_(x)
            }
            operation B_Int_(x : Int) : Int {
                C_Int_(x)
            }
            operation C_Int_(x : Int) : Int {
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn mono_nested_diamond() {
    // Diamond: A calls B and C, both call D.
    // D should be specialized only once.
    let source = indoc! {r#"
                operation D<'T>(x : 'T) : 'T { x }
                operation B<'T>(x : 'T) : 'T { D(x) }
                operation C<'T>(x : 'T) : 'T { D(x) }
                operation A<'T>(x : 'T) : 'T {
                    let _ = B(x);
                    C(x)
                }
                operation Main() : Int { A(42) }
            "#};
    check(
        source,
        &expect![[r#"
            A: generics=1, input=Param<0>, output=Param<0>
            A<Int>: generics=0, input=Int, output=Int
            B: generics=1, input=Param<0>, output=Param<0>
            B<Int>: generics=0, input=Int, output=Int
            C: generics=1, input=Param<0>, output=Param<0>
            C<Int>: generics=0, input=Int, output=Int
            D: generics=1, input=Param<0>, output=Param<0>
            D<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation D(x : 'T0) : 'T0 {
                x
            }
            operation B(x : 'T0) : 'T0 {
                D < 'T0 > (x)
            }
            operation C(x : 'T0) : 'T0 {
                D < 'T0 > (x)
            }
            operation A(x : 'T0) : 'T0 {
                let _ : 'T0 = B < 'T0 > (x);
                C < 'T0 > (x)
            }
            operation Main() : Int {
                A < Int > (42)
            }
            // entry
            Main()

            AFTER:
            operation D(x : 'T0) : 'T0 {
                x
            }
            operation B(x : 'T0) : 'T0 {
                D(x)
            }
            operation C(x : 'T0) : 'T0 {
                D(x)
            }
            operation A(x : 'T0) : 'T0 {
                let _ : 'T0 = B(x);
                C(x)
            }
            operation Main() : Int {
                A_Int_(42)
            }
            operation A_Int_(x : Int) : Int {
                let _ : Int = B_Int_(x);
                C_Int_(x)
            }
            operation B_Int_(x : Int) : Int {
                D_Int_(x)
            }
            operation C_Int_(x : Int) : Int {
                D_Int_(x)
            }
            operation D_Int_(x : Int) : Int {
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_arrow_param() {
    // Generic callable with arrow-typed parameter.
    let source = indoc! {r#"
                operation ApplyOp<'T>(f : 'T => 'T, x : 'T) : 'T { f(x) }
                operation DoubleInt(x : Int) : Int { x * 2 }
                operation Main() : Int { ApplyOp(DoubleInt, 5) }
            "#};
    check(
        source,
        &expect![[r#"
            ApplyOp: generics=2, input=((Param<0> => Param<0> is 1), Param<0>), output=Param<0>
            ApplyOp<Int, Empty>: generics=0, input=((Int => Int), Int), output=Int
            DoubleInt: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(f : ('T0 => 'T0), x : 'T0) : 'T0 {
                f(x)
            }
            operation DoubleInt(x : Int) : Int {
                x * 2
            }
            operation Main() : Int {
                ApplyOp < Int,
                () > (DoubleInt, 5)
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(f : ('T0 => 'T0), x : 'T0) : 'T0 {
                f(x)
            }
            operation DoubleInt(x : Int) : Int {
                x * 2
            }
            operation Main() : Int {
                ApplyOp_Int__Empty_(DoubleInt, 5)
            }
            operation ApplyOp_Int__Empty_(f : (Int => Int), x : Int) : Int {
                f(x)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_generic_with_body_locals() {
    let source = indoc! {r#"
                operation Transform<'T>(x : 'T) : 'T {
                    let tmp = x;
                    tmp
                }
                operation Main() : Int { Transform(42) }
            "#};
    check(
        source,
        &expect![[r#"
            Main: generics=0, input=Unit, output=Int
            Transform: generics=1, input=Param<0>, output=Param<0>
            Transform<Int>: generics=0, input=Int, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Transform(x : 'T0) : 'T0 {
                let tmp : 'T0 = x;
                tmp
            }
            operation Main() : Int {
                Transform < Int > (42)
            }
            // entry
            Main()

            AFTER:
            operation Transform(x : 'T0) : 'T0 {
                let tmp : 'T0 = x;
                tmp
            }
            operation Main() : Int {
                Transform_Int_(42)
            }
            operation Transform_Int_(x : Int) : Int {
                let tmp : Int = x;
                tmp
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_generic_preserves_local_chain() {
    // Multiple local bindings chained together.
    let source = indoc! {r#"
                operation Chain<'T>(x : 'T) : 'T {
                    let a = x;
                    let b = a;
                    let c = b;
                    let d = c;
                    d
                }
                operation Main() : Int { Chain(42) }
            "#};
    check(
        source,
        &expect![[r#"
            Chain: generics=1, input=Param<0>, output=Param<0>
            Chain<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Chain(x : 'T0) : 'T0 {
                let a : 'T0 = x;
                let b : 'T0 = a;
                let c : 'T0 = b;
                let d : 'T0 = c;
                d
            }
            operation Main() : Int {
                Chain < Int > (42)
            }
            // entry
            Main()

            AFTER:
            operation Chain(x : 'T0) : 'T0 {
                let a : 'T0 = x;
                let b : 'T0 = a;
                let c : 'T0 = b;
                let d : 'T0 = c;
                d
            }
            operation Main() : Int {
                Chain_Int_(42)
            }
            operation Chain_Int_(x : Int) : Int {
                let a : Int = x;
                let b : Int = a;
                let c : Int = b;
                let d : Int = c;
                d
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_generic_with_ctl_spec() {
    let source = indoc! {r#"
                operation ApplyCtl<'T>(x : 'T) : Unit is Ctl {
                    body ... { }
                    controlled (ctls, ...) { }
                }
                operation Main() : Unit {
                    use q = Qubit();
                    ApplyCtl(42);
                }
            "#};
    check(
        source,
        &expect![[r#"
            ApplyCtl: generics=1, input=Param<0>, output=Unit
            ApplyCtl<Int>: generics=0, input=Int, output=Unit
            Main: generics=0, input=Unit, output=Unit"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyCtl(x : 'T0) : Unit is Ctl {
                body ... {}
                controlled (ctls, ...) {}
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyCtl < Int > (42);
                __quantum__rt__qubit_release(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyCtl(x : 'T0) : Unit is Ctl {
                body ... {}
                controlled (ctls, ...) {}
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyCtl_Int_(42);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyCtl_Int_(x : Int) : Unit is Ctl {
                body ... {}
                controlled (ctls, ...) {}
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_closure_in_generic() {
    let source = indoc! {r#"
                operation WithClosure<'T>(x : 'T) : 'T {
                    let f = (y) -> y;
                    f(x)
                }
                operation Main() : Int { WithClosure(42) }
            "#};
    check(
        source,
        &expect![[r#"
            <lambda>: generics=0, input=(Int,), output=Int
            <lambda>: generics=0, input=(Param<0>,), output=Param<0>
            Main: generics=0, input=Unit, output=Int
            WithClosure: generics=1, input=Param<0>, output=Param<0>
            WithClosure<Int>: generics=0, input=Int, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation WithClosure(x : 'T0) : 'T0 {
                let f : ('T0 -> 'T0) = / * closure item = 3 captures = [] * / _lambda_;
                f(x)
            }
            operation Main() : Int {
                WithClosure < Int > (42)
            }
            function _lambda_(y : 'T0, ) : 'T0 {
                y
            }
            // entry
            Main()

            AFTER:
            operation WithClosure(x : 'T0) : 'T0 {
                let f : ('T0 -> 'T0) = / * closure item = 3 captures = [] * / _lambda_;
                f(x)
            }
            operation Main() : Int {
                WithClosure_Int_(42)
            }
            function _lambda_(y : 'T0, ) : 'T0 {
                y
            }
            operation WithClosure_Int_(x : Int) : Int {
                let f : (Int -> Int) = / * closure item = 5 captures = [] * / _lambda_;
                f(x)
            }
            function _lambda_(y : Int, ) : Int {
                y
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_cross_package_length() {
    // Length is a cross-package intrinsic generic callable in std.
    let source = indoc! {r#"
                operation Main() : Int {
                    let arr = [1, 2, 3];
                    Length(arr)
                }
            "#};
    check(source, &expect!["Main: generics=0, input=Unit, output=Int"]);
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Int {
                let arr : Int[] = [1, 2, 3];
                Length < Int > (arr)
            }
            // entry
            Main()

            AFTER:
            operation Main() : Int {
                let arr : Int[] = [1, 2, 3];
                Length(arr)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_cross_package_reversed() {
    // Reversed is a cross-package generic callable.
    let source = indoc! {r#"
                operation Main() : Int[] {
                    let arr = [1, 2, 3];
                    Microsoft.Quantum.Arrays.Reversed(arr)
                }
            "#};
    check(
        source,
        &expect!["Main: generics=0, input=Unit, output=(Int)[]"],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Int[] {
                let arr : Int[] = [1, 2, 3];
                Reversed < Int > (arr)
            }
            // entry
            Main()

            AFTER:
            operation Main() : Int[] {
                let arr : Int[] = [1, 2, 3];
                Reversed_Int_(arr)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_cross_package_with_same_name() {
    // Generic function uses same name as a cross-package generic callable.
    let source = indoc! {r#"
                function Reversed<'T>(array : 'T[]) : 'T[] {
                    Microsoft.Quantum.Arrays.Reversed(array)
                }
                operation Main() : Int[] {
                    let arr = [1, 2, 3];
                    Reversed(arr)
                }
            "#};
    check(
        source,
        &expect![[r#"
            Main: generics=0, input=Unit, output=(Int)[]
            Reversed: generics=1, input=(Param<0>)[], output=(Param<0>)[]
            Reversed<Int>: generics=0, input=(Int)[], output=(Int)[]"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            function Reversed(array : 'T0[]) : 'T0[] {
                Reversed < 'T0 > (array)
            }
            operation Main() : Int[] {
                let arr : Int[] = [1, 2, 3];
                Reversed < Int > (arr)
            }
            // entry
            Main()

            AFTER:
            function Reversed(array : 'T0[]) : 'T0[] {
                Reversed(array)
            }
            operation Main() : Int[] {
                let arr : Int[] = [1, 2, 3];
                Reversed_Int_(arr)
            }
            function Reversed_Int_(array : Int[]) : Int[] {
                Reversed_Int_(array)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_identity_instantiation_not_duplicated() {
    // When Outer<'T> calls Inner<'T>, the Inner<Param(0)> reference is
    // an identity instantiation. Only concrete instantiations (from the
    // entry) should produce specializations.
    let source = indoc! {r#"
                operation Inner<'T>(x : 'T) : 'T { x }
                operation Outer<'T>(x : 'T) : 'T { Inner(x) }
                operation Main() : Int { Outer(42) }
            "#};
    check(
        source,
        &expect![[r#"
            Inner: generics=1, input=Param<0>, output=Param<0>
            Inner<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int
            Outer: generics=1, input=Param<0>, output=Param<0>
            Outer<Int>: generics=0, input=Int, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Inner(x : 'T0) : 'T0 {
                x
            }
            operation Outer(x : 'T0) : 'T0 {
                Inner < 'T0 > (x)
            }
            operation Main() : Int {
                Outer < Int > (42)
            }
            // entry
            Main()

            AFTER:
            operation Inner(x : 'T0) : 'T0 {
                x
            }
            operation Outer(x : 'T0) : 'T0 {
                Inner(x)
            }
            operation Main() : Int {
                Outer_Int_(42)
            }
            operation Outer_Int_(x : Int) : Int {
                Inner_Int_(x)
            }
            operation Inner_Int_(x : Int) : Int {
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_two_type_params() {
    let source = indoc! {r#"
                operation Pair<'A, 'B>(a : 'A, b : 'B) : 'A { a }
                operation Main() : Int {
                    use q = Qubit();
                    Pair(42, q)
                }
            "#};
    check(
        source,
        &expect![[r#"
            Main: generics=0, input=Unit, output=Int
            Pair: generics=2, input=(Param<0>, Param<1>), output=Param<0>
            Pair<Int, Qubit>: generics=0, input=(Int, Qubit), output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Pair(a : 'T0, b : 'T1) : 'T0 {
                a
            }
            operation Main() : Int {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_35 : Int = Pair < Int,
                Qubit > (42, q);
                __quantum__rt__qubit_release(q);
                _generated_ident_35
            }
            // entry
            Main()

            AFTER:
            operation Pair(a : 'T0, b : 'T1) : 'T0 {
                a
            }
            operation Main() : Int {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_35 : Int = Pair_Int__Qubit_(42, q);
                __quantum__rt__qubit_release(q);
                _generated_ident_35
            }
            operation Pair_Int__Qubit_(a : Int, b : Qubit) : Int {
                a
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_missing_same_package_specialization_panics() {
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(indoc! {r#"
            function Identity<'T>(x : 'T) : 'T { x }
            function Main() : Int { Identity(42) }
        "#});

    let expr_ids: Vec<_> = store.get(pkg_id).exprs.iter().map(|(id, _)| id).collect();
    crate::test_utils::assert_panics_with(
        "Non-intrinsic same-package callable has no monomorphized specialization",
        || {
            rewrite_call_sites(
                store.get_mut(pkg_id),
                pkg_id,
                &rustc_hash::FxHashMap::default(),
                &expr_ids,
            );
        },
    );
}

#[test]
fn mono_recursive_generic() {
    // Recursive generic callable — self-references should be rewritten
    // to point at the specialized clone.
    let source = indoc! {r#"
                operation Repeat<'T>(x : 'T, n : Int) : 'T {
                    if n <= 0 {
                        x
                    } else {
                        Repeat(x, n - 1)
                    }
                }
                operation Main() : Int { Repeat(42, 3) }
            "#};
    check(
        source,
        &expect![[r#"
            Main: generics=0, input=Unit, output=Int
            Repeat: generics=1, input=(Param<0>, Int), output=Param<0>
            Repeat<Int>: generics=0, input=(Int, Int), output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Repeat(x : 'T0, n : Int) : 'T0 {
                if n <= 0 {
                    x
                } else {
                    Repeat < 'T0 > (x, n - 1)
                }

            }
            operation Main() : Int {
                Repeat < Int > (42, 3)
            }
            // entry
            Main()

            AFTER:
            operation Repeat(x : 'T0, n : Int) : 'T0 {
                if n <= 0 {
                    x
                } else {
                    Repeat(x, n - 1)
                }

            }
            operation Main() : Int {
                Repeat_Int_(42, 3)
            }
            operation Repeat_Int_(x : Int, n : Int) : Int {
                if n <= 0 {
                    x
                } else {
                    Repeat_Int_(x, n - 1)
                }

            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_generic_with_simulatable_intrinsic() {
    // A generic function used via a simulatable intrinsic path.
    // Length is a cross-package intrinsic: verify it's specialized.
    let source = indoc! {r#"
                operation Wrap<'T>(arr : 'T[]) : Int { Length(arr) }
                operation Main() : Int {
                    Wrap([1, 2, 3])
                }
            "#};
    check(
        source,
        &expect![[r#"
            Main: generics=0, input=Unit, output=Int
            Wrap: generics=1, input=(Param<0>)[], output=Int
            Wrap<Int>: generics=0, input=(Int)[], output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Wrap(arr : 'T0[]) : Int {
                Length < 'T0 > (arr)
            }
            operation Main() : Int {
                Wrap < Int > ([1, 2, 3])
            }
            // entry
            Main()

            AFTER:
            operation Wrap(arr : 'T0[]) : Int {
                Length(arr)
            }
            operation Main() : Int {
                Wrap_Int_([1, 2, 3])
            }
            operation Wrap_Int_(arr : Int[]) : Int {
                Length(arr)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_generic_with_functor_param() {
    // Generic callable with a functor-parameterized operation parameter.
    let source = indoc! {r#"
                operation RunOp<'T>(op : 'T => Unit, x : 'T) : Unit { op(x) }
                operation NoOp(x : Int) : Unit {}
                operation Main() : Unit { RunOp(NoOp, 42) }
            "#};
    check(
        source,
        &expect![[r#"
            Main: generics=0, input=Unit, output=Unit
            NoOp: generics=0, input=Int, output=Unit
            RunOp: generics=2, input=((Param<0> => Unit is 1), Param<0>), output=Unit
            RunOp<Int, Empty>: generics=0, input=((Int => Unit), Int), output=Unit"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation RunOp(op : ('T0 => Unit), x : 'T0) : Unit {
                op(x)
            }
            operation NoOp(x : Int) : Unit {}
            operation Main() : Unit {
                RunOp < Int,
                () > (NoOp, 42)
            }
            // entry
            Main()

            AFTER:
            operation RunOp(op : ('T0 => Unit), x : 'T0) : Unit {
                op(x)
            }
            operation NoOp(x : Int) : Unit {}
            operation Main() : Unit {
                RunOp_Int__Empty_(NoOp, 42)
            }
            operation RunOp_Int__Empty_(op : (Int => Unit), x : Int) : Unit {
                op(x)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_functor_specialized_clone_preserves_explicit_specs() {
    check_details(
        indoc! {r#"
                operation ApplyOp<'T>(op : 'T => Unit is Adj + Ctl, x : 'T) : Unit is Adj + Ctl {
                    body ... { op(x); }
                    adjoint ... { Adjoint op(x); }
                    controlled (ctls, ...) { Controlled op(ctls, x); }
                    controlled adjoint (ctls, ...) { Controlled Adjoint op(ctls, x); }
                }
                operation Main() : Unit {
                    use q = Qubit();
                    ApplyOp(S, q);
                }
            "#},
        &expect![[r#"
                        callable ApplyOp<Qubit, AdjCtl>: input_ty=((Qubit => Unit is Adj + Ctl), Qubit), output_ty=Unit
                          body: block_ty=Unit
                            [0] Semi ty=Unit Call(Local(op), arg_ty=Qubit)
                          adj: block_ty=Unit
                            [0] Semi ty=Unit Call(Functor Adj(Local(op)), arg_ty=Qubit)
                          ctl: block_ty=Unit
                            [0] Semi ty=Unit Call(Functor Ctl(Local(op)), arg_ty=((Qubit)[], Qubit))
                          ctl_adj: block_ty=Unit
                            [0] Semi ty=Unit Call(Functor Ctl(Functor Adj(Local(op))), arg_ty=((Qubit)[], Qubit))
                        callable Main: input_ty=Unit, output_ty=Unit
                          body: block_ty=Unit
                            [0] Local pat_ty=Qubit init_ty=Qubit Call(Item(Item 8 (Package 0)), arg_ty=Unit)
                            [1] Semi ty=Unit Call(ApplyOp<Qubit, AdjCtl>, arg_ty=((Qubit => Unit is Adj + Ctl), Qubit))
                            [2] Semi ty=Unit Call(Item(Item 10 (Package 0)), arg_ty=Qubit)"#]],
    );
}

#[test]
fn mono_generic_with_adj_ctl_specs_in_body() {
    // Generic operation with adjoint + controlled specs.
    let source = indoc! {r#"
                operation DoIt<'T>(x : 'T) : Unit is Adj + Ctl {
                    body ... { }
                    adjoint self;
                    controlled (ctls, ...) { }
                    controlled adjoint self;
                }
                operation Main() : Unit {
                    DoIt(42);
                }
            "#};
    check(
        source,
        &expect![[r#"
            DoIt: generics=1, input=Param<0>, output=Unit
            DoIt<Int>: generics=0, input=Int, output=Unit
            Main: generics=0, input=Unit, output=Unit"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation DoIt(x : 'T0) : Unit is Adj + Ctl {
                body ... {}
                adjoint ... {}
                controlled (ctls, ...) {}
                controlled adjoint (ctls, ...) {}
            }
            operation Main() : Unit {
                DoIt < Int > (42);
            }
            // entry
            Main()

            AFTER:
            operation DoIt(x : 'T0) : Unit is Adj + Ctl {
                body ... {}
                adjoint ... {}
                controlled (ctls, ...) {}
                controlled adjoint (ctls, ...) {}
            }
            operation Main() : Unit {
                DoIt_Int_(42);
            }
            operation DoIt_Int_(x : Int) : Unit is Adj + Ctl {
                body ... {}
                adjoint ... {}
                controlled (ctls, ...) {}
                controlled adjoint (ctls, ...) {}
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_generic_captures_variable() {
    // A closure inside a generic callable captures a variable typed with
    // the generic parameter.
    let source = indoc! {r#"
                operation WithCapture<'T>(x : 'T) : 'T {
                    let captured = x;
                    let f = () -> captured;
                    f()
                }
                operation Main() : Int { WithCapture(42) }
            "#};
    check(
        source,
        &expect![[r#"
            <lambda>: generics=0, input=(Int, Unit), output=Int
            <lambda>: generics=0, input=(Param<0>, Unit), output=Param<0>
            Main: generics=0, input=Unit, output=Int
            WithCapture: generics=1, input=Param<0>, output=Param<0>
            WithCapture<Int>: generics=0, input=Int, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation WithCapture(x : 'T0) : 'T0 {
                let captured : 'T0 = x;
                let f : (Unit -> 'T0) = / * closure item = 3 captures = [captured] * / _lambda_;
                f()
            }
            operation Main() : Int {
                WithCapture < Int > (42)
            }
            function _lambda_(captured : 'T0, ()) : 'T0 {
                captured
            }
            // entry
            Main()

            AFTER:
            operation WithCapture(x : 'T0) : 'T0 {
                let captured : 'T0 = x;
                let f : (Unit -> 'T0) = / * closure item = 3 captures = [captured] * / _lambda_;
                f()
            }
            operation Main() : Int {
                WithCapture_Int_(42)
            }
            function _lambda_(captured : 'T0, ()) : 'T0 {
                captured
            }
            operation WithCapture_Int_(x : Int) : Int {
                let captured : Int = x;
                let f : (Unit -> Int) = / * closure item = 5 captures = [captured] * / _lambda_;
                f()
            }
            function _lambda_(captured : Int, ()) : Int {
                captured
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_generic_array_of_type_param() {
    // Generic callable taking an array of the type parameter.
    let source = indoc! {r#"
                operation First<'T>(arr : 'T[]) : 'T { arr[0] }
                operation Main() : Int { First([10, 20, 30]) }
            "#};
    check(
        source,
        &expect![[r#"
            First: generics=1, input=(Param<0>)[], output=Param<0>
            First<Int>: generics=0, input=(Int)[], output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation First(arr : 'T0[]) : 'T0 {
                arr[0]
            }
            operation Main() : Int {
                First < Int > ([10, 20, 30])
            }
            // entry
            Main()

            AFTER:
            operation First(arr : 'T0[]) : 'T0 {
                arr[0]
            }
            operation Main() : Int {
                First_Int_([10, 20, 30])
            }
            operation First_Int_(arr : Int[]) : Int {
                arr[0]
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_generic_nested_tuple_types() {
    // Generic callable returning a nested tuple containing the type param.
    let source = indoc! {r#"
                operation Nest<'T>(x : 'T) : (('T, Int), Bool) { ((x, 0), true) }
                operation Main() : ((Int, Int), Bool) { Nest(42) }
            "#};
    check(
        source,
        &expect![[r#"
            Main: generics=0, input=Unit, output=((Int, Int), Bool)
            Nest: generics=1, input=Param<0>, output=((Param<0>, Int), Bool)
            Nest<Int>: generics=0, input=Int, output=((Int, Int), Bool)"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Nest(x : 'T0) : (('T0, Int), Bool) {
                ((x, 0), true)
            }
            operation Main() : ((Int, Int), Bool) {
                Nest < Int > (42)
            }
            // entry
            Main()

            AFTER:
            operation Nest(x : 'T0) : (('T0, Int), Bool) {
                ((x, 0), true)
            }
            operation Main() : ((Int, Int), Bool) {
                Nest_Int_(42)
            }
            operation Nest_Int_(x : Int) : ((Int, Int), Bool) {
                ((x, 0), true)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_mutual_recursion_different_types() {
    // Two mutually recursive generic callables with the same type parameter.
    let source = indoc! {r#"
                operation Ping<'T>(x : 'T, n : Int) : 'T {
                    if n <= 0 { x } else { Pong(x, n - 1) }
                }
                operation Pong<'T>(x : 'T, n : Int) : 'T {
                    Ping(x, n)
                }
                operation Main() : Int { Ping(42, 2) }
            "#};
    check(
        source,
        &expect![[r#"
            Main: generics=0, input=Unit, output=Int
            Ping: generics=1, input=(Param<0>, Int), output=Param<0>
            Ping<Int>: generics=0, input=(Int, Int), output=Int
            Pong: generics=1, input=(Param<0>, Int), output=Param<0>
            Pong<Int>: generics=0, input=(Int, Int), output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation Ping(x : 'T0, n : Int) : 'T0 {
                if n <= 0 {
                    x
                } else {
                    Pong < 'T0 > (x, n - 1)
                }

            }
            operation Pong(x : 'T0, n : Int) : 'T0 {
                Ping < 'T0 > (x, n)
            }
            operation Main() : Int {
                Ping < Int > (42, 2)
            }
            // entry
            Main()

            AFTER:
            operation Ping(x : 'T0, n : Int) : 'T0 {
                if n <= 0 {
                    x
                } else {
                    Pong(x, n - 1)
                }

            }
            operation Pong(x : 'T0, n : Int) : 'T0 {
                Ping(x, n)
            }
            operation Main() : Int {
                Ping_Int_(42, 2)
            }
            operation Ping_Int_(x : Int, n : Int) : Int {
                if n <= 0 {
                    x
                } else {
                    Pong_Int_(x, n - 1)
                }

            }
            operation Pong_Int_(x : Int, n : Int) : Int {
                Ping_Int_(x, n)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mono_generic_with_adj_spec_only() {
    // Generic operation with adjoint-only functor specification.
    let source = indoc! {r#"
                operation MyAdj<'T>(x : 'T) : Unit is Adj {
                    body ... { }
                    adjoint self;
                }
                operation Main() : Unit {
                    MyAdj(42);
                    Adjoint MyAdj(42);
                }
            "#};
    check(
        source,
        &expect![[r#"
            Main: generics=0, input=Unit, output=Unit
            MyAdj: generics=1, input=Param<0>, output=Unit
            MyAdj<Int>: generics=0, input=Int, output=Unit"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            operation MyAdj(x : 'T0) : Unit is Adj {
                body ... {}
                adjoint ... {}
            }
            operation Main() : Unit {
                MyAdj < Int > (42);
                Adjoint MyAdj < Int > (42);
            }
            // entry
            Main()

            AFTER:
            operation MyAdj(x : 'T0) : Unit is Adj {
                body ... {}
                adjoint ... {}
            }
            operation Main() : Unit {
                MyAdj_Int_(42);
                Adjoint MyAdj_Int_(42);
            }
            operation MyAdj_Int_(x : Int) : Unit is Adj {
                body ... {}
                adjoint ... {}
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn mutual_recursion_between_generics_specializes_both() {
    // Two mutually recursive generic functions: IsEven<'T> calls IsOdd<'T>
    // and vice versa. Both should be specialized for Int.
    let source = indoc! {r#"
            function IsEven<'T>(n : Int, val : 'T) : Bool {
                if n == 0 { true } else { IsOdd(n - 1, val) }
            }

            function IsOdd<'T>(n : Int, val : 'T) : Bool {
                if n == 0 { false } else { IsEven(n - 1, val) }
            }

            function Main() : Bool {
                IsEven(4, 0)
            }
        "#};
    check(
        source,
        &expect![[r#"
            IsEven: generics=1, input=(Int, Param<0>), output=Bool
            IsEven<Int>: generics=0, input=(Int, Int), output=Bool
            IsOdd: generics=1, input=(Int, Param<0>), output=Bool
            IsOdd<Int>: generics=0, input=(Int, Int), output=Bool
            Main: generics=0, input=Unit, output=Bool"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            function IsEven(n : Int, val : 'T0) : Bool {
                if n == 0 {
                    true
                } else {
                    IsOdd < 'T0 > (n - 1, val)
                }

            }
            function IsOdd(n : Int, val : 'T0) : Bool {
                if n == 0 {
                    false
                } else {
                    IsEven < 'T0 > (n - 1, val)
                }

            }
            function Main() : Bool {
                IsEven < Int > (4, 0)
            }
            // entry
            Main()

            AFTER:
            function IsEven(n : Int, val : 'T0) : Bool {
                if n == 0 {
                    true
                } else {
                    IsOdd(n - 1, val)
                }

            }
            function IsOdd(n : Int, val : 'T0) : Bool {
                if n == 0 {
                    false
                } else {
                    IsEven(n - 1, val)
                }

            }
            function Main() : Bool {
                IsEven_Int_(4, 0)
            }
            function IsEven_Int_(n : Int, val : Int) : Bool {
                if n == 0 {
                    true
                } else {
                    IsOdd_Int_(n - 1, val)
                }

            }
            function IsOdd_Int_(n : Int, val : Int) : Bool {
                if n == 0 {
                    false
                } else {
                    IsEven_Int_(n - 1, val)
                }

            }
            // entry
            Main()
        "#]],
    );
    // Verify PostMono invariants hold (no Ty::Param remaining).
    let _ = crate::test_utils::compile_and_run_pipeline_to(
        source,
        crate::test_utils::PipelineStage::Mono,
    );
}

#[test]
fn deeply_nested_generic_args_specialize_correctly() {
    // Generic callable instantiated with a complex nested type arg:
    // (Int, Double) as the type parameter.
    let source = indoc! {r#"
                function Wrap<'T>(val : 'T) : 'T[] {
                    [val]
                }

                function Main() : (Int, Double)[] {
                    Wrap((1, 2.0))
                }
            "#};
    check(
        source,
        &expect![[r#"
            Main: generics=0, input=Unit, output=((Int, Double))[]
            Wrap: generics=1, input=Param<0>, output=(Param<0>)[]
            Wrap<(Int, Double)>: generics=0, input=(Int, Double), output=((Int, Double))[]"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            function Wrap(val : 'T0) : 'T0[] {
                [val]
            }
            function Main() : (Int, Double)[] {
                Wrap < (Int, Double) > (1, 2.)
            }
            // entry
            Main()

            AFTER:
            function Wrap(val : 'T0) : 'T0[] {
                [val]
            }
            function Main() : (Int, Double)[] {
                Wrap__Int__Double__(1, 2.)
            }
            function Wrap__Int__Double__(val : (Int, Double)) : (Int, Double)[] {
                [val]
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn cross_package_non_intrinsic_generic_specializes() {
    // Enumerated is a non-intrinsic cross-package generic that returns
    // (Int, 'TElement)[] — structurally different output type from
    // Reversed, and internally chains through MappedByIndex.
    let source = indoc! {r#"
                function Main() : (Int, Int)[] {
                    Microsoft.Quantum.Arrays.Enumerated([10, 20, 30])
                }
            "#};
    check(
        source,
        &expect!["Main: generics=0, input=Unit, output=((Int, Int))[]"],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            function Main() : (Int, Int)[] {
                Enumerated < Int > ([10, 20, 30])
            }
            // entry
            Main()

            AFTER:
            function Main() : (Int, Int)[] {
                Enumerated_Int_([10, 20, 30])
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn monomorphize_no_entry_panics() {
    // Compile as a library (no @EntryPoint) so package.entry is None.
    // monomorphize should panic because it requires an entry expression.
    use qsc_data_structures::{
        language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
    };
    use qsc_frontend::compile as frontend_compile;
    use qsc_hir::hir::PackageId as HirPackageId;
    use qsc_passes::{PackageType, lower_hir_to_fir, run_core_passes, run_default_passes};

    let mut core_unit = frontend_compile::core();
    let core_errors = run_core_passes(&mut core_unit);
    assert!(core_errors.is_empty());
    let mut hir_store = frontend_compile::PackageStore::new(core_unit);

    let mut std_unit = frontend_compile::std(&hir_store, TargetCapabilityFlags::empty());
    let std_errors = run_default_passes(hir_store.core(), &mut std_unit, PackageType::Lib);
    assert!(std_errors.is_empty());
    hir_store.insert(std_unit);

    let std_id = HirPackageId::CORE.successor();
    let sources = SourceMap::new(
        vec![(
            "lib.qs".into(),
            "function Helper<'T>(x : 'T) : 'T { x }".into(),
        )],
        None,
    );
    let mut unit = frontend_compile::compile(
        &hir_store,
        &[(HirPackageId::CORE, None), (std_id, None)],
        sources,
        TargetCapabilityFlags::empty(),
        LanguageFeatures::default(),
    );
    crate::test_utils::assert_no_compile_errors("user code", &unit.errors);
    let pass_errors = run_default_passes(hir_store.core(), &mut unit, PackageType::Lib);
    assert!(pass_errors.is_empty());
    let hir_pkg_id = hir_store.insert(unit);
    let (mut fir_store, fir_pkg_id, _) = lower_hir_to_fir(&hir_store, hir_pkg_id);

    assert!(fir_store.get(fir_pkg_id).entry.is_none());

    let mut assigners = crate::package_assigners::PackageAssigners::entry(&fir_store, fir_pkg_id);
    crate::test_utils::assert_panics_with("package must have an entry expression", || {
        monomorphize(&mut fir_store, fir_pkg_id, &mut assigners);
    });
}

#[test]
fn mono_preserves_simulatable_intrinsic_impl() {
    // A generic @SimulatableIntrinsic callable should, after monomorphization,
    // produce a specialization that retains the SimulatableIntrinsic variant.
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(indoc! {r#"
            @SimulatableIntrinsic()
            operation MySimIntrinsic<'T>(x : 'T) : 'T { x }
            operation Main() : Int { MySimIntrinsic(42) }
        "#});
    let mut assigners = crate::package_assigners::PackageAssigners::entry(&store, pkg_id);
    monomorphize(&mut store, pkg_id, &mut assigners);

    let package = store.get(pkg_id);
    let mut found_specialized = false;
    for (_, item) in &package.items {
        if let ItemKind::Callable(decl) = &item.kind
            && decl.name.name.as_ref() == "MySimIntrinsic<Int>"
        {
            assert!(
                matches!(decl.implementation, CallableImpl::SimulatableIntrinsic(_)),
                "specialized callable should preserve SimulatableIntrinsic variant"
            );
            assert!(
                decl.generics.is_empty(),
                "specialized callable should have no generic params"
            );
            found_specialized = true;
        }
    }
    assert!(
        found_specialized,
        "should find a specialized MySimIntrinsic<Int> callable"
    );
}

#[test]
fn monomorphize_is_idempotent() {
    let source = indoc! {r#"
            operation Identity<'T>(x : 'T) : 'T { x }
            operation Main() : Int { Identity(42) }
        "#};
    let (mut store, pkg_id) = crate::test_utils::compile_and_run_pipeline_to(
        source,
        crate::test_utils::PipelineStage::Mono,
    );
    let first = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigners = crate::package_assigners::PackageAssigners::entry(&store, pkg_id);
    monomorphize(&mut store, pkg_id, &mut assigners);
    let second = crate::pretty::write_package_qsharp(&store, pkg_id);
    assert_eq!(first, second, "monomorphize should be idempotent");
}

fn render_before_after_mono(source: &str) -> (String, String) {
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(source);
    let before = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    let mut assigners = crate::package_assigners::PackageAssigners::entry(&store, pkg_id);
    monomorphize(&mut store, pkg_id, &mut assigners);
    let after = crate::pretty::write_package_qsharp_parseable(&store, pkg_id);
    (before, after)
}

fn check_before_after(source: &str, expect: &Expect) {
    let (before, after) = render_before_after_mono(source);
    expect.assert_eq(&format!("BEFORE:\n{before}\nAFTER:\n{after}"));
}

#[test]
fn before_after_generic_specialization() {
    check_before_after(
        indoc! {r#"
            operation Identity<'T>(x : 'T) : 'T { x }
            operation Main() : Int { Identity(42) }
        "#},
        &expect![[r#"
            BEFORE:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Main() : Int {
                Identity < Int > (42)
            }
            // entry
            Main()

            AFTER:
            operation Identity(x : 'T0) : 'T0 {
                x
            }
            operation Main() : Int {
                Identity_Int_(42)
            }
            operation Identity_Int_(x : Int) : Int {
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn shared_input_and_arrow_generic_param_specializes() {
    check_before_after(
        indoc! {r#"
            function double<'T: Add>(x : 'T) : 'T { x + x }
            function doDouble<'T>(a : 'T, doubler : ('T -> 'T)) : 'T { doubler(a) }
            operation Main() : Unit {
                use q = Qubit();
                if M(q) == One {
                    doDouble(3, double);
                } else {
                    doDouble(3.0, double);
                }
            }
        "#},
        &expect![[r#"
            BEFORE:
            function double(x : 'T0) : 'T0 {
                x + x
            }
            function doDouble(a : 'T0, doubler : ('T0 -> 'T0)) : 'T0 {
                doubler(a)
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_64 : Unit = if M(q) == One {
                    doDouble < Int > (3, double < Int >);
                } else {
                    doDouble < Double > (3., double < Double >);
                };
                __quantum__rt__qubit_release(q);
                _generated_ident_64
            }
            // entry
            Main()

            AFTER:
            function double(x : 'T0) : 'T0 {
                x + x
            }
            function doDouble(a : 'T0, doubler : ('T0 -> 'T0)) : 'T0 {
                doubler(a)
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_64 : Unit = if M(q) == One {
                    doDouble_Int_(3, double_Int_);
                } else {
                    doDouble_Double_(3., double_Double_);
                };
                __quantum__rt__qubit_release(q);
                _generated_ident_64
            }
            function doDouble_Int_(a : Int, doubler : (Int -> Int)) : Int {
                doubler(a)
            }
            function double_Int_(x : Int) : Int {
                x + x
            }
            function doDouble_Double_(a : Double, doubler : (Double -> Double)) : Double {
                doubler(a)
            }
            function double_Double_(x : Double) : Double {
                x + x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn unreachable_generic_call_site_not_specialized() {
    // Monomorphize only processes reachable callables.
    // The dead callable's generic call with a different type arg
    // never generates a specialization. Verify that only the reachable
    // Int specialization is produced.
    let source = indoc! {"
            namespace Test {
                @EntryPoint()
                function Main() : Int {
                    Identity(42)
                }
                function Identity<'T>(x : 'T) : 'T { x }
            }
        "};
    check(
        source,
        &expect![[r#"
            Identity: generics=1, input=Param<0>, output=Param<0>
            Identity<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
    check_before_after(
        source,
        &expect![[r#"
            BEFORE:
            function Main() : Int {
                Identity < Int > (42)
            }
            function Identity(x : 'T0) : 'T0 {
                x
            }
            // entry
            Main()

            AFTER:
            function Main() : Int {
                Identity_Int_(42)
            }
            function Identity(x : 'T0) : 'T0 {
                x
            }
            function Identity_Int_(x : Int) : Int {
                x
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn cross_package_generic_function_monomorphized() {
    let lib_source = indoc! {"
        namespace TestLib {
            function Identity<'T>(x: 'T) : 'T { x }
            function Pair<'T, 'U>(a: 'T, b: 'U) : ('T, 'U) { (a, b) }
            export Identity, Pair;
        }
    "};

    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        operation Main() : (Int, (Bool, Double)) {
            let x = Identity(42);
            let p = Pair(true, 3.14);
            (x, p)
        }
    "};

    crate::test_utils::check_semantic_equivalence_with_library(lib_source, user_source);
}

#[test]
fn cross_package_generic_specialization_lives_in_library_package() {
    // A generic library callable instantiated from the user package is
    // monomorphized in place into the library (owning) package rather than
    // cloned into the user package.
    let lib_source = indoc! {"
        namespace TestLib {
            function Identity<'T>(x : 'T) : 'T { x }
            export Identity;
        }
    "};
    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Int {
            Identity(42)
        }
    "};

    let (mut store, user_pkg_id) =
        crate::test_utils::compile_to_fir_with_library(lib_source, user_source);
    let mut assigners = crate::package_assigners::PackageAssigners::entry(&store, user_pkg_id);
    monomorphize(&mut store, user_pkg_id, &mut assigners);

    let spec_name = "Identity<Int>";
    let reachable = crate::reachability::collect_reachable_from_entry(&store, user_pkg_id);
    let owning_pkg_id = reachable
        .iter()
        .find(|store_id| {
            matches!(
                &store.get(store_id.package).get_item(store_id.item).kind,
                ItemKind::Callable(decl) if decl.name.name.as_ref() == spec_name
            )
        })
        .map(|store_id| store_id.package)
        .expect("Identity<Int> specialization should be reachable");

    assert_ne!(
        owning_pkg_id, user_pkg_id,
        "specialization must not be cloned into the user package"
    );

    // The owning (library) package render must contain the monomorphized
    // specialization, confirming it was emitted in place there.
    let lib_render = crate::pretty::write_package_qsharp(&store, owning_pkg_id);
    assert!(
        lib_render.contains("Identity<Int>"),
        "library package should contain the monomorphized Identity specialization:\n{lib_render}"
    );
}

#[test]
fn no_generic_callable_reachable_after_full_pipeline() {
    // After the full pipeline, monomorphization must leave no generic
    // callable reachable in any package. The original generics in both the user
    // and the library package become entry-unreachable once their call sites are
    // redirected to the concrete specializations, and item DCE prunes them.
    // Running the pipeline also exercises the in-pass `assert_no_reachable_generic`
    // guard; this test re-verifies the property across the whole reachable
    // closure.
    let lib = indoc! {"
        namespace Lib {
            function LibId<'T>(x : 'T) : 'T { x }
            operation LibUsed(q : Qubit) : Int { X(q); LibId(7) }
            export LibUsed;
        }
    "};
    let user = indoc! {"
        import Lib.*;
        function UserId<'T>(x : 'T) : 'T { x }
        @EntryPoint()
        operation Main() : Int {
            use q = Qubit();
            let a = LibUsed(q);
            a + UserId(42)
        }
    "};

    let (store, user_pkg_id) = crate::test_utils::compile_and_run_pipeline_to_with_library(
        lib,
        user,
        crate::test_utils::PipelineStage::Full,
    );

    let reachable = crate::reachability::collect_reachable_from_entry(&store, user_pkg_id);
    for store_item_id in &reachable {
        let package = store.get(store_item_id.package);
        if let Some(item) = package.items.get(store_item_id.item)
            && let ItemKind::Callable(decl) = &item.kind
        {
            assert!(
                decl.generics.is_empty(),
                "generic callable `{}` is reachable after the full pipeline",
                decl.name.name
            );
        }
    }
}
