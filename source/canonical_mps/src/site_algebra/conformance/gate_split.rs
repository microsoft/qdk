// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The cases of [`SiteAlgebra::gate_split`]. Each splits seeded pairs at
//! the settings its rule needs. It compares the outcome with `θ`, its
//! singular values, and the fractions of their squares kept and dropped.

use super::TOLERANCE;
use super::formulas::{
    columns_isometry_error, distance, dropped_fraction_of, gated_pair, normalised,
    relative_distance, retained_fraction_of, rows_isometry_error, singular_values,
};
use super::inputs::{
    CAPS, CUTOFFS, LARGE_SHAPE, Pair, SHAPES, derived, identity_gate, two_site_gate,
};
use super::pair_tensors::{
    failure_if_a_new_site_reads_past_its_end, read_inputs, read_new_sites, upload_pair,
};
use crate::site_algebra::{CentreSide, GateSplitCall, GateSplitOutcome, SiteAlgebra};
use dense_linalg::{Matrix, norm};
use num_complex::Complex64;
use std::fmt;

/// The share of the squared singular values a split may drop beyond its
/// cutoff, as [`GateSplitCall::cutoff`] allows. Those are values of
/// rounding's size, up to 1e-10 of the norm. A backend's SVD may give them
/// where `θ`'s are zero. A split at a cutoff of zero that drops a value of
/// 1e-8 of the norm, a share of 1e-16, fails.
const ROUNDING_SHARE: f64 = 1e-20;

/// Without truncation (a cutoff of zero, no cap), the product of the new sites
/// is `θ` divided by its norm. That holds on either side, on pairs up to
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
/// largest values, divided by their norm. Let `W` be the share of the squares
/// of those values. No matrix of rank `k` is nearer `θ/‖θ‖` than `1 − W` in the
/// squared Frobenius norm (Eckart–Young). Where the `k`-th value exceeds the
/// next, as the graded pairs' do, only that truncation is that near. So the
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
        let product_norm = norm(split.product().elements());
        if distance(product_norm, 1.0) > TOLERANCE {
            return Err(format!(
                "{scene}, {setting}: the product of the new sites has norm {product_norm}, not 1"
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

/// At every setting, the bond lies between 1 and the smallest of the cap and
/// the two sides of `θ`, `2·left_extent` and `2·right_extent`. The case runs
/// on graded pairs, and on pairs whose bond exceeds a side of `θ`. Catches a
/// cap ignored, a bond of zero, and a bond that counts the pair's bond instead
/// of `θ`'s sides. For instance, a cap ignored fails it with:
/// `the pair [1, 1, 1] of seed 300, cap 1, cutoff 0, centre Left: bond 2 is
/// not in 1..=1`.
pub(super) fn bond_lies_within_the_cap_and_theta_sides<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    let mut scenes = graded_scenes(seed)?;
    for (index, shape) in [(1, 4, 2), (2, 5, 1)].into_iter().enumerate() {
        let pair_seed = derived(seed, 470 + 10 * index as u64);
        scenes.push(Scene::new(
            Pair::graded(shape, pair_seed),
            two_site_gate(pair_seed.wrapping_add(5)),
        )?);
    }
    for scene in &scenes {
        for setting in Setting::all() {
            // `split` checks the bond's bounds before it reads.
            scene.split(algebra, setting)?;
        }
    }
    Ok(())
}

/// At every setting, the site away from the centre is an isometry. The left
/// site's columns are orthonormal when the centre is on the right. The right
/// site's rows are when it is on the left. Catches the values absorbed on the
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

/// At every setting, unless the cap cut the bond, the squares of the dropped
/// values sum to at most the cutoff's share of all of them. At a cutoff of
/// zero, a value of 1e-8 of the norm is kept: its square is far above
/// rounding's share ([`ROUNDING_SHARE`]). Catches a cutoff applied to the
/// values rather than their squares, a cutoff taken as larger than it is, and a
/// cutoff of zero that drops values above rounding's size. For instance, a
/// split that drops up to twice its cutoff's share fails it with: `the pair [4,
/// 3, 2] of seed 320, cap 3, cutoff 0.05, centre Left: bond 2 drops 7.466e-2 of
/// the squares`.
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

/// At every setting, the retained fraction is the share of the squared values
/// kept. Catches a fraction of the values rather than of their squares, and
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
            scene.check_retained_fraction(setting, &split)?;
        }
    }
    Ok(())
}

