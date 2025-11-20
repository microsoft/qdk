// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use expect_test::expect;
use qsc_data_structures::span::Span;

struct FakeCompilation {}

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
}
impl FakeCompilation {
    const LIBRARY_PACKAGE_ID: usize = 0;
    const USER_PACKAGE_ID: usize = 2;

    fn user_package_ids() -> Vec<PackageId> {
        vec![Self::USER_PACKAGE_ID.into()]
    }

    fn library_frame(offset: u32) -> Frame {
        Self::frame(Self::LIBRARY_PACKAGE_ID, offset)
    }

    fn user_code_frame(offset: u32) -> Frame {
        Self::frame(Self::USER_PACKAGE_ID, offset)
    }

    fn frame(package_id: usize, offset: u32) -> Frame {
        Frame {
            span: Span {
                lo: offset,
                hi: offset + 1,
            },
            id: qsc_fir::fir::StoreItemId {
                package: package_id.into(),
                item: 0.into(),
            },
            caller: PackageId::CORE,
            functor: Default::default(),
        }
    }
}

#[test]
fn exceed_max_operations() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 2,
            source_locations: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(&[], "X", false, &[0], &[], None);
    builder.gate(&[], "X", false, &[0], &[], None);
    builder.gate(&[], "X", false, &[0], &[], None);

    let circuit = builder.finish(&FakeCompilation {});

    // The current behavior is to silently truncate the circuit
    // if it exceeds the maximum allowed number of operations.
    expect![[r#"
        q_0    ── X ──── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn source_locations_enabled() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(
        &[FakeCompilation::user_code_frame(10)],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&FakeCompilation {});

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
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: false,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(
        &[FakeCompilation::user_code_frame(10)],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&FakeCompilation {});

    expect![[r#"
        q_0    ── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn source_locations_multiple_user_frames() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(
        &[
            FakeCompilation::user_code_frame(10),
            FakeCompilation::user_code_frame(20),
        ],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&FakeCompilation {});

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
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);
    builder.gate(
        &[
            FakeCompilation::user_code_frame(10),
            FakeCompilation::library_frame(20),
        ],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&FakeCompilation {});

    // Most recent frame is a library frame - source
    // location should fall back to the nearest user frame.
    expect![[r#"
        q_0    ─ X@user_code.qs:0:10 ─
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn source_locations_only_library_frames() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(
        &[
            FakeCompilation::library_frame(20),
            FakeCompilation::library_frame(30),
        ],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&FakeCompilation {});

    // Only library frames, no user source to show
    expect![[r#"
        q_0    ── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn source_locations_enabled_no_stack() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[], 0);

    builder.gate(&[], "X", false, &[0], &[], None);

    let circuit = builder.finish(&FakeCompilation {});

    // No stack was passed, so no source location to show
    expect![[r#"
        q_0    ── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn qubit_source_locations_via_stack() {
    let mut builder = CircuitTracer::new(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
        },
        &FakeCompilation::user_package_ids(),
    );

    builder.qubit_allocate(&[FakeCompilation::user_code_frame(10)], 0);

    builder.gate(&[], "X", false, &[0], &[], None);

    let circuit = builder.finish(&FakeCompilation {});

    expect![[r#"
        q_0@user_code.qs:0:10 ── X ──
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn qubit_labels_for_preallocated_qubits() {
    let mut builder = CircuitTracer::with_qubit_input_params(
        TracerConfig {
            max_operations: 10,
            source_locations: true,
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
        &[FakeCompilation::user_code_frame(20)],
        "X",
        false,
        &[0],
        &[],
        None,
    );

    let circuit = builder.finish(&FakeCompilation {});

    expect![[r#"
        q_0@user_code.qs:0:10 ─ X@user_code.qs:0:20 ─
        q_1@user_code.qs:0:10 ───────────────────────
    "#]]
    .assert_eq(&circuit.to_string());
}
