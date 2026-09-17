// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use pyo3::{exceptions::PyValueError, prelude::*, types::PyDict};
use qdk_simulators::execution::{
    AdaptiveCommand, AdaptiveExecution, AdaptiveResponse, CircuitTensorNetwork,
    PreparedAdaptiveProgram,
};

use super::adaptive_program_from_pydict;

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
        AdaptiveCommand::Measure(_) | AdaptiveCommand::Complete(_) => {}
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
