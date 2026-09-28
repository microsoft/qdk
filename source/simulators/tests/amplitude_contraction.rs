// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The shared amplitude orchestration, through its public functions and fake
//! backends that record every call.

use std::{cell::RefCell, fmt, rc::Rc};

use num_complex::Complex64;
use qdk_simulators::{
    MeasurementResult,
    execution::{
        AmplitudeContractionError, CircuitTensorNetwork, ContractionContext, ContractionCost,
        ContractionReports, CostEstimate, EstimateKind, ExecutableContraction, ExecutionFailure,
        ExecutionLimits, FixedOutcomeCircuit, FixedOutcomeOperation, InputMutability,
        PlannedContraction, PlanningReport, PreparationFailure, ResourceReport, UnitaryOperation,
        contract_amplitude, contraction_cost,
    },
};
use tensornet::{ContractionPlan, ContractionQuery, ContractionStep, Operand};

#[derive(Debug, PartialEq)]
struct TestError(&'static str);

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Plan,
    Prepare(ExecutionLimits),
    Register {
        dimensions: Vec<usize>,
        values: Vec<Complex64>,
        mutability: InputMutability,
    },
    Execute(Vec<usize>),
    Close,
}

/// Scripted backend behaviour; every call is appended to `log`.
#[derive(Default)]
struct Context {
    log: Rc<RefCell<Vec<Event>>>,
    fail_prepare: bool,
    fail_register: bool,
    output: Option<Result<Vec<Complex64>, TestError>>,
    fail_close: bool,
}

struct Executable<'c> {
    context: &'c mut Context,
    report: ResourceReport,
    registered: usize,
}

struct Input<'a> {
    dimensions: &'a [usize],
    values: &'a [Complex64],
}

impl ContractionContext for Context {
    type Executable<'c> = Executable<'c>;
    type Report = ResourceReport;
    type Error = TestError;

    #[allow(
        clippy::result_large_err,
        reason = "the contract returns evidence by value"
    )]
    fn prepare(
        &mut self,
        _query: &ContractionQuery<'_>,
        _plan: &ContractionPlan,
        limits: ExecutionLimits,
    ) -> Result<Executable<'_>, PreparationFailure<TestError>> {
        self.log.borrow_mut().push(Event::Prepare(limits));
        let report = ResourceReport {
            device_scratch_minimum: Some(4096),
            ..ResourceReport::default()
        };
        if self.fail_prepare {
            return Err(PreparationFailure {
                partial: report,
                error: TestError("over budget"),
                cleanup: None,
            });
        }
        Ok(Executable {
            context: self,
            report,
            registered: 0,
        })
    }
}

impl ExecutableContraction for Executable<'_> {
    type Input<'a> = Input<'a>;
    type InputId = usize;
    type Report = ResourceReport;
    type Output = Vec<Complex64>;
    type Error = TestError;

    fn register_input(
        &mut self,
        input: Input<'_>,
        mutability: InputMutability,
    ) -> Result<usize, TestError> {
        self.context.log.borrow_mut().push(Event::Register {
            dimensions: input.dimensions.to_vec(),
            values: input.values.to_vec(),
            mutability,
        });
        if self.context.fail_register {
            return Err(TestError("register"));
        }
        self.registered += 1;
        Ok(self.registered - 1)
    }

    fn replace_input(&mut self, _id: usize, _input: Input<'_>) -> Result<(), TestError> {
        unreachable!("immutable inputs are never replaced")
    }

    fn execute(&mut self, inputs: &[usize]) -> Result<Vec<Complex64>, TestError> {
        self.context
            .log
            .borrow_mut()
            .push(Event::Execute(inputs.to_vec()));
        self.context
            .output
            .take()
            .unwrap_or_else(|| Ok(vec![Complex64::new(0.5, 0.5)]))
    }

    fn resources(&self) -> &ResourceReport {
        &self.report
    }

    fn close(self) -> Result<(), TestError> {
        self.context.log.borrow_mut().push(Event::Close);
        if self.context.fail_close {
            Err(TestError("close"))
        } else {
            Ok(())
        }
    }
}

