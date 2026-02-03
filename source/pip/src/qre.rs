// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::ptr::NonNull;

use pyo3::{
    IntoPyObjectExt,
    exceptions::{PyException, PyKeyError, PyTypeError},
    prelude::*,
    types::{PyDict, PyTuple},
};
use qre::TraceTransform;
use serde::{Deserialize, Serialize};

pub(crate) fn register_qre_submodule(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ISA>()?;
    m.add_class::<ISARequirements>()?;
    m.add_class::<Instruction>()?;
    m.add_class::<Constraint>()?;
    m.add_class::<Property>()?;
    m.add_class::<IntFunction>()?;
    m.add_class::<FloatFunction>()?;
    m.add_class::<ConstraintBound>()?;
    m.add_class::<Trace>()?;
    m.add_class::<Block>()?;
    m.add_class::<PSSPC>()?;
    m.add_class::<LatticeSurgery>()?;
    m.add_class::<EstimationResult>()?;
    m.add_class::<EstimationCollection>()?;
    m.add_class::<FactoryResult>()?;
    m.add_class::<InstructionFrontier>()?;
    m.add_function(wrap_pyfunction!(constant_function, m)?)?;
    m.add_function(wrap_pyfunction!(linear_function, m)?)?;
    m.add_function(wrap_pyfunction!(block_linear_function, m)?)?;
    m.add_function(wrap_pyfunction!(estimate_parallel, m)?)?;

    m.add("EstimationError", m.py().get_type::<EstimationError>())?;

    add_instruction_ids(m)?;

    Ok(())
}

pyo3::create_exception!(qsharp.qre, EstimationError, PyException);

#[allow(clippy::upper_case_acronyms)]
#[pyclass]
pub struct ISA(qre::ISA);

#[pymethods]
impl ISA {
    #[new]
    #[pyo3(signature = (*instructions))]
    pub fn new(instructions: &Bound<'_, PyTuple>) -> PyResult<ISA> {
        if instructions.len() == 1 {
            let item = instructions.get_item(0)?;
            if let Ok(seq) = item.cast_into::<pyo3::types::PyList>() {
                let mut instrs = Vec::with_capacity(seq.len());
                for item in seq.iter() {
                    let instr = item.cast_into::<Instruction>()?;
                    instrs.push(instr.borrow().0.clone());
                }
                return Ok(ISA(instrs.into_iter().collect()));
            }
        }

        instructions
            .into_iter()
            .map(|instr| {
                let instr = instr.cast_into::<Instruction>()?;
                Ok(instr.borrow().0.clone())
            })
            .collect::<PyResult<qre::ISA>>()
            .map(ISA)
    }

    pub fn __add__(&self, other: &ISA) -> PyResult<ISA> {
        Ok(ISA(self.0.clone() + other.0.clone()))
    }

    pub fn satisfies(&self, requirements: &ISARequirements) -> PyResult<bool> {
        Ok(self.0.satisfies(&requirements.0))
    }

    pub fn __len__(&self) -> usize {
        self.0.len()
    }

    pub fn __getitem__(&self, id: u64) -> PyResult<Instruction> {
        match self.0.get(&id) {
            Some(instr) => Ok(Instruction(instr.clone())),
            None => Err(PyKeyError::new_err(format!(
                "Instruction with id {id} not found"
            ))),
        }
    }

