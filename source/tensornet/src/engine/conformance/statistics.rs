// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kit's tests of an engine's samples, each with the probability that a
//! correct engine fails it, which the kit keeps below 1e-8 in all.
//!
//! **Frequencies.** Each joint outcome of a call's sites, of probability
//! `p` in the reference, is drawn in a fraction `f` of `N` independent
//! shots. Chernoff's bound for a binomial count gives, on each side of `p`,
//! `P(N·KL(f‖p) ≥ t) ≤ e^(−t)`, where `KL(f‖p) = f·ln(f/p) + (1−f)·ln((1−f)
//! / (1−p))` (and `0·ln 0 = 0`). The kit fails an outcome when
//! `N·KL(f‖p) > ln(2/δ)`, with `δ` = [`OUTCOME_FALSE_ALARM`] = 1e-11, so a
//! correct engine fails it with probability at most 1e-11. The bound
//! scales with `p`: an outcome of probability zero fails if it is drawn
//! once, and a rare one fails if it is never drawn, where an absolute
//! tolerance would pass both. Its power, at the kit's [`POWER_SHOTS`]
//! shots: an outcome of probability 0.49 drawn as 0.51 (a bias of 0.02)
//! has `N·KL` ≈ 120, beyond `ln(2/δ)` ≈ 26.0, so it passes only when its
//! frequency falls 8.3 standard deviations short of 0.51 (probability
//! below 1e-16); an outcome of probability 0.01 never drawn has
//! `N·KL = −N·ln(0.99)` ≈ 1508.
//!
//! **Independence.** For shots `L` apart (every lag `L` from 1 to `N − 1`,
//! so every pair of shots), and for every pair of positions `t`, `t′` of
//! the sites drawn, with `xₛₜ` the value of position `t` in shot `s` (1 for
//! One) and `pₜ` its probability of One, the kit sums
//! `S = Σₛ (xₛₜ − pₜ)(xₛ₊ₗ,ₜ′ − pₜ′)` over the `n = N − L` pairs. When the
//! shots are independent, each term has mean zero given the shots before
//! `s + L`, so the terms, taken in the order of `s + L`, are the increments
//! of a martingale, each at most `c = max(pₜ, 1−pₜ)·max(pₜ′, 1−pₜ′)` in
//! size, and Azuma's inequality gives `P(|S| ≥ λ) ≤ 2·exp(−λ²/(2nc²))`.
//! The kit fails a sum beyond `λ = c·√(2n·ln(2T/δ))`, with `T` the number
//! of sums and `δ` = [`INDEPENDENCE_FALSE_ALARM`] = 4e-9 shared among them,
//! so a correct engine fails one call with probability at most 4e-9. At
//! 4000 shots of three sites of probability one half (`T` = 35 991), `λ` is
//! about 123 at lag 1: shots drawn twice in a row give `S` ≈ 500, as do the
//! second half of the shots copying the first (at lag 2000, `λ` ≈ 87), and
//! shots whose odd ones take `1 − u` for the uniform `u` of the shot before
//! (`S` ≈ −500); one shot in five copying the one before gives `S` ≈ 200
//! at lag 1, which a random draw of that defect falls below 123 with
//! probability under 1e-5.
//!
//! **Repeats.** The sums above cover the distances within one call of a
//! few thousand shots; a sampler that restarts its stream with the same
//! seed every few thousand shots or more (a chunked or threaded sampler)
//! repeats its shots at a distance beyond them. So the kit also fails
//! [`POWER_SHOTS`] shots in which [`REPEAT_WINDOW`] consecutive shots
//! repeat, shot for shot, those from an earlier shot. When the shots are
//! independent, for two starts `a < b`, each shot `b + i` of the later
//! window, drawn after shot `a + i`, equals it with probability at most
//! `pₘₐₓ`, the largest probability of a joint outcome, so the windows are
//! equal with probability at most `pₘₐₓ^W`. Over the `M(M − 1)/2` pairs of
//! the `M = N − W + 1` windows, a correct engine fails with probability at
//! most `M(M − 1)/2 · pₘₐₓ^W`: about 2.5e-30 for the kit's call (`pₘₐₓ` =
//! 0.49, `N` = 150 000, `W` = 128). The check catches a restart every `C`
//! shots for every `C` up to `N − W`, at a cost of `O(N log N)`: it hashes
//! every window, rolling the hash from one window to the next, keeps each
//! hash's first window in an ordered map, and compares the shots only
//! where two hashes agree, so a collision of hashes can hide a repeat but
//! never fail a correct engine. It reads that one call only, so it misses
//! a restart after more than `N − W` shots and a repeat of fewer than `W`
//! shots.

/// The probability that a correct engine fails the check of one outcome's
/// frequency.
pub(super) const OUTCOME_FALSE_ALARM: f64 = 1e-11;

/// The probability that a correct engine fails the independence check of
/// one call's shots, all lags and positions together.
pub(super) const INDEPENDENCE_FALSE_ALARM: f64 = 4e-9;

/// How many shots the call that measures the kit's power draws.
pub(super) const POWER_SHOTS: usize = 150_000;

/// How many consecutive shots [`unrepeated`] compares.
pub(super) const REPEAT_WINDOW: usize = 128;

