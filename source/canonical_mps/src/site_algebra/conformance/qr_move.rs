// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The cases of [`SiteAlgebra::qr_move`]. Each moves the centre across
//! seeded pairs, with and without a gate, either way. It compares the
//! outcome with the gated product `A·B`.

use super::TOLERANCE;
use super::formulas::{columns_isometry_error, gated_site, relative_distance, rows_isometry_error};
use super::inputs::{LARGE_SHAPE, MOVE_SHAPES, Pair, SHAPES, derived, one_site_gate};
use super::pair_tensors::{
    failure_if_a_new_site_reads_past_its_end, read_inputs, read_new_sites, upload_pair,
};
use crate::site_algebra::{CentreSide, QrMoveCall, QrMoveOutcome, SiteAlgebra};
use dense_linalg::Matrix;
use num_complex::Complex64;
use std::fmt;

/// The product of the new sites is the product of the pair, the gate applied to
/// the site the centre leaves. Nothing is truncated. Catches a truncating move,
/// the gate applied to the other site or transposed, and a remaining factor
/// absorbed on the wrong side. For instance, a gate applied to the other site
/// fails it with: `the pair [1, 1, 1] of seed 600, a gate, centre to the Right:
/// the product of the new sites is 1.2e0 from the gated pair's`.
pub(super) fn gives_back_the_gated_product<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    for scene in scenes(seed) {
        let moved = scene.move_centre(algebra)?;
        let error = relative_distance(moved.product().elements(), scene.gated_product().elements());
        if error > TOLERANCE {
            return Err(format!(
                "{scene}: the product of the new sites is {error:.1e} from the gated pair's"
            ));
        }
    }
    Ok(())
}

/// The bond after the move is the smaller of the pair's bond and the side of
/// the site the centre leaves. That side is `2·left_extent` moving right, and
/// `2·right_extent` moving left. Catches the pair's bond reported unchanged,
/// and the other site's side. For instance, the pair's bond reported unchanged
/// fails it with: `the pair [1, 4, 2] of seed 670, no gate, centre to the
/// Right: bond 4, not 2`.
pub(super) fn bond_is_the_smaller_of_the_bond_and_the_side_it_leaves<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    for scene in scenes(seed) {
        // `move_centre` checks the bond before it reads.
        scene.move_centre(algebra)?;
    }
    Ok(())
}

/// The site the centre leaves is an isometry. The left site's columns are
/// orthonormal after a move to the right. The right site's rows are after a
/// move to the left. Catches a move that leaves the factor on the site it
/// leaves. For instance, a move that doubles the site it leaves and halves the
/// other fails it with: `the pair [1, 1, 1] of seed 600, no gate, centre to the
/// Right: the left site is 3.0e0 from an isometry`.
pub(super) fn site_it_leaves_is_an_isometry<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    for scene in scenes(seed) {
        let moved = scene.move_centre(algebra)?;
        let (error, side) = match scene.centre {
            CentreSide::Right => (columns_isometry_error(&moved.left), "left"),
            CentreSide::Left => (rows_isometry_error(&moved.right), "right"),
        };
        if error > TOLERANCE {
            return Err(format!(
                "{scene}: the {side} site is {error:.1e} from an isometry"
            ));
        }
    }
    Ok(())
}

/// The move leaves its input tensors unchanged, with a gate and without, either
/// way. The engine may still hold them (the sites of a fork's source).
/// Catches a move computed in the inputs' storage, or a gate applied in place.
/// For instance, a move that zeroes its left input fails it with: `the pair [1,
/// 1, 1] of seed 600, no gate, centre to the Right: the move changed its
/// inputs`.
pub(super) fn leaves_its_inputs_unchanged<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    for scene in scenes(seed) {
        let moved = scene.move_centre(algebra)?;
        if moved.inputs != [scene.pair.left.clone(), scene.pair.right.clone()] {
            return Err(format!("{scene}: the move changed its inputs"));
        }
    }
    Ok(())
}

/// The new sites hold exactly the elements their bond gives them, `2·l·bond`
/// and `bond·2·r`, with a gate and without, either way. A read of one more is
/// refused, with an error or a panic, as [`SiteAlgebra::read`] promises.
/// Catches sites held at a capacity larger than the bond. Their reads past the
/// site answer elements that are not the site's. For instance, a left site
/// held with one element more fails it with: `the pair [1, 1, 1] of seed
/// 600, no gate, centre to the Right: bond 1: the left site answered a read
/// of 3 elements, one past its 2`.
pub(super) fn new_sites_hold_exactly_their_elements<A: SiteAlgebra>(
    algebra: &A,
    seed: u64,
) -> Result<(), String> {
    for scene in scenes(seed) {
        let outcome = scene.outcome(algebra)?.outcome;
        let new_sites = [&outcome.left, &outcome.right];
        if let Some(message) =
            failure_if_a_new_site_reads_past_its_end(algebra, &scene.pair, new_sites, outcome.bond)
        {
            return Err(format!("{scene}: {message}"));
        }
    }
    Ok(())
}

