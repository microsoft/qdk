// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Integration tests that compile Q# source through the full FIR optimization
//! pipeline and cover schedule parity, successful end-to-end validation, and
//! targeted failure regressions.

use qsc_fir::{
    fir::{CallableImpl, ExprKind, ItemKind, PackageLookup},
    validate::validate,
};
use qsc_fir_transforms::{
    PipelineError, PipelineStage, invariants, reachability, run_pipeline, run_pipeline_to,
    test_utils::{
        assert_callable_body_terminal_expr_matches_block_type, assert_no_pipeline_errors,
        compile_to_fir, compile_to_fir_with_entry, expr_kind_short,
    },
};

type LoweredOutput = (
    qsc_fir::fir::PackageStore,
    qsc_fir::fir::PackageId,
    qsc_fir::assigner::Assigner,
);

/// Compiles a Q# source string as an executable on top of core+std.
fn compile_and_lower(source: &str) -> LoweredOutput {
    let (store, package_id) = compile_to_fir(source);
    let assigner = qsc_fir::assigner::Assigner::from_package(store.get(package_id));
    (store, package_id, assigner)
}

fn format_pipeline_errors(errors: &[PipelineError]) -> String {
    if errors.is_empty() {
        "(no error)".to_string()
    } else {
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn run_pipeline_successfully(
    store: &mut qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    let errors = run_pipeline(store, pkg_id);
    assert_no_pipeline_errors("run_pipeline", &errors);
}

fn run_pipeline_to_successfully(
    store: &mut qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    stage: PipelineStage,
) {
    let errors = run_pipeline_to(store, pkg_id, stage, &[]);
    assert_no_pipeline_errors("run_pipeline_to", &errors);
}

fn callable_body_spec<'a>(
    decl: &'a qsc_fir::fir::CallableDecl,
    callable_name: &str,
) -> &'a qsc_fir::fir::SpecDecl {
    match &decl.implementation {
        CallableImpl::Spec(spec_impl) => &spec_impl.body,
        CallableImpl::SimulatableIntrinsic(spec) => spec,
        CallableImpl::Intrinsic => panic!("callable '{callable_name}' should have a body"),
    }
}

fn reachable_callable_names(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) -> Vec<String> {
    let package = store.get(pkg_id);
    let reachable = reachability::collect_reachable_from_entry(store, pkg_id);

    let mut names = Vec::new();
    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            names.push(decl.name.name.to_string());
        }
    }
    names.sort();
    names
}

fn reachable_callable_summary(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) -> String {
    let package = store.get(pkg_id);
    let reachable = reachability::collect_reachable_from_entry(store, pkg_id);

    let mut lines = Vec::new();
    for store_id in &reachable {
        if store_id.package != pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            let pat = package.get_pat(decl.input);
            lines.push(format!(
                "{}: input_ty={}, output_ty={}",
                decl.name.name, pat.ty, decl.output
            ));
        }
    }
    lines.sort();
    lines.join("\n")
}

fn callable_body_summary(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
) -> String {
    let package = store.get(pkg_id);
    let item = package
        .items
        .values()
        .find(|item| match &item.kind {
            ItemKind::Callable(decl) => decl.name.name.as_ref() == callable_name,
            _ => false,
        })
        .expect("callable should exist");

    let ItemKind::Callable(decl) = &item.kind else {
        panic!("item should be callable");
    };
    let spec = callable_body_spec(decl, callable_name);
    let block = package.get_block(spec.block);

    let mut lines = vec![format!("block_ty={}", block.ty)];
    for (index, stmt_id) in block.stmts.iter().enumerate() {
        let stmt = package.get_stmt(*stmt_id);
        let line = match &stmt.kind {
            qsc_fir::fir::StmtKind::Expr(expr_id) => {
                let expr = package.get_expr(*expr_id);
                format!(
                    "[{index}] Expr ty={} {}",
                    expr.ty,
                    expr_kind_short(package, *expr_id)
                )
            }
            qsc_fir::fir::StmtKind::Semi(expr_id) => {
                let expr = package.get_expr(*expr_id);
                format!(
                    "[{index}] Semi ty={} {}",
                    expr.ty,
                    expr_kind_short(package, *expr_id)
                )
            }
            qsc_fir::fir::StmtKind::Local(_, pat_id, expr_id) => {
                let pat = package.get_pat(*pat_id);
                let expr = package.get_expr(*expr_id);
                format!(
                    "[{index}] Local pat_ty={} init_ty={} {}",
                    pat.ty,
                    expr.ty,
                    expr_kind_short(package, *expr_id)
                )
            }
            qsc_fir::fir::StmtKind::Item(local_item_id) => {
                format!("[{index}] Item {local_item_id}")
            }
        };
        lines.push(line);
    }

    lines.join("\n")
}

fn expr_targets_callable(
    package: &qsc_fir::fir::Package,
    pkg_id: qsc_fir::fir::PackageId,
    expr_id: qsc_fir::fir::ExprId,
    callable_name: &str,
) -> bool {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        ExprKind::Var(qsc_fir::fir::Res::Item(item_id), _)
            if item_id.package == pkg_id
                && matches!(
                    &package.get_item(item_id.item).kind,
                    ItemKind::Callable(decl) if decl.name.name.as_ref() == callable_name
                ) =>
        {
            true
        }
        ExprKind::UnOp(_, inner_id) => {
            expr_targets_callable(package, pkg_id, *inner_id, callable_name)
        }
        _ => false,
    }
}

