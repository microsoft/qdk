// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How far the engine's state is from an exact state vector.

use super::{Res, c, dense::Dense, rng::Rng};
use canonical_mps::{CanonicalMpsState, CpuAlgebra, CpuMpsEngine};
use tensornet::{MpsEngine, Pauli, PauliSum, PauliTerm};

/// How far the engine's state is from an exact one: the largest gap in a
/// qubit's P(One) and in a random Pauli string's expectation, and the total
/// variation distance of the whole distribution, estimated from samples.
pub struct Agreement {
    pub max_probability_gap: f64,
    pub max_pauli_gap: f64,
    pub sampled_tvd: f64,
}

/// Compares `state` (qubit `q` on site `order[q]`) with `exact`, on every
/// qubit's P(One), `strings` random Pauli strings of weight 1 to 4, and the
/// TVD from `samples` outcomes drawn from `exact`.
pub fn agreement(
    engine: &CpuMpsEngine,
    state: &CanonicalMpsState<CpuAlgebra>,
    order: &[usize],
    exact: &Dense,
    strings: usize,
    samples: usize,
) -> Res<Agreement> {
    let n = order.len();
    let mut rng = Rng::new(0x00C0_FFEE);
    let mut max_probability_gap: f64 = 0.0;
    for (q, &site) in order.iter().enumerate() {
        let p = engine.probability_of_one(state, site)?;
        max_probability_gap = max_probability_gap.max((p - exact.probability_of_one(q)).abs());
    }
    let mut max_pauli_gap: f64 = 0.0;
    for _ in 0..strings {
        let weight = 1 + rng.below(4.min(n));
        let mut qubits: Vec<usize> = (0..n).collect();
        rng.shuffle(&mut qubits);
        let factors: Vec<(usize, Pauli)> = qubits[..weight]
            .iter()
            .map(|&q| (q, [Pauli::X, Pauli::Y, Pauli::Z][rng.below(3)]))
            .collect();
        let on_sites = factors.iter().map(|&(q, p)| (order[q], p)).collect();
        let observable = PauliSum::new(c(0.0, 0.0), vec![PauliTerm::new(c(1.0, 0.0), on_sites)?])?;
        let value = engine.expectation(state, &observable)?.re;
        max_pauli_gap = max_pauli_gap.max((value - exact.pauli_expectation(&factors)).abs());
    }
    let sampled_tvd = sampled_tvd(engine, state, order, exact, samples, &mut rng)?;
    Ok(Agreement {
        max_probability_gap,
        max_pauli_gap,
        sampled_tvd,
    })
}

/// The TVD of the whole distribution, estimated from outcomes `x` drawn
/// from the exact `p`: with `q(x)` the engine's probability (projections on
/// a fork), `Σ|p − q| = E_p|1 − q/p| + (1 − E_p[q/p])`, the second term
/// being `q`'s weight outside `p`'s support. NaN for no samples.
#[expect(clippy::cast_precision_loss, reason = "a sample count")]
fn sampled_tvd(
    engine: &CpuMpsEngine,
    state: &CanonicalMpsState<CpuAlgebra>,
    order: &[usize],
    exact: &Dense,
    samples: usize,
    rng: &mut Rng,
) -> Res<f64> {
    if samples == 0 {
        return Ok(f64::NAN);
    }
    let mut cumulative = Vec::with_capacity(exact.psi.len());
    let mut total = 0.0;
    for v in &exact.psi {
        total += v.norm_sqr();
        cumulative.push(total);
    }
    let (mut deviation, mut ratio) = (0.0, 0.0);
    for _ in 0..samples {
        let u = rng.uniform() * total;
        let x = cumulative
            .partition_point(|&c| c <= u)
            .min(exact.psi.len() - 1);
        let p = exact.psi[x].norm_sqr() / total;
        let mut fork = engine.fork(state)?;
        let mut q = 1.0;
        for (qubit, &site) in order.iter().enumerate() {
            let one = x >> qubit & 1 == 1;
            let p_one = engine.probability_of_one(&fork, site)?;
            let value = if one { p_one } else { 1.0 - p_one };
            if value <= 1e-15 {
                q = 0.0;
                break;
            }
            q *= value;
            engine.project(&mut fork, site, one)?;
        }
        deviation += (1.0 - q / p).abs();
        ratio += q / p;
    }
    let n = samples as f64;
    Ok(f64::midpoint(deviation / n, (1.0 - ratio / n).max(0.0)))
}
