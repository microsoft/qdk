// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::num::NonZeroUsize;

use crate::MpsError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Precision {
    Complex64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TruncationPolicy {
    pub max_relative_discarded_squared_weight_per_split: Option<f64>,
    pub max_bond_dimension: Option<NonZeroUsize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourcePolicy {
    pub max_cpu_threads: Option<NonZeroUsize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionPolicy {
    pub precision: Precision,
    pub truncation: TruncationPolicy,
    pub shot_seed: u64,
    pub resources: ResourcePolicy,
}

impl ExecutionPolicy {
    pub fn validate(&self) -> Result<(), MpsError> {
        if let Some(threshold) = self
            .truncation
            .max_relative_discarded_squared_weight_per_split
            && (!threshold.is_finite() || !(0.0..1.0).contains(&threshold))
        {
            return Err(MpsError::InvalidPolicy(format!(
                "max_relative_discarded_squared_weight_per_split must be finite and in [0, 1), got {threshold}"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(threshold: f64) -> ExecutionPolicy {
        ExecutionPolicy {
            precision: Precision::Complex64,
            truncation: TruncationPolicy {
                max_relative_discarded_squared_weight_per_split: Some(threshold),
                max_bond_dimension: None,
            },
            shot_seed: 0,
            resources: ResourcePolicy {
                max_cpu_threads: None,
            },
        }
    }

    #[test]
    fn validates_local_threshold() {
        assert!(policy(0.0).validate().is_ok());
        assert!(policy(1.0).validate().is_err());
        assert!(policy(-f64::EPSILON).validate().is_err());
        assert!(policy(f64::NAN).validate().is_err());
    }
}
