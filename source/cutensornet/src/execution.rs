use crate::{
    AvailabilityError, discover,
    simulation::{
        Circuit, CuTensorNetMpsConsumerError, CuTensorNetSampleMatrix, Gate, ProbabilityResult,
        ProjectedCircuit, SamplingRequest, SimulationError, StateQueryResult,
        UnitaryOperationConversionError,
    },
};
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use crate::{
    library::MpsSession,
    simulation::{ExecutionPolicy, collect_sampled_shots},
};
use num_complex::Complex64;
use qdk_simulators::{
    MeasurementResult, OutputRecord, QubitID,
    execution::{
        FixedOutcomeCircuit, MeasurementRequest, PauliSum, PreparedAdaptiveProgram,
        QuantumEvolutionRegion, RegionConsumer, drive_prepared_shot,
    },
};
use rand::{RngExt, SeedableRng, rngs::StdRng};
use std::fmt;
use thiserror::Error;

const SAMPLER_HYPER_SAMPLES: i32 = 8;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct MpsExecutionError {
    message: String,
    environment: bool,
}

impl MpsExecutionError {
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

#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(
        dead_code,
        reason = "host-independent preflight prepares native inputs before reporting an unsupported target"
    )
)]
#[derive(Debug)]
struct PreparedMpsRun {
    circuit: Circuit,
    sampled_qubits: Box<[QubitID]>,
    shot_count: usize,
    sampling_request: SamplingRequest,
}

