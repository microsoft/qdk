// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Backend-generic orchestration of one closed amplitude network: plan,
//! prepare, register the coefficient bank, execute, check the scalar, close.
//!
//! ```text
//!  closed network ─► planner(context, query) ─► prepare(plan, limits) ─┬─ cost:      close
//!                    (backend optimizer)        (ContractionContext)   └─ amplitude: register each buffer once
//!                                                                                    ─► execute ─► scalar A ─► close
//! ```
//!
//! Backends plug in behind [`ContractionContext`] and a planning closure; they
//! own settings, limits and error classification, not the sequence.

use std::fmt;

use num_complex::Complex64;
use tensornet::{ContractionError, ContractionPlan, ContractionQuery};

use crate::QubitID;

use super::{
    CircuitTensorNetwork, ContractionContext, EstimateKind, ExecutableContraction, ExecutionLimits,
    InputMutability, PlanningReport, PreparationFailure, ResourceReport,
};

/// What a backend planner returns: the plan, its report, and the limits
/// preparation must respect (for example, the budget the optimizer planned for).
#[derive(Debug)]
pub struct PlannedContraction<R> {
    pub plan: ContractionPlan,
    pub report: R,
    pub limits: ExecutionLimits,
}

/// The planning report and the resources discovered by preparation.
#[derive(Clone, Debug, PartialEq)]
pub struct ContractionReports<R, X> {
    pub planning: R,
    pub resources: X,
}

/// The scalar A of a closed network and the reports of its evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct ContractedAmplitude<R, X> {
    pub amplitude: Complex64,
    pub reports: ContractionReports<R, X>,
}

/// Why the executable failed after a successful preparation.
#[derive(Debug, PartialEq)]
pub enum ExecutionFailure<E> {
    Backend(E),
    /// A closed network must contract to exactly one element.
    NotScalar {
        elements: usize,
    },
}

/// A failed evaluation, with every report observed before the failure.
///
/// Execution and cleanup errors are both retained; neither replaces the other.
#[derive(Debug)]
pub enum AmplitudeContractionError<P, E, R, X> {
    /// The network has open output axes, on these qubits; checked before planning.
    OpenNetwork {
        qubits: Vec<QubitID>,
    },
    Query(ContractionError),
    Planning(P),
    Preparation {
        planning: R,
        failure: PreparationFailure<E, X>,
    },
    Execution {
        reports: ContractionReports<R, X>,
        error: ExecutionFailure<E>,
        cleanup: Option<E>,
    },
    Cleanup {
        reports: ContractionReports<R, X>,
        error: E,
    },
}

impl<P: fmt::Display, E: fmt::Display, R, X> fmt::Display
    for AmplitudeContractionError<P, E, R, X>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OpenNetwork { qubits } => write!(
                f,
                "the amplitude network is open on qubits {qubits:?}: every qubit with gates \
                 after its last measurement must be measured to evaluate a probability"
            ),
            Self::Query(error) => error.fmt(f),
            Self::Planning(error) => write!(f, "contraction planning failed: {error}"),
            Self::Preparation { failure, .. } => failure.fmt(f),
            Self::Execution { error, cleanup, .. } => {
                match error {
                    ExecutionFailure::Backend(error) => {
                        write!(f, "contraction execution failed: {error}")?;
                    }
                    ExecutionFailure::NotScalar { elements } => write!(
                        f,
                        "a closed network contracted to {elements} elements, not one"
                    )?,
                }
                if let Some(cleanup) = cleanup {
                    write!(f, "; cleanup also failed: {cleanup}")?;
                }
                Ok(())
            }
            Self::Cleanup { error, .. } => write!(f, "contraction cleanup failed: {error}"),
        }
    }
}

impl<P: fmt::Debug + fmt::Display, E: fmt::Debug + fmt::Display, R: fmt::Debug, X: fmt::Debug>
    std::error::Error for AmplitudeContractionError<P, E, R, X>
{
}

/// The preview `Cost` of a contraction, from the planning and resource reports.
///
/// `None` means the backend did not report the quantity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContractionCost {
    /// log₂ of the largest intermediate tensor's element count.
    pub width: Option<f64>,
    /// The optimizer's FLOP estimate, in its own counting convention.
    pub flops: Option<f64>,
    /// Minimum device scratch preparation needs: the need, not the allocation.
    pub workspace_bytes: Option<usize>,
}

impl ContractionCost {
    #[must_use]
    pub fn from_reports(planning: &PlanningReport, resources: &ResourceReport) -> Self {
        let estimate = |kind| {
            planning
                .estimates
                .iter()
                .find(|estimate| estimate.quantity == kind)
                .map(|estimate| estimate.value)
        };
        Self {
            width: estimate(EstimateKind::LargestIntermediateElements).map(f64::log2),
            flops: estimate(EstimateKind::FlopCount),
            workspace_bytes: resources.device_scratch_minimum,
        }
    }
}

type RunError<P, C, R> = AmplitudeContractionError<
    P,
    <C as ContractionContext>::Error,
    R,
    <C as ContractionContext>::Report,
>;