    #[pyo3(signature = (id, default=None))]
    pub fn get(&self, id: u64, default: Option<&Instruction>) -> Option<Instruction> {
        match self.0.get(&id) {
            Some(instr) => Some(Instruction(instr.clone())),
            None => default.cloned(),
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn __iter__(slf: PyRef<'_, Self>) -> PyResult<Py<ISAIterator>> {
        let iter = ISAIterator {
            iter: (*slf.0).clone().into_iter(),
        };
        Py::new(slf.py(), iter)
    }

    fn __str__(&self) -> String {
        format!("{}", self.0)
    }
}

#[pyclass]
pub struct ISAIterator {
    iter: std::collections::hash_map::IntoIter<u64, qre::Instruction>,
}

#[pymethods]
impl ISAIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<Instruction> {
        slf.iter.next().map(|(_, instr)| Instruction(instr))
    }
}

#[pyclass]
pub struct ISARequirements(qre::ISARequirements);

#[pymethods]
impl ISARequirements {
    #[new]
    #[pyo3(signature = (*constraints))]
    pub fn new(constraints: &Bound<'_, PyTuple>) -> PyResult<ISARequirements> {
        if constraints.len() == 1 {
            let item = constraints.get_item(0)?;
            if let Ok(seq) = item.cast::<pyo3::types::PyList>() {
                let mut instrs = Vec::with_capacity(seq.len());
                for item in seq.iter() {
                    let instr = item.cast_into::<Constraint>()?;
                    instrs.push(instr.borrow().0.clone());
                }
                return Ok(ISARequirements(instrs.into_iter().collect()));
            }
        }

        constraints
            .into_iter()
            .map(|instr| {
                let instr = instr.cast_into::<Constraint>()?;
                Ok(instr.borrow().0.clone())
            })
            .collect::<PyResult<qre::ISARequirements>>()
            .map(ISARequirements)
    }
}

#[allow(clippy::unsafe_derive_deserialize)]
#[pyclass]
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Instruction(qre::Instruction);

#[pymethods]
impl Instruction {
    #[staticmethod]
    pub fn fixed_arity(
        id: u64,
        encoding: u64,
        arity: u64,
        time: u64,
        space: Option<u64>,
        length: Option<u64>,
        error_rate: f64,
    ) -> PyResult<Instruction> {
        Ok(Instruction(qre::Instruction::fixed_arity(
            id,
            convert_encoding(encoding)?,
            arity,
            time,
            space,
            length,
            error_rate,
        )))
    }

    #[staticmethod]
    pub fn variable_arity(
        id: u64,
        encoding: u64,
        time_fn: &IntFunction,
        space_fn: &IntFunction,
        error_rate_fn: &FloatFunction,
        length_fn: Option<&IntFunction>,
    ) -> PyResult<Instruction> {
        Ok(Instruction(qre::Instruction::variable_arity(
            id,
            convert_encoding(encoding)?,
            time_fn.0.clone(),
            space_fn.0.clone(),
            length_fn.map(|f| f.0.clone()),
            error_rate_fn.0.clone(),
        )))
    }

    #[getter]
    pub fn id(&self) -> u64 {
        self.0.id()
    }

    #[getter]
    pub fn encoding(&self) -> u64 {
        match self.0.encoding() {
            qre::Encoding::Physical => 0,
            qre::Encoding::Logical => 1,
        }
    }

    #[getter]
    pub fn arity(&self) -> Option<u64> {
        self.0.arity()
    }

    #[pyo3(signature = (arity=None))]
    pub fn space(&self, arity: Option<u64>) -> Option<u64> {
        self.0.space(arity)
    }

    #[pyo3(signature = (arity=None))]
    pub fn time(&self, arity: Option<u64>) -> Option<u64> {
        self.0.time(arity)
    }

    #[pyo3(signature = (arity=None))]
    pub fn error_rate(&self, arity: Option<u64>) -> Option<f64> {
        self.0.error_rate(arity)
    }

    #[pyo3(signature = (arity=None))]
    pub fn expect_space(&self, arity: Option<u64>) -> PyResult<u64> {
        Ok(self.0.expect_space(arity))
    }

    #[pyo3(signature = (arity=None))]
    pub fn expect_time(&self, arity: Option<u64>) -> PyResult<u64> {
        Ok(self.0.expect_time(arity))
    }

    #[pyo3(signature = (arity=None))]
    pub fn expect_error_rate(&self, arity: Option<u64>) -> PyResult<f64> {
        Ok(self.0.expect_error_rate(arity))
    }

    fn __str__(&self) -> String {
        format!("{}", self.0)
    }
}

impl qre::ParetoItem3D for Instruction {
    type Objective1 = u64;
    type Objective2 = u64;
    type Objective3 = f64;

    fn objective1(&self) -> Self::Objective1 {
        self.0.expect_space(None)
    }