#[derive(Debug, Error)]
enum CircuitPreparationError {
    #[error(transparent)]
    Consumer(#[from] CuTensorNetMpsConsumerError),

    #[error(transparent)]
    Conversion(#[from] UnitaryOperationConversionError),

    #[error(transparent)]
    Circuit(#[from] SimulationError),
}

struct CircuitPreparationConsumer {
    circuit: Circuit,
    last_measured_qubit: Option<QubitID>,
}

impl CircuitPreparationConsumer {
    fn new(qubit_count: u32) -> Result<Self, CircuitPreparationError> {
        Ok(Self {
            circuit: Circuit::new(qubit_count)?,
            last_measured_qubit: None,
        })
    }
}

impl RegionConsumer for CircuitPreparationConsumer {
    type PreparedRegion<'region> = &'region QuantumEvolutionRegion;
    type RegionReport = ();
    type ExecutionReport = ();
    type Error = CircuitPreparationError;

    fn prepare_region<'region>(
        &mut self,
        region: &'region QuantumEvolutionRegion,
    ) -> Result<Self::PreparedRegion<'region>, Self::Error> {
        for &operation in region.operations() {
            if let Some(gate) = Gate::from_unitary_operation(operation)? {
                self.circuit.push(gate)?;
            }
        }
        Ok(region)
    }

    fn execute_region(
        &mut self,
        _region: Self::PreparedRegion<'_>,
    ) -> Result<Self::RegionReport, Self::Error> {
        if let Some(qubit) = self.last_measured_qubit {
            return Err(CuTensorNetMpsConsumerError::UnsupportedFeedforward { qubit }.into());
        }
        Ok(())
    }

    fn measure(&mut self, request: MeasurementRequest) -> Result<MeasurementResult, Self::Error> {
        self.last_measured_qubit = Some(request.qubit);
        Ok(MeasurementResult::Zero)
    }

    fn reset(&mut self, qubit: QubitID) -> Result<(), Self::Error> {
        Err(CuTensorNetMpsConsumerError::UnsupportedReset { qubit }.into())
    }

    fn finish_execution(&mut self) -> Result<Self::ExecutionReport, Self::Error> {
        Ok(())
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Executes a prepared Base-profile program through one cuTensorNet MPS session.
///
/// This is an internal cross-crate entrypoint for the Python native module.
#[doc(hidden)]
pub fn run_mps_shots(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    shots: u32,
    seed: Option<u32>,
) -> Result<Vec<Vec<OutputRecord>>, MpsExecutionError> {
    let prepared_run = prepare_mps_run(prepared_program, shots, seed)?;
    execute_mps_run(prepared_program, &prepared_run)
}

fn prepare_mps_run(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    shots: u32,
    seed: Option<u32>,
) -> Result<PreparedMpsRun, MpsExecutionError> {
    let circuit = prepare_circuit(prepared_program)?;
    let measured_qubits = prepared_program.measured_qubits().map_err(|error| {
        MpsExecutionError::program(CuTensorNetMpsConsumerError::InvalidMeasurementMetadata {
            error,
        })
    })?;
    let sampled_qubits = CuTensorNetSampleMatrix::sampled_qubits(measured_qubits);
    let shot_count =
        usize::try_from(shots).map_err(|error| MpsExecutionError::program(error.to_string()))?;
    let derived_seed = derive_sampler_seed(seed);
    let sampling_request = SamplingRequest::new(
        shot_count,
        SAMPLER_HYPER_SAMPLES,
        Some(derived_seed),
        derived_seed,
    )
    .map_err(MpsExecutionError::program)?;

    Ok(PreparedMpsRun {
        circuit,
        sampled_qubits,
        shot_count,
        sampling_request,
    })
}

/// Resolves the program's single unitary region into a circuit, rejecting
/// more regions, feedforward and reset on the host, before device discovery.
fn prepare_circuit(
    prepared_program: &PreparedAdaptiveProgram<u64>,
) -> Result<Circuit, MpsExecutionError> {
    let region_count = prepared_program.regions().len();
    if region_count > 1 {
        return Err(MpsExecutionError::program(
            CuTensorNetMpsConsumerError::UnsupportedRegionCount {
                actual: region_count,
            },
        ));
    }

    let mut consumer = CircuitPreparationConsumer::new(prepared_program.program().num_qubits)
        .map_err(MpsExecutionError::program)?;
    drive_prepared_shot(prepared_program, &mut consumer).map_err(MpsExecutionError::program)?;
    if region_count != 1 {
        return Err(MpsExecutionError::program(
            CuTensorNetMpsConsumerError::UnsupportedRegionCount {
                actual: region_count,
            },
        ));
    }
    Ok(consumer.circuit)
}

/// How a state query represents ψ.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateMethod {
    /// The gates stay a lazy network; each expectation contracts it exactly.
    Exact,
    /// ψ is a matrix product state with bonds capped at χ; `None` keeps the
    /// backend default.
    Mps { max_bond_dimension: Option<u32> },
}

/// One query on a cuTensorNet state.
///
/// A call reads exactly one state, so [`StateQuery::Cost`] always describes
/// the one MPS it computed:
///
/// ```text
/// no Probability:  ψ  = U|0…0⟩, the state before the terminal measurements
///                  Expectation, Cost
/// Probability:     ψ̃ = Πₖ Oₖ|0…0⟩ on the path the outcomes fix, Oₖ ∈ {U, |r⟩⟨b|}
///                  Probability, Cost
/// ```
///
/// Probability and Expectation in one call are rejected: they read different
/// states, and one Cost cannot describe two MPS evaluations. Allowing the mix
/// would mean evaluating each state separately and reporting one Cost per
/// state (or a Cost per query), which changes the Cost result's shape.
#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub enum StateQuery {
    /// `⟨ψ|O|ψ⟩ / ⟨ψ|ψ⟩`.
    Expectation(PauliSum),
    /// `⟨ψ̃|ψ̃⟩`, the single-pass probability of the fixed outcomes on a
    /// bond-capped MPS, never renormalized; only for [`StateMethod::Mps`],
    /// and requires outcomes.
    Probability,
    /// The resources of the one MPS this call computed; only for
    /// [`StateMethod::Mps`].
    Cost,
}

#[doc(hidden)]
#[derive(Clone, Debug, PartialEq)]
pub enum StateQueryValue {
    Expectation(Complex64),
    Probability(f64),
    Cost(MpsCost),
}

/// Resources of one MPS evaluation.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MpsCost {
    /// The largest bond the truncated state reached, at most χ.
    pub max_bond_dimension: usize,
    /// Device bytes of the realized site tensors.
    pub state_bytes: usize,
    /// The largest device scratch any step of this evaluation requested:
    /// computing the state, or any expectation evaluated with it.
    pub workspace_bytes: usize,
}

