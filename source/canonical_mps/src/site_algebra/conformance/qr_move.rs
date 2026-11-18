// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The cases of [`SiteAlgebra::qr_move`]: each moves the centre across
//! seeded pairs, with and without a gate, either way, and compares the
//! outcome with the gated product `A·B`.

use super::formulas::{columns_isometry_error, gated_site, relative_distance, rows_isometry_error};
use super::inputs::{LARGE_SHAPE, MOVE_SHAPES, Pair, SHAPES, one_site_gate};
use super::{TOLERANCE, read, upload};
use crate::dense_linalg::Matrix;
use crate::site_algebra::{CentreSide, QrMoveCall, SiteAlgebra};
use num_complex::Complex64;
use std::fmt;

/// The product of the new sites is the product of the pair, the gate
/// applied to the site the centre leaves: nothing is truncated. Catches a
/// truncating move, the gate applied to the other site or transposed, and
/// a remaining factor absorbed on the wrong side.
pub(super) fn product<A: SiteAlgebra>(algebra: &A) -> Result<(), String> {
    for scene in scenes() {
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

/// The bond after the move is the smaller of the pair's bond and the side
/// of the site the centre leaves: `2·left_extent` moving right,
/// `2·right_extent` moving left. Catches the pair's bond reported
/// unchanged, and the other site's side.
pub(super) fn bond<A: SiteAlgebra>(algebra: &A) -> Result<(), String> {
    for scene in scenes() {
        // `move_centre` checks the bond before it reads.
        scene.move_centre(algebra)?;
    }
    Ok(())
}

/// The site the centre leaves is an isometry: the left site's columns are
/// orthonormal after a move to the right, the right site's rows after a
/// move to the left. Catches a move that leaves the factor on the site it
/// leaves.
pub(super) fn isometry<A: SiteAlgebra>(algebra: &A) -> Result<(), String> {
    for scene in scenes() {
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

/// The move leaves its input tensors unchanged, with a gate and without,
/// either way, since the engine may still hold them (the sites of a
/// fork's source). Catches a move computed in the inputs' storage, or a
/// gate applied in place.
pub(super) fn inputs_unchanged<A: SiteAlgebra>(algebra: &A) -> Result<(), String> {
    for scene in scenes() {
        let moved = scene.move_centre(algebra)?;
        if moved.inputs != [scene.pair.left.clone(), scene.pair.right.clone()] {
            return Err(format!("{scene}: the move changed its inputs"));
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

    /// Moves the centre and reads the new sites back, after checking the
    /// bond, so that no read asks for more than a correct tensor of that
    /// bond holds; then reads the input tensors back.
    fn move_centre<A: SiteAlgebra>(&self, algebra: &A) -> Result<Moved, String> {
        let Pair { l, m, r, .. } = self.pair;
        let failed = |message: String| format!("{self}: {message}");
        let left = upload(algebra, &self.pair.left).map_err(failed)?;
        let right = upload(algebra, &self.pair.right).map_err(failed)?;
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
        let new_left = read(algebra, &outcome.left, 2 * l * k).map_err(failed)?;
        let new_right = read(algebra, &outcome.right, k * 2 * r).map_err(failed)?;
        let inputs = [
            read(algebra, &left, 2 * l * m).map_err(failed)?,
            read(algebra, &right, m * 2 * r).map_err(failed)?,
        ];
        Ok(Moved {
            left: Matrix::new(2 * l, k, new_left),
            right: Matrix::new(k, 2 * r, new_right),
            inputs,
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
/// random gate and without, either way.
fn scenes() -> impl Iterator<Item = Scene> {
    let shapes = SHAPES.into_iter().chain([LARGE_SHAPE]).chain(MOVE_SHAPES);
    (shapes.zip(0_u64..)).flat_map(|(shape, index)| {
        let seed = 600 + 10 * index;
        [None, Some(one_site_gate(seed + 5))]
            .into_iter()
            .flat_map(move |gate| {
                [CentreSide::Right, CentreSide::Left]
                    .into_iter()
                    .map(move |centre| Scene {
                        pair: Pair::graded(shape, seed),
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
