// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use pyo3::{Bound, FromPyObject, Py, PyRef, PyResult, Python, pyclass, pymethods};

pub(crate) mod clifford;
pub(crate) mod gpu_full_state;

#[derive(Clone, Copy, Debug, PartialEq)]
#[pyclass(eq, eq_int)]
pub enum QirInstructionId {
    I,
    H,
    X,
    Y,
    Z,
    S,
    SAdj,
    SX,
    SXAdj,
    T,
    TAdj,
    CNOT,
    CX,
    CY,
    CZ,
    CCX,
    SWAP,
    RX,
    RY,
    RZ,
    RXX,
    RYY,
    RZZ,
    RESET,
    M,
    MResetZ,
    MZ,
    ReadResult,
    ResultRecordOutput,
    BoolRecordOutput,
    IntRecordOutput,
    DoubleRecordOutput,
    TupleRecordOutput,
    ArrayRecordOutput,
}

#[derive(Debug)]
#[pyclass(module = "qsharp._native")]
#[derive(FromPyObject)]
pub enum QirInstruction {
    OneQubitGate(QirInstructionId, u32),
    TwoQubitGate(QirInstructionId, u32, u32),
    OneQubitRotationGate(QirInstructionId, f64, u32),
    TwoQubitRotationGate(QirInstructionId, f64, u32, u32),
    ThreeQubitGate(QirInstructionId, u32, u32, u32),
    OutputRecording(QirInstructionId, String, String), // inst, value, tag
}

#[derive(Debug)]
#[pyclass(module = "qsharp._native")]
pub struct NoiseConfig {
    #[pyo3(get)]
    pub x: Py<NoiseTable>,
    #[pyo3(get)]
    pub y: Py<NoiseTable>,
    #[pyo3(get)]
    pub z: Py<NoiseTable>,
    #[pyo3(get)]
    pub h: Py<NoiseTable>,
    #[pyo3(get)]
    pub s: Py<NoiseTable>,
    #[pyo3(get)]
    pub s_adj: Py<NoiseTable>,
    #[pyo3(get)]
    pub sx: Py<NoiseTable>,
    #[pyo3(get)]
    pub cz: Py<NoiseTable>,
    #[pyo3(get)]
    pub mov: Py<NoiseTable>,
    #[pyo3(get)]
    pub mresetz: Py<NoiseTable>,
    #[pyo3(get)]
    pub idle: Py<IdleNoiseParams>,
}

#[pymethods]
impl NoiseConfig {
    #[new]
    fn new(py: Python) -> PyResult<Self> {
        bind_noise_config(
            py,
            qdk_simulators::stabilizer_simulator::NoiseConfig::NOISELESS,
        )
    }
}

fn bind_noise_config(
    py: Python,
    value: qdk_simulators::stabilizer_simulator::NoiseConfig,
) -> PyResult<NoiseConfig> {
    Ok(NoiseConfig {
        x: Py::new(py, NoiseTable::from(value.x))?,
        y: Py::new(py, NoiseTable::from(value.y))?,
        z: Py::new(py, NoiseTable::from(value.z))?,
        h: Py::new(py, NoiseTable::from(value.h))?,
        s: Py::new(py, NoiseTable::from(value.s))?,
        s_adj: Py::new(py, NoiseTable::from(value.s_adj))?,
        sx: Py::new(py, NoiseTable::from(value.sx))?,
        cz: Py::new(py, NoiseTable::from(value.cz))?,
        mov: Py::new(py, NoiseTable::from(value.mov))?,
        mresetz: Py::new(py, NoiseTable::from(value.mresetz))?,
        idle: Py::new(py, IdleNoiseParams::from(value.idle))?,
    })
}

fn unbind_noise_config(
    py: Python,
    value: &Bound<NoiseConfig>,
) -> qdk_simulators::stabilizer_simulator::NoiseConfig {
    let value = value.borrow();
    qdk_simulators::stabilizer_simulator::NoiseConfig {
        x: from_noise_table_ref(&value.x.borrow(py)),
        y: from_noise_table_ref(&value.y.borrow(py)),
        z: from_noise_table_ref(&value.z.borrow(py)),
        h: from_noise_table_ref(&value.h.borrow(py)),
        s: from_noise_table_ref(&value.s.borrow(py)),
        s_adj: from_noise_table_ref(&value.s_adj.borrow(py)),
        sx: from_noise_table_ref(&value.sx.borrow(py)),
        cz: from_noise_table_ref(&value.cz.borrow(py)),
        mov: from_noise_table_ref(&value.mov.borrow(py)),
        mresetz: from_noise_table_ref(&value.mresetz.borrow(py)),
        idle: from_idle_noise_params_ref(&value.idle.borrow(py)),
    }
}

#[derive(Clone, Copy, Debug)]
#[pyclass(module = "qsharp._native")]
pub struct IdleNoiseParams {
    #[pyo3(get, set)]
    pub s_probability: f32,
}

#[pymethods]
impl IdleNoiseParams {
    #[new]
    fn new() -> Self {
        IdleNoiseParams { s_probability: 0.0 }
    }
}

impl From<IdleNoiseParams> for qdk_simulators::stabilizer_simulator::IdleNoiseParams {
    fn from(value: IdleNoiseParams) -> Self {
        qdk_simulators::stabilizer_simulator::IdleNoiseParams {
            s_probability: value.s_probability,
        }
    }
}

fn from_idle_noise_params_ref(
    value: &PyRef<'_, IdleNoiseParams>,
) -> qdk_simulators::stabilizer_simulator::IdleNoiseParams {
    qdk_simulators::stabilizer_simulator::IdleNoiseParams {
        s_probability: value.s_probability,
    }
}

impl From<qdk_simulators::stabilizer_simulator::IdleNoiseParams> for IdleNoiseParams {
    fn from(value: qdk_simulators::stabilizer_simulator::IdleNoiseParams) -> Self {
        IdleNoiseParams {
            s_probability: value.s_probability,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[pyclass(module = "qsharp._native")]
pub struct NoiseTable {
    #[pyo3(get, set)]
    pub x: f32,
    #[pyo3(get, set)]
    pub y: f32,
    #[pyo3(get, set)]
    pub z: f32,
    #[pyo3(get, set)]
    pub loss: f32,
}

#[pymethods]
impl NoiseTable {
    #[new]
    fn new() -> Self {
        NoiseTable {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            loss: 0.0,
        }
    }
}

impl From<NoiseTable> for qdk_simulators::stabilizer_simulator::NoiseTable {
    fn from(value: NoiseTable) -> Self {
        qdk_simulators::stabilizer_simulator::NoiseTable {
            x: value.x,
            y: value.y,
            z: value.z,
            loss: value.loss,
        }
    }
}

fn from_noise_table_ref(
    value: &PyRef<'_, NoiseTable>,
) -> qdk_simulators::stabilizer_simulator::NoiseTable {
    qdk_simulators::stabilizer_simulator::NoiseTable {
        x: value.x,
        y: value.y,
        z: value.z,
        loss: value.loss,
    }
}

impl From<qdk_simulators::stabilizer_simulator::NoiseTable> for NoiseTable {
    fn from(value: qdk_simulators::stabilizer_simulator::NoiseTable) -> Self {
        NoiseTable {
            x: value.x,
            y: value.y,
            z: value.z,
            loss: value.loss,
        }
    }
}
