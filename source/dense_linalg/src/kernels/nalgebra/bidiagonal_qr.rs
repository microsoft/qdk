// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`bidiagonal_qr`]: the SVD of a real upper bidiagonal matrix by the
//! implicit QR algorithm, a port of LAPACK `dbdsqr`'s, with its deflation
//! and convergence tests and its zero-shift sweeps, so that the small
//! values keep their relative accuracy.
//!
//! nalgebra's own QR algorithm on the bidiagonal is not used: nalgebra
//! 0.35's SVD returned wrong factors, without reporting a failure, on 8 of
//! 127 rank-deficient and degenerate matrices.

/// The SVD `B = U·diag(d)·Vᵀ` of the real upper bidiagonal `B` (diagonal
/// `d`, superdiagonal `e`) by LAPACK `dbdsqr`'s algorithm with relative
/// accuracy (`tol > 0`): `u` and `v` (`n × n`, column-major) are multiplied
/// on the right by the rotations, `d` ends nonnegative and nonincreasing.
/// False when the iteration does not converge in `6·n²` inner steps.
#[expect(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "LAPACK dbdsqr's names and structure, for comparison with it"
)]
pub(crate) fn bidiagonal_qr(d: &mut [f64], e: &mut [f64], u: &mut [f64], v: &mut [f64]) -> bool {
    const MAX_ITERATIONS: usize = 6;
    let n = d.len();
    // LAPACK's ε is the unit roundoff.
    let eps = f64::EPSILON / 2.0;
    let underflow = f64::MIN_POSITIVE;
    let tolerance_factor = 10f64.max(100f64.min(eps.powf(-0.125)));
    let tol = tolerance_factor * eps;
    #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
    let size = n as f64;
    if n > 1 {
        // A lower bound on the smallest singular value, for the absolute
        // threshold below which an off-diagonal is set to zero.
        let mut smallest = d[0].abs();
        if smallest != 0.0 {
            let mut mu = smallest;
            for i in 1..n {
                mu = d[i].abs() * (mu / (mu + e[i - 1].abs()));
                smallest = smallest.min(mu);
                if smallest == 0.0 {
                    break;
                }
            }
        }
        smallest /= size.sqrt();
        #[expect(clippy::cast_precision_loss, reason = "a small constant")]
        let thresh = (tol * smallest).max(MAX_ITERATIONS as f64 * (size * (size * underflow)));
        let budget = MAX_ITERATIONS * n * n;
        let mut iterations = 0;
        let (mut old_top, mut old_bottom) = (usize::MAX, usize::MAX);
        let mut chase_down = true;
        let mut m = n - 1;
        'main: while m > 0 {
            if iterations > budget {
                return false;
            }
            // The block [ll, m] with every off-diagonal above `thresh`.
            let mut largest = d[m].abs();
            let mut split = None;
            for l in (0..m).rev() {
                let (abs_d, abs_e) = (d[l].abs(), e[l].abs());
                if abs_e <= thresh {
                    split = Some(l);
                    break;
                }
                largest = largest.max(abs_d).max(abs_e);
            }
            let ll = match split {
                Some(l) => {
                    e[l] = 0.0;
                    if l == m - 1 {
                        m -= 1;
                        continue;
                    }
                    l + 1
                }
                None => 0,
            };
            if ll == m - 1 {
                let (s_min, s_max, sin_r, cos_r, sin_l, cos_l) = lasv2(d[m - 1], e[m - 1], d[m]);
                d[m - 1] = s_max;
                e[m - 1] = 0.0;
                d[m] = s_min;
                rotate(v, n, m - 1, cos_r, sin_r);
                rotate(u, n, m - 1, cos_l, sin_l);
                if m < 2 {
                    break;
                }
                m -= 2;
                continue;
            }
            // A new block: chase the bulge from its larger end.
            if old_bottom == usize::MAX || ll > old_bottom || m < old_top {
                chase_down = d[ll].abs() >= d[m].abs();
            }
            // Convergence tests.
            let mut smallest_lower;
            if chase_down {
                if e[m - 1].abs() <= tol * d[m].abs() {
                    e[m - 1] = 0.0;
                    continue;
                }
                let mut mu = d[ll].abs();
                smallest_lower = mu;
                for l in ll..m {
                    if e[l].abs() <= tol * mu {
                        e[l] = 0.0;
                        continue 'main;
                    }
                    mu = d[l + 1].abs() * (mu / (mu + e[l].abs()));
                    smallest_lower = smallest_lower.min(mu);
                }
            } else {
                if e[ll].abs() <= tol * d[ll].abs() {
                    e[ll] = 0.0;
                    continue;
                }
                let mut mu = d[m].abs();
                smallest_lower = mu;
                for l in (ll..m).rev() {
                    if e[l].abs() <= tol * mu {
                        e[l] = 0.0;
                        continue 'main;
                    }
                    mu = d[l].abs() * (mu / (mu + e[l].abs()));
                    smallest_lower = smallest_lower.min(mu);
                }
            }
            old_top = ll;
            old_bottom = m;
            // The shift: zero when it would spoil the relative accuracy of
            // the smallest value, else the smaller value of the 2 × 2 at
            // the far end.
            let mut shift = 0.0;
            if size * tol * (smallest_lower / largest) > eps.max(0.01 * tol) {
                let near;
                if chase_down {
                    near = d[ll].abs();
                    shift = las2(d[m - 1], e[m - 1], d[m]).0;
                } else {
                    near = d[m].abs();
                    shift = las2(d[ll], e[ll], d[ll + 1]).0;
                }
                if near > 0.0 && (shift / near) * (shift / near) < eps {
                    shift = 0.0;
                }
            }
            iterations += m - ll;
            if shift == 0.0 {
                // Demmel–Kahan zero-shift sweep.
                let (mut cs, mut old_cs, mut old_sn) = (1.0, 1.0, 0.0);
                if chase_down {
                    for i in ll..m {
                        let (c, sn, r) = lartg(d[i] * cs, e[i]);
                        cs = c;
                        if i > ll {
                            e[i - 1] = old_sn * r;
                        }
                        let (oc, os, di) = lartg(old_cs * r, d[i + 1] * sn);
                        old_cs = oc;
                        old_sn = os;
                        d[i] = di;
                        rotate(v, n, i, cs, sn);
                        rotate(u, n, i, old_cs, old_sn);
                    }
                    let h = d[m] * cs;
                    d[m] = h * old_cs;
                    e[m - 1] = h * old_sn;
                    if e[m - 1].abs() <= thresh {
                        e[m - 1] = 0.0;
                    }
                } else {
                    for i in (ll + 1..=m).rev() {
                        let (c, sn, r) = lartg(d[i] * cs, e[i - 1]);
                        cs = c;
                        if i < m {
                            e[i] = old_sn * r;
                        }
                        let (oc, os, di) = lartg(old_cs * r, d[i - 1] * sn);
                        old_cs = oc;
                        old_sn = os;
                        d[i] = di;
                        rotate(v, n, i - 1, old_cs, -old_sn);
                        rotate(u, n, i - 1, cs, -sn);
                    }
                    let h = d[ll] * cs;
                    d[ll] = h * old_cs;
                    e[ll] = h * old_sn;
                    if e[ll].abs() <= thresh {
                        e[ll] = 0.0;
                    }
                }
            } else if chase_down {
                let mut f = (d[ll].abs() - shift) * (1f64.copysign(d[ll]) + shift / d[ll]);
                let mut g = e[ll];
                for i in ll..m {
                    let (cos_r, sin_r, r) = lartg(f, g);
                    if i > ll {
                        e[i - 1] = r;
                    }
                    f = cos_r * d[i] + sin_r * e[i];
                    e[i] = cos_r * e[i] - sin_r * d[i];
                    g = sin_r * d[i + 1];
                    d[i + 1] *= cos_r;
                    let (cos_l, sin_l, r) = lartg(f, g);
                    d[i] = r;
                    f = cos_l * e[i] + sin_l * d[i + 1];
                    d[i + 1] = cos_l * d[i + 1] - sin_l * e[i];
                    if i + 1 < m {
                        g = sin_l * e[i + 1];
                        e[i + 1] *= cos_l;
                    }
                    rotate(v, n, i, cos_r, sin_r);
                    rotate(u, n, i, cos_l, sin_l);
                }
                e[m - 1] = f;
                if e[m - 1].abs() <= thresh {
                    e[m - 1] = 0.0;
                }
            } else {
                let mut f = (d[m].abs() - shift) * (1f64.copysign(d[m]) + shift / d[m]);
                let mut g = e[m - 1];
                for i in (ll + 1..=m).rev() {
                    let (cos_r, sin_r, r) = lartg(f, g);
                    if i < m {
                        e[i] = r;
                    }
                    f = cos_r * d[i] + sin_r * e[i - 1];
                    e[i - 1] = cos_r * e[i - 1] - sin_r * d[i];
                    g = sin_r * d[i - 1];
                    d[i - 1] *= cos_r;
                    let (cos_l, sin_l, r) = lartg(f, g);
                    d[i] = r;
                    f = cos_l * e[i - 1] + sin_l * d[i - 1];
                    d[i - 1] = cos_l * d[i - 1] - sin_l * e[i - 1];
                    if i > ll + 1 {
                        g = sin_l * e[i - 2];
                        e[i - 2] *= cos_l;
                    }
                    rotate(v, n, i - 1, cos_l, -sin_l);
                    rotate(u, n, i - 1, cos_r, -sin_r);
                }
                e[ll] = f;
                if e[ll].abs() <= thresh {
                    e[ll] = 0.0;
                }
            }
        }
    }
    for (i, value) in d.iter_mut().enumerate() {
        if *value < 0.0 {
            *value = -*value;
            for x in &mut v[n * i..n * (i + 1)] {
                *x = -*x;
            }
        }
    }
    // Sort nonincreasing (selection sort, one swap per value).
    for i in 0..n {
        let mut top = i;
        for j in i + 1..n {
            if d[j] > d[top] {
                top = j;
            }
        }
        if top != i {
            d.swap(i, top);
            swap_columns(u, n, i, top);
            swap_columns(v, n, i, top);
        }
    }
    true
}

