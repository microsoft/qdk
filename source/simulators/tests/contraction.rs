// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Portable type-contract witnesses, not a parallel numerical implementation.

use qdk_simulators::execution::{
    ContractionContext, ContractionOptimizer, CostEstimate, EstimateKind, ExecutableContraction,
    ExecutionLimits, InputMutability, PlanningConstraints, PlanningReport, PreparationFailure,
    ResourceReport,
};
use std::{error::Error, fmt};
use tensornet::{
    ContractionPlan, ContractionQuery, ContractionStep, Index, Indices, Operand, TensorNetwork,
};

#[derive(Debug, PartialEq)]
struct TestError;

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("test failure")
    }
}

impl Error for TestError {}

struct Optimizer;

struct ProviderPlanningReport {
    common: PlanningReport,
    candidates: usize,
}

impl AsRef<PlanningReport> for ProviderPlanningReport {
    fn as_ref(&self) -> &PlanningReport {
        &self.common
    }
}

impl ContractionOptimizer for Optimizer {
    type Settings = bool;
    type Report = ProviderPlanningReport;
    type Error = TestError;

    fn optimize(
        &mut self,
        query: &ContractionQuery<'_>,
        constraints: PlanningConstraints,
        honor_workspace: bool,
    ) -> Result<(ContractionPlan, Self::Report), TestError> {
        Ok((
            plan(query),
            ProviderPlanningReport {
                common: PlanningReport {
                    optimizer: "witness",
                    accepted_constraints: if honor_workspace {
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

#[derive(Debug, Default)]
struct ProviderResourceReport {
    common: ResourceReport,
    diagnostic: Option<usize>,
}

impl AsRef<ResourceReport> for ProviderResourceReport {
    fn as_ref(&self) -> &ResourceReport {
        &self.common
    }
}

#[derive(Default)]
struct Context {
    preparations: usize,
}

struct Executable<'context> {
    _context: &'context mut Context,
    report: ProviderResourceReport,
}

impl ContractionContext for Context {
    type Executable<'context> = Executable<'context>;
    type Error = TestError;
    type Report = ProviderResourceReport;

    #[allow(
        clippy::result_large_err,
        reason = "the contract returns evidence by value"
    )]
    fn prepare(
        &mut self,
        query: &ContractionQuery<'_>,
        selected: &ContractionPlan,
        _limits: ExecutionLimits,
    ) -> Result<Self::Executable<'_>, PreparationFailure<TestError, Self::Report>> {
        ContractionPlan::new(query, selected.steps().to_vec()).map_err(|_| PreparationFailure {
            partial: ProviderResourceReport::default(),
            error: TestError,
            cleanup: None,
        })?;
        self.preparations += 1;
        Ok(Executable {
            _context: self,
            report: ProviderResourceReport::default(),
        })
    }
}

impl ExecutableContraction for Executable<'_> {
    type Input<'a> = &'a [u8];
    type InputId = ();
    type Output = Vec<u8>;
    type Error = TestError;
    type Report = ProviderResourceReport;

    // These witnesses only establish associated input lifetimes and signatures.
    // Numerical and lifecycle behavior is covered through the real native owner.
    fn register_input(
        &mut self,
        _input: &[u8],
        _mutability: InputMutability,
    ) -> Result<(), TestError> {
        Err(TestError)
    }

    fn replace_input(&mut self, _id: (), _input: &[u8]) -> Result<(), TestError> {
        Err(TestError)
    }

    fn execute(&mut self, _inputs: &[()]) -> Result<Vec<u8>, TestError> {
        Err(TestError)
    }

    fn resources(&self) -> &ProviderResourceReport {
        &self.report
    }

    fn close(self) -> Result<(), TestError> {
        Ok(())
    }
}

fn network() -> TensorNetwork {
    let axes = Indices::new(vec![Index::new(0, 2).expect("axis")]).expect("axes");
    TensorNetwork::new(vec![axes.clone(), axes]).expect("network")
}

fn query(network: &TensorNetwork) -> ContractionQuery<'_> {
    ContractionQuery::new(network, Indices::new(vec![]).expect("scalar")).expect("query")
}

fn plan(query: &ContractionQuery<'_>) -> ContractionPlan {
    ContractionPlan::new(
        query,
        vec![ContractionStep::new(
            vec![Operand::Input(0), Operand::Input(1)],
            query.keep().clone(),
        )],
    )
    .expect("plan")
}

#[allow(
    clippy::result_large_err,
    reason = "the contract returns evidence by value"
)]
fn prepare_local<C: ContractionContext>(
    context: &mut C,
) -> Result<C::Executable<'_>, PreparationFailure<C::Error, C::Report>> {
    let network = network();
    let query = query(&network);
    context.prepare(&query, &plan(&query), ExecutionLimits::default())
}

