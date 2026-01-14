// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use pyo3::{
    Bound, PyAny, PyResult,
    types::{PyAnyMethods, PyDict},
};
use resource_estimator::estimates::ErrorCorrection;

use super::utils::{SerializableBound, extract_and_check_method, maybe_extract_and_check_method};

/// A wrapper around a Python instance to compute quantum error correction
/// properties in generic resource estimation.
///
/// The code parameter (e.g., distance) is generic and the possible code
/// parameter values are returned via the `code_parameter_range` method and
/// compared (for sorting) via the `code_parameter_cmp` method:
///
/// ```python
///     def code_parameter_range(self):
///         # e.g., return [7, 9, 11]
///         ...
///
///     def code_parameter_cmp(self, qubit, p1, p2):
///         # qubit properties (via qubit) may be considered for comparison
///         #
///         # must return -1 if p1 < p2, 0 if p1 == p2, and 1 if p1 > p2
///         ...
/// ```
///
/// The number of physical and logical qubits for one code patch depends on the
/// code parameter and is computed with the methods `physical_qubits` and
/// `logical_qubits`, respectively:
///
/// ```python
///     def physical_qubits(self, param):
///         # must return an int
///         ...
///
///     def logical_qubits(self, param):
///         # must return an int
///         ...
/// ```
///
/// Finally, the logical cycle time and logical error rate depend on qubit
/// properties and the code parameter.  They are computed using the methods
/// `logical_cycle_time` and `logical_error_rate`, respectively, in which the
/// qubit is a Python dictionary.
///
/// ```python
///     def logical_cycle_time(self, qubit, param):
///         # returns logical cycle time in nano seconds (int)
///         ...
///
///     def logical_error_rate(self, qubit, param):
///         # must return a float
///         ...
/// ```
pub struct PythonQEC<'py> {
    qec: Bound<'py, PyAny>,
    physical_qubits_method: Bound<'py, PyAny>,
    logical_qubits_method: Bound<'py, PyAny>,
    logical_cycle_time_method: Bound<'py, PyAny>,
    logical_error_rate_method: Bound<'py, PyAny>,
    code_parameter_cmp_method: Bound<'py, PyAny>,
    adjust_code_parameter_method: Option<Bound<'py, PyAny>>,
    params: Vec<SerializableBound<'py>>,
}

impl<'py> PythonQEC<'py> {
    pub fn from_bound(qec: Bound<'py, PyAny>) -> PyResult<Self> {
        let physical_qubits_method = extract_and_check_method(&qec, "physical_qubits")?;
        let logical_qubits_method = extract_and_check_method(&qec, "logical_qubits")?;
        let logical_cycle_time_method = extract_and_check_method(&qec, "logical_cycle_time")?;
        let logical_error_rate_method = extract_and_check_method(&qec, "logical_error_rate")?;
        let code_parameter_range_method = extract_and_check_method(&qec, "code_parameter_range")?;
        let code_parameter_cmp_method = extract_and_check_method(&qec, "code_parameter_cmp")?;

        let adjust_code_parameter_method =
            maybe_extract_and_check_method(&qec, "adjust_code_parameter")?;

        let params0: Vec<Bound<'py, PyAny>> = code_parameter_range_method.call0()?.extract()?;

        let params: Vec<_> = params0.into_iter().map(SerializableBound).collect();
        Ok(Self {
            qec,
            physical_qubits_method,
            logical_qubits_method,
            logical_cycle_time_method,
            logical_error_rate_method,
            code_parameter_cmp_method,
            adjust_code_parameter_method,
            params,
        })
    }

    pub fn bound(&self) -> &Bound<'py, PyAny> {
        &self.qec
    }
}

impl<'py> ErrorCorrection for PythonQEC<'py> {
    type Qubit = Bound<'py, PyDict>;

    type Parameter = SerializableBound<'py>;

    fn physical_qubits(&self, code_parameter: &Self::Parameter) -> Result<u64, String> {
        let result = self
            .physical_qubits_method
            .call1((&**code_parameter,))
            .map_err(|e| e.to_string())?;

        result.extract().map_err(|e: pyo3::PyErr| e.to_string())
    }

    fn logical_qubits(&self, code_parameter: &Self::Parameter) -> Result<u64, String> {
        let result = self
            .logical_qubits_method
            .call1((&**code_parameter,))
            .map_err(|e| e.to_string())?;

        result.extract().map_err(|e: pyo3::PyErr| e.to_string())
    }

    fn logical_cycle_time(
        &self,
        qubit: &Self::Qubit,
        code_parameter: &Self::Parameter,
    ) -> Result<u64, String> {
        let result = self
            .logical_cycle_time_method
            .call1((qubit, &**code_parameter))
            .map_err(|e| e.to_string())?;

        result.extract().map_err(|e: pyo3::PyErr| e.to_string())
    }

    fn logical_error_rate(
        &self,
        qubit: &Self::Qubit,
        code_parameter: &Self::Parameter,
    ) -> Result<f64, String> {
        let result = self
            .logical_error_rate_method
            .call1((qubit, &**code_parameter))
            .map_err(|e| e.to_string())?;

        result.extract().map_err(|e: pyo3::PyErr| e.to_string())
    }

    fn code_parameter_range(
        &self,
        _lower_bound: Option<&Self::Parameter>,
    ) -> impl Iterator<Item = Self::Parameter> {
        self.params.iter().cloned()
    }

    fn code_parameter_cmp(
        &self,
        qubit: &Self::Qubit,
        p1: &Self::Parameter,
        p2: &Self::Parameter,
    ) -> std::cmp::Ordering {
        let result: i32 = self
            .code_parameter_cmp_method
            .call1((qubit, &**p1, &**p2))
            .expect("can call code_parameter_cmp method")
            .extract()
            .expect("can convert code_parameter_cmp return value into i32");

        result.cmp(&0)
    }

    fn adjust_code_parameter(&self, parameter: Self::Parameter) -> Result<Self::Parameter, String> {
        if let Some(method) = &self.adjust_code_parameter_method {
            let result = method.call1((&*parameter,)).map_err(|e| e.to_string())?;
            result
                .extract()
                .map(SerializableBound)
                .map_err(|e| e.to_string())
        } else {
            Ok(parameter)
        }
    }
}