    fn objective2(&self) -> Self::Objective2 {
        self.0.expect_time(None)
    }

    fn objective3(&self) -> Self::Objective3 {
        self.0.expect_error_rate(None)
    }
}

#[pyclass]
pub struct Constraint(qre::InstructionConstraint);

#[pymethods]
impl Constraint {
    #[new]
    pub fn new(
        id: u64,
        encoding: u64,
        arity: Option<u64>,
        error_rate: Option<&ConstraintBound>,
    ) -> PyResult<Self> {
        Ok(Constraint(qre::InstructionConstraint::new(
            id,
            convert_encoding(encoding)?,
            arity,
            error_rate.map(|error_rate| error_rate.0),
        )))
    }
}

fn convert_encoding(encoding: u64) -> PyResult<qre::Encoding> {
    match encoding {
        0 => Ok(qre::Encoding::Physical),
        1 => Ok(qre::Encoding::Logical),
        _ => Err(EstimationError::new_err("Invalid encoding value")),
    }
}

#[pyclass]
pub struct ConstraintBound(qre::ConstraintBound<f64>);

#[pymethods]
impl ConstraintBound {
    #[staticmethod]
    pub fn lt(value: f64) -> ConstraintBound {
        ConstraintBound(qre::ConstraintBound::less_than(value))
    }

    #[staticmethod]
    pub fn le(value: f64) -> ConstraintBound {
        ConstraintBound(qre::ConstraintBound::less_equal(value))
    }

    #[staticmethod]
    pub fn eq(value: f64) -> ConstraintBound {
        ConstraintBound(qre::ConstraintBound::equal(value))
    }

    #[staticmethod]
    pub fn gt(value: f64) -> ConstraintBound {
        ConstraintBound(qre::ConstraintBound::greater_than(value))
    }

    #[staticmethod]
    pub fn ge(value: f64) -> ConstraintBound {
        ConstraintBound(qre::ConstraintBound::greater_equal(value))
    }
}

#[pyclass]
pub struct Property(qre::Property);

#[pymethods]
impl Property {
    #[new]
    pub fn new(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if value.is_instance_of::<pyo3::types::PyBool>() {
            Ok(Property(qre::Property::new_bool(value.extract()?)))
        } else if let Ok(i) = value.extract::<i64>() {
            Ok(Property(qre::Property::new_int(i)))
        } else if let Ok(f) = value.extract::<f64>() {
            Ok(Property(qre::Property::new_float(f)))
        } else {
            Ok(Property(qre::Property::new_str(value.to_string())))
        }
    }

    fn as_bool(&self) -> Option<bool> {
        self.0.as_bool()
    }

    fn as_int(&self) -> Option<i64> {
        self.0.as_int()
    }

    fn as_float(&self) -> Option<f64> {
        self.0.as_float()
    }

    fn as_str(&self) -> Option<String> {
        self.0.as_str().map(String::from)
    }

    fn is_bool(&self) -> bool {
        self.0.is_bool()
    }

    fn is_int(&self) -> bool {
        self.0.is_int()
    }

    fn is_float(&self) -> bool {
        self.0.is_float()
    }

    fn is_str(&self) -> bool {
        self.0.is_str()
    }

    fn __str__(&self) -> String {
        format!("{}", self.0)
    }
}

#[pyclass]
pub struct IntFunction(qre::VariableArityFunction<u64>);

#[pyclass]
pub struct FloatFunction(qre::VariableArityFunction<f64>);

#[pyfunction]
pub fn constant_function<'py>(value: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if let Ok(v) = value.extract::<u64>() {
        IntFunction(qre::VariableArityFunction::Constant { value: v }).into_bound_py_any(value.py())
    } else if let Ok(v) = value.extract::<f64>() {
        FloatFunction(qre::VariableArityFunction::Constant { value: v })
            .into_bound_py_any(value.py())
    } else {
        Err(PyTypeError::new_err(
            "Value must be either an integer or a float",
        ))
    }
}

