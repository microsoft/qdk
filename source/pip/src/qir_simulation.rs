// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use num_traits::Float;
use pyo3::{
    Bound, FromPyObject, Py, PyRef, PyResult, Python,
    exceptions::{PyAttributeError, PyIOError, PyKeyError, PyTypeError, PyValueError},
    pybacked::PyBackedStr,
    pyclass, pymethods,
    types::{PyAnyMethods, PyTuple},
};
use qdk_simulators::noise_config::{encode_pauli, is_pauli_identity};
use rustc_hash::FxHashMap;
use std::str::FromStr;

pub(crate) mod cpu_simulators;
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
    /// This is really a family of instructions.
    /// All instructions in the intrinsics fields of the [`NoiseConfig`]
    /// are mapped to this `QirInstructionId`.
    CorrelatedNoise,
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
    CorrelatedNoise(
        QirInstructionId,
        u32,      /* table id */
        Vec<u32>, /* qubit args */
    ),
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
    // Idle noise parameters not yet supported
    // #[pyo3(get)]
    // pub idle: Py<IdleNoiseParams>,
    #[pyo3(get)]
    pub intrinsics: Py<NoiseIntrinsicsTable>,
}

#[pymethods]
impl NoiseConfig {
    #[new]
    fn new(py: Python) -> PyResult<Self> {
        bind_noise_config(
            py,
            &<qdk_simulators::noise_config::NoiseConfig<f64, f64>>::NOISELESS,
        )
    }

    fn intrinsic<'py>(
        &'py mut self,
        py: Python<'py>,
        name: &str,
        num_qubits: u32,
    ) -> PyResult<Py<NoiseTable>> {
        if self.intrinsics.borrow(py).contains_key(name) {
            Ok(self
                .intrinsics
                .borrow(py)
                .get(py, name)
                .expect("the key should be in the table"))
        } else {
            let new_table = Py::new(
                py,
                NoiseTable::from(qdk_simulators::noise_config::NoiseTable::<f64>::noiseless(
                    num_qubits,
                )),
            )?;
            self.intrinsics
                .borrow_mut(py)
                .insert(name.to_string(), new_table);
            Ok(self
                .intrinsics
                .borrow(py)
                .get(py, name)
                .expect("the key should be in the table"))
        }
    }

    fn load_csv_dir(&mut self, py: Python<'_>, dir_path: &str) -> PyResult<()> {
        use rayon::prelude::*;

        // Get all valid file paths.
        // Use entry.file_type() instead of path.is_file() to avoid an
        // extra stat syscall per entry (the OS caches the type in the
        // directory listing).
        let paths: Vec<_> = std::fs::read_dir(dir_path)?
            .filter_map(std::result::Result::ok)
            .filter(|e| {
                e.file_type().is_ok_and(|ft| ft.is_file())
                    && e.path().extension() == Some("csv".as_ref())
            })
            .map(|e| e.path())
            .collect();

        // Release the GIL while doing file I/O and parsing — none of
        // this work touches Python objects.
        let results: Vec<_> = py.detach(|| {
            paths
                .par_iter()
                .map(|path| {
                    // Design notes:
                    //   1. Memory-map the file to avoid a hundreds of MB heap allocation + copy.
                    //   2. memmap2 depends on the libc crate, we already take a dependency on
                    //      libc through PyO3, so we don't run a risk of supporting fewer
                    //      platforms by using it.
                    //
                    // SAFETY:
                    //   The risk with file-backed memory maps is the underlying file
                    //   being changed while the map is in use. If this happens the noise
                    //   could be loaded incorrectly.
                    //   This is used for simulation, and the risk of someone immediatly
                    //   changing the contents of a file immediatly after running their Python
                    //   code is low. The reward / risk ratio is high in this situation.
                    //   So, using a memory map makes sense.
                    let file = std::fs::File::open(path)?;
                    let mmap = unsafe { memmap2::Mmap::map(&file) }?;
                    let contents = std::str::from_utf8(&mmap).map_err(|e| {
                        PyValueError::new_err(format!(
                            "File {} is not valid UTF-8: {e}",
                            path.display()
                        ))
                    })?;
                    let filename = path
                        .file_stem()
                        .expect("file should have a name")
                        .to_str()
                        .expect("file name should be a valid unicode string");
                    parse_noise_table(contents).map(|table| (filename.to_string(), table))
                })
                .collect::<Result<Vec<_>, _>>()
        })?;

        // Insert into Python objects on the main thread (GIL required).
        for (name, table) in results {
            let new_table = Py::new(py, table)?;
            self.intrinsics.borrow_mut(py).insert(name, new_table);
        }

        Ok(())
    }
}

