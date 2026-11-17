use qep::Parameter::ResultId;
use qep::interface::Driver;
use qep::{
    Batcher, DEFAULT_MAX_BATCH_SIZE, DemoModel1, InstructionId, OPERATION_COUNT, Parameter,
    Profiler, QuantumBackend, QuantumBackendInstruction, QuantumBackends,
    SequentialCountingProfiler, StreamBackend, TemporalProfiler, TemporalProfilerConfig,
    instruction_id_to_name, instructions,
};
use smallvec::{SmallVec, smallvec};
use std::{
    ffi::{CStr, c_char},
    sync::{LazyLock, Mutex, MutexGuard},
};

enum BackendKind {
    Profiler(Box<dyn Profiler + Send>),
    Other(Box<dyn QuantumBackend + Send>),
    Stdio(Box<dyn QuantumBackend + Send>),
}
impl QuantumBackend for BackendKind {
    /// Executes instructions through the selected backend's retained trait object.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<qep::BatchResult, String> {
        match self {
            Self::Profiler(profiler) => profiler.execute_batch(instructions),
            Self::Other(backend) | Self::Stdio(backend) => backend.execute_batch(instructions),
        }
    }
}

struct BackendWrapper {
    batcher: Batcher<BackendKind>,
}

impl BackendWrapper {
    /// Uses inherited protocol pipes when configured, otherwise stdout and stdin.
    fn stdio() -> Self {
        let backend: Box<dyn QuantumBackend + Send> = match (
            std::env::var_os("QDK_INSTRUCTION_FD"),
            std::env::var_os("QDK_RESULT_FD"),
        ) {
            (None, None) => Box::new(StreamBackend::stdio()),
            (Some(instructions), Some(results)) => {
                #[cfg(unix)]
                {
                    Box::new(StreamBackend::new(
                        duplicate_pipe(&instructions, "QDK_INSTRUCTION_FD"),
                        duplicate_pipe(&results, "QDK_RESULT_FD"),
                    ))
                }
                #[cfg(not(unix))]
                {
                    let _ = (instructions, results);
                    abort_with_error("dedicated protocol pipes require Unix");
                }
            }
            _ => abort_with_error(
                "QDK_INSTRUCTION_FD and QDK_RESULT_FD must either both be set or both be absent",
            ),
        };
        Self::from_kind(BackendKind::Stdio(backend))
    }

    /// Wraps an owned backend without exposing profiling support.
    fn new(backend: impl QuantumBackend + Send + 'static) -> Self {
        Self::from_kind(BackendKind::Other(Box::new(backend)))
    }

    /// Wraps an owned profiler while retaining access to its profiling data.
    fn for_profiler(profiler: impl Profiler + Send + 'static) -> Self {
        Self::from_kind(BackendKind::Profiler(Box::new(profiler)))
    }

    /// Creates the shared instruction buffer for the selected backend kind.
    fn from_kind(backend: BackendKind) -> Self {
        Self {
            batcher: Batcher::new(backend, DEFAULT_MAX_BATCH_SIZE),
        }
    }
}

static BACKEND: LazyLock<Mutex<BackendWrapper>> =
    LazyLock::new(|| Mutex::new(BackendWrapper::stdio()));

