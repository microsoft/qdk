// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Portable witnesses for the shared contracts, not numerical backend tests.

use std::{cell::RefCell, error::Error, fmt, rc::Rc};

use qdk_simulators::execution::{
    ContractionExecutor, ContractionOptimizer, CostEstimate, EstimateKind, ExecutableContraction,
    ExecutionLimits, PlanningConstraints, PlanningReport, PreparationFailure, ResourceReport,
};
use tensornet::{
    ContractionPlan, ContractionQuery, ContractionStep, Index, Indices, Operand, PlanError,
    TensorNetwork,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum TestError {
    Plan(PlanError),
    UnsupportedArity { step: usize, arity: usize },
    InvalidCoefficients,
    DeviceLimit { required: usize, maximum: usize },
    HostLimit { required: usize, maximum: usize },
    Execute,
    Unusable,
    Cleanup,
}

impl fmt::Display for TestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for TestError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Settings {
    honor_workspace: bool,
}

#[derive(Debug, Eq, PartialEq)]
enum Event {
    Optimize(PlanningConstraints, Settings),
    Prepare(ExecutionLimits),
    Execute,
    Close,
}

type Events = Rc<RefCell<Vec<Event>>>;

struct ProviderReport {
    planning: PlanningReport,
    candidates: usize,
}

impl AsRef<PlanningReport> for ProviderReport {
    fn as_ref(&self) -> &PlanningReport {
        &self.planning
    }
}

struct Optimizer {
    events: Events,
}

impl ContractionOptimizer for Optimizer {
    type Settings = Settings;
    type Report = ProviderReport;
    type Error = TestError;

    fn optimize(
        &mut self,
        query: &ContractionQuery<'_>,
        constraints: PlanningConstraints,
        settings: Self::Settings,
    ) -> Result<(ContractionPlan, Self::Report), Self::Error> {
        self.events
            .borrow_mut()
            .push(Event::Optimize(constraints, settings));
        let plan = ContractionPlan::new(
            query,
            vec![ContractionStep::new(
                vec![Operand::Input(0), Operand::Input(1)],
                query.keep().clone(),
            )],
        )
        .map_err(TestError::Plan)?;
        Ok((
            plan,
            ProviderReport {
                planning: PlanningReport {
                    optimizer: "fake",
                    accepted_constraints: if settings.honor_workspace {
                        constraints
                    } else {
                        PlanningConstraints::default()
                    },
                    ..PlanningReport::default()
                },
                candidates: 1,
            },
        ))
    }
}

struct PlainReportOptimizer(Optimizer);

impl ContractionOptimizer for PlainReportOptimizer {
    type Settings = Settings;
    type Report = PlanningReport;
    type Error = TestError;

    fn optimize(
        &mut self,
        query: &ContractionQuery<'_>,
        constraints: PlanningConstraints,
        settings: Self::Settings,
    ) -> Result<(ContractionPlan, Self::Report), Self::Error> {
        self.0
            .optimize(query, constraints, settings)
            .map(|(plan, report)| (plan, report.planning))
    }
}

#[derive(Clone, Copy, Default)]
struct Faults {
    execute: bool,
    cleanup: bool,
}

struct Executor {
    events: Events,
    faults: Faults,
}

impl Executor {
    fn failure(&self, error: TestError, partial: ResourceReport) -> PreparationFailure<TestError> {
        self.events.borrow_mut().push(Event::Close);
        PreparationFailure {
            partial,
            error,
            cleanup: self.faults.cleanup.then_some(TestError::Cleanup),
        }
    }
}

impl ContractionExecutor for Executor {
    type Coefficients = Vec<Vec<u8>>;
    type Executable = Executable;
    type Error = TestError;

    #[allow(
        clippy::result_large_err,
        reason = "the shared contract deliberately returns preparation failure evidence by value"
    )]
    fn prepare(
        &mut self,
        query: &ContractionQuery<'_>,
        plan: &ContractionPlan,
        coefficients: Self::Coefficients,
        limits: ExecutionLimits,
    ) -> Result<Self::Executable, PreparationFailure<Self::Error>> {
        self.events.borrow_mut().push(Event::Prepare(limits));
        ContractionPlan::new(query, plan.steps().to_vec())
            .map_err(|error| self.failure(TestError::Plan(error), ResourceReport::default()))?;
        for (step, operation) in plan.steps().iter().enumerate() {
            if operation.arity() != 2 {
                return Err(self.failure(
                    TestError::UnsupportedArity {
                        step,
                        arity: operation.arity(),
                    },
                    ResourceReport::default(),
                ));
            }
        }
        if coefficients.len() != query.network().nodes().len()
            || coefficients
                .iter()
                .zip(query.network().nodes())
                .any(|(buffer, axes)| Some(buffer.len()) != axes.element_count())
        {
            return Err(self.failure(TestError::InvalidCoefficients, ResourceReport::default()));
        }
        let mut report = ResourceReport {
            coefficient_bytes: Some(coefficients.iter().map(Vec::len).sum()),
            unique_buffers: Some(coefficients.len()),
            output_bytes: query.keep().element_count(),
            device_scratch_minimum: Some(8),
            device_scratch_recommended: Some(32),
            host_scratch_minimum: Some(4),
            host_scratch_recommended: Some(16),
            ..ResourceReport::default()
        };
        if let Some(maximum) = limits.device_scratch_bytes
            && maximum < 8
        {
            return Err(self.failure(
                TestError::DeviceLimit {
                    required: 8,
                    maximum,
                },
                report,
            ));
        }
        if let Some(maximum) = limits.host_scratch_bytes
            && maximum < 4
        {
            return Err(self.failure(
                TestError::HostLimit {
                    required: 4,
                    maximum,
                },
                report,
            ));
        }
        report.device_scratch_allocated = Some(8);
        report.host_scratch_allocated = Some(4);
        report.owned_device_bytes = report
            .coefficient_bytes
            .zip(report.output_bytes)
            .map(|(coefficients, output)| coefficients + output + 8);
        Ok(Executable {
            events: Rc::clone(&self.events),
            faults: self.faults,
            report,
            usable: true,
            // A fixed test payload, not a tensor contraction implementation.
            output: vec![7; query.keep().element_count().expect("small fixture")],
        })
    }
}