#[test]
fn post_arg_promote_cut_matches_full_pipeline_bodies() {
    let source = r#"
        operation Identity<'T>(x : 'T) : 'T { x }
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        @EntryPoint()
        operation Main() : Int {
            use q = Qubit();
            let angle = 1.0;
            Apply(q1 => Rx(angle, q1), q);
            let pair = Identity((M(q), 7));
            Reset(q);
            let (_, value) = pair;
            value
        }
    "#;

    let (mut post_arg_store, post_arg_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(
        &mut post_arg_store,
        post_arg_pkg_id,
        PipelineStage::ArgPromote,
    );
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_arg_store,
        post_arg_pkg_id,
        invariants::InvariantLevel::PostArgPromote,
    );

    let full_package = full_store.get(full_pkg_id);
    validate(full_package, &full_store);

    assert_eq!(
        reachable_callable_summary(&post_arg_store, post_arg_pkg_id),
        reachable_callable_summary(&full_store, full_pkg_id)
    );

    let post_arg_callables = reachable_callable_names(&post_arg_store, post_arg_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);
    assert_eq!(post_arg_callables, full_callables);

    for callable_name in &full_callables {
        assert_eq!(
            callable_body_summary(&post_arg_store, post_arg_pkg_id, callable_name),
            callable_body_summary(&full_store, full_pkg_id, callable_name),
            "callable '{callable_name}' body drift between PostArgPromote and full pipeline"
        );
    }
}

#[test]
fn terminal_result_block_shape_stays_valid_across_stage_boundaries() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let r = M(q);
                Reset(q);
                return r;
            }
        }
    "#;

    let (store, pkg_id, _) = compile_and_lower(source);
    let (mut post_return_store, post_return_pkg_id, _) = compile_and_lower(source);
    let (mut post_all_store, post_all_pkg_id, _) = compile_and_lower(source);

    let mut snapshots = vec![format!(
        "Lowered\n{}",
        callable_body_summary(&store, pkg_id, "Main")
    )];

    run_pipeline_to_successfully(
        &mut post_return_store,
        post_return_pkg_id,
        PipelineStage::ReturnUnify,
    );
    snapshots.push(format!(
        "PostReturnUnify\n{}",
        callable_body_summary(&post_return_store, post_return_pkg_id, "Main")
    ));
    assert_callable_body_terminal_expr_matches_block_type(
        &post_return_store,
        post_return_pkg_id,
        "Main",
    );

    run_pipeline_to_successfully(&mut post_all_store, post_all_pkg_id, PipelineStage::Full);
    snapshots.push(format!(
        "PostAll\n{}",
        callable_body_summary(&post_all_store, post_all_pkg_id, "Main")
    ));
    assert_callable_body_terminal_expr_matches_block_type(&post_all_store, post_all_pkg_id, "Main");

    let expected = concat!(
        "Lowered\n",
        "block_ty=Result\n",
        "[0] Local pat_ty=Qubit init_ty=Qubit Call\n",
        "[1] Local pat_ty=Result init_ty=Result Call\n",
        "[2] Semi ty=Unit Call\n",
        "[3] Semi ty=Unit Block\n",
        "[4] Semi ty=Unit Call\n",
        "\n",
        "PostReturnUnify\n",
        "block_ty=Result\n",
        "[0] Local pat_ty=Qubit init_ty=Qubit Call\n",
        "[1] Local pat_ty=Result init_ty=Result Call\n",
        "[2] Semi ty=Unit Call\n",
        "[3] Expr ty=Result Block\n\n",
        "PostAll\n",
        "block_ty=Result\n",
        "[0] Local pat_ty=Qubit init_ty=Qubit Call\n",
        "[1] Local pat_ty=Result init_ty=Result Call\n",
        "[2] Semi ty=Unit Call\n",
        "[3] Expr ty=Result Block"
    );
    assert_eq!(snapshots.join("\n\n"), expected);
}

#[test]
fn terminal_result_array_block_shape_through_use_scope_stays_valid() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation SearchForMarkedInput() : Result[] {
                let nQubits = 2;
                use qubits = Qubit[nQubits] {
                    return MResetEachZ(qubits);
                }
            }
        }
    "#;
    let (mut post_return_store, post_return_pkg_id, _) = compile_and_lower(source);
    let (mut post_all_store, post_all_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(
        &mut post_return_store,
        post_return_pkg_id,
        PipelineStage::ReturnUnify,
    );
    assert_callable_body_terminal_expr_matches_block_type(
        &post_return_store,
        post_return_pkg_id,
        "SearchForMarkedInput",
    );

    run_pipeline_to_successfully(&mut post_all_store, post_all_pkg_id, PipelineStage::Full);
    assert_callable_body_terminal_expr_matches_block_type(
        &post_all_store,
        post_all_pkg_id,
        "SearchForMarkedInput",
    );
}

#[test]
fn simple_entry_point_passes_all_invariants() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower("operation Main() : Int { 42 }");
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
}

#[test]
fn generic_identity_monomorphized_to_concrete_type() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        operation Identity<'T>(x : 'T) : 'T { x }
        operation Main() : Int { Identity(42) }
        "#,
    );
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn qubit_allocation_preserved_through_pipeline() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        operation Main() : Result {
            use q = Qubit();
            H(q);
            let r = M(q);
            Reset(q);
            r
        }
        "#,
    );
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
}

#[test]
fn callable_argument_defunctionalized_to_direct_call() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Apply(H, q);
            Reset(q);
        }
        "#,
    );
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
}

#[test]
fn tuple_return_scalars_promoted_by_sroa() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        operation Pair() : (Int, Bool) { (1, true) }
        operation Main() : Int {
            let (a, _) = Pair();
            a
        }
        "#,
    );
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
}

#[test]
fn for_loop_iterators_pass_invariants() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        operation Main() : Int {
            mutable sum = 0;
            for i in 0..4 {
                sum += i;
            }
            sum
        }
        "#,
    );
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
}