/// Duplicates a configured pipe descriptor, rejecting invalid environment values.
#[cfg(unix)]
fn duplicate_pipe(value: &std::ffi::OsStr, name: &str) -> std::fs::File {
    use std::os::fd::FromRawFd;

    let descriptor = value
        .to_str()
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or_else(|| abort_with_error(&format!("{name} must be a file descriptor")));
    // SAFETY: fcntl checks the supplied integer descriptor and duplicates it on success.
    let duplicated = unsafe { libc::fcntl(descriptor, libc::F_DUPFD_CLOEXEC, 0) };
    if duplicated < 0 {
        abort_with_error(&format!(
            "cannot duplicate {name}: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: fcntl returned a new, valid descriptor owned exclusively by this File.
    unsafe { std::fs::File::from_raw_fd(duplicated) }
}

/// Reports an unrecoverable C API error without unwinding across the FFI boundary.
fn abort_with_error(message: &str) -> ! {
    eprintln!("qep_interop: {message}");
    std::process::abort();
}

/// Locks the shared backend, terminating if a previous call poisoned the mutex.
fn lock_backend() -> MutexGuard<'static, BackendWrapper> {
    BACKEND
        .lock()
        .unwrap_or_else(|error| abort_with_error(&format!("backend mutex poisoned: {error}")))
}

/// Internal helper to call for instructions with no results.
fn instruction(instruction_id: InstructionId, arguments: SmallVec<[Parameter; 3]>) {
    let mut profiler = lock_backend();
    let instruction = QuantumBackendInstruction {
        instruction_id,
        parameters: arguments,
    };
    if let Err(error) = profiler.batcher.push(instruction) {
        abort_with_error(&format!(
            "qdk_{} failed: {error}",
            instruction_id_to_name(instruction_id)
        ));
    }
}

/// Internal helper to call for instructions with single result.
fn instruction_with_result(
    instruction_id: InstructionId,
    arguments: SmallVec<[Parameter; 3]>,
) -> usize {
    let mut profiler = lock_backend();
    let instruction = QuantumBackendInstruction {
        instruction_id,
        parameters: arguments,
    };
    let result = profiler
        .batcher
        .push_with_result(instruction)
        .unwrap_or_else(|error| {
            abort_with_error(&format!(
                "qdk_{} failed: {error}",
                instruction_id_to_name(instruction_id)
            ))
        });
    match result {
        qep::QuantumBackendResult::Zero => 0,
        qep::QuantumBackendResult::One => 1,
        qep::QuantumBackendResult::Error => 2,
    }
}

/// Replaces the current backend, adapting non-native instructions for the temporal profiler.
///
/// # Safety
///
/// A non-null `backend` must point to a readable, NUL-terminated UTF-8 string
/// that remains valid and unmodified for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_init(backend: *const c_char) {
    if backend.is_null() {
        abort_with_error("qdk_init() backend must not be null");
    }
    // SAFETY: The caller guarantees a readable, NUL-terminated string.
    let backend = unsafe { CStr::from_ptr(backend) }
        .to_str()
        .unwrap_or_else(|error| {
            abort_with_error(&format!("qdk_init() backend is not UTF-8: {error}"))
        });
    let mut stored_backend = lock_backend();
    *stored_backend = match backend {
        "stdio" => BackendWrapper::stdio(),
        "sparse_sim" => BackendWrapper::new(QuantumBackends::sparse_sim()),
        "counting_profiler" => {
            let timings = (0..OPERATION_COUNT)
                .map(|id| (instruction_id_to_name(id).to_owned(), 0.0))
                .collect();
            let profiler = SequentialCountingProfiler::new(1, timings).unwrap_or_else(|error| {
                abort_with_error(&format!("qdk_init() counting_profiler failed: {error}"))
            });
            BackendWrapper::for_profiler(profiler)
        }
        "temporal_profiler" => {
            let config = TemporalProfilerConfig {
                record_trace: true,
                ..TemporalProfilerConfig::default()
            };
            BackendWrapper::for_profiler(qep::ResultAdapter::new(
                qep::TranspilingAdapter::with_defaults(TemporalProfiler::new(
                    DemoModel1::default(),
                    config,
                )),
            ))
        }
        _ => abort_with_error(&format!(
            "qdk_init() unknown backend `{backend}`; expected stdio, sparse_sim, counting_profiler, or temporal_profiler"
        )),
    };
}

/// Flushes pending instructions and saves profiling data when supported.
/// Reports unsupported profiling on stderr without writing a file.
/// In stdio mode, only prints guidance; it does not flush or inspect the path.
/// Invalid paths and save failures abort the process.
///
/// # Safety
///
/// Outside stdio mode, a non-null `path` must point to a readable, NUL-terminated
/// string that remains valid and unmodified for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_save_profile(path: *const c_char) {
    let mut backend = lock_backend();
    if matches!(backend.batcher.get_backend(), BackendKind::Stdio(_)) {
        eprintln!(
            "qep_interop: do not call qdk_save_profile() in stdio mode; use a profiler listening on the other end of the instruction stream"
        );
        return;
    }
    if path.is_null() {
        abort_with_error("qdk_save_profile() path must not be null");
    }
    // SAFETY: The caller guarantees a readable, NUL-terminated string.
    let path = unsafe { CStr::from_ptr(path) }
        .to_str()
        .unwrap_or_else(|error| {
            abort_with_error(&format!("qdk_save_profile() path is not UTF-8: {error}"))
        });
    backend.batcher.finish().unwrap_or_else(|error| {
        abort_with_error(&format!("qdk_save_profile() flush failed: {error}"))
    });
    let BackendKind::Profiler(profiler) = backend.batcher.get_backend() else {
        eprintln!("qep_interop: qdk_save_profile() backend does not support profiling");
        return;
    };
    let result = profiler.get_result();
    if let Err(error) = result.to_json(path) {
        abort_with_error(&format!("qdk_save_profile() failed: {error}"));
    }
    println!("Profile saved to {path}");
}

