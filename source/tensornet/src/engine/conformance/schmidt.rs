// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The truncation the kit expects of an engine that truncates: at one cut,
//! the state's χ largest Schmidt pairs, not renormalized, which is what a
//! truncated singular value decomposition of the chain at that cut keeps.
//!
//! The pairs are found from the reduced density matrix of the smaller side
//! of the cut, whose eigenvectors are that side's Schmidt vectors and whose
//! eigenvalues are the squared Schmidt values: the kit's states have at
//! most five sites, so the matrix has at most 4×4 entries, and a cyclic
//! Jacobi diagonalization finds them to rounding.

use crate::reference::ReferenceState;
use num_complex::Complex64;

/// How far apart, at least, the χ-th and the next squared Schmidt value
/// must be for the kit to truncate between them: at a tie, two engines may
/// keep different pairs, each rightly.
const GAP: f64 = 1e-6;

/// What a bond of `chi` at `cut` keeps of `state`: its projection on the χ
/// Schmidt vectors of the side of the cut with fewer sites whose Schmidt
/// values are the largest, not renormalized, so its ⟨ψ|ψ⟩ is the sum of
/// their squares. Cut `k` lies between sites `k` and `k + 1`.
///
/// # Errors
///
/// When the χ-th and the next squared Schmidt values are within [`GAP`] of
/// each other while the next is above zero: the kit then has no single
/// state to expect. And when the kit's own decomposition fails, which no
/// state of the kit should make it do: its Jacobi sweeps do not converge,
/// or its squared Schmidt values do not sum to the trace of the reduced
/// density matrix to rounding.
pub(crate) fn truncated(
    state: &ReferenceState,
    cut: usize,
    chi: usize,
) -> Result<ReferenceState, String> {
    let n = state.site_count();
    assert!(cut + 1 < n, "cut {cut} lies within a chain of {n} sites");
    assert!(chi > 0, "a bond keeps at least one pair");
    // The side with fewer sites: the left one is sites 0..=cut.
    let left = 2 * (cut + 1) <= n;
    let (small, other) = if left {
        (cut + 1, n - cut - 1)
    } else {
        (n - cut - 1, cut + 1)
    };
    let index = |s: usize, o: usize| {
        if left {
            s | o << (cut + 1)
        } else {
            o | s << (cut + 1)
        }
    };
    let dim = 1 << small;
    let mut rho = vec![Complex64::new(0.0, 0.0); dim * dim];
    for x in 0..dim {
        for y in 0..dim {
            rho[x * dim + y] = (0..1 << other)
                .map(|o| state.amplitude(index(x, o)) * state.amplitude(index(y, o)).conj())
                .sum();
        }
    }
    let (values, vectors) = eigen(rho.clone(), dim)
        .ok_or_else(|| format!("the kit's own decomposition at cut {cut} did not converge"))?;
    // A unitary rotation keeps the trace: a sum that differs beyond
    // rounding is the kit's fault, reported rather than expected of an
    // engine.
    let trace: f64 = (0..dim).map(|k| rho[k * dim + k].re).sum();
    let sum: f64 = values.iter().sum();
    if (sum - trace).abs() > 1e-13 * trace.max(1.0) {
        return Err(format!(
            "the kit's own decomposition at cut {cut} lost trace: its squared \
             Schmidt values sum to {sum}, not {trace}"
        ));
    }
    let mut order: Vec<usize> = (0..dim).collect();
    order.sort_by(|&a, &b| values[b].total_cmp(&values[a]));
    if chi < dim {
        let (last, next) = (values[order[chi - 1]], values[order[chi]]);
        if next > 1e-12 && last - next < GAP {
            return Err(format!(
                "the kit's own state has a tie at cut {cut}: its squared Schmidt values \
                 {last} and {next} are too close to truncate between them"
            ));
        }
    }
    let kept = &order[..chi.min(dim)];
    let mut amplitudes = vec![Complex64::new(0.0, 0.0); 1 << n];
    for o in 0..1 << other {
        for &k in kept {
            let u = |x: usize| vectors[x * dim + k];
            let overlap: Complex64 = (0..dim)
                .map(|y| u(y).conj() * state.amplitude(index(y, o)))
                .sum();
            for x in 0..dim {
                amplitudes[index(x, o)] += u(x) * overlap;
            }
        }
    }
    Ok(ReferenceState::from_amplitudes(&amplitudes))
}