fn swap_columns(m: &mut [f64], n: usize, i: usize, j: usize) {
    let (low, high) = (i.min(j), i.max(j));
    let (head, tail) = m.split_at_mut(n * high);
    head[n * low..n * (low + 1)].swap_with_slice(&mut tail[..n]);
}

/// `[x_j, x_{j+1}] ← [c·x_j + s·x_{j+1}, c·x_{j+1} − s·x_j]` on the
/// columns `j`, `j + 1` of the `n × n` column-major `m` (LAPACK `dlasr`'s
/// and `drot`'s convention).
#[expect(
    clippy::many_single_char_names,
    clippy::float_cmp,
    reason = "LAPACK's names; skipping the exact identity is exact"
)]
fn rotate(m: &mut [f64], n: usize, j: usize, c: f64, s: f64) {
    if c == 1.0 && s == 0.0 {
        return;
    }
    let (x, y) = m[n * j..n * (j + 2)].split_at_mut(n);
    for (a, b) in x.iter_mut().zip(y.iter_mut()) {
        let (p, q) = (*a, *b);
        *a = c * p + s * q;
        *b = c * q - s * p;
    }
}

/// The plane rotation `(c, s, r)` with `[c s; −s c]·[f; g] = [r; 0]`, `r`
/// of `f`'s sign (LAPACK 3.10 `dlartg`), safe from overflow.
fn lartg(f: f64, g: f64) -> (f64, f64, f64) {
    const SAFE_MIN: f64 = f64::MIN_POSITIVE;
    if g == 0.0 {
        return (1.0, 0.0, f);
    }
    if f == 0.0 {
        return (0.0, 1f64.copysign(g), g.abs());
    }
    let root_min = SAFE_MIN.sqrt();
    let root_max = (1.0 / SAFE_MIN / 2.0).sqrt();
    let (f1, g1) = (f.abs(), g.abs());
    if f1 > root_min && f1 < root_max && g1 > root_min && g1 < root_max {
        let d = (f * f + g * g).sqrt();
        let r = d.copysign(f);
        (f1 / d, g / r, r)
    } else {
        let scale = f1.max(g1).clamp(SAFE_MIN, 1.0 / SAFE_MIN);
        let (fs, gs) = (f / scale, g / scale);
        let d = (fs * fs + gs * gs).sqrt();
        let r = d.copysign(f);
        (fs.abs() / d, gs / r, r * scale)
    }
}

