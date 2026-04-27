// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests for the defunctionalization pass.

use std::any::Any;

use expect_test::{Expect, expect};
use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_fir::fir::{self, ItemId, ItemKind, LocalItemId, PackageLookup, PackageStoreLookup};

use super::analysis as defunc_analysis;
use super::defunctionalize;
use super::types::{
    CallableParam, CalleeLattice, ConcreteCallable, ConcreteCallableKey, SpecKey, compose_functors,
};
use crate::reachability::collect_reachable_from_entry;
use crate::test_utils::{
    compile_to_monomorphized_fir, compile_to_monomorphized_fir_with_capabilities,
};
use crate::{invariants as fir_invariants, invariants::InvariantLevel};
use qsc_data_structures::functors::FunctorApp;

mod analysis;
mod cross_package;
mod fixpoint;
mod invariants;
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
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(source);
    let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_pkg_id));
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigner);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
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

/// Compiles Q# source, runs analysis only, and snapshots the analysis results.
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
    let result = defunc_analysis::analyze(&mut fir_store, fir_pkg_id, &reachable);

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
    let mut assigner = qsc_fir::assigner::Assigner::from_package(fir_store.get(fir_pkg_id));
    let errors = defunctionalize(&mut fir_store, fir_pkg_id, &mut assigner);
    assert_no_defunctionalization_errors("defunctionalization", &errors);
    fir_invariants::check(&fir_store, fir_pkg_id, InvariantLevel::PostDefunc);
}

/// Compiles Q# source, runs defunctionalization, and snapshots the returned
/// error messages for comparison.
fn check_errors(source: &str, expect: &Expect) {
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let mut assigner = qsc_fir::assigner::Assigner::from_package(store.get(package_id));
    let errors = defunctionalize(&mut store, package_id, &mut assigner);
    expect.assert_eq(&format_defunctionalization_errors(&errors));
}

/// Compiles Q# source and runs the full FIR pipeline including monomorphization,
/// defunctionalization, and subsequent passes.
fn check_pipeline(source: &str) {
    let (mut fir_store, fir_pkg_id) = crate::test_utils::compile_to_fir(source);
    let errors = crate::run_pipeline(&mut fir_store, fir_pkg_id);
    crate::test_utils::assert_no_pipeline_errors("run_pipeline", &errors);
}

#[test]
fn compose_functors_identity() {
    let a = FunctorApp::default();
    let b = FunctorApp::default();
    let result = compose_functors(&a, &b);
    assert_eq!(result, FunctorApp::default());
}

#[test]
fn compose_functors_adj_toggle() {
    let a = FunctorApp {
        adjoint: true,
        controlled: 0,
    };
    let b = FunctorApp {
        adjoint: true,
        controlled: 0,
    };
    let result = compose_functors(&a, &b);
    assert!(!result.adjoint, "adj XOR adj should cancel");
    assert_eq!(result.controlled, 0);
}

#[test]
fn compose_functors_ctl_stack() {
    let a = FunctorApp {
        adjoint: false,
        controlled: 1,
    };
    let b = FunctorApp {
        adjoint: false,
        controlled: 1,
    };
    let result = compose_functors(&a, &b);
    assert!(!result.adjoint);
    assert_eq!(result.controlled, 2);
}

#[test]
fn compose_functors_adj_and_ctl() {
    let a = FunctorApp {
        adjoint: true,
        controlled: 1,
    };
    let b = FunctorApp {
        adjoint: false,
        controlled: 1,
    };
    let result = compose_functors(&a, &b);
    assert!(result.adjoint, "true XOR false = true");
    assert_eq!(result.controlled, 2);
}

#[test]
fn spec_key_equality() {
    let key1 = SpecKey {
        hof_id: LocalItemId::from(5usize),
        concrete_args: vec![ConcreteCallableKey::Global {
            item_id: ItemId {
                package: fir::PackageId::from(1usize),
                item: LocalItemId::from(10usize),
            },
            functor: FunctorApp::default(),
        }],
    };
    let key2 = SpecKey {
        hof_id: LocalItemId::from(5usize),
        concrete_args: vec![ConcreteCallableKey::Global {
            item_id: ItemId {
                package: fir::PackageId::from(1usize),
                item: LocalItemId::from(10usize),
            },
            functor: FunctorApp::default(),
        }],
    };
    assert_eq!(key1, key2);
}