/// The base of [`unrepeated`]'s polynomial hash of a window: odd, so that
/// its powers modulo 2⁶⁴ are never zero.
const HASH_BASE: u64 = 0x0000_0100_0000_01B3;

/// Whether `count` draws of an outcome of probability `probability`, in
/// `shots` shots, are within the Chernoff bound: false for a NaN.
pub(crate) fn frequency_fits(count: usize, shots: usize, probability: f64) -> bool {
    let p = probability.clamp(0.0, 1.0);
    #[expect(clippy::cast_precision_loss, reason = "shot counts are small")]
    let (f, n) = (count as f64 / shots as f64, shots as f64);
    let term = |a: f64, b: f64| if a <= 0.0 { 0.0 } else { a * (a / b).ln() };
    let divergence = term(f, p) + term(1.0 - f, 1.0 - p);
    n * divergence <= (2.0 / OUTCOME_FALSE_ALARM).ln()
}

/// Checks that the shots of `values`, `width` values each, are independent,
/// position `t` of each being One with probability `marginals[t]`: `Err`
/// names the first sum beyond its bound.
pub(crate) fn independent(values: &[bool], width: usize, marginals: &[f64]) -> Result<(), String> {
    let shots = values.len() / width.max(1);
    if shots < 2 {
        return Ok(());
    }
    let words = shots.div_ceil(64);
    let mut bits = vec![vec![0u64; words]; width];
    let mut ones = vec![vec![0usize; shots + 1]; width];
    for (s, shot) in values.chunks_exact(width).enumerate() {
        for (t, &one) in shot.iter().enumerate() {
            if one {
                bits[t][s / 64] |= 1 << (s % 64);
            }
            ones[t][s + 1] = ones[t][s] + usize::from(one);
        }
    }
    let tests = (shots - 1) * width * width;
    #[expect(clippy::cast_precision_loss, reason = "test counts are small")]
    let log_term = (2.0 * tests as f64 / INDEPENDENCE_FALSE_ALARM).ln();
    for lag in 1..shots {
        let n = shots - lag;
        for t in 0..width {
            for u in 0..width {
                let (p, q) = (marginals[t].clamp(0.0, 1.0), marginals[u].clamp(0.0, 1.0));
                let both = and_count(&bits[t], &bits[u], lag, n);
                let (sx, sy) = (ones[t][n], ones[u][shots] - ones[u][lag]);
                #[expect(clippy::cast_precision_loss, reason = "shot counts are small")]
                let (both, sx, sy, n) = (both as f64, sx as f64, sy as f64, n as f64);
                let sum = both - q * sx - p * sy + n * p * q;
                let size = p.max(1.0 - p) * q.max(1.0 - q);
                let bound = size * (2.0 * n * log_term).sqrt();
                if sum.abs() > bound {
                    return Err(format!(
                        "for shots {lag} apart, Σ (x − p)(y − q) over position {t} of the \
                         first and position {u} of the second is {sum:.1}, beyond {bound:.1}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Checks that no [`REPEAT_WINDOW`] consecutive shots of `values`, `width`
/// values each, repeat those from an earlier shot: `Err` names the first
/// repeat found.
pub(super) fn unrepeated(values: &[bool], width: usize) -> Result<(), String> {
    let width = width.max(1);
    let shots = values.len() / width;
    if shots < REPEAT_WINDOW {
        return Ok(());
    }
    // Every code is above zero, so that a shot of Zeros moves the hash.
    let codes: Vec<u64> = values
        .chunks_exact(width)
        .map(|shot| {
            shot.iter().fold(1u64, |code, &one| {
                code.wrapping_mul(3).wrapping_add(u64::from(one))
            })
        })
        .collect();
    let lead = HASH_BASE.wrapping_pow(u32::try_from(REPEAT_WINDOW - 1).unwrap_or(u32::MAX));
    let window = |s: usize| &values[s * width..(s + REPEAT_WINDOW) * width];
    let mut hash = codes[..REPEAT_WINDOW].iter().fold(0u64, |hash, &code| {
        hash.wrapping_mul(HASH_BASE).wrapping_add(code)
    });
    let mut first = std::collections::BTreeMap::new();
    for start in 0..=shots - REPEAT_WINDOW {
        if start > 0 {
            hash = hash
                .wrapping_sub(codes[start - 1].wrapping_mul(lead))
                .wrapping_mul(HASH_BASE)
                .wrapping_add(codes[start + REPEAT_WINDOW - 1]);
        }
        let earlier = *first.entry(hash).or_insert(start);
        if earlier != start && window(earlier) == window(start) {
            return Err(format!(
                "the {REPEAT_WINDOW} shots from shot {start} repeat those from shot {earlier}"
            ));
        }
    }
    Ok(())
}

/// `Σ_{s<n} a[s]·b[s + lag]` of two bit sets.
fn and_count(a: &[u64], b: &[u64], lag: usize, n: usize) -> usize {
    let (skip, shift) = (lag / 64, lag % 64);
    let word = |j: usize| b.get(j).copied().unwrap_or(0);
    (0..n.div_ceil(64))
        .map(|w| {
            let high = if shift == 0 {
                0
            } else {
                word(w + skip + 1) << (64 - shift)
            };
            // A shot `s ≥ n` of `a` meets a bit past the last shot of `b`,
            // which is zero.
            (a[w] & (word(w + skip) >> shift | high)).count_ones() as usize
        })
        .sum()
}