#[pyfunction]
pub fn linear_function<'py>(slope: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
    if let Ok(s) = slope.extract::<u64>() {
        IntFunction(qre::VariableArityFunction::linear(s)).into_bound_py_any(slope.py())
    } else if let Ok(s) = slope.extract::<f64>() {
        FloatFunction(qre::VariableArityFunction::linear(s)).into_bound_py_any(slope.py())
    } else {
        Err(PyTypeError::new_err(
            "Slope must be either an integer or a float",
        ))
    }
}

#[pyfunction]
pub fn block_linear_function<'py>(
    block_size: u64,
    slope: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    if let Ok(s) = slope.extract::<u64>() {
        IntFunction(qre::VariableArityFunction::block_linear(block_size, s))
            .into_bound_py_any(slope.py())
    } else if let Ok(s) = slope.extract::<f64>() {
        FloatFunction(qre::VariableArityFunction::block_linear(block_size, s))
            .into_bound_py_any(slope.py())
    } else {
        Err(PyTypeError::new_err(
            "Slope must be either an integer or a float",
        ))
    }
}

#[derive(Default)]
#[pyclass]
pub struct EstimationCollection(qre::EstimationCollection);

#[pymethods]
impl EstimationCollection {
    #[new]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, result: &EstimationResult) {
        self.0.insert(result.0.clone());
    }

    pub fn __len__(&self) -> usize {
        self.0.len()
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn __iter__(slf: PyRef<'_, Self>) -> PyResult<Py<EstimationCollectionIterator>> {
        let iter = EstimationCollectionIterator {
            iter: slf.0.iter().cloned().collect::<Vec<_>>().into_iter(),
        };
        Py::new(slf.py(), iter)
    }
}

#[pyclass]
pub struct EstimationCollectionIterator {
    iter: std::vec::IntoIter<qre::EstimationResult>,
}

#[pymethods]
impl EstimationCollectionIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<EstimationResult> {
        slf.iter.next().map(EstimationResult)
    }
}

#[pyclass]
pub struct EstimationResult(qre::EstimationResult);

#[pymethods]
impl EstimationResult {
    #[getter]
    pub fn qubits(&self) -> u64 {
        self.0.qubits()
    }

    #[getter]
    pub fn runtime(&self) -> u64 {
        self.0.runtime()
    }

    #[getter]
    pub fn error(&self) -> f64 {
        self.0.error()
    }

    #[allow(clippy::needless_pass_by_value)]
    #[getter]
    pub fn factories(self_: PyRef<'_, Self>) -> PyResult<Bound<'_, PyDict>> {
        let dict = PyDict::new(self_.py());

        for (id, factory) in self_.0.factories() {
            dict.set_item(id, FactoryResult(factory.clone()))?;
        }

        Ok(dict)
    }

    fn __str__(&self) -> String {
        format!("{}", self.0)
    }
}

#[pyclass]
pub struct FactoryResult(qre::FactoryResult);

#[pymethods]
impl FactoryResult {
    #[getter]
    pub fn copies(&self) -> u64 {
        self.0.copies()
    }

    #[getter]
    pub fn runs(&self) -> u64 {
        self.0.runs()
    }

    #[getter]
    pub fn states(&self) -> u64 {
        self.0.states()
    }

    #[getter]
    pub fn error_rate(&self) -> f64 {
        self.0.error_rate()
    }
}

#[pyclass]
pub struct Trace(qre::Trace);

#[pymethods]
impl Trace {
    #[new]
    pub fn new(compute_qubits: u64) -> Self {
        Trace(qre::Trace::new(compute_qubits))
    }

    #[pyo3(signature = (compute_qubits = None))]
    pub fn clone_empty(&self, compute_qubits: Option<u64>) -> Self {
        Trace(self.0.clone_empty(compute_qubits))
    }

    #[getter]
    pub fn compute_qubits(&self) -> u64 {
        self.0.compute_qubits()
    }

    #[getter]
    pub fn base_error(&self) -> f64 {
        self.0.base_error()
    }

    pub fn increment_base_error(&mut self, amount: f64) {
        self.0.increment_base_error(amount);
    }

    pub fn set_property(&mut self, key: String, value: &Property) {
        self.0.set_property(key, value.0.clone());
    }