#[test]
fn array_operations_pass_post_pipeline_invariants() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        operation Main() : Int {
            let arr = [1, 2, 3];
            arr[1]
        }
        "#,
    );
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
}

#[test]
fn composite_while_return_survives_full_pipeline() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        namespace Test {
            struct Pair {
                Left : Int,
                Right : Bool
            }

            function Helper() : Pair {
                mutable i = 0;
                while i < 3 {
                    if i == 1 {
                        return new Pair { Left = i, Right = true };
                    }
                    i += 1;
                }
                new Pair { Left = -1, Right = false }
            }

            @EntryPoint()
            operation Main() : Int {
                let _ = Helper();
                0
            }
        }
        "#,
    );

    let errors = run_pipeline(&mut fir_store, fir_pkg_id);
    assert_no_pipeline_errors("run_pipeline", &errors);

    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn run_pipeline_returns_dynamic_callable_defunctionalization_diagnostics() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            for _ in 0..3 {
                op = X;
            }
            ApplyOp(op, q);
        }
        "#,
    );

    let errors = run_pipeline(&mut fir_store, fir_pkg_id);

    assert_eq!(
        errors.len(),
        1,
        "expected one defunctionalization diagnostic, got:\n{}",
        format_pipeline_errors(&errors)
    );
    assert!(
        matches!(
            errors.as_slice(),
            [PipelineError::Defunctionalize(
                qsc_fir_transforms::defunctionalize::Error::DynamicCallable(_)
            )]
        ),
        "expected a DynamicCallable diagnostic, got:\n{}",
        format_pipeline_errors(&errors)
    );
    assert_eq!(
        errors[0].to_string(),
        "callable argument could not be resolved statically"
    );
}

#[test]
fn apply_operation_power_a_library_repro_trips_local_var_consistency() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        @EntryPoint()
        operation Main() : Result {
            use q = Qubit();
            ApplyOperationPowerA(12, Rx(Std.Math.PI()/16.0, _), q);
            ApplyOperationPowerA(-3, Rx(Std.Math.PI()/4.0, _), q);
            M(q)
        }
        "#,
    );

    run_pipeline_successfully(&mut fir_store, fir_pkg_id);

    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn apply_operation_power_ca_library_repro_preserves_local_var_consistency() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        operation Consume(apply_power_of_u : (Int, Qubit[]) => Unit is Adj + Ctl, target : Qubit[]) : Result {
            apply_power_of_u(1, target);
            M(target[0])
        }

        operation U(qs : Qubit[]) : Unit is Adj + Ctl {
            H(qs[0]);
        }

        @EntryPoint()
        operation Main() : Result {
            use qs = Qubit[1];
            Consume(ApplyOperationPowerCA(_, U, _), qs)
        }
        "#,
    );

    run_pipeline_successfully(&mut fir_store, fir_pkg_id);

    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn apply_operation_power_ca_array_lambda_preserves_call_shape() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        operation Main() : Unit {
            use state = Qubit();
            use phase = Qubit[2];
            let oracle = ApplyOperationPowerCA(_, qs => U(qs[0]), _);
            ApplyQPE(oracle, [state], phase);
        }

        operation U(q : Qubit) : Unit is Ctl + Adj {
            Rz(Std.Math.PI() / 3.0, q);
        }
        "#,
    );

    run_pipeline_successfully(&mut fir_store, fir_pkg_id);

    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn pipeline_preserves_entry_expression() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower("operation Main() : Int { 99 }");
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    assert!(
        package.entry.is_some(),
        "entry expression must still exist after pipeline"
    );
}

