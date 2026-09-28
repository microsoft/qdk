// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeMap;

use pyo3::{
    exceptions::{PyOSError, PyValueError},
    prelude::*,
    types::{PyDict, PyList},
};
use qdk_cutensornet::{
    ContractionExecutionError, ContractionSettings, closed_amplitude_cost,
    contract_closed_amplitude,
};
use qdk_simulators::execution::{
    AdaptiveCommand, AdaptiveExecution, AdaptiveResponse, CircuitTensorNetwork, ContractionCost,
    FixedOutcomeCircuit, FixedOutcomeOperation, PreparedAdaptiveProgram,
};

use super::adaptive_program_from_pydict;

/// Summarizes the fixed-outcome circuit that `outcomes` selects, for host
/// qualification. Does not build or contract a tensor network.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn _fixed_outcome_probe<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyDict>,
    outcomes: Vec<bool>,
) -> PyResult<Bound<'py, PyDict>> {
    let program = adaptive_program_from_pydict::<u64>(input)?;
    let prepared = PreparedAdaptiveProgram::new(program)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let circuit = FixedOutcomeCircuit::from_prepared_program(&prepared, &outcomes)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;

    let mut gate_counts = BTreeMap::<&str, usize>::new();
    let mut measure_count = 0;
    let mut reset_measure_count = 0;
    for operation in circuit.operations() {
        match operation {
            FixedOutcomeOperation::Unitary(unitary) => {
                *gate_counts.entry(unitary.name()).or_default() += 1;
            }
            FixedOutcomeOperation::Measure { reset, .. } => {
                measure_count += 1;
                reset_measure_count += usize::from(*reset);
            }
        }
    }
    let report = PyDict::new(py);
    report.set_item("qubit_count", circuit.qubit_count())?;
    report.set_item("gate_counts", gate_counts)?;
    report.set_item("measure_count", measure_count)?;
    report.set_item("reset_measure_count", reset_measure_count)?;
    report.set_item("region_count", prepared.regions().len())?;
    Ok(report)
}

/// Describes the shape of the amplitude network of the fixed-outcome circuit
/// that `outcomes` selects, for host width estimates: each node's axis ids and
/// the output axis ids. Every axis has dimension two; no coefficients are
/// copied and nothing is contracted.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn _fixed_outcome_network_probe<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyDict>,
    outcomes: Vec<bool>,
) -> PyResult<Bound<'py, PyDict>> {
    let circuit = fixed_outcome_circuit(input, &outcomes)?;
    let built = amplitude_network(&circuit)?;
    let report = PyDict::new(py);
    report.set_item(
        "nodes",
        built
            .network()
            .nodes()
            .iter()
            .map(|node| node.as_slice().iter().map(|axis| axis.id()).collect())
            .collect::<Vec<Vec<_>>>(),
    )?;
    report.set_item(
        "output_axes",
        built
            .output_axes()
            .as_slice()
            .iter()
            .map(|axis| axis.id())
            .collect::<Vec<_>>(),
    )?;
    Ok(report)
}

/// Builds a single leading region for host qualification, without executing
/// measurements, contracting tensors, or validating the terminal suffix.
#[pyfunction]
pub(crate) fn _tensor_network_build_probe<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyDict>> {
    let program = adaptive_program_from_pydict::<u64>(input)?;
    let prepared = PreparedAdaptiveProgram::new(program)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let program = prepared.program();
    if program.block_table.len() != 1
        || program.entry_block != 0
        || prepared.regions().len() != 1
        || prepared.regions()[0].instruction_range.start != 0
    {
        return Err(PyValueError::new_err(
            "tensor-network build probe requires one block with one leading unitary region",
        ));
    }
    let mut execution = AdaptiveExecution::new(&prepared);
    let AdaptiveCommand::ExecuteRegion { region, .. } = execution
        .next_command(None)
        .map_err(|error| PyValueError::new_err(error.to_string()))?
    else {
        return Err(PyValueError::new_err("expected a leading unitary region"));
    };
    let built = CircuitTensorNetwork::from_zero_state(
        usize::try_from(program.num_qubits)
            .map_err(|error| PyValueError::new_err(error.to_string()))?,
        &region,
    )
    .map_err(|error| PyValueError::new_err(error.to_string()))?;
    match execution
        .next_command(Some(AdaptiveResponse::RegionComplete))
        .map_err(|error| PyValueError::new_err(error.to_string()))?
    {
        AdaptiveCommand::Measure(_)
        | AdaptiveCommand::Reset { .. }
        | AdaptiveCommand::Complete(_) => {}
        AdaptiveCommand::ExecuteRegion { .. } => {
            return Err(PyValueError::new_err("unexpected second unitary region"));
        }
    }
    let measured = prepared
        .measured_qubits()
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let report = describe_network(py, &built)?;
    report.set_item("operation_count", region.operations().len())?;
    report.set_item(
        "measurement_qubits",
        measured
            .iter()
            .map(|measurement| measurement.qubit)
            .collect::<Vec<_>>(),
    )?;
    report.set_item(
        "measurement_result_ids",
        measured
            .iter()
            .map(|measurement| measurement.result_id)
            .collect::<Vec<_>>(),
    )?;
    Ok(report)
}