/// Plans and prepares the closed network, then closes it without registering
/// inputs or contracting: the cost of evaluating it, not its value.
///
/// # Errors
///
/// An open network (before planning), then planning, preparation or cleanup
/// failures, each with the reports observed before it.
#[allow(
    clippy::result_large_err,
    reason = "preparation failure evidence is returned by value"
)]
pub fn contraction_cost<C, P, R, PE>(
    context: &mut C,
    network: &CircuitTensorNetwork,
    planner: P,
) -> Result<ContractionReports<R, C::Report>, RunError<PE, C, R>>
where
    C: ContractionContext,
    C::Report: Clone,
    P: FnOnce(&mut C, &ContractionQuery<'_>) -> Result<PlannedContraction<R>, PE>,
{
    let query = closed_amplitude_query(network)?;
    let selected = planner(context, &query).map_err(AmplitudeContractionError::Planning)?;
    let executable = match context.prepare(&query, &selected.plan, selected.limits) {
        Ok(executable) => executable,
        Err(failure) => {
            return Err(AmplitudeContractionError::Preparation {
                planning: selected.report,
                failure,
            });
        }
    };
    let reports = ContractionReports {
        planning: selected.report,
        resources: executable.resources().clone(),
    };
    match executable.close() {
        Ok(()) => Ok(reports),
        Err(error) => Err(AmplitudeContractionError::Cleanup { reports, error }),
    }
}

/// Contracts the closed network to its scalar amplitude A.
///
/// Registers each coefficient buffer once, immutably, and selects it for every
/// node that shares it (`node_buffer_ids`). `input` converts a buffer's
/// dimensions (the node's axis extents, in order) and column-major values into
/// the backend's input view. The executable is always closed.
///
/// # Errors
///
/// An open network (before planning), then planning, preparation, execution
/// (including a non-scalar output) or cleanup failures, each with the reports
/// observed before it.
#[allow(
    clippy::result_large_err,
    reason = "preparation failure evidence is returned by value"
)]
pub fn contract_amplitude<'c, C, P, R, PE, F>(
    context: &'c mut C,
    network: &CircuitTensorNetwork,
    planner: P,
    input: F,
) -> Result<ContractedAmplitude<R, C::Report>, RunError<PE, C, R>>
where
    C: ContractionContext + 'c,
    C::Report: Clone,
    P: FnOnce(&mut C, &ContractionQuery<'_>) -> Result<PlannedContraction<R>, PE>,
    F: for<'a> Fn(
        &'a [usize],
        &'a [Complex64],
    ) -> <C::Executable<'c> as ExecutableContraction>::Input<'a>,
    <C::Executable<'c> as ExecutableContraction>::Output: AsRef<[Complex64]>,
{
    let query = closed_amplitude_query(network)?;
    let selected = planner(&mut *context, &query).map_err(AmplitudeContractionError::Planning)?;
    let mut executable: C::Executable<'c> =
        match context.prepare(&query, &selected.plan, selected.limits) {
            Ok(executable) => executable,
            Err(failure) => {
                return Err(AmplitudeContractionError::Preparation {
                    planning: selected.report,
                    failure,
                });
            }
        };
    let result = register_and_execute(&mut executable, network, &input);
    let reports = ContractionReports {
        planning: selected.report,
        resources: executable.resources().clone(),
    };
    let cleanup = executable.close();
    match (result, cleanup) {
        (Ok(amplitude), Ok(())) => Ok(ContractedAmplitude { amplitude, reports }),
        (Ok(_), Err(error)) => Err(AmplitudeContractionError::Cleanup { reports, error }),
        (Err(error), cleanup) => Err(AmplitudeContractionError::Execution {
            reports,
            error,
            cleanup: cleanup.err(),
        }),
    }
}

/// The scalar query of a closed network: the check [`contract_amplitude`] and
/// [`contraction_cost`] apply before planning. Backends may call it earlier,
/// for example to reject an open network before acquiring a device.
///
/// # Errors
///
/// [`AmplitudeContractionError::OpenNetwork`] naming the open qubits, or
/// [`AmplitudeContractionError::Query`] for an invalid query.
pub fn closed_amplitude_query<P, E, R, X>(
    network: &CircuitTensorNetwork,
) -> Result<ContractionQuery<'_>, AmplitudeContractionError<P, E, R, X>> {
    if !network.output_qubits().is_empty() {
        return Err(AmplitudeContractionError::OpenNetwork {
            qubits: network.output_qubits().to_vec(),
        });
    }
    network.query().map_err(AmplitudeContractionError::Query)
}

fn register_and_execute<X, F>(
    executable: &mut X,
    network: &CircuitTensorNetwork,
    input: &F,
) -> Result<Complex64, ExecutionFailure<X::Error>>
where
    X: ExecutableContraction,
    F: for<'a> Fn(&'a [usize], &'a [Complex64]) -> X::Input<'a>,
    X::Output: AsRef<[Complex64]>,
{
    let nodes = network.network().nodes();
    let mut ids: Vec<Option<X::InputId>> = vec![None; network.buffers().len()];
    for (node, &buffer) in network.node_buffer_ids().iter().enumerate() {
        if ids[buffer].is_some() {
            continue;
        }
        let dimensions: Vec<usize> = nodes[node]
            .as_slice()
            .iter()
            .map(|axis| axis.dim())
            .collect();
        let id = executable
            .register_input(
                input(&dimensions, &network.buffers()[buffer]),
                InputMutability::Immutable,
            )
            .map_err(ExecutionFailure::Backend)?;
        ids[buffer] = Some(id);
    }
    let selection: Vec<X::InputId> = network
        .node_buffer_ids()
        .iter()
        .map(|&buffer| {
            ids[buffer]
                .clone()
                .expect("every used buffer was registered")
        })
        .collect();
    let output = executable
        .execute(&selection)
        .map_err(ExecutionFailure::Backend)?;
    match output.as_ref() {
        [amplitude] => Ok(*amplitude),
        values => Err(ExecutionFailure::NotScalar {
            elements: values.len(),
        }),
    }
}
