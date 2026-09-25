// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use pyo3::{
    exceptions::PyNotImplementedError,
    prelude::*,
    types::{PyDict, PyList},
};

/// Evaluates queries on a cuTensorNet state: exact when `mps` is `None`,
/// otherwise an MPS configured by `mps` (for example `max_bond_dimension`).
///
/// `queries` holds the dicts built by `qdk.simulation.tensornetwork_qir`;
/// `outcomes[i]` fixes QIR result `i`. Returns one value per query, in order.
#[pyfunction]
#[pyo3(signature = (input, queries, outcomes=None, mps=None))]
pub(crate) fn _tensor_network_state_query<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyDict>,
    queries: &Bound<'py, PyList>,
    outcomes: Option<Vec<bool>>,
    mps: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyList>> {
    let _ = (py, input, queries, outcomes, mps);
    Err(PyNotImplementedError::new_err(
        "tensor-network state queries are not implemented yet",
    ))
}