    pub fn get_property(&self, key: &str) -> Option<Property> {
        self.0.get_property(key).map(|p| Property(p.clone()))
    }

    #[allow(clippy::needless_pass_by_value)]
    #[getter]
    pub fn resource_states(self_: PyRef<'_, Self>) -> PyResult<Bound<'_, PyDict>> {
        let dict = PyDict::new(self_.py());
        if let Some(resource_states) = self_.0.get_resource_states() {
            for (resource_id, count) in resource_states {
                if *count != 0 {
                    dict.set_item(resource_id, *count)?;
                }
            }
        }
        Ok(dict)
    }

    #[getter]
    pub fn depth(&self) -> u64 {
        self.0.depth()
    }

    #[pyo3(signature = (isa, max_error = None))]
    pub fn estimate(&self, isa: &ISA, max_error: Option<f64>) -> Option<EstimationResult> {
        self.0
            .estimate(&isa.0, max_error)
            .map(EstimationResult)
            .ok()
    }

    #[pyo3(signature = (id, qubits, params = vec![]))]
    pub fn add_operation(&mut self, id: u64, qubits: Vec<u64>, params: Vec<f64>) {
        self.0.add_operation(id, qubits, params);
    }

    #[pyo3(signature = (repetitions = 1))]
    pub fn add_block(mut slf: PyRefMut<'_, Self>, repetitions: u64) -> PyResult<Block> {
        let block = slf.0.add_block(repetitions);
        let ptr = NonNull::from(block);
        Ok(Block {
            ptr,
            parent: slf.into(),
        })
    }

    pub fn increment_resource_state(&mut self, resource_id: u64, amount: u64) {
        self.0.increment_resource_state(resource_id, amount);
    }

    fn __str__(&self) -> String {
        format!("{}", self.0)
    }
}

#[pyclass(unsendable)]
pub struct Block {
    ptr: NonNull<qre::Block>,
    #[allow(dead_code)]
    parent: Py<Trace>,
}

#[pymethods]
impl Block {
    #[pyo3(signature = (id, qubits, params = vec![]))]
    pub fn add_operation(&mut self, id: u64, qubits: Vec<u64>, params: Vec<f64>) {
        unsafe { self.ptr.as_mut() }.add_operation(id, qubits, params);
    }

    #[pyo3(signature = (repetitions = 1))]
    pub fn add_block(&mut self, py: Python<'_>, repetitions: u64) -> PyResult<Block> {
        let block = unsafe { self.ptr.as_mut() }.add_block(repetitions);
        let ptr = NonNull::from(block);
        Ok(Block {
            ptr,
            parent: self.parent.clone_ref(py),
        })
    }

    fn __str__(&self) -> String {
        format!("{}", unsafe { self.ptr.as_ref() })
    }
}

#[allow(clippy::upper_case_acronyms)]
#[pyclass]
pub struct PSSPC(qre::PSSPC);

#[pymethods]
impl PSSPC {
    #[new]
    pub fn new(num_ts_per_rotation: u64, ccx_magic_states: bool) -> Self {
        PSSPC(qre::PSSPC::new(num_ts_per_rotation, ccx_magic_states))
    }

    pub fn transform(&self, trace: &Trace) -> PyResult<Trace> {
        self.0
            .transform(&trace.0)
            .map(Trace)
            .map_err(|e| EstimationError::new_err(format!("{e}")))
    }
}

#[derive(Default)]
#[pyclass]
pub struct LatticeSurgery(qre::LatticeSurgery);

#[pymethods]
impl LatticeSurgery {
    #[new]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn transform(&self, trace: &Trace) -> PyResult<Trace> {
        self.0
            .transform(&trace.0)
            .map(Trace)
            .map_err(|e| EstimationError::new_err(format!("{e}")))
    }
}

#[pyclass]
pub struct InstructionFrontier(qre::ParetoFrontier3D<Instruction>);

impl Default for InstructionFrontier {
    fn default() -> Self {
        InstructionFrontier(qre::ParetoFrontier3D::new())
    }
}

#[pymethods]
impl InstructionFrontier {
    #[new]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, point: &Instruction) {
        self.0.insert(point.clone());
    }