struct Executable {
    events: Events,
    faults: Faults,
    report: ResourceReport,
    usable: bool,
    output: Vec<u8>,
}

impl ExecutableContraction for Executable {
    type Output = Vec<u8>;
    type Error = TestError;

    fn execute(&mut self) -> Result<Self::Output, Self::Error> {
        if !self.usable {
            return Err(TestError::Unusable);
        }
        self.events.borrow_mut().push(Event::Execute);
        if self.faults.execute {
            self.usable = false;
            return Err(TestError::Execute);
        }
        Ok(self.output.clone())
    }

    fn resources(&self) -> &ResourceReport {
        &self.report
    }

    fn close(self) -> Result<(), Self::Error> {
        self.events.borrow_mut().push(Event::Close);
        if self.faults.cleanup {
            Err(TestError::Cleanup)
        } else {
            Ok(())
        }
    }
}

fn axes(values: &[(u32, usize)]) -> Indices {
    Indices::new(
        values
            .iter()
            .map(|&(id, dimension)| Index::new(id, dimension).expect("nonzero dimension"))
            .collect(),
    )
    .expect("consistent axes")
}

fn network() -> TensorNetwork {
    TensorNetwork::new(vec![axes(&[(0, 2), (1, 3)]), axes(&[(1, 3), (2, 4)])])
        .expect("valid matrix product")
}

fn query(network: &TensorNetwork) -> ContractionQuery<'_> {
    ContractionQuery::new(network, axes(&[(0, 2), (2, 4)])).expect("valid output")
}

