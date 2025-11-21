// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use pyo3::{
    Bound, FromPyObject, Py, PyRef, PyResult, Python, exceptions::PyValueError, pyclass, pymethods,
};

pub(crate) mod clifford;
pub(crate) mod cpu_full_state;
pub(crate) mod gpu_full_state;

#[allow(
    clippy::upper_case_acronyms,
    reason = "these gates are named as in the rest of our stack"
)]
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
    Move,
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
    pub i: Py<NoiseTable>,
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
    pub t: Py<NoiseTable>,
    #[pyo3(get)]
    pub t_adj: Py<NoiseTable>,
    #[pyo3(get)]
    pub sx: Py<NoiseTable>,
    #[pyo3(get)]
    pub sx_adj: Py<NoiseTable>,
    #[pyo3(get)]
    pub rx: Py<NoiseTable>,
    #[pyo3(get)]
    pub ry: Py<NoiseTable>,
    #[pyo3(get)]
    pub rz: Py<NoiseTable>,
    #[pyo3(get)]
    pub cx: Py<NoiseTable>,
    #[pyo3(get)]
    pub cz: Py<NoiseTable>,
    #[pyo3(get)]
    pub rxx: Py<NoiseTable>,
    #[pyo3(get)]
    pub ryy: Py<NoiseTable>,
    #[pyo3(get)]
    pub rzz: Py<NoiseTable>,
    #[pyo3(get)]
    pub swap: Py<NoiseTable>,
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
        bind_noise_config(py, &qdk_simulators::noise_config::NoiseConfig::NOISELESS)
    }
}

fn bind_noise_config(
    py: Python,
    value: &qdk_simulators::noise_config::NoiseConfig,
) -> PyResult<NoiseConfig> {
    Ok(NoiseConfig {
        i: Py::new(py, NoiseTable::from(value.i))?,
        x: Py::new(py, NoiseTable::from(value.x))?,
        y: Py::new(py, NoiseTable::from(value.y))?,
        z: Py::new(py, NoiseTable::from(value.z))?,
        h: Py::new(py, NoiseTable::from(value.h))?,
        s: Py::new(py, NoiseTable::from(value.s))?,
        s_adj: Py::new(py, NoiseTable::from(value.s_adj))?,
        t: Py::new(py, NoiseTable::from(value.t))?,
        t_adj: Py::new(py, NoiseTable::from(value.t_adj))?,
        sx: Py::new(py, NoiseTable::from(value.sx))?,
        sx_adj: Py::new(py, NoiseTable::from(value.sx_adj))?,
        rx: Py::new(py, NoiseTable::from(value.rx))?,
        ry: Py::new(py, NoiseTable::from(value.ry))?,
        rz: Py::new(py, NoiseTable::from(value.rz))?,
        cx: Py::new(py, NoiseTable::from(value.cx))?,
        cz: Py::new(py, NoiseTable::from(value.cz))?,
        rxx: Py::new(py, NoiseTable::from(value.rxx))?,
        ryy: Py::new(py, NoiseTable::from(value.ryy))?,
        rzz: Py::new(py, NoiseTable::from(value.rzz))?,
        swap: Py::new(py, NoiseTable::from(value.swap))?,
        mov: Py::new(py, NoiseTable::from(value.mov))?,
        mresetz: Py::new(py, NoiseTable::from(value.mresetz))?,
        idle: Py::new(py, IdleNoiseParams::from(value.idle))?,
    })
}

