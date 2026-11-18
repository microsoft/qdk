// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The distribution of the qubits a program reads out at its end, and its
//! distance to a recorded one.

use super::{Res, c};
use canonical_mps::{CanonicalMpsState, CpuAlgebra, CpuMpsEngine};
use tensornet::{MpsEngine, Pauli, PauliSum, PauliTerm};

/// The distribution of `sites` read together, outcome `x` with `sites[0]`
/// as its most significant bit, from the expectations of every Z string.
pub fn readout_distribution(
    engine: &CpuMpsEngine,
    state: &CanonicalMpsState<CpuAlgebra>,
    sites: &[usize],
) -> Res<Vec<f64>> {
    let k = sites.len();
    let mut z = vec![0.0; 1 << k];
    z[0] = 1.0;
    for (subset, slot) in z.iter_mut().enumerate().skip(1) {
        let factors: Vec<(usize, Pauli)> = (0..k)
            .filter(|bit| subset >> (k - 1 - bit) & 1 == 1)
            .map(|bit| (sites[bit], Pauli::Z))
            .collect();
        let observable = PauliSum::new(c(0.0, 0.0), vec![PauliTerm::new(c(1.0, 0.0), factors)?])?;
        *slot = engine.expectation(state, &observable)?.re;
    }
    Ok((0..1usize << k)
        .map(|outcome| {
            z.iter()
                .enumerate()
                .map(|(subset, value)| {
                    if (outcome & subset).count_ones() % 2 == 1 {
                        -value
                    } else {
                        *value
                    }
                })
                .sum::<f64>()
                / f64::from(1 << k)
        })
        .collect())
}

/// The total variation distance of `distribution`, outcome `x` with the
/// first readout qubit as its most significant bit, to a recorded one
/// ([`super::recordings::readout_file`]), over all of `distribution`'s
/// outcomes: an outcome the file does not list counts as probability 0.
pub fn tvd_to(distribution: &[f64], recorded: &[(String, f64)]) -> Res<f64> {
    let mut exact = vec![0.0; distribution.len()];
    for (bits, p) in recorded {
        let outcome = usize::from_str_radix(bits, 2)?;
        *exact
            .get_mut(outcome)
            .ok_or_else(|| format!("outcome {bits} outside the readout"))? += p;
    }
    Ok(distribution
        .iter()
        .zip(exact)
        .map(|(p, q)| 0.5 * (p - q).abs())
        .sum())
}