/// Entries parsed from a single chunk: `(encoded_pauli, probability)` pairs
/// and the qubit count observed in the chunk (if any data lines were present).
type ChunkEntries = (Vec<(u64, f64)>, Option<u32>);

fn parse_noise_table(contents: &str) -> PyResult<NoiseTable> {
    use rayon::prelude::*;

    let bytes = contents.as_bytes();
    let num_threads = rayon::current_num_threads().max(1);

    // For small inputs, avoid parallelism overhead.
    if contents.len() < 128 * 1024 || num_threads <= 1 {
        let (entries, qubits) = parse_noise_chunk(contents, 0)?;
        let pauli_noise = FxHashMap::from_iter(entries);
        return Ok(NoiseTable {
            qubits: qubits.unwrap_or(0),
            pauli_noise,
            loss: 0.0,
        });
    }

    // Split the buffer into roughly equal chunks at line boundaries.
    let chunk_size = contents.len() / num_threads;
    let mut boundaries = vec![0usize];
    for i in 1..num_threads {
        let approx = i * chunk_size;
        if approx < contents.len() {
            // Advance past the current (possibly partial) line.
            let end = match memchr::memchr(b'\n', &bytes[approx..]) {
                Some(offset) => approx + offset + 1,
                None => contents.len(),
            };
            if end < contents.len() {
                boundaries.push(end);
            }
        }
    }
    boundaries.push(contents.len());

    // Compute the starting line number for each chunk so error messages
    // report the correct global line number.
    let mut line_offsets = Vec::with_capacity(boundaries.len());
    line_offsets.push(0usize);
    for i in 0..boundaries.len() - 1 {
        let chunk_bytes = &bytes[boundaries[i]..boundaries[i + 1]];
        let nl = memchr::memchr_iter(b'\n', chunk_bytes).count();
        line_offsets.push(line_offsets[i] + nl);
    }

    // Build (slice, line_offset) pairs for each chunk.
    let chunks: Vec<_> = boundaries
        .windows(2)
        .zip(line_offsets.iter())
        .map(|(w, &offset)| (&contents[w[0]..w[1]], offset))
        .collect();

    // Parse all chunks in parallel.
    let chunk_results: Vec<_> = chunks
        .par_iter()
        .map(|&(chunk, line_offset)| parse_noise_chunk(chunk, line_offset))
        .collect::<Result<Vec<_>, _>>()?;

    // Merge: verify consistent qubit counts and insert directly into the map.
    let total_entries: usize = chunk_results.iter().map(|(e, _)| e.len()).sum();
    let mut pauli_noise = FxHashMap::with_capacity_and_hasher(total_entries, Default::default());
    let mut expected_qubits: Option<u32> = None;

    for (entries, chunk_qubits) in chunk_results {
        if let Some(q) = chunk_qubits {
            match expected_qubits {
                None => expected_qubits = Some(q),
                Some(exp) if exp != q => {
                    return Err(PyValueError::new_err(format!(
                        "Inconsistent Pauli string length: expected {exp} qubits, found {q}"
                    )));
                }
                _ => (),
            }
        }
        for (key, prob) in entries {
            pauli_noise.insert(key, prob);
        }
    }

    let qubits = expected_qubits.unwrap_or(0);

    Ok(NoiseTable {
        qubits,
        pauli_noise,
        loss: 0.0,
    })
}