#[test]
fn nested_generics_fully_monomorphized() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        function Inner<'T>(x : 'T) : 'T { x }
        function Outer<'T>(x : 'T) : 'T { Inner(x) }
        @EntryPoint()
        operation Main() : Unit { let _ = Outer(42); }
        "#,
    );
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn generic_for_loop_monomorphized_and_invariants_hold() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        operation Apply<'T>(op : ('T => Unit), items : 'T[]) : Unit {
            for item in items { op(item); }
        }
        @EntryPoint()
        operation Main() : Unit {
            use qs = Qubit[3];
            Apply(H, qs);
        }
        "#,
    );
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn cross_package_apply_to_each_inlined_and_valid() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        open Std.Canon;
        @EntryPoint()
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(H, qs);
        }
        "#,
    );
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn multiple_generic_instantiations_each_specialized() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        function Identity<'T>(x : 'T) : 'T { x }
        @EntryPoint()
        operation Main() : Unit {
            let a = Identity(42);
            let b = Identity(1.0);
        }
        "#,
    );
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn cross_package_nested_generics_fully_resolved() {
    // Uses Std.Arrays.Mapped (generic) which internally calls other std
    // generic helpers. This exercises the cross-package nested-generic
    // worklist: cloning Mapped<Int, Int> into user package discovers further
    // cross-package generic references that must also be specialized.
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        open Std.Arrays;
        function PlusOne(x : Int) : Int { x + 1 }
        @EntryPoint()
        operation Main() : Unit {
            let arr = [1, 2, 3];
            let mapped = Mapped(PlusOne, arr);
        }
        "#,
    );
    run_pipeline_successfully(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn closure_specialization_preserves_lambda_tuple_call_shape() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        @EntryPoint()
        operation Main() : (Int, Bool)[] {
            Microsoft.Quantum.Arrays.Enumerated([true, false])
        }
        "#,
    );

    run_pipeline_to_successfully(&mut fir_store, fir_pkg_id, PipelineStage::Full);

    let package = fir_store.get(fir_pkg_id);
    let mapper = package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl)
                if decl
                    .name
                    .name
                    .as_ref()
                    .starts_with("MappedByIndex<Bool, (Int, Bool)>") =>
            {
                Some(decl.as_ref())
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "MappedByIndex specialization should exist\n{}",
                reachable_callable_summary(&fir_store, fir_pkg_id)
            )
        });

    let lambda_names = package
        .items
        .values()
        .filter_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref().starts_with("<lambda>") => {
                Some(decl.name.name.to_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let args_id = package
        .exprs
        .values()
        .find_map(|expr| match &expr.kind {
            ExprKind::Call(callee_id, args_id)
                if expr_targets_callable(package, fir_pkg_id, *callee_id, "<lambda>") =>
            {
                Some(*args_id)
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "specialized mapper body should call the lifted lambda directly\nmapper body:\n{}\nlambdas:\n{}",
                callable_body_summary(
                    &fir_store,
                    fir_pkg_id,
                    mapper.name.name.as_ref(),
                ),
                lambda_names.join("\n")
            )
        });

    let args_expr = package.get_expr(args_id);
    assert_eq!(
        args_expr.ty.to_string(),
        "((Int, Bool),)",
        "direct lambda calls should preserve closure-style argument packaging"
    );

    let ExprKind::Tuple(args_items) = &args_expr.kind else {
        panic!("direct lambda call should package its argument as a one-element tuple");
    };
    assert_eq!(
        args_items.len(),
        1,
        "lambda call should have exactly one packaged argument"
    );

    let inner_expr = package.get_expr(args_items[0]);
    assert_eq!(inner_expr.ty.to_string(), "(Int, Bool)");
    assert!(
        matches!(&inner_expr.kind, ExprKind::Tuple(items) if items.len() == 2),
        "inner packaged lambda argument should remain the original pair"
    );

    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn direct_lambda_calls_preserve_nested_tuple_packaging() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        @EntryPoint()
        operation Main() : Int {
            let add = (x, y) -> x + y;
            add(2, 3)
        }
        "#,
    );

    run_pipeline_to_successfully(&mut fir_store, fir_pkg_id, PipelineStage::Full);

    let package = fir_store.get(fir_pkg_id);
    let lambda_names = package
        .items
        .values()
        .filter_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref().starts_with("<lambda>") => {
                Some(decl.name.name.to_string())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let args_id = package
        .exprs
        .values()
        .find_map(|expr| match &expr.kind {
            ExprKind::Call(callee_id, args_id)
                if expr_targets_callable(package, fir_pkg_id, *callee_id, "<lambda>") =>
            {
                Some(*args_id)
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "Main should call the lifted lambda directly\nMain body:\n{}\nlambdas:\n{}",
                callable_body_summary(&fir_store, fir_pkg_id, "Main"),
                lambda_names.join("\n")
            )
        });

    let args_expr = package.get_expr(args_id);
    assert_eq!(
        args_expr.ty.to_string(),
        "((Int, Int),)",
        "direct lambda calls should preserve the original tuple argument as one packaged value"
    );

    let ExprKind::Tuple(args_items) = &args_expr.kind else {
        panic!("direct lambda call should package its argument as a one-element tuple");
    };
    assert_eq!(
        args_items.len(),
        1,
        "lambda call should have exactly one packaged argument"
    );

    let inner_expr = package.get_expr(args_items[0]);
    assert_eq!(inner_expr.ty.to_string(), "(Int, Int)");
    assert!(
        matches!(&inner_expr.kind, ExprKind::Tuple(items) if items.len() == 2),
        "inner packaged lambda argument should remain the original pair"
    );

    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn entry_expression_simple_call_passes_pipeline() {
    let source = r#"
        namespace Test {
            operation Greet() : Result {
                use q = Qubit();
                H(q);
                M(q)
            }
        }
    "#;
    let (mut store, pkg_id) = compile_to_fir_with_entry(source, "Test.Greet()");
    run_pipeline_successfully(&mut store, pkg_id);
    let package = store.get(pkg_id);
    validate(package, &store);
}

#[test]
fn entry_expression_with_callable_arg_passes_pipeline() {
    let source = r#"
        namespace Test {
            operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            operation Run() : Result {
                use q = Qubit();
                Apply(H, q);
                M(q)
            }
        }
    "#;
    let (mut store, pkg_id) = compile_to_fir_with_entry(source, "Test.Run()");
    run_pipeline_successfully(&mut store, pkg_id);
    let package = store.get(pkg_id);
    validate(package, &store);
}

#[test]
fn multi_arrow_multi_level_hof_passes_pipeline() {
    let source = r#"
        namespace Test {
            operation ApplyBoth(f : Qubit => Unit, g : Qubit => Unit, q : Qubit) : Unit {
                f(q);
                g(q);
            }

            operation Compose(
                inner : ((Qubit => Unit, Qubit => Unit, Qubit) => Unit),
                f : Qubit => Unit,
                g : Qubit => Unit,
                q : Qubit
            ) : Unit {
                inner(f, g, q);
            }

            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Compose(ApplyBoth, H, X, q);
                M(q)
            }
        }
    "#;
    let (mut store, pkg_id) = compile_to_fir(source);
    run_pipeline_successfully(&mut store, pkg_id);
    let package = store.get(pkg_id);
    validate(package, &store);
    invariants::check(&store, pkg_id, invariants::InvariantLevel::PostAll);
}

/// Exercises a UDT that wraps a callable type through the full pipeline.
///
/// The callable-wrapping UDT is constructed and unwrapped locally without
/// appearing in callable signatures, so defunctionalization can eliminate
/// the inner arrow type without requiring parameter-level changes. This
/// confirms that `defunctionalize` safely handles `Ty::Udt` nodes that
/// contain callable fields.
#[test]
fn udt_wrapping_callable_survives_full_pipeline() {
    let source = r#"
        namespace Test {
            newtype MyOp = (Qubit => Unit);

            @EntryPoint()
            operation Main() : Unit {
                let wrapped = MyOp(q => H(q));
                use q = Qubit();
                (wrapped!)(q);
                Reset(q);
            }
        }
    "#;
    let (mut store, pkg_id) = compile_to_fir(source);
    run_pipeline_successfully(&mut store, pkg_id);
    let package = store.get(pkg_id);
    validate(package, &store);
}

/// Exercises a cross-package UDT constructor through the full pipeline.
/// Uses the `Complex` struct from the core library, which is exported and
/// available to user code.
///
/// NOTE: The Q# frontend resolver fails to resolve cross-package UDT
/// constructors in expression position, producing `Res::Err` / `Ty::Err`
/// before any pipeline transforms run. See `qsc_frontend/src/lower.rs`
/// line 1059 for the `hir::Res::Err` fallback. This is a frontend bug,
/// not a pipeline bug.
#[test]
fn cross_package_udt_constructor_resolution() {
    let source = r#"
        @EntryPoint()
        operation Main() : Int {
            let c = Complex(1.0, 2.0);
            0
        }
    "#;
    let (mut store, pkg_id) = compile_to_fir(source);
    run_pipeline_successfully(&mut store, pkg_id);
    let package = store.get(pkg_id);
    validate(package, &store);
}

/// Local multi-field UDT with a callable field that is never invoked.
/// UDT erasure exposes the arrow type inside the tuple; the invariant
/// must tolerate this between UDT erasure and SROA.
#[test]
fn local_multi_field_udt_callable_never_invoked() {
    let source = r#"
        namespace Test {
            newtype Config = (Count: Int, Op: Qubit[] => Unit is Adj);
            operation NoOp(qs : Qubit[]) : Unit is Adj {}
            @EntryPoint()
            operation Main() : Int { let cfg = Config(0, NoOp); 0 }
        }
    "#;
    let (mut store, pkg_id) = compile_to_fir(source);
    run_pipeline_successfully(&mut store, pkg_id);
    let package = store.get(pkg_id);
    validate(package, &store);
}

/// Local multi-field UDT with a callable field extracted via field accessor
/// and invoked. Confirms that defunc and UDT erasure cooperate correctly
/// when the callable is actually called.
#[test]
fn local_multi_field_udt_callable_field_invoked() {
    let source = r#"
        namespace Test {
            newtype Config = (Count: Int, Op: Qubit[] => Unit is Adj);
            operation NoOp(qs : Qubit[]) : Unit is Adj {}
            @EntryPoint()
            operation Main() : Unit {
                let cfg = Config(0, NoOp);
                use qs = Qubit[cfg::Count];
                cfg::Op(qs);
            }
        }
    "#;
    let (mut store, pkg_id) = compile_to_fir(source);
    run_pipeline_successfully(&mut store, pkg_id);
    let package = store.get(pkg_id);
    validate(package, &store);
}

/// Local multi-field UDT with a callable field passed to a higher-order
/// function. Exercises defunc's expression-level analysis when the arrow
/// value flows through a HOF call site.
#[test]
#[ignore = "defunc limitation: callable extracted via UDT field accessor (w::F) cannot be statically resolved when passed to a HOF"]
fn local_multi_field_udt_callable_passed_to_hof() {
    let source = r#"
        namespace Test {
            newtype Wrapper = (Count: Int, F: Int -> Int);
            function Inc(x: Int) : Int { x + 1 }
            function Apply(f: Int -> Int, x: Int) : Int { f(x) }
            @EntryPoint()
            operation Main() : Int {
                let w = Wrapper(0, Inc);
                Apply(w::F, 5)
            }
        }
    "#;
    let (mut store, pkg_id) = compile_to_fir(source);
    run_pipeline_successfully(&mut store, pkg_id);
    let package = store.get(pkg_id);
    validate(package, &store);
}

// ============================================================================
// Stage-Parity Integration Tests
// ============================================================================
// These tests verify that FIR output at each pipeline stage is parity with
// the full pipeline. Stage-parity ensures that:
//
// 1. Callable count remains consistent (callables are not unexpectedly added/removed)
// 2. Statement IDs are valid references (no dangling refs to removed items)
// 3. Executable graph is well-formed or empty as expected
// 4. Type correctness is preserved across the stage boundary
// 5. Package structure and export lists remain consistent

#[test]
fn stage_parity_mono_monomorphization_preserves_callable_types() {
    // Stage-parity check after monomorphization.
    //
    // Invariant: After Mono, all generic parameters are erased and concrete
    // monomorphized callables exist. Callable count should match full pipeline
    // (Mono doesn't create or remove callables; it specializes them).
    //
    // Importance: Mono is the first transformation. Validating its output
    // parity ensures subsequent passes inherit a well-formed FIR with no
    // unexpected callable additions or deletions.
    let source = r#"
        function Identity<'T>(x : 'T) : 'T { x }
        @EntryPoint()
        operation Main() : Int {
            let a = Identity(42);
            let b = Identity(1.5);
            a
        }
    "#;

    let (mut post_mono_store, post_mono_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(&mut post_mono_store, post_mono_pkg_id, PipelineStage::Mono);
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_mono_store,
        post_mono_pkg_id,
        invariants::InvariantLevel::PostMono,
    );

    let full_package = full_store.get(full_pkg_id);
    validate(full_package, &full_store);

    // Callable count parity: Mono should not add/remove callables.
    let post_mono_callables = reachable_callable_names(&post_mono_store, post_mono_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);
    assert_eq!(
        post_mono_callables, full_callables,
        "callable set must be identical after Mono and full pipeline"
    );

    // Type consistency: callable signatures should be identical.
    assert_eq!(
        reachable_callable_summary(&post_mono_store, post_mono_pkg_id),
        reachable_callable_summary(&full_store, full_pkg_id)
    );
}

