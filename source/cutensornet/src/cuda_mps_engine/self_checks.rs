// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): [`self_check`], which runs
//! `GateSplit` calls on [`DeviceTensors`] and checks each against the
//! gated block `θ` (the pair contracted with the gate, by plain loops on
//! the host), to answer on the GPU what the documentation leaves open:
//! - `U·V` is `θ` normalised, with and without a reduction (F8: U and V are
//!   laid out at the reduced extent): `‖θ̂ − √(1 − d)·U·V‖² = d`, the
//!   reported discarded weight `d` (Eckart–Young), so a cut by the fixed
//!   extent counts in `d` too;
//! - the cutoff discards at most its weight;
//! - a cutoff that would drop every value keeps the first (F2), as the CPU
//!   engine's rule does;
//! - the side the partition names is the one left non-isometric;
//! - informational: whether the singular values are written under a
//!   US/SV partition and the cutoff kept no more values than it needs; at
//!   a tie (a Bell pair, two equal values, the cutoff at half the weight),
//!   which way the device rounds.
//!
//! Agreement with the CPU engine's own SVD is checked by the engine-level
//! tests on the GPU, against `CpuMpsEngine`.

use super::{DeviceTensors, GateSplitCall, Partition, TensorCalls};
use num_complex::Complex64;

/// One check's outcome: its name, whether it held, and what it measured.
#[derive(Clone, Debug)]
pub struct SelfCheck {
    /// What it checks (`product-us-no-cutoff`).
    pub name: String,
    /// Whether it held.
    pub passed: bool,
    /// What it measured.
    pub detail: String,
}

/// The checks on `device` (see the module's documentation), each on
/// random sites whose singular values decay geometrically.
#[must_use]
pub fn self_check(device: &DeviceTensors) -> Vec<SelfCheck> {
    let mut checks = Vec::new();
    let mut random = SplitMix(0x5eed);
    for &(l, m, r) in &[(1, 1, 1), (2, 2, 4), (8, 8, 8), (16, 12, 8), (32, 32, 32)] {
        let left = graded(&mut random, l * 2 * m, l, m);
        let right = random.complex_vec(m * 2 * r);
        let gate = random.gate();
        let full = (2 * l).min(2 * r).min(4 * m);
        let cases = [
            ("no-cutoff", None, full),
            ("cutoff", Some(1e-6), full),
            ("extent", Some(1e-14), full.div_ceil(2).max(1)),
            ("first-value", Some(0.999), full),
        ];
        for (case, cutoff, max_extent) in cases {
            for partition in [Partition::Us, Partition::Sv] {
                let name = format!("l{l}-m{m}-r{r}-{case}-{partition:?}");
                checks.push(compare(
                    device, &name, &left, &right, l, m, r, &gate, max_extent, cutoff, partition,
                ));
            }
        }
    }
    let one = Complex64::new(1.0, 0.0);
    let zero = Complex64::new(0.0, 0.0);
    let half = Complex64::new(std::f64::consts::FRAC_1_SQRT_2, 0.0);
    let mut bell = [zero; 16];
    bell[0] = half;
    bell[3 * 4] = half;
    for partition in [Partition::Us, Partition::Sv] {
        let mut check = compare(
            device,
            &format!("l1-m1-r1-tie-{partition:?}"),
            &[one, zero],
            &[one, zero],
            1,
            1,
            1,
            &bell,
            2,
            Some(0.5),
            partition,
        );
        check.passed = true;
        check
            .detail
            .push_str(" informational=tie-rounds-either-way");
        checks.push(check);
    }
    checks
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    reason = "a development check"
)]
fn compare(
    device: &DeviceTensors,
    name: &str,
    left: &[Complex64],
    right: &[Complex64],
    l: usize,
    m: usize,
    r: usize,
    gate: &[Complex64; 16],
    max_extent: usize,
    cutoff: Option<f64>,
    partition: Partition,
) -> SelfCheck {
    let outcome = (|| {
        let device_left = device.upload(left)?;
        let device_right = device.upload(right)?;
        let mut on_device_call = call(
            &device_left,
            &device_right,
            l,
            m,
            r,
            gate,
            max_extent,
            cutoff,
            partition,
        );
        on_device_call.read_values = true;
        let on_device = device.gate_split(&on_device_call)?;
        let k = on_device.reduced;
        let device_u = device.download(&on_device.u, l * 2 * k)?;
        let device_v = device.download(&on_device.v, k * 2 * r)?;
        Ok::<_, super::CudaMpsError>((on_device, device_u, device_v))
    })();
    let (on_device, device_u, device_v) = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            return SelfCheck {
                name: name.to_owned(),
                passed: false,
                detail: format!("error={error}"),
            };
        }
    };
    let k = on_device.reduced;
    let d = on_device.discarded_weight;
    let rows = 2 * l;
    let cols = 2 * r;
    let theta = gated_block(left, right, l, m, r, gate);
    let theta_norm = theta.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
    let rescale = (1.0 - d).max(0.0).sqrt();
    let device_product = product(&device_u, &device_v, rows, k, cols);
    let residual = theta
        .iter()
        .zip(&device_product)
        .map(|(t, p)| (t / theta_norm - p * rescale).norm_sqr())
        .sum::<f64>();
    let product_error = (residual - d).abs();
    let isometry = match partition {
        Partition::Us => right_isometry_error(&device_v, k, cols),
        Partition::Sv => left_isometry_error(&device_u, rows, k),
    };
    let within_cutoff =
        cutoff.is_none_or(|cutoff| on_device.full > max_extent || d <= cutoff + 1e-12);
    let all_kept = cutoff.is_some() || max_extent < on_device.full || k == on_device.full;
    let passed =
        k <= max_extent && product_error < 1e-10 && isometry < 1e-10 && within_cutoff && all_kept;
    // Informational: the values as the device wrote them (normalised to an
    // L2 norm of 1 when written), and whether one fewer value would still
    // have met the cutoff.
    let values = &on_device.singular_values;
    let values_norm = values.iter().map(|s| s * s).sum::<f64>().sqrt();
    let minimal = match (cutoff, values.last()) {
        (Some(cutoff), Some(&last)) if k > 1 && k < max_extent => {
            (d + last * last * (1.0 - d) > cutoff).to_string()
        }
        _ => "-".to_owned(),
    };
    SelfCheck {
        name: name.to_owned(),
        passed,
        detail: format!(
            "reduced={k} full={} max_extent={max_extent} discarded={d:.6e} \
             product_error={product_error:.2e} isometry_error={isometry:.2e} \
             values_norm={values_norm:.6} minimal={minimal}",
            on_device.full,
        ),
    }
}

