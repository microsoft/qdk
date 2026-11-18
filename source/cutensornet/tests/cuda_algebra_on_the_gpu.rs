// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA algebra and the CUDA MPS engine on a GPU: the algebra meets
//! `SiteAlgebra`'s contract (`canonical_mps`'s conformance kit), and the
//! engine runs `canonical_mps`'s circuits against the exact reference, and
//! a capped circuit as the engine on the CPU does.
//!
//! It needs a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13, so
//! it is ignored in CI; run it with `--ignored` on such a host. The
//! algebra opens on the real libraries only on Linux on x86-64, the
//! platform they were audited on.

#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use canonical_mps::{
    CanonicalMpsState, CpuAlgebra, CpuMpsEngine, engine_conformance, site_algebra::conformance,
};
use num_complex::Complex64;
use qdk_cutensornet::{CudaAlgebra, CudaMpsEngine, discover};
use tensornet::{MpsEngine, MpsShape};

fn algebra() -> CudaAlgebra {
    let availability = discover().expect("the audited libraries are installed");
    CudaAlgebra::new(&availability).expect("an algebra opens on device 0")
}

/// The engine at a cutoff of 0, as the capped circuit compares it with the
/// CPU engine.
fn engine() -> CudaMpsEngine {
    CudaMpsEngine::new(algebra()).with_cutoff(0.0)
}

/// `canonical_mps`'s conformance kit passes on cuTensorNet's gate splits
/// and QRs.
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn the_cuda_algebra_conforms() {
    let failures = conformance::check(&algebra());
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A long circuit, untruncated, reads as the exact reference does
/// (`canonical_mps::engine_conformance`).
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn an_untruncated_circuit_agrees_with_the_exact_reference() {
    engine_conformance::assert_untruncated_circuit_agrees_with_exact_reference(algebra());
}

/// A truncating circuit stays within its truncation bound of the exact
/// reference (`canonical_mps::engine_conformance`).
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn a_truncated_circuit_stays_within_its_truncation_bound() {
    engine_conformance::assert_truncated_circuit_stays_within_its_truncation_bound(algebra());
}

/// A pseudo-random circuit on 6 sites at a bond of 3, which truncates at
/// most splits, with projections on the way, reads as on the engine on
/// the CPU: every P(One), and the retained weight, agree within `1e-9`,
/// since both drop the same singular values. The truncating circuit above
/// checks its truncation bound; this one checks, read by read, that
/// cuTensorNet truncates as the CPU algebra does.
#[test]
#[ignore = "requires a CUDA device, the CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn a_capped_circuit_reads_as_on_the_cpu_engine() {
    let sites = 6;
    let engine = engine();
    let cpu = CpuMpsEngine::new(CpuAlgebra).with_cutoff(0.0);
    let chain = MpsShape::qubit_chain(sites, 3).expect("a chain of qubits");
    let mut state = engine.prepare(&chain).expect("prepare");
    let mut cpu_state = cpu.prepare(&chain).expect("prepare");
    let mut next = xorshift(0x2545_F491_4F6C_DD1D);
    let agree = |state: &CanonicalMpsState<CudaAlgebra>,
                 cpu_state: &CanonicalMpsState<CpuAlgebra>| {
        for site in 0..sites {
            let p = engine.probability_of_one(state, site).expect("P(One)");
            let q = cpu.probability_of_one(cpu_state, site).expect("P(One)");
            assert!((p - q).abs() < 1e-9, "site {site}: {p} vs {q}");
        }
        let w = engine.retained_weight(state).expect("weight");
        let v = cpu.retained_weight(cpu_state).expect("weight");
        assert!((w - v).abs() < 1e-9, "retained weight: {w} vs {v}");
    };
    for step in 0..40 {
        let x = f64::from(u32::try_from(step).expect("small"));
        let a = next(sites);
        let b = (a + 1 + next(sites - 1)) % sites;
        let gate = if step % 4 == 0 {
            cnot()
        } else {
            entangler(0.4 + 0.07 * x)
        };
        let turn = ry(0.3 + 0.02 * x);
        for (on, gate) in [(&[a, b][..], &gate), (&[b], &turn)] {
            engine.apply(&mut state, on, gate).expect("apply");
            cpu.apply(&mut cpu_state, on, gate).expect("apply");
        }
        if step % 10 == 9 {
            agree(&state, &cpu_state);
            let site = next(sites);
            let one = cpu.probability_of_one(&cpu_state, site).expect("P(One)") > 0.5;
            engine.project(&mut state, site, one).expect("project");
            cpu.project(&mut cpu_state, site, one).expect("project");
        }
    }
    agree(&state, &cpu_state);
    let weight = cpu.retained_weight(&cpu_state).expect("weight");
    assert!(
        weight < 1.0 - 1e-6,
        "precondition: the cap truncates ({weight})"
    );
}

fn real(re: f64) -> Complex64 {
    Complex64::new(re, 0.0)
}

fn ry(theta: f64) -> Vec<Complex64> {
    let (s, k) = (theta / 2.0).sin_cos();
    vec![real(k), real(-s), real(s), real(k)]
}

fn cnot() -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    for (row, col) in [(0, 0), (1, 1), (2, 3), (3, 2)] {
        m[4 * row + col] = real(1.0);
    }
    m
}

/// A complex entangling gate: a partial iSWAP on |01⟩, |10⟩ and a phase on
/// |11⟩.
fn entangler(theta: f64) -> Vec<Complex64> {
    let mut m = vec![real(0.0); 16];
    let (s, k) = (theta / 2.0).sin_cos();
    m[0] = real(1.0);
    m[4 + 1] = real(k);
    m[4 + 2] = Complex64::new(0.0, -s);
    m[8 + 1] = Complex64::new(0.0, -s);
    m[8 + 2] = real(k);
    m[15] = Complex64::from_polar(1.0, theta);
    m
}

/// A xorshift generator from `seed`: each call gives a number below its
/// argument, the same sequence on every run.
fn xorshift(mut seed: u64) -> impl FnMut(usize) -> usize {
    move |below| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        usize::try_from(seed % u64::try_from(below).expect("small")).expect("small")
    }
}