fn unbind_noise_config(
    py: Python,
    value: &Bound<NoiseConfig>,
) -> qdk_simulators::noise_config::NoiseConfig {
    let value = value.borrow();
    qdk_simulators::noise_config::NoiseConfig {
        i: from_noise_table_ref(&value.i.borrow(py)),
        x: from_noise_table_ref(&value.x.borrow(py)),
        y: from_noise_table_ref(&value.y.borrow(py)),
        z: from_noise_table_ref(&value.z.borrow(py)),
        h: from_noise_table_ref(&value.h.borrow(py)),
        s: from_noise_table_ref(&value.s.borrow(py)),
        s_adj: from_noise_table_ref(&value.s_adj.borrow(py)),
        t: from_noise_table_ref(&value.t.borrow(py)),
        t_adj: from_noise_table_ref(&value.t_adj.borrow(py)),
        sx: from_noise_table_ref(&value.sx.borrow(py)),
        sx_adj: from_noise_table_ref(&value.sx_adj.borrow(py)),
        rx: from_noise_table_ref(&value.rx.borrow(py)),
        ry: from_noise_table_ref(&value.ry.borrow(py)),
        rz: from_noise_table_ref(&value.rz.borrow(py)),
        cx: from_noise_table_ref(&value.cx.borrow(py)),
        cz: from_noise_table_ref(&value.cz.borrow(py)),
        rxx: from_noise_table_ref(&value.rxx.borrow(py)),
        ryy: from_noise_table_ref(&value.ryy.borrow(py)),
        rzz: from_noise_table_ref(&value.rzz.borrow(py)),
        swap: from_noise_table_ref(&value.swap.borrow(py)),
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

impl From<IdleNoiseParams> for qdk_simulators::noise_config::IdleNoiseParams {
    fn from(value: IdleNoiseParams) -> Self {
        qdk_simulators::noise_config::IdleNoiseParams {
            s_probability: value.s_probability,
        }
    }
}

fn from_idle_noise_params_ref(
    value: &PyRef<'_, IdleNoiseParams>,
) -> qdk_simulators::noise_config::IdleNoiseParams {
    qdk_simulators::noise_config::IdleNoiseParams {
        s_probability: value.s_probability,
    }
}

impl From<qdk_simulators::noise_config::IdleNoiseParams> for IdleNoiseParams {
    fn from(value: qdk_simulators::noise_config::IdleNoiseParams) -> Self {
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

    ///
    /// The depolarizing noise to use in simulation.
    ///
    pub fn set_depolarizing(&mut self, value: f32) -> PyResult<()> {
        self.validate_propability(value)?;
        self.x = value / 3.0;
        self.y = value / 3.0;
        self.z = value / 3.0;
        Ok(())
    }

    ///
    /// The bit flip noise to use in simulation.
    ///
    pub fn set_bitflip(&mut self, value: f32) -> PyResult<()> {
        self.validate_propability(value)?;
        self.x = value;
        self.y = 0.0;
        self.z = 0.0;
        Ok(())
    }

    ///
    /// The phase flip noise to use in simulation.
    ///
    pub fn set_phaseflip(&mut self, value: f32) -> PyResult<()> {
        self.validate_propability(value)?;
        self.x = 0.0;
        self.y = 0.0;
        self.z = value;
        Ok(())
    }

    #[allow(clippy::unused_self)]
    fn validate_propability(&self, value: f32) -> PyResult<()> {
        if value < 0.0 {
            return Err(PyValueError::new_err(
                "Pauli noise probabilities must be non-negative.",
            ));
        }
        if value > 1.0 {
            return Err(PyValueError::new_err(
                "The sum of Pauli noise probabilities must be at most 1.",
            ));
        }
        Ok(())
    }
}

impl From<NoiseTable> for qdk_simulators::noise_config::NoiseTable {
    fn from(value: NoiseTable) -> Self {
        qdk_simulators::noise_config::NoiseTable {
            x: value.x,
            y: value.y,
            z: value.z,
            loss: value.loss,
        }
    }
}

fn from_noise_table_ref(value: &PyRef<'_, NoiseTable>) -> qdk_simulators::noise_config::NoiseTable {
    qdk_simulators::noise_config::NoiseTable {
        x: value.x,
        y: value.y,
        z: value.z,
        loss: value.loss,
    }
}

impl From<qdk_simulators::noise_config::NoiseTable> for NoiseTable {
    fn from(value: qdk_simulators::noise_config::NoiseTable) -> Self {
        NoiseTable {
            x: value.x,
            y: value.y,
            z: value.z,
            loss: value.loss,
        }
    }
}
