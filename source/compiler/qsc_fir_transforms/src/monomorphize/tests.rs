// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::{Expect, expect};
use indoc::indoc;
use qsc_fir::assigner::Assigner;
use qsc_fir::fir::NodeId;
use rustc_hash::FxHashSet;

/// Compiles Q# source, runs monomorphization, and snapshots all callables
/// in the user package showing name, generic-param count, input type, and
/// output type. Sorted for determinism.
fn check(source: &str, expect: &Expect) {
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(source);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    monomorphize(&mut store, pkg_id, &mut assigner);

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

/// Compiles Q# source, runs monomorphization, and asserts no
/// `ExprKind::Var` in the user package still carries generic args.
fn assert_no_generic_args(source: &str) {
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(source);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    monomorphize(&mut store, pkg_id, &mut assigner);

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

#[test]
fn mono_explicit_entry_expression_rewritten() {
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir_with_entry(
        indoc! {r#"
                namespace Test {
                    function Identity<'T>(x : 'T) : 'T { x }
                }
            "#},
        "Test.Identity(42)",
    );
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    monomorphize(&mut store, pkg_id, &mut assigner);

    let package = store.get(pkg_id);
    let entry_id = package
        .entry
        .expect("package should have an entry expression");
    let entry_expr = package.get_expr(entry_id);
    let ExprKind::Call(callee_id, _) = entry_expr.kind else {
        panic!("entry expression should remain a call")
    };
    let callee_expr = package.get_expr(callee_id);
    let ExprKind::Var(Res::Item(item_id), ref generic_args) = callee_expr.kind else {
        panic!("entry callee should be a callable reference")
    };

    assert!(
        generic_args.is_empty(),
        "entry-expression callee should not retain generic args after monomorphization"
    );

    let item = package.get_item(item_id.item);
    let ItemKind::Callable(decl) = &item.kind else {
        panic!("entry callee should resolve to a callable item")
    };
    assert_eq!(decl.name.name.as_ref(), "Identity<Int>");
}

#[test]
fn mono_identity_int() {
    check(
        indoc! {r#"
                operation Identity<'T>(x : 'T) : 'T { x }
                operation Main() : Int { Identity(42) }
            "#},
        &expect![[r#"
            Identity: generics=1, input=Param<0>, output=Param<0>
            Identity<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
}

#[test]
fn mono_identity_qubit() {
    check(
        indoc! {r#"
                operation Identity<'T>(x : 'T) : 'T { x }
                operation Main() : Unit {
                    use q = Qubit();
                    let _ = Identity(q);
                }
            "#},
        &expect![[r#"
            Identity: generics=1, input=Param<0>, output=Param<0>
            Identity<Qubit>: generics=0, input=Qubit, output=Qubit
            Main: generics=0, input=Unit, output=Unit"#]],
    );
}

#[test]
fn mono_two_instantiations() {
    check(
        indoc! {r#"
                operation Identity<'T>(x : 'T) : 'T { x }
                operation Main() : Unit {
                    let _ = Identity(42);
                    use q = Qubit();
                    let _ = Identity(q);
                }
            "#},
        &expect![[r#"
            Identity: generics=1, input=Param<0>, output=Param<0>
            Identity<Int>: generics=0, input=Int, output=Int
            Identity<Qubit>: generics=0, input=Qubit, output=Qubit
            Main: generics=0, input=Unit, output=Unit"#]],
    );
}

#[test]
fn mono_no_generic_args() {
    check(
        "operation Main() : Int { 42 }",
        &expect!["Main: generics=0, input=Unit, output=Int"],
    );
}

#[test]
fn mono_multiple_call_sites_same_args() {
    // Two call sites with Identity<Int> should produce only one
    // specialization.
    check(
        indoc! {r#"
                operation Identity<'T>(x : 'T) : 'T { x }
                operation Main() : Unit {
                    let _ = Identity(1);
                    let _ = Identity(2);
                }
            "#},
        &expect![[r#"
            Identity: generics=1, input=Param<0>, output=Param<0>
            Identity<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Unit"#]],
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
    check(
        indoc! {r#"
                operation Identity<'T>(x : 'T) : 'T { x }
                operation Outer<'T>(x : 'T) : 'T { Identity(x) }
                operation Main() : Int { Outer(42) }
            "#},
        &expect![[r#"
            Identity: generics=1, input=Param<0>, output=Param<0>
            Identity<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int
            Outer: generics=1, input=Param<0>, output=Param<0>
            Outer<Int>: generics=0, input=Int, output=Int"#]],
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

    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(source);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    monomorphize(&mut store, pkg_id, &mut assigner);
    let reachable = crate::reachability::collect_reachable_from_entry(&store, pkg_id);
    let package = store.get(pkg_id);
    let offenders = package
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
            let functor_param = matches!(input_ty, qsc_fir::ty::Ty::Arrow(arrow) if matches!(arrow.functors, qsc_fir::ty::FunctorSet::Param(_)));

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
        .collect::<Vec<_>>();
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
    check(
        indoc! {r#"
                operation C<'T>(x : 'T) : 'T { x }
                operation B<'T>(x : 'T) : 'T { C(x) }
                operation A<'T>(x : 'T) : 'T { B(x) }
                operation Main() : Int { A(42) }
            "#},
        &expect![[r#"
            A: generics=1, input=Param<0>, output=Param<0>
            A<Int>: generics=0, input=Int, output=Int
            B: generics=1, input=Param<0>, output=Param<0>
            B<Int>: generics=0, input=Int, output=Int
            C: generics=1, input=Param<0>, output=Param<0>
            C<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
}

#[test]
fn mono_nested_diamond() {
    // Diamond: A calls B and C, both call D.
    // D should be specialized only once.
    check(
        indoc! {r#"
                operation D<'T>(x : 'T) : 'T { x }
                operation B<'T>(x : 'T) : 'T { D(x) }
                operation C<'T>(x : 'T) : 'T { D(x) }
                operation A<'T>(x : 'T) : 'T {
                    let _ = B(x);
                    C(x)
                }
                operation Main() : Int { A(42) }
            "#},
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
}

#[test]
fn mono_arrow_param() {
    // Generic callable with arrow-typed parameter.
    check(
        indoc! {r#"
                operation ApplyOp<'T>(f : 'T => 'T, x : 'T) : 'T { f(x) }
                operation DoubleInt(x : Int) : Int { x * 2 }
                operation Main() : Int { ApplyOp(DoubleInt, 5) }
            "#},
        &expect![[r#"
            ApplyOp: generics=2, input=((Param<0> => Param<0> is 1), Param<0>), output=Param<0>
            ApplyOp<Int, Empty>: generics=0, input=((Int => Int), Int), output=Int
            DoubleInt: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
}

#[test]
fn mono_generic_with_body_locals() {
    check(
        indoc! {r#"
                operation Transform<'T>(x : 'T) : 'T {
                    let tmp = x;
                    tmp
                }
                operation Main() : Int { Transform(42) }
            "#},
        &expect![[r#"
            Main: generics=0, input=Unit, output=Int
            Transform: generics=1, input=Param<0>, output=Param<0>
            Transform<Int>: generics=0, input=Int, output=Int"#]],
    );
}

#[test]
fn mono_generic_preserves_local_chain() {
    // Multiple local bindings chained together.
    check(
        indoc! {r#"
                operation Chain<'T>(x : 'T) : 'T {
                    let a = x;
                    let b = a;
                    let c = b;
                    let d = c;
                    d
                }
                operation Main() : Int { Chain(42) }
            "#},
        &expect![[r#"
            Chain: generics=1, input=Param<0>, output=Param<0>
            Chain<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
}

#[test]
fn mono_generic_with_ctl_spec() {
    check(
        indoc! {r#"
                operation ApplyCtl<'T>(x : 'T) : Unit is Ctl {
                    body ... { }
                    controlled (ctls, ...) { }
                }
                operation Main() : Unit {
                    use q = Qubit();
                    ApplyCtl(42);
                }
            "#},
        &expect![[r#"
            ApplyCtl: generics=1, input=Param<0>, output=Unit
            ApplyCtl<Int>: generics=0, input=Int, output=Unit
            Main: generics=0, input=Unit, output=Unit"#]],
    );
}

#[test]
fn mono_closure_in_generic() {
    check(
        indoc! {r#"
                operation WithClosure<'T>(x : 'T) : 'T {
                    let f = (y) -> y;
                    f(x)
                }
                operation Main() : Int { WithClosure(42) }
            "#},
        &expect![[r#"
            <lambda>: generics=0, input=(Int,), output=Int
            <lambda>: generics=0, input=(Param<0>,), output=Param<0>
            Main: generics=0, input=Unit, output=Int
            WithClosure: generics=1, input=Param<0>, output=Param<0>
            WithClosure<Int>: generics=0, input=Int, output=Int"#]],
    );
}

#[test]
fn mono_cross_package_length() {
    // Length is a cross-package intrinsic generic callable in std.
    check(
        indoc! {r#"
                operation Main() : Int {
                    let arr = [1, 2, 3];
                    Length(arr)
                }
            "#},
        &expect![[r#"
                Length: generics=0, input=(Int)[], output=Int
                Main: generics=0, input=Unit, output=Int"#]],
    );
}

#[test]
fn mono_cross_package_reversed() {
    // Reversed is a cross-package generic callable.
    check(
        indoc! {r#"
                operation Main() : Int[] {
                    let arr = [1, 2, 3];
                    Microsoft.Quantum.Arrays.Reversed(arr)
                }
            "#},
        &expect![[r#"
            Main: generics=0, input=Unit, output=(Int)[]
            Reversed<Int>: generics=0, input=(Int)[], output=(Int)[]"#]],
    );
}

#[test]
fn mono_cross_package_with_same_name() {
    // Generic function uses same name as a cross-package generic callable.
    check(
        indoc! {r#"
                function Reversed<'T>(array : 'T[]) : 'T[] {
                    Microsoft.Quantum.Arrays.Reversed(array)
                }
                operation Main() : Int[] {
                    let arr = [1, 2, 3];
                    Reversed(arr)
                }
            "#},
        &expect![[r#"
            Main: generics=0, input=Unit, output=(Int)[]
            Reversed: generics=1, input=(Param<0>)[], output=(Param<0>)[]
            Reversed<Int>: generics=0, input=(Int)[], output=(Int)[]
            Reversed<Int>: generics=0, input=(Int)[], output=(Int)[]"#]],
    );
}

#[test]
fn mono_identity_instantiation_not_duplicated() {
    // When Outer<'T> calls Inner<'T>, the Inner<Param(0)> reference is
    // an identity instantiation. Only concrete instantiations (from the
    // entry) should produce specializations.
    check(
        indoc! {r#"
                operation Inner<'T>(x : 'T) : 'T { x }
                operation Outer<'T>(x : 'T) : 'T { Inner(x) }
                operation Main() : Int { Outer(42) }
            "#},
        &expect![[r#"
            Inner: generics=1, input=Param<0>, output=Param<0>
            Inner<Int>: generics=0, input=Int, output=Int
            Main: generics=0, input=Unit, output=Int
            Outer: generics=1, input=Param<0>, output=Param<0>
            Outer<Int>: generics=0, input=Int, output=Int"#]],
    );
}

#[test]
fn mono_two_type_params() {
    check(
        indoc! {r#"
                operation Pair<'A, 'B>(a : 'A, b : 'B) : 'A { a }
                operation Main() : Int {
                    use q = Qubit();
                    Pair(42, q)
                }
            "#},
        &expect![[r#"
            Main: generics=0, input=Unit, output=Int
            Pair: generics=2, input=(Param<0>, Param<1>), output=Param<0>
            Pair<Int, Qubit>: generics=0, input=(Int, Qubit), output=Int"#]],
    );
}

#[test]
fn mono_specialized_callable_node_ids_do_not_collide_with_spec_nodes() {
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(indoc! {r#"
            operation ApplyCtl<'T>(x : 'T) : Unit is Ctl {
                body ... { }
                controlled (ctls, ...) { }
            }
            operation Main() : Unit {
                ApplyCtl(42);
            }
        "#});
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    monomorphize(&mut store, pkg_id, &mut assigner);

    let package = store.get(pkg_id);
    let mut seen = FxHashSet::default();
    for item in package.items.values() {
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        assert_node_id_is_unique(decl.id, &mut seen);
        match &decl.implementation {
            CallableImpl::Spec(spec_impl) => {
                assert_node_id_is_unique(spec_impl.body.id, &mut seen);
                for spec in [&spec_impl.adj, &spec_impl.ctl, &spec_impl.ctl_adj]
                    .into_iter()
                    .flatten()
                {
                    assert_node_id_is_unique(spec.id, &mut seen);
                }
            }
            CallableImpl::SimulatableIntrinsic(spec) => {
                assert_node_id_is_unique(spec.id, &mut seen);
            }
            CallableImpl::Intrinsic => {}
        }
    }
}

#[test]
#[should_panic(
    expected = "Non-intrinsic same-package callable has no monomorphized specialization"
)]
fn mono_missing_same_package_specialization_panics() {
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(indoc! {r#"
            function Identity<'T>(x : 'T) : 'T { x }
            function Main() : Int { Identity(42) }
        "#});

    rewrite_call_sites(store.get_mut(pkg_id), pkg_id, &[]);
}

fn assert_node_id_is_unique(node_id: NodeId, seen: &mut FxHashSet<u32>) {
    assert!(
        seen.insert(u32::from(node_id)),
        "NodeId {node_id:?} should be unique after monomorphization"
    );
}

#[test]
fn mono_recursive_generic() {
    // Recursive generic callable — self-references should be rewritten
    // to point at the specialized clone.
    check(
        indoc! {r#"
                operation Repeat<'T>(x : 'T, n : Int) : 'T {
                    if n <= 0 {
                        x
                    } else {
                        Repeat(x, n - 1)
                    }
                }
                operation Main() : Int { Repeat(42, 3) }
            "#},
        &expect![[r#"
            Main: generics=0, input=Unit, output=Int
            Repeat: generics=1, input=(Param<0>, Int), output=Param<0>
            Repeat<Int>: generics=0, input=(Int, Int), output=Int"#]],
    );
}

#[test]
fn mono_invariants_hold_post_pass() {
    let (store, pkg_id) = crate::test_utils::compile_and_run_pipeline_to(
        indoc! {r#"
                operation Identity<'T>(x : 'T) : 'T { x }
                operation Outer<'T>(x : 'T) : 'T { Identity(x) }
                operation Main() : Int { Outer(42) }
            "#},
        crate::test_utils::PipelineStage::Mono,
    );
    // If we reach here, the invariant check inside
    // compile_and_run_pipeline_to already passed.
    let _ = (store, pkg_id);
}

#[test]
fn mono_generic_with_simulatable_intrinsic() {
    // A generic function used via a simulatable intrinsic path.
    // Length is a cross-package intrinsic: verify it's specialized.
    check(
        indoc! {r#"
                operation Wrap<'T>(arr : 'T[]) : Int { Length(arr) }
                operation Main() : Int {
                    Wrap([1, 2, 3])
                }
            "#},
        &expect![[r#"
            Length: generics=0, input=(Int)[], output=Int
            Main: generics=0, input=Unit, output=Int
            Wrap: generics=1, input=(Param<0>)[], output=Int
            Wrap<Int>: generics=0, input=(Int)[], output=Int"#]],
    );
}

#[test]
fn mono_generic_with_functor_param() {
    // Generic callable with a functor-parameterized operation parameter.
    check(
        indoc! {r#"
                operation RunOp<'T>(op : 'T => Unit, x : 'T) : Unit { op(x) }
                operation NoOp(x : Int) : Unit {}
                operation Main() : Unit { RunOp(NoOp, 42) }
            "#},
        &expect![[r#"
            Main: generics=0, input=Unit, output=Unit
            NoOp: generics=0, input=Int, output=Unit
            RunOp: generics=2, input=((Param<0> => Unit is 1), Param<0>), output=Unit
            RunOp<Int, Empty>: generics=0, input=((Int => Unit), Int), output=Unit"#]],
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
    check(
        indoc! {r#"
                operation DoIt<'T>(x : 'T) : Unit is Adj + Ctl {
                    body ... { }
                    adjoint self;
                    controlled (ctls, ...) { }
                    controlled adjoint self;
                }
                operation Main() : Unit {
                    DoIt(42);
                }
            "#},
        &expect![[r#"
            DoIt: generics=1, input=Param<0>, output=Unit
            DoIt<Int>: generics=0, input=Int, output=Unit
            Main: generics=0, input=Unit, output=Unit"#]],
    );
}

#[test]
fn mono_generic_captures_variable() {
    // A closure inside a generic callable captures a variable typed with
    // the generic parameter.
    check(
        indoc! {r#"
                operation WithCapture<'T>(x : 'T) : 'T {
                    let captured = x;
                    let f = () -> captured;
                    f()
                }
                operation Main() : Int { WithCapture(42) }
            "#},
        &expect![[r#"
            <lambda>: generics=0, input=(Int, Unit), output=Int
            <lambda>: generics=0, input=(Param<0>, Unit), output=Param<0>
            Main: generics=0, input=Unit, output=Int
            WithCapture: generics=1, input=Param<0>, output=Param<0>
            WithCapture<Int>: generics=0, input=Int, output=Int"#]],
    );
}

#[test]
fn mono_generic_array_of_type_param() {
    // Generic callable taking an array of the type parameter.
    check(
        indoc! {r#"
                operation First<'T>(arr : 'T[]) : 'T { arr[0] }
                operation Main() : Int { First([10, 20, 30]) }
            "#},
        &expect![[r#"
            First: generics=1, input=(Param<0>)[], output=Param<0>
            First<Int>: generics=0, input=(Int)[], output=Int
            Main: generics=0, input=Unit, output=Int"#]],
    );
}

#[test]
fn mono_generic_nested_tuple_types() {
    // Generic callable returning a nested tuple containing the type param.
    check(
        indoc! {r#"
                operation Nest<'T>(x : 'T) : (('T, Int), Bool) { ((x, 0), true) }
                operation Main() : ((Int, Int), Bool) { Nest(42) }
            "#},
        &expect![[r#"
            Main: generics=0, input=Unit, output=((Int, Int), Bool)
            Nest: generics=1, input=Param<0>, output=((Param<0>, Int), Bool)
            Nest<Int>: generics=0, input=Int, output=((Int, Int), Bool)"#]],
    );
}

#[test]
fn mono_mutual_recursion_different_types() {
    // Two mutually recursive generic callables with the same type parameter.
    check(
        indoc! {r#"
                operation Ping<'T>(x : 'T, n : Int) : 'T {
                    if n <= 0 { x } else { Pong(x, n - 1) }
                }
                operation Pong<'T>(x : 'T, n : Int) : 'T {
                    Ping(x, n)
                }
                operation Main() : Int { Ping(42, 2) }
            "#},
        &expect![[r#"
            Main: generics=0, input=Unit, output=Int
            Ping: generics=1, input=(Param<0>, Int), output=Param<0>
            Ping<Int>: generics=0, input=(Int, Int), output=Int
            Pong: generics=1, input=(Param<0>, Int), output=Param<0>
            Pong<Int>: generics=0, input=(Int, Int), output=Int"#]],
    );
}

#[test]
fn mono_generic_with_adj_spec_only() {
    // Generic operation with adjoint-only functor specification.
    check(
        indoc! {r#"
                operation MyAdj<'T>(x : 'T) : Unit is Adj {
                    body ... { }
                    adjoint self;
                }
                operation Main() : Unit {
                    MyAdj(42);
                    Adjoint MyAdj(42);
                }
            "#},
        &expect![[r#"
            Main: generics=0, input=Unit, output=Unit
            MyAdj: generics=1, input=Param<0>, output=Unit
            MyAdj<Int>: generics=0, input=Int, output=Unit"#]],
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
    check(
        indoc! {r#"
                function Wrap<'T>(val : 'T) : 'T[] {
                    [val]
                }

                function Main() : (Int, Double)[] {
                    Wrap((1, 2.0))
                }
            "#},
        &expect![[r#"
            Main: generics=0, input=Unit, output=((Int, Double))[]
            Wrap: generics=1, input=Param<0>, output=(Param<0>)[]
            Wrap<(Int, Double)>: generics=0, input=(Int, Double), output=((Int, Double))[]"#]],
    );
}

#[test]
fn cross_package_non_intrinsic_generic_specializes() {
    // Enumerated is a non-intrinsic cross-package generic that returns
    // (Int, 'TElement)[] — structurally different output type from
    // Reversed, and internally chains through MappedByIndex.
    check(
        indoc! {r#"
                function Main() : (Int, Int)[] {
                    Microsoft.Quantum.Arrays.Enumerated([10, 20, 30])
                }
            "#},
        &expect![[r#"
            <lambda>: generics=0, input=((Int, Int),), output=(Int, Int)
            Enumerated<Int>: generics=0, input=(Int)[], output=((Int, Int))[]
            Length: generics=0, input=(Int)[], output=Int
            Main: generics=0, input=Unit, output=((Int, Int))[]
            MappedByIndex<Int, (Int, Int)>: generics=0, input=(((Int, Int) -> (Int, Int)), (Int)[]), output=((Int, Int))[]"#]],
    );
}

#[test]
fn monomorphize_no_entry_returns_immediately() {
    // Compile as a library (no @EntryPoint) so package.entry is None.
    // monomorphize should return immediately, leaving generics untouched.
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

    // Confirm entry is None before calling monomorphize.
    assert!(fir_store.get(fir_pkg_id).entry.is_none());

    // Capture item count before.
    let items_before: Vec<_> = fir_store
        .get(fir_pkg_id)
        .items
        .iter()
        .map(|(id, _)| id)
        .collect();

    let mut assigner = Assigner::from_package(fir_store.get(fir_pkg_id));
    monomorphize(&mut fir_store, fir_pkg_id, &mut assigner);

    // Item count must be unchanged — no specializations were created.
    let items_after: Vec<_> = fir_store
        .get(fir_pkg_id)
        .items
        .iter()
        .map(|(id, _)| id)
        .collect();
    assert_eq!(items_before, items_after);

    // The generic function should still be generic.
    let package = fir_store.get(fir_pkg_id);
    for (_, item) in &package.items {
        if let ItemKind::Callable(decl) = &item.kind
            && decl.name.name.as_ref() == "Helper"
        {
            assert!(
                !decl.generics.is_empty(),
                "Helper should still be generic after no-op monomorphize"
            );
        }
    }
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
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    monomorphize(&mut store, pkg_id, &mut assigner);

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
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    monomorphize(&mut store, pkg_id, &mut assigner);
    let second = crate::pretty::write_package_qsharp(&store, pkg_id);
    assert_eq!(first, second, "monomorphize should be idempotent");
}

fn render_before_after_mono(source: &str) -> (String, String) {
    let (mut store, pkg_id) = crate::test_utils::compile_to_fir(source);
    let before = crate::pretty::write_package_qsharp(&store, pkg_id);
    let mut assigner = Assigner::from_package(store.get(pkg_id));
    monomorphize(&mut store, pkg_id, &mut assigner);
    let after = crate::pretty::write_package_qsharp(&store, pkg_id);
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
            // namespace test
            operation Identity<''T > x : 'T0 : 'T0 {
                body {
                    x
                }
            }
            operation Main() : Int {
                body {
                    Identity < Int > (42)
                }
            }
            // entry
            Main()

            AFTER:
            // namespace test
            operation Identity<''T > x : 'T0 : 'T0 {
                body {
                    x
                }
            }
            operation Main() : Int {
                body {
                    Identity < Int > (42)
                }
            }
            operation Identity<Int> x : Int : Int {
                body {
                    x
                }
            }
            // entry
            Main()
        "#]], // snapshot populated by UPDATE_EXPECT=1
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
            // namespace test
            function double<''T > x : 'T0 : 'T0 {
                body {
                    q + q
                }
            }
            function doDouble<''T > (a : 'T0, doubler : ('T0 -> 'T0)) : 'T0 {
                body {
                    @generated_ident_64(q)
                }
            }
            operation Main() : Unit {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    let
                    @generated_ident_64 : Unit = if M(q) == One {
                        doDouble < Int > (3, double < Int >);
                    } else {
                        doDouble < Double > (3., double < Double >);
                    };
                    __quantum__rt__qubit_release(q);
                    @generated_ident_64
                }
            }
            // entry
            Main()

            AFTER:
            // namespace test
            function double<''T > x : 'T0 : 'T0 {
                body {
                    doubler + doubler
                }
            }
            function doDouble<''T > (a : 'T0, doubler : ('T0 -> 'T0)) : 'T0 {
                body {
                    @generated_ident_64(doubler)
                }
            }
            operation Main() : Unit {
                body {
                    let q : Qubit = __quantum__rt__qubit_allocate();
                    let
                    @generated_ident_64 : Unit = if M(doubler) == One {
                        doDouble < Int > (3, double < Int >);
                    } else {
                        doDouble < Double > (3., double < Double >);
                    };
                    __quantum__rt__qubit_release(doubler);
                    @generated_ident_64
                }
            }
            function Lengtha : Qubit[] : Int {
                body intrinsic;
            }
            function Lengtha : Pauli[] : Int {
                body intrinsic;
            }
            function doDouble<Int>(a : Int, doubler : (Int -> Int)) : Int {
                body {
                    doubler(x)
                }
            }
            function double<Int> x : Int : Int {
                body {
                    x + x
                }
            }
            function doDouble<Double>(a : Double, doubler : (Double -> Double)) : Double {
                body {
                    doubler(x)
                }
            }
            function double<Double> x : Double : Double {
                body {
                    x + x
                }
            }
            // entry
            Main()
        "#]],
    );
}