/// Parse a single chunk of CSV content, returning the non-identity entries
/// and the observed qubit count (if any data lines were present).
/// `line_offset` is the global line number of the first line in this chunk,
/// used for error messages.
fn parse_noise_chunk(contents: &str, line_offset: usize) -> PyResult<ChunkEntries> {
    let capacity = contents.len() / 40;
    let mut entries = Vec::with_capacity(capacity);
    let mut expected_qubits: Option<u32> = None;

    for (local_i, line) in contents.lines().enumerate() {
        let i = line_offset + local_i;

        // Fast skip: check first byte before doing any work.
        if line.is_empty() {
            continue;
        }
        let first = line.as_bytes()[0];
        if first == b'#' || first == b'p' || first == b' ' || first == b'\t' {
            // Full check only for the rare header/comment/whitespace lines.
            if first == b'#' || line.starts_with("pauli") || line.trim().is_empty() {
                continue;
            }
        }

        // --- Inline parse_line + validation + identity check in a single pass ---
        let Some(comma) = memchr::memchr(b',', line.as_bytes()) else {
            return Err(PyIOError::new_err(format!(
                "invalid csv row in line {i}: `{line}`"
            )));
        };

        // Ensure there is no second comma.
        if memchr::memchr(b',', &line.as_bytes()[comma + 1..]).is_some() {
            return Err(PyIOError::new_err(format!(
                "invalid csv row in line {i}: `{line}`"
            )));
        }

        let pauli = &line[..comma];
        let prob_str = &line[comma + 1..];

        let Ok(prob) = f64::from_str(prob_str) else {
            return Err(PyIOError::new_err(format!(
                "invalid float in line {i}: `{line}`"
            )));
        };
        NoiseTable::validate_probability(prob)?;

        let num_qubits = u32::try_from(pauli.len()).expect("pauli string size should fit in a u32");

        match expected_qubits {
            None => expected_qubits = Some(num_qubits),
            Some(expected) if expected != num_qubits => {
                return Err(PyValueError::new_err(format!(
                    "Inconsistent Pauli string length in line {i}: expected {expected} qubits, found {num_qubits}"
                )));
            }
            _ => (),
        }

        // Validate characters, and encode to u64 in one pass.
        let pauli_bytes = pauli.as_bytes();
        let mut key: u64 = 0;
        for &b in pauli_bytes {
            let bits = match b {
                b'I' => 0u64,
                b'X' => 1u64,
                b'Y' => 2u64,
                b'Z' => 3u64,
                _ => {
                    return Err(PyAttributeError::new_err(format!(
                        "Invalid Pauli string char in line {i}: {line}"
                    )));
                }
            };
            key = (key << 2) | bits;
        }

        if key != 0 && prob != 0.0 {
            entries.push((key, prob));
        }
    }

    Ok((entries, expected_qubits))
}

fn generic_float_cast<T: Float, Q: Float>(value: T) -> Q {
    // SAFETY:
    //   Casts from f32 to f32, f32 to f64, and f64 to f64 work without issue.
    //   Casting from f64 to f32 will also work but there might be truncation.
    num_traits::NumCast::from(value).expect("casting f64 to f32 should succeed")
}

fn bind_noise_config<T: Float, Q: Float>(
    py: Python,
    value: &qdk_simulators::noise_config::NoiseConfig<T, Q>,
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
        // idle: Py::new(py, IdleNoiseParams::from(value.idle))?,
        intrinsics: Py::new(py, NoiseIntrinsicsTable::default())?,
    })
}

