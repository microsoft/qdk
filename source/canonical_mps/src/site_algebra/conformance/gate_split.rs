// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The cases of [`SiteAlgebra::gate_split`]: each splits seeded pairs at
//! the settings its rule needs and compares the outcome with `θ`, its
//! singular values, and the fractions of their squares kept and dropped.

use super::formulas::{
    columns_isometry_error, distance, dropped_fraction_of, frobenius, gated_pair,
    relative_distance, retained_fraction_of, rows_isometry_error, singular_values,
};
use super::inputs::{
    CAPS, CUTOFFS, LARGE_SHAPE, Pair, SHAPES, derived, identity_gate, two_site_gate,
};
use super::{TOLERANCE, read, upload};
use crate::dense_linalg::Matrix;
use crate::site_algebra::{CentreSide, GateSplitCall, SiteAlgebra};
use num_complex::Complex64;
use std::fmt;

/// The share of the squared singular values a split may drop beyond its
/// cutoff, as [`GateSplitCall::cutoff`] allows: values of rounding's size,
/// which a backend's SVD may give where `θ`'s are zero, up to 1e-10 of the
/// norm. A split at a cutoff of zero that drops a value of 1e-8 of the
/// norm, a share of 1e-16, fails.
const ROUNDING_SHARE: f64 = 1e-20;

/// Without truncation (a cutoff of zero, no cap), the product of the new sites
/// is `θ` divided by its norm, on either side, on pairs up to
/// [`LARGE_SHAPE`]'s. Catches a gate read transposed or by columns, a site read
/// in another layout, and values not absorbed. For instance, a gate read
/// transposed fails it with: `the pair [1, 1, 1] of seed 300, no cap, cutoff 0,
/// centre Left: the product of the new sites is 1.1e0 from θ/‖θ‖`.
pub(super) fn untruncated_split_gives_back_theta<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    for scene in graded_scenes(seed)?.into_iter().chain([large_scene(seed)?]) {
        for centre in [CentreSide::Left, CentreSide::Right] {
            let setting = Setting::new(usize::MAX, 0.0, centre);
            let split = scene.split(algebra, setting)?;
            let error = relative_distance(split.product().elements(), scene.theta.elements());
            if error > TOLERANCE {
                return Err(format!(
                    "{scene}, {setting}: the product of the new sites is {error:.1e} from θ/‖θ‖"
                ));
            }
        }
    }
    Ok(())
}

/// Truncated, the product of the new sites is `θ`'s SVD truncated to its `k`
/// largest values, divided by their norm. With `W` the share of the squares of
/// those values, no matrix of rank `k` is nearer `θ/‖θ‖` than `1 − W` in the
/// squared Frobenius norm (Eckart–Young), and only that truncation is that near
/// where the `k`-th value exceeds the next, as the graded pairs' do; so the
/// case checks that `√W` times the product is that near, and that the product
/// has norm 1. Catches the smallest values kept, or other values than the
/// largest, and values not normalised. For instance, a split that keeps the
/// smallest values fails it with: `the pair [1, 1, 1] of seed 300, cap 1,
/// cutoff 0, centre Left: bond 1: the product is 1.601e0 (squared) from θ/‖θ‖,
/// where θ's truncated SVD is 3.444e-2`.
pub(super) fn truncated_split_keeps_the_largest_values<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    let mut scenes = graded_scenes(seed)?;
    scenes.push(Scene::with_values(
        &[0.8, 0.5, 0.3, 0.1],
        2,
        2,
        derived(seed, 400),
    )?);
    let runs = scenes
        .iter()
        .flat_map(|scene| Setting::all().map(move |s| (scene, s)));
    for (scene, setting) in runs {
        let split = scene.split(algebra, setting)?;
        let norm = frobenius(split.product().elements());
        if distance(norm, 1.0) > TOLERANCE {
            return Err(format!(
                "{scene}, {setting}: the product of the new sites has norm {norm}, not 1"
            ));
        }
        let dropped = dropped_fraction_of(&scene.values, split.bond);
        let residual = scene.residual(&split);
        if distance(residual, dropped) > TOLERANCE {
            return Err(format!(
                "{scene}, {setting}: bond {}: the product is {residual:.3e} (squared) from \
                 θ/‖θ‖, where θ's truncated SVD is {dropped:.3e}",
                split.bond
            ));
        }
    }
    Ok(())
}