fn input<'a>(dimensions: &'a [usize], values: &'a [Complex64]) -> Input<'a> {
    Input { dimensions, values }
}

const LIMITS: ExecutionLimits = ExecutionLimits {
    device_scratch_bytes: Some(1 << 20),
    host_scratch_bytes: None,
};

fn planning_report() -> PlanningReport {
    PlanningReport {
        optimizer: "fake",
        estimates: vec![
            CostEstimate {
                provider: "fake",
                quantity: EstimateKind::FlopCount,
                value: 96.0,
            },
            CostEstimate {
                provider: "fake",
                quantity: EstimateKind::LargestIntermediateElements,
                value: 1024.0,
            },
        ],
        ..PlanningReport::default()
    }
}

/// Plans left to right, and records the call in the context's log.
fn planner(
    context: &mut Context,
    query: &ContractionQuery<'_>,
) -> Result<PlannedContraction<PlanningReport>, TestError> {
    context.log.borrow_mut().push(Event::Plan);
    let count = query.network().nodes().len();
    let mut steps = Vec::new();
    let mut previous = Operand::Input(0);
    for node in 1..count {
        let axes = if node + 1 == count {
            query.keep().clone()
        } else {
            let mut remaining: Vec<_> = query.network().nodes()[node + 1..]
                .iter()
                .flat_map(|axes| axes.as_slice().iter().copied())
                .collect();
            remaining.sort_unstable();
            remaining.dedup();
            let seen: Vec<_> = query.network().nodes()[..=node]
                .iter()
                .flat_map(|axes| axes.as_slice().iter().copied())
                .filter(|axis| remaining.contains(axis))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            tensornet::Indices::new(seen).expect("distinct axes")
        };
        steps.push(ContractionStep::new(
            vec![previous, Operand::Input(node)],
            axes,
        ));
        previous = Operand::Result(node - 1);
    }
    Ok(PlannedContraction {
        plan: ContractionPlan::new(query, steps).expect("left-to-right plan"),
        report: planning_report(),
        limits: LIMITS,
    })
}

fn failing_planner(
    context: &mut Context,
    _query: &ContractionQuery<'_>,
) -> Result<PlannedContraction<PlanningReport>, TestError> {
    context.log.borrow_mut().push(Event::Plan);
    Err(TestError("planning"))
}

fn measure(qubit: usize, result_id: usize, outcome: MeasurementResult) -> FixedOutcomeOperation {
    FixedOutcomeOperation::Measure {
        qubit,
        result_id,
        outcome,
        reset: false,
    }
}

/// Sx·Cz·Sx on two qubits, both measured: a closed network in which the
/// |0⟩ starts and ⟨0| caps share one buffer and both Sx gates another.
fn closed_network() -> CircuitTensorNetwork {
    let circuit = FixedOutcomeCircuit::new(
        2,
        vec![
            FixedOutcomeOperation::Unitary(UnitaryOperation::Sx { target: 0 }),
            FixedOutcomeOperation::Unitary(UnitaryOperation::Cz {
                control: 0,
                target: 1,
            }),
            FixedOutcomeOperation::Unitary(UnitaryOperation::Sx { target: 0 }),
            measure(0, 0, MeasurementResult::One),
            measure(1, 1, MeasurementResult::Zero),
        ],
    )
    .expect("valid circuit");
    CircuitTensorNetwork::from_fixed_outcome_circuit(&circuit).expect("closed network")
}

fn open_network() -> CircuitTensorNetwork {
    let circuit = FixedOutcomeCircuit::new(
        3,
        vec![
            FixedOutcomeOperation::Unitary(UnitaryOperation::Sx { target: 0 }),
            FixedOutcomeOperation::Unitary(UnitaryOperation::Sx { target: 2 }),
            measure(0, 0, MeasurementResult::Zero),
        ],
    )
    .expect("valid circuit");
    CircuitTensorNetwork::from_fixed_outcome_circuit(&circuit).expect("open network")
}

fn events(context: &Context) -> Vec<Event> {
    context.log.borrow().clone()
}