fn supplied_plan(query: &ContractionQuery<'_>) -> ContractionPlan {
    ContractionPlan::new(
        query,
        vec![ContractionStep::new(
            vec![Operand::Input(0), Operand::Input(1)],
            query.keep().clone(),
        )],
    )
    .expect("valid supplied plan")
}

fn coefficients() -> Vec<Vec<u8>> {
    vec![vec![1; 6], vec![2; 12]]
}

fn executor(events: &Events, faults: Faults) -> Executor {
    Executor {
        events: Rc::clone(events),
        faults,
    }
}

type ExecutionOutcomes<Output, E> = (Result<Output, E>, Result<(), E>);

fn execute_and_close<P: ExecutableContraction>(
    mut prepared: P,
) -> ExecutionOutcomes<P::Output, P::Error> {
    let execution = prepared.execute();
    let cleanup = prepared.close();
    (execution, cleanup)
}

#[allow(
    clippy::result_large_err,
    reason = "the shared contract deliberately returns preparation failure evidence by value"
)]
fn prepare_supplied<E: ContractionExecutor>(
    executor: &mut E,
    query: &ContractionQuery<'_>,
    plan: &ContractionPlan,
    coefficients: E::Coefficients,
) -> Result<E::Executable, PreparationFailure<E::Error>> {
    executor.prepare(query, plan, coefficients, ExecutionLimits::default())
}

fn observed_planning_report<O: ContractionOptimizer>(
    optimizer: &mut O,
    query: &ContractionQuery<'_>,
    constraints: PlanningConstraints,
    settings: O::Settings,
) -> Result<(ContractionPlan, PlanningReport), O::Error> {
    let (plan, report) = optimizer.optimize(query, constraints, settings)?;
    Ok((plan, report.as_ref().clone()))
}

#[test]
fn optimized_plan_outlives_optimizer_and_query_and_keeps_provider_diagnostics() {
    let events = Events::default();
    let constraints = PlanningConstraints {
        workspace_bytes: Some(128),
    };
    let settings = Settings {
        honor_workspace: true,
    };
    let (plan, report) = {
        let network = network();
        let mut optimizer = Optimizer {
            events: Rc::clone(&events),
        };
        optimizer
            .optimize(&query(&network), constraints, settings)
            .expect("optimized plan")
    };
    assert_eq!(report.candidates, 1);
    let planning: &PlanningReport = report.as_ref();
    assert_eq!(planning.optimizer, "fake");
    assert_eq!(planning.accepted_constraints, constraints);
    assert_eq!(planning.search_seconds, None);
    assert!(planning.estimates.is_empty());

    let prepared = {
        let network = network();
        let mut executor = executor(&events, Faults::default());
        prepare_supplied(&mut executor, &query(&network), &plan, coefficients())
            .expect("fresh query and owner")
    };
    drop(plan);
    let (output, cleanup) = execute_and_close(prepared);
    assert_eq!(output, Ok(vec![7; 8]));
    assert_eq!(cleanup, Ok(()));
    assert_eq!(
        *events.borrow(),
        [
            Event::Optimize(constraints, settings),
            Event::Prepare(ExecutionLimits::default()),
            Event::Execute,
            Event::Close,
        ]
    );
}

#[test]
fn supplied_plan_needs_no_optimizer_or_planning_report() {
    let events = Events::default();
    let prepared = {
        let network = network();
        let query = query(&network);
        let plan = supplied_plan(&query);
        prepare_supplied(
            &mut executor(&events, Faults::default()),
            &query,
            &plan,
            coefficients(),
        )
        .expect("supplied plan")
    };
    let resources = prepared.resources();
    assert_eq!(resources.coefficient_bytes, Some(18));
    assert_eq!(resources.unique_buffers, Some(2));
    assert_eq!(resources.output_bytes, Some(8));
    assert_eq!(resources.device_scratch_minimum, Some(8));
    assert_eq!(resources.device_scratch_recommended, Some(32));
    assert_eq!(resources.device_scratch_allocated, Some(8));
    assert_eq!(resources.host_scratch_minimum, Some(4));
    assert_eq!(resources.host_scratch_recommended, Some(16));
    assert_eq!(resources.host_scratch_allocated, Some(4));
    assert_eq!(resources.owned_device_bytes, Some(34));
    let (output, cleanup) = execute_and_close(prepared);
    assert_eq!(output, Ok(vec![7; 8]));
    assert_eq!(cleanup, Ok(()));
    assert_eq!(
        *events.borrow(),
        [
            Event::Prepare(ExecutionLimits::default()),
            Event::Execute,
            Event::Close,
        ]
    );
}