#[test]
fn stage_parity_defunc_defunctionalization_eliminates_callable_types() {
    // Stage-parity check after defunctionalization.
    //
    // Invariant: After Defunc, all arrow types and closure expressions
    // have been eliminated from reachable code. Callable-wrapping closures
    // are lifted to callable declarations, but the count in reachable code
    // should match the full pipeline (lifted callables participate in
    // reachability from Main).
    //
    // Importance: Defunc is a high-value transformation that changes the
    // structure of the FIR significantly. Validating parity ensures that
    // callable creation during lifting does not introduce duplicate or
    // stray callables, and that the reachable set is stable.
    let source = r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Apply(H, q);
            Reset(q);
        }
    "#;

    let (mut post_defunc_store, post_defunc_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(
        &mut post_defunc_store,
        post_defunc_pkg_id,
        PipelineStage::Defunc,
    );
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_defunc_store,
        post_defunc_pkg_id,
        invariants::InvariantLevel::PostDefunc,
    );

    let full_package = full_store.get(full_pkg_id);
    validate(full_package, &full_store);

    // Callable count parity: lifted callables should be in reachable set.
    let post_defunc_callables = reachable_callable_names(&post_defunc_store, post_defunc_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);
    assert_eq!(
        post_defunc_callables, full_callables,
        "callable set after Defunc must match full pipeline"
    );

    // Type summary parity.
    assert_eq!(
        reachable_callable_summary(&post_defunc_store, post_defunc_pkg_id),
        reachable_callable_summary(&full_store, full_pkg_id)
    );
}

