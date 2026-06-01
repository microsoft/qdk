// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Integration tests that compile Q# source through the full FIR optimization
//! pipeline and cover schedule parity, successful end-to-end validation, and
//! targeted failure regressions.

use qsc_eval::val::Value;
use qsc_fir::{
    fir::{ExecGraphConfig, ExprKind, ItemKind, PackageLookup, StoreItemId},
    validate::validate,
};
use qsc_fir_transforms::{
    PipelineError, PipelineStage, invariants, reachability, run_pipeline_to_with_diagnostics,
    run_pipeline_with_diagnostics,
    test_utils::{
        assert_callable_body_terminal_expr_matches_block_type, assert_full_pipeline_succeeds,
        assert_no_pipeline_errors, assert_pipeline_stage_succeeds, compile_to_fir,
        compile_to_fir_with_entry, compile_to_fir_with_library, format_callable_body_summary,
        format_pipeline_errors, format_reachable_callable_summary,
    },
};

type LoweredOutput = (
    qsc_fir::fir::PackageStore,
    qsc_fir::fir::PackageId,
    qsc_fir::assigner::Assigner,
);

const EXCESSIVE_SPECIALIZATIONS_SOURCE: &str = r#"
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
"#;

/// Compiles a Q# source string as an executable on top of core+std.
fn compile_and_lower(source: &str) -> LoweredOutput {
    let (store, package_id) = compile_to_fir(source);
    let assigner = qsc_fir::assigner::Assigner::from_package(store.get(package_id));
    (store, package_id, assigner)
}

fn run_pipeline_successfully(
    store: &mut qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) {
    assert_full_pipeline_succeeds("pipeline_integration::run_pipeline(Full)", store, pkg_id);
}

fn run_pipeline_to_successfully(
    store: &mut qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    stage: PipelineStage,
) {
    let context = format!("pipeline_integration::run_pipeline_to({stage:?})");
    assert_pipeline_stage_succeeds(&context, store, pkg_id, stage);
}

