// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::unicode_not_nfc)]

mod group_scopes;
mod logical_stack_trace;
mod prune_classical_qubits;

use std::{rc::Rc, vec};

use super::*;
use expect_test::expect;
use indoc::indoc;
use qsc_data_structures::{functors::FunctorApp, span::Span};
use qsc_eval::debug::Frame;
use qsc_fir::fir::{self, ExprKind, PackageLookup, StoreItemId};
use qsc_frontend::compile::{self, PackageStore, compile};
use qsc_lowerer::map_hir_package_to_fir;
use qsc_passes::{PackageType, run_core_passes, run_default_passes};
use rustc_hash::FxHashMap;

#[derive(Default)]
struct FakeCompilation {
    scopes: Scopes,
}

impl SourceLookup for FakeCompilation {
    fn resolve_package_offset(&self, package_offset: &PackageOffset) -> SourceLocation {
        SourceLocation {
            file: match usize::from(package_offset.package_id) {
                Self::USER_PACKAGE_ID => "user_code.qs".to_string(),
                Self::LIBRARY_PACKAGE_ID => "library_code.qs".to_string(),
                _ => panic!("unexpected package id"),
            },
            line: 0,
            column: package_offset.offset,
        }
    }

    fn resolve_scope(&self, scope: &Scope) -> LexicalScope {
        match scope {
            Scope::Callable(CallableId::Id(store_item_id, functor_app)) => {
                let name = self
                    .scopes
                    .id_to_name
                    .get(store_item_id)
                    .expect("unknown scope id")
                    .clone();
                LexicalScope {
                    name,
                    location: Some(PackageOffset {
                        package_id: store_item_id.package,
                        offset: 0,
                    }),
                    is_adjoint: functor_app.adjoint,
                    is_classically_controlled: false,
                }
            }
            s => panic!("unexpected scope id {s:?}"),
        }
    }

    fn resolve_logical_stack_entry_location(
        &self,
        location: LogicalStackEntryLocation,
    ) -> Option<PackageOffset> {
        match location {
            LogicalStackEntryLocation::Source(package_offset) => Some(package_offset),
            LogicalStackEntryLocation::Branch(package_offset, _) => package_offset,
            _ => panic!("only Call and Branch locations are supported in tests"),
        }
    }

    fn is_synthesized_callable_scope(&self, _scope: &Scope) -> bool {
        false
    }
}

impl FakeCompilation {
    const LIBRARY_PACKAGE_ID: usize = 0;
    const USER_PACKAGE_ID: usize = 2;

    fn user_package_ids() -> Vec<PackageId> {
        vec![Self::USER_PACKAGE_ID.into()]
    }

    fn library_frame(&mut self, offset: u32) -> Frame {
        let scope_id =
            self.scopes
                .get_or_create_scope(Self::LIBRARY_PACKAGE_ID, "library_item", false);
        Self::frame(&scope_id, offset, false)
    }

    fn user_code_frame(&mut self, scope_name: &str, offset: u32) -> Frame {
        let scope_id = self
            .scopes
            .get_or_create_scope(Self::USER_PACKAGE_ID, scope_name, false);
        Self::frame(&scope_id, offset, false)
    }

    fn user_code_adjoint_frame(&mut self, scope_name: &str, offset: u32) -> Frame {
        let scope_id = self
            .scopes
            .get_or_create_scope(Self::USER_PACKAGE_ID, scope_name, true);
        Self::frame(&scope_id, offset, true)
    }

    fn frame(scope_item_id: &Scope, offset: u32, is_adjoint: bool) -> Frame {
        match scope_item_id {
            Scope::Callable(CallableId::Id(store_item_id, _)) => Frame {
                span: fir::PackageSpan::new(
                    store_item_id.package,
                    Span {
                        lo: offset,
                        hi: offset + 1,
                    },
                ),
                id: *store_item_id,
                caller: PackageId::CORE, // unused in tests
                functor: FunctorApp {
                    adjoint: is_adjoint,
                    controlled: 0,
                },
                loop_iterations: Vec::new(),
            },
            _ => panic!("unexpected scope id {scope_item_id:?}"),
        }
    }
}

