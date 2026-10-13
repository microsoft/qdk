// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::super::apply_all_gates;
use crate::{NoopBackend, QirBackend};
use rand::{SeedableRng, rngs::StdRng};

/// Creates a deterministic backend for measurement tests.
fn seeded_backend() -> NoopBackend {
    NoopBackend::with_rng(StdRng::seed_from_u64(42))
}

#[test]
/// Verifies that the default backend accepts every supported intrinsic.
fn default_backend_supports_all_intrinsics() -> Result<(), String> {
    let mut backend = NoopBackend::default();
    apply_all_gates(&mut backend)?;
    backend.mz(0, 0)?;
    backend.read_result(0)?;
    backend.mresetz(1, 1)?;
    backend.read_result(1)?;
    backend.write_result(2, true)?;
    assert!(backend.read_result(2)?);
    Ok(())
}

#[test]
/// Verifies that ignored gates and resets do not alter sampled measurements.
fn gates_and_reset_do_not_affect_measurements() -> Result<(), String> {
    let mut backend = seeded_backend();
    let mut reference = seeded_backend();

    for _ in 0..100 {
        apply_all_gates(&mut backend)?;
        backend.mz(0, 0)?;
        reference.mz(0, 0)?;
        assert_eq!(backend.read_result(0)?, reference.read_result(0)?);
        backend.mresetz(1, 1)?;
        reference.mz(1, 1)?;
        assert_eq!(backend.read_result(1)?, reference.read_result(1)?);
    }
    Ok(())
}

#[test]
/// Verifies that measurements are balanced and resampled on each issuance.
fn measurements_are_balanced_and_not_cached() -> Result<(), String> {
    for measure in [NoopBackend::mz, NoopBackend::mresetz] {
        let mut backend = seeded_backend();
        let mut ones = 0;
        let mut transitions = 0;
        measure(&mut backend, 0, 0)?;
        let mut previous = backend.read_result(0)?;

        for _ in 0..10_000 {
            measure(&mut backend, 0, 0)?;
            let result = backend.read_result(0)?;
            ones += usize::from(result);
            transitions += usize::from(result != previous);
            previous = result;
        }

        assert!((4_500..=5_500).contains(&ones), "ones: {ones}");
        assert!(
            (4_500..=5_500).contains(&transitions),
            "transitions: {transitions}"
        );
    }
    Ok(())
}

#[test]
/// Verifies that repeated reads are stable and do not advance the RNG.
fn reads_are_stable_and_do_not_consume_randomness() -> Result<(), String> {
    let mut backend = seeded_backend();
    let mut reference = seeded_backend();
    backend.mz(0, 7)?;
    reference.mz(0, 7)?;
    let expected = reference.read_result(7)?;
    for _ in 0..100 {
        assert_eq!(backend.read_result(7)?, expected);
    }
    backend.mz(0, 8)?;
    reference.mz(0, 8)?;
    assert_eq!(backend.read_result(8)?, reference.read_result(8)?);
    assert_eq!(backend.read_result(7)?, expected);
    backend.write_result(7, !expected)?;
    assert_eq!(backend.read_result(7)?, !expected);
    assert!(backend.read_result(9).is_err());
    Ok(())
}