fn eval_entry_value(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
) -> Result<Value, String> {
    use qsc_eval::backend::{SparseSim, TracingBackend};
    use qsc_eval::output::GenericReceiver;

    let package = store.get(pkg_id);
    let entry_graph = package.entry_exec_graph.clone();
    let mut env = qsc_eval::Env::default();
    let mut sim = SparseSim::new();
    let mut output = Vec::<u8>::new();
    let mut receiver = GenericReceiver::new(&mut output);
    qsc_eval::eval(
        pkg_id,
        Some(42),
        entry_graph,
        ExecGraphConfig::NoDebug,
        store,
        &mut env,
        &mut TracingBackend::no_tracer(&mut sim),
        &mut receiver,
    )
    .map_err(|(err, _frames)| format!("{err:?}"))
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

fn package_has_callable_named(
    store: &qsc_fir::fir::PackageStore,
    pkg_id: qsc_fir::fir::PackageId,
    callable_name: &str,
) -> bool {
    let package = store.get(pkg_id);
    package.items.values().any(|item| match &item.kind {
        ItemKind::Callable(decl) => decl.name.name.as_ref() == callable_name,
        _ => false,
    })
}

fn warning_is_excessive_specializations(warning: &PipelineError) -> bool {
    matches!(
        warning,
        PipelineError::Defunctionalize(
            qsc_fir_transforms::defunctionalize::Error::ExcessiveSpecializations(..)
        )
    )
}

fn store_with_removed_pinned_callable() -> (
    qsc_fir::fir::PackageStore,
    qsc_fir::fir::PackageId,
    StoreItemId,
) {
    let (mut store, pkg_id) = compile_to_fir(
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int { 42 }
            operation Pinned() : Int { 99 }
        }
        "#,
    );
    let pinned_item = {
        let package = store.get(pkg_id);
        package
            .items
            .iter()
            .find_map(|(item_id, item)| match &item.kind {
                ItemKind::Callable(decl) if decl.name.name.as_ref() == "Pinned" => Some(item_id),
                _ => None,
            })
            .expect("Pinned callable should exist")
    };
    let pinned_store_id = StoreItemId::from((pkg_id, pinned_item));
    store.get_mut(pkg_id).items.remove(pinned_item);
    (store, pkg_id, pinned_store_id)
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
fn post_tuple_decompose2_cut_matches_full_pipeline_bodies() {
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

    let (mut post_tuple_decompose2_store, post_tuple_decompose2_pkg_id, _) =
        compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);

    // `TupleDecompose2` is the final optimization stage (it runs after `arg_promote`);
    // the trailing `Gc`/`ItemDce`/`ExecGraphRebuild` stages do not alter
    // reachable callable bodies, so the post-`TupleDecompose2` cut must match the full
    // pipeline.
    run_pipeline_to_successfully(
        &mut post_tuple_decompose2_store,
        post_tuple_decompose2_pkg_id,
        PipelineStage::TupleDecompose2,
    );
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(
        &post_tuple_decompose2_store,
        post_tuple_decompose2_pkg_id,
        invariants::InvariantLevel::PostArgPromote,
    );

    let full_package = full_store.get(full_pkg_id);
    validate(full_package, &full_store);

    assert_eq!(
        format_reachable_callable_summary(
            &post_tuple_decompose2_store,
            post_tuple_decompose2_pkg_id
        ),
        format_reachable_callable_summary(&full_store, full_pkg_id),
        "post-TupleDecompose2 reachable callable summary should match the full pipeline"
    );

    let post_tuple_decompose2_callables =
        reachable_callable_names(&post_tuple_decompose2_store, post_tuple_decompose2_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);
    assert_eq!(post_tuple_decompose2_callables, full_callables);

    for callable_name in &full_callables {
        assert_eq!(
            format_callable_body_summary(
                &post_tuple_decompose2_store,
                post_tuple_decompose2_pkg_id,
                callable_name
            ),
            format_callable_body_summary(&full_store, full_pkg_id, callable_name),
            "callable '{callable_name}' body drift between post-TupleDecompose2 and full pipeline"
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
        format_callable_body_summary(&store, pkg_id, "Main")
    )];

    run_pipeline_to_successfully(
        &mut post_return_store,
        post_return_pkg_id,
        PipelineStage::ReturnUnify,
    );
    snapshots.push(format!(
        "PostReturnUnify\n{}",
        format_callable_body_summary(&post_return_store, post_return_pkg_id, "Main")
    ));
    assert_callable_body_terminal_expr_matches_block_type(
        &post_return_store,
        post_return_pkg_id,
        "Main",
    );

    run_pipeline_to_successfully(&mut post_all_store, post_all_pkg_id, PipelineStage::Full);
    snapshots.push(format!(
        "PostAll\n{}",
        format_callable_body_summary(&post_all_store, post_all_pkg_id, "Main")
    ));
    assert_callable_body_terminal_expr_matches_block_type(&post_all_store, post_all_pkg_id, "Main");

    // The Lowered shape is identical in both modes; the post-pipeline shape
    // reflects the flag strategy prepending `__has_returned`/`__ret_val`
    // bindings and emitting the merge as a `Var` read.
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
        "[0] Local pat_ty=Bool init_ty=Bool Lit(Bool(false))\n",
        "[1] Local pat_ty=Result init_ty=Result Lit(Result(Zero))\n",
        "[2] Local pat_ty=Qubit init_ty=Qubit Call\n",
        "[3] Local pat_ty=Result init_ty=Result Call\n",
        "[4] Semi ty=Unit Call\n",
        "[5] Semi ty=Unit Block\n",
        "[6] Semi ty=Unit If\n",
        "[7] Expr ty=Result Var\n\n",
        "PostAll\n",
        "block_ty=Result\n",
        "[0] Local pat_ty=Bool init_ty=Bool Lit(Bool(false))\n",
        "[1] Local pat_ty=Result init_ty=Result Lit(Result(Zero))\n",
        "[2] Local pat_ty=Qubit init_ty=Qubit Call\n",
        "[3] Local pat_ty=Result init_ty=Result Call\n",
        "[4] Semi ty=Unit Call\n",
        "[5] Semi ty=Unit Block\n",
        "[6] Semi ty=Unit If\n",
        "[7] Expr ty=Result Var"
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
fn tuple_return_scalars_promoted_by_tuple_decompose() {
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

    let result = run_pipeline_with_diagnostics(&mut fir_store, fir_pkg_id);
    assert_no_pipeline_errors(
        "pipeline_integration::composite_while_return_survives_full_pipeline::run_pipeline(Full)",
        &result.errors,
    );

    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn mixed_full_pipeline_semantic_regression_preserves_result() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(
        r#"
        namespace Test {
            struct Pair { A : Int, B : Int }

            function Id<'T>(x : 'T) : 'T { x }
            function SumPair(pair : Pair) : Int { pair.A + pair.B }
            function ApplyInt(f : Int -> Int, value : Int) : Int { f(value) }

            function Adjust(value : Int) : Int {
                if value == 0 {
                    return 99;
                }
                value + 1
            }

            @EntryPoint()
            operation Main() : Int {
                let base = new Pair { A = Id(2), B = 3 };
                let updated = new Pair { ...base, B = 4 };
                let tuple = (updated.A, updated.B);
                let tupleMatched = tuple == (2, 4);
                let value = ApplyInt(Adjust, SumPair(updated));
                if tupleMatched {
                    return value;
                }
                0
            }
        }
        "#,
    );

    run_pipeline_successfully(&mut fir_store, fir_pkg_id);

    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);

    let value = eval_entry_value(&fir_store, fir_pkg_id).expect("entry evaluation should succeed");
    assert_eq!(value, Value::Int(7));
}