#[test]
fn plain_reports_expose_honored_and_unhonored_constraints_through_the_same_contract() {
    let network = network();
    let constraints = PlanningConstraints {
        workspace_bytes: Some(0),
    };
    for honor_workspace in [false, true] {
        let mut optimizer = PlainReportOptimizer(Optimizer {
            events: Events::default(),
        });
        let (_, report) = observed_planning_report(
            &mut optimizer,
            &query(&network),
            constraints,
            Settings { honor_workspace },
        )
        .expect("plain report");
        assert_eq!(
            report.accepted_constraints.workspace_bytes,
            honor_workspace.then_some(0)
        );
    }
}

#[test]
fn missing_observations_are_distinct_from_reported_zero() {
    let unknown = ResourceReport::default();
    assert_eq!(
        unknown,
        ResourceReport {
            coefficient_bytes: None,
            unique_buffers: None,
            output_bytes: None,
            device_scratch_minimum: None,
            device_scratch_recommended: None,
            device_scratch_allocated: None,
            host_scratch_minimum: None,
            host_scratch_recommended: None,
            host_scratch_allocated: None,
            owned_device_bytes: None,
        }
    );
    let zero = ResourceReport {
        coefficient_bytes: Some(0),
        unique_buffers: Some(0),
        output_bytes: Some(0),
        device_scratch_minimum: Some(0),
        device_scratch_recommended: Some(0),
        device_scratch_allocated: Some(0),
        host_scratch_minimum: Some(0),
        host_scratch_recommended: Some(0),
        host_scratch_allocated: Some(0),
        owned_device_bytes: Some(0),
    };
    assert_ne!(unknown, zero);
    assert_eq!(zero.clone(), zero);

    let mut report = PlanningReport::default();
    assert_eq!(report.search_seconds, None);
    assert!(report.estimates.is_empty());
    assert_eq!(report.accepted_constraints.workspace_bytes, None);
    let estimates = vec![
        CostEstimate {
            provider: "first",
            quantity: EstimateKind::FlopCount,
            value: 0.0,
        },
        CostEstimate {
            provider: "second",
            quantity: EstimateKind::LargestIntermediateElements,
            value: 0.0,
        },
    ];
    report.search_seconds = Some(0.0);
    report.estimates.clone_from(&estimates);
    assert_ne!(report.search_seconds, None);
    assert_eq!(report.as_ref().estimates, estimates);
}

#[test]
fn preparation_revalidates_against_the_current_ordered_query() {
    let network = network();
    let original = query(&network);
    let plan = supplied_plan(&original);
    let reversed =
        ContractionQuery::new(&network, axes(&[(2, 4), (0, 2)])).expect("reordered query");
    let error = prepare_supplied(
        &mut executor(&Events::default(), Faults::default()),
        &reversed,
        &plan,
        coefficients(),
    )
    .err()
    .expect("wrong final output order");
    assert!(matches!(
        error.error,
        TestError::Plan(PlanError::WrongOutputAxes { .. })
    ));
    assert_eq!(error.partial, ResourceReport::default());
    assert_eq!(error.cleanup, None);
}