/// The bond lies between 1 and the smallest of the cap and the two sides of
/// `θ`, `2·left_extent` and `2·right_extent`, at every setting. Catches a cap
/// ignored, a bond of zero, and a bond that counts the pair's bond instead of
/// `θ`'s sides. For instance, a cap ignored fails it with: `the pair [1, 1, 1]
/// of seed 300, cap 1, cutoff 0, centre Left: bond 2 is not in 1..=1`.
pub(super) fn bond_lies_within_the_cap_and_theta_sides<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    for scene in graded_scenes(seed)? {
        for setting in Setting::all() {
            // `split` checks the bond's bounds before it reads.
            scene.split(algebra, setting)?;
        }
    }
    Ok(())
}

/// The site away from the centre is an isometry at every setting: the left
/// site's columns are orthonormal when the centre is on the right, and the
/// right site's rows when it is on the left. Catches the values absorbed on the
/// wrong side, and factors that are not orthonormal, such as a correct product
/// whose bond carries a scaling and its inverse. For instance, the values
/// absorbed on the wrong side fails it with: `the pair [1, 1, 1] of seed 300,
/// cap 2, cutoff 0, centre Left: the right site is 9.7e-1 from an isometry`.
pub(super) fn site_away_from_the_centre_is_an_isometry<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    for scene in graded_scenes(seed)? {
        for setting in Setting::all() {
            let split = scene.split(algebra, setting)?;
            let (error, side) = match setting.centre {
                CentreSide::Right => (columns_isometry_error(&split.left), "left"),
                CentreSide::Left => (rows_isometry_error(&split.right), "right"),
            };
            if error > TOLERANCE {
                return Err(format!(
                    "{scene}, {setting}: the {side} site is {error:.1e} from an isometry"
                ));
            }
        }
    }
    Ok(())
}

/// The squares of the dropped values sum to at most the cutoff's share of all
/// of them, unless the cap cut the bond, at every setting; and at a cutoff of
/// zero, a value of 1e-8 of the norm, whose square is far above rounding's
/// share ([`ROUNDING_SHARE`]), is kept. Catches a cutoff applied to the values
/// rather than their squares, a cutoff taken as larger than it is, and a cutoff
/// of zero that drops values above rounding's size. For instance, a split that
/// drops up to twice its cutoff's share fails it with: `the pair [4, 3, 2] of
/// seed 320, cap 3, cutoff 0.05, centre Left: bond 2 drops 7.466e-2 of the
/// squares`.
pub(super) fn drops_at_most_the_cutoffs_share<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    let small = 1e-8_f64;
    let mut scenes = graded_scenes(seed)?;
    scenes.push(Scene::with_values(
        &[(1.0 - small * small).sqrt(), small],
        1,
        1,
        derived(seed, 410),
    )?);
    for scene in &scenes {
        for setting in Setting::all() {
            let split = scene.split(algebra, setting)?;
            if split.bond == setting.max_extent {
                continue;
            }
            let dropped = dropped_fraction_of(&scene.values, split.bond);
            if dropped > allowance(setting.cutoff) {
                return Err(format!(
                    "{scene}, {setting}: bond {} drops {dropped:.3e} of the squares",
                    split.bond
                ));
            }
        }
    }
    Ok(())
}

/// The retained fraction is the share of the squared values kept, at every
/// setting. Catches a fraction of the values rather than of their squares, and
/// a fraction of all the values rather than of those kept. For instance, a
/// split that reports the square root of its retained fraction fails it with:
/// `the pair [1, 1, 1] of seed 300, cap 1, cutoff 0, centre Left: bond 1
/// retains 0.9826278225417038, not 0.9655574376330502`.
pub(super) fn retains_the_fraction_of_its_kept_squares<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    let mut scenes = graded_scenes(seed)?;
    scenes.push(Scene::with_values(
        &[0.8, 0.5, 0.3, 0.1],
        2,
        2,
        derived(seed, 420),
    )?);
    for scene in &scenes {
        for setting in Setting::all() {
            let split = scene.split(algebra, setting)?;
            let expected = retained_fraction_of(&scene.values, split.bond);
            let fraction = split.retained_fraction;
            if distance(fraction, expected) > TOLERANCE {
                return Err(format!(
                    "{scene}, {setting}: bond {} retains {fraction}, not {expected}",
                    split.bond
                ));
            }
        }
    }
    Ok(())
}

