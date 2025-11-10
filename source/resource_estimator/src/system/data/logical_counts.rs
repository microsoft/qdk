// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    estimates::{ErrorBudget, ErrorBudgetStrategy, Overhead},
    system::constants::{
        NUM_MEASUREMENTS_PER_R, NUM_MEASUREMENTS_PER_TOF, NUM_TS_PER_ROTATION_A_COEFFICIENT,
        NUM_TS_PER_ROTATION_B_COEFFICIENT,
    },
};
use serde::{Deserialize, Serialize};

use super::PartitioningOverhead;

pub trait LayoutReportData {
    fn num_qubits(&self) -> u64;
    fn t_count(&self) -> u64;
    fn rotation_count(&self) -> u64;
    fn rotation_depth(&self) -> u64;
    fn ccz_count(&self) -> u64;
    fn ccix_count(&self) -> u64;
    fn measurement_count(&self) -> u64;
    fn num_ts_per_rotation(&self, eps_synthesis: f64) -> Option<u64>;
}

/// Resource counts output from `qir_estimate_counts` program
#[derive(Default, Debug, Deserialize, Serialize)]
#[serde(
    rename_all(deserialize = "camelCase", serialize = "camelCase"),
    deny_unknown_fields
)]
pub struct LogicalResourceCounts {
    pub num_qubits: u64,
    #[serde(default)]
    pub t_count: u64,
    #[serde(default)]
    pub rotation_count: u64,
    #[serde(default)]
    pub rotation_depth: u64,
    #[serde(default)]
    pub ccz_count: u64,
    #[serde(default)]
    pub ccix_count: u64,
    #[serde(default)]
    pub measurement_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_compute_qubits: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_from_memory_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_to_memory_count: Option<u64>,
}

/// Models the logical resources after layout
///
/// The logical resources comprise the logical depth, the number of qubits, and
/// the number of T states.  If there are rotations, optionally the number of T
/// gates per rotation are specified.
impl Overhead for LogicalResourceCounts {
    // number of qubits per one logical qubit (part of Q in paper)
    fn logical_qubits(&self) -> Result<u64, String> {
        // number of logical qubits for padding (part of Q in paper)
        let qubit_padding = ((8 * self.num_qubits) as f64).sqrt().ceil() as u64 + 1;

        Ok(2 * self.num_qubits + qubit_padding)
    }

    fn logical_depth(&self, budget: &ErrorBudget) -> Result<u64, String> {
        Ok(
            (self.measurement_count + self.rotation_count + self.t_count) * NUM_MEASUREMENTS_PER_R
                + (self.ccz_count + self.ccix_count) * NUM_MEASUREMENTS_PER_TOF
                + self
                    .num_ts_per_rotation(budget.rotations())
                    .unwrap_or_default()
                    * self.rotation_depth
                    * NUM_MEASUREMENTS_PER_R,
        )
    }

    fn num_magic_states(&self, budget: &ErrorBudget, _index: usize) -> Result<u64, String> {
        Ok(4 * (self.ccz_count + self.ccix_count)
            + self.t_count
            + self
                .num_ts_per_rotation(budget.rotations())
                .unwrap_or_default()
                * self.rotation_count)
    }

    fn prune_error_budget(
        &self,
        budget: &mut ErrorBudget,
        strategy: ErrorBudgetStrategy,
    ) -> Result<(), String> {
        if matches![strategy, ErrorBudgetStrategy::PruneLogicalAndRotations] {
            if let Some(num_ts_per_rotation) = self.num_ts_per_rotation(budget.rotations()) {
                let new_rotations_budget = (self.rotation_count as f64)
                    / 2.0_f64.powf(
                        ((num_ts_per_rotation as f64) - NUM_TS_PER_ROTATION_B_COEFFICIENT)
                            / NUM_TS_PER_ROTATION_A_COEFFICIENT,
                    );

                let diff = budget.rotations() - new_rotations_budget;
                budget.set_rotations(new_rotations_budget);
                budget.set_magic_states(budget.magic_states() + diff);
            }
        }

        Ok(())
    }
}

impl PartitioningOverhead for LogicalResourceCounts {
    fn has_tgates(&self) -> bool {
        self.t_count > 0 || self.ccz_count > 0 || self.ccix_count > 0 || self.rotation_count > 0
    }

    fn has_rotations(&self) -> bool {
        self.rotation_count > 0
    }
}

impl LayoutReportData for LogicalResourceCounts {
    fn num_qubits(&self) -> u64 {
        self.num_qubits
    }

    fn t_count(&self) -> u64 {
        self.t_count
    }

    fn rotation_count(&self) -> u64 {
        self.rotation_count
    }

    fn rotation_depth(&self) -> u64 {
        self.rotation_depth
    }

    fn ccz_count(&self) -> u64 {
        self.ccz_count
    }

    fn ccix_count(&self) -> u64 {
        self.ccix_count
    }

    fn measurement_count(&self) -> u64 {
        self.measurement_count
    }

    fn num_ts_per_rotation(&self, eps_synthesis: f64) -> Option<u64> {
        if self.rotation_count > 0 {
            Some(
                (NUM_TS_PER_ROTATION_A_COEFFICIENT
                    * ((self.rotation_count as f64) / eps_synthesis).log2()
                    + NUM_TS_PER_ROTATION_B_COEFFICIENT)
                    .ceil() as _,
            )
        } else {
            None
        }
    }
}
