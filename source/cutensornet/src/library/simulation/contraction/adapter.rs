//! Shared planning and Context preparation over the real native resource owners.

use super::execution::{
    ContractionExecutionApi, CuTensorNetExecutableContraction, CuTensorNetResourceReport,
};
use super::{
    ContractionApi, ContractionResources, NativeMetadata, NativeOptimizerSettings,
    OptimizerEstimate, SessionApi, SessionResources, SimulationError, Topology,
    combine_execution_and_cleanup, invalid, native_count, unexpected,
};
use crate::simulation::memory_workspace::MemoryWorkspaceApi;
use qdk_simulators::execution::{
    ContractionContext, ContractionOptimizer, CostEstimate, EstimateKind, ExecutionLimits,
    PlanningConstraints, PlanningReport, PreparationFailure,
};
use std::collections::{BTreeMap, BTreeSet};
use tensornet::{ContractionPlan, ContractionQuery, ContractionStep, Index, Indices, Operand};

#[derive(Clone, Copy, Debug)]
pub(crate) struct CuTensorNetContractionOptimizerSettings {
    pub(crate) hyper_samples: i32,
    pub(crate) threads: i32,
    pub(crate) seed: i32,
    pub(crate) reconfiguration_iterations: i32,
    pub(crate) disable_rank_simplification: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WorkspaceBudgetSource {
    Explicit,
    Automatic,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CuTensorNetPlanningReport {
    pub(crate) planning: PlanningReport,
    pub(crate) effective_workspace_bytes: u64,
    pub(crate) workspace_source: WorkspaceBudgetSource,
}

impl AsRef<PlanningReport> for CuTensorNetPlanningReport {
    fn as_ref(&self) -> &PlanningReport {
        &self.planning
    }
}

/// Borrows a thread-confined session, but owns no native children between calls.
pub(crate) struct CuTensorNetContractionOptimizer<'session, Api: SessionApi + ContractionApi> {
    session: &'session mut SessionResources<Api>,
}

impl<'session, Api: SessionApi + ContractionApi> CuTensorNetContractionOptimizer<'session, Api> {
    pub(crate) fn new(session: &'session mut SessionResources<Api>) -> Self {
        Self { session }
    }
}

impl<Api: SessionApi + ContractionApi + MemoryWorkspaceApi> ContractionOptimizer
    for CuTensorNetContractionOptimizer<'_, Api>
{
    type Settings = CuTensorNetContractionOptimizerSettings;
    type Report = CuTensorNetPlanningReport;
    type Error = SimulationError;

    fn optimize(
        &mut self,
        query: &ContractionQuery<'_>,
        constraints: PlanningConstraints,
        settings: Self::Settings,
    ) -> Result<(ContractionPlan, Self::Report), Self::Error> {
        supported_topology(query)?;
        let (workspace_constraint, workspace_source) =
            resolve_workspace(self.session, constraints)?;
        let native_settings = NativeOptimizerSettings {
            workspace_constraint,
            hyper_samples: settings.hyper_samples,
            threads: settings.threads,
            seed: settings.seed,
            reconfiguration_iterations: settings.reconfiguration_iterations,
            disable_rank_simplification: settings.disable_rank_simplification,
            disable_slicing: true,
        };
        native_settings.attributes()?;

        let mut resources = ContractionResources::new(self.session, query)?;
        let result = (|| {
            resources.optimize(native_settings)?;
            let metadata = resources.export()?;
            let plan = from_native_metadata(query, &metadata, &resources.intermediate_modes()?)?;
            let mut estimates = Vec::with_capacity(2);
            for (native, quantity) in [
                (OptimizerEstimate::FlopCount, EstimateKind::FlopCount),
                (
                    OptimizerEstimate::LargestTensor,
                    EstimateKind::LargestIntermediateElements,
                ),
            ] {
                estimates.push(CostEstimate {
                    provider: "cuTensorNet",
                    quantity,
                    value: resources.estimate(native)?,
                });
            }
            Ok((
                plan,
                CuTensorNetPlanningReport {
                    planning: PlanningReport {
                        optimizer: "cuTensorNet",
                        search_seconds: None,
                        accepted_constraints: constraints,
                        estimates,
                    },
                    effective_workspace_bytes: workspace_constraint,
                    workspace_source,
                },
            ))
        })();
        combine_execution_and_cleanup(result, resources.close())
    }
}

fn resolve_workspace<Api: SessionApi + MemoryWorkspaceApi>(
    session: &SessionResources<Api>,
    constraints: PlanningConstraints,
) -> Result<(u64, WorkspaceBudgetSource), SimulationError> {
    match constraints.workspace_bytes {
        Some(0) => Err(invalid(
            "workspace_bytes must be positive, or None for automatic budgeting; \
             it is a path-search device-workspace budget in bytes, not an allocation \
             or a total-memory limit",
        )),
        Some(bytes) => Ok((bytes, WorkspaceBudgetSource::Explicit)),
        None => {
            session.bind_device()?;
            let (free, total) = session.api().memory_info()?;
            if free > total {
                return Err(unexpected("free device memory exceeds total device memory"));
            }
            let bytes =
                u64::try_from(free / 2).map_err(|_| SimulationError::ResourceSizeOverflow {
                    resource: "optimizer workspace constraint",
                })?;
            if bytes == 0 {
                return Err(invalid(
                    "automatic workspace budget (half of free device memory) is zero; \
                     free device memory or set a positive workspace_bytes explicitly; \
                     this path-search budget does not reserve execution memory",
                ));
            }
            Ok((bytes, WorkspaceBudgetSource::Automatic))
        }
    }
}

fn supported_topology(query: &ContractionQuery<'_>) -> Result<Topology, SimulationError> {
    Topology::new(query).map_err(|error| match error {
        SimulationError::InvalidContractionConfiguration { reason } => {
            SimulationError::UnsupportedContraction { reason }
        }
        other => other,
    })
}

fn to_native_metadata(
    query: &ContractionQuery<'_>,
    plan: &ContractionPlan,
) -> Result<NativeMetadata, SimulationError> {
    ContractionPlan::new(query, plan.steps().to_vec())
        .map_err(|error| SimulationError::InvalidContractionPlan { error })?;
    supported_topology(query)?;
    if !plan.is_pairwise() {
        return Err(SimulationError::UnsupportedContraction {
            reason: "native paths require pairwise contraction steps",
        });
    }
    let mut available: Vec<_> = (0..query.network().nodes().len())
        .map(Operand::Input)
        .collect();
    let mut path = Vec::with_capacity(plan.steps().len());
    for (step_index, step) in plan.steps().iter().enumerate() {
        let [first, second] = [step.operands()[0], step.operands()[1]].map(|operand| {
            available
                .iter()
                .position(|live| *live == operand)
                .expect("shared validation established live operands")
        });
        path.push([
            native_count(first, "path position")?,
            native_count(second, "path position")?,
        ]);
        replace_pair(&mut available, first, second, Operand::Result(step_index));
    }
    Ok(NativeMetadata {
        path,
        slicing: Vec::new(),
        num_slices: 1,
    })
}

pub(super) fn from_native_metadata(
    query: &ContractionQuery<'_>,
    metadata: &NativeMetadata,
    intermediate_modes: &[Vec<i32>],
) -> Result<ContractionPlan, SimulationError> {
    let topology = supported_topology(query)?;
    topology
        .validate(metadata)
        .map_err(|error| unexpected(error.to_string()))?;
    if !metadata.slicing.is_empty() || metadata.num_slices != 1 {
        return Err(SimulationError::UnsupportedContraction {
            reason: "portable contraction plans do not represent slicing",
        });
    }
    if intermediate_modes.len() != metadata.path.len() {
        return Err(unexpected(
            "native intermediate count does not match the path",
        ));
    }
    let mode_indices: BTreeMap<_, _> = query
        .network()
        .nodes()
        .iter()
        .flat_map(Indices::as_slice)
        .map(|&axis| {
            (
                i32::try_from(axis.id()).expect("topology checked native mode widths"),
                axis,
            )
        })
        .collect();
    let mut available: Vec<_> = query
        .network()
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, axes)| (Operand::Input(index), axes.clone()))
        .collect();
    let mut steps = Vec::with_capacity(metadata.path.len());
    for (step_index, (&[first, second], modes)) in
        metadata.path.iter().zip(intermediate_modes).enumerate()
    {
        let first = usize::try_from(first).expect("native path positions were validated");
        let second = usize::try_from(second).expect("native path positions were validated");
        let mut remaining = BTreeSet::new();
        for mode in modes {
            let axis = mode_indices
                .get(mode)
                .ok_or_else(|| unexpected("native intermediate contains an unknown mode"))?;
            if !remaining.insert(*axis) {
                return Err(unexpected("native intermediate contains a duplicate mode"));
            }
        }
        let operands = vec![available[first].0, available[second].0];
        let ordered: Vec<Index> = available[first]
            .1
            .as_slice()
            .iter()
            .chain(available[second].1.as_slice())
            .copied()
            .filter(|axis| remaining.remove(axis))
            .collect();
        if !remaining.is_empty() {
            return Err(unexpected(
                "native intermediate contains modes absent from its operands",
            ));
        }
        let result_axes = if step_index + 1 == metadata.path.len() {
            let observed: BTreeSet<_> = ordered.iter().copied().collect();
            let output: BTreeSet<_> = query.keep().as_slice().iter().copied().collect();
            if observed != output {
                return Err(unexpected(
                    "native final modes do not match the query output",
                ));
            }
            query.keep().clone()
        } else {
            Indices::new(ordered).expect("distinct axes taken from a validated query")
        };
        steps.push(ContractionStep::new(operands, result_axes.clone()));
        replace_pair(
            &mut available,
            first,
            second,
            (Operand::Result(step_index), result_axes),
        );
    }
    ContractionPlan::new(query, steps).map_err(|error| unexpected(error.to_string()))
}

