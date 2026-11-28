// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`DefectiveFakeAlgebra`]: the CPU algebra with one rule of the
//! contract broken, so that the kit's tests can show which case catches
//! each fault.
//!
//! Its tensors are shared buffers that a drop fills with NaN, as a backend
//! frees its memory, so that a duplicate that shares its source's storage
//! shows; its reads copy, and refuse a read past a tensor's end with an
//! error, so that the kit's own reads are checked to stay within bounds.

use crate::CpuAlgebra;
use crate::site_algebra::{
    CentreSide, GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use dense_linalg::SvdError;
use num_complex::Complex64;
use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

/// The rule a [`DefectiveFakeAlgebra`] breaks, each as a backend might.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Defect {
    /// A read returns one element fewer than asked.
    ShortRead,
    /// A read returns as many elements as asked, its first one negated.
    AlteredRead,
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
    /// A split reports a bond of zero.
    ZeroBond,
    /// A split whose bond is smaller than its cap and the pair's bond pads
    /// it with zeros to the smaller of those two: it bounds the bond by the
    /// pair's bond, not by `θ`'s sides.
    BondPaddedToThePairsBond,
    /// A split absorbs the values on the side away from the centre.
    CentreOnTheWrongSide,
    /// A split at a cutoff above zero scales the first bond index of the
    /// site away from the centre by 2 and that of the centre's site by
    /// ½: the product is unchanged, the isometry lost.
    BondScaledAtAPositiveCutoff,
    /// A split at a cutoff above zero returns sites of NaN, with the bond
    /// and the retained fraction right.
    NanSitesAtAPositiveCutoff,
    /// A split drops up to twice its cutoff's share.
    CutoffDoubled,
    /// A split takes a cutoff of zero as 1e-12.
    SmallValuesDroppedAtCutoffZero,
    /// A split reports the square root of its retained fraction.
    RootOfTheRetainedFraction,
    /// A split that drops nothing reports a retained fraction one rounding
    /// below 1.
    RetainedFractionBelowOne,
    /// A split that drops nothing reports a retained fraction two roundings
    /// above 1.
    RetainedFractionAboveOne,
    /// A split at a cutoff above zero reports a retained fraction of NaN,
    /// with the sites and the bond right.
    NanFractionAtAPositiveCutoff,
    /// A split that meets a tail of exactly its cutoff's share drops it,
    /// and reports the retained fraction of keeping it.
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
    /// A split or a move zeroes its left input.
    InputsOverwritten,
    /// A split holds its new sites padded with zeros to the smaller of its
    /// cap and `θ`'s sides, not to its bond, and a move holds each new site
    /// with one zero more.
    SitesHeldPastTheirEnd,
}

/// The CPU algebra with `defect`, or with none.
pub(super) struct DefectiveFakeAlgebra {
    defect: Option<Defect>,
}

impl DefectiveFakeAlgebra {
    /// The CPU algebra with no defect, on [`DefectiveTensor`]s.
    pub(super) fn faithful() -> Self {
        Self { defect: None }
    }

    /// The CPU algebra with `defect`.
    pub(super) fn with(defect: Defect) -> Self {
        Self {
            defect: Some(defect),
        }
    }

    fn has(&self, defect: Defect) -> bool {
        self.defect == Some(defect)
    }

    /// Zeroes `input` if the defect is [`Defect::InputsOverwritten`].
    fn overwrite_if_defective(&self, input: &DefectiveTensor) {
        if self.has(Defect::InputsOverwritten) {
            input.0.borrow_mut().fill(Complex64::ZERO);
        }
    }

    /// The CPU algebra's split of `call` at `settings`, which may differ
    /// from the call's own.
    fn split_at(
        call: &GateSplitCall<'_, DefectiveTensor>,
        settings: &SplitSettings,
    ) -> Result<GateSplitOutcome<Vec<Complex64>>, DefectiveError> {
        let (left, right) = (call.left.values(), call.right.values());
        let inner = GateSplitCall {
            left: &left,
            right: &right,
            left_extent: call.left_extent,
            bond: call.bond,
            right_extent: call.right_extent,
            gate: settings.gate,
            max_extent: settings.max_extent,
            cutoff: settings.cutoff,
            centre: settings.centre,
        };
        Ok(CpuAlgebra.gate_split(&inner)?)
    }

