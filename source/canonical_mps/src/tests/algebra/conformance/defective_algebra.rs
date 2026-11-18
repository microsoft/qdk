// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DefectiveAlgebra`]: the reference algebra with one rule of the
//! contract broken, so that the kit's tests can show which case catches
//! each fault.
//!
//! Its tensors are shared buffers that a drop fills with NaN, as a backend
//! frees its memory, so that a duplicate that shares its source's storage
//! shows; its reads copy, and refuse a read past a tensor's end with an
//! error, so that the kit's own reads are checked to stay within bounds.

use crate::CpuAlgebra;
use crate::algebra::{
    CentreSide, GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use crate::dense_linalg::SvdError;
use num_complex::Complex64;
use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

/// The rule a [`DefectiveAlgebra`] breaks, each as a backend might.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Defect {
    /// A read returns one element fewer than asked.
    ShortRead,
    /// A duplicate holds its source's elements rotated by one.
    ShiftedDuplicate,
    /// A duplicate shares its source's storage.
    AliasedDuplicate,
    /// A split reads the two-site gate transposed.
    TransposedGate,
    /// A split keeps the smallest values within its bond, not the largest.
    SmallestValuesKept,
    /// A split absorbs the kept values without dividing them by their
    /// norm.
    UnnormalisedValues,
    /// A split ignores its cap.
    CapIgnored,
    /// A split absorbs the values on the side away from the centre.
    CentreOnTheWrongSide,
    /// A split drops up to twice its cutoff's share.
    CutoffDoubled,
    /// A split takes a cutoff of zero as 1e-12.
    SmallValuesDroppedAtCutoffZero,
    /// A split reports the square root of the share it kept.
    RootOfTheShare,
    /// A split that drops nothing reports a share one rounding below 1.
    ShareBelowOne,
    /// A split that meets a tail of exactly its cutoff's share drops it,
    /// and reports the share of keeping it.
    TieTakenOneWayReportedTheOther,
    /// A move zeroes the last column of the left site and the last row of
    /// the right one.
    TruncatingMove,
    /// A move applies its one-site gate to the site the centre reaches.
    GateOnTheWrongSite,
    /// A move reports the pair's bond unchanged.
    MoveBondUnchanged,
    /// A move doubles the site the centre leaves and halves the other.
    FactorLeftOnTheLeavingSite,
    /// A move fails.
    MoveFails,
}

/// The reference algebra with `defect`, or with none.
pub(super) struct DefectiveAlgebra {
    defect: Option<Defect>,
}

impl DefectiveAlgebra {
    /// The reference algebra with no defect, on [`DefectiveTensor`]s.
    pub(super) fn faithful() -> Self {
        Self { defect: None }
    }

    /// The reference algebra with `defect`.
    pub(super) fn with(defect: Defect) -> Self {
        Self {
            defect: Some(defect),
        }
    }

    fn has(&self, defect: Defect) -> bool {
        self.defect == Some(defect)
    }

    /// The reference algebra's split of `call` at another cap and cutoff.
    fn split_at(
        call: &GateSplitCall<'_, DefectiveTensor>,
        gate: [Complex64; 16],
        max_extent: usize,
        cutoff: f64,
        centre: CentreSide,
    ) -> Result<GateSplitOutcome<Vec<Complex64>>, DefectiveError> {
        let (left, right) = (call.left.values(), call.right.values());
        let inner = GateSplitCall {
            left: &left,
            right: &right,
            left_extent: call.left_extent,
            bond: call.bond,
            right_extent: call.right_extent,
            gate,
            max_extent,
            cutoff,
            centre,
        };
        Ok(CpuAlgebra.gate_split(&inner)?)
    }
}

/// A [`DefectiveAlgebra`]'s tensor: a buffer the tensor's duplicates may
/// share, filled with NaN when the tensor is dropped.
pub(super) struct DefectiveTensor(Rc<RefCell<Vec<Complex64>>>);

impl DefectiveTensor {
    fn new(values: Vec<Complex64>) -> Self {
        Self(Rc::new(RefCell::new(values)))
    }

    fn values(&self) -> Vec<Complex64> {
        self.0.borrow().clone()
    }
}

impl Drop for DefectiveTensor {
    fn drop(&mut self) {
        self.0.borrow_mut().fill(Complex64::new(f64::NAN, f64::NAN));
    }
}

/// Why a [`DefectiveAlgebra`]'s call failed.
#[derive(Debug, thiserror::Error)]
pub(super) enum DefectiveError {
    #[error(transparent)]
    Svd(#[from] SvdError),
    #[error("a read of {len} elements of a tensor of {held}")]
    ReadPastTheEnd { len: usize, held: usize },
    #[error("the move failed")]
    MoveFailed,
}

impl SiteAlgebra for DefectiveAlgebra {
    type Tensor = DefectiveTensor;
    type Error = DefectiveError;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error> {
        Ok(DefectiveTensor::new(values.to_vec()))
    }

    fn read<'t>(
        &self,
        tensor: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error> {
        let values = tensor.0.borrow();
        if len > values.len() {
            return Err(DefectiveError::ReadPastTheEnd {
                len,
                held: values.len(),
            });
        }
        let len = if self.has(Defect::ShortRead) {
            len.saturating_sub(1)
        } else {
            len
        };
        Ok(Cow::Owned(values[..len].to_vec()))
    }

    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Self::Error> {
        if self.has(Defect::AliasedDuplicate) {
            return Ok(DefectiveTensor(Rc::clone(&tensor.0)));
        }
        let mut copy = tensor.0.borrow()[..len].to_vec();
        if self.has(Defect::ShiftedDuplicate) {
            copy.rotate_left(1);
        }
        Ok(DefectiveTensor::new(copy))
    }

