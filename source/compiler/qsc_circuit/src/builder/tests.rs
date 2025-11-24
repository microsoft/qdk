// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::vec;

use super::*;
use expect_test::expect;
use qsc_data_structures::span::Span;
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
            .get(&scope_id)
            .expect("unknown scope id")
            .clone();
        LexicalScope::Named {
            name,
            location: PackageOffset {
                package_id: scope_id.0.package,
                offset: 0,
            },
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
        let scope_id = self
            .scopes
            .get_or_create_scope(Self::LIBRARY_PACKAGE_ID, "library_item");
        Self::frame(scope_id, offset)
    }

    pub(crate) fn user_code_frame(&mut self, name: &str, offset: u32) -> Frame {
        let scope_id = self.scopes.get_or_create_scope(Self::USER_PACKAGE_ID, name);
        Self::frame(scope_id, offset)
    }

    fn frame(scope_item_id: ScopeId, offset: u32) -> Frame {
        Frame {
            span: Span {
                lo: offset,
                hi: offset + 1,
            },
            id: scope_item_id.0,
            caller: PackageId::CORE,     // unused
            functor: Default::default(), // unused
        }
    }
}

#[derive(Default)]
pub(crate) struct Scopes {
    id_to_name: FxHashMap<ScopeId, Rc<str>>,
    name_to_id: FxHashMap<Rc<str>, ScopeId>,
}

impl Scopes {
    fn get_or_create_scope(&mut self, package_id: usize, name: &str) -> ScopeId {
        let name: Rc<str> = name.into();
        if let Some(scope_id) = self.name_to_id.get(&name) {
            *scope_id
        } else {
            let scope_id = ScopeId(StoreItemId {
                package: package_id.into(),
                item: self.id_to_name.len().into(),
            });
            self.id_to_name.insert(scope_id, name.clone());
            self.name_to_id.insert(name.clone(), scope_id);
            scope_id
        }
    }
}

#[test]
fn exceed_max_operations() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 2,
            source_locations: false,
            loop_detection: false,
            group_scopes: false,
            collapse_qubit_registers: false,
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
            group_scopes: false,
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
            group_scopes: false,
            collapse_qubit_registers: false,
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
            group_scopes: false,
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
            group_scopes: false,
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
            group_scopes: false,
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
            group_scopes: false,
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
            group_scopes: false,
            ..Default::default()
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[c.user_code_frame("Main", 10)], 0);

    builder.gate(&[], "X", false, &[0], &[], None);

    let circuit = builder.finish(&c);

    expect![[r#"
        q_0@user_code.qs:0:10 ── X ──
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
            group_scopes: false,
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
        q_0@user_code.qs:0:10 ─ X@user_code.qs:0:20 ─
        q_1@user_code.qs:0:10 ───────────────────────
    "#]]
    .assert_eq(&circuit.to_string());
}
