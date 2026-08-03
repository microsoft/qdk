// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the defunctionalization pass.

use std::any::Any;

use expect_test::{Expect, expect};
use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_fir::fir::{self, ItemId, ItemKind, PackageLookup, PackageStoreLookup};

use super::analysis as defunc_analysis;
use super::defunctionalize;
use super::types::{CallableParam, CalleeLattice, ConcreteCallable};
use crate::fir_builder::reachable_local_callables;
use crate::package_assigners::PackageAssigners;
use crate::reachability::collect_reachable_from_entry;
use crate::test_utils::{
    compile_to_monomorphized_fir, compile_to_monomorphized_fir_with_capabilities,
};
use crate::walk_utils::collect_expr_ids_in_entry_and_local_callables;
use crate::{invariants as fir_invariants, invariants::InvariantLevel};
use qsc_data_structures::functors::FunctorApp;

mod analysis;
mod cleanup;
mod cross_package;
mod fixpoint;
mod invariants;
mod prepass;
mod specialization;

fn adaptive_qirgen_capabilities() -> TargetCapabilityFlags {
    TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations
}

fn format_defunctionalization_errors(errors: &[super::Error]) -> String {
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

fn assert_no_defunctionalization_errors(context: &str, errors: &[super::Error]) {
    assert!(
        errors.is_empty(),
        "{context} produced errors:\n{}",
        format_defunctionalization_errors(errors)
    );
}

fn panic_message(panic: Box<dyn Any + Send>) -> String {
    match panic.downcast::<String>() {
        Ok(message) => *message,
        Err(panic) => match panic.downcast::<&str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "(non-string panic payload)".to_string(),
        },
    }
}

/// Compiles Q# source, runs defunctionalization, and snapshots the reachable
/// callable names and their input pattern types from the user package.
fn check(source: &str, expect: &Expect) {
    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let package = fir_store.get(fir_pkg_id);
    let reachable = collect_reachable_from_entry(&fir_store, fir_pkg_id);

    let mut lines: Vec<String> = Vec::new();
    for store_id in &reachable {
        if store_id.package != fir_pkg_id {
            continue;
        }
        let item = package.get_item(store_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            let pat = package.get_pat(decl.input);
            lines.push(format!("{}: input_ty={}", decl.name.name, pat.ty));
        }
    }
    lines.sort();
    expect.assert_eq(&lines.join("\n"));
}

fn compile_and_defunctionalize(source: &str) -> (fir::PackageStore, fir::PackageId) {
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    (fir_store, fir_pkg_id)
}

/// Compiles Q# source and snapshots the pretty-printed FIR before and after
/// defunctionalization, so the visual effect of the pass on the user package
/// can be reviewed directly in the test snapshot.
fn check_rewrite(source: &str, expect: &Expect) {
    check_rewrite_with_capabilities(source, TargetCapabilityFlags::empty(), expect);
}

/// Like [`check_rewrite`] but compiles with the given target capabilities so
/// before/after snapshots can be captured for sources that require non-default
/// capabilities (e.g. adaptive QIR generation).
fn check_rewrite_with_capabilities(
    source: &str,
    capabilities: TargetCapabilityFlags,
    expect: &Expect,
) {
    let (mut fir_store, fir_pkg_id) =
        compile_to_monomorphized_fir_with_capabilities(source, capabilities);
    let before = crate::pretty::write_package_qsharp_parseable(&fir_store, fir_pkg_id);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    let after = crate::pretty::write_package_qsharp_parseable(&fir_store, fir_pkg_id);
    expect.assert_eq(&format!("BEFORE:\n{before}\nAFTER:\n{after}"));
}

fn callable_decl<'a>(package: &'a fir::Package, callable_name: &str) -> &'a fir::CallableDecl {
    package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == callable_name => {
                Some(decl.as_ref())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("callable '{callable_name}' not found"))
}