/// At a cutoff of zero and no cap, take a split that drops no value `θ` has
/// (only values of rounding's size, where `θ`'s are zero). It reports a
/// retained fraction of exactly 1. So the engine's retained weight stays
/// exactly 1 while nothing is truncated. The case runs on graded pairs, and on
/// pairs of rank 1, 3 and 12 whose `θ`, under the identity gate, has 4, 16 or
/// 32 rows. Catches a fraction computed as one minus a rounded sum. For
/// instance, a fraction one rounding below 1 fails it with: `the pair [1, 1, 1]
/// of seed 300, no cap, cutoff 0, centre Left: bond 2 of 2 values retains
/// 2.220446049250313e-16 short of 1`.
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
/// drop it. Either way, its bond and its retained fraction agree. The case runs
/// on the values `(1/√2, 1/√2)` at a cutoff of 0.5. It also runs on the values
/// whose squares are `(0.5, 0.25, 0.25)` at a cutoff of 0.25, capped or not.
/// The product is checked as near `θ` as its bond allows. It is not checked
/// against one SVD, which a tie does not single out. Catches a backend that
/// takes the tie one way and reports the other's retained fraction. For
/// instance, a split that drops a tied tail and reports the fraction of keeping
/// it fails it with: `the pair [1, 2, 1] of seed 450, cap 2, cutoff 0.5, centre
/// Left: bond 1 retains 1, not 0.5000000000000002`.
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
        for setting in tie_settings(*cutoff) {
            let split = scene.split(algebra, setting)?;
            let k = split.bond;
            let dropped = dropped_fraction_of(&scene.values, k);
            if k < setting.max_extent && dropped > allowance(setting.cutoff) {
                return Err(format!("{scene}, {setting}: bond {k} drops {dropped:.3e}"));
            }
            scene.check_retained_fraction(setting, &split)?;
            let residual = scene.residual(&split);
            if distance(residual, dropped) > TOLERANCE {
                return Err(format!(
                    "{scene}, {setting}: bond {k}: the product is {residual:.3e} (squared) \
                     from θ/‖θ‖, not {dropped:.3e}"
                ));
            }
        }
    }
    Ok(())
}

/// The settings of a tie at `cutoff`: a cap of 2, then none, each on
/// either side.
fn tie_settings(cutoff: f64) -> impl Iterator<Item = Setting> {
    [2, usize::MAX].into_iter().flat_map(move |max_extent| {
        [CentreSide::Left, CentreSide::Right]
            .into_iter()
            .map(move |centre| Setting::new(max_extent, cutoff, centre))
    })
}

/// The split leaves its input tensors unchanged, capped or not, on either side.
/// The engine may still hold them (the sites of a fork's source). Catches
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

/// At every setting, the new sites hold exactly the elements their bond gives
/// them, `2·l·bond` and `bond·2·r`. A read of one more is refused, with an
/// error or a panic, as [`SiteAlgebra::read`] promises. Catches sites held at
/// a capacity larger than the bond kept, such as the cap or `θ`'s sides. Their
/// reads past the site answer elements that are not the site's. For
/// instance, sites held at the smaller of the cap and `θ`'s sides fail it
/// with: `the pair [1, 1, 1] of seed 300, cap 2, cutoff 0.05, centre Left:
/// bond 1: the left site answered a read of 3 elements, one past its 2`.
pub(super) fn new_sites_hold_exactly_their_elements<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    for scene in graded_scenes(seed)? {
        for setting in Setting::all() {
            let outcome = scene.outcome(algebra, setting)?.outcome;
            let new_sites = [&outcome.left, &outcome.right];
            if let Some(message) = failure_if_a_new_site_reads_past_its_end(
                algebra,
                &scene.pair,
                new_sites,
                outcome.bond,
            ) {
                return Err(format!("{scene}, {setting}: {message}"));
            }
        }
    }
    Ok(())
}

/// The largest dropped fraction a split at `cutoff` may report. It is the
/// cutoff's, with [`TOLERANCE`] of it for a tail at the cutoff, plus values of
/// rounding's size ([`ROUNDING_SHARE`]).
fn allowance(cutoff: f64) -> f64 {
    cutoff * (1.0 + TOLERANCE) + ROUNDING_SHARE
}

