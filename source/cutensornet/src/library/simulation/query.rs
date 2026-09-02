use super::SimulationError;
use super::SimulationResult;
use super::circuit::WorkspaceReport;
use num_complex::Complex64;

pub(super) const B2_EXPECTATION_HYPER_SAMPLES: i32 = 8;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AdjacentZQuery {
    pub(super) width: u32,
    pub(super) terms: Vec<[u32; 2]>,
}

impl AdjacentZQuery {
    pub(super) fn new(width: u32) -> Result<Self, SimulationError> {
        if width < 2 {
            return Err(SimulationError::InvalidCircuit {
                reason: "adjacent-ZZ Query requires at least two qubits".to_string(),
            });
        }
        Ok(Self {
            width,
            terms: (0..width - 1).map(|left| [left, left + 1]).collect(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct QueryResult {
    pub(super) raw_expectation: Complex64,
    pub(super) squared_norm: Complex64,
    pub(super) normalized_expectation: Complex64,
    pub(super) hyper_samples: i32,
    pub(super) workspace: WorkspaceReport,
    pub(super) timings: QueryPhaseTimings,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct BaseQueryResult {
    pub(super) state: SimulationResult,
    pub(super) query: QueryResult,
    pub(super) through_query_completion_seconds: f64,
    pub(super) replay_cleanup_seconds: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "phase evidence keeps seconds explicit on every reported duration"
)]
pub(super) struct QueryPhaseTimings {
    pub(super) construction_seconds: f64,
    pub(super) preparation_path_planning_seconds: f64,
    pub(super) workspace_allocation_attachment_seconds: f64,
    pub(super) compute_call_seconds: f64,
    pub(super) synchronization_seconds: f64,
    pub(super) output_validation_seconds: f64,
}

pub(super) fn normalize_expectation(
    raw_expectation: Complex64,
    squared_norm: Complex64,
) -> Result<QueryResult, SimulationError> {
    if !finite(raw_expectation) || !finite(squared_norm) {
        return Err(invalid_query("Query returned a non-finite value"));
    }
    if squared_norm.re <= 0.0 {
        return Err(invalid_query("Query returned a nonpositive squared norm"));
    }
    let imaginary_tolerance = 1.0e-12 * squared_norm.re.abs().max(1.0);
    if squared_norm.im.abs() > imaginary_tolerance {
        return Err(invalid_query(
            "Query squared norm has a material imaginary component",
        ));
    }
    let normalized_expectation = raw_expectation / squared_norm;
    if !finite(normalized_expectation) {
        return Err(invalid_query("normalized Query value is non-finite"));
    }
    Ok(QueryResult {
        raw_expectation,
        squared_norm,
        normalized_expectation,
        hyper_samples: B2_EXPECTATION_HYPER_SAMPLES,
        workspace: WorkspaceReport {
            total_bytes: 0,
            free_before_bytes: 0,
            requested_maximum_bytes: 0,
            native_recommended_bytes: 0,
            allocated_bytes: 0,
            free_after_cleanup_bytes: 0,
        },
        timings: QueryPhaseTimings::default(),
    })
}

fn finite(value: Complex64) -> bool {
    value.re.is_finite() && value.im.is_finite()
}

fn invalid_query(reason: &'static str) -> SimulationError {
    SimulationError::InvalidNativeResult {
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AdjacentZQuery, B2_EXPECTATION_HYPER_SAMPLES, QueryResult, WorkspaceReport,
        normalize_expectation,
    };
    use crate::library::simulation::SimulationError;
    use num_complex::Complex64;

    #[test]
    fn adjacent_z_query_contains_one_ordered_product_per_bond() {
        let query = AdjacentZQuery::new(5).expect("width should be valid");

        assert_eq!(query.width, 5);
        assert_eq!(query.terms, [[0, 1], [1, 2], [2, 3], [3, 4]]);
        assert!(AdjacentZQuery::new(1).is_err());
    }

    #[test]
    fn normalizes_raw_expectation_by_squared_norm() {
        let result =
            normalize_expectation(Complex64::new(6.0, 0.0), Complex64::new(2.0, f64::EPSILON))
                .expect("valid Query outputs should normalize");

        assert_eq!(
            result,
            QueryResult {
                raw_expectation: Complex64::new(6.0, 0.0),
                squared_norm: Complex64::new(2.0, f64::EPSILON),
                normalized_expectation: Complex64::new(3.0, -3.330_669_073_875_469_6e-16),
                hyper_samples: B2_EXPECTATION_HYPER_SAMPLES,
                workspace: WorkspaceReport {
                    total_bytes: 0,
                    free_before_bytes: 0,
                    requested_maximum_bytes: 0,
                    native_recommended_bytes: 0,
                    allocated_bytes: 0,
                    free_after_cleanup_bytes: 0,
                },
                timings: super::QueryPhaseTimings::default(),
            }
        );
    }

    #[test]
    fn rejects_invalid_query_outputs_before_normalization() {
        let invalid = [
            (Complex64::new(f64::NAN, 0.0), Complex64::new(1.0, 0.0)),
            (Complex64::new(1.0, 0.0), Complex64::new(f64::INFINITY, 0.0)),
            (Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)),
            (Complex64::new(1.0, 0.0), Complex64::new(-1.0, 0.0)),
            (Complex64::new(1.0, 0.0), Complex64::new(1.0, 1.0e-6)),
        ];

        for (expectation, norm) in invalid {
            assert!(matches!(
                normalize_expectation(expectation, norm),
                Err(SimulationError::InvalidNativeResult { .. })
            ));
        }
    }
}