/// Lifted lambdas are renamed `.lambda_<item>`, embedding the defining item id
/// so distinct lambdas in the same package receive distinct names. This guards
/// the prefix-preserving lambda rename at the FIR layer.
#[test]
fn lifted_lambda_names_embed_item_id_and_are_distinct() {
    let (store, pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation ApplyOp(f : Qubit => Unit, q : Qubit) : Unit {
            f(q);
        }
        operation Parametrized(angle : Double, q : Qubit) : Unit {
            Rz(angle, q);
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            let op1 = Parametrized(0.5, _);
            let op2 = Parametrized(1.5, _);
            ApplyOp(op1, q);
            ApplyOp(op2, q);
        }
        "#,
    );
    let package = store.get(pkg_id);
    let lambda_names: Vec<String> = package
        .items
        .values()
        .filter_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.starts_with(".lambda") => {
                Some(decl.name.name.to_string())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        lambda_names.len(),
        2,
        "expected two lifted lambdas; got: {lambda_names:?}"
    );
    // Each lifted lambda keeps the `.lambda_` prefix and embeds a numeric item id.
    for name in &lambda_names {
        let suffix = name
            .strip_prefix(".lambda_")
            .unwrap_or_else(|| panic!("lambda name must keep the `.lambda_` prefix; got {name:?}"));
        assert!(
            !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()),
            "lambda name must embed a numeric item id; got {name:?}"
        );
    }
    // The two lifted lambdas receive distinct names.
    assert_ne!(
        lambda_names[0], lambda_names[1],
        "expected distinct lambda names; got {lambda_names:?}"
    );
}

fn call_arg_tuple_lengths_after_defunc(source: &str, callee_name: &str) -> Vec<usize> {
    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let package = fir_store.get(fir_pkg_id);
    let mut lengths = Vec::new();
    for expr in package.exprs.values() {
        let fir::ExprKind::Call(callee_id, args_id) = &expr.kind else {
            continue;
        };
        let callee = package.get_expr(*callee_id);
        let fir::ExprKind::Var(fir::Res::Item(item_id), _) = &callee.kind else {
            continue;
        };
        if resolve_item_name(&fir_store, item_id) != callee_name {
            continue;
        }
        let args = package.get_expr(*args_id);
        let len = match &args.kind {
            fir::ExprKind::Tuple(elements) => elements.len(),
            _ => 1,
        };
        lengths.push(len);
    }
    lengths.sort_unstable();
    lengths
}

fn callable_call_targets_after_defunc(source: &str, callable_name: &str) -> Vec<String> {
    let (fir_store, fir_pkg_id) = compile_and_defunctionalize(source);
    let package = fir_store.get(fir_pkg_id);
    let decl = callable_decl(package, callable_name);
    let mut targets = Vec::new();
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &decl.implementation,
        &mut |_expr_id, expr| {
            if let fir::ExprKind::Call(callee_id, _) = &expr.kind
                && let Some(target) = call_target_name(&fir_store, package, *callee_id)
            {
                targets.push(target);
            }
        },
    );
    targets.sort();
    targets
}

fn call_target_name(
    store: &fir::PackageStore,
    package: &fir::Package,
    expr_id: fir::ExprId,
) -> Option<String> {
    let expr = package.get_expr(expr_id);
    match &expr.kind {
        fir::ExprKind::Var(fir::Res::Item(item_id), _) => Some(resolve_item_name(store, item_id)),
        fir::ExprKind::UnOp(fir::UnOp::Functor(fir::Functor::Adj), inner) => {
            call_target_name(store, package, *inner).map(|name| format!("Adjoint {name}"))
        }
        fir::ExprKind::UnOp(fir::UnOp::Functor(fir::Functor::Ctl), inner) => {
            call_target_name(store, package, *inner).map(|name| format!("Controlled {name}"))
        }
        _ => None,
    }
}

/// Resolves an `ItemId` to its callable name, falling back to the raw display.
fn resolve_item_name(store: &fir::PackageStore, id: &ItemId) -> String {
    let store_id = fir::StoreItemId {
        package: id.package,
        item: id.item,
    };
    let item = store.get_item(store_id);
    if let ItemKind::Callable(decl) = &item.kind {
        decl.name.name.to_string()
    } else {
        format!("{id}")
    }
}

/// Formats a `FunctorApp` as a short specialization label.
fn functor_app_short(f: FunctorApp) -> &'static str {
    match (f.adjoint, f.controlled) {
        (false, 0) => "Body",
        (true, 0) => "Adj",
        (false, _) => "Ctl",
        (true, _) => "CtlAdj",
    }
}

/// Formats a `ConcreteCallable` for snapshot display.
fn format_concrete_callable(cc: &ConcreteCallable, store: &fir::PackageStore) -> String {
    match cc {
        ConcreteCallable::Global { item_id, functor } => {
            let name = resolve_item_name(store, item_id);
            let spec = functor_app_short(*functor);
            format!("{name}:{spec}")
        }
        ConcreteCallable::Closure {
            target, functor, ..
        } => {
            let spec = functor_app_short(*functor);
            format!("Closure({target}):{spec}")
        }
        ConcreteCallable::Dynamic => "Dynamic".to_string(),
    }
}