fn replace_pair<T>(available: &mut Vec<T>, first: usize, second: usize, result: T) {
    available.remove(first.max(second));
    available.remove(first.min(second));
    available.push(result);
}

/// Creates and checks a fresh native owner without searching or preparing kernels.
///
/// Native metadata omits logical intermediate ordering. Keep the supplied
/// portable plan separately if its exact representation must be re-exported.
#[allow(
    clippy::result_large_err,
    reason = "preparation evidence is returned by value"
)]
pub(crate) fn import_plan<'session, Api: SessionApi + ContractionApi>(
    session: &'session mut SessionResources<Api>,
    query: &ContractionQuery<'_>,
    plan: &ContractionPlan,
) -> Result<ContractionResources<'session, Api>, PreparationFailure<SimulationError>> {
    let metadata = to_native_metadata(query, plan).map_err(|error| PreparationFailure {
        partial: CuTensorNetResourceReport::default().common,
        error,
        cleanup: None,
    })?;
    let mut resources = ContractionResources::new_for_preparation(session, query)?;
    let result = (|| {
        resources.import(&metadata)?;
        let observed = resources.export()?;
        if observed != metadata {
            return Err(unexpected(
                "native import changed the supplied path or slicing",
            ));
        }
        from_native_metadata(query, &observed, &resources.intermediate_modes()?)?;
        Ok(())
    })();
    if let Err(error) = result {
        return Err(PreparationFailure {
            partial: CuTensorNetResourceReport::default().common,
            error,
            cleanup: resources.close().err(),
        });
    }

    Ok(resources)
}

impl<Api: ContractionExecutionApi> ContractionContext for SessionResources<Api> {
    type Executable<'context>
        = CuTensorNetExecutableContraction<'context, Api>
    where
        Self: 'context;
    type Report = CuTensorNetResourceReport;
    type Error = SimulationError;

    #[allow(
        clippy::result_large_err,
        reason = "preparation evidence is returned by value"
    )]
    fn prepare(
        &mut self,
        query: &ContractionQuery<'_>,
        plan: &ContractionPlan,
        limits: ExecutionLimits,
    ) -> Result<Self::Executable<'_>, PreparationFailure<Self::Error, Self::Report>> {
        let resources = import_plan(self, query, plan).map_err(|failure| PreparationFailure {
            partial: CuTensorNetResourceReport {
                common: failure.partial,
                ..CuTensorNetResourceReport::default()
            },
            error: failure.error,
            cleanup: failure.cleanup,
        })?;
        CuTensorNetExecutableContraction::prepare(resources, plan.clone(), limits)
    }
}