/// The singular values `(min, max)` of `[f g; 0 h]` (LAPACK `dlas2`).
#[expect(clippy::many_single_char_names, reason = "LAPACK dlas2's names")]
fn las2(f: f64, g: f64, h: f64) -> (f64, f64) {
    let (fa, ga, ha) = (f.abs(), g.abs(), h.abs());
    let (low, high) = (fa.min(ha), fa.max(ha));
    if low == 0.0 {
        let max = if high == 0.0 {
            ga
        } else {
            let (a, b) = (high.max(ga), high.min(ga));
            a * (1.0 + (b / a) * (b / a)).sqrt()
        };
        (0.0, max)
    } else if ga < high {
        let a_s = 1.0 + low / high;
        let a_t = (high - low) / high;
        let a_u = (ga / high) * (ga / high);
        let c = 2.0 / ((a_s * a_s + a_u).sqrt() + (a_t * a_t + a_u).sqrt());
        (low * c, high / c)
    } else {
        let a_u = high / ga;
        if a_u == 0.0 {
            ((low * high) / ga, ga)
        } else {
            let a_s = 1.0 + low / high;
            let a_t = (high - low) / high;
            let c = 1.0
                / ((1.0 + (a_s * a_u) * (a_s * a_u)).sqrt()
                    + (1.0 + (a_t * a_u) * (a_t * a_u)).sqrt());
            let min = (low * c) * a_u;
            (min + min, ga / (c + c))
        }
    }
}