fn unbind_noise_config<T: Float, Q: Float>(
    py: Python,
    value: &Bound<NoiseConfig>,
) -> qdk_simulators::noise_config::NoiseConfig<T, Q> {
    let value = value.borrow();
    qdk_simulators::noise_config::NoiseConfig {
        i: from_noise_table_ref(value.i.borrow(py)),
        x: from_noise_table_ref(value.x.borrow(py)),
        y: from_noise_table_ref(value.y.borrow(py)),
        z: from_noise_table_ref(value.z.borrow(py)),
        h: from_noise_table_ref(value.h.borrow(py)),
        s: from_noise_table_ref(value.s.borrow(py)),
        s_adj: from_noise_table_ref(value.s_adj.borrow(py)),
        t: from_noise_table_ref(value.t.borrow(py)),
        t_adj: from_noise_table_ref(value.t_adj.borrow(py)),
        sx: from_noise_table_ref(value.sx.borrow(py)),
        sx_adj: from_noise_table_ref(value.sx_adj.borrow(py)),
        rx: from_noise_table_ref(value.rx.borrow(py)),
        ry: from_noise_table_ref(value.ry.borrow(py)),
        rz: from_noise_table_ref(value.rz.borrow(py)),
        cx: from_noise_table_ref(value.cx.borrow(py)),
        cz: from_noise_table_ref(value.cz.borrow(py)),
        rxx: from_noise_table_ref(value.rxx.borrow(py)),
        ryy: from_noise_table_ref(value.ryy.borrow(py)),
        rzz: from_noise_table_ref(value.rzz.borrow(py)),
        swap: from_noise_table_ref(value.swap.borrow(py)),
        mov: from_noise_table_ref(value.mov.borrow(py)),
        mresetz: from_noise_table_ref(value.mresetz.borrow(py)),
        idle: qdk_simulators::noise_config::IdleNoiseParams::NOISELESS, // _from_idle_noise_params_ref(value.idle.borrow(py)),
        intrinsics: from_intrinsics_table_ref(py, value.intrinsics.borrow(py)),
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

#[allow(clippy::needless_pass_by_value, reason = "we are passing a reference")]
fn _from_idle_noise_params_ref(
    value: PyRef<'_, IdleNoiseParams>,
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
    pauli_noise: FxHashMap<u64, Probability>,
    #[pyo3(get, set)]
    pub loss: Probability,
}

impl NoiseTable {
    fn validate_probability(p: Probability) -> PyResult<()> {
        // If the user enters an entry with a probability of zero, we delete this
        // entry from the noise table if it was previously set, or ignore it if
        // it is not in the noise table.
        if !(0.0..=1.0).contains(&p) {
            return Err(PyValueError::new_err(format!(
                "Probabilities must be in the range [0, 1], found {p}."
            )));
        }
        Ok(())
    }

    fn validate_pauli_string(&self, pauli_string: &str) -> PyResult<()> {
        // Validate pauli string chars.
        if !pauli_string
            .chars()
            .all(|c| matches!(c, 'I' | 'X' | 'Y' | 'Z'))
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
        let key = encode_pauli(&name);
        if let Some(p) = self.pauli_noise.get(&key) {
            return Ok(*p);
        }
        Err(PyAttributeError::new_err(format!(
            "'NoiseTable' object has no attribute '{name}'",
        )))
    }

    /// Set the probability of noise for an element on the [`NoiseTable`]
    /// without validating the pauli string or the probability.
    ///
    /// Make sure to validate the pauli strings and probabilities before hand.
    unsafe fn set_pauli_noise_elt_unchecked(&mut self, pauli: &str, value: Probability) {
        let key = encode_pauli(pauli);
        if !is_pauli_identity(key) {
            if self.pauli_noise.contains_key(&key) && value == 0.0 {
                self.pauli_noise.remove(&key);
            } else {
                self.pauli_noise.insert(key, value);
            }
        }
    }

    fn set_pauli_noise_elt(&mut self, pauli: &str, value: Probability) -> PyResult<()> {
        self.validate_pauli_string(pauli)?;
        Self::validate_probability(value)?;

        // SAFETY: we validated the pauli string and probability above.
        unsafe {
            self.set_pauli_noise_elt_unchecked(pauli, value);
        }
        Ok(())
    }

    fn set_pauli_noise_from_zipped_lists(
        &mut self,
        list: Vec<(PyBackedStr, Probability)>,
    ) -> PyResult<()> {
        // Do all validation first.
        for (pauli, value) in &list {
            self.validate_pauli_string(pauli)?;
            Self::validate_probability(*value)?;
        }
        for (pauli, value) in list {
            // SAFETY: we validated all the pauli strings and probabilities above.
            unsafe {
                self.set_pauli_noise_elt_unchecked(pauli.as_ref(), value);
            }
        }
        Ok(())
    }

    fn set_pauli_noise_from_lists(
        &mut self,
        paulis: Vec<PyBackedStr>,
        probs: Vec<Probability>,
    ) -> PyResult<()> {
        // Do all validation first.
        for pauli in &paulis {
            self.validate_pauli_string(pauli)?;
        }
        for p in &probs {
            Self::validate_probability(*p)?;
        }
        let additional = paulis.len().saturating_sub(self.pauli_noise.len());
        self.pauli_noise.reserve(additional);
        for (pauli, value) in paulis.into_iter().zip(probs.into_iter()) {
            // SAFETY: we validated all the pauli strings and probabilities above.
            unsafe {
                self.set_pauli_noise_elt_unchecked(pauli.as_ref(), value);
            }
        }
        Ok(())
    }
}

#[allow(
    clippy::doc_markdown,
    clippy::doc_link_with_quotes,
    reason = "these docstrings conform to the python docstring format"
)]
#[pymethods]
impl NoiseTable {
    #[new]
    fn new(num_qubits: u32) -> Self {
        NoiseTable {
            qubits: num_qubits,
            pauli_noise: FxHashMap::default(),
            loss: 0.0,
        }
    }