#[derive(Default)]
struct Scopes {
    id_to_name: FxHashMap<StoreItemId, Rc<str>>,
    name_to_id: FxHashMap<Rc<str>, StoreItemId>,
}

impl Scopes {
    fn get_or_create_scope(&mut self, package_id: usize, name: &str, is_adjoint: bool) -> Scope {
        let name: Rc<str> = name.into();
        let item_id = if let Some(item_id) = self.name_to_id.get(&name) {
            *item_id
        } else {
            let item_id = StoreItemId {
                package: package_id.into(),
                item: self.id_to_name.len().into(),
            };
            self.id_to_name.insert(item_id, name.clone());
            self.name_to_id.insert(name, item_id);
            item_id
        };
        Scope::Callable(CallableId::Id(
            item_id,
            FunctorApp {
                adjoint: is_adjoint,
                controlled: 0,
            },
        ))
    }
}

/// Builds matching HIR and FIR package stores with core, dependency, and user
/// packages for callable-origin lookup tests.
fn compile_origin_lookup_stores() -> (PackageStore, fir::PackageStore, PackageId, PackageId) {
    let mut fir_lowerer = qsc_lowerer::Lowerer::new();

    let mut core = compile::core();
    run_core_passes(&mut core);

    let lowering_store = fir::PackageStore::new();
    let core_fir = fir_lowerer.lower_package(
        &core.package,
        &lowering_store,
        qsc_fir::fir::PackageId::CORE,
    );
    let mut store = PackageStore::new(core);

    let library_source = indoc! {
        r#"
        namespace Library {
            operation LibraryHelper() : Unit { }
        }
        "#
    };
    let mut library_unit = compile(
        &store,
        &[],
        qsc_data_structures::source::SourceMap::new(
            [("Library.qs".into(), library_source.into())],
            None,
        ),
        qsc_data_structures::target::TargetCapabilityFlags::all(),
        qsc_data_structures::language_features::LanguageFeatures::default(),
    );
    assert!(library_unit.errors.is_empty(), "{:?}", library_unit.errors);
    let library_pass_errors = run_default_passes(store.core(), &mut library_unit, PackageType::Lib);
    assert!(library_pass_errors.is_empty(), "{library_pass_errors:?}");
    let dep_unit_id = store.insert(library_unit);
    let dep_pkg_id = map_hir_package_to_fir(dep_unit_id);
    let library_fir = fir_lowerer.lower_package(
        &store
            .get(dep_unit_id)
            .expect("package should exist")
            .package,
        &lowering_store,
        dep_pkg_id,
    );

    let user_source = indoc! {
        r#"
        namespace User {
            operation UserHelper() : Unit { }
        }
        "#
    };
    let mut user_unit = compile(
        &store,
        &[],
        qsc_data_structures::source::SourceMap::new([("User.qs".into(), user_source.into())], None),
        qsc_data_structures::target::TargetCapabilityFlags::all(),
        qsc_data_structures::language_features::LanguageFeatures::default(),
    );
    assert!(user_unit.errors.is_empty(), "{:?}", user_unit.errors);
    let user_pass_errors = run_default_passes(store.core(), &mut user_unit, PackageType::Lib);
    assert!(user_pass_errors.is_empty(), "{user_pass_errors:?}");
    let app_unit_id = store.insert(user_unit);
    let app_pkg_id = map_hir_package_to_fir(app_unit_id);
    let user_fir = fir_lowerer.lower_package(
        &store
            .get(app_unit_id)
            .expect("package should exist")
            .package,
        &lowering_store,
        app_pkg_id,
    );

    let mut fir_store = fir::PackageStore::new();
    fir_store.insert(
        map_hir_package_to_fir(qsc_hir::hir::PackageId::CORE),
        core_fir,
    );
    fir_store.insert(dep_pkg_id, library_fir);
    fir_store.insert(app_pkg_id, user_fir);

    (store, fir_store, dep_pkg_id, app_pkg_id)
}