fn callable_param_display_path(param: &CallableParam) -> Vec<usize> {
    std::iter::once(param.top_level_param)
        .chain(param.field_path.iter().copied())
        .collect()
}

/// Compiles Q# source, runs the defunctionalization pre-pass and analysis, and
/// snapshots the analysis results.
fn check_analysis(source: &str, expect: &Expect) {
    check_analysis_with_capabilities(source, TargetCapabilityFlags::empty(), expect);
}

fn check_analysis_with_capabilities(
    source: &str,
    capabilities: TargetCapabilityFlags,
    expect: &Expect,
) {
    let (mut fir_store, fir_pkg_id) =
        compile_to_monomorphized_fir_with_capabilities(source, capabilities);
    let reachable = collect_reachable_from_entry(&fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    let local_item_ids: Vec<_> = reachable_local_callables(package, fir_pkg_id, &reachable)
        .map(|(id, _)| id)
        .collect();
    let reachable_expr_ids =
        collect_expr_ids_in_entry_and_local_callables(package, &local_item_ids);
    let collapsed_spans = super::prepass::run(&mut fir_store, fir_pkg_id, &reachable_expr_ids);
    let result = defunc_analysis::analyze(&mut fir_store, fir_pkg_id, &reachable, &collapsed_spans);

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("callable_params: {}", result.callable_params.len()));
    for param in &result.callable_params {
        lines.push(format!(
            "  param: callable_id={}, path={:?}, ty={}",
            param.callable_id,
            callable_param_display_path(param),
            param.param_ty
        ));
    }
    lines.push(format!("call_sites: {}", result.call_sites.len()));
    for cs in &result.call_sites {
        let hof_name = resolve_item_name(&fir_store, &cs.hof_item_id);
        let arg_desc = match &cs.callable_arg {
            ConcreteCallable::Global { item_id, functor } => {
                let name = resolve_item_name(&fir_store, item_id);
                let spec = functor_app_short(*functor);
                format!("Global({name}, {spec})")
            }
            ConcreteCallable::Closure {
                target, functor, ..
            } => {
                let spec = functor_app_short(*functor);
                format!("Closure(target={target}, {spec})")
            }
            ConcreteCallable::Dynamic => "Dynamic".to_string(),
        };
        lines.push(format!("  site: hof={hof_name}, arg={arg_desc}"));
    }

    let mut direct_call_site_lines: Vec<_> = result
        .direct_call_sites
        .iter()
        .map(|site| {
            let condition = if site.condition.is_empty() {
                "default".to_string()
            } else {
                let guards = site
                    .condition
                    .iter()
                    .map(|expr| format!("{expr:?}"))
                    .collect::<Vec<_>>()
                    .join(" and ");
                format!("condition={guards}")
            };
            format!(
                "  site: callee={}, {condition}",
                format_concrete_callable(&site.callable, &fir_store)
            )
        })
        .collect();
    if !direct_call_site_lines.is_empty() {
        lines.push(format!(
            "direct_call_sites: {}",
            direct_call_site_lines.len()
        ));
        direct_call_site_lines.sort();
        lines.extend(direct_call_site_lines);
    }

    let mut lattice_items: Vec<_> = result.lattice_states.iter().collect();
    lattice_items.sort_by_key(|(id, _)| **id);
    if !lattice_items.is_empty() {
        lines.push("lattice states:".to_string());
        for (item_id, entries) in &lattice_items {
            let callable_item_id = ItemId {
                package: fir_pkg_id,
                item: **item_id,
            };
            let name = resolve_item_name(&fir_store, &callable_item_id);
            lines.push(format!("  callable {name}:"));
            for (var_id, lattice) in *entries {
                let desc = match lattice {
                    CalleeLattice::Bottom => continue,
                    CalleeLattice::Single(cc) => {
                        format!("Single({})", format_concrete_callable(cc, &fir_store))
                    }
                    CalleeLattice::Multi(candidates) => {
                        let names: Vec<String> = candidates
                            .iter()
                            .map(|(cc, _)| format_concrete_callable(cc, &fir_store))
                            .collect();
                        format!("Multi([{}])", names.join(", "))
                    }
                    CalleeLattice::Dynamic => "Dynamic".to_string(),
                };
                lines.push(format!("    {var_id}: {desc}"));
            }
        }
    }

    expect.assert_eq(&lines.join("\n"));
}