    /// Defining __getattr__ allows getting noise like this
    ///
    /// noise_table.ziz
    ///
    /// for arbitrary pauli fields.
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
    /// noise_table = NoiseTable(3)
    /// noise_table.ziz = 0.005
    ///
    /// for arbitrary pauli fields. Setting an element that was
    /// previously set overrides that entry with the new value.
    fn __setattr__(&mut self, name: &str, value: Probability) -> PyResult<()> {
        if name == "loss" {
            self.loss = value;
            Ok(())
        } else {
            self.set_pauli_noise_elt(&name.to_uppercase(), value)
        }
    }

    /// The correlated pauli noise to use in simulation. Setting an element
    /// that was previously set overrides that entry with the new value.
    ///
    /// Example:
    ///     noise_table = NoiseTable(2)
    ///     noise_table.set_pauli_noise("XZ", 1e-10)
    ///     noise_table.set_pauli_noise(["XI", "XZ"], [1e-10, 3.7e-8])
    ///     noise_table.set_pauli_noise([("XI", 1e-10), ("XZ", 1e-8)])
    ///
    ///
    #[pyo3(signature = (*py_args))]
    pub fn set_pauli_noise(&mut self, py_args: &Bound<'_, PyTuple>) -> PyResult<()> {
        type Pair = (PyBackedStr, Probability);

        if let Ok((pauli, value)) = py_args.extract::<Pair>() {
            return self.set_pauli_noise_elt(&pauli, value);
        }
        if let Ok((paulis, probs)) = py_args.extract::<(Vec<PyBackedStr>, Vec<Probability>)>() {
            return self.set_pauli_noise_from_lists(paulis, probs);
        }

        if let Ok((list,)) = py_args.extract::<(Vec<Pair>,)>() {
            return self.set_pauli_noise_from_zipped_lists(list);
        }
        Err(PyTypeError::new_err(format!(
            "Expected two arguments of types 'str, float',
or two arguments of types 'list[str], list[float]',
or one argument of type 'list[tuple[str, float]]', but found {py_args:?}"
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

        let val = value / Probability::from(4_u32.pow(self.qubits) - 1);
        let mut probabilities = Vec::with_capacity(pauli_strings.len());
        for _ in 0..pauli_strings.len() {
            probabilities.push(val);
        }

        self.pauli_noise = pauli_strings
            .iter()
            .map(|s| encode_pauli(s))
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

impl<T: Float> From<NoiseTable> for qdk_simulators::noise_config::NoiseTable<T> {
    fn from(value: NoiseTable) -> Self {
        let mut pauli_strings = Vec::with_capacity(value.pauli_noise.len());
        let mut probabilities = Vec::with_capacity(value.pauli_noise.len());
        for (key, probability) in value.pauli_noise {
            pauli_strings.push(key);
            probabilities.push(generic_float_cast(probability));
        }
        qdk_simulators::noise_config::NoiseTable {
            qubits: value.qubits,
            pauli_strings,
            probabilities,
            loss: generic_float_cast(value.loss),
        }
    }
}

#[allow(clippy::needless_pass_by_value, reason = "we are passing a reference")]
fn from_noise_table_ref<T: Float>(
    value: PyRef<'_, NoiseTable>,
) -> qdk_simulators::noise_config::NoiseTable<T> {
    let mut pauli_strings = Vec::with_capacity(value.pauli_noise.len());
    let mut probabilities: Vec<T> = Vec::with_capacity(value.pauli_noise.len());
    for (key, probability) in &value.pauli_noise {
        pauli_strings.push(*key);
        probabilities.push(generic_float_cast(*probability));
    }
    qdk_simulators::noise_config::NoiseTable {
        qubits: value.qubits,
        pauli_strings,
        probabilities,
        loss: generic_float_cast(value.loss),
    }
}

impl<T: Float> From<qdk_simulators::noise_config::NoiseTable<T>> for NoiseTable {
    fn from(value: qdk_simulators::noise_config::NoiseTable<T>) -> Self {
        let pauli_noise = value
            .pauli_strings
            .iter()
            .copied()
            .zip(
                value
                    .probabilities
                    .into_iter()
                    .map(|p| generic_float_cast(p)),
            )
            .collect::<FxHashMap<_, _>>();
        NoiseTable {
            qubits: value.qubits,
            pauli_noise,
            loss: generic_float_cast(value.loss),
        }
    }
}

#[derive(Debug, Default)]
#[pyclass(module = "qsharp._native")]
pub struct NoiseIntrinsicsTable {
    next_id: u32,
    table: FxHashMap<String, (u32, Py<NoiseTable>)>,
}

impl Clone for NoiseIntrinsicsTable {
    fn clone(&self) -> Self {
        Python::attach(|py| Self {
            next_id: self.next_id,
            table: self
                .table
                .iter()
                .map(|(k, (id, noise))| (k.clone(), (*id, noise.clone_ref(py))))
                .collect(),
        })
    }
}

impl NoiseIntrinsicsTable {
    fn contains_key(&self, key: &str) -> bool {
        self.table.contains_key(key)
    }

    fn insert(&mut self, key: String, value: Py<NoiseTable>) {
        // If the intrinsic was already in the noise table, override it.
        if let Ok(id) = self.get_intrinsic_id(&key) {
            self.table.insert(key, (id, value));
            return;
        }
        self.table.insert(key, (self.next_id, value));
        self.next_id += 1;
    }

    fn get(&self, py: Python, key: &str) -> Option<Py<NoiseTable>> {
        self.table.get(key).map(|tuple| tuple.1.clone_ref(py))
    }
}

#[pymethods]
impl NoiseIntrinsicsTable {
    #[new]
    fn new() -> Self {
        Self::default()
    }

    fn __contains__(&self, py: Python, key: &str) -> bool {
        self.get(py, key).is_some()
    }

    fn __getitem__(&self, py: Python, key: &str) -> PyResult<Py<NoiseTable>> {
        if let Some(value) = self.get(py, key) {
            Ok(value.clone_ref(py))
        } else {
            Err(PyKeyError::new_err(key.to_string()))
        }
    }

    fn __setitem__(&mut self, key: &str, value: Py<NoiseTable>) {
        self.insert(key.to_string(), value);
    }

    fn get_intrinsic_id(&self, key: &str) -> PyResult<u32> {
        if let Some((id, _)) = self.table.get(key) {
            Ok(*id)
        } else {
            Err(PyKeyError::new_err(key.to_string()))
        }
    }
}

#[allow(clippy::needless_pass_by_value, reason = "we are passing a reference")]
fn from_intrinsics_table_ref<T: Float>(
    py: Python,
    value: PyRef<'_, NoiseIntrinsicsTable>,
) -> FxHashMap<u32, qdk_simulators::noise_config::NoiseTable<T>> {
    value
        .table
        .values()
        .map(|(k, v)| (*k, from_noise_table_ref(v.borrow(py))))
        .collect()
}