#[test]
fn model_valid_arity_can_be_rejected_as_an_executor_capability() {
    let network = TensorNetwork::new(vec![axes(&[]); 3]).expect("scalar inputs");
    let query = ContractionQuery::new(&network, axes(&[])).expect("scalar output");
    let plan = ContractionPlan::new(
        &query,
        vec![ContractionStep::new(
            vec![Operand::Input(0), Operand::Input(1), Operand::Input(2)],
            axes(&[]),
        )],
    )
    .expect("valid ternary step");
    let failure = prepare_supplied(
        &mut executor(&Events::default(), Faults::default()),
        &query,
        &plan,
        vec![vec![1]; 3],
    )
    .err()
    .expect("executor supports pairwise steps only");
    assert_eq!(
        failure.error,
        TestError::UnsupportedArity { step: 0, arity: 3 }
    );
    assert_eq!(failure.partial, ResourceReport::default());
}

#[test]
fn preparation_rejects_coefficient_bindings_without_fabricating_measurements() {
    let network = network();
    let query = query(&network);
    let plan = supplied_plan(&query);
    for coefficients in [vec![], vec![vec![0; 6], vec![0; 11]]] {
        let failure = prepare_supplied(
            &mut executor(&Events::default(), Faults::default()),
            &query,
            &plan,
            coefficients,
        )
        .err()
        .expect("invalid bindings");
        assert_eq!(failure.error, TestError::InvalidCoefficients);
        assert_eq!(failure.partial, ResourceReport::default());
    }
}

#[test]
fn scratch_rejection_retains_discovery_and_both_error_outcomes() {
    let network = network();
    let query = query(&network);
    let plan = supplied_plan(&query);
    for (limits, expected) in [
        (
            ExecutionLimits {
                device_scratch_bytes: Some(0),
                host_scratch_bytes: None,
            },
            TestError::DeviceLimit {
                required: 8,
                maximum: 0,
            },
        ),
        (
            ExecutionLimits {
                device_scratch_bytes: Some(7),
                host_scratch_bytes: None,
            },
            TestError::DeviceLimit {
                required: 8,
                maximum: 7,
            },
        ),
        (
            ExecutionLimits {
                device_scratch_bytes: None,
                host_scratch_bytes: Some(3),
            },
            TestError::HostLimit {
                required: 4,
                maximum: 3,
            },
        ),
    ] {
        for cleanup in [false, true] {
            let events = Events::default();
            let failure = executor(
                &events,
                Faults {
                    execute: false,
                    cleanup,
                },
            )
            .prepare(&query, &plan, coefficients(), limits)
            .err()
            .expect("insufficient scratch ceiling");
            assert_eq!(failure.error, expected);
            assert_eq!(failure.cleanup, cleanup.then_some(TestError::Cleanup));
            assert_eq!(
                failure.partial,
                ResourceReport {
                    coefficient_bytes: Some(18),
                    unique_buffers: Some(2),
                    output_bytes: Some(8),
                    device_scratch_minimum: Some(8),
                    device_scratch_recommended: Some(32),
                    host_scratch_minimum: Some(4),
                    host_scratch_recommended: Some(16),
                    ..ResourceReport::default()
                }
            );
            assert_eq!(*events.borrow(), [Event::Prepare(limits), Event::Close]);
        }
    }
}

#[test]
fn exact_minimum_and_unspecified_scratch_ceilings_allow_preparation() {
    let network = network();
    let query = query(&network);
    let plan = supplied_plan(&query);
    for limits in [
        ExecutionLimits::default(),
        ExecutionLimits {
            device_scratch_bytes: Some(8),
            host_scratch_bytes: Some(4),
        },
    ] {
        let prepared = executor(&Events::default(), Faults::default())
            .prepare(&query, &plan, coefficients(), limits)
            .expect("minimum fits, even though recommendations are larger");
        assert_eq!(prepared.resources().device_scratch_allocated, Some(8));
        assert_eq!(prepared.resources().host_scratch_allocated, Some(4));
        prepared.close().expect("cleanup");
    }
}

