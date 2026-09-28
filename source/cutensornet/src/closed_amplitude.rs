// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The cuTensorNet policy for the shared closed-amplitude orchestration.
#![cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(
        dead_code,
        reason = "preflight and policy are also tested without CUDA"
    )
)]

use std::{fmt, num::NonZeroUsize};

use num_complex::Complex64;
use qdk_simulators::execution::{
    AmplitudeContractionError, CircuitTensorNetwork, ContractionCost, ContractionReports,
    ExecutionFailure, closed_amplitude_query,
};
use thiserror::Error;

use crate::{
    AvailabilityError, discover,
    simulation::{
        SimulationError,
        contraction::{
            adapter::{CuTensorNetContractionOptimizerSettings, CuTensorNetPlanningReport},
            execution::CuTensorNetResourceReport,
        },
    },
};

#[derive(Clone, Copy, Debug)]
pub struct ContractionSettings {
    pub hyper_samples: u32,
    pub seed: u32,
}

#[derive(Debug)]
pub struct ClosedAmplitude {
    pub amplitude: Complex64,
    pub cost: ContractionCost,
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ContractionExecutionError {
    message: String,
    environment: bool,
}

impl ContractionExecutionError {
    #[must_use]
    pub const fn is_environment_error(&self) -> bool {
        self.environment
    }

    fn program(error: impl fmt::Display) -> Self {
        Self {
            message: error.to_string(),
            environment: false,
        }
    }

    fn environment(error: impl fmt::Display) -> Self {
        Self {
            message: error.to_string(),
            environment: true,
        }
    }
}

impl From<AvailabilityError> for ContractionExecutionError {
    fn from(error: AvailabilityError) -> Self {
        Self::environment(error)
    }
}

type Reports = ContractionReports<CuTensorNetPlanningReport, CuTensorNetResourceReport>;
type RunError = AmplitudeContractionError<
    SimulationError,
    SimulationError,
    CuTensorNetPlanningReport,
    CuTensorNetResourceReport,
>;

/// Use all available CPU threads for search. The seed fixes the search's
/// random choices, but threads vary by host: record them with the results.
/// The SDK's 500 reconfiguration
/// iterations refine local sub-orderings; zero suits tiny correctness fixtures,
/// not large circuits. Keep rank simplification enabled because rank-one
/// boundaries dominate fixed-outcome networks and need not inflate the search.
///
/// TODO(slicing): the adapter forces unsliced plans. Enabling slicing requires
/// sliced indices in `tensornet::ContractionPlan`, adapter import/export
/// pass-through, and a slice count in Cost.
fn optimizer_settings(
    settings: ContractionSettings,
    threads: NonZeroUsize,
) -> Result<CuTensorNetContractionOptimizerSettings, ContractionExecutionError> {
    let hyper_samples = i32::try_from(settings.hyper_samples)
        .ok()
        .filter(|&samples| samples > 0)
        .ok_or_else(|| ContractionExecutionError::program("hyper_samples must be in [1, 2^31)"))?;
    let seed = i32::try_from(settings.seed)
        .map_err(|_| ContractionExecutionError::program("seed must be in [0, 2^31)"))?;
    Ok(CuTensorNetContractionOptimizerSettings {
        hyper_samples,
        threads: i32::try_from(threads.get()).unwrap_or(i32::MAX),
        seed,
        reconfiguration_iterations: 500,
        disable_rank_simplification: false,
    })
}

fn is_program_error(error: &SimulationError) -> bool {
    matches!(
        error,
        SimulationError::InvalidCircuit { .. }
            | SimulationError::InvalidContractionConfiguration { .. }
            | SimulationError::InvalidContractionPlan { .. }
            | SimulationError::UnsupportedContraction { .. }
            | SimulationError::WorkspaceLimitExceeded { .. }
            | SimulationError::ResourceSizeOverflow { .. }
    )
}

fn classify(error: RunError) -> ContractionExecutionError {
    let program = match &error {
        RunError::OpenNetwork { .. } | RunError::Query(_) => true,
        RunError::Planning(error) => is_program_error(error),
        RunError::Preparation { failure, .. } => {
            failure.cleanup.is_none() && is_program_error(&failure.error)
        }
        RunError::Execution { error, cleanup, .. } => {
            cleanup.is_none()
                && match error {
                    ExecutionFailure::Backend(error) => is_program_error(error),
                    ExecutionFailure::NotScalar { .. } => true,
                }
        }
        RunError::Cleanup { .. } => false,
    };
    if program {
        ContractionExecutionError::program(error)
    } else {
        ContractionExecutionError::environment(error)
    }
}

fn cost_or_limit(
    result: Result<Reports, RunError>,
) -> Result<ContractionCost, ContractionExecutionError> {
    match result {
        Ok(reports) => Ok(ContractionCost::from_reports(
            reports.planning.as_ref(),
            reports.resources.as_ref(),
        )),
        Err(RunError::Preparation { planning, failure })
            if failure.cleanup.is_none()
                && matches!(
                    failure.error,
                    SimulationError::WorkspaceLimitExceeded { .. }
                ) =>
        {
            Ok(ContractionCost::from_reports(
                planning.as_ref(),
                failure.partial.as_ref(),
            ))
        }
        Err(error) => Err(classify(error)),
    }
}

/// Rejects invalid settings and open networks before device discovery.
fn preflight(
    network: &CircuitTensorNetwork,
    settings: ContractionSettings,
) -> Result<CuTensorNetContractionOptimizerSettings, ContractionExecutionError> {
    let settings = optimizer_settings(
        settings,
        std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN),
    )?;
    closed_amplitude_query(network).map_err(classify)?;
    Ok(settings)
}