/// Evaluates probability and cost queries by general tensor-network contraction.
///
/// `queries` holds the dicts built by `qdk.simulation.tensornetwork_qir`;
/// `outcomes[i]` fixes QIR result `i`. Returns one value per query, in order.
#[pyfunction]
#[pyo3(signature = (input, queries, outcomes, options))]
pub(crate) fn _tensor_network_contraction_query<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyDict>,
    queries: &Bound<'py, PyList>,
    outcomes: Option<Vec<bool>>,
    options: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyList>> {
    let outcomes =
        outcomes.ok_or_else(|| PyValueError::new_err("contraction requires outcomes"))?;
    let mut probability_queries = Vec::with_capacity(queries.len());
    for query in queries {
        let kind: String = query.get_item("kind")?.extract()?;
        probability_queries.push(match kind.as_str() {
            "probability" => true,
            "cost" => false,
            _ => {
                return Err(PyValueError::new_err(format!(
                    "unsupported contraction query: {kind}"
                )));
            }
        });
    }
    if probability_queries.is_empty() {
        return Err(PyValueError::new_err(
            "queries must contain at least one query",
        ));
    }
    let network = amplitude_network(&fixed_outcome_circuit(input, &outcomes)?)?;
    let settings = contraction_settings(options)?;
    let (probability, cost) = if probability_queries.contains(&true) {
        let result = contract_closed_amplitude(&network, settings).map_err(contraction_error)?;
        // TODO(selection-normalization): this is P_pass, not P_selected.
        // Plan section 5.2: SELECT on one Bell outcome accepts with probability
        // 1/2; the accepted record has P_pass = 1/2 but P_selected = 1.
        // Normalization needs acceptance marginals, not this single amplitude.
        (Some(result.amplitude.norm_sqr()), result.cost)
    } else {
        (
            None,
            closed_amplitude_cost(&network, settings).map_err(contraction_error)?,
        )
    };
    let results = PyList::empty(py);
    for is_probability in probability_queries {
        if is_probability {
            results.append(probability.expect("probability was evaluated"))?;
        } else {
            results.append(contraction_cost_dict(py, cost)?)?;
        }
    }
    Ok(results)
}

/// Flips one cap without changing the accepted control-flow path. This is
/// deliberately separate from supplying a record that fails a selection check.
#[pyfunction]
#[pyo3(signature = (input, outcomes, flip_result_id, options))]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn _fixed_outcome_contraction_probe(
    input: &Bound<'_, PyDict>,
    outcomes: Vec<bool>,
    flip_result_id: usize,
    options: &Bound<'_, PyDict>,
) -> PyResult<f64> {
    let circuit = fixed_outcome_circuit(input, &outcomes)?;
    let current = outcomes
        .get(flip_result_id)
        .ok_or_else(|| PyValueError::new_err(format!("unknown result {flip_result_id}")))?;
    let flipped = circuit
        .with_outcome(flip_result_id, !current)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let result = contract_closed_amplitude(
        &amplitude_network(&flipped)?,
        contraction_settings(options)?,
    )
    .map_err(contraction_error)?;
    Ok(result.amplitude.norm_sqr())
}

fn fixed_outcome_circuit(
    input: &Bound<'_, PyDict>,
    outcomes: &[bool],
) -> PyResult<FixedOutcomeCircuit> {
    let program = adaptive_program_from_pydict::<u64>(input)?;
    let prepared = PreparedAdaptiveProgram::new(program)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    FixedOutcomeCircuit::from_prepared_program(&prepared, outcomes)
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

fn amplitude_network(circuit: &FixedOutcomeCircuit) -> PyResult<CircuitTensorNetwork> {
    CircuitTensorNetwork::from_fixed_outcome_circuit(circuit)
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

/// Python fills every default (`_contraction_options_dict`); a missing key is
/// a caller error, so defaults are defined in one place.
fn contraction_settings(options: &Bound<'_, PyDict>) -> PyResult<ContractionSettings> {
    let get = |name: &str| -> PyResult<u32> {
        options
            .get_item(name)?
            .ok_or_else(|| PyValueError::new_err(format!("options must include {name}")))?
            .extract()
            .map_err(|error| PyValueError::new_err(format!("{name}: {error}")))
    };
    Ok(ContractionSettings {
        hyper_samples: get("hyper_samples")?,
        seed: get("seed")?,
    })
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "used directly with Result::map_err"
)]
fn contraction_error(error: ContractionExecutionError) -> PyErr {
    if error.is_environment_error() {
        PyOSError::new_err(error.to_string())
    } else {
        PyValueError::new_err(error.to_string())
    }
}

fn contraction_cost_dict(py: Python<'_>, cost: ContractionCost) -> PyResult<Bound<'_, PyDict>> {
    let report = PyDict::new(py);
    report.set_item("width", cost.width)?;
    report.set_item("flops", cost.flops)?;
    report.set_item("workspace_bytes", cost.workspace_bytes)?;
    Ok(report)
}

fn describe_network<'py>(
    py: Python<'py>,
    built: &CircuitTensorNetwork,
) -> PyResult<Bound<'py, PyDict>> {
    let query = built
        .query()
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let report = PyDict::new(py);
    report.set_item(
        "nodes",
        built
            .network()
            .nodes()
            .iter()
            .map(|node| {
                node.as_slice()
                    .iter()
                    .map(|axis| (axis.id(), axis.dim()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
    )?;
    report.set_item(
        "buffers",
        built
            .buffers()
            .iter()
            .map(|buffer| buffer.to_vec())
            .collect::<Vec<_>>(),
    )?;
    report.set_item("node_buffer_ids", built.node_buffer_ids())?;
    report.set_item(
        "output_axes",
        query
            .keep()
            .as_slice()
            .iter()
            .map(|axis| (axis.id(), axis.dim()))
            .collect::<Vec<_>>(),
    )?;
    report.set_item(
        "hyperedges",
        query
            .hyperedges()
            .as_slice()
            .iter()
            .map(|axis| axis.id())
            .collect::<Vec<_>>(),
    )?;
    report.set_item(
        "marginalized",
        query
            .marginalized()
            .as_slice()
            .iter()
            .map(|axis| axis.id())
            .collect::<Vec<_>>(),
    )?;
    Ok(report)
}