#[test]
fn amplitude_registers_each_buffer_once_and_selects_it_for_every_sharing_node() {
    let network = closed_network();
    assert!(network.buffers().len() < network.node_buffer_ids().len());
    let mut context = Context::default();
    let result = contract_amplitude(&mut context, &network, planner, input).expect("amplitude");
    assert_eq!(result.amplitude, Complex64::new(0.5, 0.5));
    assert_eq!(result.reports.planning, planning_report());
    assert_eq!(result.reports.resources.device_scratch_minimum, Some(4096));

    let log = events(&context);
    assert_eq!(log[..2], [Event::Plan, Event::Prepare(LIMITS)]);
    assert_eq!(log.last(), Some(&Event::Close));
    let registered: Vec<_> = log
        .iter()
        .filter_map(|event| match event {
            Event::Register {
                dimensions,
                values,
                mutability,
            } => Some((dimensions.clone(), values.clone(), *mutability)),
            _ => None,
        })
        .collect();
    assert_eq!(registered.len(), network.buffers().len());
    let Some(Event::Execute(selection)) = log.get(log.len() - 2) else {
        panic!("execute before close: {log:?}");
    };
    assert_eq!(selection.len(), network.node_buffer_ids().len());
    for (node, &input_id) in selection.iter().enumerate() {
        let (dimensions, values, mutability) = &registered[input_id];
        let axes = &network.network().nodes()[node];
        let expected: Vec<_> = axes.as_slice().iter().map(|axis| axis.dim()).collect();
        assert_eq!(dimensions, &expected, "node {node}");
        assert_eq!(
            values.as_slice(),
            &*network.buffers()[network.node_buffer_ids()[node]],
            "node {node}"
        );
        assert_eq!(*mutability, InputMutability::Immutable);
    }
}

#[test]
fn cost_plans_and_prepares_but_never_registers_or_executes() {
    let network = closed_network();
    let mut context = Context::default();
    let reports = contraction_cost(&mut context, &network, planner).expect("cost");
    assert_eq!(
        events(&context),
        [Event::Plan, Event::Prepare(LIMITS), Event::Close]
    );
    assert_eq!(
        ContractionCost::from_reports(&reports.planning, &reports.resources),
        ContractionCost {
            width: Some(10.0),
            flops: Some(96.0),
            workspace_bytes: Some(4096),
        }
    );
}

#[test]
fn cost_mapping_leaves_unreported_quantities_unknown() {
    assert_eq!(
        ContractionCost::from_reports(&PlanningReport::default(), &ResourceReport::default()),
        ContractionCost {
            width: None,
            flops: None,
            workspace_bytes: None,
        }
    );
}

#[test]
fn open_network_is_rejected_with_its_qubits_before_planning() {
    let network = open_network();
    assert_eq!(network.output_qubits(), &[2]);
    let mut context = Context::default();
    let error = contract_amplitude(&mut context, &network, planner, input).expect_err("open");
    assert!(matches!(
        &error,
        AmplitudeContractionError::OpenNetwork { qubits } if qubits == &[2]
    ));
    assert!(error.to_string().contains("open on qubits [2]"));
    let error = contraction_cost(&mut context, &network, planner).expect_err("open");
    assert!(matches!(
        error,
        AmplitudeContractionError::OpenNetwork { .. }
    ));
    assert!(events(&context).is_empty());
}

#[test]
fn planning_failure_stops_before_preparation() {
    let mut context = Context::default();
    let error = contract_amplitude(&mut context, &closed_network(), failing_planner, input)
        .expect_err("planning");
    assert!(matches!(
        error,
        AmplitudeContractionError::Planning(TestError("planning"))
    ));
    assert_eq!(events(&context), [Event::Plan]);
}