#[test]
fn preparation_failure_preserves_allocation_evidence_and_exposes_both_causes() {
    for cleanup in [None, Some(TestError::Cleanup)] {
        let partial = ResourceReport {
            device_scratch_allocated: Some(8),
            host_scratch_allocated: Some(0),
            owned_device_bytes: Some(8),
            ..ResourceReport::default()
        };
        let failure = PreparationFailure {
            partial: partial.clone(),
            error: TestError::InvalidCoefficients,
            cleanup: cleanup.clone(),
        };
        assert_eq!(failure.partial, partial);
        assert_eq!(
            failure
                .source()
                .and_then(|source| source.downcast_ref::<TestError>()),
            Some(&TestError::InvalidCoefficients)
        );
        assert_eq!(failure.cleanup, cleanup);
        assert_eq!(
            failure.to_string(),
            if cleanup.is_some() {
                "contraction preparation failed: InvalidCoefficients; cleanup also failed: Cleanup"
            } else {
                "contraction preparation failed: InvalidCoefficients"
            }
        );
    }
}

#[test]
fn independent_owners_and_returned_outputs_survive_other_owners_closing() {
    let events = Events::default();
    let (mut first, mut second) = {
        let network = network();
        let query = query(&network);
        let plan = supplied_plan(&query);
        let mut executor = executor(&events, Faults::default());
        (
            prepare_supplied(&mut executor, &query, &plan, coefficients()).expect("first owner"),
            prepare_supplied(&mut executor, &query, &plan, coefficients()).expect("second owner"),
        )
    };
    let mut first_output = first.execute().expect("first execution");
    let repeated_output = first.execute().expect("repeat execution");
    assert_eq!(first_output, repeated_output);
    first_output[0] = 99;
    assert_eq!(repeated_output, vec![7; 8]);
    first.close().expect("close first owner");
    let second_output = second.execute().expect("independent second owner");
    second.close().expect("close second owner");
    assert_eq!(second_output, repeated_output);
    assert_eq!(first_output[0], 99);
    assert_eq!(
        *events.borrow(),
        [
            Event::Prepare(ExecutionLimits::default()),
            Event::Prepare(ExecutionLimits::default()),
            Event::Execute,
            Event::Execute,
            Event::Close,
            Event::Execute,
            Event::Close,
        ]
    );
}

#[test]
fn failed_execution_prohibits_retry_but_preserves_evidence_and_cleanup() {
    let events = Events::default();
    let network = network();
    let query = query(&network);
    let mut prepared = prepare_supplied(
        &mut executor(
            &events,
            Faults {
                execute: true,
                cleanup: false,
            },
        ),
        &query,
        &supplied_plan(&query),
        coefficients(),
    )
    .expect("prepared");
    let resources = prepared.resources().clone();
    assert_eq!(prepared.execute(), Err(TestError::Execute));
    assert_eq!(prepared.execute(), Err(TestError::Unusable));
    assert_eq!(prepared.resources(), &resources);
    prepared.close().expect("close after failure");
    assert_eq!(
        *events.borrow(),
        [
            Event::Prepare(ExecutionLimits::default()),
            Event::Execute,
            Event::Close,
        ]
    );
}

#[test]
fn fallible_close_is_observable_after_success_or_execution_failure() {
    let network = network();
    let query = query(&network);
    for execute in [false, true] {
        let events = Events::default();
        let prepared = prepare_supplied(
            &mut executor(
                &events,
                Faults {
                    execute,
                    cleanup: true,
                },
            ),
            &query,
            &supplied_plan(&query),
            coefficients(),
        )
        .expect("prepared");
        let (output, cleanup) = execute_and_close(prepared);
        assert_eq!(
            output,
            if execute {
                Err(TestError::Execute)
            } else {
                Ok(vec![7; 8])
            }
        );
        assert_eq!(cleanup, Err(TestError::Cleanup));
        assert_eq!(
            *events.borrow(),
            [
                Event::Prepare(ExecutionLimits::default()),
                Event::Execute,
                Event::Close,
            ]
        );
    }
}