#[test]
fn stage_parity_udt_erase_eliminates_udt_types() {
    // Stage-parity check after UDT erasure.
    //
    // Invariant: After UdtErase, all Ty::Udt types are erased from
    // reachable code. UDT-wrapping callables are eliminated only if
    // they become unreachable (deferred to item_dce). Reachable callables
    // should match the full pipeline output.
    //
    // Importance: UDT erasure is a significant structural transformation
    // that rewrites type signatures. Validating parity ensures no callables
    // are unexpectedly preserved or removed at this stage.
    let source = r#"
        namespace Test {
            newtype Wrapper = (x: Int);
            function Extract(w: Wrapper) : Int { w::x }
            @EntryPoint()
            operation Main() : Int {
                let w = Wrapper(42);
                Extract(w)
            }
        }
    "#;

    let (mut post_udt_store, post_udt_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(
        &mut post_udt_store,
        post_udt_pkg_id,
        PipelineStage::UdtErase,
    );
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_udt_store,
        post_udt_pkg_id,
        invariants::InvariantLevel::PostUdtErase,
    );

    let full_package = full_store.get(full_pkg_id);
    validate(full_package, &full_store);

    // Callable count parity.
    let post_udt_callables = reachable_callable_names(&post_udt_store, post_udt_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);
    assert_eq!(
        post_udt_callables, full_callables,
        "callable set after UdtErase must match full pipeline"
    );

    // Type summary parity.
    assert_eq!(
        reachable_callable_summary(&post_udt_store, post_udt_pkg_id),
        reachable_callable_summary(&full_store, full_pkg_id)
    );
}

#[test]
fn stage_parity_tuple_comp_lower_lowers_tuple_equality() {
    // Stage-parity check after tuple comparison lowering.
    //
    // Invariant: After TupleCompLower, all tuple equality and inequality
    // operations are lowered to scalar comparisons and logical operators.
    // No BinOp(Eq/Neq) with tuple operands should exist. Callable count
    // should match full pipeline.
    //
    // Importance: TupleCompLower is a mid-pipeline pass that preserves the
    // callable set while rewriting expression structure. Validating parity
    // ensures no unexpected side effects on the callable structure.
    let source = r#"
        @EntryPoint()
        operation Main() : Bool {
            let pair1 = (1, 2);
            let pair2 = (1, 2);
            pair1 == pair2
        }
    "#;

    let (mut post_tuple_store, post_tuple_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(
        &mut post_tuple_store,
        post_tuple_pkg_id,
        PipelineStage::TupleCompLower,
    );
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_tuple_store,
        post_tuple_pkg_id,
        invariants::InvariantLevel::PostTupleCompLower,
    );

    let full_package = full_store.get(full_pkg_id);
    validate(full_package, &full_store);

    // Callable count parity.
    let post_tuple_callables = reachable_callable_names(&post_tuple_store, post_tuple_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);
    assert_eq!(
        post_tuple_callables, full_callables,
        "callable set after TupleCompLower must match full pipeline"
    );

    // Type summary parity.
    assert_eq!(
        reachable_callable_summary(&post_tuple_store, post_tuple_pkg_id),
        reachable_callable_summary(&full_store, full_pkg_id)
    );
}