/// At a cutoff of zero and no cap, a split that drops no value `θ` has (only
/// values of rounding's size, where `θ`'s are zero) reports a retained fraction
/// of exactly 1, so that the engine's retained weight stays exactly 1 while
/// nothing is truncated. On graded pairs, and on pairs of rank 1, 3 and 12
/// whose `θ`, under the identity gate, has 4, 16 or 32 rows. Catches a fraction
/// computed as one minus a rounded sum. For instance, a fraction one rounding
/// below 1 fails it with: `the pair [1, 1, 1] of seed 300, no cap, cutoff 0,
/// centre Left: bond 2 of 2 values retains 2.220446049250313e-16 short of 1`.
#[expect(clippy::float_cmp, reason = "the contract promises exactly 1")]
pub(super) fn dropping_nothing_retains_exactly_one<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    let mut scenes = graded_scenes(seed)?;
    for (index, shape) in [(2, 1, 2), (2, 3, 2), (8, 3, 16), (16, 12, 8)]
        .into_iter()
        .enumerate()
    {
        let pair_seed = derived(seed, 430 + 10 * index as u64);
        scenes.push(Scene::new(Pair::graded(shape, pair_seed), identity_gate())?);
    }
    for scene in &scenes {
        for centre in [CentreSide::Left, CentreSide::Right] {
            let setting = Setting::new(usize::MAX, 0.0, centre);
            let split = scene.split(algebra, setting)?;
            if split.bond < scene.values.len() {
                // A value θ has was dropped: the cutoff's case judges it.
                continue;
            }
            if split.retained_fraction != 1.0 {
                return Err(format!(
                    "{scene}, {setting}: bond {} of {} values retains {:e} short of 1",
                    split.bond,
                    scene.values.len(),
                    1.0 - split.retained_fraction
                ));
            }
        }
    }
    Ok(())
}