#[test]
fn context_borrow_is_independent_of_local_descriptions_and_allows_sequential_reuse() {
    let mut context = Context::default();
    for _ in 0..2 {
        let prepared = prepare_local(&mut context).expect("local descriptions do not escape");
        assert_eq!(prepared.resources().as_ref(), &ResourceReport::default());
        assert_eq!(prepared.resources().diagnostic, None);
        prepared.close().expect("close");
    }
    assert_eq!(context.preparations, 2);
}

#[test]
fn independent_contexts_can_prepare_from_the_same_descriptions() {
    let mut first = Context::default();
    let mut second = Context::default();
    let network = network();
    let query = query(&network);
    let plan = plan(&query);
    let a = first
        .prepare(&query, &plan, ExecutionLimits::default())
        .expect("first");
    let b = second
        .prepare(&query, &plan, ExecutionLimits::default())
        .expect("second");
    a.close().expect("first close");
    b.close().expect("second close");
}

#[test]
fn optimizer_reports_echo_only_honored_constraints_and_outlive_descriptions() {
    for honor in [false, true] {
        let constraints = PlanningConstraints {
            workspace_bytes: Some(0),
        };
        let (selected, report) = {
            let network = network();
            Optimizer
                .optimize(&query(&network), constraints, honor)
                .expect("plan")
        };
        assert_eq!(report.candidates, 1);
        assert_eq!(
            report.as_ref().accepted_constraints.workspace_bytes,
            honor.then_some(0)
        );
        assert_eq!(report.as_ref().search_seconds, None);
        assert!(report.as_ref().estimates.is_empty());
        let network = network();
        let mut context = Context::default();
        context
            .prepare(&query(&network), &selected, ExecutionLimits::default())
            .expect("reusable plan")
            .close()
            .expect("close");
    }
}

#[test]
fn plain_reports_and_native_extensions_share_the_same_view() {
    let mut planning = PlanningReport::default();
    let estimates = vec![
        CostEstimate {
            provider: "witness",
            quantity: EstimateKind::FlopCount,
            value: 0.0,
        },
        CostEstimate {
            provider: "witness",
            quantity: EstimateKind::LargestIntermediateElements,
            value: 0.0,
        },
    ];
    planning.estimates.clone_from(&estimates);
    assert_eq!(planning.as_ref().estimates, estimates);
    let common = ResourceReport {
        selected_input_bytes: Some(0),
        selected_input_count: Some(0),
        resident_input_bytes: Some(16),
        resident_input_count: Some(1),
        output_bytes: Some(0),
        device_scratch_minimum: Some(0),
        device_scratch_recommended: Some(0),
        device_scratch_allocated: Some(0),
        host_scratch_minimum: Some(0),
        host_scratch_recommended: Some(0),
        host_scratch_allocated: Some(0),
        owned_device_bytes: Some(16),
    };
    let extended = ProviderResourceReport {
        common: common.clone(),
        diagnostic: Some(7),
    };
    assert_eq!(common.as_ref(), extended.as_ref());
    assert_eq!(extended.diagnostic, Some(7));
    assert_ne!(ResourceReport::default(), common);
    assert_eq!(ResourceReport::default().selected_input_bytes, None);
}

#[test]
fn preparation_failure_retains_provider_evidence_and_separate_causes_by_value() {
    for cleanup in [None, Some(TestError)] {
        let failure = PreparationFailure {
            partial: ProviderResourceReport {
                common: ResourceReport {
                    owned_device_bytes: Some(8),
                    ..ResourceReport::default()
                },
                diagnostic: Some(42),
            },
            error: TestError,
            cleanup,
        };
        assert_eq!(failure.partial.as_ref().owned_device_bytes, Some(8));
        assert_eq!(failure.partial.diagnostic, Some(42));
        assert!(failure.source().expect("primary source").is::<TestError>());
        assert_eq!(
            failure.to_string(),
            if failure.cleanup.is_some() {
                "contraction preparation failed: test failure; cleanup also failed: test failure"
            } else {
                "contraction preparation failed: test failure"
            }
        );
    }
    let failure: PreparationFailure<TestError> = PreparationFailure {
        partial: ResourceReport::default(),
        error: TestError,
        cleanup: None,
    };
    assert_eq!(failure.partial, ResourceReport::default());
}