#[test]
fn excessive_specializations_warning_reaches_full_pipeline() {
    let (mut fir_store, fir_pkg_id, _) = compile_and_lower(EXCESSIVE_SPECIALIZATIONS_SOURCE);

    let result = run_pipeline_with_diagnostics(&mut fir_store, fir_pkg_id);

    assert!(
        result.errors.is_empty(),
        "expected no fatal pipeline errors, got:\n{}",
        format_pipeline_errors(&result.errors)
    );
    assert_eq!(
        result.warnings.len(),
        1,
        "expected one warning, got:\n{}",
        format_pipeline_errors(&result.warnings)
    );
    assert!(
        warning_is_excessive_specializations(&result.warnings[0]),
        "expected ExcessiveSpecializations warning, got:\n{}",
        format_pipeline_errors(&result.warnings)
    );

    let package = fir_store.get(fir_pkg_id);
    validate(package, &fir_store);
    invariants::check(&fir_store, fir_pkg_id, invariants::InvariantLevel::PostAll);
}

#[test]
fn run_pipeline_with_diagnostics_returns_dynamic_callable_as_fatal_error() {
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

    let result = run_pipeline_with_diagnostics(&mut fir_store, fir_pkg_id);

    assert!(
        result.warnings.is_empty(),
        "expected no warnings, got:\n{}",
        format_pipeline_errors(&result.warnings)
    );
    assert!(
        matches!(
            result.errors.as_slice(),
            [PipelineError::Defunctionalize(
                qsc_fir_transforms::defunctionalize::Error::DynamicCallable(_)
            )]
        ),
        "expected DynamicCallable fatal error, got:\n{}",
        format_pipeline_errors(&result.errors)
    );
}

#[test]
fn run_pipeline_to_missing_pinned_item_reports_diagnostic() {
    let (mut store, pkg_id, pinned_store_id) = store_with_removed_pinned_callable();

    let result = run_pipeline_to_with_diagnostics(
        &mut store,
        pkg_id,
        PipelineStage::ItemDce,
        &[pinned_store_id],
    );

    assert!(
        result.warnings.is_empty(),
        "expected no warnings, got:\n{}",
        format_pipeline_errors(&result.warnings)
    );
    assert!(
        matches!(
            result.errors.as_slice(),
            [PipelineError::MissingPinnedItem(item_id)] if *item_id == pinned_store_id
        ),
        "expected MissingPinnedItem diagnostic, got:\n{}",
        format_pipeline_errors(&result.errors)
    );
}