    /// The settings a split of `call` runs at: the call's own, or those
    /// the defect misreads them as.
    fn split_settings_as_read(&self, call: &GateSplitCall<'_, DefectiveTensor>) -> SplitSettings {
        let mut settings = SplitSettings {
            gate: call.gate,
            max_extent: call.max_extent,
            cutoff: call.cutoff,
            centre: call.centre,
        };
        match self.defect {
            Some(Defect::TransposedGate) => {
                settings.gate = std::array::from_fn(|i| call.gate[(i % 4) * 4 + i / 4]);
            }
            Some(Defect::CapIgnored) => settings.max_extent = usize::MAX,
            Some(Defect::CentreOnTheWrongSide) => settings.centre = other_side(call.centre),
            Some(Defect::CutoffDoubled) => settings.cutoff = (2.0 * call.cutoff).min(0.9),
            Some(Defect::SmallValuesDroppedAtCutoffZero) => {
                settings.cutoff = call.cutoff.max(1e-12);
            }
            Some(Defect::TieTakenOneWayReportedTheOther) => {
                settings.cutoff = (call.cutoff * (1.0 + 1e-6)).min(1.0 - f64::EPSILON / 2.0);
            }
            _ => {}
        }
        settings
    }

    /// Alters `outcome`, the split of `call` at `settings`, as the defect
    /// does after the split.
    #[expect(
        clippy::float_cmp,
        reason = "the defects alter a retained fraction of exactly 1"
    )]
    fn alter_split_outcome(
        &self,
        call: &GateSplitCall<'_, DefectiveTensor>,
        settings: &SplitSettings,
        outcome: &mut GateSplitOutcome<Vec<Complex64>>,
    ) -> Result<(), DefectiveError> {
        let rows = 2 * call.left_extent;
        let cols = 2 * call.right_extent;
        match self.defect {
            Some(Defect::SmallestValuesKept) => {
                let untruncated = SplitSettings {
                    max_extent: usize::MAX,
                    cutoff: 0.0,
                    ..*settings
                };
                let full = Self::split_at(call, &untruncated)?;
                keep_the_smallest_values(outcome, &full, rows, cols, settings.centre);
            }
            Some(Defect::UnnormalisedValues) => {
                let scale = outcome.retained_fraction.sqrt();
                for x in centre_site(outcome, settings.centre) {
                    *x *= scale;
                }
            }
            Some(Defect::RootOfTheRetainedFraction) => {
                outcome.retained_fraction = outcome.retained_fraction.sqrt();
            }
            Some(Defect::RetainedFractionBelowOne) if outcome.retained_fraction == 1.0 => {
                outcome.retained_fraction = 1.0 - f64::EPSILON;
            }
            Some(Defect::RetainedFractionAboveOne) if outcome.retained_fraction == 1.0 => {
                outcome.retained_fraction = 1.0 + 2.0 * f64::EPSILON;
            }
            Some(Defect::NanFractionAtAPositiveCutoff) if call.cutoff > 0.0 => {
                outcome.retained_fraction = f64::NAN;
            }
            Some(Defect::NanSitesAtAPositiveCutoff) if call.cutoff > 0.0 => {
                let nan = Complex64::new(f64::NAN, f64::NAN);
                outcome.left.fill(nan);
                outcome.right.fill(nan);
            }
            Some(Defect::BondScaledAtAPositiveCutoff) if call.cutoff > 0.0 => {
                scale_the_first_bond_index(outcome, rows, settings.centre);
            }
            Some(Defect::TieTakenOneWayReportedTheOther) => {
                let kept = SplitSettings {
                    cutoff: call.cutoff * (1.0 - 1e-6),
                    ..*settings
                };
                let other = Self::split_at(call, &kept)?;
                outcome.retained_fraction = other.retained_fraction;
            }
            Some(Defect::SitesHeldPastTheirEnd) => {
                let capacity = settings.max_extent.min(rows).min(cols);
                outcome.left.resize(rows * capacity, Complex64::ZERO);
                outcome.right.resize(capacity * cols, Complex64::ZERO);
            }
            Some(Defect::ZeroBond) => outcome.bond = 0,
            Some(Defect::BondPaddedToThePairsBond) => {
                pad_bond(outcome, rows, settings.max_extent.min(call.bond));
            }
            _ => {}
        }
        Ok(())
    }

    /// Alters `outcome`, the move of `call`, as the defect does after the
    /// move.
    fn alter_move_outcome(
        &self,
        call: &QrMoveCall<'_, DefectiveTensor>,
        outcome: &mut QrMoveOutcome<Vec<Complex64>>,
    ) {
        let (l, r) = (call.left_extent, call.right_extent);
        let k = outcome.bond;
        match self.defect {
            Some(Defect::TruncatingMove) if k > 1 => {
                outcome.left[2 * l * (k - 1)..].fill(Complex64::ZERO);
                for j in 0..2 * r {
                    outcome.right[k * j + k - 1] = Complex64::ZERO;
                }
            }
            Some(Defect::MoveBondUnchanged) => outcome.bond = call.bond,
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
            Some(Defect::SitesHeldPastTheirEnd) => {
                outcome.left.push(Complex64::ZERO);
                outcome.right.push(Complex64::ZERO);
            }
            _ => {}
        }
    }
}