/// Checks that a split's retained `fraction` lies in `(0, 1]`, which a NaN
/// does not.
fn check_retained_fraction_is_a_share(fraction: f64) -> Result<(), String> {
    if fraction > 0.0 && fraction <= 1.0 {
        Ok(())
    } else {
        Err(format!("the retained fraction {fraction} is not in (0, 1]"))
    }
}

/// What a split's outcome is compared with: a pair, a gate, `θ/‖θ‖`, and
/// `θ`'s singular values.
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
        let gated = gated_pair(&pair, &gate);
        let values = singular_values(&gated)?;
        Ok(Self {
            pair,
            gate,
            theta: normalised(&gated),
            values,
        })
    }

    /// A pair of [`Pair::with_values`] under the identity gate.
    fn with_values(values: &[f64], l: usize, r: usize, seed: u64) -> Result<Self, String> {
        Self::new(Pair::with_values(values, l, r, seed), identity_gate())
    }

    /// Splits the pair at `setting` and reads the new sites back. Before
    /// reading, it checks two things:
    ///
    /// - the bond lies within its bounds, so that no read asks for more
    ///   than a correct tensor of that bond holds;
    /// - the retained fraction lies in `(0, 1]`, which a NaN does not.
    ///
    /// Then it reads the input tensors back.
    fn split<A: SiteAlgebra>(&self, algebra: &A, setting: Setting) -> Result<Split, String> {
        let failed = |message: String| format!("{self}, {setting}: {message}");
        let SplitTensors { outcome, inputs } = self.outcome(algebra, setting)?;
        let new_sites = [&outcome.left, &outcome.right];
        let [left, right] =
            read_new_sites(algebra, &self.pair, new_sites, outcome.bond).map_err(failed)?;
        let inputs = read_inputs(algebra, &self.pair, &inputs).map_err(failed)?;
        Ok(Split {
            left,
            right,
            bond: outcome.bond,
            retained_fraction: outcome.retained_fraction,
            inputs,
        })
    }

    /// Splits the pair at `setting`. Checks the bond's bounds and the
    /// retained fraction as [`Scene::split`] says. Returns the outcome, and
    /// the input tensors.
    fn outcome<A: SiteAlgebra>(
        &self,
        algebra: &A,
        setting: Setting,
    ) -> Result<SplitTensors<A>, String> {
        let Pair { l, m, r, .. } = self.pair;
        let failed = |message: String| format!("{self}, {setting}: {message}");
        let [left, right] = upload_pair(algebra, &self.pair).map_err(failed)?;
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
        self.check_bond_bounds(setting, outcome.bond)
            .map_err(failed)?;
        check_retained_fraction_is_a_share(outcome.retained_fraction).map_err(failed)?;
        Ok(SplitTensors {
            outcome,
            inputs: [left, right],
        })
    }

    /// Checks that the bond `k` of a split at `setting` lies between 1 and
    /// the smallest of the cap and `θ`'s sides, `2·l` and `2·r`.
    fn check_bond_bounds(&self, setting: Setting, k: usize) -> Result<(), String> {
        let Pair { l, r, .. } = self.pair;
        let bound = setting.max_extent.min(2 * l).min(2 * r);
        if (1..=bound).contains(&k) {
            Ok(())
        } else {
            Err(format!("bond {k} is not in 1..={bound}"))
        }
    }

    /// Checks that `split`, at `setting`, retains the share of the squares
    /// of `θ`'s values its bond keeps.
    fn check_retained_fraction(&self, setting: Setting, split: &Split) -> Result<(), String> {
        let expected = retained_fraction_of(&self.values, split.bond);
        let fraction = split.retained_fraction;
        if distance(fraction, expected) > TOLERANCE {
            return Err(format!(
                "{self}, {setting}: bond {} retains {fraction}, not {expected}",
                split.bond
            ));
        }
        Ok(())
    }

    /// `‖θ/‖θ‖ − √W·L·R‖²`, with `L·R` the split's product and `W` the
    /// share of the squares of `θ`'s `k` largest values. By Eckart–Young,
    /// it is the share of the dropped squares when `L·R` is `θ`'s SVD
    /// truncated to those values. It is more for any other product of bond
    /// `k`.
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

/// A split's outcome and its input tensors, held by the algebra, not read
/// back yet.
struct SplitTensors<A: SiteAlgebra> {
    outcome: GateSplitOutcome<A::Tensor>,
    inputs: [A::Tensor; 2],
}
