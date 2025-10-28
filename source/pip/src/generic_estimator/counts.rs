// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use pyo3::{
    Bound, PyAny, PyResult,
    types::{PyAnyMethods, PyDict},
};
use resource_estimator::estimates::{ErrorBudget, ErrorBudgetStrategy, Overhead};

use crate::generic_estimator::utils::maybe_extract_and_check_method;

use super::utils::extract_and_check_method;

/// A wrapper around a Python instance to compute post-layout logical overhead
/// for generic resource estimation.
///
/// The Python instance must implement three methods to compute the number of
/// logical qubits, the logical depth, and the number of magic states.  For the
/// last two methods, the method receives a parameter `budget`, which is a
/// dictionary with access to the chosen error budget for logical errors
/// (`"logical"`), rotation synthesis budget (`"rotations"`), and magic state
/// budget (`"magic_states"`):
///
/// ```python
///     def logical_qubits(self):
///         # must return an int
///         ...
///
///     def logical_depth(self, budget):
///         # must return an int
///         #
///         # budget is a dictionary of the form
///         # {"logical": ..., "rotations": ..., "magic_states": ...}
///         ...
///
///     def num_magic_states(self, budget, index):
///         # must return an int
///         #
///         # here index is the type of magic state; normally this number is 0,
///         # as there is only one magic state, but if there are multiple, this
///         # is some number starting from 0.
///         ...
/// ```
///
/// It's important to note that the number of factory builders provided in the
/// `estimate_generic` function determine how many magic states are being
/// requested from the logical counts model.  If only one factory is provided,
/// then the `num_magic_states` method is only requested for the index `0` and
/// not for any other indices.
///
/// Optionally, the instance may implement a method called `algorithm_overhead`,
/// which also takes the error budget parameter `parameter` and returns a Python
/// dictionary that is serialized into the estimation result, accessible via the
/// key `"algorithmOverhead"`.  This can contain layout-specific variables that
/// are interesting statistics to expose.
///
/// ```python
///     def algorithm_overhead(self, budget):
///         # returns a serializable Python dictionary
///         ...
/// ```
pub struct PythonCounts<'py> {
    counts: Bound<'py, PyAny>,
    logical_qubits_method: Bound<'py, PyAny>,
    logical_depth_method: Bound<'py, PyAny>,
    num_magic_states_method: Bound<'py, PyAny>,
    prune_error_budget: Option<Bound<'py, PyAny>>,
}

impl<'py> PythonCounts<'py> {
    pub fn from_bound(counts: Bound<'py, PyAny>) -> PyResult<Self> {
        let logical_qubits_method = extract_and_check_method(&counts, "logical_qubits")?;
        let logical_depth_method = extract_and_check_method(&counts, "logical_depth")?;
        let num_magic_states_method = extract_and_check_method(&counts, "num_magic_states")?;
        let prune_error_budget = maybe_extract_and_check_method(&counts, "prune_error_budget")?;

        Ok(Self {
            counts,
            logical_qubits_method,
            logical_depth_method,
            num_magic_states_method,
            prune_error_budget,
        })
    }

    fn convert_budget(&self, budget: &ErrorBudget) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(self.counts.py());

        dict.set_item("logical", budget.logical())?;
        dict.set_item("rotations", budget.rotations())?;
        dict.set_item("magic_states", budget.magic_states())?;

        Ok(dict)
    }

    fn convert_error_budget_strategy(strategy: ErrorBudgetStrategy) -> u32 {
        match strategy {
            ErrorBudgetStrategy::Static => 0,
            ErrorBudgetStrategy::PruneLogicalAndRotations => 1,
        }
    }

    pub fn algorithm_overhead(
        &self,
        error_budget: &ErrorBudget,
    ) -> PyResult<Option<Bound<'py, PyAny>>> {
        if let Ok(result) = self
            .counts
            .getattr("algorithm_overhead")
            .and_then(|f| f.call1((self.convert_budget(error_budget)?,)))
        {
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }
}

impl Overhead for PythonCounts<'_> {
    fn logical_qubits(&self) -> Result<u64, String> {
        let result = self
            .logical_qubits_method
            .call0()
            .map_err(|e| e.to_string())?;
        result.extract().map_err(|e| e.to_string())
    }

    fn logical_depth(&self, budget: &ErrorBudget) -> Result<u64, String> {
        let budget = self.convert_budget(budget).map_err(|e| e.to_string())?;

        let result = self
            .logical_depth_method
            .call1((budget,))
            .map_err(|e| e.to_string())?;

        result.extract().map_err(|e| e.to_string())
    }

    fn num_magic_states(&self, budget: &ErrorBudget, index: usize) -> Result<u64, String> {
        let budget = self.convert_budget(budget).map_err(|e| e.to_string())?;
        let result = self
            .num_magic_states_method
            .call1((budget, index))
            .map_err(|e| e.to_string())?;

        result.extract().map_err(|e| e.to_string())
    }

    fn prune_error_budget(
        &self,
        budget: &mut ErrorBudget,
        strategy: ErrorBudgetStrategy,
    ) -> Result<(), String> {
        if let Some(method) = &self.prune_error_budget {
            let budget_dict = self.convert_budget(budget).map_err(|e| e.to_string())?;
            let strategy = Self::convert_error_budget_strategy(strategy);

            method
                .call1((budget_dict.clone(), strategy))
                .map_err(|e| e.to_string())?;

            budget.set_logical(
                budget_dict
                    .get_item("logical")
                    .map_err(|e| e.to_string())?
                    .extract()
                    .map_err(|e| e.to_string())?,
            );
            budget.set_rotations(
                budget_dict
                    .get_item("rotations")
                    .map_err(|e| e.to_string())?
                    .extract()
                    .map_err(|e| e.to_string())?,
            );
            budget.set_magic_states(
                budget_dict
                    .get_item("magic_states")
                    .map_err(|e| e.to_string())?
                    .extract()
                    .map_err(|e| e.to_string())?,
            );
        }
        Ok(())
    }
}
