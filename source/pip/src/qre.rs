// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{ptr::NonNull, sync::Arc};

use pyo3::{
    IntoPyObjectExt,
    exceptions::{PyException, PyKeyError, PyTypeError},
    prelude::*,
    types::{PyBool, PyDict, PyFloat, PyInt, PyString, PyTuple},
};
use qre::TraceTransform;
use serde::{Deserialize, Serialize};

pub(crate) fn register_qre_submodule(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ISA>()?;
    m.add_class::<ISARequirements>()?;
    m.add_class::<Instruction>()?;
    m.add_class::<Constraint>()?;
    m.add_class::<IntFunction>()?;
    m.add_class::<FloatFunction>()?;
    m.add_class::<ConstraintBound>()?;
    m.add_class::<ProvenanceGraph>()?;
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
    m.add_function(wrap_pyfunction!(generic_function, m)?)?;
    m.add_function(wrap_pyfunction!(estimate_parallel, m)?)?;
    m.add_function(wrap_pyfunction!(binom_ppf, m)?)?;
    m.add_function(wrap_pyfunction!(instruction_name, m)?)?;

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

    pub fn append(&mut self, instruction: &Instruction) {
        self.0.add_instruction(instruction.0.clone());
    }

    pub fn __add__(&self, other: &ISA) -> PyResult<ISA> {
        Ok(ISA(self.0.clone() + other.0.clone()))
    }

    pub fn __contains__(&self, id: u64) -> bool {
        self.0.contains(&id)
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
#[pyclass(name = "_Instruction")]
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

    pub fn with_id(&self, id: u64) -> Self {
        Instruction(self.0.with_id(id))
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

    pub fn set_source(&mut self, index: usize) {
        self.0.set_source(index);
    }

    #[getter]
    pub fn source(&self) -> usize {
        self.0.source()
    }

    pub fn set_property(&mut self, key: u64, value: u64) {
        self.0.set_property(key, value);
    }

    pub fn get_property(&self, key: u64) -> Option<u64> {
        self.0.get_property(&key)
    }

    pub fn has_property(&self, key: u64) -> bool {
        self.0.has_property(&key)
    }

    #[pyo3(signature = (key, default))]
    pub fn get_property_or(&self, key: u64, default: u64) -> u64 {
        self.0.get_property_or(&key, default)
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

    pub fn add_property(&mut self, property: u64) {
        self.0.add_property(property);
    }

    pub fn has_property(&self, property: u64) -> bool {
        self.0.has_property(&property)
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

#[derive(Default)]
#[pyclass(name = "_ProvenanceGraph")]
pub struct ProvenanceGraph(qre::ProvenanceGraph);

#[pymethods]
impl ProvenanceGraph {
    #[new]
    pub fn new() -> Self {
        Self(qre::ProvenanceGraph::new())
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn add_node(&mut self, id: u64, transform: u64, children: Vec<usize>) -> usize {
        self.0.add_node(id, transform, &children)
    }

    pub fn instruction_id(&self, node_index: usize) -> u64 {
        self.0.instruction_id(node_index)
    }

    pub fn transform_id(&self, node_index: usize) -> u64 {
        self.0.transform_id(node_index)
    }

    pub fn children(&self, node_index: usize) -> Vec<usize> {
        self.0.children(node_index).to_vec()
    }

    pub fn num_nodes(&self) -> usize {
        self.0.num_nodes()
    }

    pub fn num_edges(&self) -> usize {
        self.0.num_edges()
    }
}

#[pyclass(name = "_IntFunction")]
pub struct IntFunction(qre::VariableArityFunction<u64>);

#[pyclass(name = "_FloatFunction")]
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

#[pyfunction]
pub fn generic_function<'py>(
    py: Python<'py>,
    func: Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    // Try to get return type annotation from the function
    let is_int = if let Ok(annotations) = func.getattr("__annotations__") {
        if let Ok(return_type) = annotations.get_item("return") {
            // Check if return type is float
            let float_type = py.get_type::<pyo3::types::PyInt>();
            return_type.eq(float_type).unwrap_or(false)
        } else {
            false
        }
    } else {
        false
    };

    let func: Py<PyAny> = func.unbind();

    if is_int {
        let closure = move |arity: u64| -> u64 {
            Python::attach(|py| {
                let result = func.call1(py, (arity,));
                match result {
                    Ok(value) => value.extract::<u64>(py).unwrap_or(0),
                    Err(_) => 0,
                }
            })
        };

        let arc: Arc<dyn Fn(u64) -> u64 + Send + Sync> = Arc::new(closure);
        IntFunction(qre::VariableArityFunction::generic_from_arc(arc)).into_bound_py_any(py)
    } else {
        let closure = move |arity: u64| -> f64 {
            Python::attach(|py| {
                let result = func.call1(py, (arity,));
                match result {
                    Ok(value) => value.extract::<f64>(py).unwrap_or(0.0),
                    Err(_) => 0.0,
                }
            })
        };

        let arc: Arc<dyn Fn(u64) -> f64 + Send + Sync> = Arc::new(closure);
        FloatFunction(qre::VariableArityFunction::generic_from_arc(arc)).into_bound_py_any(py)
    }
}

#[derive(Default)]
#[pyclass(name = "_EstimationCollection")]
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

    #[getter]
    pub fn isa(&self) -> ISA {
        ISA(self.0.isa().clone())
    }

    #[allow(clippy::needless_pass_by_value)]
    #[getter]
    pub fn properties(self_: PyRef<'_, Self>) -> PyResult<Bound<'_, PyDict>> {
        let dict = PyDict::new(self_.py());

        for (key, value) in self_.0.properties() {
            match value {
                qre::Property::Bool(b) => dict.set_item(key, *b)?,
                qre::Property::Int(i) => dict.set_item(key, *i)?,
                qre::Property::Float(f) => dict.set_item(key, *f)?,
                qre::Property::Str(s) => dict.set_item(key, s.clone())?,
            }
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

    pub fn set_property(&mut self, key: String, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let property = if value.is_instance_of::<pyo3::types::PyBool>() {
            qre::Property::new_bool(value.extract()?)
        } else if let Ok(i) = value.extract::<i64>() {
            qre::Property::new_int(i)
        } else if let Ok(f) = value.extract::<f64>() {
            qre::Property::new_float(f)
        } else {
            qre::Property::new_str(value.to_string())
        };

        self.0.set_property(key, property);

        Ok(())
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn get_property<'py>(self_: PyRef<'py, Self>, key: &str) -> Option<Bound<'py, PyAny>> {
        if let Some(value) = self_.0.get_property(key) {
            match value {
                qre::Property::Bool(b) => PyBool::new(self_.py(), *b)
                    .into_bound_py_any(self_.py())
                    .ok(),
                qre::Property::Int(i) => PyInt::new(self_.py(), *i)
                    .into_bound_py_any(self_.py())
                    .ok(),
                qre::Property::Float(f) => PyFloat::new(self_.py(), *f)
                    .into_bound_py_any(self_.py())
                    .ok(),
                qre::Property::Str(s) => PyString::new(self_.py(), s)
                    .into_bound_py_any(self_.py())
                    .ok(),
            }
        } else {
            None
        }
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

    #[getter]
    pub fn memory_qubits(&self) -> Option<u64> {
        self.0.memory_qubits()
    }

    pub fn has_memory_qubits(&self) -> bool {
        self.0.has_memory_qubits()
    }

    pub fn set_memory_qubits(&mut self, qubits: u64) {
        self.0.set_memory_qubits(qubits);
    }

    pub fn increment_memory_qubits(&mut self, amount: u64) {
        self.0.increment_memory_qubits(amount);
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
    pub fn new(slow_down_factor: f64) -> Self {
        Self(qre::LatticeSurgery::new(slow_down_factor))
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

    #[allow(clippy::needless_pass_by_value)]
    pub fn extend(&mut self, points: Vec<PyRef<'_, Instruction>>) {
        self.0
            .extend(points.iter().map(|p| Instruction(p.0.clone())));
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
#[pyfunction(name = "_estimate_parallel", signature = (traces, isas, max_error = 1.0))]
pub fn estimate_parallel(
    py: Python<'_>,
    traces: Vec<PyRef<'_, Trace>>,
    isas: Vec<PyRef<'_, ISA>>,
    max_error: f64,
) -> EstimationCollection {
    let traces: Vec<_> = traces.iter().map(|t| &t.0).collect();
    let isas: Vec<_> = isas.iter().map(|i| &i.0).collect();

    // Release the GIL before entering the parallel section.
    // Worker threads spawned by qre::estimate_parallel may need to acquire
    // the GIL to evaluate Python callbacks (via generic_function closures).
    // If the calling thread holds the GIL while blocked in
    // std::thread::scope, the worker threads deadlock.
    let collection = release_gil(py, || {
        qre::estimate_parallel(&traces, &isas, Some(max_error))
    });
    EstimationCollection(collection)
}

/// Releases the GIL for the duration of the closure `f`, allowing other
/// threads to acquire it.  A RAII guard ensures the thread state is restored
/// even if `f` panics.
///
/// # Safety
///
/// The caller must ensure that no `Bound<'_, _>` or `Python<'_>` references
/// are used inside `f`.  GIL-independent `Py<T>` handles are fine because
/// they re-acquire the GIL via `Python::attach` when needed.
///
/// We cannot use `py.allow_threads` here because the captured data
/// (`&qre::ISA`) transitively contains `Arc<dyn Fn + Send + Sync>` whose
/// trait object does not carry the `Ungil` auto-trait bound.
fn release_gil<F, R>(_py: Python<'_>, f: F) -> R
where
    F: FnOnce() -> R,
{
    struct RestoreGuard(*mut pyo3::ffi::PyThreadState);

    impl Drop for RestoreGuard {
        fn drop(&mut self) {
            // SAFETY: called on the same thread that saved the state.
            unsafe { pyo3::ffi::PyEval_RestoreThread(self.0) };
        }
    }

    // SAFETY: we hold the GIL (proven by the `_py` token) and release it
    // here so that worker threads can acquire it for Python callbacks.
    let _guard = RestoreGuard(unsafe { pyo3::ffi::PyEval_SaveThread() });
    f()
}

#[pyfunction(name = "_binom_ppf")]
pub fn binom_ppf(q: f64, n: usize, p: f64) -> usize {
    qre::binom_ppf(q, n, p)
}

#[pyfunction]
pub fn instruction_name(id: u64) -> Option<String> {
    qre::instruction_name(id).map(String::from)
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
        ONE_QUBIT_UNITARY,
        TWO_QUBIT_UNITARY,
        MULTI_PAULI_MEAS,
        LATTICE_SURGERY,
        READ_FROM_MEMORY,
        WRITE_TO_MEMORY,
        MEMORY,
        CYCLIC_SHIFT,
        GENERIC
    );

    m.add_submodule(&instruction_ids)?;

    Ok(())
}