/// Compiles Q# source, runs defunctionalization, and asserts `PostDefunc`
/// invariants hold.
fn check_invariants(source: &str) {
    check_invariants_with_capabilities(source, TargetCapabilityFlags::empty());
}

fn check_invariants_with_capabilities(source: &str, capabilities: TargetCapabilityFlags) {
    let (mut fir_store, fir_pkg_id) =
        compile_to_monomorphized_fir_with_capabilities(source, capabilities);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    fir_invariants::check(&fir_store, fir_pkg_id, InvariantLevel::PostDefunc);
}

/// Compiles Q# source, runs defunctionalization, and snapshots the returned
/// error messages for comparison.
fn check_errors(source: &str, expect: &Expect) {
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&store, package_id);
    let errors = defunctionalize(&mut store, package_id, &mut assigners);
    expect.assert_eq(&format_defunctionalization_errors(&errors));
}

/// Compiles Q# source and runs the full FIR pipeline including monomorphization,
/// defunctionalization, and subsequent passes.
fn check_pipeline(source: &str) {
    let (mut fir_store, fir_pkg_id) = crate::test_utils::compile_to_fir(source);
    let result = crate::run_pipeline_with_diagnostics(&mut fir_store, fir_pkg_id);
    crate::test_utils::assert_no_pipeline_errors("run_pipeline", &result.errors);
}

/// Returns `true` if the body block of `callable_name` contains a `let`
/// binding for a local named `binding_name`.
fn body_binds_local(package: &fir::Package, callable_name: &str, binding_name: &str) -> bool {
    let decl = callable_decl(package, callable_name);
    let fir::CallableImpl::Spec(spec) = &decl.implementation else {
        return false;
    };
    let block = package.get_block(spec.body.block);
    block.stmts.iter().any(|&stmt_id| {
        let stmt = package.get_stmt(stmt_id);
        if let fir::StmtKind::Local(_, pat_id, _) = &stmt.kind {
            let pat = package.get_pat(*pat_id);
            matches!(&pat.kind, fir::PatKind::Bind(ident) if ident.name.as_ref() == binding_name)
        } else {
            false
        }
    })
}

/// Regression test: a callable-typed local used only inside a
/// live struct field was wrongly pruned by defunctionalize because the
/// use-collectors skipped recursing into `Struct` expressions. The `let f`
/// binding in `Pick` must survive defunctionalization.
#[test]
fn callable_local_used_only_in_struct_field_survives_defunc() {
    let source = "
namespace Test {
    struct Holder { Cb : (Int => Int) }
    function Pick(arr : (Int => Int)[]) : Holder {
        let f = arr[0];
        new Holder { Cb = f }
    }
    @EntryPoint()
    operation Main() : Unit {
        let ops : (Int => Int)[] = [x => x + 1];
        let h = Pick(ops);
        let _ = h.Cb(3);
    }
}
";
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    // The callable stored in the field originates from a dynamic array index,
    // so defunctionalize cannot fully resolve it (non-convergence is expected
    // and orthogonal to this regression). We only assert binding survival.
    let _ = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    let package = fir_store.get(fir_pkg_id);
    assert!(
        body_binds_local(package, "Pick", "f"),
        "the `let f` binding in `Pick` must survive defunctionalization"
    );
}

#[test]
fn error_diagnostic_has_code() {
    use miette::Diagnostic;
    use qsc_data_structures::span::Span;
    use qsc_fir::fir::{PackageId, PackageSpan};

    let error =
        super::Error::DynamicCallable(PackageSpan::new(PackageId::from(2usize), Span::default()));
    let code = error
        .code()
        .expect("DynamicCallable should have a diagnostic code");
    assert_eq!(code.to_string(), "Qdk.Qsc.Defunctionalize.DynamicCallable");
}