/// Applies a doubly controlled X gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_ccx(control0: usize, control1: usize, target: usize) {
    instruction(
        instructions::CCX,
        smallvec![
            Parameter::QubitId(control0),
            Parameter::QubitId(control1),
            Parameter::QubitId(target)
        ],
    );
}

/// Uncomputes an AND target to zero using backend-local measurement feedback.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_and_adj(control0: usize, control1: usize, target: usize) {
    instruction(
        instructions::AND_ADJ,
        smallvec![
            Parameter::QubitId(control0),
            Parameter::QubitId(control1),
            Parameter::QubitId(target)
        ],
    );
}

/// Applies a controlled X gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_cx(control: usize, target: usize) {
    instruction(
        instructions::CX,
        smallvec![Parameter::QubitId(control), Parameter::QubitId(target)],
    );
}

/// Applies a controlled Y gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_cy(control: usize, target: usize) {
    instruction(
        instructions::CY,
        smallvec![Parameter::QubitId(control), Parameter::QubitId(target)],
    );
}

/// Applies a controlled Z gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_cz(control: usize, target: usize) {
    instruction(
        instructions::CZ,
        smallvec![Parameter::QubitId(control), Parameter::QubitId(target)],
    );
}

/// Rotates a qubit around the X axis by an angle in radians.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_rx(qubit_id: usize, angle: f64) {
    instruction(
        instructions::RX,
        smallvec![Parameter::QubitId(qubit_id), Parameter::Float(angle)],
    );
}

/// Applies an XX rotation by an angle in radians.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_rxx(qubit0: usize, qubit1: usize, angle: f64) {
    instruction(
        instructions::RXX,
        smallvec![
            Parameter::QubitId(qubit0),
            Parameter::QubitId(qubit1),
            Parameter::Float(angle)
        ],
    );
}

/// Rotates a qubit around the Y axis by an angle in radians.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_ry(qubit_id: usize, angle: f64) {
    instruction(
        instructions::RY,
        smallvec![Parameter::QubitId(qubit_id), Parameter::Float(angle)],
    );
}

/// Applies a YY rotation by an angle in radians.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_ryy(qubit0: usize, qubit1: usize, angle: f64) {
    instruction(
        instructions::RYY,
        smallvec![
            Parameter::QubitId(qubit0),
            Parameter::QubitId(qubit1),
            Parameter::Float(angle)
        ],
    );
}

/// Rotates a qubit around the Z axis by an angle in radians.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_rz(qubit_id: usize, angle: f64) {
    instruction(
        instructions::RZ,
        smallvec![Parameter::QubitId(qubit_id), Parameter::Float(angle)],
    );
}

