// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use pyo3::{
    Bound, FromPyObject, Py, PyRef, PyResult, Python,
    exceptions::{PyAttributeError, PyTypeError, PyValueError},
    pyclass, pymethods,
    types::{PyAnyMethods, PyTuple},
};
use rustc_hash::FxHashMap;

pub(crate) mod clifford;
pub(crate) mod cpu_full_state;
pub(crate) mod gpu_full_state;

type Probability = f64;

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
        i: Py::new(py, NoiseTable::from(value.i.clone()))?,
        x: Py::new(py, NoiseTable::from(value.x.clone()))?,
        y: Py::new(py, NoiseTable::from(value.y.clone()))?,
        z: Py::new(py, NoiseTable::from(value.z.clone()))?,
        h: Py::new(py, NoiseTable::from(value.h.clone()))?,
        s: Py::new(py, NoiseTable::from(value.s.clone()))?,
        s_adj: Py::new(py, NoiseTable::from(value.s_adj.clone()))?,
        t: Py::new(py, NoiseTable::from(value.t.clone()))?,
        t_adj: Py::new(py, NoiseTable::from(value.t_adj.clone()))?,
        sx: Py::new(py, NoiseTable::from(value.sx.clone()))?,
        sx_adj: Py::new(py, NoiseTable::from(value.sx_adj.clone()))?,
        rx: Py::new(py, NoiseTable::from(value.rx.clone()))?,
        ry: Py::new(py, NoiseTable::from(value.ry.clone()))?,
        rz: Py::new(py, NoiseTable::from(value.rz.clone()))?,
        cx: Py::new(py, NoiseTable::from(value.cx.clone()))?,
        cz: Py::new(py, NoiseTable::from(value.cz.clone()))?,
        rxx: Py::new(py, NoiseTable::from(value.rxx.clone()))?,
        ryy: Py::new(py, NoiseTable::from(value.ryy.clone()))?,
        rzz: Py::new(py, NoiseTable::from(value.rzz.clone()))?,
        swap: Py::new(py, NoiseTable::from(value.swap.clone()))?,
        mov: Py::new(py, NoiseTable::from(value.mov.clone()))?,
        mresetz: Py::new(py, NoiseTable::from(value.mresetz.clone()))?,
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
    pub s_probability: Probability,
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
            #[allow(clippy::cast_possible_truncation)]
            s_probability: value.s_probability as f32,
        }
    }
}

fn from_idle_noise_params_ref(
    value: &PyRef<'_, IdleNoiseParams>,
) -> qdk_simulators::noise_config::IdleNoiseParams {
    qdk_simulators::noise_config::IdleNoiseParams {
        #[allow(clippy::cast_possible_truncation)]
        s_probability: value.s_probability as f32,
    }
}

impl From<qdk_simulators::noise_config::IdleNoiseParams> for IdleNoiseParams {
    fn from(value: qdk_simulators::noise_config::IdleNoiseParams) -> Self {
        IdleNoiseParams {
            s_probability: f64::from(value.s_probability),
        }
    }
}

#[derive(Clone, Debug)]
#[pyclass(module = "qsharp._native")]
pub struct NoiseTable {
    qubits: u32,
    pauli_noise: FxHashMap<String, Probability>,
    #[pyo3(get, set)]
    pub loss: Probability,
}

impl NoiseTable {
    fn validate_probability(value: Probability) -> PyResult<()> {
        if value < 0.0 {
            return Err(PyValueError::new_err("Probabilities must be non-negative."));
        }
        if value > 1.0 {
            return Err(PyValueError::new_err("Probabilities must be at most 1."));
        }
        Ok(())
    }

    fn validate_pauli_string(&self, pauli_string: &str) -> PyResult<()> {
        // Validate pauli string chars.
        if !pauli_string
            .chars()
            .all(|c| c == 'I' || c == 'X' || c == 'Y' || c == 'Z')
        {
            return Err(PyAttributeError::new_err(format!(
                "Pauli string can only contain 'I', 'X', 'Y', 'Z' characters, found {pauli_string}"
            )));
        }
        // Validate number of qubits.
        if pauli_string.len() != self.qubits as usize {
            return Err(PyAttributeError::new_err(format!(
                "Expected a pauli string with {} characters for this operation, found {}",
                self.qubits, pauli_string
            )));
        }
        Ok(())
    }

    fn generate_pauli_strings(n: u32, strings: Vec<String>) -> Vec<String> {
        // Base case.
        if n == 0 {
            return strings;
        }

        // Recursive case.
        let mut extended_strings = Vec::with_capacity(strings.len() * 4);
        for s in &strings {
            extended_strings.push(s.clone() + "X");
            extended_strings.push(s.clone() + "Y");
            extended_strings.push(s.clone() + "Z");
            extended_strings.push(s.clone() + "I");
        }
        Self::generate_pauli_strings(n - 1, extended_strings)
    }

    fn get_pauli_noise(&self, name: &str) -> PyResult<Probability> {
        let name = name.to_uppercase();
        if let Some(p) = self.pauli_noise.get(&name) {
            return Ok(*p);
        }
        Err(PyAttributeError::new_err(format!(
            "'NoiseTable' object has no attribute '{name}'",
        )))
    }

    fn set_pauli_noise_elt(&mut self, pauli: &str, value: Probability) -> PyResult<()> {
        let pauli = pauli.to_uppercase();
        self.validate_pauli_string(&pauli)?;
        Self::validate_probability(value)?;

        if self.pauli_noise.contains_key(&pauli) && value == 0.0 {
            self.pauli_noise.remove(&pauli);
        } else {
            self.pauli_noise.insert(pauli, value);
        }
        Ok(())
    }