#[test]
fn preparation_failure_keeps_the_planning_report_and_partial_resources() {
    for cost_only in [false, true] {
        let mut context = Context {
            fail_prepare: true,
            ..Context::default()
        };
        let network = closed_network();
        let error = if cost_only {
            contraction_cost(&mut context, &network, planner).expect_err("preparation")
        } else {
            contract_amplitude(&mut context, &network, planner, input).expect_err("preparation")
        };
        let AmplitudeContractionError::Preparation { planning, failure } = error else {
            panic!("expected a preparation failure");
        };
        assert_eq!(
            ContractionCost::from_reports(&planning, &failure.partial).workspace_bytes,
            Some(4096)
        );
        assert_eq!(failure.error, TestError("over budget"));
        assert_eq!(events(&context), [Event::Plan, Event::Prepare(LIMITS)]);
    }
}

#[test]
fn execution_and_cleanup_errors_are_both_retained_and_close_always_runs() {
    for fail_close in [false, true] {
        let mut context = Context {
            output: Some(Err(TestError("execute"))),
            fail_close,
            ..Context::default()
        };
        let error = contract_amplitude(&mut context, &closed_network(), planner, input)
            .expect_err("execution");
        let AmplitudeContractionError::Execution {
            reports,
            error,
            cleanup,
        } = &error
        else {
            panic!("expected an execution failure");
        };
        assert_eq!(*error, ExecutionFailure::Backend(TestError("execute")));
        assert_eq!(cleanup.is_some(), fail_close);
        assert_eq!(reports.resources.device_scratch_minimum, Some(4096));
        assert_eq!(events(&context).last(), Some(&Event::Close));
    }
}

#[test]
fn registration_failure_skips_execution_but_closes() {
    let mut context = Context {
        fail_register: true,
        ..Context::default()
    };
    let error = contract_amplitude(&mut context, &closed_network(), planner, input)
        .expect_err("registration");
    assert!(matches!(
        error,
        AmplitudeContractionError::Execution {
            error: ExecutionFailure::Backend(TestError("register")),
            cleanup: None,
            ..
        }
    ));
    let log = events(&context);
    assert!(!log.iter().any(|event| matches!(event, Event::Execute(_))));
    assert_eq!(log.last(), Some(&Event::Close));
}

#[test]
fn non_scalar_output_is_an_execution_failure() {
    for elements in [0, 2] {
        let mut context = Context {
            output: Some(Ok(vec![Complex64::default(); elements])),
            ..Context::default()
        };
        let error = contract_amplitude(&mut context, &closed_network(), planner, input)
            .expect_err("non-scalar");
        assert!(matches!(
            error,
            AmplitudeContractionError::Execution {
                error: ExecutionFailure::NotScalar { elements: actual },
                ..
            } if actual == elements
        ));
        assert_eq!(events(&context).last(), Some(&Event::Close));
    }
}

#[test]
fn cleanup_failure_after_success_keeps_the_reports() {
    let mut context = Context {
        fail_close: true,
        ..Context::default()
    };
    let error =
        contract_amplitude(&mut context, &closed_network(), planner, input).expect_err("cleanup");
    let AmplitudeContractionError::Cleanup { reports, error } = error else {
        panic!("expected a cleanup failure");
    };
    assert_eq!(error, TestError("close"));
    assert_eq!(
        reports,
        ContractionReports {
            planning: planning_report(),
            resources: ResourceReport {
                device_scratch_minimum: Some(4096),
                ..ResourceReport::default()
            },
        }
    );
}

#[test]
fn a_context_is_reusable_after_each_evaluation() {
    let network = closed_network();
    let mut context = Context::default();
    contraction_cost(&mut context, &network, planner).expect("cost");
    contract_amplitude(&mut context, &network, planner, input).expect("amplitude");
    let closes = events(&context)
        .iter()
        .filter(|event| **event == Event::Close)
        .count();
    assert_eq!(closes, 2);
}

#[test]
fn cost_cleanup_failure_is_not_a_successful_cost() {
    let mut context = Context {
        fail_close: true,
        ..Context::default()
    };
    let error = contraction_cost(&mut context, &closed_network(), planner).expect_err("cleanup");
    assert!(matches!(
        error,
        AmplitudeContractionError::Cleanup {
            error: TestError("close"),
            ..
        }
    ));
    assert_eq!(
        events(&context),
        [Event::Plan, Event::Prepare(LIMITS), Event::Close]
    );
}