#[test]
fn spec_key_different() {
    let key1 = SpecKey {
        hof_id: LocalItemId::from(5usize),
        concrete_args: vec![ConcreteCallableKey::Global {
            item_id: ItemId {
                package: fir::PackageId::from(1usize),
                item: LocalItemId::from(10usize),
            },
            functor: FunctorApp::default(),
        }],
    };
    let key2 = SpecKey {
        hof_id: LocalItemId::from(5usize),
        concrete_args: vec![ConcreteCallableKey::Global {
            item_id: ItemId {
                package: fir::PackageId::from(1usize),
                item: LocalItemId::from(20usize),
            },
            functor: FunctorApp::default(),
        }],
    };
    assert_ne!(key1, key2);
}

#[test]
fn spec_key_hash_consistent() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let key1 = SpecKey {
        hof_id: LocalItemId::from(5usize),
        concrete_args: vec![ConcreteCallableKey::Global {
            item_id: ItemId {
                package: fir::PackageId::from(1usize),
                item: LocalItemId::from(10usize),
            },
            functor: FunctorApp::default(),
        }],
    };
    let key2 = key1.clone();

    let mut hasher1 = DefaultHasher::new();
    key1.hash(&mut hasher1);
    let mut hasher2 = DefaultHasher::new();
    key2.hash(&mut hasher2);
    assert_eq!(hasher1.finish(), hasher2.finish());
}

#[test]
fn concrete_callable_key_global() {
    let key = ConcreteCallableKey::Global {
        item_id: ItemId {
            package: fir::PackageId::from(1usize),
            item: LocalItemId::from(42usize),
        },
        functor: FunctorApp {
            adjoint: true,
            controlled: 1,
        },
    };
    match &key {
        ConcreteCallableKey::Global { item_id, functor } => {
            assert_eq!(item_id.item, LocalItemId::from(42usize));
            assert!(functor.adjoint);
            assert_eq!(functor.controlled, 1);
        }
        ConcreteCallableKey::Closure { .. } => panic!("expected Global variant"),
    }
}

#[test]
fn concrete_callable_key_closure() {
    let key = ConcreteCallableKey::Closure {
        target: LocalItemId::from(7usize),
        functor: FunctorApp {
            adjoint: false,
            controlled: 2,
        },
    };
    match &key {
        ConcreteCallableKey::Closure { target, functor } => {
            assert_eq!(*target, LocalItemId::from(7usize));
            assert!(!functor.adjoint);
            assert_eq!(functor.controlled, 2);
        }
        ConcreteCallableKey::Global { .. } => panic!("expected Closure variant"),
    }
}

#[test]
fn error_diagnostic_has_code() {
    use miette::Diagnostic;
    use qsc_data_structures::span::Span;

    let error = super::Error::DynamicCallable(Span::default());
    let code = error
        .code()
        .expect("DynamicCallable should have a diagnostic code");
    assert_eq!(code.to_string(), "Qsc.Defunctionalize.DynamicCallable");
}

#[test]
fn error_recursive_specialization() {
    use miette::Diagnostic;
    use qsc_data_structures::span::Span;

    let error = super::Error::RecursiveSpecialization(Span { lo: 42, hi: 50 });
    expect!["specialization leads to infinite recursion"].assert_eq(&error.to_string());
    let code = error
        .code()
        .expect("RecursiveSpecialization should have a diagnostic code");
    assert_eq!(
        code.to_string(),
        "Qsc.Defunctionalize.RecursiveSpecialization"
    );
}

#[test]
fn empty_entrypoint_remains_unchanged() {
    check(
        "operation Main() : Unit { }",
        &expect![[r#"
            Main: input_ty=Unit"#]],
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