    pub fn __len__(&self) -> usize {
        self.0.len()
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn __iter__(slf: PyRef<'_, Self>) -> PyResult<Py<InstructionFrontierIterator>> {
        let iter = InstructionFrontierIterator {
            iter: slf.0.iter().cloned().collect::<Vec<_>>().into_iter(),
        };
        Py::new(slf.py(), iter)
    }

    #[staticmethod]
    pub fn load(filename: &str) -> PyResult<Self> {
        let content = std::fs::read_to_string(filename)?;
        let frontier =
            serde_json::from_str(&content).map_err(|e| EstimationError::new_err(format!("{e}")))?;
        Ok(InstructionFrontier(frontier))
    }

    pub fn dump(&self, filename: &str) -> PyResult<()> {
        let content =
            serde_json::to_string(&self.0).map_err(|e| EstimationError::new_err(format!("{e}")))?;
        Ok(std::fs::write(filename, content)?)
    }
}

#[pyclass]
pub struct InstructionFrontierIterator {
    iter: std::vec::IntoIter<Instruction>,
}

#[pymethods]
impl InstructionFrontierIterator {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>) -> Option<Instruction> {
        slf.iter.next()
    }
}

#[allow(clippy::needless_pass_by_value)]
#[pyfunction(signature = (traces, isas, max_error = 1.0))]
pub fn estimate_parallel(
    traces: Vec<PyRef<'_, Trace>>,
    isas: Vec<PyRef<'_, ISA>>,
    max_error: f64,
) -> EstimationCollection {
    let traces: Vec<_> = traces.iter().map(|t| &t.0).collect();
    let isas: Vec<_> = isas.iter().map(|i| &i.0).collect();

    let collection = qre::estimate_parallel(&traces, &isas, Some(max_error));
    EstimationCollection(collection)
}

fn add_instruction_ids(m: &Bound<'_, PyModule>) -> PyResult<()> {
    #[allow(clippy::wildcard_imports)]
    use qre::instruction_ids::*;

    let instruction_ids = PyModule::new(m.py(), "instruction_ids")?;

    macro_rules! add_ids {
        ($($name:ident),* $(,)?) => {
            $(instruction_ids.add(stringify!($name), $name)?;)*
        };
    }

    add_ids!(
        PAULI_I,
        PAULI_X,
        PAULI_Y,
        PAULI_Z,
        H,
        H_XZ,
        H_XY,
        H_YZ,
        SQRT_X,
        SQRT_X_DAG,
        SQRT_Y,
        SQRT_Y_DAG,
        S,
        SQRT_Z,
        S_DAG,
        SQRT_Z_DAG,
        CNOT,
        CX,
        CY,
        CZ,
        SWAP,
        PREP_X,
        PREP_Y,
        PREP_Z,
        ONE_QUBIT_CLIFFORD,
        TWO_QUBIT_CLIFFORD,
        N_QUBIT_CLIFFORD,
        MEAS_X,
        MEAS_Y,
        MEAS_Z,
        MEAS_RESET_X,
        MEAS_RESET_Y,
        MEAS_RESET_Z,
        MEAS_XX,
        MEAS_YY,
        MEAS_ZZ,
        MEAS_XZ,
        MEAS_XY,
        MEAS_YZ,
        SQRT_SQRT_X,
        SQRT_SQRT_X_DAG,
        SQRT_SQRT_Y,
        SQRT_SQRT_Y_DAG,
        SQRT_SQRT_Z,
        T,
        SQRT_SQRT_Z_DAG,
        T_DAG,
        CCX,
        CCY,
        CCZ,
        CSWAP,
        AND,
        AND_DAG,
        RX,
        RY,
        RZ,
        CRX,
        CRY,
        CRZ,
        RXX,
        RYY,
        RZZ,
        MULTI_PAULI_MEAS,
        LATTICE_SURGERY,
        READ_FROM_MEMORY,
        WRITE_TO_MEMORY,
        CYCLIC_SHIFT,
        GENERIC
    );

    m.add_submodule(&instruction_ids)?;

    Ok(())
}