#[test]
fn empty_entrypoint_remains_unchanged() {
    let source = "operation Main() : Unit { }";
    check(
        source,
        &expect![[r#"
            Main: input_ty=Unit"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {}
            // entry
            Main()

            AFTER:
            operation Main() : Unit {}
            // entry
            Main()
        "#]],
    );
}

#[test]
fn test_helpers_surface_defunctionalization_errors() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            mutable n = 3;
            while n > 0 {
                op = X;
                n -= 1;
            }
            ApplyOp(op, q);
        }
        "#;

    let check_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check(source, &expect![[r#"should not reach snapshot assertion"#]]);
    }))
    .expect_err("check should panic when defunctionalization returns errors");
    let check_message = panic_message(check_panic);
    assert!(
        check_message.contains("defunctionalization produced errors"),
        "unexpected check panic: {check_message}"
    );
    assert!(
        check_message.contains("callable argument could not be resolved statically"),
        "unexpected check panic: {check_message}"
    );

    let pipeline_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        check_pipeline(source);
    }))
    .expect_err("check_pipeline should panic when run_pipeline returns defunctionalization errors");
    let pipeline_message = panic_message(pipeline_panic);
    assert!(
        pipeline_message.contains("produced FIR transform pipeline errors"),
        "unexpected check_pipeline panic: {pipeline_message}"
    );
    assert!(
        pipeline_message.contains("callable argument could not be resolved statically"),
        "unexpected check_pipeline panic: {pipeline_message}"
    );
}

/// A HOF whose body defines a nested item — either a lifted lambda
/// (`StmtKind::Item` + `ExprKind::Closure`) or a named nested function
/// (`StmtKind::Item`) — must have that item included in the extracted body
/// package so that `FirCloner::clone_nested_item` can find it during
/// specialization. Both flavors must produce a concrete specialized clone of
/// the HOF (`Transform`), proving specialization actually ran rather than just
/// not panicking.
#[test]
fn hof_with_nested_item_in_body_specializes_correctly() {
    fn assert_transform_specialized(source: &str) {
        let (store, pkg_id) = compile_and_defunctionalize(source);
        let package = store.get(pkg_id);
        let names: Vec<String> = package
            .items
            .values()
            .filter_map(|item| match &item.kind {
                ItemKind::Callable(decl) => Some(decl.name.name.to_string()),
                ItemKind::Ty(..) => None,
            })
            .collect();
        // The original generic HOF remains (item DCE has not run yet), plus a
        // freshly specialized clone whose name carries the specialization
        // suffix — concrete proof that `Transform` was specialized for the
        // `x -> x + 1` argument, with its nested item successfully extracted.
        assert!(
            names.iter().any(|n| n == "Transform"),
            "original Transform HOF should remain pre-DCE; callables: {names:?}"
        );
        assert!(
            names
                .iter()
                .any(|n| n != "Transform" && n.starts_with("Transform")),
            "a specialized Transform clone should be created; callables: {names:?}"
        );
    }

    // Nested *lambda* lifted to an item: the compiler lifts `helper` to a
    // nested item referenced via `StmtKind::Item` + `ExprKind::Closure`.
    assert_transform_specialized(
        r#"
        function Transform(f : Int -> Int, x : Int) : Int {
            let helper = y -> y * 2;
            helper(f(x))
        }
        function Main() : Int {
            Transform(x -> x + 1, 5)
        }
    "#,
    );

    // Nested *named function* item appearing directly as `StmtKind::Item`.
    assert_transform_specialized(
        r#"
        function Transform(f : Int -> Int, x : Int) : Int {
            function Helper(y : Int) : Int { y * 2 }
            Helper(f(x))
        }
        function Main() : Int {
            Transform(x -> x + 1, 5)
        }
    "#,
    );
}

#[test]
fn unreachable_closure_structure_preserved() {
    // Reachable: Main calls Apply with a closure.
    // Dead: DeadFn uses a different closure pattern.
    // Document whether the dead closure structure is mutated by defunctionalization.
    use indoc::indoc;
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(indoc! {"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                Apply(x -> x + 1, 5)
            }
            function Apply(f : Int -> Int, x : Int) : Int { f(x) }
            // Dead — never called from entry
            function DeadFn() : Int {
                Apply(x -> x * 2, 10)
            }
        }
    "});
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors("unreachable_closure_structure_preserved", &errors);

    // Structure preserved: defunctionalize only rewrites *reachable* call
    // sites, so DeadFn's body must still contain the un-specialized HOF call
    // `Apply(x -> x * 2, 10)` — its lifted closure survives and the `Apply`
    // arrow argument was not eliminated for the dead site.
    let package = fir_store.get(fir_pkg_id);
    let dead_decl = package
        .items
        .values()
        .find_map(|item| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.as_ref() == "DeadFn" => Some(decl),
            _ => None,
        })
        .expect("DeadFn should still exist pre-DCE");

    let mut dead_has_closure = false;
    let mut dead_calls_unspecialized_apply = false;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &dead_decl.implementation,
        &mut |_id, expr| {
            if matches!(expr.kind, fir::ExprKind::Closure(..)) {
                dead_has_closure = true;
            }
            if let fir::ExprKind::Call(callee_id, _) = &expr.kind {
                let callee = package.get_expr(*callee_id);
                if let fir::ExprKind::Var(fir::Res::Item(item_id), _) = &callee.kind
                    && resolve_item_name(&fir_store, item_id) == "Apply"
                {
                    dead_calls_unspecialized_apply = true;
                }
            }
        },
    );
    assert!(
        dead_has_closure,
        "DeadFn's lifted `x -> x * 2` closure must survive defunctionalization unchanged"
    );
    assert!(
        dead_calls_unspecialized_apply,
        "DeadFn must still call the un-specialized `Apply` HOF (dead site not rewritten)"
    );
}