/// The squared size, relative to ‖ρ‖² (the squared Frobenius norm), below
/// which an off-diagonal entry is left alone: |b| ≤ 1e-17·‖ρ‖, about a
/// twentieth of the rounding ρ's own entries carry (ε·‖ρ‖, ε ≈ 2.2e-16).
/// Leaving it moves the eigenvalues by at most |b|²/gap and the kept
/// vectors by |b|/gap, below 1e-11 at the smallest gap the kit truncates
/// across ([`GAP`]), far below the kit's tolerance; and it keeps the
/// rotation's phase away from subnormal entries, whose few mantissa bits
/// gave a phase of modulus other than 1, a rotation that was not unitary,
/// and a decomposition that lost trace. A subnormal `b` is always left
/// alone: its `norm_sqr()` underflows to 0, at or below any bound.
const NEGLIGIBLE: f64 = 1e-34;

/// At most this many sweeps; cyclic Jacobi converges quadratically, so a
/// 4×4 matrix needs a handful.
const SWEEPS: usize = 100;

/// The eigenvalues of the Hermitian `dim`×`dim` row-major matrix `a`, and
/// its eigenvectors as the columns of a row-major unitary, by cyclic
/// Jacobi rotations until a sweep finds no entry above [`NEGLIGIBLE`];
/// `None` when [`SWEEPS`] do not reach that.
#[expect(
    clippy::many_single_char_names,
    reason = "the Jacobi method's textbook names"
)]
fn eigen(mut a: Vec<Complex64>, dim: usize) -> Option<(Vec<f64>, Vec<Complex64>)> {
    let mut v: Vec<Complex64> = (0..dim * dim)
        .map(|at| Complex64::new(if at % (dim + 1) == 0 { 1.0 } else { 0.0 }, 0.0))
        .collect();
    let scale: f64 = a.iter().map(Complex64::norm_sqr).sum();
    for _ in 0..SWEEPS {
        let mut rotated = false;
        for p in 0..dim {
            for q in p + 1..dim {
                let b = a[p * dim + q];
                if b.norm_sqr() <= NEGLIGIBLE * scale {
                    continue;
                }
                rotated = true;
                // A phase on q makes the pair's entry real, |b|; a real
                // rotation then zeroes it. Above `NEGLIGIBLE`, |b| is at
                // least 1e-17·‖ρ‖, far from subnormal, so the phase has
                // modulus 1 to rounding and `w` is unitary.
                let phase = (b / b.norm()).conj();
                let theta = (a[q * dim + q].re - a[p * dim + p].re) / (2.0 * b.norm());
                let t = theta.signum() / (theta.abs() + theta.hypot(1.0));
                let cos = 1.0 / t.hypot(1.0);
                let sin = t * cos;
                // The 2×2 unitary w = diag(1, phase)·[[cos, sin], [−sin, cos]].
                let w = [
                    Complex64::new(cos, 0.0),
                    Complex64::new(sin, 0.0),
                    -phase * sin,
                    phase * cos,
                ];
                for k in 0..dim {
                    let (kp, kq) = (a[k * dim + p], a[k * dim + q]);
                    a[k * dim + p] = kp * w[0] + kq * w[2];
                    a[k * dim + q] = kp * w[1] + kq * w[3];
                    let (vp, vq) = (v[k * dim + p], v[k * dim + q]);
                    v[k * dim + p] = vp * w[0] + vq * w[2];
                    v[k * dim + q] = vp * w[1] + vq * w[3];
                }
                for k in 0..dim {
                    let (pk, qk) = (a[p * dim + k], a[q * dim + k]);
                    a[p * dim + k] = w[0].conj() * pk + w[2].conj() * qk;
                    a[q * dim + k] = w[1].conj() * pk + w[3].conj() * qk;
                }
                // The rotation zeroes the pair's entry; what rounding
                // leaves there would only be rotated again, sweep after
                // sweep.
                a[p * dim + q] = Complex64::new(0.0, 0.0);
                a[q * dim + p] = Complex64::new(0.0, 0.0);
            }
        }
        if !rotated {
            return Some(((0..dim).map(|k| a[k * dim + k].re).collect(), v));
        }
    }
    None
}

/// `state` divided by its norm.
pub(crate) fn normalized(state: &ReferenceState) -> ReferenceState {
    let amplitudes: Vec<Complex64> = (0..1 << state.site_count())
        .map(|i| state.amplitude(i))
        .collect();
    let norm = amplitudes
        .iter()
        .map(Complex64::norm_sqr)
        .sum::<f64>()
        .sqrt();
    let amplitudes: Vec<Complex64> = amplitudes.iter().map(|a| a / norm).collect();
    ReferenceState::from_amplitudes(&amplitudes)
}
