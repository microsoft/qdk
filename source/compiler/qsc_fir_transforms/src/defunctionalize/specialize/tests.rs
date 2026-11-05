// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Internal contracts for closure-dispatch argument layouts and capture scope.
//!
//! Package-identity and capture-scope checks use compiled Q#; other type and
//! functor compatibility checks use constructed types.

use super::*;
use crate::test_utils::{callable_id_by_name, compile_to_monomorphized_fir};
use crate::walk_utils::collect_expr_ids_in_local_callables;
use qsc_fir::fir::CallableKind;

#[test]
fn shared_capture_binding_requires_matching_callable_keys() {
    use qsc_data_structures::functors::FunctorApp;

    let source = r#"
        operation A(q : Qubit) : Unit is Adj + Ctl { X(q); }
        operation B(q : Qubit) : Unit is Adj + Ctl { H(q); }
        function Make(f : Qubit => Unit is Adj + Ctl) : Qubit => Unit is Adj + Ctl {
            q => f(q)
        }
        @EntryPoint() operation Main() : Unit {
            use q = Qubit();
            Make(A)(q);
            Reset(q);
        }
    "#;
    let (store, package_id) = compile_to_monomorphized_fir(source);
    let package = store.get(package_id);
    let (occurrence, target) = package
        .exprs
        .iter()
        .find_map(|(id, expr)| match &expr.kind {
            ExprKind::Closure(captures, target) if !captures.is_empty() => Some((id, *target)),
            _ => None,
        })
        .expect("source contains a capturing closure");
    let global = |name, functor| ConcreteCallable::Global {
        item_id: ItemId {
            package: package_id,
            item: callable_id_by_name(package, name),
        },
        functor,
    };
    let a = global("A", FunctorApp::default());
    let b = global("B", FunctorApp::default());
    let adj_a = global(
        "A",
        FunctorApp {
            adjoint: true,
            controlled: 0,
        },
    );
    let metadata = UdtMetadata::new(&store);
    for (first, next, conflict) in [
        (vec![a.clone()], vec![a.clone()], false),
        (vec![a.clone()], vec![b.clone()], true),
        (vec![a.clone()], vec![adj_a], true),
        (
            vec![a.clone(), b.clone()],
            vec![a.clone(), b.clone()],
            false,
        ),
        (vec![a.clone(), b.clone()], vec![b, a.clone()], true),
        (vec![a.clone()], vec![a.clone(), a.clone()], true),
    ] {
        let mut tracker = CaptureSpecializations::new(&metadata);
        assert!(tracker.first_use(package, target, occurrence, 0, &first[0], &first));
        assert!(!tracker.failed);
        assert!(!tracker.first_use(package, target, occurrence, 0, &next[0], &next));
        assert_eq!(tracker.failed, conflict, "{first:?} followed by {next:?}");
    }
    let mut tracker = CaptureSpecializations::new(&metadata);
    assert!(!tracker.first_use(package, target, occurrence, usize::MAX, &a, &[]));
    assert!(
        tracker.failed,
        "an unresolved binding must not count as reuse"
    );
}

