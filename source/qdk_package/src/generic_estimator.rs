// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{rc::Rc, time::Instant};

use pyo3::{prelude::*, types::PyDict};
use resource_estimator::estimates::{ErrorBudget, ErrorBudgetStrategy, PhysicalResourceEstimation};
use serde_json::json;

use crate::generic_estimator::{
    code::PythonQEC,
    counts::PythonCounts,
    factory::{PythonFactoryBuilder, PythonFactoryBuilderDispatch},
    utils::json_map_to_python_dict,
};

mod code;
mod counts;
mod factory;
mod utils;

#[cfg(test)]
mod tests;

pub(crate) fn register_generic_estimator_submodule(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(estimate_custom, m)?)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(
    signature = (algorithm, qubit, qec, factories = vec![], *, error_budget = 0.01, max_factories = None, logical_depth_factor = None, max_physical_qubits = None, max_duration = None, error_budget_pruning = false),
)]
fn estimate_custom<'py>(
    algorithm: Bound<'py, PyAny>,
    qubit: Bound<'py, PyDict>,
    qec: Bound<'py, PyAny>,
    factories: Vec<Bound<'py, PyAny>>,
    error_budget: f64,
    max_factories: Option<u64>,
    logical_depth_factor: Option<f64>,
    max_physical_qubits: Option<u64>,
    max_duration: Option<u64>,
    error_budget_pruning: bool,
) -> PyResult<Bound<'py, PyDict>> {
    let error_budget = ErrorBudget::new(error_budget / 3.0, error_budget / 3.0, error_budget / 3.0);

    // evaluate algorithm to compute post-mapping logical resource counts
    let time_algorithm = Instant::now();
    let algorithm_overhead = Rc::new(PythonCounts::from_bound(algorithm)?);
    let time_algorithm = time_algorithm.elapsed().as_nanos();

    // prepare estimation input
    let qubit = Rc::new(qubit);
    let code = PythonQEC::from_bound(qec)?;

    // load factories from Python
    let factories = factories
        .into_iter()
        .map(PythonFactoryBuilder::from_bound)
        .collect::<PyResult<Vec<_>>>()?;

    // create resource estimator
    let mut estimation = PhysicalResourceEstimation::new(
        code,
        qubit.clone(),
        PythonFactoryBuilderDispatch(factories),
        algorithm_overhead.clone(),
    );
    if let Some(max_factories) = max_factories {
        estimation.set_max_factories(max_factories);
    }
    if let Some(logical_depth_factor) = logical_depth_factor {
        estimation.set_logical_depth_factor(logical_depth_factor);
    }
    if let Some(max_physical_qubits) = max_physical_qubits {
        estimation.set_max_physical_qubits(max_physical_qubits);
    }
    if let Some(max_duration) = max_duration {
        estimation.set_max_duration(max_duration);
    }
    if error_budget_pruning {
        estimation.set_error_budget_strategy(ErrorBudgetStrategy::PruneLogicalAndRotations);
    }

    // perform estimation
    let time_estimation = Instant::now();
    let result = estimation
        .estimate(&error_budget)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
    let time_estimation = time_estimation.elapsed().as_nanos();

    // we first serialize the result to JSON, then convert it to a Python
    // dictionary.  The alternative would be to convert the result to a Python
    // dictionary directly, but that would either:
    // - require to ensure that all fields are consistently added to the result
    //   dictionary,
    // - require to implement a custom serializer for a Python dictionary
    let json = json!(&result);

    // serialize the result to a Python dictionary and add some other fields
    let dict = json_map_to_python_dict(
        qubit.as_ref().py(),
        json.as_object().expect("result is a JSON object"),
    )?;

    dict.set_item("qubit", qubit.as_ref())?;

    if let Some(value) = algorithm_overhead.algorithm_overhead(&error_budget)? {
        dict.set_item("algorithmOverhead", value)?;
    }

    let execution_stats = PyDict::new(qubit.as_ref().py());
    execution_stats.set_item("timeAlgorithm", time_algorithm)?;
    execution_stats.set_item("timeEstimation", time_estimation)?;
    dict.set_item("executionStats", execution_stats)?;

    Ok(dict)
}