/// A pair, a gate or none, and the way the centre moves.
struct Scene {
    pair: Pair,
    gate: Option<[Complex64; 4]>,
    centre: CentreSide,
}

impl Scene {
    /// The pair's sites as the `(2·l) × m` and `m × (2·r)` matrices, the
    /// gate applied to the site the centre leaves, multiplied.
    fn gated_product(&self) -> Matrix {
        let Pair { l, m, r, .. } = self.pair;
        let (mut left, mut right) = (self.pair.left.clone(), self.pair.right.clone());
        if let Some(gate) = &self.gate {
            match self.centre {
                CentreSide::Right => left = gated_site(&left, l, m, gate),
                CentreSide::Left => right = gated_site(&right, m, r, gate),
            }
        }
        let (left, right) = (Matrix::new(2 * l, m, left), Matrix::new(m, 2 * r, right));
        Matrix::product(left.view(), right.view())
    }

    /// The bond the move must report.
    fn expected_bond(&self) -> usize {
        let Pair { l, m, r, .. } = self.pair;
        match self.centre {
            CentreSide::Right => m.min(2 * l),
            CentreSide::Left => m.min(2 * r),
        }
    }

    /// Moves the centre and reads the new sites back. It checks the bond
    /// first, so that no read asks for more than a correct tensor of that
    /// bond holds. Then it reads the input tensors back.
    fn move_centre<A: SiteAlgebra>(&self, algebra: &A) -> Result<Moved, String> {
        let failed = |message: String| format!("{self}: {message}");
        let MoveTensors { outcome, inputs } = self.outcome(algebra)?;
        let new_sites = [&outcome.left, &outcome.right];
        let [left, right] =
            read_new_sites(algebra, &self.pair, new_sites, outcome.bond).map_err(failed)?;
        let inputs = read_inputs(algebra, &self.pair, &inputs).map_err(failed)?;
        Ok(Moved {
            left,
            right,
            inputs,
        })
    }

    /// Moves the centre, and checks the bond as [`Scene::move_centre`]
    /// says. Returns the outcome, and the input tensors.
    fn outcome<A: SiteAlgebra>(&self, algebra: &A) -> Result<MoveTensors<A>, String> {
        let Pair { l, m, r, .. } = self.pair;
        let failed = |message: String| format!("{self}: {message}");
        let [left, right] = upload_pair(algebra, &self.pair).map_err(failed)?;
        let call = QrMoveCall {
            left: &left,
            right: &right,
            left_extent: l,
            bond: m,
            right_extent: r,
            gate: self.gate,
            centre: self.centre,
        };
        let outcome = algebra
            .qr_move(&call)
            .map_err(|error| failed(format!("the move failed: {error}")))?;
        let k = outcome.bond;
        let expected = self.expected_bond();
        if k != expected {
            return Err(failed(format!("bond {k}, not {expected}")));
        }
        Ok(MoveTensors {
            outcome,
            inputs: [left, right],
        })
    }
}

impl fmt::Display for Scene {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let gate = if self.gate.is_some() {
            "a gate"
        } else {
            "no gate"
        };
        write!(f, "{}, {gate}, centre to the {:?}", self.pair, self.centre)
    }
}

/// Every shape of [`SHAPES`], [`LARGE_SHAPE`] and [`MOVE_SHAPES`], with a
/// random gate and without, either way, drawn from the kit's `seed`.
fn scenes(seed: u64) -> impl Iterator<Item = Scene> {
    let shapes = SHAPES.into_iter().chain([LARGE_SHAPE]).chain(MOVE_SHAPES);
    (shapes.zip(0_u64..)).flat_map(move |(shape, index)| {
        let pair_seed = derived(seed, 600 + 10 * index);
        [None, Some(one_site_gate(pair_seed.wrapping_add(5)))]
            .into_iter()
            .flat_map(move |gate| {
                [CentreSide::Right, CentreSide::Left]
                    .into_iter()
                    .map(move |centre| Scene {
                        pair: Pair::graded(shape, pair_seed),
                        gate,
                        centre,
                    })
            })
    })
}

/// A move's outcome, read back.
struct Moved {
    /// The new left site, as the `(2·l) × bond` matrix.
    left: Matrix,
    /// The new right site, as the `bond × (2·r)` matrix.
    right: Matrix,
    /// The input tensors, read back after the move.
    inputs: [Vec<Complex64>; 2],
}

impl Moved {
    fn product(&self) -> Matrix {
        Matrix::product(self.left.view(), self.right.view())
    }
}

/// A move's outcome and its input tensors, held by the algebra, not read
/// back yet.
struct MoveTensors<A: SiteAlgebra> {
    outcome: QrMoveOutcome<A::Tensor>,
    inputs: [A::Tensor; 2],
}
