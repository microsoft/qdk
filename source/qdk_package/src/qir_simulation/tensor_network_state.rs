// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_complex::Complex64;
use pyo3::{
    exceptions::{PyOSError, PyValueError},
    prelude::*,
    types::{PyDict, PyList},
};
use qdk_cutensornet::{
    MpsCost, MpsExecutionError, StateMethod, StateQuery, StateQueryValue, evaluate_state_queries,
};
use qdk_simulators::execution::{PauliSum, PreparedAdaptiveProgram};

use super::adaptive_program_from_pydict;

/// Evaluates queries on a cuTensorNet state: exact when `mps` is `None`,
/// otherwise an MPS configured by `mps` (for example `max_bond_dimension`).
///
/// `queries` holds the dicts built by `qdk.simulation.tensornetwork_qir`.
/// Expectation and Cost read ψ, the state before the terminal measurements,
/// which no outcome changes, so they ignore `outcomes`. A call with a
/// Probability query instead reads the unnormalized state on the path
/// `outcomes` fixes (`outcomes[i]` is QIR result `i`), and its Cost describes
/// that MPS; Probability and Expectation cannot share a call. Returns one
/// value per query, in order.
#[pyfunction]
#[pyo3(signature = (input, queries, outcomes=None, mps=None))]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn _tensor_network_state_query<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyDict>,
    queries: &Bound<'py, PyList>,
    outcomes: Option<Vec<bool>>,
    mps: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyList>> {
    let method = match mps {
        None => StateMethod::Exact,
        Some(options) => StateMethod::Mps {
            max_bond_dimension: options
                .get_item("max_bond_dimension")?
                .filter(|value| !value.is_none())
                .map(|value| value.extract::<u32>())
                .transpose()
                .map_err(|error| PyValueError::new_err(format!("max_bond_dimension: {error}")))?,
        },
    };
    let mut state_queries = Vec::with_capacity(queries.len());
    for query in queries {
        let kind: String = query.get_item("kind")?.extract()?;
        state_queries.push(match kind.as_str() {
            "expectation" => StateQuery::Expectation(pauli_sum(&query)?),
            "cost" => StateQuery::Cost,
            "probability" => StateQuery::Probability,
            _ => {
                return Err(PyValueError::new_err(format!(
                    "unsupported state query: {kind}"
                )));
            }
        });
    }

    let program = adaptive_program_from_pydict::<u64>(input)?;
    let prepared = PreparedAdaptiveProgram::new(program)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    let values = evaluate_state_queries(&prepared, &state_queries, outcomes.as_deref(), method)
        .map_err(state_query_error)?;
    let results = PyList::empty(py);
    for value in values {
        match value {
            StateQueryValue::Expectation(value) => results.append(value)?,
            StateQueryValue::Probability(probability) => results.append(probability)?,
            StateQueryValue::Cost(cost) => results.append(mps_cost_dict(py, cost)?)?,
        }
    }
    Ok(results)
}

fn pauli_sum(query: &Bound<'_, PyAny>) -> PyResult<PauliSum> {
    let terms: Vec<(String, Vec<usize>, Complex64)> = query.get_item("terms")?.extract()?;
    let mut sum = PauliSum::new();
    for (labels, qubits, coefficient) in terms {
        sum.push_labels(coefficient, &labels, &qubits)
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
    }
    Ok(sum)
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "used directly with Result::map_err"
)]
fn state_query_error(error: MpsExecutionError) -> PyErr {
    if error.is_environment_error() {
        PyOSError::new_err(error.to_string())
    } else {
        PyValueError::new_err(error.to_string())
    }
}

fn mps_cost_dict(py: Python<'_>, cost: MpsCost) -> PyResult<Bound<'_, PyDict>> {
    let report = PyDict::new(py);
    report.set_item("max_bond_dimension", cost.max_bond_dimension)?;
    report.set_item("state_bytes", cost.state_bytes)?;
    report.set_item("workspace_bytes", cost.workspace_bytes)?;
    Ok(report)
}
