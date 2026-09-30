// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use super::check;

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "Keeps the labelled source/validator parity matrix in one contract test."
)]
fn concrete_instantiation_matches_source_constraints() {
    use crate::{
        compile::{self, PackageStore},
        typeck::validate_instantiation,
    };
    use miette::Diagnostic;
    use qsc_data_structures::{
        language_features::LanguageFeatures, source::SourceMap, span::Span,
        target::TargetCapabilityFlags,
    };
    use qsc_hir::{
        hir::{ItemId, ItemKind},
        ty::{FunctorSet, GenericArg, ParamId, Ty, TypeParameter},
    };
    use rustc_hash::FxHashMap;

    let store = PackageStore::new(compile::core());
    let compile = |source: &str, expr: &str| {
        compile::compile(
            &store,
            &[],
            SourceMap::new([("test".into(), source.into())], Some(expr.into())),
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
        )
    };
    let prefix = r#"
        namespace Test {
            newtype Wrapper = (Double, Double);
            newtype Complex = (Double, Double);
            operation Plain() : Unit {}
            operation AdjOnly() : Unit is Adj {}
            operation CtlOnly() : Unit is Ctl {}
            operation Both() : Unit is Adj + Ctl {}
            function Helper<'T: Add>(x: 'T) : 'T { x }
    "#;
    let identity = |bounds: &str| format!("function Target<'T: {bounds}>(x: 'T) : Unit {{}}");
    let cases = [
        ("bool eq/show", identity("Eq + Show"), "true", vec![]),
        (
            "bool add",
            identity("Add"),
            "true",
            vec![("MissingClassAdd", "type Bool does not support plus")],
        ),
        ("int add", identity("Add"), "1", vec![]),
        ("bigint add", identity("Add"), "1L", vec![]),
        ("double add", identity("Add"), "1.0", vec![]),
        ("string add", identity("Add"), "\"s\"", vec![]),
        (
            "array add needs no element class",
            identity("Add"),
            "[Test.Plain]",
            vec![],
        ),
        (
            "nominal complex add",
            identity("Add"),
            "new Std.Core.Complex { Real = 1.0, Imag = 2.0 }",
            vec![],
        ),
        (
            "complex-shaped tuple is not complex",
            identity("Add"),
            "(1.0, 2.0)",
            vec![(
                "MissingClassAdd",
                "type (Double, Double) does not support plus",
            )],
        ),
        (
            "same-named nominal is not complex",
            identity("Add"),
            "Test.Complex(1.0, 2.0)",
            vec![("MissingClassAdd", "type Complex does not support plus")],
        ),
        (
            "other nominal is not complex",
            identity("Add"),
            "Test.Wrapper(1.0, 2.0)",
            vec![("MissingClassAdd", "type Wrapper does not support plus")],
        ),
        (
            "nominal complex eq",
            identity("Eq"),
            "new Std.Core.Complex { Real = 1.0, Imag = 2.0 }",
            vec![],
        ),
        (
            "nominal complex show",
            identity("Show"),
            "new Std.Core.Complex { Real = 1.0, Imag = 2.0 }",
            vec![(
                "MissingClassShow",
                "type Complex cannot be converted into a string",
            )],
        ),
        (
            "recursive eq/show",
            identity("Eq + Show"),
            "([true], (1, \"s\"))",
            vec![],
        ),
        (
            "recursive eq failure",
            identity("Eq"),
            "([Test.Plain], true)",
            vec![(
                "MissingClassEq",
                "type (Unit => Unit) does not support equality",
            )],
        ),
        (
            "recursive show failure",
            identity("Show"),
            "(true, [Test.Plain])",
            vec![(
                "MissingClassShow",
                "type (Unit => Unit) cannot be converted into a string",
            )],
        ),
        ("multiple bounds", identity("Add + Integral"), "1", vec![]),
        (
            "multiple bounds failure",
            identity("Add + Integral"),
            "\"s\"",
            vec![("MissingClassInteger", "type String is not an integer")],
        ),
        (
            "numeric class conjunction",
            identity("Sub + Mul + Div + Mod + Signed"),
            "1",
            vec![],
        ),
        (
            "all bounds checked",
            identity("Add + Integral"),
            "true",
            vec![
                ("MissingClassAdd", "type Bool does not support plus"),
                ("MissingClassInteger", "type Bool is not an integer"),
            ],
        ),
        (
            "transitive bounds",
            "function Target<'T: Add + Show>(x: 'T) : 'T { Helper(x) }".into(),
            "1",
            vec![],
        ),
        (
            "transitive bounds failure",
            "function Target<'T: Add + Show>(x: 'T) : 'T { Helper(x) }".into(),
            "true",
            vec![("MissingClassAdd", "type Bool does not support plus")],
        ),
        ("bigint exponent", identity("Exp[Int]"), "1L", vec![]),
        ("double exponent", identity("Exp[Double]"), "1.0", vec![]),
        (
            "wrong exponent",
            identity("Exp[Int]"),
            "1.0",
            vec![("TyMismatch", "expected Double, found Int")],
        ),
        (
            "unsupported exponent base",
            identity("Exp[Int]"),
            "true",
            vec![(
                "MissingClassExp",
                "type Bool does not support exponentiation",
            )],
        ),
        (
            "iterable element",
            identity("Iterable[Bool]"),
            "[true]",
            vec![],
        ),
        ("iterable range", identity("Iterable[Int]"), "0..2", vec![]),
        (
            "wrong iterable element",
            identity("Iterable[Bool]"),
            "[1]",
            vec![("TyMismatch", "expected Int, found Bool")],
        ),
        (
            "non iterable",
            identity("Iterable[Bool]"),
            "true",
            vec![("MissingClassIterable", "type Bool is not iterable")],
        ),
        (
            "functor superset",
            "function Target(op: (Unit => Unit is Adj)) : Unit {}".into(),
            "Test.Both",
            vec![],
        ),
        (
            "exact functor",
            "function Target(op: (Unit => Unit is Adj)) : Unit {}".into(),
            "Test.AdjOnly",
            vec![],
        ),
        (
            "missing functor",
            "function Target(op: (Unit => Unit is Adj)) : Unit {}".into(),
            "Test.Plain",
            vec![(
                "MissingFunctor",
                "expected superset of Adj, found empty set",
            )],
        ),
        (
            "wrong functor",
            "function Target(op: (Unit => Unit is Adj)) : Unit {}".into(),
            "Test.CtlOnly",
            vec![("MissingFunctor", "expected superset of Adj, found Ctl")],
        ),
    ];
    for (label, declaration, expression, expected) in cases {
        let source = format!("{prefix}{declaration}}}");
        let unit = compile(&source, expression);
        assert!(unit.errors.is_empty(), "{label}: {:?}", unit.errors);
        let target = unit
            .package
            .items
            .iter()
            .find_map(|(_, item)| match &item.kind {
                ItemKind::Callable(callable) if callable.name.name.as_ref() == "Target" => {
                    Some(callable)
                }
                _ => None,
            })
            .expect("compiled target");
        let parameters: Vec<_> = target
            .generics
            .iter()
            .cloned()
            .enumerate()
            .map(|(id, parameter)| (ParamId::from(id), parameter))
            .collect();
        assert_eq!(parameters.len(), 1, "{label}");
        let candidate_ty = &unit
            .package
            .entry
            .as_ref()
            .expect("candidate expression")
            .ty;
        let candidate = match &parameters[0].1 {
            TypeParameter::Ty { .. } => GenericArg::Ty(candidate_ty.clone()),
            TypeParameter::Functor(_) => {
                let Ty::Arrow(arrow) = candidate_ty else {
                    panic!("{label}: expected arrow")
                };
                GenericArg::Functor(*arrow.functors.borrow())
            }
        };
        let candidates = FxHashMap::from_iter([(parameters[0].0, candidate)]);
        let udts: FxHashMap<_, _> = store
            .iter()
            .map(|(id, unit)| (id, &unit.package))
            .chain([(unit.package.package_id, &unit.package)])
            .flat_map(|(package, unit)| {
                unit.items.iter().filter_map(move |(item_id, item)| {
                    if let ItemKind::Ty(_, udt) = &item.kind {
                        Some((
                            ItemId {
                                package,
                                item: item_id,
                            },
                            udt.clone(),
                        ))
                    } else {
                        None
                    }
                })
            })
            .collect();
        let span = unit.package.entry.as_ref().expect("entry").span;
        let errors = validate_instantiation(&parameters, &candidates, &udts, span)
            .err()
            .unwrap_or_default();
        if label == "nominal complex add" {
            let missing =
                validate_instantiation(&parameters, &candidates, &FxHashMap::default(), span)
                    .expect_err("nominal identity requires its owning definition");
            assert_eq!(missing.len(), 1);
            assert_eq!(
                missing[0].code().expect("code").to_string(),
                "Qdk.Qsc.TypeCk.MissingTypeDefinition"
            );
            assert_eq!(
                missing[0].to_string(),
                format!("missing definition for nominal type {}", ItemId::complex())
            );
        }
        let actual: Vec<_> = errors
            .iter()
            .map(|error| {
                assert_eq!(error.parameter, Some(parameters[0].0), "{label}");
                let labels: Vec<_> = error.labels().expect("diagnostic location").collect();
                assert_eq!(labels.len(), 1, "{label}");
                assert_eq!(labels[0].offset(), span.lo as usize, "{label}");
                assert_eq!(labels[0].len(), (span.hi - span.lo) as usize, "{label}");
                (
                    error.code().expect("diagnostic code").to_string(),
                    error.to_string(),
                )
            })
            .collect();
        let expected: Vec<_> = expected
            .iter()
            .map(|(code, message)| (format!("Qdk.Qsc.TypeCk.{code}"), (*message).to_string()))
            .collect();
        assert_eq!(actual, expected, "{label}: shared validator");
        let source_call = compile(&source, &format!("Test.Target({expression})"));
        let source_errors: Vec<_> = source_call
            .errors
            .iter()
            .map(|error| {
                (
                    error.code().expect("source diagnostic code").to_string(),
                    std::error::Error::source(error)
                        .expect("type diagnostic")
                        .to_string(),
                )
            })
            .collect();
        assert_eq!(source_errors, expected, "{label}: source call");
    }

    // Dependent payloads are compared to source calls with the same concrete bound.
    // Source generic inference does not substitute parameters inside class payloads.
    for (label, bound, concrete_bound, expression, expected) in [
        ("dependent exponent", "Exp['P]", "Exp[Int]", "(2, 1L)", None),
        (
            "dependent exponent mismatch",
            "Exp['P]",
            "Exp[Bool]",
            "(true, 1L)",
            Some("expected Int, found Bool"),
        ),
        (
            "dependent iterable",
            "Iterable['P]",
            "Iterable[Bool]",
            "(true, [true])",
            None,
        ),
        (
            "dependent iterable mismatch",
            "Iterable['P]",
            "Iterable[Bool]",
            "(true, [1])",
            Some("expected Int, found Bool"),
        ),
    ] {
        let declaration = |bound| {
            format!("{prefix}function Target<'P, 'T: {bound}>(p: 'P, x: 'T) : Unit {{}} }}")
        };
        let unit = compile(&declaration(bound), expression);
        assert!(unit.errors.is_empty(), "{label}: {:?}", unit.errors);
        let target = unit
            .package
            .items
            .iter()
            .find_map(|(_, item)| match &item.kind {
                ItemKind::Callable(callable) if callable.name.name.as_ref() == "Target" => {
                    Some(callable)
                }
                _ => None,
            })
            .expect("compiled target");
        let parameters: Vec<_> = target
            .generics
            .iter()
            .cloned()
            .enumerate()
            .map(|(id, parameter)| (ParamId::from(id), parameter))
            .collect();
        let Ty::Tuple(types) = &unit.package.entry.as_ref().expect("candidate").ty else {
            panic!("candidate tuple")
        };
        let candidates = types
            .iter()
            .cloned()
            .enumerate()
            .map(|(id, ty)| (ParamId::from(id), GenericArg::Ty(ty)))
            .collect();
        let errors = validate_instantiation(
            &parameters,
            &candidates,
            &FxHashMap::default(),
            Span::default(),
        )
        .err()
        .unwrap_or_default();
        assert_eq!(
            errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
            expected.into_iter().collect::<Vec<_>>(),
            "{label}"
        );
        for error in &errors {
            assert_eq!(error.parameter, Some(ParamId::from(1)));
            assert_eq!(
                error.code().expect("code").to_string(),
                "Qdk.Qsc.TypeCk.TyMismatch"
            );
        }
        let source_call = compile(
            &declaration(concrete_bound),
            &format!("Test.Target{expression}"),
        );
        assert_eq!(
            source_call
                .errors
                .iter()
                .map(|error| std::error::Error::source(error)
                    .expect("type diagnostic")
                    .to_string())
                .collect::<Vec<_>>(),
            expected.into_iter().collect::<Vec<_>>(),
            "{label}: concrete source bound"
        );
    }

    let source = format!("{prefix}{}}}", identity("Eq"));
    let unit = compile(&source, "true");
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let target = unit
        .package
        .items
        .iter()
        .find_map(|(_, item)| match &item.kind {
            ItemKind::Callable(callable) if callable.name.name.as_ref() == "Target" => {
                Some(callable)
            }
            _ => None,
        })
        .expect("compiled target");
    let id = ParamId::from(7);
    let parameters = [(id, target.generics[0].clone())];
    let candidate = GenericArg::Ty(unit.package.entry.as_ref().expect("candidate").ty.clone());
    let candidates = FxHashMap::from_iter([(id, candidate.clone())]);
    assert!(
        validate_instantiation(
            &parameters,
            &candidates,
            &FxHashMap::default(),
            Span::default()
        )
        .is_ok(),
        "sparse inherited IDs do not need local declaration slots"
    );
    let arrow_unit = compile(&source, "Test.Plain");
    assert!(arrow_unit.errors.is_empty(), "{:?}", arrow_unit.errors);
    let arrow_ty = arrow_unit.package.entry.expect("candidate arrow").ty;
    let inherited_failure = validate_instantiation(
        &parameters,
        &FxHashMap::from_iter([(id, GenericArg::Ty(arrow_ty.clone()))]),
        &FxHashMap::default(),
        Span::default(),
    )
    .expect_err("inherited bounds are checked");
    assert_eq!(inherited_failure.len(), 1);
    assert_eq!(inherited_failure[0].parameter, Some(id));
    assert_eq!(
        inherited_failure[0].code().expect("code").to_string(),
        "Qdk.Qsc.TypeCk.MissingClassEq"
    );
    assert_eq!(
        inherited_failure[0].to_string(),
        "type (Unit => Unit) does not support equality"
    );
    let Ty::Arrow(arrow) = &arrow_ty else {
        panic!("candidate arrow")
    };
    let unknown_functors = FunctorSet::Infer(Default::default());
    *arrow.functors.borrow_mut() = unknown_functors;
    let incomplete = validate_instantiation(
        &parameters,
        &FxHashMap::from_iter([(id, GenericArg::Ty(arrow_ty))]),
        &FxHashMap::default(),
        Span::default(),
    )
    .expect_err("nested unresolved functors are not concrete");
    assert_eq!(incomplete.len(), 1);
    assert_eq!(
        incomplete[0].code().expect("code").to_string(),
        "Qdk.Qsc.TypeCk.NonConcreteFunctor"
    );
    assert_eq!(
        incomplete[0].to_string(),
        format!("expected concrete functors, found {unknown_functors}")
    );
    for (label, parameters, candidates, expected) in [
        (
            "arity",
            parameters.to_vec(),
            FxHashMap::default(),
            vec![
                "expected 1 generic arguments, found 0",
                "no candidate was supplied for generic parameter 7",
            ],
        ),
        (
            "missing ID",
            parameters.to_vec(),
            FxHashMap::from_iter([(ParamId::from(8), candidate.clone())]),
            vec!["no candidate was supplied for generic parameter 7"],
        ),
        (
            "duplicate ID",
            vec![parameters[0].clone(), parameters[0].clone()],
            FxHashMap::from_iter([
                (id, candidate.clone()),
                (ParamId::from(8), candidate.clone()),
            ]),
            vec!["generic parameter 7 is declared more than once"],
        ),
        (
            "wrong kind",
            parameters.to_vec(),
            FxHashMap::from_iter([(
                id,
                GenericArg::Functor(FunctorSet::Value(Default::default())),
            )]),
            vec!["generic argument has the wrong kind for parameter 7"],
        ),
        (
            "unresolved parameter",
            parameters.to_vec(),
            FxHashMap::from_iter([(id, GenericArg::Ty(target.input.ty.clone()))]),
            vec!["expected a concrete type, found 'T"],
        ),
        (
            "unresolved inference",
            parameters.to_vec(),
            FxHashMap::from_iter([(id, GenericArg::Ty(Ty::Infer(Default::default())))]),
            vec!["expected a concrete type, found ?"],
        ),
        (
            "error type",
            parameters.to_vec(),
            FxHashMap::from_iter([(id, GenericArg::Ty(Ty::Err))]),
            vec!["expected a concrete type, found ?"],
        ),
        (
            "nested error type",
            parameters.to_vec(),
            FxHashMap::from_iter([(id, GenericArg::Ty(Ty::Array(Box::new(Ty::Err))))]),
            vec!["expected a concrete type, found ?"],
        ),
    ] {
        let errors = validate_instantiation(
            &parameters,
            &candidates,
            &FxHashMap::default(),
            Span::default(),
        )
        .expect_err(label);
        assert_eq!(
            errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
            expected,
            "{label}"
        );
    }

    let unit = compile(
        &format!("{prefix}function Target(op: (Unit => Unit is Adj)) : Unit {{}} }}"),
        "Test.Both",
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let target = unit
        .package
        .items
        .iter()
        .find_map(|(_, item)| match &item.kind {
            ItemKind::Callable(callable) if callable.name.name.as_ref() == "Target" => {
                Some(callable)
            }
            _ => None,
        })
        .expect("compiled target");
    let parameters = [(id, target.generics[0].clone())];
    for functors in [
        FunctorSet::Infer(Default::default()),
        match &target.input.ty {
            Ty::Arrow(arrow) => *arrow.functors.borrow(),
            _ => panic!("operation parameter"),
        },
    ] {
        let candidates = FxHashMap::from_iter([(id, GenericArg::Functor(functors))]);
        let errors = validate_instantiation(
            &parameters,
            &candidates,
            &FxHashMap::default(),
            Span::default(),
        )
        .expect_err("unresolved functor");
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].code().expect("code").to_string(),
            "Qdk.Qsc.TypeCk.NonConcreteFunctor"
        );
        assert_eq!(
            errors[0].to_string(),
            format!("expected concrete functors, found {functors}")
        );
    }

    let unit = compile(
        &format!("{prefix}function Target<'P, 'T: Exp['P]>(p: 'P, x: 'T) : Unit {{}} }}"),
        "1L",
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let target = unit
        .package
        .items
        .iter()
        .find_map(|(_, item)| match &item.kind {
            ItemKind::Callable(callable) if callable.name.name.as_ref() == "Target" => {
                Some(callable)
            }
            _ => None,
        })
        .expect("compiled target");
    let parameters = [(ParamId::from(1), target.generics[1].clone())];
    let candidates = FxHashMap::from_iter([(
        ParamId::from(1),
        GenericArg::Ty(unit.package.entry.expect("candidate").ty),
    )]);
    let errors = validate_instantiation(
        &parameters,
        &candidates,
        &FxHashMap::default(),
        Span::default(),
    )
    .expect_err("missing dependent payload candidate");
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0].code().expect("code").to_string(),
        "Qdk.Qsc.TypeCk.NonConcreteType"
    );
    assert_eq!(errors[0].to_string(), "expected a concrete type, found 'P");
}