/// Copies a named FIR callable into another package while preserving its source
/// span, matching synthesized callables that keep their original source origin.
fn clone_callable_into_package(
    fir_store: &mut fir::PackageStore,
    source_package: PackageId,
    target_package: PackageId,
    source_name: &str,
    suffix: &str,
) -> StoreItemId {
    let source_item = fir_store
        .get(source_package)
        .items
        .iter()
        .find_map(|(item_id, item)| match &item.kind {
            fir::ItemKind::Callable(decl) if decl.name.name.as_ref() == source_name => {
                Some((item_id, item.clone()))
            }
            _ => None,
        })
        .expect("expected callable in source package")
        .1;

    let target = fir_store.get_mut(target_package);
    let new_item_id = target
        .items
        .iter()
        .map(|(item_id, _)| usize::from(item_id))
        .max()
        .map_or(0, |max_id| max_id + 1)
        .into();

    let mut new_item = source_item;
    new_item.id = new_item_id;
    if let fir::ItemKind::Callable(decl) = &mut new_item.kind {
        decl.name.name = Rc::from(format!("{}{suffix}", decl.name.name));
    }
    target.items.insert(new_item_id, new_item);

    StoreItemId {
        package: target_package,
        item: new_item_id,
    }
}

#[test]
fn id_backed_callable_scopes_preserve_storage_and_source_identity() {
    let (store, mut fir_store, library_package_id, user_package_id) =
        compile_origin_lookup_stores();

    let library_clone = clone_callable_into_package(
        &mut fir_store,
        library_package_id,
        user_package_id,
        "LibraryHelper",
        "<Adj>",
    );
    let duplicate_library_clone = clone_callable_into_package(
        &mut fir_store,
        library_package_id,
        user_package_id,
        "LibraryHelper",
        "<Ctl>",
    );
    let user_clone = clone_callable_into_package(
        &mut fir_store,
        user_package_id,
        library_package_id,
        "UserHelper",
        "{H}",
    );

    let callable_body_span = |callable_id| {
        let item = fir_store.get_item(callable_id);
        let fir::ItemKind::Callable(decl) = &item.kind else {
            panic!("expected callable item");
        };
        let fir::CallableImpl::Spec(spec_impl) = &decl.implementation else {
            panic!("expected specialization implementation");
        };
        spec_impl.body.span
    };
    let library_source_span = callable_body_span(library_clone);
    let duplicate_library_source_span = callable_body_span(duplicate_library_clone);
    let user_source_span = callable_body_span(user_clone);
    assert_ne!(library_clone, duplicate_library_clone);
    assert_eq!(library_clone.package, user_package_id);
    assert_eq!(duplicate_library_clone.package, user_package_id);
    assert_eq!(user_clone.package, library_package_id);
    assert_eq!(library_source_span, duplicate_library_source_span);
    assert_eq!(library_source_span.package, library_package_id);
    assert_eq!(user_source_span.package, user_package_id);

    let library_scope = Scope::Callable(CallableId::Id(library_clone, FunctorApp::default()));
    let duplicate_library_scope = Scope::Callable(CallableId::Id(
        duplicate_library_clone,
        FunctorApp::default(),
    ));
    let user_scope = Scope::Callable(CallableId::Id(user_clone, FunctorApp::default()));
    let lookup = (&store, &fir_store);

    assert_ne!(library_scope, duplicate_library_scope);
    assert_eq!(
        lookup.resolve_scope(&library_scope).location,
        Some(PackageOffset {
            package_id: library_source_span.package,
            offset: library_source_span.lo,
        })
    );
    assert_eq!(
        lookup.resolve_scope(&duplicate_library_scope).location,
        Some(PackageOffset {
            package_id: duplicate_library_source_span.package,
            offset: duplicate_library_source_span.lo,
        })
    );
    assert_eq!(
        lookup.resolve_scope(&user_scope).location,
        Some(PackageOffset {
            package_id: user_source_span.package,
            offset: user_source_span.lo,
        })
    );
    assert!(lookup.is_synthesized_callable_scope(&library_scope));
    assert!(lookup.is_synthesized_callable_scope(&duplicate_library_scope));
    assert!(lookup.is_synthesized_callable_scope(&user_scope));

    let retained = retain_user_frames(
        &[user_package_id],
        LogicalStack(vec![
            LogicalStackEntry::new_call_site(
                PackageOffset {
                    package_id: library_source_span.package,
                    offset: library_source_span.lo,
                },
                library_scope.clone(),
            ),
            LogicalStackEntry::new_call_site(
                PackageOffset {
                    package_id: user_source_span.package,
                    offset: user_source_span.lo,
                },
                user_scope,
            ),
        ]),
    );
    assert_eq!(retained.0.len(), 1);
    assert_eq!(retained.0[0].lexical_scope(), &library_scope);
}