/// The SVD of `[f g; 0 h]` (LAPACK `dlasv2`): `(s_min, s_max, sin_r,
/// cos_r, sin_l, cos_l)` with
/// `[cos_l sin_l; −sin_l cos_l]·[f g; 0 h]·[cos_r −sin_r; sin_r cos_r]
/// = diag(s_max, s_min)`.
#[expect(
    clippy::many_single_char_names,
    reason = "LAPACK dlasv2's names, for comparison with it"
)]
fn lasv2(f: f64, g: f64, h: f64) -> (f64, f64, f64, f64, f64, f64) {
    let eps = f64::EPSILON / 2.0;
    let (mut ft, mut fa, mut ht, mut ha) = (f, f.abs(), h, h.abs());
    // Which entry is largest: 1 f, 2 g, 3 h.
    let mut pmax = 1;
    let swap = ha > fa;
    if swap {
        pmax = 3;
        std::mem::swap(&mut ft, &mut ht);
        std::mem::swap(&mut fa, &mut ha);
    }
    let (gt, ga) = (g, g.abs());
    let (mut clt, mut crt, mut slt, mut srt);
    let (mut s_min, mut s_max);
    if ga == 0.0 {
        s_min = ha;
        s_max = fa;
        clt = 1.0;
        crt = 1.0;
        slt = 0.0;
        srt = 0.0;
    } else {
        let mut ga_small = true;
        s_min = 0.0;
        s_max = 0.0;
        clt = 0.0;
        crt = 0.0;
        slt = 0.0;
        srt = 0.0;
        if ga > fa {
            pmax = 2;
            if fa / ga < eps {
                ga_small = false;
                s_max = ga;
                s_min = if ha > 1.0 {
                    fa / (ga / ha)
                } else {
                    (fa / ga) * ha
                };
                clt = 1.0;
                slt = ht / gt;
                srt = 1.0;
                crt = ft / gt;
            }
        }
        if ga_small {
            let dd = fa - ha;
            // LAPACK's exact test: `ha` too small to change `fa`.
            #[expect(clippy::float_cmp, reason = "LAPACK dlasv2's exact test")]
            let mut l = if dd == fa { 1.0 } else { dd / fa };
            let m = gt / ft;
            let mut t = 2.0 - l;
            let mm = m * m;
            let tt = t * t;
            let s = (tt + mm).sqrt();
            let r = if l == 0.0 {
                m.abs()
            } else {
                (l * l + mm).sqrt()
            };
            let a = 0.5 * (s + r);
            s_min = ha / a;
            s_max = fa * a;
            if mm == 0.0 {
                t = if l == 0.0 {
                    2f64.copysign(ft) * 1f64.copysign(gt)
                } else {
                    gt / dd.copysign(ft) + m / t
                };
            } else {
                t = (m / (s + t) + m / (r + l)) * (1.0 + a);
            }
            l = (t * t + 4.0).sqrt();
            crt = 2.0 / l;
            srt = t / l;
            clt = (crt + srt * m) / a;
            slt = (ht / ft) * srt / a;
        }
    }
    let (cos_l, sin_l, cos_r, sin_r) = if swap {
        (srt, crt, slt, clt)
    } else {
        (clt, slt, crt, srt)
    };
    let sign = |x: f64| 1f64.copysign(x);
    let t_sign = match pmax {
        1 => sign(cos_r) * sign(cos_l) * sign(f),
        2 => sign(sin_r) * sign(cos_l) * sign(g),
        _ => sign(sin_r) * sign(sin_l) * sign(h),
    };
    s_max = s_max.copysign(t_sign);
    s_min = s_min.copysign(t_sign * sign(f) * sign(h));
    (s_min, s_max, sin_r, cos_r, sin_l, cos_l)
}