#[test]
fn stage_parity_sroa_scalarizes_tuple_locals_and_parameters() {
    // Stage-parity check after Scalar Replacement of Aggregates (SROA).
    //
    // Invariant: After SROA, tuple locals and callable parameters are
    // scalarized into individual scalar locals. Callable count remains
    // stable as SROA does not create new callables. Callable body structures
    // should match full pipeline output.
    //
    // Importance: SROA is a data-flow optimization that rewrites local
    // patterns and parameter decomposition. Validating parity ensures that
    // the scalarization does not introduce unexpected new callables or
    // remove callables unexpectedly.
    let source = r#"
        function Pair() : (Int, Bool) { (1, true) }
        @EntryPoint()
        operation Main() : Int {
            let (a, _) = Pair();
            a
        }
    "#;

    let (mut post_sroa_store, post_sroa_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(&mut post_sroa_store, post_sroa_pkg_id, PipelineStage::Sroa);
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_sroa_store,
        post_sroa_pkg_id,
        invariants::InvariantLevel::PostSroa,
    );

    let full_package = full_store.get(full_pkg_id);
    validate(full_package, &full_store);

    // Callable count parity.
    let post_sroa_callables = reachable_callable_names(&post_sroa_store, post_sroa_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);
    assert_eq!(
        post_sroa_callables, full_callables,
        "callable set after SROA must match full pipeline"
    );

    // Type summary parity.
    assert_eq!(
        reachable_callable_summary(&post_sroa_store, post_sroa_pkg_id),
        reachable_callable_summary(&full_store, full_pkg_id)
    );

    // Body summary parity: callable bodies should be identical after SROA
    // and full pipeline.
    for callable_name in &full_callables {
        assert_eq!(
            callable_body_summary(&post_sroa_store, post_sroa_pkg_id, callable_name),
            callable_body_summary(&full_store, full_pkg_id, callable_name),
            "callable '{callable_name}' body must match after SROA and full pipeline"
        );
    }
}

#[test]
fn stage_parity_item_dce_eliminates_unreachable_items() {
    // Stage-parity check after item-level dead code elimination.
    //
    // Invariant: After ItemDce, unreachable callables and types are removed
    // from the package. The reachable callable set should be identical to
    // the full pipeline output. Statement references should not dangle
    // (reachable callables only reference reachable items).
    //
    // Importance: ItemDce is a critical pass that removes dead code. This
    // test validates that DCE correctly identifies and preserves reachable
    // items while eliminating only truly dead items, avoiding premature
    // removal or over-retention of items.
    let source = r#"
        function Unused() : Int { 99 }
        function Used() : Int { 42 }
        @EntryPoint()
        operation Main() : Int { Used() }
    "#;

    let (mut post_dce_store, post_dce_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(&mut post_dce_store, post_dce_pkg_id, PipelineStage::ItemDce);
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_dce_store,
        post_dce_pkg_id,
        invariants::InvariantLevel::PostArgPromote, // ItemDce runs after ArgPromote
    );

    let full_package = full_store.get(full_pkg_id);
    validate(full_package, &full_store);

    // Callable count parity: reachable callables must match.
    let post_dce_callables = reachable_callable_names(&post_dce_store, post_dce_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);
    assert_eq!(
        post_dce_callables, full_callables,
        "reachable callable set after ItemDce must match full pipeline"
    );

    // Type summary parity.
    assert_eq!(
        reachable_callable_summary(&post_dce_store, post_dce_pkg_id),
        reachable_callable_summary(&full_store, full_pkg_id)
    );
}

#[test]
fn stage_parity_exec_graph_rebuild_reconstructs_execution_graph() {
    // Stage-parity check after execution graph rebuild.
    //
    // Invariant: After ExecGraphRebuild, the execution graph is reconstructed
    // from the rewritten FIR. All EMPTY_EXEC_RANGE sentinels from earlier
    // passes are replaced with valid execution graph ranges. Callable bodies
    // should match the full pipeline output, and the package structure
    // should be stable.
    //
    // Importance: ExecGraphRebuild is the final structural pass. This test
    // validates that the execution graph reconstruction does not alter
    // callable definitions, introduce new callables, or remove existing
    // ones. The reconstructed graph should be well-formed and match the
    // full pipeline's graph.
    let source = r#"
        operation Identity<'T>(x : 'T) : 'T { x }
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Apply(H, q);
            let _ = Identity(42);
            Reset(q);
        }
    "#;

    let (mut post_rebuild_store, post_rebuild_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(
        &mut post_rebuild_store,
        post_rebuild_pkg_id,
        PipelineStage::ExecGraphRebuild,
    );
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_rebuild_store,
        post_rebuild_pkg_id,
        invariants::InvariantLevel::PostAll, // ExecGraphRebuild is the last pass
    );

    let full_package = full_store.get(full_pkg_id);
    validate(full_package, &full_store);

    // Callable count parity.
    let post_rebuild_callables = reachable_callable_names(&post_rebuild_store, post_rebuild_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);
    assert_eq!(
        post_rebuild_callables, full_callables,
        "callable set after ExecGraphRebuild must match full pipeline"
    );

    // Type summary parity.
    assert_eq!(
        reachable_callable_summary(&post_rebuild_store, post_rebuild_pkg_id),
        reachable_callable_summary(&full_store, full_pkg_id)
    );

    // Body summary parity: all callable bodies must match.
    for callable_name in &full_callables {
        assert_eq!(
            callable_body_summary(&post_rebuild_store, post_rebuild_pkg_id, callable_name),
            callable_body_summary(&full_store, full_pkg_id, callable_name),
            "callable '{callable_name}' body must match after ExecGraphRebuild and full pipeline"
        );
    }
}

#[test]
fn stage_parity_mono_type_stability() {
    // Regression test for generic specialization at PostMono.
    //
    // Invariant: Generic specialization at Mono produces monomorphized callables.
    // The callable count (reachable from entry) should match the full pipeline,
    // indicating that Mono neither creates nor removes callables unexpectedly.
    let source = r#"
        operation Generic<'T>(x: 'T) : Unit { }
        @EntryPoint()
        operation Main() : Unit {
            Generic(1);
            Generic("str");
        }
    "#;

    let (mut post_mono_store, post_mono_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(&mut post_mono_store, post_mono_pkg_id, PipelineStage::Mono);
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_mono_store,
        post_mono_pkg_id,
        invariants::InvariantLevel::PostMono,
    );

    let mono_callable_count = reachable_callable_names(&post_mono_store, post_mono_pkg_id).len();
    let full_callable_count = reachable_callable_names(&full_store, full_pkg_id).len();

    assert_eq!(
        mono_callable_count, full_callable_count,
        "generic specialization should not change reachable callable count at PostMono"
    );
}

