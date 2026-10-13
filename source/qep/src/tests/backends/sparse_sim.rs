// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::super::apply_all_gates;
use crate::{QirBackend, SparseSimBackend};
use num_bigint::BigUint;

#[test]
/// Verifies that the sparse simulator backend accepts every supported intrinsic.
fn sparse_backend_supports_all_intrinsics() -> Result<(), String> {
    let mut backend = SparseSimBackend::default();
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
/// Verifies sparse simulation, measurement caching, and reset behavior.
fn sparse_backend_simulates_and_resets_qubits() -> Result<(), String> {
    let mut backend = SparseSimBackend::default();
    backend.h(0)?;
    backend.cx(0, 1)?;
    backend.mz(0, 0)?;
    backend.mz(1, 1)?;
    assert_eq!(backend.read_result(0)?, backend.read_result(1)?);

    backend.x(2)?;
    backend.mresetz(2, 2)?;
    assert!(backend.read_result(2)?);
    backend.mz(2, 3)?;
    assert!(!backend.read_result(3)?);

    backend.x(3)?;
    backend.reset(3)?;
    backend.mz(3, 4)?;
    assert!(!backend.read_result(4)?);
    Ok(())
}

#[test]
/// Verifies that wavefunction inspection returns sparse Bell-state amplitudes.
fn sparse_backend_exposes_wavefunction() -> Result<(), String> {
    let mut backend = SparseSimBackend::default();
    backend.h(0)?;
    backend.cx(0, 1)?;

    let wavefunction = backend.wavefunction();

    assert_eq!(wavefunction.len(), 2);
    assert_eq!(wavefunction[0].0, BigUint::from(0_u8));
    assert_eq!(wavefunction[1].0, BigUint::from(3_u8));
    for (_, amplitude) in wavefunction {
        assert!((amplitude.norm_sqr() - 0.5).abs() < f64::EPSILON);
    }
    Ok(())
}
