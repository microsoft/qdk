// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): the Schmidt values of a
//! state at every cut, to read the bond a cut needs for a given discarded
//! weight without truncating anything.
//!
//! On a copy of the chain, the centre is moved to the first site; then, with
//! the centre on site `k`, the sites left of it left-orthonormal and those
//! right of it right-orthonormal, the singular values of site `k` read as a
//! `2·left × right` matrix are the Schmidt values of the cut `k | k + 1`.
//! The centre then moves one site right by a QR, as reads move it. It
//! costs one SVD and one QR per site; the state is left as it was.

use super::Engine;
use crate::{CpuMpsError, chain::Chain, linear_algebra::LinearAlgebra};

impl<A: LinearAlgebra> Engine<A> {
    /// The Schmidt values of each cut, nonincreasing, normalized so that
    /// their squares sum to 1.
    pub(crate) fn dev_schmidt_values(&self, chain: &Chain) -> Result<Vec<Vec<f64>>, CpuMpsError> {
        let mut chain = chain.clone();
        self.centre_on(&mut chain, 0)?;
        let cuts = chain.sites.len() - 1;
        let mut spectra = Vec::with_capacity(cuts);
        for _ in 0..cuts {
            let values = self
                .algebra
                .svd(chain.sites[chain.centre].rows_left())?
                .singular_values;
            let norm: f64 = values.iter().map(|s| s * s).sum::<f64>().sqrt();
            spectra.push(values.iter().map(|s| s / norm).collect());
            self.shift_centre_right(&mut chain)?;
        }
        Ok(spectra)
    }
}