#[test]
fn run_pipeline_to_missing_pinned_item_reports_diagnostic_before_exec_rebuild() {
    let (mut store, pkg_id, pinned_store_id) = store_with_removed_pinned_callable();

    let result = run_pipeline_to_with_diagnostics(
        &mut store,
        pkg_id,
        PipelineStage::ExecGraphRebuild,
        &[pinned_store_id],
    );

    assert!(
        result.warnings.is_empty(),
        "expected no warnings before exec graph rebuild, got:\n{}",
        format_pipeline_errors(&result.warnings)
    );
    assert!(
        matches!(
            result.errors.as_slice(),
            [PipelineError::MissingPinnedItem(item_id)] if *item_id == pinned_store_id
        ),
        "expected MissingPinnedItem diagnostic before exec graph rebuild, got:\n{}",
        format_pipeline_errors(&result.errors)
    );
}

#[test]
fn run_pipeline_to_non_callable_pinned_item_reports_diagnostic() {
    let (mut store, pkg_id) = compile_to_fir(
        r#"
        namespace Test {
            newtype Marker = Int;
            @EntryPoint()
            operation Main() : Int { 42 }
        }
        "#,
    );
    let pinned_item = {
        let package = store.get(pkg_id);
        package
            .items
            .iter()
            .find_map(|(item_id, item)| match &item.kind {
                ItemKind::Ty(name, _) if name.name.as_ref() == "Marker" => Some(item_id),
                _ => None,
            })
            .expect("Marker type item should exist")
    };
    let pinned_store_id = StoreItemId::from((pkg_id, pinned_item));

    let result = run_pipeline_to_with_diagnostics(
        &mut store,
        pkg_id,
        PipelineStage::ItemDce,
        &[pinned_store_id],
    );

    assert!(
        result.warnings.is_empty(),
        "expected no warnings, got:\n{}",
        format_pipeline_errors(&result.warnings)
    );
    assert!(
        matches!(
            result.errors.as_slice(),
            [PipelineError::PinnedItemNotCallable(item_id)] if *item_id == pinned_store_id
        ),
        "expected PinnedItemNotCallable diagnostic, got:\n{}",
        format_pipeline_errors(&result.errors)
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
                format_reachable_callable_summary(&fir_store, fir_pkg_id)
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
                format_callable_body_summary(
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
                format_callable_body_summary(&fir_store, fir_pkg_id, "Main"),
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
/// must tolerate this between UDT erasure and tuple-decompose.
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

/// Cross-package multi-field UDT with a callable field, modeled after
/// `Std.TableLookup.AndChain` which has `(NGarbageQubits: Int, Apply: Qubit[] => Unit is Adj)`.
///
/// The library defines the UDT and a factory function that constructs it
/// with a closure capturing the factory's arguments. User code calls the
/// factory cross-package, exercises the callable field, and returns the
/// integer field. This exercises defunctionalization, UDT erasure, and tuple-decompose
/// on a callable value flowing through a cross-package struct boundary.
#[test]
fn cross_package_multi_field_udt_with_callable_field() {
    let lib_source = r#"
        namespace TestLib {
            struct Config {
                Count: Int,
                Apply: Qubit[] => Unit is Adj,
            }
            export Config, MakeConfig;

            operation NoOpImpl(qs : Qubit[]) : Unit is Adj {}

            function MakeConfig(n : Int) : Config {
                new Config { Count = n, Apply = NoOpImpl }
            }
        }
    "#;

    let user_source = r#"
        import TestLib.*;

        @EntryPoint()
        operation Main() : Int {
            let cfg = MakeConfig(3);
            use qs = Qubit[cfg.Count];
            cfg.Apply(qs);
            cfg.Count
        }
    "#;

    let (mut store, pkg_id) = compile_to_fir_with_library(lib_source, user_source);
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

/// Asserts stage-parity: running the pipeline to `stage` produces the same
/// reachable callable surface as a full pipeline run on the same source.
///
/// Returns `(staged_store, staged_pkg, full_store, full_pkg)` for callers that
/// need stage-specific assertions beyond the common checks.
#[track_caller]
fn assert_stage_parity(
    source: &str,
    stage: PipelineStage,
    invariant_level: invariants::InvariantLevel,
) -> (
    qsc_fir::fir::PackageStore,
    qsc_fir::fir::PackageId,
    qsc_fir::fir::PackageStore,
    qsc_fir::fir::PackageId,
) {
    let (mut staged_store, staged_pkg_id, _) = compile_and_lower(source);
    let (mut full_store, full_pkg_id, _) = compile_and_lower(source);
    let parity_context = format!("stage={stage:?} invariant={invariant_level:?}");

    run_pipeline_to_successfully(&mut staged_store, staged_pkg_id, stage);
    run_pipeline_successfully(&mut full_store, full_pkg_id);

    invariants::check(&staged_store, staged_pkg_id, invariant_level);

    let full_package = full_store.get(full_pkg_id);
    validate(full_package, &full_store);

    // Callable set parity.
    let staged_callables = reachable_callable_names(&staged_store, staged_pkg_id);
    let full_callables = reachable_callable_names(&full_store, full_pkg_id);
    assert_eq!(
        staged_callables, full_callables,
        "{parity_context} view=reachable_callable_names differs from Full"
    );

    // Type summary parity.
    assert_eq!(
        format_reachable_callable_summary(&staged_store, staged_pkg_id),
        format_reachable_callable_summary(&full_store, full_pkg_id),
        "{parity_context} view=reachable_callable_summary differs from Full"
    );

    (staged_store, staged_pkg_id, full_store, full_pkg_id)
}

#[test]
fn stage_parity_mono_monomorphization_preserves_callable_types() {
    let source = r#"
        function Identity<'T>(x : 'T) : 'T { x }
        @EntryPoint()
        operation Main() : Int {
            let a = Identity(42);
            let b = Identity(1.5);
            a
        }
    "#;

    let (staged, staged_pkg, full, full_pkg) = assert_stage_parity(
        source,
        PipelineStage::Mono,
        invariants::InvariantLevel::PostMono,
    );

    assert_eq!(
        format_callable_body_summary(&staged, staged_pkg, "Main"),
        format_callable_body_summary(&full, full_pkg, "Main"),
        "Main body shape should already match full pipeline after Mono for pure generic calls"
    );
}

#[test]
fn stage_parity_defunc_defunctionalization_eliminates_callable_types() {
    let source = r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Apply(H, q);
            Reset(q);
        }
    "#;

    let (staged, staged_pkg, full, full_pkg) = assert_stage_parity(
        source,
        PipelineStage::Defunc,
        invariants::InvariantLevel::PostDefunc,
    );

    assert_eq!(
        format_callable_body_summary(&staged, staged_pkg, "Main"),
        format_callable_body_summary(&full, full_pkg, "Main"),
        "Main body shape should stay stable after defunctionalization for direct H calls"
    );
}

#[test]
fn stage_parity_udt_erase_eliminates_udt_types() {
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

    let (staged, staged_pkg, full, full_pkg) = assert_stage_parity(
        source,
        PipelineStage::UdtErase,
        invariants::InvariantLevel::PostUdtErase,
    );

    assert_eq!(
        format_callable_body_summary(&staged, staged_pkg, "Extract"),
        format_callable_body_summary(&full, full_pkg, "Extract"),
        "single-field erased UDT accessor body should match the full pipeline"
    );
}

#[test]
fn stage_parity_tuple_comp_lower_lowers_tuple_equality() {
    let source = r#"
        @EntryPoint()
        operation Main() : Bool {
            let pair1 = (1, 2);
            let pair2 = (1, 2);
            pair1 == pair2
        }
    "#;

    let (staged, staged_pkg, _, _) = assert_stage_parity(
        source,
        PipelineStage::TupleCompLower,
        invariants::InvariantLevel::PostTupleCompLower,
    );

    let main_body = format_callable_body_summary(&staged, staged_pkg, "Main");
    assert!(
        main_body.contains("BinOp(AndL)"),
        "tuple equality should lower to a conjunction in Main body:\n{main_body}"
    );
}

#[test]
fn stage_parity_tuple_decompose_body_shape_matches_full_pipeline() {
    let source = r#"
        function Pair() : (Int, Bool) { (1, true) }
        @EntryPoint()
        operation Main() : Int {
            let (a, _) = Pair();
            a
        }
    "#;

    let (staged, staged_pkg, full, full_pkg) = assert_stage_parity(
        source,
        PipelineStage::TupleDecompose,
        invariants::InvariantLevel::PostTupleDecompose,
    );

    for name in &reachable_callable_names(&full, full_pkg) {
        assert_eq!(
            format_callable_body_summary(&staged, staged_pkg, name),
            format_callable_body_summary(&full, full_pkg, name),
            "callable '{name}' body must match after tuple-decompose and full pipeline"
        );
    }
}

#[test]
fn stage_parity_item_dce_reachable_surface_matches_full_pipeline() {
    let source = r#"
        function Unused() : Int { 99 }
        function Used() : Int { 42 }
        @EntryPoint()
        operation Main() : Int { Used() }
    "#;

    let (staged, staged_pkg, full, full_pkg) = assert_stage_parity(
        source,
        PipelineStage::ItemDce,
        invariants::InvariantLevel::PostItemDce,
    );

    assert_eq!(
        format_callable_body_summary(&staged, staged_pkg, "Main"),
        format_callable_body_summary(&full, full_pkg, "Main"),
        "entry body should match full pipeline after ItemDce"
    );
}

#[test]
fn stage_parity_exec_graph_rebuild_reconstructs_execution_graph() {
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

    let (staged, staged_pkg, full, full_pkg) = assert_stage_parity(
        source,
        PipelineStage::ExecGraphRebuild,
        invariants::InvariantLevel::PostAll,
    );

    for name in &reachable_callable_names(&full, full_pkg) {
        assert_eq!(
            format_callable_body_summary(&staged, staged_pkg, name),
            format_callable_body_summary(&full, full_pkg, name),
            "callable '{name}' body must match after ExecGraphRebuild and full pipeline"
        );
    }
}

#[test]
fn stage_parity_mono_type_stability() {
    assert_stage_parity(
        r#"
        operation Generic<'T>(x: 'T) : Unit { }
        @EntryPoint()
        operation Main() : Unit {
            Generic(1);
            Generic("str");
        }
    "#,
        PipelineStage::Mono,
        invariants::InvariantLevel::PostMono,
    );
}

#[test]
fn stage_parity_defunc_hof_elimination() {
    assert_stage_parity(
        r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Apply(H, q);
            Apply(X, q);
        }
    "#,
        PipelineStage::Defunc,
        invariants::InvariantLevel::PostDefunc,
    );
}

