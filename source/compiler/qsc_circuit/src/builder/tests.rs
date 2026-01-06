// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::unicode_not_nfc)]

mod group_scopes;
mod prune_classical_qubits;

use std::vec;

use super::*;
use expect_test::expect;
use qsc_data_structures::{functors::FunctorApp, span::Span};
use rustc_hash::FxHashMap;

#[derive(Default)]
pub(crate) struct FakeCompilation {
    scopes: Scopes,
}

impl SourceLookup for FakeCompilation {
    fn resolve_location(&self, package_offset: &PackageOffset) -> ResolvedSourceLocation {
        ResolvedSourceLocation {
            file: match usize::from(package_offset.package_id) {
                Self::USER_PACKAGE_ID => "user_code.qs".to_string(),
                Self::LIBRARY_PACKAGE_ID => "library_code.qs".to_string(),
                _ => panic!("unexpected package id"),
            },
            line: 0,
            column: package_offset.offset,
        }
    }

    fn resolve_scope(&self, scope_id: ScopeId) -> LexicalScope {
        let name = self
            .scopes
            .id_to_name
            .get(&scope_id.0)
            .expect("unknown scope id")
            .clone();
        LexicalScope::Callable {
            name,
            location: PackageOffset {
                package_id: scope_id.0.package,
                offset: 0,
            },
            functor_app: scope_id.1,
        }
    }
}

impl FakeCompilation {
    const LIBRARY_PACKAGE_ID: usize = 0;
    pub(crate) const USER_PACKAGE_ID: usize = 2;

    pub(crate) fn user_package_ids() -> Vec<PackageId> {
        vec![Self::USER_PACKAGE_ID.into()]
    }

    fn library_frame(&mut self, offset: u32) -> Frame {
        let scope_id =
            self.scopes
                .get_or_create_scope(Self::LIBRARY_PACKAGE_ID, "library_item", false);
        Self::frame(scope_id, offset, false)
    }

    fn user_code_frame(&mut self, scope_name: &str, offset: u32) -> Frame {
        let scope_id = self
            .scopes
            .get_or_create_scope(Self::USER_PACKAGE_ID, scope_name, false);
        Self::frame(scope_id, offset, false)
    }

    fn user_code_adjoint_frame(&mut self, scope_name: &str, offset: u32) -> Frame {
        let scope_id = self
            .scopes
            .get_or_create_scope(Self::USER_PACKAGE_ID, scope_name, true);
        Self::frame(scope_id, offset, true)
    }

    fn frame(scope_item_id: ScopeId, offset: u32, is_adjoint: bool) -> Frame {
        Frame {
            span: Span {
                lo: offset,
                hi: offset + 1,
            },
            id: scope_item_id.0,
            caller: PackageId::CORE, // unused in tests
            functor: FunctorApp {
                adjoint: is_adjoint,
                controlled: 0,
            },
        }
    }
}

#[derive(Default)]
struct Scopes {
    id_to_name: FxHashMap<StoreItemId, Rc<str>>,
    name_to_id: FxHashMap<Rc<str>, StoreItemId>,
}

impl Scopes {
    fn get_or_create_scope(&mut self, package_id: usize, name: &str, is_adjoint: bool) -> ScopeId {
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
        ScopeId(
            item_id,
            FunctorApp {
                adjoint: is_adjoint,
                controlled: 0,
            },
        )
    }
}

#[test]
fn exceed_max_operations() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 2,
            source_locations: false,
            loop_detection: false,
            group_by_scope: false,
            ..Default::default()
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
            ..Default::default()
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
    .assert_eq(&circuit.display_basic().to_string());
}

#[test]
fn source_locations_disabled() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: false,
            loop_detection: false,
            group_by_scope: false,
            ..Default::default()
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
            ..Default::default()
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
    .assert_eq(&circuit.display_basic().to_string());
}

#[test]
fn source_locations_library_frames_excluded() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
            group_by_scope: false,
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
fn qubit_source_locations_via_stack() {
    let mut c = FakeCompilation::default();
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
            group_by_scope: false,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[c.user_code_frame("Main", 10)], 0);

    builder.gate(&[], "X", false, &[0], &[], None);

    let circuit = builder.finish(&c);

    expect![[r#"
        q_0@user_code.qs:0:10  ── X ──
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
            ..Default::default()
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
            ..Default::default()
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