/// Evaluates `queries` through one cuTensorNet session, on the state the
/// queries read (see [`StateQuery`]).
///
/// Without a Probability query, the state is ψ, reached by a prepared
/// Base-profile program before its terminal measurements; `outcomes` is
/// ignored. With one, the state is ψ̃ on the path `outcomes` fixes
/// (`outcomes[i]` is QIR result `i`), and the program may measure mid-circuit,
/// reset and branch.
///
/// Program errors are reported before device discovery: no query; Cost or
/// Probability without an MPS; Probability mixed with Expectation or without
/// outcomes; on ψ, more than one region, feedforward, reset or a query on an
/// absent qubit; on ψ̃, outcomes that do not fix one accepted path (wrong
/// count, a record failing a selection check).
///
/// This is an internal cross-crate entrypoint for the Python native module.
#[doc(hidden)]
pub fn evaluate_state_queries(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    queries: &[StateQuery],
    outcomes: Option<&[bool]>,
    method: StateMethod,
) -> Result<Vec<StateQueryValue>, MpsExecutionError> {
    if queries.is_empty() {
        return Err(MpsExecutionError::program(
            "queries must contain at least one query",
        ));
    }
    if queries.contains(&StateQuery::Probability) {
        return evaluate_fixed_outcome_queries(prepared_program, queries, outcomes, method);
    }
    if method == StateMethod::Exact && queries.contains(&StateQuery::Cost) {
        return Err(MpsExecutionError::program(
            "Cost on a cuTensorNet state requires an MPS",
        ));
    }
    let circuit = prepare_circuit(prepared_program)?;
    let qubit_count = circuit.qubit_count() as QubitID;
    for query in queries {
        if let StateQuery::Expectation(observable) = query
            && let Some(qubit) = observable.max_qubit()
            && qubit >= qubit_count
        {
            return Err(MpsExecutionError::program(format_args!(
                "Expectation acts on qubit {qubit}, but the program has {qubit_count} qubits"
            )));
        }
    }
    let observables = queries
        .iter()
        .filter_map(|query| match query {
            StateQuery::Expectation(observable) => Some(observable.clone()),
            StateQuery::Probability | StateQuery::Cost => None,
        })
        .collect::<Vec<_>>();
    let result = evaluate_observables(&circuit, &observables, method)?;
    let mut expectations = result.expectations.into_iter();
    queries
        .iter()
        .map(|query| match query {
            StateQuery::Expectation(_) => Ok(StateQueryValue::Expectation(
                expectations
                    .next()
                    .expect("one expectation per Expectation query"),
            )),
            StateQuery::Probability => {
                unreachable!("Probability queries take the fixed-outcome route")
            }
            StateQuery::Cost => result
                .cost
                .map(StateQueryValue::Cost)
                .ok_or_else(|| MpsExecutionError::program("an MPS evaluation reported no Cost")),
        })
        .collect()
}