#[test]
fn stage_parity_tuple_comp_lower_no_residual() {
    assert_stage_parity(
        r#"
        @EntryPoint()
        operation Main() : Bool {
            let pair = (1, 2);
            let other = (1, 2);
            pair == other
        }
    "#,
        PipelineStage::TupleCompLower,
        invariants::InvariantLevel::PostTupleCompLower,
    );
}

#[test]
fn stage_parity_item_dce_removes_unreachable_callable_items() {
    // Regression test for item DCE removing dead callable items.
    //
    // Invariant: After item DCE, callable items that are not reachable from
    // the entry expression are removed from the package item table.
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

    assert!(
        package_has_callable_named(&pre_dce_store, pre_dce_pkg_id, "Unused"),
        "pre-ItemDce package should still contain dead callable item 'Unused'"
    );
    assert!(
        post_dce_callables.len() <= pre_dce_callables.len(),
        "item DCE should not increase reachable callable count"
    );
    assert!(
        !package_has_callable_named(&post_dce_store, post_dce_pkg_id, "Unused"),
        "ItemDce should remove unreachable callable item 'Unused'"
    );
    assert!(
        package_has_callable_named(&post_dce_store, post_dce_pkg_id, "Used"),
        "ItemDce should keep reachable callable item 'Used'"
    );
    assert!(
        package_has_callable_named(&post_dce_store, post_dce_pkg_id, "Main"),
        "ItemDce should keep the entry callable item 'Main'"
    );

    invariants::check(
        &post_dce_store,
        post_dce_pkg_id,
        invariants::InvariantLevel::PostItemDce,
    );
}