#[test]
fn replay_disposition_uses_shared_guard_discard_contract() {
    use super::super::rewrite::{
        ConsumptionSite, EvaluationDisposition, consumed_callable_expr_disposition,
    };
    for guard in ["{ set marker += 1; true }", "Guard()", "true"] {
        let source = indoc::formatdoc! {r#"
            function A(x : Int) : Int {{ x + 1 }}
            function Guard() : Bool {{ fail "guard must run" }}
            @EntryPoint() operation Main() : Int {{
                mutable marker = 0;
                let selected = if {guard} {{ A }} else {{ A }};
                selected(0) + marker
            }}
        "#};
        let (store, package_id) = compile_to_monomorphized_fir(&source);
        let package = store.get(package_id);
        let main = callable_id_by_name(package, "Main");
        let selection = collect_expr_ids_in_local_callables(package, &[main])
            .into_iter()
            .find(|id| matches!(package.get_expr(*id).kind, ExprKind::If(..)))
            .expect("source contains a callable selection");
        for site in [ConsumptionSite::Argument, ConsumptionSite::Binding] {
            let disposition = consumed_callable_expr_disposition(
                package,
                package_id,
                selection,
                site,
                &FxHashSet::default(),
            );
            assert_eq!(
                disposition,
                if guard == "true" {
                    EvaluationDisposition::Discarded
                } else {
                    EvaluationDisposition::Replayed
                },
                "guard {guard} at {site:?}"
            );
        }
    }
}

#[test]
fn specialization_closure_target_requires_matching_package() {
    let mut fixture = make_dispatch_layout_fixture();
    let target = fixture
        .package
        .exprs
        .values()
        .find_map(|expr| match expr.kind {
            ExprKind::Closure(_, target) => Some(target),
            _ => None,
        })
        .expect("fixture contains a closure");
    let local_package = PackageId::from(0_usize);
    let foreign_package = PackageId::from(1_usize);
    let callee_ty = arrow_ty(
        CallableKind::Operation,
        fixture.target_input.clone(),
        Ty::UNIT,
        FunctorSetValue::Empty,
    );
    let span = fixture.package.synthetic_span();
    for (package, expected) in [(local_package, true), (foreign_package, false)] {
        let expr = alloc_item_var_expr(
            &mut fixture.package,
            &mut fixture.assigner,
            ItemId {
                package,
                item: target,
            },
            callee_ty.clone(),
            span,
        );
        assert_eq!(
            expr_is_closure_target_callee(&fixture.package, expr, local_package, target),
            expected
        );
    }
}

fn removal_ty(udts: &UdtMetadata, ty: &Ty, paths: &[&[usize]]) -> Ty {
    InputRemoval::new(udts, ty, paths)
        .expect("valid source paths")
        .reduced_ty(udts, ty)
}

#[test]
fn removal_layout_distinguishes_original_unit_and_empty_payloads() {
    let metadata = UdtMetadata::default();
    let int = Ty::Prim(Prim::Int);
    let original = Ty::Tuple(vec![Ty::Tuple(vec![int.clone()]), Ty::UNIT]);
    let removal =
        InputRemoval::new(&metadata, &original, &[&[0, 0], &[0, 0]]).expect("valid original paths");
    assert_eq!(
        removal.reduced_ty(&metadata, &original),
        Ty::Tuple(vec![Ty::UNIT, Ty::UNIT])
    );
    assert!(!removal.consumes_original());
    assert_eq!(removal.rebase(&[0, 0]), None);
    assert_eq!(removal.rebase(&[1]), Some(vec![1]));
    let nested = Ty::Tuple(vec![Ty::Tuple(vec![int.clone(), int])]);
    let removal = InputRemoval::new(&metadata, &nested, &[&[0, 0]]).expect("valid nested path");
    assert_eq!(
        removal.reduced_ty(&metadata, &nested),
        Ty::Tuple(vec![Ty::Prim(Prim::Int)])
    );
    assert_eq!(removal.rebase(&[0, 1]), Some(vec![0]));
    assert!(InputRemoval::new(&metadata, &original, &[&[2]]).is_none());
}

#[test]
fn specialization_removal_coverage_preserves_unit_data() {
    let metadata = UdtMetadata::default();
    let arrow = arrow_ty(
        CallableKind::Function,
        Ty::Prim(Prim::Int),
        Ty::Prim(Prim::Int),
        FunctorSetValue::Empty,
    );
    let with_unit = Ty::Tuple(vec![arrow.clone(), Ty::UNIT]);
    assert!(
        !InputRemoval::new(&metadata, &with_unit, &[&[0]])
            .expect("valid removal")
            .consumes_original()
    );
    assert!(
        !InputRemoval::new(&metadata, &Ty::UNIT, &[])
            .expect("empty removal")
            .consumes_original()
    );
    let nested = Ty::Tuple(vec![Ty::Tuple(vec![arrow.clone(), arrow])]);
    assert!(
        InputRemoval::new(&metadata, &nested, &[&[0, 0], &[0, 1]])
            .expect("valid nested removal")
            .consumes_original()
    );
    assert!(
        !InputRemoval::new(&metadata, &nested, &[&[0, 0]])
            .expect("valid partial removal")
            .consumes_original()
    );
}

#[test]
fn nested_removal_batches_preserve_original_paths_and_unit_fields() {
    let arrow = arrow_ty(
        CallableKind::Function,
        Ty::Prim(Prim::Int),
        Ty::Prim(Prim::Int),
        FunctorSetValue::Empty,
    );
    let metadata = UdtMetadata::default();
    for data in [Ty::UNIT, Ty::Prim(Prim::Int)] {
        let original = Ty::Tuple(vec![
            Ty::Tuple(vec![arrow.clone(), arrow.clone()]),
            data.clone(),
        ]);
        let paths: &[&[usize]] = &[&[0, 0], &[0, 1]];
        let removal = InputRemoval::new(&metadata, &original, paths).expect("valid original paths");
        assert_eq!(
            removal.reduced_ty(&metadata, &original),
            Ty::Tuple(vec![Ty::UNIT, data])
        );
        assert_eq!(removal.rebase(&[1]), Some(vec![1]));
        assert_eq!(removal.rebase(&[0, 0]), None);
    }
    let original = Ty::Tuple(vec![
        arrow.clone(),
        Ty::Tuple(vec![arrow, Ty::Prim(Prim::Int)]),
    ]);
    let removal =
        InputRemoval::new(&metadata, &original, &[&[0], &[1, 0]]).expect("valid original paths");
    assert_eq!(
        removal.reduced_ty(&metadata, &original),
        Ty::Prim(Prim::Int)
    );
    assert_eq!(removal.rebase(&[1, 1]), Some(vec![]));
}

#[test]
fn nested_removal_batches_rewrite_source_destructuring() {
    let (store, package_id) = compile_to_monomorphized_fir(
        r#"
        function Inc(x : Int) : Int { x + 1 }
        function Twice(x : Int) : Int { 2 * x }
        function Run(pair : ((Int -> Int, Int -> Int), Int)) : Int {
            let ((f, g), n) = pair;
            f(n) + g(n)
        }
        @EntryPoint() operation Main() : Int { Run(((Inc, Twice), 3)) }
    "#,
    );
    let metadata = UdtMetadata::new(&store);
    let mut package = store.get(package_id).clone();
    let id = callable_id_by_name(&package, "Run");
    let ItemKind::Callable(decl) = &package.get_item(id).kind else {
        panic!("callable")
    };
    let CallableImpl::Spec(implementation) = &decl.implementation else {
        panic!("body")
    };
    let first = package.get_block(implementation.body.block).stmts[0];
    let qsc_fir::fir::StmtKind::Local(_, pattern, _) = package.get_stmt(first).kind else {
        panic!("destructuring")
    };
    let n = find_bind_local_at_field_path(&package, pattern, &[1]);
    assert!(n.is_some());
    assert!(remove_pat_at_field_paths(
        &metadata,
        &mut package,
        pattern,
        &[&[0, 0], &[0, 1]]
    ));
    assert_eq!(
        package.get_pat(pattern).ty,
        Ty::Tuple(vec![Ty::UNIT, Ty::Prim(Prim::Int)])
    );
    assert_eq!(find_bind_local_at_field_path(&package, pattern, &[1]), n);
    assert_eq!(
        find_bind_local_at_field_path(&package, pattern, &[0, 0]),
        None
    );
}

struct NominalInputFixture {
    package: Package,
    metadata: UdtMetadata,
    read_id: LocalItemId,
    input_local: LocalVarId,
    input_ty: Ty,
}

fn compile_nominal_input_fixture(source: &str) -> NominalInputFixture {
    let (store, package_id) = compile_to_monomorphized_fir(source);
    let metadata = UdtMetadata::new(&store);
    let package = store.get(package_id).clone();
    let read_id = callable_id_by_name(&package, "Read");
    let ItemKind::Callable(decl) = &package.get_item(read_id).kind else {
        panic!("callable")
    };
    let original = package.get_pat(decl.input);
    let PatKind::Bind(binding) = &original.kind else {
        panic!("input")
    };
    let input_local = binding.id;
    let input_ty = original.ty.clone();
    NominalInputFixture {
        package,
        metadata,
        read_id,
        input_local,
        input_ty,
    }
}

fn make_nominal_input_fixture() -> NominalInputFixture {
    compile_nominal_input_fixture(
        r#"
        struct Data { N : Int, Tail : Int }
        struct Payload { F : Int -> Int, D : Data, Values : Data[] }
        function Inc(x : Int) : Int { x + 1 }
        function Read(p : Payload) : Int { p.F(p.D.N) + p.Values[0].Tail }
        @EntryPoint() operation Main() : Int {
            Read(new Payload {
                F = Inc, D = new Data { N = 3, Tail = 0 },
                Values = [new Data { N = 0, Tail = 10 }]
            })
        }
    "#,
    )
}

fn rebase_fixture(fixture: &mut NominalInputFixture, paths: &[&[usize]]) {
    let ItemKind::Callable(read) = &fixture.package.get_item(fixture.read_id).kind else {
        panic!("callable")
    };
    let implementation = read.implementation.clone();
    let mut assigner = Assigner::from_package(&fixture.package);
    reindex_sibling_field_access(
        &fixture.metadata,
        &mut fixture.package,
        &implementation,
        fixture.input_local,
        paths,
        &fixture.input_ty,
        &mut assigner,
    );
}

#[test]
fn removal_layout_keeps_nominal_children() {
    let fixture = make_nominal_input_fixture();
    let Ty::Tuple(fields) = fixture.metadata.underlying_ty(&fixture.input_ty) else {
        panic!("fields")
    };
    assert_eq!(
        removal_ty(&fixture.metadata, &fixture.input_ty, &[&[0]]),
        Ty::Tuple(fields[1..].to_vec())
    );
}

#[test]
fn removal_layout_keeps_projection_types() {
    let mut fixture = make_nominal_input_fixture();
    let projections: Vec<_> =
        collect_expr_ids_in_local_callables(&fixture.package, &[fixture.read_id])
            .into_iter()
            .filter_map(|expr| {
                let path =
                    collect_field_path_from_param(&fixture.package, expr, fixture.input_local)?;
                matches!(path.as_slice(), [1 | 2] | [1, 0])
                    .then(|| (expr, fixture.package.get_expr(expr).ty.clone()))
            })
            .collect();
    assert_eq!(projections.len(), 3);
    rebase_fixture(&mut fixture, &[&[0]]);
    for (expr, original) in projections {
        assert_eq!(fixture.package.get_expr(expr).ty, original);
    }
}

#[test]
fn removal_layout_keeps_empty_path_extraction_types() {
    for (definition, value, projection, output) in [
        ("newtype Data = (N : Int);", "Data(10)", "(p.D)::N", "Int"),
        (
            "newtype Inner = (N : Int); newtype Data = (Value : Inner);",
            "Data(Inner(10))",
            "((p.D)::Value)::N",
            "Int",
        ),
        (
            "newtype Data = (Value : (Int, Int));",
            "Data((10, 2))",
            "(p.D)::Value",
            "(Int, Int)",
        ),
    ] {
        let source = indoc::formatdoc! {r#"
            {definition}
            struct Payload {{ F : Int -> Int, D : Data }}
            function Inc(x : Int) : Int {{ x + 1 }}
            function Read(p : Payload) : (Data, {output}) {{ (p.D, {projection}) }}
            @EntryPoint() operation Main() : (Data, {output}) {{
                Read(new Payload {{ F = Inc, D = {value} }})
            }}
        "#};
        let mut fixture = compile_nominal_input_fixture(&source);
        let projections: Vec<_> = collect_expr_ids_in_local_callables(&fixture.package, &[fixture.read_id])
            .into_iter().filter_map(|id| {
                let path = collect_field_path_from_param(&fixture.package, id, fixture.input_local)?;
                let expr = fixture.package.get_expr(id);
                path.starts_with(&[1]).then(|| (id, expr.ty.clone(),
                    matches!(&expr.kind, ExprKind::Field(_, Field::Path(path)) if path.indices.is_empty())))
            }).collect();
        assert!(projections.iter().any(|(_, _, empty)| *empty));
        rebase_fixture(&mut fixture, &[&[0]]);
        for (id, original, empty) in projections {
            let expr = fixture.package.get_expr(id);
            assert_eq!(expr.ty, original, "{source}");
            if empty {
                let ExprKind::Field(base, Field::Path(path)) = &expr.kind else {
                    panic!("nominal extraction")
                };
                assert!(path.indices.is_empty());
                let Ty::Udt(Res::Item(item)) = &fixture.package.get_expr(*base).ty else {
                    panic!("nominal base")
                };
                assert_eq!(fixture.metadata.pure_ty(*item), Some(&expr.ty));
            }
        }
    }
}

#[test]
fn nested_removal_batches_rebase_source_field_projections() {
    let mut fixture = compile_nominal_input_fixture(
        r#"
        struct Inner { F : Int -> Int, G : Int -> Int, N : Int }
        struct Outer { Value : Inner, Tail : Int }
        function Inc(x : Int) : Int { x + 1 }
        function Read(pair : Outer) : Int { pair.Value.N + pair.Tail }
        @EntryPoint() operation Main() : Int {
            Read(new Outer { Value = new Inner { F = Inc, G = Inc, N = 3 }, Tail = 100 })
        }
    "#,
    );
    let projections: Vec<_> =
        collect_expr_ids_in_local_callables(&fixture.package, &[fixture.read_id])
            .into_iter()
            .filter_map(|id| {
                let path =
                    collect_field_path_from_param(&fixture.package, id, fixture.input_local)?;
                (path == [0, 2] || path == [1]).then_some((id, path))
            })
            .collect();
    assert_eq!(projections.len(), 2);
    rebase_fixture(&mut fixture, &[&[0, 0], &[0, 1]]);
    for (id, original) in projections {
        let expected = if original == [0, 2] { vec![0] } else { vec![1] };
        assert_eq!(
            collect_field_path_from_param(&fixture.package, id, fixture.input_local),
            Some(expected)
        );
        assert_eq!(fixture.package.get_expr(id).ty, Ty::Prim(Prim::Int));
    }
}

#[test]
fn closure_call_convention_controls_zero_capture_packaging() {
    use super::super::rewrite::{CallConvention, build_closure_call_args};
    let mut fixture = make_dispatch_layout_fixture();
    let original = fixture.package.get_expr(fixture.args_id).clone();
    let input = Ty::Tuple(vec![original.ty.clone()]);
    for (convention, expected) in [
        (CallConvention::DirectItem, input.clone()),
        (CallConvention::ClosureValue, original.ty.clone()),
    ] {
        let result = build_closure_call_args(
            &mut fixture.package,
            fixture.args_id,
            fixture.destination,
            &[],
            &input,
            0,
            convention,
            &mut fixture.assigner,
        )
        .expect("convention matches declared input");
        assert!(result.prefix.is_empty());
        assert_eq!(fixture.package.get_expr(result.value).ty, expected);
        assert_eq!(
            fixture.package.get_expr(fixture.args_id).kind,
            original.kind
        );
        assert_eq!(fixture.package.get_expr(fixture.args_id).ty, original.ty);
    }
}

fn wrap_dispatch_fixture_controls(fixture: &mut DispatchLayoutFixture, layers: usize) {
    let span = fixture.package.get_expr(fixture.args_id).span;
    for _ in 0..layers {
        let controls = alloc_expr(
            &mut fixture.package,
            &mut fixture.assigner,
            qubit_array_ty(),
            ExprKind::Array(Vec::new()),
            span,
        );
        let ty = Ty::Tuple(vec![
            qubit_array_ty(),
            fixture.package.get_expr(fixture.args_id).ty.clone(),
        ]);
        fixture.args_id = alloc_expr(
            &mut fixture.package,
            &mut fixture.assigner,
            ty,
            ExprKind::Tuple(vec![controls, fixture.args_id]),
            span,
        );
    }
}

#[test]
fn dispatch_layout_capability_packing_is_idempotent() {
    use super::super::rewrite::{CallConvention, build_closure_call_args};
    for required in [
        FunctorSetValue::Empty,
        FunctorSetValue::Adj,
        FunctorSetValue::Ctl,
        FunctorSetValue::CtlAdj,
    ] {
        for tag_first in [false, true] {
            for layers in 0..=2 {
                let mut fixture = make_dispatch_layout_fixture();
                let mut types = vec![qubit_array_op_ty(required), Ty::Prim(Prim::Int)];
                if tag_first {
                    fixture.captures.reverse();
                    types.reverse();
                }
                types.push(qubit_array_ty());
                fixture.target_input = Ty::Tuple(types);
                let payload = fixture.package.get_expr(fixture.args_id).clone();
                let actual = Ty::Tuple(
                    fixture
                        .captures
                        .iter()
                        .map(|c| c.ty.clone())
                        .chain(std::iter::once(payload.ty.clone()))
                        .collect(),
                );
                wrap_dispatch_fixture_controls(&mut fixture, layers);
                let mut value = fixture.args_id;
                for _ in 0..2 {
                    let result = build_closure_call_args(
                        &mut fixture.package,
                        value,
                        fixture.destination,
                        &fixture.captures,
                        &fixture.target_input,
                        layers,
                        CallConvention::DirectItem,
                        &mut fixture.assigner,
                    )
                    .expect("stronger capabilities populate the declared input");
                    assert!(result.prefix.is_empty());
                    value = result.value;
                    let mut base = value;
                    for _ in 0..layers {
                        let ExprKind::Tuple(fields) = &fixture.package.get_expr(base).kind else {
                            panic!("control shell")
                        };
                        assert_eq!(fields.len(), 2);
                        base = fields[1];
                    }
                    let packed = fixture.package.get_expr(base);
                    assert_eq!(packed.ty, actual);
                    assert!(dispatch_layout_types_compatible(
                        &packed.ty,
                        &fixture.target_input
                    ));
                    let ExprKind::Tuple(fields) = &packed.kind else {
                        panic!("packed input")
                    };
                    assert_eq!(fields.len(), fixture.captures.len() + 1);
                    for (field, capture) in fields.iter().zip(&fixture.captures) {
                        assert!(
                            matches!(fixture.package.get_expr(*field).kind, ExprKind::Var(Res::Local(local), _) if local == capture.local.var)
                        );
                    }
                    assert_eq!(fixture.package.get_expr(fields[2]).ty, payload.ty);
                    assert_eq!(fixture.package.get_expr(fields[2]).kind, payload.kind);
                }
            }
        }
    }
}

#[test]
fn dispatch_layout_capability_packing_rejects_missing_functors() {
    use super::super::rewrite::{CallConvention, build_closure_call_args};
    let mut fixture = make_dispatch_layout_fixture();
    fixture.captures[0].ty = qubit_array_op_ty(FunctorSetValue::Adj);
    wrap_dispatch_fixture_controls(&mut fixture, 2);
    let original = fixture.package.get_expr(fixture.args_id).clone();
    assert!(
        build_closure_call_args(
            &mut fixture.package,
            fixture.args_id,
            fixture.destination,
            &fixture.captures,
            &fixture.target_input,
            2,
            CallConvention::DirectItem,
            &mut fixture.assigner
        )
        .is_none()
    );
    assert_eq!(
        fixture.package.get_expr(fixture.args_id).kind,
        original.kind
    );
    assert_eq!(fixture.package.get_expr(fixture.args_id).ty, original.ty);
}

#[test]
fn dispatch_layout_capability_packing_rejects_weaker_packed_input() {
    use super::super::rewrite::{CallConvention, build_closure_call_args};
    let mut fixture = make_dispatch_layout_fixture();
    fixture.captures[0].ty = qubit_array_op_ty(FunctorSetValue::Adj);
    let weaker = target_input_ty(fixture.captures[0].ty.clone(), Ty::Prim(Prim::Int));
    wrap_dispatch_fixture_controls(&mut fixture, 2);
    let packed = build_closure_call_args(
        &mut fixture.package,
        fixture.args_id,
        fixture.destination,
        &fixture.captures,
        &weaker,
        2,
        CallConvention::DirectItem,
        &mut fixture.assigner,
    )
    .expect("weaker declaration accepts weaker captures");
    assert!(packed.prefix.is_empty());
    let original = fixture.package.get_expr(packed.value).clone();
    assert!(
        build_closure_call_args(
            &mut fixture.package,
            packed.value,
            fixture.destination,
            &fixture.captures,
            &fixture.target_input,
            2,
            CallConvention::DirectItem,
            &mut fixture.assigner
        )
        .is_none()
    );
    assert_eq!(fixture.package.get_expr(packed.value).kind, original.kind);
    assert_eq!(fixture.package.get_expr(packed.value).ty, original.ty);
}

fn specialization_key_site() -> CallSite {
    CallSite {
        call_expr_id: ExprId::from(0_u32),
        call_pkg_id: PackageId::from(0_usize),
        hof_item_id: ItemId {
            package: PackageId::from(0_usize),
            item: LocalItemId::from(0_usize),
        },
        top_level_param: 0,
        field_path: Vec::new(),
        hof_input_is_tuple: true,
        callable_arg: ConcreteCallable::Global {
            item_id: ItemId {
                package: PackageId::from(0_usize),
                item: LocalItemId::from(1_usize),
            },
            functor: FunctorApp::default(),
        },
        arg_expr_id: ExprId::from(1_u32),
        dispatch_source: None,
        condition: Vec::new(),
    }
}

#[test]
fn specialization_metadata_keeps_unknown_udts_opaque() {
    let metadata = UdtMetadata::default();
    let unknown = Ty::Udt(Res::Item(ItemId {
        package: PackageId::from(12345_usize),
        item: LocalItemId::from(0_usize),
    }));
    assert_eq!(metadata.resolve(&unknown), unknown);
    assert_eq!(metadata.underlying_ty(&unknown), &unknown);
    assert!(metadata.contains_arrow(&unknown));
    assert!(metadata.contains_arrow(&Ty::Array(Box::new(unknown.clone()))));
    assert!(metadata.contains_arrow(&Ty::Tuple(vec![Ty::UNIT, unknown])));
    assert!(!metadata.contains_arrow(&Ty::UNIT));
}

#[test]
fn specialization_metadata_resolves_foreign_nested_layouts() {
    let library = r#"
        namespace Lib {
            struct Inner { F : Int -> Int, N : Int }
            struct Outer { Value : Inner, Tag : Int }
            function Read(p : Outer) : Int { p.Value.F(p.Value.N) + p.Tag }
            export Outer, Read;
        }
    "#;
    let source = r#"
        struct Different { F : Int -> Int }
        function Use(p : Lib.Outer) : Int { Lib.Read(p) }
        @EntryPoint() operation Main() : Unit {}
    "#;
    let (mut store, package_id) = crate::test_utils::compile_to_fir_with_library(library, source);
    let metadata = UdtMetadata::new(&store);
    let package = store.get(package_id);
    let item = callable_id_by_name(package, "Use");
    let ItemKind::Callable(decl) = &package.get_item(item).kind else {
        panic!("Use should be callable");
    };
    let original = package.get_pat(decl.input).ty.clone();
    let arrow = arrow_ty(
        CallableKind::Function,
        Ty::Prim(Prim::Int),
        Ty::Prim(Prim::Int),
        FunctorSetValue::Empty,
    );
    let structural = Ty::Tuple(vec![
        Ty::Tuple(vec![arrow, Ty::Prim(Prim::Int)]),
        Ty::Prim(Prim::Int),
    ]);
    assert_eq!(metadata.resolve(&original), structural);
    assert!(metadata.contains_arrow(&original));
    let Ty::Tuple(fields) = metadata.underlying_ty(&original) else {
        panic!("Outer should be a tuple");
    };
    let Ty::Udt(Res::Item(inner)) = fields[0] else {
        panic!("untouched Inner must remain nominal");
    };
    assert_ne!(inner.package, package_id);
    assert!(
        matches!(&package.get_item(inner.item).kind, ItemKind::Ty(name, _) if name.name.as_ref() == "Different"),
        "fixture must contain a conflicting package-local item ID",
    );
    assert_eq!(package.get_pat(decl.input).ty, original);
    store.get_mut(inner.package).items.remove(inner.item);
    assert_eq!(
        metadata.resolve(&original),
        structural,
        "a later package mutation must not change the immutable snapshot",
    );
}

#[test]
fn specialization_key_tracks_removed_positions() {
    let site = specialization_key_site();
    let mut keys = FxHashSet::default();
    for (top_level_param, field_path) in [(0, vec![]), (1, vec![]), (0, vec![0]), (0, vec![1])] {
        let member = CallSite {
            top_level_param,
            field_path,
            ..site.clone()
        };
        let key = build_spec_key(&member);
        assert_eq!(
            key.param_positions,
            vec![(member.top_level_param, member.field_path.clone())]
        );
        assert!(keys.insert(key), "each removed position needs its own key");
    }
}

#[test]
fn specialization_key_normalizes_discovery_order() {
    let first = specialization_key_site();
    let second = CallSite {
        top_level_param: 1,
        ..first.clone()
    };
    assert_eq!(
        build_combined_spec_key(first.hof_item_id, &[&first, &second]),
        build_combined_spec_key(first.hof_item_id, &[&second, &first]),
        "discovery order must not change position-aligned combined identity",
    );
}

#[test]
fn specialization_key_preserves_repeated_slot_occurrences() {
    let first = CallSite {
        callable_arg: ConcreteCallable::Closure {
            target: LocalItemId::from(1_usize),
            captures: vec![],
            functor: FunctorApp::default(),
        },
        ..specialization_key_site()
    };
    let second = CallSite {
        top_level_param: 1,
        ..first.clone()
    };
    let key = crate::defunctionalize::build_static_callable_array_combined_spec_key(
        first.hof_item_id,
        &[&second, &first, &second, &first],
    );
    assert_eq!(
        key.param_positions,
        vec![(0, vec![]), (0, vec![]), (1, vec![]), (1, vec![])]
    );
    let occurrences: Vec<_> = key
        .concrete_args
        .iter()
        .map(|key| match key {
            crate::defunctionalize::types::ConcreteCallableKey::Closure { occurrence, .. } => {
                *occurrence
            }
            ConcreteCallableKey::Global { .. } => panic!("expected closure key"),
        })
        .collect();
    assert_eq!(occurrences, vec![Some(0), Some(1), Some(0), Some(1)]);
    assert_ne!(key.concrete_args[0], key.concrete_args[1]);
    assert_eq!(key.concrete_args[0], key.concrete_args[2]);
}

#[test]
fn specialization_key_keeps_closure_package_with_removed_position() {
    let first = CallSite {
        field_path: vec![1],
        callable_arg: ConcreteCallable::Closure {
            target: LocalItemId::from(1_usize),
            captures: vec![],
            functor: FunctorApp::default(),
        },
        ..specialization_key_site()
    };
    let second = CallSite {
        call_pkg_id: PackageId::from(1_usize),
        ..first.clone()
    };
    let first_key = build_spec_key(&first);
    let second_key = build_spec_key(&second);
    assert_eq!(first_key.param_positions, second_key.param_positions);
    assert_ne!(first_key, second_key);
}

#[test]
fn specialization_key_keeps_embedded_callable_with_removed_position() {
    let make_site = |item| CallSite {
        field_path: vec![1],
        callable_arg: ConcreteCallable::Closure {
            target: LocalItemId::from(1_usize),
            captures: vec![CapturedVar {
                local: ScopedLocal::new(LocalVarId::from(0_u32), CaptureScope::Entry),
                ty: arrow_ty(
                    CallableKind::Function,
                    Ty::Prim(Prim::Int),
                    Ty::Prim(Prim::Int),
                    FunctorSetValue::Empty,
                ),
                static_callable: Some((
                    ItemId {
                        package: PackageId::from(0_usize),
                        item,
                    },
                    FunctorApp::default(),
                )),
                expr: None,
                caller_substitutions: vec![],
            }],
            functor: FunctorApp::default(),
        },
        ..specialization_key_site()
    };
    let first = build_spec_key(&make_site(LocalItemId::from(2_usize)));
    let second = build_spec_key(&make_site(LocalItemId::from(3_usize)));
    assert_eq!(first.param_positions, second.param_positions);
    assert_ne!(first, second);
}

/// Item numbers are only unique within a package. Here the library's `Inc`
/// (`Int -> Int`) and the user's `Decoy` (`Bool -> Bool`) deliberately share
/// a number. Looking up `Inc` in the user package would give it `Decoy`'s type.
///
/// First verify that the fixture really contains that collision. Then rebuild
/// the foreign reference and check that its original `Int -> Int` type survives.
/// No replacement type means the builder keeps the supplied type hint.
/// The reference must also not match the local `Decoy` as a closure-call target,
/// which would allow capture arguments to be attached to the wrong call.
#[test]
fn foreign_callable_references_preserve_package_identity() {
    let (store, package_id) = crate::test_utils::compile_to_fir_with_library(
        r#"
        namespace Lib {
            function Inc(value : Int) : Int { value + 1 }
            export Inc;
        }
        "#,
        r#"
        namespace Test {
            function Decoy(value : Bool) : Bool { not value }
            @EntryPoint()
            operation Main() : Int { Lib.Inc(3) }
        }
        "#,
    );
    let package = store.get(package_id);
    let (expr_id, item_id, hint) = package
        .exprs
        .values()
        .find_map(|expr| {
            let ExprKind::Var(Res::Item(item), _) = expr.kind else {
                return None;
            };
            if item.package == package_id {
                return None;
            }
            matches!(&store.get(item.package).get_item(item.item).kind,
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "Inc")
            .then_some((expr.id, item, expr.ty.clone()))
        })
        .expect("the user calls the foreign function");
    assert!(
        matches!(&package.get_item(item_id.item).kind, ItemKind::Callable(decl)
            if decl.name.name.as_ref() == "Decoy"),
        "the fixture must exercise a colliding package-local item ID"
    );
    assert!(
        !expr_is_closure_target_callee(package, expr_id, package_id, item_id.item),
        "a foreign callable must not match a same-numbered local target"
    );
    let (_, _, actual) = build_callable_value_parts(
        package,
        CaptureScope::Entry,
        &ConcreteCallable::Global {
            item_id,
            functor: FunctorApp::default(),
        },
        &hint,
    )
    .expect("a global callable can be reconstructed");
    assert_eq!(actual.unwrap_or_else(|| hint.clone()), hint);
}

#[test]
fn specialization_cloner_keeps_local_watermark_across_targets_and_resets() {
    let (store, package_id) = compile_to_monomorphized_fir(
        r#"
        function Make(offset : Int) : Int -> Int {
            value -> {
                let first = offset + value;
                let second = first + 1;
                let third = second + 1;
                third
            }
        }
        @EntryPoint() operation Main() : Int {
            let f = Make(10);
            f(1)
        }
        "#,
    );
    let source = store.get(package_id);
    let make = callable_id_by_name(source, "Make");
    let ItemKind::Callable(decl) = &source.get_item(make).kind else {
        panic!("Make should be callable");
    };
    let PatKind::Bind(offset) = &source.get_pat(decl.input).kind else {
        panic!("Make should bind its offset");
    };

    for initial_floor in [0_u32, 100] {
        let mut target = Package {
            id: package_id,
            ..Package::default()
        };
        let mut assigner = Assigner::new();
        assigner.set_next_local(LocalVarId::from(initial_floor));
        let mut cloner = FirCloner::from_assigner(assigner);
        cloner.clone_pat(source, decl.input, &mut target);
        cloner.clone_callable_impl(source, &decl.implementation, &mut target);
        let capture = cloner.alloc_local(offset.id);
        let nested_max = target
            .pats
            .values()
            .filter_map(|pat| match &pat.kind {
                PatKind::Bind(ident) => Some(ident.id),
                _ => None,
            })
            .max()
            .expect("the cloned target binds locals");
        assert!(
            nested_max > capture,
            "a restored outer counter must not hide larger nested-target locals"
        );

        cloner.reset_maps();
        cloner.clone_pat(source, decl.input, &mut target);
        let mut assigner = cloner.into_assigner();
        let fresh = assigner.next_local();
        assert!(fresh >= LocalVarId::from(initial_floor));
        assert!(
            fresh > nested_max && fresh > capture,
            "recovered assigner must include nested locals and appended captures"
        );
        assert!(assigner.next_local() > fresh);
    }
}

/// `Qubit[]`, the scalar payload and the captured operation's input.
fn qubit_array_ty() -> Ty {
    Ty::Array(Box::new(Ty::Prim(Prim::Qubit)))
}

/// An arrow type with an explicit functor set value.
fn arrow_ty(kind: CallableKind, input: Ty, output: Ty, functors: FunctorSetValue) -> Ty {
    Ty::Arrow(Box::new(Arrow {
        kind,
        input: Box::new(input),
        output: Box::new(output),
        functors: FunctorSet::Value(functors),
    }))
}

/// `Qubit[] => Unit is <functors>`, the shape of the captured operation.
fn qubit_array_op_ty(functors: FunctorSetValue) -> Ty {
    arrow_ty(
        CallableKind::Operation,
        qubit_array_ty(),
        Ty::UNIT,
        functors,
    )
}

/// The specialization target's input, `(op, tag, qs)`, using the supplied
/// callable and tag types.
fn target_input_ty(op_ty: Ty, tag_ty: Ty) -> Ty {
    Ty::Tuple(vec![op_ty, tag_ty, qubit_array_ty()])
}

struct DispatchLayoutFixture {
    package: Package,
    assigner: Assigner,
    destination: CaptureScope,
    args_id: ExprId,
    captures: Vec<CapturedVar>,
    target_input: Ty,
}

#[allow(clippy::too_many_lines)]
fn make_dispatch_layout_fixture() -> DispatchLayoutFixture {
    let (store, package_id) = compile_to_monomorphized_fir(
        r#"
        operation Caller(op : Qubit[] => Unit is Adj + Ctl, tag : Int, payload : Qubit[]) : Unit {
            let closure = remaining => { if tag == 0 { op(remaining); } };
            closure(payload);
        }
        operation Idle(register : Qubit[]) : Unit is Adj + Ctl {}
        operation Main() : Unit {
            use register = Qubit[0];
            Caller(Idle, 7, register);
        }
        "#,
    );
    let mut package = store.get(package_id).clone();
    let mut assigner = Assigner::from_package(&package);
    let main = callable_id_by_name(&package, "Main");
    let caller_id = collect_expr_ids_in_local_callables(&package, &[main])
        .into_iter()
        .find_map(|id| {
            if let ExprKind::Var(Res::Item(item), _) = &package.get_expr(id).kind
                && item.package == package_id
                && let ItemKind::Callable(decl) = &package.get_item(item.item).kind
                && decl.name.name.starts_with("Caller")
            {
                Some(item.item)
            } else {
                None
            }
        })
        .expect("Main should reference a concrete Caller");
    let ItemKind::Callable(caller) = &package.get_item(caller_id).kind else {
        panic!("Caller should be callable");
    };
    let destination = CaptureScope::Callable(caller_id);
    let input = package.get_pat(caller.input);
    let caller_exprs = collect_expr_ids_in_local_callables(&package, &[caller_id]);
    let PatKind::Tuple(parameters) = &input.kind else {
        panic!("Caller should have tuple parameters");
    };
    let bindings: Vec<_> = parameters.iter().map(|id| package.get_pat(*id)).collect();
    let PatKind::Bind(payload) = &bindings[2].kind else {
        panic!("payload should be a binding");
    };
    let args_id = caller_exprs.iter().find_map(|id| {
        matches!(&package.get_expr(*id).kind, ExprKind::Var(Res::Local(local), _) if *local == payload.id)
            .then_some(*id)
    }).expect("Caller should reference its payload");
    let (capture_ids, target) = caller_exprs
        .iter()
        .find_map(|id| {
            if let ExprKind::Closure(captures, target) = &package.get_expr(*id).kind {
                Some((captures, *target))
            } else {
                None
            }
        })
        .expect("lambda should lower to a closure");
    assert_eq!(capture_ids.len(), 2);
    let captures: Vec<_> = capture_ids
        .iter()
        .map(|local| {
            let binding = bindings
                .iter()
                .find(|binding| matches!(&binding.kind, PatKind::Bind(ident) if ident.id == *local))
                .expect("closure should capture a Caller parameter");
            CapturedVar {
                local: ScopedLocal::new(*local, destination),
                ty: binding.ty.clone(),
                static_callable: None,
                expr: None,
                caller_substitutions: Vec::new(),
            }
        })
        .collect();
    let ItemKind::Callable(target) = &package.get_item(target).kind else {
        panic!("closure target should be callable");
    };
    let target_input = package.get_pat(target.input).ty.clone();
    assert_eq!(captures[0].ty, qubit_array_op_ty(FunctorSetValue::CtlAdj));
    assert_eq!(captures[1].ty, Ty::Prim(Prim::Int));
    assert_eq!(
        target_input,
        target_input_ty(captures[0].ty.clone(), captures[1].ty.clone())
    );
    assert_eq!(package.get_expr(args_id).ty, qubit_array_ty());
    assert!(
        build_closure_dispatch_branch_args_data(
            &mut package,
            destination,
            args_id,
            &captures,
            &target_input,
            &mut assigner,
        )
        .is_some(),
        "source-derived layout should be valid before corruption"
    );

    DispatchLayoutFixture {
        package,
        assigner,
        destination,
        args_id,
        captures,
        target_input,
    }
}

#[test]
fn callable_and_clone_scope_collision_declines_capture_write() {
    let mut fixture = make_dispatch_layout_fixture();
    let destination_capture = fixture.captures[0].clone();
    let CaptureScope::Callable(owner) = fixture.destination else {
        panic!("source capture should belong to Caller");
    };
    fixture.captures[0].local.scope = CaptureScope::CloneScope(owner);

    assert_ne!(
        fixture.captures[0], destination_capture,
        "callable and clone domains with the same integer id must remain distinct",
    );

    let before = fixture.package.get_expr(fixture.args_id).clone();
    rewrite_closure_dispatch_branch_args(
        &mut fixture.package,
        fixture.destination,
        fixture.args_id,
        &fixture.captures,
        &fixture.target_input,
        0,
        CallConvention::DirectItem,
        &mut fixture.assigner,
    );
    let after = fixture.package.get_expr(fixture.args_id);

    assert_eq!(
        after.kind, before.kind,
        "a numerically colliding capture from another callable must not be written",
    );
    assert_eq!(
        after.ty, before.ty,
        "a declined write must preserve its type"
    );
}

#[test]
fn dispatch_layout_rejects_incompatible_types() {
    let operation = qubit_array_op_ty(FunctorSetValue::CtlAdj);
    let expected = target_input_ty(operation.clone(), Ty::Prim(Prim::Int));
    let cases = [
        (
            "callable input shape",
            target_input_ty(
                arrow_ty(
                    CallableKind::Operation,
                    Ty::Prim(Prim::Qubit),
                    Ty::UNIT,
                    FunctorSetValue::CtlAdj,
                ),
                Ty::Prim(Prim::Int),
            ),
        ),
        (
            "callable output type",
            target_input_ty(
                arrow_ty(
                    CallableKind::Operation,
                    qubit_array_ty(),
                    Ty::Prim(Prim::Int),
                    FunctorSetValue::CtlAdj,
                ),
                Ty::Prim(Prim::Int),
            ),
        ),
        (
            "callable kind",
            target_input_ty(
                arrow_ty(
                    CallableKind::Function,
                    qubit_array_ty(),
                    Ty::UNIT,
                    FunctorSetValue::CtlAdj,
                ),
                Ty::Prim(Prim::Int),
            ),
        ),
        (
            "non-callable capture field",
            target_input_ty(operation.clone(), Ty::Prim(Prim::Double)),
        ),
        (
            "tuple arity",
            Ty::Tuple(vec![operation, Ty::Prim(Prim::Int)]),
        ),
        (
            "unsatisfied functor requirement",
            target_input_ty(qubit_array_op_ty(FunctorSetValue::Adj), Ty::Prim(Prim::Int)),
        ),
    ];

    assert!(dispatch_layout_types_compatible(&expected, &expected));
    for (label, actual) in cases {
        assert!(
            !dispatch_layout_types_compatible(&actual, &expected),
            "a {label} mismatch must remain incompatible: capability matching applies to functor \
             sets only",
        );
    }
}

#[test]
fn dispatch_layout_accepts_extra_functors_in_nested_values() {
    let nested = |functors| {
        Ty::Tuple(vec![
            Ty::Array(Box::new(qubit_array_op_ty(functors))),
            Ty::Prim(Prim::Int),
        ])
    };
    assert!(dispatch_layout_types_compatible(
        &nested(FunctorSetValue::CtlAdj),
        &nested(FunctorSetValue::Adj),
    ));
    assert!(!dispatch_layout_types_compatible(
        &nested(FunctorSetValue::Adj),
        &nested(FunctorSetValue::CtlAdj),
    ));
}

#[test]
fn dispatch_functors_follow_capability_not_equality() {
    let value = FunctorSet::Value;
    let satisfied = [
        (FunctorSetValue::Empty, FunctorSetValue::Empty),
        (FunctorSetValue::Adj, FunctorSetValue::Empty),
        (FunctorSetValue::Ctl, FunctorSetValue::Empty),
        (FunctorSetValue::CtlAdj, FunctorSetValue::Empty),
        (FunctorSetValue::Adj, FunctorSetValue::Adj),
        (FunctorSetValue::Ctl, FunctorSetValue::Ctl),
        (FunctorSetValue::CtlAdj, FunctorSetValue::Adj),
        (FunctorSetValue::CtlAdj, FunctorSetValue::Ctl),
        (FunctorSetValue::CtlAdj, FunctorSetValue::CtlAdj),
    ];
    for (actual, expected) in satisfied {
        assert!(
            dispatch_functors_compatible(value(actual), value(expected)),
            "{actual:?} should satisfy a requirement of {expected:?}",
        );
    }

    let unsatisfied = [
        (FunctorSetValue::Empty, FunctorSetValue::Adj),
        (FunctorSetValue::Empty, FunctorSetValue::Ctl),
        (FunctorSetValue::Empty, FunctorSetValue::CtlAdj),
        (FunctorSetValue::Adj, FunctorSetValue::Ctl),
        (FunctorSetValue::Adj, FunctorSetValue::CtlAdj),
        (FunctorSetValue::Ctl, FunctorSetValue::Adj),
        (FunctorSetValue::Ctl, FunctorSetValue::CtlAdj),
    ];
    for (actual, expected) in unsatisfied {
        assert!(
            !dispatch_functors_compatible(value(actual), value(expected)),
            "{actual:?} should not satisfy a requirement of {expected:?}",
        );
    }
}
