// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This module is a thin `PyO3` wrapper around the rust `noisy_simulator` crate.

use noisy_simulator::{ComplexVector, NoisySimulator, SquareMatrix};
use num_complex::Complex;
use pyo3::{exceptions::PyException, prelude::*};
type PythonMatrix = Vec<Vec<Complex<f64>>>;

pub(crate) fn register_noisy_simulator_submodule<'a>(
    py: Python<'a>,
    m: &Bound<'a, PyModule>,
) -> PyResult<()> {
    m.add("NoisySimulatorError", py.get_type::<NoisySimulatorError>())?;
    m.add_class::<Operation>()?;
    m.add_class::<Instrument>()?;
    m.add_class::<DensityMatrixSimulator>()?;
    m.add_class::<StateVectorSimulator>()?;
    Ok(())
}

/// Performance Warning:
///  nalgebra stores its matrices in column major order, and we want to send
///  them from Python in row major order, this means that there will be lots of
///  cache-misses in the conversion from one format to another.
///
///  This function is only used on a non-critical path for performance. Namely,
///  the input to the simulator to set it up, and getting the final output.
///  This warning is just to avoid any performance accidents in the future.
fn python_matrix_to_nalgebra_matrix(matrix: PythonMatrix) -> PyResult<SquareMatrix> {
    let nrows = matrix.len();
    let ncols = matrix[0].len();
    // Check that matrix is well formed.
    for row in &matrix {
        if ncols != row.len() {
            return Err(NoisySimulatorError::new_err(
                "ill formed matrix, all rows should be the same length".to_string(),
            ));
        }
    }
    // Move matrix into a linear container.
    let mut data = Vec::with_capacity(nrows * ncols);
    for mut row in matrix {
        data.append(&mut row);
    }
    Ok(SquareMatrix::from_row_iterator(nrows, ncols, data))
}

fn nalgebra_matrix_to_python_matrix(matrix: &SquareMatrix) -> PythonMatrix {
    // Performance note: Because of the performance optimization in
    // noisy_simulator/src/operation.rs/Operation::new, the simulator stores its matrices
    // transposed. When we give them back to python, we need to transpose them back,
    // that's why we `python_row.extend(nalgebra_col.iter())`.

    let (nrows, ncols) = matrix.shape();
    let mut python_list = Vec::with_capacity(ncols);

    for nalgebra_col in matrix.column_iter() {
        let mut python_row = Vec::with_capacity(nrows);
        python_row.extend(nalgebra_col.iter());
        python_list.push(python_row);
    }

    python_list
}

pyo3::create_exception!(qsharp.noisy_sim, NoisySimulatorError, PyException);

#[pyclass]
#[derive(Clone)]
pub(crate) struct Operation(noisy_simulator::Operation);

#[pymethods]
impl Operation {
    #[new]
    pub fn new(kraus_operators: Vec<PythonMatrix>) -> PyResult<Self> {
        let kraus_operators: PyResult<Vec<SquareMatrix>> = kraus_operators
            .into_iter()
            .map(python_matrix_to_nalgebra_matrix)
            .collect();
        noisy_simulator::Operation::new(kraus_operators?)
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
            .map(Self)
    }

    pub fn get_effect_matrix(&self) -> PythonMatrix {
        nalgebra_matrix_to_python_matrix(self.0.effect_matrix())
    }

    pub fn get_operation_matrix(&self) -> PythonMatrix {
        nalgebra_matrix_to_python_matrix(self.0.matrix())
    }

    pub fn get_kraus_operators(&self) -> Vec<PythonMatrix> {
        let mut kraus_operators = Vec::new();
        for kraus_operator in self.0.kraus_operators() {
            kraus_operators.push(nalgebra_matrix_to_python_matrix(kraus_operator));
        }
        kraus_operators
    }

    pub fn get_number_of_qubits(&self) -> usize {
        self.0.number_of_qubits()
    }
}

#[pyclass]
pub(crate) struct Instrument(noisy_simulator::Instrument);

#[pymethods]
impl Instrument {
    #[new]
    pub fn new(operations: Vec<Operation>) -> PyResult<Self> {
        let operations = operations.into_iter().map(|op| op.0).collect();
        noisy_simulator::Instrument::new(operations)
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
            .map(Self)
    }
}