/// Plans and prepares a closed network, without registering inputs or
/// executing a contraction. An over-budget plan still reports its minimum
/// workspace requirement.
pub fn closed_amplitude_cost(
    network: &CircuitTensorNetwork,
    settings: ContractionSettings,
) -> Result<ContractionCost, ContractionExecutionError> {
    let settings = preflight(network, settings)?;
    execute_cost(network, settings)
}

/// Contracts a closed network to its scalar amplitude.
pub fn contract_closed_amplitude(
    network: &CircuitTensorNetwork,
    settings: ContractionSettings,
) -> Result<ClosedAmplitude, ContractionExecutionError> {
    let settings = preflight(network, settings)?;
    execute_amplitude(network, settings)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use native::{execute_amplitude, execute_cost};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod native {
    use super::*;
    use crate::{
        library::CuTensorNetApi,
        simulation::{
            contraction::{adapter::CuTensorNetContractionOptimizer, execution::TensorInput},
            resources::SessionResources,
        },
    };
    use qdk_simulators::execution::{
        ContractionOptimizer, ExecutionLimits, PlannedContraction, PlanningConstraints,
        contract_amplitude, contraction_cost,
    };
    use std::sync::Arc;
    use tensornet::ContractionQuery;

    fn plan(
        session: &mut SessionResources<CuTensorNetApi>,
        query: &ContractionQuery<'_>,
        settings: CuTensorNetContractionOptimizerSettings,
    ) -> Result<PlannedContraction<CuTensorNetPlanningReport>, SimulationError> {
        let (plan, report) = CuTensorNetContractionOptimizer::new(session).optimize(
            query,
            PlanningConstraints::default(),
            settings,
        )?;
        // Preparation enforces the same automatic (half-free-memory) budget
        // used in search, so an oversized plan reports its need before allocating.
        let limits = ExecutionLimits {
            device_scratch_bytes: Some(usize::try_from(report.effective_workspace_bytes).map_err(
                |_| SimulationError::ResourceSizeOverflow {
                    resource: "optimizer workspace budget",
                },
            )?),
            host_scratch_bytes: None,
        };
        Ok(PlannedContraction {
            plan,
            report,
            limits,
        })
    }

    fn tensor_input<'a>(dimensions: &'a [usize], values: &'a [Complex64]) -> TensorInput<'a> {
        TensorInput { dimensions, values }
    }

    pub(super) fn execute_cost(
        network: &CircuitTensorNetwork,
        settings: CuTensorNetContractionOptimizerSettings,
    ) -> Result<ContractionCost, ContractionExecutionError> {
        with_session(|session| {
            cost_or_limit(contraction_cost(session, network, |session, query| {
                plan(session, query, settings)
            }))
        })
    }

    pub(super) fn execute_amplitude(
        network: &CircuitTensorNetwork,
        settings: CuTensorNetContractionOptimizerSettings,
    ) -> Result<ClosedAmplitude, ContractionExecutionError> {
        with_session(|session| {
            let result = contract_amplitude(
                session,
                network,
                |session, query| plan(session, query, settings),
                tensor_input,
            )
            .map_err(classify)?;
            Ok(ClosedAmplitude {
                amplitude: result.amplitude,
                cost: ContractionCost::from_reports(
                    result.reports.planning.as_ref(),
                    result.reports.resources.as_ref(),
                ),
            })
        })
    }

    fn with_session<T>(
        run: impl FnOnce(&mut SessionResources<CuTensorNetApi>) -> Result<T, ContractionExecutionError>,
    ) -> Result<T, ContractionExecutionError> {
        let availability = discover()?;
        let mut session = SessionResources::new(Arc::clone(&availability.libraries), 0)
            .map_err(ContractionExecutionError::environment)?;
        let result = run(&mut session);
        combine_session_cleanup(result, session.close())
    }
}

fn combine_session_cleanup<T>(
    result: Result<T, ContractionExecutionError>,
    cleanup: Result<(), SimulationError>,
) -> Result<T, ContractionExecutionError> {
    match (result, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(ContractionExecutionError::environment(error)),
        (Err(error), Err(cleanup)) => Err(ContractionExecutionError::environment(format_args!(
            "execution failed ({error}); cleanup also failed ({cleanup})"
        ))),
    }
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn execute_cost(
    _network: &CircuitTensorNetwork,
    _settings: CuTensorNetContractionOptimizerSettings,
) -> Result<ContractionCost, ContractionExecutionError> {
    Err(discover()
        .expect_err("cuTensorNet is unsupported on this target")
        .into())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn execute_amplitude(
    _network: &CircuitTensorNetwork,
    _settings: CuTensorNetContractionOptimizerSettings,
) -> Result<ClosedAmplitude, ContractionExecutionError> {
    Err(discover()
        .expect_err("cuTensorNet is unsupported on this target")
        .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::contraction::adapter::WorkspaceBudgetSource;
    use qdk_simulators::execution::{
        CostEstimate, EstimateKind, FixedOutcomeCircuit, FixedOutcomeOperation, PlanningReport,
        PreparationFailure, ResourceReport, UnitaryOperation,
    };

    const SETTINGS: ContractionSettings = ContractionSettings {
        hyper_samples: 8,
        seed: 17,
    };

    fn reports() -> Reports {
        Reports {
            planning: CuTensorNetPlanningReport {
                planning: PlanningReport {
                    estimates: vec![
                        CostEstimate {
                            provider: "test",
                            quantity: EstimateKind::LargestIntermediateElements,
                            value: 1024.0,
                        },
                        CostEstimate {
                            provider: "test",
                            quantity: EstimateKind::FlopCount,
                            value: 1234.0,
                        },
                    ],
                    ..PlanningReport::default()
                },
                effective_workspace_bytes: 512,
                workspace_source: WorkspaceBudgetSource::Automatic,
            },
            resources: CuTensorNetResourceReport {
                common: ResourceReport {
                    device_scratch_minimum: Some(4096),
                    ..ResourceReport::default()
                },
                ..CuTensorNetResourceReport::default()
            },
        }
    }

    fn preparation(error: SimulationError, cleanup: Option<SimulationError>) -> RunError {
        let reports = reports();
        RunError::Preparation {
            planning: reports.planning,
            failure: PreparationFailure {
                partial: reports.resources,
                error,
                cleanup,
            },
        }
    }

    fn over_budget() -> SimulationError {
        SimulationError::WorkspaceLimitExceeded {
            required: 4096,
            maximum: 512,
        }
    }

    fn native_error() -> SimulationError {
        SimulationError::NativeCallFailed {
            component: "test",
            operation: "test",
            status: 1,
            message: "native failure".into(),
        }
    }

    #[test]
    fn settings_map_search_effort_and_bound_native_integers() {
        let settings = optimizer_settings(
            ContractionSettings {
                hyper_samples: 12,
                seed: 123,
            },
            NonZeroUsize::new(7).expect("nonzero"),
        )
        .expect("valid settings");
        assert_eq!(settings.hyper_samples, 12);
        assert_eq!(settings.seed, 123);
        assert_eq!(settings.threads, 7);
        assert_eq!(settings.reconfiguration_iterations, 500);
        assert!(!settings.disable_rank_simplification);
        assert_eq!(
            optimizer_settings(SETTINGS, NonZeroUsize::MAX)
                .expect("clamped threads")
                .threads,
            i32::MAX,
        );
        for settings in [
            ContractionSettings {
                hyper_samples: 0,
                ..SETTINGS
            },
            ContractionSettings {
                hyper_samples: 1 << 31,
                ..SETTINGS
            },
            ContractionSettings {
                seed: 1 << 31,
                ..SETTINGS
            },
        ] {
            assert!(
                !optimizer_settings(settings, NonZeroUsize::MIN)
                    .expect_err("invalid settings")
                    .is_environment_error()
            );
        }
        let boundary = optimizer_settings(
            ContractionSettings {
                hyper_samples: i32::MAX as u32,
                seed: i32::MAX as u32,
            },
            NonZeroUsize::MIN,
        )
        .expect("valid maximum settings");
        assert_eq!(boundary.hyper_samples, i32::MAX);
        assert_eq!(boundary.seed, i32::MAX);
        assert_eq!(
            optimizer_settings(
                ContractionSettings {
                    seed: 0,
                    ..SETTINGS
                },
                NonZeroUsize::MIN
            )
            .expect("zero seed is valid")
            .seed,
            0
        );
    }

    #[test]
    fn cost_reports_the_need_even_when_the_plan_exceeds_its_budget() {
        let expected = ContractionCost {
            width: Some(10.0),
            flops: Some(1234.0),
            workspace_bytes: Some(4096),
        };
        assert_eq!(cost_or_limit(Ok(reports())).expect("cost"), expected);
        assert_eq!(
            cost_or_limit(Err(preparation(over_budget(), None))).expect("partial cost"),
            expected
        );
        let error = classify(preparation(over_budget(), None));
        assert!(!error.is_environment_error());
        assert!(error.to_string().contains("4096 bytes"));
        assert!(error.to_string().contains("512-byte limit"));
    }

    #[test]
    fn other_preparation_failures_are_not_cost_results() {
        for (error, cleanup, environment) in [
            (over_budget(), Some(native_error()), true),
            (native_error(), None, true),
            (
                SimulationError::UnsupportedContraction { reason: "test" },
                None,
                false,
            ),
        ] {
            let error = cost_or_limit(Err(preparation(error, cleanup))).expect_err("failure");
            assert_eq!(error.is_environment_error(), environment);
        }
    }

    #[test]
    fn errors_keep_their_messages_and_program_or_environment_classification() {
        for (error, environment) in [
            (RunError::OpenNetwork { qubits: vec![1] }, false),
            (
                RunError::Query(tensornet::ContractionError::UnknownKeptIndex { id: 1 }),
                false,
            ),
            (RunError::Planning(over_budget()), false),
            (
                RunError::Planning(SimulationError::InvalidCircuit {
                    reason: "test".into(),
                }),
                false,
            ),
            (
                RunError::Planning(SimulationError::InvalidContractionConfiguration {
                    reason: "test",
                }),
                false,
            ),
            (
                RunError::Planning(SimulationError::InvalidContractionPlan {
                    error: tensornet::PlanError::EmptyNetwork,
                }),
                false,
            ),
            (
                RunError::Planning(SimulationError::ResourceSizeOverflow { resource: "test" }),
                false,
            ),
            (RunError::Planning(native_error()), true),
            (
                RunError::Planning(SimulationError::ExecutionAndCleanupFailed {
                    execution: Box::new(over_budget()),
                    cleanup: Box::new(native_error()),
                }),
                true,
            ),
            (
                RunError::Execution {
                    reports: reports(),
                    error: ExecutionFailure::NotScalar { elements: 2 },
                    cleanup: None,
                },
                false,
            ),
            (
                RunError::Execution {
                    reports: reports(),
                    error: ExecutionFailure::NotScalar { elements: 2 },
                    cleanup: Some(native_error()),
                },
                true,
            ),
            (
                RunError::Execution {
                    reports: reports(),
                    error: ExecutionFailure::Backend(over_budget()),
                    cleanup: None,
                },
                false,
            ),
            (
                RunError::Execution {
                    reports: reports(),
                    error: ExecutionFailure::Backend(native_error()),
                    cleanup: None,
                },
                true,
            ),
            (
                RunError::Cleanup {
                    reports: reports(),
                    error: over_budget(),
                },
                true,
            ),
        ] {
            let message = error.to_string();
            let error = classify(error);
            assert_eq!(error.to_string(), message);
            assert_eq!(error.is_environment_error(), environment, "{error}");
        }
    }

    #[test]
    fn session_cleanup_failure_overrides_program_classification_not_its_evidence() {
        let error = combine_session_cleanup::<()>(
            Err(ContractionExecutionError::program("program failed")),
            Err(native_error()),
        )
        .expect_err("execution and cleanup failed");
        assert!(error.is_environment_error());
        assert!(error.to_string().contains("program failed"));
        assert!(error.to_string().contains("native failure"));
        assert!(
            combine_session_cleanup(Ok(()), Err(native_error()))
                .expect_err("cleanup failed")
                .is_environment_error()
        );
        assert!(combine_session_cleanup(Ok(()), Ok(())).is_ok());
        assert!(
            !combine_session_cleanup::<()>(
                Err(ContractionExecutionError::program("program failed")),
                Ok(())
            )
            .expect_err("program failed")
            .is_environment_error()
        );
    }

    #[test]
    fn both_entry_points_reject_invalid_inputs_before_discovery() {
        for (operation, settings, message) in [
            (
                UnitaryOperation::Sx { target: 1 },
                SETTINGS,
                "open on qubits [1]",
            ),
            (
                UnitaryOperation::I { target: 0 },
                ContractionSettings {
                    hyper_samples: 0,
                    ..SETTINGS
                },
                "hyper_samples",
            ),
        ] {
            let circuit =
                FixedOutcomeCircuit::new(2, vec![FixedOutcomeOperation::Unitary(operation)])
                    .expect("valid circuit");
            let network =
                CircuitTensorNetwork::from_fixed_outcome_circuit(&circuit).expect("valid network");
            for error in [
                closed_amplitude_cost(&network, settings).expect_err("preflight"),
                contract_closed_amplitude(&network, settings).expect_err("preflight"),
            ] {
                assert!(!error.is_environment_error(), "{error}");
                assert!(error.to_string().contains(message), "{error}");
            }
        }
    }
}