/// Where a tail's squares are exactly the cutoff's share, the split may keep or
/// drop it, and either way its bond and its retained fraction agree: the values
/// `(1/√2, 1/√2)` at a cutoff of 0.5, and the values whose squares are `(0.5,
/// 0.25, 0.25)` at a cutoff of 0.25, capped or not. The product is checked as
/// near `θ` as its bond allows, but not against one SVD, which a tie does not
/// single out. Catches a backend that takes the tie one way and reports the
/// other's retained fraction. For instance, a split that drops a tied tail and
/// reports the fraction of keeping it fails it with: `the pair [1, 2, 1] of
/// seed 450, cap 2, cutoff 0.5, centre Left: bond 1 retains 1, not
/// 0.5000000000000002`.
pub(super) fn at_a_tie_retains_the_fraction_of_its_bond<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    let half = 0.5_f64.sqrt();
    let scenes = [
        (
            Scene::with_values(&[half, half], 1, 1, derived(seed, 450))?,
            0.5,
        ),
        (
            Scene::with_values(&[half, 0.5, 0.5], 2, 2, derived(seed, 460))?,
            0.25,
        ),
    ];
    for (scene, cutoff) in &scenes {
        for max_extent in [2, usize::MAX] {
            for centre in [CentreSide::Left, CentreSide::Right] {
                let setting = Setting::new(max_extent, *cutoff, centre);
                let split = scene.split(algebra, setting)?;
                let k = split.bond;
                let dropped = dropped_fraction_of(&scene.values, k);
                if k < max_extent && dropped > allowance(*cutoff) {
                    return Err(format!("{scene}, {setting}: bond {k} drops {dropped:.3e}"));
                }
                let expected = retained_fraction_of(&scene.values, k);
                if distance(split.retained_fraction, expected) > TOLERANCE {
                    return Err(format!(
                        "{scene}, {setting}: bond {k} retains {}, not {expected}",
                        split.retained_fraction
                    ));
                }
                let residual = scene.residual(&split);
                if distance(residual, dropped) > TOLERANCE {
                    return Err(format!(
                        "{scene}, {setting}: bond {k}: the product is {residual:.3e} (squared) \
                         from θ/‖θ‖, not {dropped:.3e}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The split leaves its input tensors unchanged, capped or not, on either side,
/// since the engine may still hold them (the sites of a fork's source). Catches
/// a split computed in the inputs' storage. For instance, a split that zeroes
/// its left input fails it with: `the pair [1, 1, 1] of seed 300, cap 2, cutoff
/// 0.05, centre Left: the split changed its inputs`.
pub(super) fn leaves_its_inputs_unchanged<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    for scene in graded_scenes(seed)? {
        for (max_extent, centre) in [(2, CentreSide::Left), (usize::MAX, CentreSide::Right)] {
            let setting = Setting::new(max_extent, CUTOFFS[1], centre);
            let split = scene.split(algebra, setting)?;
            if split.inputs != [scene.pair.left.clone(), scene.pair.right.clone()] {
                return Err(format!("{scene}, {setting}: the split changed its inputs"));
            }
        }
    }
    Ok(())
}

/// The largest dropped fraction a split at `cutoff` may report: the cutoff's,
/// with [`TOLERANCE`] of it for a tail at the cutoff, and values of
/// rounding's size ([`ROUNDING_SHARE`]).
fn allowance(cutoff: f64) -> f64 {
    cutoff * (1.0 + TOLERANCE) + ROUNDING_SHARE
}

/// A pair, a gate, `θ/‖θ‖`, and `θ`'s singular values: what a split's
/// outcome is compared with.
struct Scene {
    pair: Pair,
    gate: [Complex64; 16],
    /// `θ` divided by its Frobenius norm.
    theta: Matrix,
    /// `θ`'s singular values ([`singular_values`]).
    values: Vec<f64>,
}

impl Scene {
    fn new(pair: Pair, gate: [Complex64; 16]) -> Result<Self, String> {
        let theta = gated_pair(&pair, &gate);
        let values = singular_values(&theta)?;
        let norm = frobenius(theta.elements());
        let elements = theta.elements().iter().map(|x| x / norm).collect();
        let theta = Matrix::new(theta.rows(), theta.cols(), elements);
        Ok(Self {
            pair,
            gate,
            theta,
            values,
        })
    }

    /// A pair of [`Pair::with_values`] under the identity gate.
    fn with_values(values: &[f64], l: usize, r: usize, seed: u64) -> Result<Self, String> {
        Self::new(Pair::with_values(values, l, r, seed), identity_gate())
    }

    /// Splits the pair at `setting` and reads the new sites back, after
    /// checking that the bond lies within its bounds, so that no read asks
    /// for more than a correct tensor of that bond holds, and that the
    /// retained fraction lies in `(0, 1]`, which a NaN does not; then
    /// reads the input tensors back.
    fn split<A: SiteAlgebra>(&self, algebra: &A, setting: Setting) -> Result<Split, String> {
        let Pair { l, m, r, .. } = self.pair;
        let failed = |message: String| format!("{self}, {setting}: {message}");
        let left = upload(algebra, &self.pair.left).map_err(failed)?;
        let right = upload(algebra, &self.pair.right).map_err(failed)?;
        let call = GateSplitCall {
            left: &left,
            right: &right,
            left_extent: l,
            bond: m,
            right_extent: r,
            gate: self.gate,
            max_extent: setting.max_extent,
            cutoff: setting.cutoff,
            centre: setting.centre,
        };
        let outcome = algebra
            .gate_split(&call)
            .map_err(|error| failed(format!("the split failed: {error}")))?;
        let k = outcome.bond;
        let bound = setting.max_extent.min(2 * l).min(2 * r);
        if !(1..=bound).contains(&k) {
            return Err(failed(format!("bond {k} is not in 1..={bound}")));
        }
        let fraction = outcome.retained_fraction;
        if !(fraction > 0.0 && fraction <= 1.0) {
            return Err(failed(format!(
                "the retained fraction {fraction} is not in (0, 1]"
            )));
        }
        let new_left = read(algebra, &outcome.left, 2 * l * k).map_err(failed)?;
        let new_right = read(algebra, &outcome.right, k * 2 * r).map_err(failed)?;
        let inputs = [
            read(algebra, &left, 2 * l * m).map_err(failed)?,
            read(algebra, &right, m * 2 * r).map_err(failed)?,
        ];
        Ok(Split {
            left: Matrix::new(2 * l, k, new_left),
            right: Matrix::new(k, 2 * r, new_right),
            bond: k,
            retained_fraction: outcome.retained_fraction,
            inputs,
        })
    }

    /// `‖θ/‖θ‖ − √W·L·R‖²`, with `L·R` the split's product and `W` the
    /// share of the squares of `θ`'s `k` largest values: by Eckart–Young,
    /// the share of the dropped squares when `L·R` is `θ`'s SVD truncated
    /// to those values, and more for any other product of bond `k`.
    fn residual(&self, split: &Split) -> f64 {
        let scale = retained_fraction_of(&self.values, split.bond).sqrt();
        (self.theta.elements().iter())
            .zip(split.product().elements())
            .map(|(t, x)| (t - scale * x).norm_sqr())
            .sum()
    }
}

impl fmt::Display for Scene {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.pair)
    }
}

/// The graded pairs of every shape of [`SHAPES`], each under its own
/// random gate, drawn from the kit's `seed`.
fn graded_scenes(seed: u64) -> Result<Vec<Scene>, String> {
    (SHAPES.iter().zip(0_u64..))
        .map(|(&shape, index)| {
            let pair_seed = derived(seed, 300 + 10 * index);
            Scene::new(
                Pair::graded(shape, pair_seed),
                two_site_gate(pair_seed.wrapping_add(5)),
            )
        })
        .collect()
}

/// The graded pair of [`LARGE_SHAPE`], under a random gate, drawn from the
/// kit's `seed`.
fn large_scene(seed: u64) -> Result<Scene, String> {
    let pair_seed = derived(seed, 390);
    Scene::new(
        Pair::graded(LARGE_SHAPE, pair_seed),
        two_site_gate(pair_seed.wrapping_add(5)),
    )
}

/// A split's outcome, read back.
struct Split {
    /// The new left site, as the `(2·l) × bond` matrix.
    left: Matrix,
    /// The new right site, as the `bond × (2·r)` matrix.
    right: Matrix,
    bond: usize,
    retained_fraction: f64,
    /// The input tensors, read back after the split.
    inputs: [Vec<Complex64>; 2],
}

impl Split {
    fn product(&self) -> Matrix {
        Matrix::product(self.left.view(), self.right.view())
    }
}

/// A split's cap, cutoff and centre.
#[derive(Clone, Copy)]
struct Setting {
    max_extent: usize,
    cutoff: f64,
    centre: CentreSide,
}

impl Setting {
    fn new(max_extent: usize, cutoff: f64, centre: CentreSide) -> Self {
        Self {
            max_extent,
            cutoff,
            centre,
        }
    }

    /// Every cap of [`CAPS`] with every cutoff of [`CUTOFFS`], on either
    /// side.
    fn all() -> impl Iterator<Item = Self> {
        CAPS.into_iter().flat_map(|max_extent| {
            CUTOFFS.into_iter().flat_map(move |cutoff| {
                [CentreSide::Left, CentreSide::Right]
                    .into_iter()
                    .map(move |centre| Self::new(max_extent, cutoff, centre))
            })
        })
    }
}

impl fmt::Display for Setting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.max_extent == usize::MAX {
            write!(f, "no cap")?;
        } else {
            write!(f, "cap {}", self.max_extent)?;
        }
        write!(f, ", cutoff {}, centre {:?}", self.cutoff, self.centre)
    }
}
