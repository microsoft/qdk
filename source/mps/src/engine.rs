// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_complex::Complex64;

use crate::{EngineInfo, Measurement, MpsError, SitePauli};

pub type Matrix2 = [[Complex64; 2]; 2];
pub type Matrix4 = [[Complex64; 4]; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SiteId(pub usize);

pub trait MpsEngine: Send {
    fn info(&self) -> EngineInfo;
    fn append_zero_site(&mut self) -> Result<SiteId, MpsError>;
    fn apply_one(&mut self, site: SiteId, matrix: &Matrix2) -> Result<(), MpsError>;
    fn apply_adjacent_two(
        &mut self,
        first: SiteId,
        second: SiteId,
        matrix: &Matrix4,
    ) -> Result<(), MpsError>;
    /// Returns the normalized probability of measuring One.
    fn probability_one(&mut self, site: SiteId) -> Result<f64, MpsError>;
    /// Projects onto and normalizes the selected outcome.
    fn project_z(&mut self, site: SiteId, outcome: Measurement) -> Result<(), MpsError>;
    /// Returns the unnormalized inner product with the Pauli-transformed state.
    fn expectation_pauli_product(&self, factors: &[SitePauli]) -> Result<Complex64, MpsError>;
    fn state_norm(&mut self) -> Result<f64, MpsError>;
    fn reached_bond_dimension(&self) -> usize;
}