    #[expect(clippy::float_cmp, reason = "a share of exactly 1 is the rule broken")]
    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
        let (mut gate, mut max_extent, mut cutoff, mut centre) =
            (call.gate, call.max_extent, call.cutoff, call.centre);
        match self.defect {
            Some(Defect::TransposedGate) => {
                gate = std::array::from_fn(|i| call.gate[(i % 4) * 4 + i / 4]);
            }
            Some(Defect::CapIgnored) => max_extent = usize::MAX,
            Some(Defect::CentreOnTheWrongSide) => centre = other_side(centre),
            Some(Defect::CutoffDoubled) => cutoff = (2.0 * cutoff).min(0.9),
            Some(Defect::SmallValuesDroppedAtCutoffZero) => cutoff = cutoff.max(1e-12),
            Some(Defect::TieTakenOneWayReportedTheOther) => {
                cutoff = (cutoff * (1.0 + 1e-6)).min(1.0 - f64::EPSILON / 2.0);
            }
            _ => {}
        }
        let mut outcome = Self::split_at(call, gate, max_extent, cutoff, centre)?;
        let rows = 2 * call.left_extent;
        let cols = 2 * call.right_extent;
        match self.defect {
            Some(Defect::SmallestValuesKept) => {
                let full = Self::split_at(call, gate, usize::MAX, 0.0, centre)?;
                let (n, k) = (full.bond, outcome.bond);
                outcome.left = full.left[rows * (n - k)..].to_vec();
                outcome.right = (0..cols)
                    .flat_map(|j| full.right[n * j + n - k..n * (j + 1)].iter().copied())
                    .collect();
                let site = match centre {
                    CentreSide::Left => &mut outcome.left,
                    CentreSide::Right => &mut outcome.right,
                };
                let norm = site.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
                for x in site {
                    *x /= norm;
                }
            }
            Some(Defect::UnnormalisedValues) => {
                let site = match centre {
                    CentreSide::Left => &mut outcome.left,
                    CentreSide::Right => &mut outcome.right,
                };
                let scale = outcome.retained_fraction.sqrt();
                for x in site {
                    *x *= scale;
                }
            }
            Some(Defect::RootOfTheShare) => {
                outcome.retained_fraction = outcome.retained_fraction.sqrt();
            }
            Some(Defect::ShareBelowOne) if outcome.retained_fraction == 1.0 => {
                outcome.retained_fraction = 1.0 - f64::EPSILON;
            }
            Some(Defect::TieTakenOneWayReportedTheOther) => {
                let kept = call.cutoff * (1.0 - 1e-6);
                let other = Self::split_at(call, gate, max_extent, kept, centre)?;
                outcome.retained_fraction = other.retained_fraction;
            }
            _ => {}
        }
        Ok(GateSplitOutcome {
            left: DefectiveTensor::new(outcome.left),
            right: DefectiveTensor::new(outcome.right),
            bond: outcome.bond,
            retained_fraction: outcome.retained_fraction,
        })
    }

    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error> {
        if self.has(Defect::MoveFails) {
            return Err(DefectiveError::MoveFailed);
        }
        let (l, m, r) = (call.left_extent, call.bond, call.right_extent);
        let (mut left, mut right) = (call.left.values(), call.right.values());
        let mut gate = call.gate;
        if self.has(Defect::GateOnTheWrongSite)
            && let Some(g) = gate.take()
        {
            match call.centre {
                CentreSide::Right => apply_one(&mut right, m, r, &g),
                CentreSide::Left => apply_one(&mut left, l, m, &g),
            }
        }
        let inner = QrMoveCall {
            left: &left,
            right: &right,
            left_extent: l,
            bond: m,
            right_extent: r,
            gate,
            centre: call.centre,
        };
        let mut outcome = CpuAlgebra
            .qr_move(&inner)
            .expect("the reference algebra's moves never fail");
        let k = outcome.bond;
        match self.defect {
            Some(Defect::TruncatingMove) if k > 1 => {
                outcome.left[2 * l * (k - 1)..].fill(Complex64::ZERO);
                for j in 0..2 * r {
                    outcome.right[k * j + k - 1] = Complex64::ZERO;
                }
            }
            Some(Defect::MoveBondUnchanged) => outcome.bond = m,
            Some(Defect::FactorLeftOnTheLeavingSite) => {
                let (leaving, reached) = match call.centre {
                    CentreSide::Right => (&mut outcome.left, &mut outcome.right),
                    CentreSide::Left => (&mut outcome.right, &mut outcome.left),
                };
                for x in leaving {
                    *x *= 2.0;
                }
                for x in reached {
                    *x *= 0.5;
                }
            }
            _ => {}
        }
        Ok(QrMoveOutcome {
            left: DefectiveTensor::new(outcome.left),
            right: DefectiveTensor::new(outcome.right),
            bond: outcome.bond,
        })
    }
}

fn other_side(centre: CentreSide) -> CentreSide {
    match centre {
        CentreSide::Left => CentreSide::Right,
        CentreSide::Right => CentreSide::Left,
    }
}

/// Applies the one-site `gate` (`G[s, p]` at `2s + p`) to the site of
/// extents `[left, 2, right]`.
fn apply_one(site: &mut [Complex64], left: usize, right: usize, gate: &[Complex64; 4]) {
    for b in 0..right {
        for a in 0..left {
            let at = |s: usize| a + left * (s + 2 * b);
            let (zero, one) = (site[at(0)], site[at(1)]);
            site[at(0)] = gate[0] * zero + gate[1] * one;
            site[at(1)] = gate[2] * zero + gate[3] * one;
        }
    }
}
