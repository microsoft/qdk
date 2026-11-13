// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! EXPERIMENTAL, development only (not for review): [`DevBatchRouter`],
//! the resumer's routing and gate fusion on batches of gates given on
//! sites, for a harness that drives an engine without a QIR program (the
//! polyene QPE prefix, `examples/polyene_routing_dev.rs`).
//!
//! A batch goes through the same calls as in `GroupedRun::run_group`
//! (`Placement::route`, then `rewrite_runs` with the fusion, or the other
//! way round when the routing says so), and a bond read through
//! `Placement::bond_read`, as `GroupedRun::read_bond` makes it. The layout
//! is the caller's: site `i` holds qubit `i`.

use num_complex::Complex64;

use crate::qir::OperatorMatrix;

use super::lowering::{Operator, Step};
use super::rewrites::fusion::GateFusion;
use super::rewrites::rewrite_runs;
use super::routing::{Placement, Routing};

/// EXPERIMENTAL, development only: one state's routing and fusion, batch
/// by batch.
pub struct DevBatchRouter {
    routing: Routing,
    fusion: GateFusion,
    placement: Option<Placement>,
    max_bond_dimension: usize,
}

impl DevBatchRouter {
    /// The router of a state of `sites` sites in the layout's order, with
    /// bonds of at most `max_bond_dimension`.
    #[must_use]
    pub fn new(
        sites: usize,
        max_bond_dimension: usize,
        routing: Routing,
        fusion: GateFusion,
    ) -> Self {
        Self {
            routing,
            fusion,
            placement: routing
                .tuning()
                .map(|tuning| Placement::tuned(sites, tuning)),
            max_bond_dimension,
        }
    }

    /// The operators the engine applies for one batch of `gates`, each on
    /// layout sites with its row-major matrix (2×2 or 4×4), in order.
    ///
    /// # Panics
    ///
    /// When a gate is on neither one nor two sites, or its matrix is not
    /// 2×2 or 4×4.
    #[must_use]
    pub fn batch<'g>(
        &mut self,
        gates: impl IntoIterator<Item = (&'g [usize], &'g [Complex64])>,
    ) -> Vec<(Vec<usize>, Vec<Complex64>)> {
        let steps: Vec<Step> = gates
            .into_iter()
            .map(|(sites, matrix)| {
                let (site_slots, matrix) = match (sites, matrix.len()) {
                    (&[a], 4) => (
                        [a, 0],
                        OperatorMatrix::One(matrix.try_into().expect("a 2×2 matrix")),
                    ),
                    (&[a, b], 16) => (
                        [a, b],
                        OperatorMatrix::Two(matrix.try_into().expect("a 4×4 matrix")),
                    ),
                    _ => panic!("a gate on one or two sites, with its 2×2 or 4×4 matrix"),
                };
                Step::Apply(Operator { site_slots, matrix })
            })
            .collect();
        let steps = match (self.routing.tuning(), self.placement.as_mut()) {
            (Some(tuning), Some(placement)) if !tuning.after_fusion => {
                rewrite_runs(placement.route(steps), &self.fusion)
            }
            (Some(_), Some(placement)) => placement.route(rewrite_runs(steps, &self.fusion)),
            _ => rewrite_runs(steps, &self.fusion),
        };
        steps
            .into_iter()
            .map(|step| match step {
                Step::Apply(operator) => (
                    operator.sites().to_vec(),
                    operator.matrix.row_major().to_vec(),
                ),
                Step::Measure { .. } | Step::Reset { .. } => {
                    unreachable!("a batch of gates has no draws")
                }
            })
            .collect()
    }

    /// Records a read of the state's largest bond, as the resumer reads it
    /// after each batch: at the max bond dimension, the next batch restores
    /// the layout's order and the state falls back to the engine's routing.
    pub fn bond_read(&mut self, largest_bond: usize) {
        if let Some(placement) = self.placement.as_mut() {
            placement.bond_read(largest_bond, self.max_bond_dimension);
        }
    }
}