/// The `StmtKind::Semi(Return(_))` arm in defunctionalize analysis
/// (`resolve_callable_return`) is genuinely live for bodies that originate
/// cross-package. `check_no_returns` skips cross-package items and
/// return-unification runs local-package-only, so a library callable that
/// returns a callable via an explicit `return` keeps its `Semi(Return)` tail.
/// Monomorphization specializes the generic helper in place in its owning
/// (library) package, where defunctionalize then analyzes the `Semi(Return)`
/// arm cross-package. A returned `Global` callable carries its own package and
/// resolves across packages; if the arm were dead or broken, the HOF argument
/// could not be resolved statically and defunctionalization would surface an
/// error; asserting no errors proves the arm is reached and resolves the
/// returned callable.
#[test]
fn cross_package_return_stmt_is_analyzed() {
    let lib_source = r#"
        namespace TestLib {
            function LibStep(x : Int) : Int { x + 1 }
            function MakeStep<'T>(unused : 'T) : (Int -> Int) {
                return LibStep;
            }
            export MakeStep, LibStep;
        }
    "#;
    let user_source = r#"
        import TestLib.*;

        function Apply(f : Int -> Int, x : Int) : Int { f(x) }
        @EntryPoint()
        operation Main() : Int {
            Apply(MakeStep(0), 5)
        }
    "#;
    let (mut fir_store, fir_pkg_id) =
        crate::test_utils::compile_to_fir_with_library(lib_source, user_source);

    // Monomorphization specializes `MakeIdentity<Int>` in place in its owning
    // (library) package; its body still ends in `return x -> x;`
    // (`Semi(Return)`), since return unification has not run on the freshly
    // cloned cross-package body.
    let mut assigners = PackageAssigners::new(&fir_store, fir_pkg_id);
    crate::monomorphize::monomorphize(&mut fir_store, fir_pkg_id, &mut assigners);

    // Precondition: a reachable callable (in whichever package owns the
    // specialization) now ends in `Semi(Return)` of a callable-typed value —
    // exactly the shape the analysis arm consumes.
    let semi_return_callable_present = {
        let reachable = crate::reachability::collect_reachable_from_entry(&fir_store, fir_pkg_id);
        reachable.iter().any(|store_id| {
            let package = fir_store.get(store_id.package);
            let item = package.get_item(store_id.item);
            let ItemKind::Callable(decl) = &item.kind else {
                return false;
            };
            let fir::CallableImpl::Spec(spec) = &decl.implementation else {
                return false;
            };
            if !matches!(decl.output, qsc_fir::ty::Ty::Arrow(_)) {
                return false;
            }
            let block = package.get_block(spec.body.block);
            block.stmts.last().is_some_and(|&stmt_id| {
                let stmt = package.get_stmt(stmt_id);
                matches!(
                    &stmt.kind,
                    fir::StmtKind::Semi(expr_id)
                        if matches!(package.get_expr(*expr_id).kind, fir::ExprKind::Return(_))
                )
            })
        })
    };
    assert!(
        semi_return_callable_present,
        "monomorphized cross-package body returning a callable must retain its \
         `Semi(Return)` tail for the analysis arm to consume"
    );

    // Defunctionalize analysis traverses the `Semi(Return)` arm to resolve the
    // returned callable; success (no errors) proves the arm is live.
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigners);
    assert_no_defunctionalization_errors("cross_package_return_stmt_is_analyzed", &errors);
}