/// A split's gate, cap, cutoff and centre, as a [`DefectiveFakeAlgebra`]
/// reads them.
#[derive(Clone, Copy)]
struct SplitSettings {
    gate: [Complex64; 16],
    max_extent: usize,
    cutoff: f64,
    centre: CentreSide,
}

/// A [`DefectiveFakeAlgebra`]'s tensor: a buffer the tensor's duplicates may
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

/// Why a [`DefectiveFakeAlgebra`]'s call failed.
#[derive(Debug, thiserror::Error)]
pub(super) enum DefectiveError {
    #[error(transparent)]
    Svd(#[from] SvdError),
    #[error("a read of {len} elements of a tensor of {held}")]
    ReadPastTheEnd { len: usize, held: usize },
    #[error("the move failed")]
    MoveFailed,
}

impl SiteAlgebra for DefectiveFakeAlgebra {
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
        let mut read = values[..len].to_vec();
        if self.has(Defect::AlteredRead)
            && let Some(first) = read.first_mut()
        {
            *first = -*first;
        }
        Ok(Cow::Owned(read))
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

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
        let settings = self.split_settings_as_read(call);
        let mut outcome = Self::split_at(call, &settings)?;
        self.overwrite_if_defective(call.left);
        self.alter_split_outcome(call, &settings, &mut outcome)?;
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
            .expect("the CPU algebra's moves never fail");
        self.overwrite_if_defective(call.left);
        self.alter_move_outcome(call, &mut outcome);
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

/// The site of `outcome` that holds the centre.
fn centre_site(
    outcome: &mut GateSplitOutcome<Vec<Complex64>>,
    centre: CentreSide,
) -> &mut Vec<Complex64> {
    match centre {
        CentreSide::Left => &mut outcome.left,
        CentreSide::Right => &mut outcome.right,
    }
}

/// Replaces `outcome`'s sites, of bond `k`, with the `k` smallest values'
/// columns and rows of `full`, the untruncated split of `n` values: the
/// left site's last `k` columns (`rows` each) and the right site's last
/// `k` rows (of `cols` columns). Then the centre's site is normalised.
fn keep_the_smallest_values(
    outcome: &mut GateSplitOutcome<Vec<Complex64>>,
    full: &GateSplitOutcome<Vec<Complex64>>,
    rows: usize,
    cols: usize,
    centre: CentreSide,
) {
    let (n, k) = (full.bond, outcome.bond);
    outcome.left = full.left[rows * (n - k)..].to_vec();
    outcome.right = (0..cols)
        .flat_map(|j| full.right[n * j + n - k..n * (j + 1)].iter().copied())
        .collect();
    let site = centre_site(outcome, centre);
    let norm = site.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
    for x in site {
        *x /= norm;
    }
}

/// Scales the first bond index of `outcome`'s sites, `L·D` and `D⁻¹·R`,
/// with `D = diag(2, 1, …)` when the centre is on the right and its
/// inverse when on the left: the product is unchanged, the isometry lost.
/// The left site's columns have `rows` elements.
fn scale_the_first_bond_index(
    outcome: &mut GateSplitOutcome<Vec<Complex64>>,
    rows: usize,
    centre: CentreSide,
) {
    let (left_scale, right_scale) = match centre {
        CentreSide::Right => (2.0, 0.5),
        CentreSide::Left => (0.5, 2.0),
    };
    for x in &mut outcome.left[..rows] {
        *x *= left_scale;
    }
    for x in outcome.right.iter_mut().step_by(outcome.bond) {
        *x *= right_scale;
    }
}

/// Pads `outcome`'s bond with zeros up to `padded`, when that is larger:
/// zero columns after the left site's (`rows` each), zero rows below the
/// right site's.
fn pad_bond(outcome: &mut GateSplitOutcome<Vec<Complex64>>, rows: usize, padded: usize) {
    let kept = outcome.bond;
    if padded > kept {
        outcome.left.resize(rows * padded, Complex64::ZERO);
        outcome.right = (outcome.right.chunks_exact(kept))
            .flat_map(|column| {
                let zeros = std::iter::repeat_n(Complex64::ZERO, padded - kept);
                column.iter().copied().chain(zeros)
            })
            .collect();
        outcome.bond = padded;
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