#[pyclass]
#[derive(Clone)]
pub(crate) struct DensityMatrix {
    /// Dimension of the matrix. E.g.: If the matrix is 5 x 5, then dimension is 5.
    dimension: usize,
    /// Number of qubits in the system.
    number_of_qubits: usize,
    /// Theoretical change in trace due to operations that have been applied so far.
    trace_change: f64,
    /// Vector storing the entries of the density matrix.
    data: Vec<Complex<f64>>,
}

#[pymethods]
impl DensityMatrix {
    /// Returns a copy of the matrix data.
    fn data(&self) -> Vec<Vec<Complex<f64>>> {
        let mut density_matrix = Vec::with_capacity(self.dimension);
        for row in 0..self.dimension {
            let mut row_vec = Vec::with_capacity(self.dimension);
            for col in 0..self.dimension {
                row_vec.push(self.data[row * self.dimension + col]);
            }
            density_matrix.push(row_vec);
        }
        density_matrix
    }

    /// Returns the dimension of the matrix.
    fn dimension(&self) -> usize {
        self.dimension
    }

    /// Returns the number of qubits in the system.
    fn number_of_qubits(&self) -> usize {
        self.number_of_qubits
    }
}

impl From<&noisy_simulator::DensityMatrix> for DensityMatrix {
    fn from(dm: &noisy_simulator::DensityMatrix) -> Self {
        Self {
            dimension: dm.dimension(),
            number_of_qubits: dm.number_of_qubits(),
            trace_change: dm.trace_change(),
            data: dm.data().iter().copied().collect::<Vec<_>>(),
        }
    }
}

impl TryInto<noisy_simulator::DensityMatrix> for DensityMatrix {
    type Error = PyErr;

    fn try_into(self) -> PyResult<noisy_simulator::DensityMatrix> {
        noisy_simulator::DensityMatrix::try_from(
            self.dimension,
            self.number_of_qubits,
            self.trace_change,
            ComplexVector::from_vec(self.data),
        )
        .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }
}

#[pyclass]
pub(crate) struct DensityMatrixSimulator(noisy_simulator::DensityMatrixSimulator);

#[pymethods]
impl DensityMatrixSimulator {
    #[new]
    #[pyo3(signature=(number_of_qubits, seed=None))]
    pub fn new(number_of_qubits: usize, seed: Option<u64>) -> Self {
        if let Some(seed) = seed {
            Self(noisy_simulator::DensityMatrixSimulator::new_with_seed(
                number_of_qubits,
                seed,
            ))
        } else {
            Self(noisy_simulator::DensityMatrixSimulator::new(
                number_of_qubits,
            ))
        }
    }

    /// Apply an arbitrary operation to given qubit ids.
    #[allow(clippy::needless_pass_by_value)]
    pub fn apply_operation(&mut self, operation: &Operation, qubits: Vec<usize>) -> PyResult<()> {
        self.0
            .apply_operation(&operation.0, &qubits)
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }

    /// Apply non selective evolution.
    #[allow(clippy::needless_pass_by_value)]
    pub fn apply_instrument(
        &mut self,
        instrument: &Instrument,
        qubits: Vec<usize>,
    ) -> PyResult<()> {
        self.0
            .apply_instrument(&instrument.0, &qubits)
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }

    /// Performs selective evolution under the given instrument.
    /// Returns the index of the observed outcome.
    ///
    /// Use this method to perform measurements on the quantum system.
    #[allow(clippy::needless_pass_by_value)]
    pub fn sample_instrument(
        &mut self,
        instrument: &Instrument,
        qubits: Vec<usize>,
    ) -> PyResult<usize> {
        self.0
            .sample_instrument(&instrument.0, &qubits)
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }

    /// Returns the `DensityMatrix` if the simulator is in a valid state.
    pub fn get_state(&self) -> Option<DensityMatrix> {
        match self.0.state() {
            Ok(dm) => Some(dm.into()),
            Err(_) => None,
        }
    }

    /// Set state of the quantum system.
    pub fn set_state(&mut self, state: DensityMatrix) -> PyResult<()> {
        self.0
            .set_state(state.try_into()?)
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }

    /// Set the trace of the quantum system.
    pub fn set_trace(&mut self, trace: f64) -> PyResult<()> {
        self.0
            .set_trace(trace)
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }
}

#[pyclass]
#[derive(Clone)]
pub(crate) struct StateVector {
    /// Dimension of the matrix. E.g.: If the matrix is 5 x 5, then dimension is 5.
    dimension: usize,
    /// Number of qubits in the system.
    number_of_qubits: usize,
    /// Theoretical change in trace due to operations that have been applied so far.
    trace_change: f64,
    /// Vector storing the entries of the density matrix.
    data: Vec<Complex<f64>>,
}

#[pymethods]
impl StateVector {
    /// Returns a copy of the matrix data.
    fn data(&self) -> Vec<Complex<f64>> {
        self.data.clone()
    }

    /// Returns the dimension of the matrix.
    fn dimension(&self) -> usize {
        self.dimension
    }

    /// Returns the number of qubits in the system.
    fn number_of_qubits(&self) -> usize {
        self.number_of_qubits
    }
}

impl From<&noisy_simulator::StateVector> for StateVector {
    fn from(dm: &noisy_simulator::StateVector) -> Self {
        Self {
            dimension: dm.dimension(),
            number_of_qubits: dm.number_of_qubits(),
            trace_change: dm.trace_change(),
            data: dm.data().iter().copied().collect::<Vec<_>>(),
        }
    }
}

impl TryInto<noisy_simulator::StateVector> for StateVector {
    type Error = PyErr;

    fn try_into(self) -> PyResult<noisy_simulator::StateVector> {
        noisy_simulator::StateVector::try_from(
            self.dimension,
            self.number_of_qubits,
            self.trace_change,
            ComplexVector::from_vec(self.data),
        )
        .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }
}

#[pyclass]
pub(crate) struct StateVectorSimulator(noisy_simulator::StateVectorSimulator);

#[pymethods]
impl StateVectorSimulator {
    #[new]
    #[pyo3(signature=(number_of_qubits, seed=None))]
    pub fn new(number_of_qubits: usize, seed: Option<u64>) -> Self {
        if let Some(seed) = seed {
            Self(noisy_simulator::StateVectorSimulator::new_with_seed(
                number_of_qubits,
                seed,
            ))
        } else {
            Self(noisy_simulator::StateVectorSimulator::new(number_of_qubits))
        }
    }

    /// Apply an arbitrary operation to given qubit ids.
    #[allow(clippy::needless_pass_by_value)]
    pub fn apply_operation(&mut self, operation: &Operation, qubits: Vec<usize>) -> PyResult<()> {
        self.0
            .apply_operation(&operation.0, &qubits)
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }

    /// Apply non selective evolution.
    #[allow(clippy::needless_pass_by_value)]
    pub fn apply_instrument(
        &mut self,
        instrument: &Instrument,
        qubits: Vec<usize>,
    ) -> PyResult<()> {
        self.0
            .apply_instrument(&instrument.0, &qubits)
            .map(|_| ())
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }

    /// Performs selective evolution under the given instrument.
    /// Returns the index of the observed outcome.
    ///
    /// Use this method to perform measurements on the quantum system.
    #[allow(clippy::needless_pass_by_value)]
    pub fn sample_instrument(
        &mut self,
        instrument: &Instrument,
        qubits: Vec<usize>,
    ) -> PyResult<usize> {
        self.0
            .sample_instrument(&instrument.0, &qubits)
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }

    /// Returns the `StateVector` if the simulator is in a valid state.
    pub fn get_state(&self) -> Option<StateVector> {
        match self.0.state() {
            Ok(dm) => Some(dm.into()),
            Err(_) => None,
        }
    }

    /// Set state of the quantum system.
    pub fn set_state(&mut self, state: StateVector) -> PyResult<()> {
        self.0
            .set_state(state.try_into()?)
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }

    /// Set the trace of the quantum system.
    pub fn set_trace(&mut self, trace: f64) -> PyResult<()> {
        self.0
            .set_trace(trace)
            .map_err(|e| NoisySimulatorError::new_err(e.to_string()))
    }
}