    fn set_pauli_noise_list(&mut self, list: Vec<(String, Probability)>) -> PyResult<()> {
        // Do all validation first.
        for (pauli, value) in &list {
            self.validate_pauli_string(&pauli.to_uppercase())?;
            Self::validate_probability(*value)?;
        }
        for (pauli, value) in list {
            let pauli = pauli.to_ascii_uppercase();
            if self.pauli_noise.contains_key(&pauli) && value == 0.0 {
                self.pauli_noise.remove(&pauli);
            } else {
                self.pauli_noise.insert(pauli, value);
            }
        }
        Ok(())
    }
}

#[pymethods]
impl NoiseTable {
    #[new]
    fn new(qubits: u32) -> Self {
        NoiseTable {
            qubits,
            pauli_noise: FxHashMap::default(),
            loss: 0.0,
        }
    }

    #[allow(
        clippy::doc_markdown,
        reason = "this docstring conforms to the python docstring format"
    )]
    fn __getattr__(&mut self, name: &str) -> PyResult<Probability> {
        if name == "loss" {
            Ok(self.loss)
        } else {
            self.get_pauli_noise(name)
        }
    }

    #[allow(
        clippy::doc_markdown,
        reason = "this docstring conforms to the python docstring format"
    )]
    /// Defining __setattr__ allows setting noise like this
    ///
    /// noise_table = NoiseTable()
    /// noise_table.ziz = 0.005
    ///
    /// for arbitrary pauli fields.
    fn __setattr__(&mut self, name: &str, value: Probability) -> PyResult<()> {
        if name == "loss" {
            self.loss = value;
            Ok(())
        } else {
            self.set_pauli_noise_elt(name, value)
        }
    }

    ///
    /// The correlated pauli noise to use in simulation.
    ///
    #[pyo3(signature = (*py_args))]
    pub fn set_pauli_noise(&mut self, py_args: &Bound<'_, PyTuple>) -> PyResult<()> {
        type Pair = (String, Probability);

        if let Ok((pauli, value)) = py_args.extract::<Pair>() {
            return self.set_pauli_noise_elt(&pauli, value);
        }
        if let Ok((list,)) = py_args.extract::<(Vec<Pair>,)>() {
            return self.set_pauli_noise_list(list);
        }
        Err(PyTypeError::new_err(format!(
            "Expected two arguments of types 'str, float' or one argument of type 'list[tuple[str, float]]', but found {py_args:?}"
        )))
    }

    ///
    /// The depolarizing noise to use in simulation.
    ///
    pub fn set_depolarizing(&mut self, value: Probability) -> PyResult<()> {
        Self::validate_probability(value)?;

        // Generate all pauli strings.
        let mut pauli_strings = Self::generate_pauli_strings(self.qubits, vec![String::new()]);
        // Remove identity.
        pauli_strings.pop();

        let val = (value / Probability::from(self.qubits))
            / Probability::from(4_u32.pow(self.qubits) - 1);
        let mut probabilities = Vec::with_capacity(pauli_strings.len());
        for _ in 0..pauli_strings.len() {
            probabilities.push(val);
        }

        self.pauli_noise = pauli_strings
            .into_iter()
            .zip(probabilities)
            .collect::<FxHashMap<_, _>>();

        Ok(())
    }

    ///
    /// The bit flip noise to use in simulation.
    ///
    pub fn set_bitflip(&mut self, value: Probability) -> PyResult<()> {
        self.set_pauli_noise_elt("X", value)
    }

    ///
    /// The phase flip noise to use in simulation.
    ///
    pub fn set_phaseflip(&mut self, value: Probability) -> PyResult<()> {
        self.set_pauli_noise_elt("Z", value)
    }
}

impl From<NoiseTable> for qdk_simulators::noise_config::NoiseTable {
    fn from(value: NoiseTable) -> Self {
        let mut pauli_strings = Vec::with_capacity(value.pauli_noise.len());
        let mut probabilities = Vec::with_capacity(value.pauli_noise.len());
        for (pauli, probability) in value.pauli_noise {
            pauli_strings.push(pauli);
            #[allow(clippy::cast_possible_truncation)]
            probabilities.push(probability as f32);
        }
        qdk_simulators::noise_config::NoiseTable {
            qubits: value.qubits,
            pauli_strings,
            probabilities,
            #[allow(clippy::cast_possible_truncation)]
            loss: value.loss as f32,
        }
    }
}

fn from_noise_table_ref(value: &PyRef<'_, NoiseTable>) -> qdk_simulators::noise_config::NoiseTable {
    let mut pauli_strings = Vec::with_capacity(value.pauli_noise.len());
    let mut probabilities = Vec::with_capacity(value.pauli_noise.len());
    for (pauli, probability) in &value.pauli_noise {
        pauli_strings.push(pauli.clone());
        #[allow(clippy::cast_possible_truncation)]
        probabilities.push(*probability as f32);
    }
    qdk_simulators::noise_config::NoiseTable {
        qubits: value.qubits,
        pauli_strings,
        probabilities,
        #[allow(clippy::cast_possible_truncation)]
        loss: value.loss as f32,
    }
}

impl From<qdk_simulators::noise_config::NoiseTable> for NoiseTable {
    fn from(value: qdk_simulators::noise_config::NoiseTable) -> Self {
        let pauli_noise = value
            .pauli_strings
            .into_iter()
            .zip(value.probabilities.into_iter().map(f64::from))
            .collect::<FxHashMap<_, _>>();
        NoiseTable {
            qubits: value.qubits,
            pauli_noise,
            loss: f64::from(value.loss),
        }
    }
}