/// Applies a ZZ rotation by an angle in radians.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_rzz(qubit0: usize, qubit1: usize, angle: f64) {
    instruction(
        instructions::RZZ,
        smallvec![
            Parameter::QubitId(qubit0),
            Parameter::QubitId(qubit1),
            Parameter::Float(angle)
        ],
    );
}

/// Applies a Hadamard gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_h(qubit_id: usize) {
    instruction(instructions::H, smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies an S phase gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_s(qubit_id: usize) {
    instruction(instructions::S, smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies the adjoint of an S phase gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_sadj(qubit_id: usize) {
    instruction(instructions::SADJ, smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies a square-root X gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_sx(qubit_id: usize) {
    instruction(instructions::SX, smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies a T phase gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_t(qubit_id: usize) {
    instruction(instructions::T, smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies the adjoint of a T phase gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_tadj(qubit_id: usize) {
    instruction(instructions::TADJ, smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies an X gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_x(qubit_id: usize) {
    instruction(instructions::X, smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies a Y gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_y(qubit_id: usize) {
    instruction(instructions::Y, smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies a Z gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_z(qubit_id: usize) {
    instruction(instructions::Z, smallvec![Parameter::QubitId(qubit_id)]);
}

/// Swaps two qubits.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_swap(qubit0: usize, qubit1: usize) {
    instruction(
        instructions::SWAP,
        smallvec![Parameter::QubitId(qubit0), Parameter::QubitId(qubit1)],
    );
}

/// Resets a qubit to zero.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_reset(qubit_id: usize) {
    instruction(instructions::RESET, smallvec![Parameter::QubitId(qubit_id)]);
}

/// Measures a qubit in Z and stores the outcome in a result register.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_mz_store(qubit_id: usize, result_id: usize) {
    instruction(
        instructions::MZ_STORE,
        smallvec![Parameter::QubitId(qubit_id), Parameter::ResultId(result_id)],
    );
}

/// Measures a qubit in Z, stores the outcome, and resets the qubit.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_mz_store_reset(qubit_id: usize, result_id: usize) {
    instruction(
        instructions::MZ_STORE_RESET,
        smallvec![Parameter::QubitId(qubit_id), Parameter::ResultId(result_id)],
    );
}

/// Reads a stored result as zero, one, or the backend error value two.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_read_result(result_id: usize) -> usize {
    instruction_with_result(
        instructions::READ_RESULT,
        smallvec![Parameter::ResultId(result_id)],
    )
}

/// Measures a qubit in Z and returns zero, one, or the backend error value two.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_mz(qubit_id: usize) -> usize {
    instruction_with_result(instructions::MZ, smallvec![Parameter::QubitId(qubit_id)])
}

/// Measures a qubit in Z and resets it, returning zero, one, or the backend error value two.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_mz_reset(qubit_id: usize) -> usize {
    instruction_with_result(
        instructions::MZ_RESET,
        smallvec![Parameter::QubitId(qubit_id)],
    )
}

/// Reads stored result IDs `0..count` into consecutive bytes of `output`.
///
/// Writes results to `output[0], ..., output[count - 1]`, using 0/1/2 for Zero/One/Error.
///
/// This function exists to allow deferred measurement of multiple qubits in a single batch.
/// Users must first call [`qdk_mz_store`] or [`qdk_mz_store_reset`] to measure qubits
/// and write them into the corresponding result IDs. All requested result IDs must
/// be initialized before calling this function. Stored results are not consumed.
///
/// It is not guaranteed that all results will be read in a single batch, but the
/// library will attempt to do so. Batching is best effort.
/// Backend execution failures or an unexpected result count abort the process.
///
/// If `count` is zero, pending instructions are still flushed, but `output` is not
/// accessed and may be null.
///
/// # Safety
///
/// For nonzero `count`, `output` must point to a single allocation containing at
/// least `count` writable bytes from that pointer, valid for the duration of this
/// call. These bytes must not be accessed concurrently by other threads.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_read_results(count: usize, output: *mut u8) {
    let mut profiler = lock_backend();
    for i in 0..count {
        profiler.batcher.push_no_flush(QuantumBackendInstruction {
            instruction_id: instructions::READ_RESULT,
            parameters: smallvec![Parameter::ResultId(i)],
        });
    }
    let result = profiler
        .batcher
        .flush(count)
        .unwrap_or_else(|error| abort_with_error(&format!("qdk_read_results failed: {error}")));
    debug_assert!(result.results.len() == count);
    for i in 0..count {
        // SAFETY: The caller guarantees output points to count writable bytes.
        unsafe {
            output.add(i).write(match result.results[i] {
                qep::QuantumBackendResult::Zero => 0,
                qep::QuantumBackendResult::One => 1,
                qep::QuantumBackendResult::Error => 2,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qep::{BatchResult, QuantumBackendResult};
    use std::{ffi::CString, fs, sync::Arc};

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    /// Checks real stdio before initialization, explicit selection, and no-op profile saving.
    fn streams_without_initialization() {
        use std::{
            io::{BufRead, BufReader, Write},
            process::{Command, Stdio},
        };

        const CHILD_ENV: &str = "QEP_INTEROP_STDIO_TEST_CHILD";
        if std::env::var_os(CHILD_ENV).is_some() {
            // SAFETY: In default stdio mode, the path is ignored, including null.
            unsafe { qdk_save_profile(std::ptr::null()) };
            qdk_x(5);
            qdk_and_adj(1, 2, 3);
            // SAFETY: In stdio mode, the path is ignored and no batch is flushed.
            unsafe { qdk_save_profile(std::ptr::null()) };
            assert_eq!(qdk_mz(5), 1);
            // SAFETY: The backend name is a static, NUL-terminated string.
            unsafe { qdk_init(c"stdio".as_ptr()) };
            assert_eq!(qdk_mz(7), 0);
            assert_eq!(qdk_mz_reset(8), 2);
            return;
        }

        let mut child = Command::new(std::env::current_exe().expect("test binary must exist"))
            .args([
                "--exact",
                "tests::streams_without_initialization",
                "--nocapture",
            ])
            .env(CHILD_ENV, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("stdio test child must start");
        let mut stdin = child.stdin.take().expect("child stdin must be piped");
        let stdout = child.stdout.take().expect("child stdout must be piped");
        let mut responses = [b"\"1\"\n", b"\"0\"\n", b"\"2\"\n"].into_iter();
        let mut batches = Vec::new();
        // The uncaptured child test harness also writes status lines to stdout.
        for line in BufReader::new(stdout).lines() {
            let line = line.expect("child output must be readable");
            if line.starts_with('[') {
                batches.push(line);
                stdin
                    .write_all(responses.next().expect("unexpected instruction batch"))
                    .expect("test response must be written");
            }
        }
        let output = child.wait_with_output().expect("child must finish");
        let stderr = String::from_utf8(output.stderr).expect("diagnostics must be UTF-8");
        assert!(output.status.success(), "{stderr}");
        assert_eq!(
            batches,
            [
                r#"["x(5)","and_adj(1,2,3)","mz(5)"]"#,
                r#"["mz(7)"]"#,
                r#"["mz_reset(8)"]"#,
            ]
        );
        let guidance = "qep_interop: do not call qdk_save_profile() in stdio mode; use a profiler listening on the other end of the instruction stream";
        assert_eq!(stderr.lines().collect::<Vec<_>>(), [guidance, guidance]);
    }

    struct RecordingBackend {
        instructions: Arc<Mutex<Vec<QuantumBackendInstruction>>>,
    }

    impl QuantumBackend for RecordingBackend {
        /// Validates and records batches with distinct measurement outcomes.
        fn execute_batch(
            &mut self,
            instructions: &[QuantumBackendInstruction],
        ) -> Result<BatchResult, String> {
            let mut result = BatchResult::default();
            for instruction in instructions {
                qep::validate_instruction(instruction);
                self.instructions
                    .lock()
                    .expect("recording mutex must not be poisoned")
                    .push(instruction.clone());
                match instruction.instruction_id {
                    instructions::READ_RESULT => result.results.push(QuantumBackendResult::Zero),
                    instructions::MZ => result.results.push(QuantumBackendResult::One),
                    instructions::MZ_RESET => result.results.push(QuantumBackendResult::Error),
                    _ => {}
                }
            }
            Ok(result)
        }
    }

    #[test]
    /// Checks every public instruction's ID, operand order, and result mapping.
    fn forwards_all_public_instructions() {
        use Parameter::{Float, QubitId, ResultId};

        let _test_lock = TEST_LOCK.lock().expect("test mutex must not be poisoned");
        let recorded = Arc::new(Mutex::new(Vec::new()));
        *lock_backend() = BackendWrapper::new(RecordingBackend {
            instructions: Arc::clone(&recorded),
        });
        qdk_ccx(1, 2, 3);
        qdk_and_adj(1, 2, 3);
        qdk_cx(1, 2);
        qdk_cy(1, 2);
        qdk_cz(1, 2);
        qdk_rx(1, 0.25);
        qdk_rxx(1, 2, 0.25);
        qdk_ry(1, 0.25);
        qdk_ryy(1, 2, 0.25);
        qdk_rz(1, 0.25);
        qdk_rzz(1, 2, 0.25);
        qdk_h(1);
        qdk_s(1);
        qdk_sadj(1);
        qdk_sx(1);
        qdk_t(1);
        qdk_tadj(1);
        qdk_x(1);
        qdk_y(1);
        qdk_z(1);
        qdk_swap(1, 2);
        qdk_reset(1);
        qdk_mz_store(1, 4);
        qdk_mz_store_reset(1, 4);
        assert!(
            recorded
                .lock()
                .expect("recording mutex must not be poisoned")
                .is_empty(),
            "instructions must remain buffered until a result is requested"
        );
        assert_eq!(qdk_read_result(4), 0);
        assert_eq!(qdk_mz(1), 1);
        assert_eq!(qdk_mz_reset(1), 2);

        let expected = [
            (
                instructions::CCX,
                smallvec![QubitId(1), QubitId(2), QubitId(3)],
            ),
            (
                instructions::AND_ADJ,
                smallvec![QubitId(1), QubitId(2), QubitId(3)],
            ),
            (instructions::CX, smallvec![QubitId(1), QubitId(2)]),
            (instructions::CY, smallvec![QubitId(1), QubitId(2)]),
            (instructions::CZ, smallvec![QubitId(1), QubitId(2)]),
            (instructions::RX, smallvec![QubitId(1), Float(0.25)]),
            (
                instructions::RXX,
                smallvec![QubitId(1), QubitId(2), Float(0.25)],
            ),
            (instructions::RY, smallvec![QubitId(1), Float(0.25)]),
            (
                instructions::RYY,
                smallvec![QubitId(1), QubitId(2), Float(0.25)],
            ),
            (instructions::RZ, smallvec![QubitId(1), Float(0.25)]),
            (
                instructions::RZZ,
                smallvec![QubitId(1), QubitId(2), Float(0.25)],
            ),
            (instructions::H, smallvec![QubitId(1)]),
            (instructions::S, smallvec![QubitId(1)]),
            (instructions::SADJ, smallvec![QubitId(1)]),
            (instructions::SX, smallvec![QubitId(1)]),
            (instructions::T, smallvec![QubitId(1)]),
            (instructions::TADJ, smallvec![QubitId(1)]),
            (instructions::X, smallvec![QubitId(1)]),
            (instructions::Y, smallvec![QubitId(1)]),
            (instructions::Z, smallvec![QubitId(1)]),
            (instructions::SWAP, smallvec![QubitId(1), QubitId(2)]),
            (instructions::RESET, smallvec![QubitId(1)]),
            (instructions::MZ_STORE, smallvec![QubitId(1), ResultId(4)]),
            (
                instructions::MZ_STORE_RESET,
                smallvec![QubitId(1), ResultId(4)],
            ),
            (instructions::READ_RESULT, smallvec![ResultId(4)]),
            (instructions::MZ, smallvec![QubitId(1)]),
            (instructions::MZ_RESET, smallvec![QubitId(1)]),
        ]
        .map(|(instruction_id, parameters)| QuantumBackendInstruction {
            instruction_id,
            parameters,
        });
        assert_eq!(
            recorded
                .lock()
                .expect("recording mutex must not be poisoned")
                .as_slice(),
            expected.as_slice()
        );
        qdk_x(1);
        assert_eq!(
            recorded
                .lock()
                .expect("recording mutex must not be poisoned")
                .len(),
            expected.len()
        );
        // SAFETY: The path is a static, NUL-terminated string; this backend writes no file.
        unsafe { qdk_save_profile(c"unused.json".as_ptr()) };
        assert_eq!(
            recorded
                .lock()
                .expect("recording mutex must not be poisoned")
                .len(),
            expected.len() + 1
        );
        *lock_backend() = BackendWrapper::stdio();
    }

    #[test]
    /// Reads stored sparse-simulator results in ID order without overwriting adjacent bytes.
    fn reads_multiple_results_with_sparse_sim() {
        let _test_lock = TEST_LOCK.lock().expect("test mutex must not be poisoned");
        // SAFETY: The backend name is a static, NUL-terminated string.
        unsafe { qdk_init(c"sparse_sim".as_ptr()) };
        qdk_x(2);
        qdk_x(1);
        qdk_mz_store_reset(2, 0);
        qdk_mz_store(0, 1);
        qdk_mz_store(1, 2);

        let mut output = [255_u8; 5];
        // SAFETY: The output allocation has three writable bytes after the first byte.
        unsafe { qdk_read_results(3, output.as_mut_ptr().add(1)) };
        assert_eq!(output, [255, 1, 0, 1, 255]);
        assert_eq!(qdk_mz(2), 0);

        output.fill(255);
        // SAFETY: The output allocation has three writable bytes after the first byte.
        unsafe { qdk_read_results(3, output.as_mut_ptr().add(1)) };
        assert_eq!(output, [255, 1, 0, 1, 255]);
        *lock_backend() = BackendWrapper::stdio();
    }

    /// Preserves result registers and decomposes combined measurement/reset for profiling.
    #[test]
    fn profiles_adapted_measurements() {
        let _test_lock = TEST_LOCK.lock().expect("test mutex must not be poisoned");
        // SAFETY: The backend name is a static, NUL-terminated string.
        unsafe { qdk_init(c"temporal_profiler".as_ptr()) };
        qdk_mz_store_reset(0, 0);
        let stored = qdk_read_result(0);
        assert!(stored <= 1);
        assert_eq!(qdk_read_result(0), stored);
        assert!(qdk_mz_reset(0) <= 1);
        assert_eq!(qdk_read_result(0), stored);
        let mut backend = lock_backend();
        let BackendKind::Profiler(profiler) = backend.batcher.get_backend() else {
            panic!("temporal profiler must retain profiling support");
        };
        let result = profiler.get_result();
        // The last reset starts at the result barrier but is not drained.
        assert!((result.totals.total_runtime_sec - 3_000e-9).abs() < 1e-15);
        for metric in ["mz rate", "reset rate"] {
            let profile = result
                .profiles
                .profiles
                .iter()
                .find(|profile| profile.metric_name == metric)
                .expect("measurements and resets must be profiled separately");
            assert!((profile.series[0].1 - 2.0).abs() < 1e-12);
        }
        *backend = BackendWrapper::stdio();
    }

    #[test]
    /// Checks fresh state, execution, and saving across repeated initialization and backend switches.
    fn initializes_and_runs_all_backends() {
        let _test_lock = TEST_LOCK.lock().expect("test mutex must not be poisoned");
        let path =
            std::env::temp_dir().join(format!("qep_interop_profile_{}.json", std::process::id()));
        assert!(!path.exists(), "test output must not already exist");
        let c_path = CString::new(path.to_str().expect("test path must be UTF-8"))
            .expect("test path must not contain NUL");
        for backend in [
            c"sparse_sim",
            c"sparse_sim",
            c"counting_profiler",
            c"counting_profiler",
            c"temporal_profiler",
            c"temporal_profiler",
            c"sparse_sim",
        ] {
            // SAFETY: The backend name is a static, NUL-terminated string.
            unsafe { qdk_init(backend.as_ptr()) };
            if let BackendKind::Profiler(profiler) = lock_backend().batcher.get_backend() {
                let result = profiler.get_result();
                assert_eq!(result.totals.logical_compute_qubits, 0);
                if backend == c"counting_profiler" {
                    assert_eq!(result.totals.total_operations_count, 0);
                }
                assert!(result.totals.total_runtime_sec.abs() < f64::EPSILON);
            }
            qdk_x(5);
            let first = qdk_mz(5);
            qdk_x(5);
            let second = qdk_mz(5);
            assert!(first <= 1 && second <= 1);

            if backend == c"sparse_sim" {
                assert_eq!((first, second), (1, 0));
                assert!(matches!(
                    lock_backend().batcher.get_backend(),
                    BackendKind::Other(_)
                ));
                fs::write(&path, "existing profile").expect("test file must be created");
                // SAFETY: The CString remains valid for the duration of the call.
                unsafe { qdk_save_profile(c_path.as_ptr()) };
                assert_eq!(
                    fs::read_to_string(&path).expect("existing file must remain readable"),
                    "existing profile"
                );
                fs::remove_file(&path).expect("test file must be removed");
                // SAFETY: The CString remains valid for the duration of the call.
                unsafe { qdk_save_profile(c_path.as_ptr()) };
                assert!(!path.exists(), "simulator must not create a profile");
                qdk_x(5);
            } else {
                {
                    let stored = lock_backend();
                    let BackendKind::Profiler(profiler) = stored.batcher.get_backend() else {
                        panic!("profiler selection must use the profiler variant");
                    };
                    let result = profiler.get_result();
                    assert_eq!(result.totals.logical_compute_qubits, 6);
                    if backend == c"counting_profiler" {
                        assert_eq!(result.totals.logical_qubits, 6);
                        assert_eq!(result.totals.physical_qubits, 6);
                        assert_eq!(result.totals.total_operations_count, 4);
                        assert_eq!(result.totals.calls_per_operation["x"], 2);
                        assert_eq!(result.totals.calls_per_operation["mz"], 2);
                        assert!(result.totals.total_runtime_sec.abs() < f64::EPSILON);
                        assert!(result.infos.is_empty());
                    } else {
                        assert!(result.trace.is_some());
                        let x_rate = result
                            .profiles
                            .profiles
                            .iter()
                            .find(|profile| profile.metric_name == "x rate")
                            .expect("temporal profiler must record X gates");
                        assert!((x_rate.series[0].1 - 2.0).abs() < 1e-12);
                    }
                }
                qdk_x(5);
                for _ in 0..2 {
                    // SAFETY: The CString remains valid for the duration of the call.
                    unsafe { qdk_save_profile(c_path.as_ptr()) };
                    assert!(
                        fs::read_to_string(&path)
                            .expect("profiler must save JSON")
                            .contains("\"totals\"")
                    );
                    if backend == c"counting_profiler" {
                        let stored = lock_backend();
                        let BackendKind::Profiler(profiler) = stored.batcher.get_backend() else {
                            panic!("profiler selection must retain profiling support");
                        };
                        let result = profiler.get_result();
                        assert_eq!(result.totals.total_operations_count, 5);
                        assert_eq!(result.totals.calls_per_operation["x"], 3);
                    }
                }
                fs::remove_file(&path).expect("test profile must be removed");
            }
        }
        *lock_backend() = BackendWrapper::stdio();
    }
}
