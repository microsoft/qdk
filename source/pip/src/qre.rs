// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use pyo3::{IntoPyObjectExt, prelude::*, types::PyTuple};

pub(crate) fn register_qre_submodule(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<ISA>()?;
    m.add_class::<ISARequirements>()?;
    m.add_class::<Instruction>()?;
    m.add_class::<Constraint>()?;
    m.add_class::<IntFunction>()?;
    m.add_class::<FloatFunction>()?;
    m.add_class::<ConstraintBound>()?;
    m.add_function(wrap_pyfunction!(constant_function, m)?)?;
    m.add_function(wrap_pyfunction!(linear_function, m)?)?;
    m.add_function(wrap_pyfunction!(block_linear_function, m)?)?;
    Ok(())
}

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
            None => Err(PyErr::new::<pyo3::exceptions::PyKeyError, _>(format!(
                "Instruction with id {id} not found"
            ))),
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

#[pyclass]
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
        _ => Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "Invalid encoding value",
        )),
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
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
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
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
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
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "Slope must be either an integer or a float",
        ))
    }
}