#[test]
fn exceed_max_operations() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 2,
            source_locations: false,
            group_by_scope: false,
            prune_classical_qubits: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(&[], "X", false, &[0], &[], None);
    builder.gate(&[], "X", false, &[0], &[], None);
    builder.gate(&[], "X", false, &[0], &[], None);

    let circuit = builder.finish(&FakeCompilation::default());

    // The current behavior is to silently truncate the circuit
    // if it exceeds the maximum allowed number of operations.
    expect![[r#"
        q_0    ── X ──── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn source_locations_enabled() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
            group_by_scope: false,
            prune_classical_qubits: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(
        &[c.user_code_frame("Main", 10)],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&c);

    expect![[r#"
        q_0    ─ X@user_code.qs:0:10 ─
    "#]]
    .assert_eq(&circuit.to_string());

    // Also render with the source location annotation disabled.
    expect![[r#"
        q_0    ── X ──
    "#]]
    .assert_eq(&circuit.display_no_locations().to_string());
}

#[test]
fn source_locations_disabled() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: false,
            group_by_scope: false,
            prune_classical_qubits: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(
        &[c.user_code_frame("Main", 10)],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&c);

    expect![[r#"
        q_0    ── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn source_locations_multiple_user_frames() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
            group_by_scope: false,
            prune_classical_qubits: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(
        &[c.user_code_frame("Main", 10), c.user_code_frame("Main", 20)],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&c);

    // Use the most current user frame for the source location.
    expect![[r#"
        q_0    ─ X@user_code.qs:0:20 ─
    "#]]
    .assert_eq(&circuit.to_string());

    // Also render with the source location annotation disabled.
    expect![[r#"
        q_0    ── X ──
    "#]]
    .assert_eq(&circuit.display_no_locations().to_string());
}

#[test]
fn source_locations_library_frames_excluded() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
            group_by_scope: false,
            prune_classical_qubits: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);
    builder.gate(
        &[c.user_code_frame("Main", 10), c.library_frame(20)],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&c);

    // Most recent frame is a library frame - source
    // location should fall back to the nearest user frame.
    expect![[r#"
        q_0    ─ X@user_code.qs:0:10 ─
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn source_locations_only_library_frames() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
            group_by_scope: false,
            prune_classical_qubits: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(
        &[c.library_frame(20), c.library_frame(30)],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&c);

    // Only library frames, no user source to show
    expect![[r#"
        q_0    ── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn source_locations_enabled_no_stack() {
    let c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
            group_by_scope: false,
            prune_classical_qubits: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(&[], "X", false, &[0], &[], None);

    let circuit = builder.finish(&c);

    // No stack was passed, so no source location to show
    expect![[r#"
        q_0    ── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn frame_source_package_controls_operation_and_qubit_locations() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
            group_by_scope: false,
            prune_classical_qubits: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    let mut frame = c.user_code_frame("Main", 10);
    frame.span.package = FakeCompilation::LIBRARY_PACKAGE_ID.into();
    let stack = [frame];

    builder.qubit_allocate(&stack, 0);
    builder.gate(&stack, "X", false, &[0], &[], None);

    let circuit = builder.finish(&c);

    expect![[r#"
        q_0@library_code.qs:0:10 ─ X@library_code.qs:0:10 ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn qubit_labels_for_preallocated_qubits() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::with_qubit_input_params(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
            group_by_scope: false,
            prune_classical_qubits: false,
        },
        &FakeCompilation::user_package_ids(),
        Some((
            FakeCompilation::USER_PACKAGE_ID.into(),
            vec![QubitParam {
                dimensions: 1,
                source_offset: 10,
            }],
        )),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(
        &[c.user_code_frame("Main", 20)],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&c);

    expect![[r#"
        q_0@user_code.qs:0:10  ─ X@user_code.qs:0:20 ─
        q_1@user_code.qs:0:10  ───────────────────────
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn measurement_target_propagated_to_group() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: usize::MAX,
            source_locations: false,
            group_by_scope: true,
            prune_classical_qubits: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(&[c.user_code_frame("Main", 1)], "H", false, &[0], &[], None);

    builder.measure(&[c.user_code_frame("Main", 2)], "M", 0, &0.into());

    let circuit = builder.finish(&c);

    // Verify that there's a grouped operation, with a measurement operation
    // inside it, and that the measurement target register is also propagated
    // to the group operation.

    // Get the group

    let mut group_ops = circuit.component_grid.iter().flat_map(|col| {
        col.components.iter().filter_map(|c| {
            if let Operation::Unitary(u) = c
                && !u.children.is_empty()
            {
                Some(u)
            } else {
                None
            }
        })
    });

    let group_op = group_ops
        .next()
        .expect("expected to find grouped operation");
    assert!(
        group_ops.next().is_none(),
        "expected only one grouped operation"
    );

    // Get the measurement operation

    let mut measurement_ops = group_op.children.iter().filter_map(|col| {
        col.components.iter().find_map(|c| {
            if let Operation::Measurement(m) = c {
                Some(m)
            } else {
                None
            }
        })
    });

    let measurement_op = measurement_ops
        .next()
        .expect("expected to find measurement operation");
    assert!(
        measurement_ops.next().is_none(),
        "expected only one measurement operation"
    );

    // Now verify that the measurement qubit and result registers exist in the parent
    // group operation's targets as well.
    group_op
        .targets
        .iter()
        .find(|reg| *reg == &measurement_op.qubits[0])
        .expect("expected measurement qubit in group operation's targets");
    group_op
        .targets
        .iter()
        .find(|reg| *reg == &measurement_op.results[0])
        .expect("expected measurement result in group operation's targets");
}

/// Verifies that loop scope resolution falls back to the loop expression when a
/// condition span cannot be mapped into the source package.
#[test]
fn resolve_scope_for_loop_tolerates_out_of_range_condition_span() {
    let mut fir_lowerer = qsc_lowerer::Lowerer::new();
    let mut core = compile::core();
    run_core_passes(&mut core);
    let lowering_store = fir::PackageStore::new();
    let core_fir = fir_lowerer.lower_package(
        &core.package,
        &lowering_store,
        qsc_fir::fir::PackageId::CORE,
    );
    let mut store = PackageStore::new(core);

    let source = indoc! {
        r#"
        namespace Test {
            operation Main() : Unit {
                mutable i = 0;
                while i < 2 {
                    set i += 1;
                }
            }
        }
        "#
    };
    let mut unit = compile(
        &store,
        &[],
        qsc_data_structures::source::SourceMap::new(
            [("A.qs".into(), source.into())],
            Some("Test.Main()".into()),
        ),
        qsc_data_structures::target::TargetCapabilityFlags::all(),
        qsc_data_structures::language_features::LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Lib);
    assert!(pass_errors.is_empty(), "{pass_errors:?}");
    let hir_package_id = store.insert(unit);
    let fir_package_id = map_hir_package_to_fir(hir_package_id);
    let unit_fir = fir_lowerer.lower_package(
        &store
            .get(hir_package_id)
            .expect("package should exist")
            .package,
        &lowering_store,
        fir_package_id,
    );

    let mut fir_store = fir::PackageStore::new();
    fir_store.insert(
        map_hir_package_to_fir(qsc_hir::hir::PackageId::CORE),
        core_fir,
    );
    fir_store.insert(fir_package_id, unit_fir);

    // Capture the while expression and its condition separately so only the
    // condition span is corrupted.
    let (loop_expr_id, cond_expr_id) = {
        let package = fir_store.get(fir_package_id);
        package
            .exprs
            .iter()
            .find_map(|(expr_id, expr)| {
                if let ExprKind::While(cond_expr_id, _) = expr.kind {
                    Some((expr_id, cond_expr_id))
                } else {
                    None
                }
            })
            .expect("expected while loop in lowered FIR")
    };

    // Simulate transform-produced FIR whose condition span points beyond the
    // source file while the enclosing loop span remains valid.
    let source_len = u32::try_from(source.len()).expect("source length should fit in u32");
    let cond_expr = fir_store
        .get_mut(fir_package_id)
        .exprs
        .get_mut(cond_expr_id)
        .expect("condition expr should exist");
    cond_expr.span.span.hi = source_len + 100;

    // Resolution should tolerate the bad condition span and still produce a
    // stable group name and source location from the loop expression itself.
    let scope =
        (&store, &fir_store).resolve_scope(&Scope::Loop(LoopId::Id(fir_package_id, loop_expr_id)));

    assert_eq!(scope.name.as_ref(), "loop: ");
    assert_eq!(
        scope.location,
        Some(PackageOffset {
            package_id: fir_package_id,
            offset: fir_store.get(fir_package_id).get_expr(loop_expr_id).span.lo,
        })
    );
}

fn compile_and_lower_test_package(
    store: &mut PackageStore,
    source_name: &str,
    source: &str,
    entry: Option<&str>,
) -> (qsc_fir::fir::PackageId, fir::Package) {
    let capabilities = qsc_data_structures::target::TargetCapabilityFlags::all();
    let mut unit = compile(
        store,
        &[],
        qsc_data_structures::source::SourceMap::new(
            [(source_name.into(), source.into())],
            entry.map(Into::into),
        ),
        capabilities,
        qsc_data_structures::language_features::LanguageFeatures::default(),
    );
    assert!(unit.errors.is_empty(), "{:?}", unit.errors);
    let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Lib);
    assert!(pass_errors.is_empty(), "{pass_errors:?}");
    let hir_package = store.insert(unit);
    let fir_package = map_hir_package_to_fir(hir_package);
    let package = qsc_lowerer::Lowerer::new().lower_package(
        &store
            .get(hir_package)
            .expect("package should exist")
            .package,
        &fir::PackageStore::new(),
        fir_package,
    );
    (fir_package, package)
}

#[test]
#[allow(clippy::too_many_lines)]
fn id_backed_loop_scope_uses_relocated_source_spans() {
    let mut core = compile::core();
    run_core_passes(&mut core);
    let mut store = PackageStore::new(core);

    let dependency_source = indoc! {
        r#"
        namespace Dependency {
            operation Foreign() : Unit {
                mutable dependency_index = 0;
                while dependency_index < 17 {
                    set dependency_index += 1;
                }
            }
        }
        "#
    };
    let (dependency_package, dependency_fir) =
        compile_and_lower_test_package(&mut store, "Dependency.qs", dependency_source, None);

    let user_source = indoc! {
        r#"
        namespace Test {
            operation Main() : Unit {
                mutable i = 0;
                while i < 2 {
                    set i += 1;
                }
            }
        }
        "#
    };
    let (user_package, user_fir) =
        compile_and_lower_test_package(&mut store, "User.qs", user_source, Some("Test.Main()"));
    let mut fir_store = fir::PackageStore::new();
    fir_store.insert(dependency_package, dependency_fir);
    fir_store.insert(user_package, user_fir);

    let (dependency_loop_span, dependency_condition_span, dependency_body_span) = {
        let package = fir_store.get(dependency_package);
        package
            .exprs
            .values()
            .find_map(|expr| match expr.kind {
                ExprKind::While(condition, body) => Some((
                    expr.span,
                    package.get_expr(condition).span,
                    package.get_block(body).span,
                )),
                _ => None,
            })
            .expect("expected dependency while loop")
    };
    let (user_loop, user_condition, user_body) = {
        let package = fir_store.get(user_package);
        package
            .exprs
            .iter()
            .find_map(|(expr_id, expr)| match expr.kind {
                ExprKind::While(condition, body) => Some((expr_id, condition, body)),
                _ => None,
            })
            .expect("expected user while loop")
    };
    assert_eq!(dependency_loop_span.package, dependency_package);
    assert_eq!(dependency_condition_span.package, dependency_package);
    assert_eq!(dependency_body_span.package, dependency_package);
    assert_ne!(dependency_package, user_package);
    let user_fir = fir_store.get_mut(user_package);
    user_fir
        .exprs
        .get_mut(user_loop)
        .expect("user loop should exist")
        .span = dependency_loop_span;
    user_fir
        .exprs
        .get_mut(user_condition)
        .expect("user condition should exist")
        .span = dependency_condition_span;
    user_fir
        .blocks
        .get_mut(user_body)
        .expect("user loop body should exist")
        .span = dependency_body_span;

    let lookup = (&store, &fir_store);
    let loop_id = LoopId::Id(user_package, user_loop);
    let scope = lookup.resolve_scope(&Scope::Loop(loop_id));

    assert_eq!(scope.name.as_ref(), "loop: dependency_index < 17");
    assert_eq!(
        scope.location,
        Some(PackageOffset {
            package_id: dependency_loop_span.package,
            offset: dependency_loop_span.lo,
        })
    );
    assert_eq!(
        lookup
            .resolve_scope(&Scope::LoopIteration(loop_id, 0))
            .location,
        Some(PackageOffset {
            package_id: dependency_body_span.package,
            offset: dependency_body_span.lo,
        })
    );
    assert_eq!(
        lookup.resolve_logical_stack_entry_location(LogicalStackEntryLocation::Loop(loop_id)),
        Some(PackageOffset {
            package_id: dependency_loop_span.package,
            offset: dependency_loop_span.lo,
        })
    );
    assert_eq!(
        lookup.resolve_logical_stack_entry_location(LogicalStackEntryLocation::LoopIteration(
            loop_id, 0,
        )),
        Some(PackageOffset {
            package_id: dependency_body_span.package,
            offset: dependency_body_span.lo,
        })
    );
}

#[test]
fn source_locations_for_groups() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
            group_by_scope: true,
            prune_classical_qubits: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(
        &[c.user_code_frame("Main", 10), c.user_code_frame("Foo", 10)],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&c);

    expect![[r#"
        q_0    ─ [ [Main] ─── [ [Foo@user_code.qs:0:10] ── X@user_code.qs:0:10 ─── ] ──── ] ──
    "#]]
    .assert_eq(&circuit.display_with_groups().to_string());
}