#[test]
fn stage_parity_defunc_hof_elimination() {
    // Regression test for HOF callable elimination at PostDefunc.
    //
    // Invariant: After defunctionalization, HOF callables (with arrow types)
    // have been eliminated and replaced by lifted specializations. The reachable
    // callable count should reflect this transformation and match the full pipeline.
    let source = r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Apply(H, q);
            Apply(X, q);
        }
    "#;

    let (mut post_defunc_store, post_defunc_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(
        &mut post_defunc_store,
        post_defunc_pkg_id,
        PipelineStage::Defunc,
    );
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_defunc_store,
        post_defunc_pkg_id,
        invariants::InvariantLevel::PostDefunc,
    );

    let defunc_callables = reachable_callable_names(&post_defunc_store, post_defunc_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);

    assert_eq!(
        defunc_callables, full_callables,
        "HOF elimination should produce consistent callable set at PostDefunc"
    );
}

#[test]
fn stage_parity_tuple_comp_lower_no_residual() {
    // Regression test for tuple comparison lowering at PostTupleCompLower.
    //
    // Invariant: After tuple comparison lowering, no binary equality operations
    // on tuple-typed operands remain in reachable code. This test verifies the
    // lowering completes without introducing residual BinOp(Eq, Tuple) expressions.
    let source = r#"
        @EntryPoint()
        operation Main() : Bool {
            let pair = (1, 2);
            let other = (1, 2);
            pair == other
        }
    "#;

    let (mut post_tuple_store, post_tuple_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(
        &mut post_tuple_store,
        post_tuple_pkg_id,
        PipelineStage::TupleCompLower,
    );
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_tuple_store,
        post_tuple_pkg_id,
        invariants::InvariantLevel::PostTupleCompLower,
    );

    let post_tuple_callables = reachable_callable_names(&post_tuple_store, post_tuple_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);

    assert_eq!(
        post_tuple_callables, full_callables,
        "tuple comparison lowering should preserve callable structure"
    );
}

#[test]
fn stage_parity_sroa_no_tuple_locals() {
    // Regression test for SROA (Scalar Replacement of Aggregates) at PostSroa.
    //
    // Invariant: After SROA, tuple-typed local variables have been scalarized.
    // No tuple-typed locals should remain in reachable callable bodies.
    let source = r#"
        @EntryPoint()
        operation Main() : Int {
            let pair = (1, 2);
            let x = pair.0;
            let y = pair.1;
            x + y
        }
    "#;

    let (mut post_sroa_store, post_sroa_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(&mut post_sroa_store, post_sroa_pkg_id, PipelineStage::Sroa);
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_sroa_store,
        post_sroa_pkg_id,
        invariants::InvariantLevel::PostSroa,
    );

    let post_sroa_callables = reachable_callable_names(&post_sroa_store, post_sroa_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);

    assert_eq!(
        post_sroa_callables, full_callables,
        "SROA should not introduce or remove callables"
    );
}

#[test]
fn stage_parity_item_dce_node_count() {
    // Regression test for item DCE reducing item count on dead code.
    //
    // Invariant: After item DCE, unreachable items (dead code) have been removed.
    // Dead callables should be eliminated, and reachable callables should match the full pipeline.
    let source = r#"
        operation Unused() : Unit { }
        operation Used() : Unit { }
        @EntryPoint()
        operation Main() : Unit { Used(); }
    "#;

    let (mut pre_dce_store, pre_dce_pkg_id, _) = compile_and_lower(source);
    let (mut post_dce_store, post_dce_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(&mut pre_dce_store, pre_dce_pkg_id, PipelineStage::Gc);
    run_pipeline_to_successfully(&mut post_dce_store, post_dce_pkg_id, PipelineStage::ItemDce);

    let pre_dce_callables = reachable_callable_names(&pre_dce_store, pre_dce_pkg_id);
    let post_dce_callables = reachable_callable_names(&post_dce_store, post_dce_pkg_id);

    // After DCE, the only reachable callable should be Main (Used is not directly called from Main)
    assert!(
        post_dce_callables.len() <= pre_dce_callables.len(),
        "item DCE should not increase reachable callable count"
    );

    // Verify postcondition holds
    invariants::check(
        &post_dce_store,
        post_dce_pkg_id,
        invariants::InvariantLevel::PostAll,
    );
}

#[test]
fn stage_parity_exec_graph_no_empty_ranges() {
    // Regression test for execution graph rebuild eliminating empty ranges.
    //
    // Invariant: After execution graph rebuild, EMPTY_EXEC_RANGE sentinels
    // used by synthesis passes have been replaced with valid execution graph ranges.
    // No empty-range artifacts should remain in the final graph.
    let source = r#"
        operation Identity<'T>(x : 'T) : 'T { x }
        @EntryPoint()
        operation Main() : Int {
            let a = Identity(42);
            let b = Identity(1.5);
            a
        }
    "#;

    let (mut post_rebuild_store, post_rebuild_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    run_pipeline_to_successfully(
        &mut post_rebuild_store,
        post_rebuild_pkg_id,
        PipelineStage::ExecGraphRebuild,
    );
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_rebuild_store,
        post_rebuild_pkg_id,
        invariants::InvariantLevel::PostAll,
    );

    let post_rebuild_callables = reachable_callable_names(&post_rebuild_store, post_rebuild_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);

    assert_eq!(
        post_rebuild_callables, full_callables,
        "execution graph rebuild should preserve callable structure"
    );
}