/// `θ[(a, s), (t, b)] = Σ G[(s, t), (p, q)] · A[a, p, j] · B[j, q, b]`,
/// column-major `2·l × 2·r`, the row `a + l·s` and the column `t + 2·b`.
#[allow(clippy::many_single_char_names, reason = "the formula's indices")]
fn gated_block(
    left: &[Complex64],
    right: &[Complex64],
    l: usize,
    m: usize,
    r: usize,
    gate: &[Complex64; 16],
) -> Vec<Complex64> {
    let rows = 2 * l;
    let zero = Complex64::new(0.0, 0.0);
    let mut x = vec![zero; rows * 2 * r];
    for column in 0..2 * r {
        for j in 0..m {
            let b = right[j + m * column];
            for row in 0..rows {
                x[row + rows * column] += left[row + rows * j] * b;
            }
        }
    }
    let mut theta = vec![zero; rows * 2 * r];
    for b in 0..r {
        for s in 0..2 {
            for t in 0..2 {
                for p in 0..2 {
                    for q in 0..2 {
                        let g = gate[(2 * s + t) * 4 + 2 * p + q];
                        let from = rows * (q + 2 * b) + l * p;
                        let to = rows * (t + 2 * b) + l * s;
                        for a in 0..l {
                            theta[to + a] += g * x[from + a];
                        }
                    }
                }
            }
        }
    }
    theta
}

#[allow(clippy::too_many_arguments, reason = "a development check")]
fn call<'b, B>(
    left: &'b B,
    right: &'b B,
    l: usize,
    m: usize,
    r: usize,
    gate: &[Complex64; 16],
    max_extent: usize,
    cutoff: Option<f64>,
    partition: Partition,
) -> GateSplitCall<'b, B> {
    GateSplitCall {
        left,
        right,
        left_extent: l,
        bond: m,
        right_extent: r,
        gate: *gate,
        max_extent,
        cutoff,
        partition,
        read_values: false,
    }
}

/// `U (rows × k) · V (k × cols)`, column-major.
fn product(u: &[Complex64], v: &[Complex64], rows: usize, k: usize, cols: usize) -> Vec<Complex64> {
    let mut out = vec![Complex64::new(0.0, 0.0); rows * cols];
    for column in 0..cols {
        for inner in 0..k {
            let b = v[inner + k * column];
            for row in 0..rows {
                out[row + rows * column] += u[row + rows * inner] * b;
            }
        }
    }
    out
}

/// `‖U†U − I‖` of `U` (`rows × k`).
fn left_isometry_error(u: &[Complex64], rows: usize, k: usize) -> f64 {
    let mut error = 0.0_f64;
    for i in 0..k {
        for j in 0..k {
            let dot: Complex64 = (0..rows)
                .map(|row| u[row + rows * i].conj() * u[row + rows * j])
                .sum();
            let expected = if i == j { 1.0 } else { 0.0 };
            error = error.max((dot - expected).norm());
        }
    }
    error
}

/// `‖V V† − I‖` of `V` (`k × cols`).
fn right_isometry_error(v: &[Complex64], k: usize, cols: usize) -> f64 {
    let mut error = 0.0_f64;
    for i in 0..k {
        for j in 0..k {
            let dot: Complex64 = (0..cols)
                .map(|column| v[i + k * column] * v[j + k * column].conj())
                .sum();
            let expected = if i == j { 1.0 } else { 0.0 };
            error = error.max((dot - expected).norm());
        }
    }
    error
}

/// A random `[l, 2, m]` tensor whose last mode's slices decay by 1e-2.
fn graded(random: &mut SplitMix, count: usize, l: usize, m: usize) -> Vec<Complex64> {
    let mut values = random.complex_vec(count);
    for j in 0..m {
        let scale = 1e-2_f64.powi(i32::try_from(j).expect("a small bond"));
        for x in &mut values[2 * l * j..2 * l * (j + 1)] {
            *x *= scale;
        }
    }
    values
}

struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        #[allow(clippy::cast_precision_loss, reason = "53 random bits")]
        let unit = (z >> 11) as f64 / (1_u64 << 53) as f64;
        2.0 * unit - 1.0
    }

    fn complex_vec(&mut self, count: usize) -> Vec<Complex64> {
        (0..count)
            .map(|_| Complex64::new(self.next(), self.next()))
            .collect()
    }

    fn gate(&mut self) -> [Complex64; 16] {
        std::array::from_fn(|_| Complex64::new(self.next(), self.next()))
    }
}