#[test]
fn eq() {
    check(
        r#"
        namespace A {
            function Foo<'T: Eq>(a: 'T, b: 'T) : Bool {
                a == b
            }
        }
        "#,
        "",
        &expect![[r##"
            #8 55-69 "(a: 'T, b: 'T)" : (Param<"'T": 0>, Param<"'T": 0>)
            #9 56-61 "a: 'T" : Param<"'T": 0>
            #13 63-68 "b: 'T" : Param<"'T": 0>
            #20 77-115 "{\n                a == b\n            }" : Bool
            #22 95-101 "a == b" : Bool
            #23 95-96 "a" : Param<"'T": 0>
            #26 100-101 "b" : Param<"'T": 0>
        "##]],
    );
}

#[test]
fn exp() {
    check(
        r#"
        namespace A {
            function Foo<'T: Exp[Int]>(a: 'T, b: Int) : 'T {
                a ^ b
            }
        }
        "#,
        "",
        &expect![[r##"
            #11 61-76 "(a: 'T, b: Int)" : (Param<"'T": 0>, Int)
            #12 62-67 "a: 'T" : Param<"'T": 0>
            #16 69-75 "b: Int" : Int
            #23 82-119 "{\n                a ^ b\n            }" : Param<"'T": 0>
            #25 100-105 "a ^ b" : Param<"'T": 0>
            #26 100-101 "a" : Param<"'T": 0>
            #29 104-105 "b" : Int
        "##]],
    );
}

#[test]
fn exp_fail() {
    check(
        r#"
        namespace A {
            function Foo<'T: Exp[Int]>(a: 'T, b: Bool) : 'T {
                a ^ b
            }
        }
        "#,
        "",
        &expect![[r##"
            #11 61-77 "(a: 'T, b: Bool)" : (Param<"'T": 0>, Bool)
            #12 62-67 "a: 'T" : Param<"'T": 0>
            #16 69-76 "b: Bool" : Bool
            #23 83-120 "{\n                a ^ b\n            }" : Param<"'T": 0>
            #25 101-106 "a ^ b" : Param<"'T": 0>
            #26 101-102 "a" : Param<"'T": 0>
            #29 105-106 "b" : Bool
            Error(Type(Error(TyMismatch(TyInfo { kind: Prim(Int), display: "Int" }, TyInfo { kind: Prim(Bool), display: "Bool" }, Span { lo: 101, hi: 106 }))))
        "##]],
    );
}
#[test]
fn extra_arg_to_exp() {
    check(
        r#"
        namespace A {
            function Foo<'E, 'T: Exp['E, Int]>(a: 'E, b: Int) : 'T {
                a ^ b
            }
            function Foo2<'E, 'T: Exp>(a: 'E, b: Int) : 'T {
                a ^ b
            }
        }
        "#,
        "",
        &expect![[r##"
            #14 69-84 "(a: 'E, b: Int)" : (Param<"'E": 0>, Int)
            #15 70-75 "a: 'E" : Param<"'E": 0>
            #19 77-83 "b: Int" : Int
            #26 90-127 "{\n                a ^ b\n            }" : Param<"'E": 0>
            #28 108-113 "a ^ b" : Param<"'E": 0>
            #29 108-109 "a" : Param<"'E": 0>
            #32 112-113 "b" : Int
            #41 166-181 "(a: 'E, b: Int)" : (Param<"'E": 0>, Int)
            #42 167-172 "a: 'E" : Param<"'E": 0>
            #46 174-180 "b: Int" : Int
            #53 187-224 "{\n                a ^ b\n            }" : Param<"'E": 0>
            #55 205-210 "a ^ b" : Param<"'E": 0>
            #56 205-206 "a" : Param<"'E": 0>
            #59 209-210 "b" : Int
            Error(Type(Error(IncorrectNumberOfConstraintParameters { expected: 1, found: 2, span: Span { lo: 56, hi: 59 } })))
            Error(Type(Error(IncorrectNumberOfConstraintParameters { expected: 1, found: 2, span: Span { lo: 56, hi: 59 } })))
            Error(Type(Error(IncorrectNumberOfConstraintParameters { expected: 1, found: 0, span: Span { lo: 162, hi: 165 } })))
            Error(Type(Error(IncorrectNumberOfConstraintParameters { expected: 1, found: 0, span: Span { lo: 162, hi: 165 } })))
            Error(Type(Error(MissingClassExp("'E", Span { lo: 108, hi: 113 }))))
            Error(Type(Error(TyMismatch(TyInfo { kind: Param, display: "'T" }, TyInfo { kind: Param, display: "'E" }, Span { lo: 108, hi: 113 }))))
            Error(Type(Error(MissingClassExp("'E", Span { lo: 205, hi: 210 }))))
            Error(Type(Error(TyMismatch(TyInfo { kind: Param, display: "'T" }, TyInfo { kind: Param, display: "'E" }, Span { lo: 205, hi: 210 }))))
        "##]],
    );
}

#[test]
fn example_should_fail() {
    check(
        r#"
        namespace A {
            function Foo<'T: Eq, 'O: Eq>(a: 'T, b: 'O) : Bool {
            // should fail because we can't compare two different types
                a == b
            }
        }
        "#,
        "",
        &expect![[r##"
            #10 63-77 "(a: 'T, b: 'O)" : (Param<"'T": 0>, Param<"'O": 1>)
            #11 64-69 "a: 'T" : Param<"'T": 0>
            #15 71-76 "b: 'O" : Param<"'O": 1>
            #22 85-195 "{\n            // should fail because we can't compare two different types\n                a == b\n            }" : Bool
            #24 175-181 "a == b" : Bool
            #25 175-176 "a" : Param<"'T": 0>
            #28 180-181 "b" : Param<"'O": 1>
            Error(Type(Error(TyMismatch(TyInfo { kind: Param, display: "'T" }, TyInfo { kind: Param, display: "'O" }, Span { lo: 180, hi: 181 }))))
        "##]],
    );
}

// This test ensures that we show a pretty error for polymorphism bounds that are not supported
// yet.
#[test]
fn iter() {
    check(
        r#"
        namespace A {
            function Foo<'T: Iterable[Bool]>(a: 'T) : Bool {
                for item in a {
                    return item;
                }
                false
            }

            function Main() : Unit {
                let x = Foo([true]);
            }
        }
        "#,
        "",
        &expect![[r##"
            #11 67-74 "(a: 'T)" : Param<"'T": 0>
            #12 68-73 "a: 'T" : Param<"'T": 0>
            #19 82-202 "{\n                for item in a {\n                    return item;\n                }\n                false\n            }" : Bool
            #21 100-166 "for item in a {\n                    return item;\n                }" : Unit
            #22 104-108 "item" : Bool
            #24 112-113 "a" : Param<"'T": 0>
            #27 114-166 "{\n                    return item;\n                }" : Unit
            #29 136-147 "return item" : Unit
            #30 143-147 "item" : Bool
            #34 183-188 "false" : Bool
            #38 229-231 "()" : Unit
            #42 239-291 "{\n                let x = Foo([true]);\n            }" : Unit
            #44 261-262 "x" : Bool
            #46 265-276 "Foo([true])" : Bool
            #47 265-268 "Foo" : (Bool[] -> Bool)
            #50 268-276 "([true])" : Bool[]
            #51 269-275 "[true]" : Bool[]
            #52 270-274 "true" : Bool
            Error(Type(Error(UnrecognizedClass { span: Span { lo: 112, hi: 113 }, name: "Iterable" })))
        "##]],
    );
}

#[test]
fn signed() {
    check(
        r#"
        namespace A {
            function Foo<'T: Signed>(a: 'T) : 'T {
                -a
            }

            function Main() : Unit {
                let x: Int = Foo(1);
                let y: Double = Foo(1.0);
                let z: BigInt = Foo(10L);
            }
        }
        "#,
        "",
        &expect![[r##"
            #8 59-66 "(a: 'T)" : Param<"'T": 0>
            #9 60-65 "a: 'T" : Param<"'T": 0>
            #15 72-106 "{\n                -a\n            }" : Param<"'T": 0>
            #17 90-92 "-a" : Param<"'T": 0>
            #18 91-92 "a" : Param<"'T": 0>
            #24 133-135 "()" : Unit
            #28 143-279 "{\n                let x: Int = Foo(1);\n                let y: Double = Foo(1.0);\n                let z: BigInt = Foo(10L);\n            }" : Unit
            #30 165-171 "x: Int" : Int
            #35 174-180 "Foo(1)" : Int
            #36 174-177 "Foo" : (Int -> Int)
            #39 177-180 "(1)" : Int
            #40 178-179 "1" : Int
            #42 202-211 "y: Double" : Double
            #47 214-222 "Foo(1.0)" : Double
            #48 214-217 "Foo" : (Double -> Double)
            #51 217-222 "(1.0)" : Double
            #52 218-221 "1.0" : Double
            #54 244-253 "z: BigInt" : BigInt
            #59 256-264 "Foo(10L)" : BigInt
            #60 256-259 "Foo" : (BigInt -> BigInt)
            #63 259-264 "(10L)" : BigInt
            #64 260-263 "10L" : BigInt
        "##]],
    );
}

#[test]
fn signed_fail() {
    check(
        r#"
        namespace A {
            function Foo<'T: Eq>(a: 'T) : 'T {
                -a
            }

            function Main() : Unit {
                let x: Int = Foo(1);
                let y: Double = Foo(1.0);
                let z: BigInt = Foo(10L);
            }
        }
        "#,
        "",
        &expect![[r##"
            #8 55-62 "(a: 'T)" : Param<"'T": 0>
            #9 56-61 "a: 'T" : Param<"'T": 0>
            #15 68-102 "{\n                -a\n            }" : Param<"'T": 0>
            #17 86-88 "-a" : Param<"'T": 0>
            #18 87-88 "a" : Param<"'T": 0>
            #24 129-131 "()" : Unit
            #28 139-275 "{\n                let x: Int = Foo(1);\n                let y: Double = Foo(1.0);\n                let z: BigInt = Foo(10L);\n            }" : Unit
            #30 161-167 "x: Int" : Int
            #35 170-176 "Foo(1)" : Int
            #36 170-173 "Foo" : (Int -> Int)
            #39 173-176 "(1)" : Int
            #40 174-175 "1" : Int
            #42 198-207 "y: Double" : Double
            #47 210-218 "Foo(1.0)" : Double
            #48 210-213 "Foo" : (Double -> Double)
            #51 213-218 "(1.0)" : Double
            #52 214-217 "1.0" : Double
            #54 240-249 "z: BigInt" : BigInt
            #59 252-260 "Foo(10L)" : BigInt
            #60 252-255 "Foo" : (BigInt -> BigInt)
            #63 255-260 "(10L)" : BigInt
            #64 256-259 "10L" : BigInt
            Error(Type(Error(MissingClassSigned("'T", Span { lo: 87, hi: 88 }))))
        "##]],
    );
}

#[test]
fn transitive_class_check() {
    check(
        r#"
        namespace A {
            function Foo<'T: Mul>(a: 'T) : 'T {
                a * a
            }

            function Bar<'F: Mul>(a: 'F) : 'F {
                Foo(a)
            }

            function Main() : Unit {
                let x: Int = Bar(1);
                let y: Double = Bar(1.0);
                let z: BigInt = Bar(10L);
            }
        }
        "#,
        "",
        &expect![[r##"
            #8 56-63 "(a: 'T)" : Param<"'T": 0>
            #9 57-62 "a: 'T" : Param<"'T": 0>
            #15 69-106 "{\n                a * a\n            }" : Param<"'T": 0>
            #17 87-92 "a * a" : Param<"'T": 0>
            #18 87-88 "a" : Param<"'T": 0>
            #21 91-92 "a" : Param<"'T": 0>
            #29 141-148 "(a: 'F)" : Param<"'F": 0>
            #30 142-147 "a: 'F" : Param<"'F": 0>
            #36 154-192 "{\n                Foo(a)\n            }" : Param<"'F": 0>
            #38 172-178 "Foo(a)" : Param<"'F": 0>
            #39 172-175 "Foo" : (Param<"'F": 0> -> Param<"'F": 0>)
            #42 175-178 "(a)" : Param<"'F": 0>
            #43 176-177 "a" : Param<"'F": 0>
            #49 219-221 "()" : Unit
            #53 229-365 "{\n                let x: Int = Bar(1);\n                let y: Double = Bar(1.0);\n                let z: BigInt = Bar(10L);\n            }" : Unit
            #55 251-257 "x: Int" : Int
            #60 260-266 "Bar(1)" : Int
            #61 260-263 "Bar" : (Int -> Int)
            #64 263-266 "(1)" : Int
            #65 264-265 "1" : Int
            #67 288-297 "y: Double" : Double
            #72 300-308 "Bar(1.0)" : Double
            #73 300-303 "Bar" : (Double -> Double)
            #76 303-308 "(1.0)" : Double
            #77 304-307 "1.0" : Double
            #79 330-339 "z: BigInt" : BigInt
            #84 342-350 "Bar(10L)" : BigInt
            #85 342-345 "Bar" : (BigInt -> BigInt)
            #88 345-350 "(10L)" : BigInt
            #89 346-349 "10L" : BigInt
        "##]],
    );
}

#[test]
fn transitive_class_check_fail() {
    check(
        r#"
        namespace A {
            function Foo<'T: Integral>(a: 'T) : 'T {
                a
            }

            function Bar<'F>(a: 'F) : 'F {
                // below should be an error as 'F has no
                // Integral bound
                Foo(a)
            }

            function Main() : Unit {
                let x: Int = Foo(1);
                // below should be an error as it is a double and not an integral type
                let y: Double = Foo(1.0);
                let z: BigInt = Foo(10L);
            }
        }
        "#,
        "",
        &expect![[r##"
            #8 61-68 "(a: 'T)" : Param<"'T": 0>
            #9 62-67 "a: 'T" : Param<"'T": 0>
            #15 74-107 "{\n                a\n            }" : Param<"'T": 0>
            #17 92-93 "a" : Param<"'T": 0>
            #24 137-144 "(a: 'F)" : Param<"'F": 0>
            #25 138-143 "a: 'F" : Param<"'F": 0>
            #31 150-279 "{\n                // below should be an error as 'F has no\n                // Integral bound\n                Foo(a)\n            }" : Param<"'F": 0>
            #33 259-265 "Foo(a)" : Param<"'F": 0>
            #34 259-262 "Foo" : (Param<"'F": 0> -> Param<"'F": 0>)
            #37 262-265 "(a)" : Param<"'F": 0>
            #38 263-264 "a" : Param<"'F": 0>
            #44 306-308 "()" : Unit
            #48 316-539 "{\n                let x: Int = Foo(1);\n                // below should be an error as it is a double and not an integral type\n                let y: Double = Foo(1.0);\n                let z: BigInt = Foo(10L);\n            }" : Unit
            #50 338-344 "x: Int" : Int
            #55 347-353 "Foo(1)" : Int
            #56 347-350 "Foo" : (Int -> Int)
            #59 350-353 "(1)" : Int
            #60 351-352 "1" : Int
            #62 462-471 "y: Double" : Double
            #67 474-482 "Foo(1.0)" : Double
            #68 474-477 "Foo" : (Double -> Double)
            #71 477-482 "(1.0)" : Double
            #72 478-481 "1.0" : Double
            #74 504-513 "z: BigInt" : BigInt
            #79 516-524 "Foo(10L)" : BigInt
            #80 516-519 "Foo" : (BigInt -> BigInt)
            #83 519-524 "(10L)" : BigInt
            #84 520-523 "10L" : BigInt
            Error(Type(Error(MissingClassInteger("'F", Span { lo: 263, hi: 264 }))))
            Error(Type(Error(MissingClassInteger("Double", Span { lo: 478, hi: 481 }))))
        "##]],
    );
}

#[test]
fn transitive_class_check_superset() {
    check(
        r#"
        namespace A {
            function Foo<'T: Sub>(a: 'T) : 'T {
                a - a
            }

            function Bar<'F: Sub + Eq>(a: 'F) : 'F {
                Foo(a)
            }

            function Main() : Unit {
                let x: Int = Bar(1);
                let y: Double = Bar(1.0);
                let z: BigInt = Bar(10L);
            }
        }
        "#,
        "",
        &expect![[r##"
            #8 56-63 "(a: 'T)" : Param<"'T": 0>
            #9 57-62 "a: 'T" : Param<"'T": 0>
            #15 69-106 "{\n                a - a\n            }" : Param<"'T": 0>
            #17 87-92 "a - a" : Param<"'T": 0>
            #18 87-88 "a" : Param<"'T": 0>
            #21 91-92 "a" : Param<"'T": 0>
            #30 146-153 "(a: 'F)" : Param<"'F": 0>
            #31 147-152 "a: 'F" : Param<"'F": 0>
            #37 159-197 "{\n                Foo(a)\n            }" : Param<"'F": 0>
            #39 177-183 "Foo(a)" : Param<"'F": 0>
            #40 177-180 "Foo" : (Param<"'F": 0> -> Param<"'F": 0>)
            #43 180-183 "(a)" : Param<"'F": 0>
            #44 181-182 "a" : Param<"'F": 0>
            #50 224-226 "()" : Unit
            #54 234-370 "{\n                let x: Int = Bar(1);\n                let y: Double = Bar(1.0);\n                let z: BigInt = Bar(10L);\n            }" : Unit
            #56 256-262 "x: Int" : Int
            #61 265-271 "Bar(1)" : Int
            #62 265-268 "Bar" : (Int -> Int)
            #65 268-271 "(1)" : Int
            #66 269-270 "1" : Int
            #68 293-302 "y: Double" : Double
            #73 305-313 "Bar(1.0)" : Double
            #74 305-308 "Bar" : (Double -> Double)
            #77 308-313 "(1.0)" : Double
            #78 309-312 "1.0" : Double
            #80 335-344 "z: BigInt" : BigInt
            #85 347-355 "Bar(10L)" : BigInt
            #86 347-350 "Bar" : (BigInt -> BigInt)
            #89 350-355 "(10L)" : BigInt
            #90 351-354 "10L" : BigInt
        "##]],
    );
}
#[test]
fn show() {
    check(
        r#"
        namespace A {
            function Foo<'T: Show>(a: 'T) : String {
                let x = $"Value: {a}";
                x
            }

            function Main() : Unit {
                let x: String = Foo(1);
                let y: String = Foo(1.0);
                let z: String = Foo(true);
            }
        }
        "#,
        "",
        &expect![[r##"
            #8 57-64 "(a: 'T)" : Param<"'T": 0>
            #9 58-63 "a: 'T" : Param<"'T": 0>
            #16 74-146 "{\n                let x = $\"Value: {a}\";\n                x\n            }" : String
            #18 96-97 "x" : String
            #20 100-113 "$\"Value: {a}\"" : String
            #21 110-111 "a" : Param<"'T": 0>
            #25 131-132 "x" : String
            #31 173-175 "()" : Unit
            #35 183-323 "{\n                let x: String = Foo(1);\n                let y: String = Foo(1.0);\n                let z: String = Foo(true);\n            }" : Unit
            #37 205-214 "x: String" : String
            #42 217-223 "Foo(1)" : String
            #43 217-220 "Foo" : (Int -> String)
            #46 220-223 "(1)" : Int
            #47 221-222 "1" : Int
            #49 245-254 "y: String" : String
            #54 257-265 "Foo(1.0)" : String
            #55 257-260 "Foo" : (Double -> String)
            #58 260-265 "(1.0)" : Double
            #59 261-264 "1.0" : Double
            #61 287-296 "z: String" : String
            #66 299-308 "Foo(true)" : String
            #67 299-302 "Foo" : (Bool -> String)
            #70 302-308 "(true)" : Bool
            #71 303-307 "true" : Bool
        "##]],
    );
}

#[test]
fn show_fail() {
    check(
        r#"
        namespace A {
            function Foo<'T>(a: 'T) : String {
               $"Value: {a}"
            }

            function Main() : Unit {
                let x = Foo(1);
                let y = Foo(1.0);
                let z = Foo(true);
            }
        }
        "#,
        "",
        &expect![[r##"
            #7 51-58 "(a: 'T)" : Param<"'T": 0>
            #8 52-57 "a: 'T" : Param<"'T": 0>
            #15 68-112 "{\n               $\"Value: {a}\"\n            }" : String
            #17 85-98 "$\"Value: {a}\"" : String
            #18 95-96 "a" : Param<"'T": 0>
            #24 139-141 "()" : Unit
            #28 149-265 "{\n                let x = Foo(1);\n                let y = Foo(1.0);\n                let z = Foo(true);\n            }" : Unit
            #30 171-172 "x" : String
            #32 175-181 "Foo(1)" : String
            #33 175-178 "Foo" : (Int -> String)
            #36 178-181 "(1)" : Int
            #37 179-180 "1" : Int
            #39 203-204 "y" : String
            #41 207-215 "Foo(1.0)" : String
            #42 207-210 "Foo" : (Double -> String)
            #45 210-215 "(1.0)" : Double
            #46 211-214 "1.0" : Double
            #48 237-238 "z" : String
            #50 241-250 "Foo(true)" : String
            #51 241-244 "Foo" : (Bool -> String)
            #54 244-250 "(true)" : Bool
            #55 245-249 "true" : Bool
            Error(Type(Error(MissingClassShow("'T", Span { lo: 95, hi: 96 }))))
        "##]],
    );
}

#[test]
fn integral() {
    check(
        r#"
        namespace A {
            function Foo<'T: Integral>(a: 'T) : 'T {
                a ^^^ a
            }

            function Main() : Unit {
                let x: Int = Foo(1);
                let y: BigInt = Foo(10L);
            }
        }
        "#,
        "",
        &expect![[r##"
            #8 61-68 "(a: 'T)" : Param<"'T": 0>
            #9 62-67 "a: 'T" : Param<"'T": 0>
            #15 74-113 "{\n                a ^^^ a\n            }" : Param<"'T": 0>
            #17 92-99 "a ^^^ a" : Param<"'T": 0>
            #18 92-93 "a" : Param<"'T": 0>
            #21 98-99 "a" : Param<"'T": 0>
            #27 140-142 "()" : Unit
            #31 150-244 "{\n                let x: Int = Foo(1);\n                let y: BigInt = Foo(10L);\n            }" : Unit
            #33 172-178 "x: Int" : Int
            #38 181-187 "Foo(1)" : Int
            #39 181-184 "Foo" : (Int -> Int)
            #42 184-187 "(1)" : Int
            #43 185-186 "1" : Int
            #45 209-218 "y: BigInt" : BigInt
            #50 221-229 "Foo(10L)" : BigInt
            #51 221-224 "Foo" : (BigInt -> BigInt)
            #54 224-229 "(10L)" : BigInt
            #55 225-228 "10L" : BigInt
        "##]],
    );
}
#[test]
fn integral_fail() {
    check(
        r#"
        namespace A {
            function Foo<'T: Integral>(a: 'T) : 'T {
                a ^^^ a
            }

            function Main() : Unit {
                let x = Foo(1.0);
                let y = Foo(true);
            }
        }
        "#,
        "",
        &expect![[r##"
            #8 61-68 "(a: 'T)" : Param<"'T": 0>
            #9 62-67 "a: 'T" : Param<"'T": 0>
            #15 74-113 "{\n                a ^^^ a\n            }" : Param<"'T": 0>
            #17 92-99 "a ^^^ a" : Param<"'T": 0>
            #18 92-93 "a" : Param<"'T": 0>
            #21 98-99 "a" : Param<"'T": 0>
            #27 140-142 "()" : Unit
            #31 150-234 "{\n                let x = Foo(1.0);\n                let y = Foo(true);\n            }" : Unit
            #33 172-173 "x" : Double
            #35 176-184 "Foo(1.0)" : Double
            #36 176-179 "Foo" : (Double -> Double)
            #39 179-184 "(1.0)" : Double
            #40 180-183 "1.0" : Double
            #42 206-207 "y" : Bool
            #44 210-219 "Foo(true)" : Bool
            #45 210-213 "Foo" : (Bool -> Bool)
            #48 213-219 "(true)" : Bool
            #49 214-218 "true" : Bool
            Error(Type(Error(MissingClassInteger("Double", Span { lo: 180, hi: 183 }))))
            Error(Type(Error(MissingClassInteger("Bool", Span { lo: 214, hi: 218 }))))
        "##]],
    );
}

#[test]
fn constraint_arguments_for_class_with_no_args() {
    check(
        r#"
        namespace A {
            function Foo<'T: Eq[Int]>() : Bool {
                true
            }
        }
        "#,
        "",
        &expect![[r##"
            #11 60-62 "()" : Unit
            #15 70-106 "{\n                true\n            }" : Bool
            #17 88-92 "true" : Bool
            Error(Type(Error(IncorrectNumberOfConstraintParameters { expected: 0, found: 1, span: Span { lo: 52, hi: 54 } })))
        "##]],
    );
}

#[test]
fn show_and_eq() {
    check(
        r#"
        namespace A {
            function Foo<'T: Eq + Show>(a: 'T, b: 'T) : String {
                if a == b {
                    $"Value: {a}"
                } else {
                    $"Value: {b}"
                }
            }

            function Main() : Unit {
                let x = Foo(1, 1);
                let y = Foo(1, 2);
            }
        }
        "#,
        "",
        &expect![[r##"
            #9 62-76 "(a: 'T, b: 'T)" : (Param<"'T": 0>, Param<"'T": 0>)
            #10 63-68 "a: 'T" : Param<"'T": 0>
            #14 70-75 "b: 'T" : Param<"'T": 0>
            #21 86-240 "{\n                if a == b {\n                    $\"Value: {a}\"\n                } else {\n                    $\"Value: {b}\"\n                }\n            }" : String
            #23 104-226 "if a == b {\n                    $\"Value: {a}\"\n                } else {\n                    $\"Value: {b}\"\n                }" : String
            #24 107-113 "a == b" : Bool
            #25 107-108 "a" : Param<"'T": 0>
            #28 112-113 "b" : Param<"'T": 0>
            #31 114-167 "{\n                    $\"Value: {a}\"\n                }" : String
            #33 136-149 "$\"Value: {a}\"" : String
            #34 146-147 "a" : Param<"'T": 0>
            #37 168-226 "else {\n                    $\"Value: {b}\"\n                }" : String
            #38 173-226 "{\n                    $\"Value: {b}\"\n                }" : String
            #40 195-208 "$\"Value: {b}\"" : String
            #41 205-206 "b" : Param<"'T": 0>
            #47 267-269 "()" : Unit
            #51 277-362 "{\n                let x = Foo(1, 1);\n                let y = Foo(1, 2);\n            }" : Unit
            #53 299-300 "x" : String
            #55 303-312 "Foo(1, 1)" : String
            #56 303-306 "Foo" : ((Int, Int) -> String)
            #59 306-312 "(1, 1)" : (Int, Int)
            #60 307-308 "1" : Int
            #61 310-311 "1" : Int
            #63 334-335 "y" : String
            #65 338-347 "Foo(1, 2)" : String
            #66 338-341 "Foo" : ((Int, Int) -> String)
            #69 341-347 "(1, 2)" : (Int, Int)
            #70 342-343 "1" : Int
            #71 345-346 "2" : Int
        "##]],
    );
}

#[test]
fn show_and_eq_should_fail() {
    check(
        r#"
        namespace A {
            function Foo<'T: Eq + Show>(a: 'T, b: 'T) : String {
                if a == b {
                    $"Value: {a}"
                } else {
                    $"Value: {b}"
                }
            }

            function Main() : Unit {
                let x = Foo(1, true);
                let y = Foo(1, "2");
            }
        }
        "#,
        "",
        &expect![[r##"
            #9 62-76 "(a: 'T, b: 'T)" : (Param<"'T": 0>, Param<"'T": 0>)
            #10 63-68 "a: 'T" : Param<"'T": 0>
            #14 70-75 "b: 'T" : Param<"'T": 0>
            #21 86-240 "{\n                if a == b {\n                    $\"Value: {a}\"\n                } else {\n                    $\"Value: {b}\"\n                }\n            }" : String
            #23 104-226 "if a == b {\n                    $\"Value: {a}\"\n                } else {\n                    $\"Value: {b}\"\n                }" : String
            #24 107-113 "a == b" : Bool
            #25 107-108 "a" : Param<"'T": 0>
            #28 112-113 "b" : Param<"'T": 0>
            #31 114-167 "{\n                    $\"Value: {a}\"\n                }" : String
            #33 136-149 "$\"Value: {a}\"" : String
            #34 146-147 "a" : Param<"'T": 0>
            #37 168-226 "else {\n                    $\"Value: {b}\"\n                }" : String
            #38 173-226 "{\n                    $\"Value: {b}\"\n                }" : String
            #40 195-208 "$\"Value: {b}\"" : String
            #41 205-206 "b" : Param<"'T": 0>
            #47 267-269 "()" : Unit
            #51 277-367 "{\n                let x = Foo(1, true);\n                let y = Foo(1, \"2\");\n            }" : Unit
            #53 299-300 "x" : String
            #55 303-315 "Foo(1, true)" : String
            #56 303-306 "Foo" : ((Int, Int) -> String)
            #59 306-315 "(1, true)" : (Int, Bool)
            #60 307-308 "1" : Int
            #61 310-314 "true" : Bool
            #63 337-338 "y" : String
            #65 341-352 "Foo(1, \"2\")" : String
            #66 341-344 "Foo" : ((Int, Int) -> String)
            #69 344-352 "(1, \"2\")" : (Int, String)
            #70 345-346 "1" : Int
            #71 348-351 "\"2\"" : String
            Error(Type(Error(TyMismatch(TyInfo { kind: Prim(Int), display: "Int" }, TyInfo { kind: Prim(Bool), display: "Bool" }, Span { lo: 310, hi: 314 }))))
            Error(Type(Error(TyMismatch(TyInfo { kind: Prim(Int), display: "Int" }, TyInfo { kind: Prim(String), display: "String" }, Span { lo: 348, hi: 351 }))))
        "##]],
    );
}

#[test]
fn unknown_class() {
    check(
        r#"
        namespace A {
            function Foo<'T: Unknown>(a: 'T) : 'T {
                a
            }

            function Main() : Unit {
                let x = Foo(1);
            }
        }"#,
        "",
        &expect![[r##"
            #8 60-67 "(a: 'T)" : Param<"'T": 0>
            #9 61-66 "a: 'T" : Param<"'T": 0>
            #15 73-106 "{\n                a\n            }" : Param<"'T": 0>
            #17 91-92 "a" : Param<"'T": 0>
            #23 133-135 "()" : Unit
            #27 143-190 "{\n                let x = Foo(1);\n            }" : Unit
            #29 165-166 "x" : Int
            #31 169-175 "Foo(1)" : Int
            #32 169-172 "Foo" : (Int -> Int)
            #35 172-175 "(1)" : Int
            #36 173-174 "1" : Int
            Error(Type(Error(UnrecognizedClass { span: Span { lo: 52, hi: 59 }, name: "Unknown" })))
            Error(Type(Error(UnrecognizedClass { span: Span { lo: 52, hi: 59 }, name: "Unknown" })))
            Error(Type(Error(UnrecognizedClass { span: Span { lo: 52, hi: 59 }, name: "Unknown" })))
            Error(Type(Error(UnrecognizedClass { span: Span { lo: 52, hi: 59 }, name: "Unknown" })))
        "##]],
    );
}

#[test]
fn class_constraint_in_lambda() {
    check(
        r#"
        namespace A {
            function Foo<'T: Eq>(a: 'T -> Bool, b: 'T) : Bool {
                a(b);
                b == b
            }
        }
    "#,
        "",
        &expect![[r##"
            #8 55-77 "(a: 'T -> Bool, b: 'T)" : ((Param<"'T": 0> -> Bool), Param<"'T": 0>)
            #9 56-69 "a: 'T -> Bool" : (Param<"'T": 0> -> Bool)
            #17 71-76 "b: 'T" : Param<"'T": 0>
            #24 85-145 "{\n                a(b);\n                b == b\n            }" : Bool
            #26 103-107 "a(b)" : Bool
            #27 103-104 "a" : (Param<"'T": 0> -> Bool)
            #30 104-107 "(b)" : Param<"'T": 0>
            #31 105-106 "b" : Param<"'T": 0>
            #35 125-131 "b == b" : Bool
            #36 125-126 "b" : Param<"'T": 0>
            #39 130-131 "b" : Param<"'T": 0>
        "##]],
    );
}

#[test]
fn test_harness_use_case() {
    check(
        r#"
        namespace A {
            function Test<'T: Eq>(test_cases: (() => 'T)[], answers: 'T[]) : Unit {
            }
        }
        "#,
        "",
        &expect![[r##"
            #8 56-97 "(test_cases: (() => 'T)[], answers: 'T[])" : ((Unit => Param<"'T": 0>)[], Param<"'T": 0>[])
            #9 57-81 "test_cases: (() => 'T)[]" : (Unit => Param<"'T": 0>)[]
            #17 83-96 "answers: 'T[]" : Param<"'T": 0>[]
            #25 105-120 "{\n            }" : Unit
        "##]],
    );
}