/// The ψ̃ route of [`evaluate_state_queries`]: Probability and Cost on the
/// fixed-outcome path, with every program error reported before discovery.
fn evaluate_fixed_outcome_queries(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    queries: &[StateQuery],
    outcomes: Option<&[bool]>,
    method: StateMethod,
) -> Result<Vec<StateQueryValue>, MpsExecutionError> {
    let StateMethod::Mps { max_bond_dimension } = method else {
        return Err(MpsExecutionError::program(
            "Probability on a cuTensorNet state requires an MPS; \
             exact Probability contracts the fixed-outcome network",
        ));
    };
    // See `StateQuery`: ψ̃ and ψ are different states, and one Cost cannot
    // describe both.
    if queries
        .iter()
        .any(|query| matches!(query, StateQuery::Expectation(_)))
    {
        return Err(MpsExecutionError::program(
            "Probability and Expectation read different MPS states and cannot share one call; \
             evaluate them in separate calls",
        ));
    }
    let outcomes =
        outcomes.ok_or_else(|| MpsExecutionError::program("Probability requires outcomes"))?;
    let circuit = FixedOutcomeCircuit::from_prepared_program(prepared_program, outcomes)
        .map_err(MpsExecutionError::program)?;
    let projected =
        ProjectedCircuit::from_fixed_outcome(&circuit).map_err(MpsExecutionError::program)?;
    // TODO(selection-normalization): this is P_pass, not P_selected, as for
    // contraction. Normalization needs acceptance marginals, not this norm.
    let result = evaluate_probability(&projected, max_bond_dimension)?;
    Ok(queries
        .iter()
        .map(|query| match query {
            StateQuery::Probability => StateQueryValue::Probability(result.probability),
            StateQuery::Cost => StateQueryValue::Cost(result.cost),
            StateQuery::Expectation(_) => unreachable!("Expectation was rejected above"),
        })
        .collect())
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn mps_policy(max_bond_dimension: Option<u32>) -> ExecutionPolicy {
    let base = ExecutionPolicy::base_qualification();
    max_bond_dimension.map_or(base, |chi| base.with_bond_cap(chi.into()))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn evaluate_probability(
    projected: &ProjectedCircuit,
    max_bond_dimension: Option<u32>,
) -> Result<ProbabilityResult, MpsExecutionError> {
    let availability = discover().map_err(MpsExecutionError::environment)?;
    let mut session = MpsSession::new(availability.libraries, mps_policy(max_bond_dimension))
        .map_err(MpsExecutionError::environment)?;
    let execution = session
        .evaluate_probability(projected)
        .map_err(MpsExecutionError::environment);
    combine_execution_and_session_cleanup(execution, session.close())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn evaluate_probability(
    _projected: &ProjectedCircuit,
    _max_bond_dimension: Option<u32>,
) -> Result<ProbabilityResult, MpsExecutionError> {
    let error = discover().expect_err("cuTensorNet discovery is unsupported on this target");
    Err(MpsExecutionError::environment(error))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn evaluate_observables(
    circuit: &Circuit,
    observables: &[PauliSum],
    method: StateMethod,
) -> Result<StateQueryResult, MpsExecutionError> {
    let (policy, mps) = match method {
        StateMethod::Exact => (ExecutionPolicy::base_qualification(), false),
        StateMethod::Mps { max_bond_dimension } => (mps_policy(max_bond_dimension), true),
    };
    let availability = discover().map_err(MpsExecutionError::environment)?;
    let mut session =
        MpsSession::new(availability.libraries, policy).map_err(MpsExecutionError::environment)?;
    let execution = session
        .evaluate_observables(circuit, observables, mps)
        .map_err(MpsExecutionError::environment);
    combine_execution_and_session_cleanup(execution, session.close())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn evaluate_observables(
    _circuit: &Circuit,
    _observables: &[PauliSum],
    _method: StateMethod,
) -> Result<StateQueryResult, MpsExecutionError> {
    let error = discover().expect_err("cuTensorNet discovery is unsupported on this target");
    Err(MpsExecutionError::environment(error))
}

fn derive_sampler_seed(seed: Option<u32>) -> i32 {
    let mut rng = if let Some(seed) = seed {
        StdRng::seed_from_u64(seed.into())
    } else {
        StdRng::from_rng(&mut rand::rng())
    };
    rng.random_range(1..=i32::MAX)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn execute_mps_run(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    prepared_run: &PreparedMpsRun,
) -> Result<Vec<Vec<OutputRecord>>, MpsExecutionError> {
    let availability = discover().map_err(MpsExecutionError::environment)?;
    let mut session = MpsSession::new(
        availability.libraries,
        ExecutionPolicy::base_qualification(),
    )
    .map_err(MpsExecutionError::environment)?;
    let execution = session
        .sample(
            &prepared_run.circuit,
            &prepared_run.sampled_qubits,
            prepared_run.sampling_request,
        )
        .map_err(MpsExecutionError::environment)
        .and_then(|samples| {
            collect_sampled_shots(prepared_program, prepared_run.shot_count, samples)
                .map_err(MpsExecutionError::program)
        });
    combine_execution_and_session_cleanup(execution, session.close())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn execute_mps_run(
    _prepared_program: &PreparedAdaptiveProgram<u64>,
    prepared_run: &PreparedMpsRun,
) -> Result<Vec<Vec<OutputRecord>>, MpsExecutionError> {
    let _ = prepared_run;
    let error = discover().expect_err("cuTensorNet discovery is unsupported on this target");
    Err(MpsExecutionError::environment(error))
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn combine_execution_and_session_cleanup<T>(
    execution: Result<T, MpsExecutionError>,
    cleanup: Result<(), SimulationError>,
) -> Result<T, MpsExecutionError> {
    match (execution, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup)) => Err(MpsExecutionError::environment(cleanup)),
        (Err(execution), Err(cleanup)) => Err(MpsExecutionError::environment(format_args!(
            "execution failed ({execution}); cleanup also failed ({cleanup})"
        ))),
    }
}

impl From<AvailabilityError> for MpsExecutionError {
    fn from(error: AvailabilityError) -> Self {
        Self::environment(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        StateMethod, StateQuery, StateQueryValue, evaluate_state_queries, prepare_mps_run,
    };
    use num_complex::Complex64;
    use qdk_simulators::{
        bytecode::{AdaptiveProgram, Block, Instruction, Op},
        execution::{PauliSum, PreparedAdaptiveProgram},
    };

    const IMMEDIATE_AUX1: u64 = 1 << 20;
    const IMMEDIATE_AUX2: u64 = 1 << 21;

    fn operation(operation_id: u64) -> Op<u64> {
        Op {
            op_id: operation_id,
            q1: 0,
            q2: 0,
            q3: 0,
            angle: 0,
        }
    }

    fn prepared_program(
        instructions: Vec<Instruction<u64>>,
        quantum_ops: Vec<Op<u64>>,
    ) -> PreparedAdaptiveProgram<u64> {
        PreparedAdaptiveProgram::new(AdaptiveProgram {
            num_qubits: 2,
            num_results: 1,
            num_registers: 0,
            entry_block: 0,
            block_table: vec![Block {
                instr_offset: 0,
                instr_count: instructions.len() as u64,
            }],
            instructions,
            function_table: Vec::new(),
            phi_entries: Vec::new(),
            switch_cases: Vec::new(),
            call_args: Vec::new(),
            constant_data: Vec::new(),
            quantum_ops,
        })
        .expect("test program should prepare")
    }

    fn gate(target: u64) -> Instruction<u64> {
        Instruction {
            opcode: 0x10 | (1 << 16) | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
            aux1: target,
            ..Instruction::default()
        }
    }

    fn measure(qubit: u64) -> Instruction<u64> {
        Instruction {
            opcode: 0x11 | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
            aux0: 1,
            aux1: qubit,
            aux2: 0,
            ..Instruction::default()
        }
    }

    fn reset(qubit: u64) -> Instruction<u64> {
        Instruction {
            opcode: 0x12 | IMMEDIATE_AUX1,
            aux0: 2,
            aux1: qubit,
            ..Instruction::default()
        }
    }

    fn ret() -> Instruction<u64> {
        Instruction {
            opcode: 0x02,
            ..Instruction::default()
        }
    }

    #[test]
    fn state_queries_reject_program_errors_before_device_discovery() {
        let program = prepared_program(
            vec![gate(0), measure(0), ret()],
            vec![operation(5), operation(21)],
        );
        let mut absent_qubit = PauliSum::new();
        absent_qubit
            .push_labels(Complex64::new(1.0, 0.0), "Z", &[2])
            .expect("the term is well formed");
        let mut present_qubit = PauliSum::new();
        present_qubit
            .push_labels(Complex64::new(1.0, 0.0), "Z", &[1])
            .expect("the term is well formed");
        let mps = StateMethod::Mps {
            max_bond_dimension: None,
        };
        let cases = [
            (Vec::new(), mps, "queries must contain at least one query"),
            (
                vec![StateQuery::Cost],
                StateMethod::Exact,
                "Cost on a cuTensorNet state requires an MPS",
            ),
            (
                vec![
                    StateQuery::Expectation(present_qubit),
                    StateQuery::Expectation(absent_qubit),
                ],
                mps,
                "Expectation acts on qubit 2, but the program has 2 qubits",
            ),
        ];

        for (queries, method, message) in cases {
            let error = evaluate_state_queries(&program, &queries, None, method)
                .expect_err("the program error should be rejected");
            assert_eq!(error.to_string(), message);
            assert!(!error.is_environment_error());
        }
    }

    #[test]
    fn state_queries_share_the_circuit_preflight() {
        let program = prepared_program(
            vec![measure(0), gate(1), ret()],
            vec![operation(5), operation(21)],
        );

        let error = evaluate_state_queries(
            &program,
            &[StateQuery::Cost],
            None,
            StateMethod::Mps {
                max_bond_dimension: Some(4),
            },
        )
        .expect_err("feedforward should be rejected");

        assert!(error.to_string().contains("after measuring qubit 0"));
        assert!(!error.is_environment_error());
    }

    #[test]
    fn probability_rejects_program_errors_before_device_discovery() {
        let bell_half = prepared_program(
            vec![gate(0), measure(0), ret()],
            vec![operation(5), operation(21)],
        );
        let mut z0 = PauliSum::new();
        z0.push_labels(Complex64::new(1.0, 0.0), "Z", &[0])
            .expect("the term is well formed");
        let mps = StateMethod::Mps {
            max_bond_dimension: Some(4),
        };
        let one_outcome: &[bool] = &[false];
        let cases = [
            (
                &bell_half,
                vec![StateQuery::Probability],
                Some(one_outcome),
                StateMethod::Exact,
                "Probability on a cuTensorNet state requires an MPS; \
                 exact Probability contracts the fixed-outcome network",
            ),
            (
                &bell_half,
                vec![StateQuery::Probability, StateQuery::Expectation(z0)],
                Some(one_outcome),
                mps,
                "Probability and Expectation read different MPS states and cannot share one \
                 call; evaluate them in separate calls",
            ),
            (
                &bell_half,
                vec![StateQuery::Cost, StateQuery::Probability],
                None,
                mps,
                "Probability requires outcomes",
            ),
            (
                &bell_half,
                vec![StateQuery::Probability],
                Some(&[false, false]),
                mps,
                "the program has 1 results, but 2 outcomes were given",
            ),
        ];

        for (program, queries, outcomes, method, message) in cases {
            let error = evaluate_state_queries(program, &queries, outcomes, method)
                .expect_err("the program error should be rejected");
            assert_eq!(error.to_string(), message);
            assert!(!error.is_environment_error());
        }
    }

    /// The ψ̃ route follows the fixed path through mid-circuit measurement,
    /// feedforward and reset, which the ψ route rejects, and reaches the
    /// device: an environment error without cuTensorNet, P = ½ with it.
    #[test]
    fn probability_follows_measurement_feedforward_and_reset_to_the_device() {
        let program = prepared_program(
            vec![gate(0), measure(0), gate(1), reset(0), ret()],
            vec![operation(5), operation(21), operation(1)],
        );

        let result = evaluate_state_queries(
            &program,
            &[StateQuery::Cost, StateQuery::Probability],
            Some(&[true]),
            StateMethod::Mps {
                max_bond_dimension: Some(4),
            },
        );

        match result {
            Ok(values) => {
                let [
                    StateQueryValue::Cost(_),
                    StateQueryValue::Probability(probability),
                ] = values.as_slice()
                else {
                    panic!("values follow query order: {values:?}");
                };
                assert!((probability - 0.5).abs() < 1e-12, "P = {probability}");
            }
            Err(error) => assert!(error.is_environment_error(), "{error}"),
        }
    }

    #[test]
    fn preflight_rejects_multiple_regions_before_device_discovery() {
        let program = prepared_program(
            vec![gate(0), measure(0), gate(1), ret()],
            vec![operation(5), operation(21)],
        );

        let error = prepare_mps_run(&program, 1, Some(42))
            .expect_err("multiple regions should be rejected");

        assert_eq!(
            error.to_string(),
            "cuTensorNet batch sampling requires exactly one quantum evolution region, found 2"
        );
        assert!(!error.is_environment_error());
    }

    #[test]
    fn preflight_rejects_feedforward_before_device_discovery() {
        let program = prepared_program(
            vec![measure(0), gate(1), ret()],
            vec![operation(5), operation(21)],
        );

        let error =
            prepare_mps_run(&program, 1, Some(42)).expect_err("feedforward should be rejected");

        assert!(
            error.to_string().contains(
                "cuTensorNet batch sampling cannot execute a quantum evolution region after measuring qubit 0"
            )
        );
        assert!(!error.is_environment_error());
    }

    #[test]
    fn preflight_accepts_every_unitary_operation() {
        // I, X, Y, Z, H, S, S†, T, T†, SX, SX†, Rx, Ry, Rz, CX, CZ, Rxx, Ryy,
        // Rzz, SWAP and CY, on qubits (0, 1) where they take two.
        for operation_id in (0..=19).filter(|&id| id != 1).chain([24, 29]) {
            let two_qubit_gate = Instruction { aux2: 1, ..gate(0) };
            let program = prepared_program(
                vec![two_qubit_gate, measure(0), ret()],
                vec![operation(operation_id), operation(21)],
            );

            prepare_mps_run(&program, 1, Some(42)).unwrap_or_else(|error| {
                panic!("operation {operation_id} should pass the preflight: {error}")
            });
        }
    }

    #[test]
    fn preflight_rejects_reset_before_device_discovery() {
        let program = prepared_program(
            vec![gate(0), reset(0), measure(0), ret()],
            vec![operation(5), operation(21), operation(1)],
        );

        let error = prepare_mps_run(&program, 1, Some(42)).expect_err("reset should be rejected");

        assert_eq!(
            error.to_string(),
            "consumer execution failed: reset is not supported by cuTensorNet MPS batch sampling (qubit 0)"
        );
        assert!(!error.is_environment_error());
    }
}
