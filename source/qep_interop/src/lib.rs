use qep::{
    OPERATION_COUNT, Parameter, Profiler, QuantumBackend, QuantumBackendInstruction,
    QuantumBackends, SequentialCountingProfiler, TemporalProfiler, TemporalProfilerConfig,
    instruction_id_to_name,
};
use smallvec::{SmallVec, smallvec};
use std::{
    ffi::{CStr, c_char},
    sync::{Mutex, MutexGuard},
};

enum Backend {
    Simulator(Box<dyn QuantumBackend + Send>),
    Profiler(Box<dyn Profiler + Send>),
}

impl Backend {
    /// Provides instruction execution for either variant using trait upcasting.
    fn as_backend_mut(&mut self) -> &mut (dyn QuantumBackend + Send) {
        match self {
            Self::Simulator(backend) => backend.as_mut(),
            Self::Profiler(profiler) => profiler.as_mut(),
        }
    }
}

static BACKEND: Mutex<Option<Backend>> = Mutex::new(None);

/// Reports an unrecoverable C API error without unwinding across the FFI boundary.
fn abort_with_error(message: &str) -> ! {
    eprintln!("qep_interop: {message}");
    std::process::abort();
}

/// Locks the shared backend, terminating if a previous call poisoned the mutex.
fn lock_backend() -> MutexGuard<'static, Option<Backend>> {
    BACKEND
        .lock()
        .unwrap_or_else(|error| abort_with_error(&format!("profiler mutex poisoned: {error}")))
}

/// Internal helper to call for instructions with no results.
fn instruction(instruction_name: &'static str, arguments: SmallVec<[Parameter; 3]>) {
    let mut profiler = lock_backend();
    let profiler = profiler.as_mut().unwrap_or_else(|| {
        abort_with_error(&format!(
            "qdk_init() must be called before qdk_{instruction_name}()"
        ))
    });
    let instruction = QuantumBackendInstruction {
        instruction_name,
        parameters: arguments,
    };
    if let Err(error) = profiler.as_backend_mut().execute_batch(&[instruction]) {
        abort_with_error(&format!("qdk_{instruction_name} failed: {error}"));
    }
}

/// Internal helper to call for instructions with single result.
fn instruction_with_result(
    instruction_name: &'static str,
    arguments: SmallVec<[Parameter; 3]>,
) -> usize {
    let mut profiler = lock_backend();
    let profiler = profiler.as_mut().unwrap_or_else(|| {
        abort_with_error(&format!(
            "qdk_init() must be called before qdk_{instruction_name}()"
        ))
    });
    let instruction = QuantumBackendInstruction {
        instruction_name,
        parameters: arguments,
    };
    let result = profiler
        .as_backend_mut()
        .execute_batch(&[instruction])
        .unwrap_or_else(|error| {
            abort_with_error(&format!("qdk_{instruction_name} failed: {error}"))
        });
    if result.results.len() != 1 {
        abort_with_error(&format!(
            "qdk_{instruction_name} must return exactly one result"
        ));
    }
    match result.results[0] {
        qep::QuantumBackendResult::Zero => 0,
        qep::QuantumBackendResult::One => 1,
        qep::QuantumBackendResult::Error => 2,
    }
}

/// Replaces the current backend with a fresh selected backend, without transpilers.
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
    *stored_backend = Some(match backend {
        "sparse_sim" => Backend::Simulator(Box::new(QuantumBackends::sparse_sim())),
        "counting_profiler" => {
            let timings = (0..OPERATION_COUNT)
                .map(|id| (instruction_id_to_name(id).to_owned(), 0.0))
                .collect();
            let profiler = SequentialCountingProfiler::new(1, timings).unwrap_or_else(|error| {
                abort_with_error(&format!("qdk_init() counting_profiler failed: {error}"))
            });
            Backend::Profiler(Box::new(profiler))
        }
        "temporal_profiler" => {
            let config = TemporalProfilerConfig {
                t_factories_count: 1,
                record_trace: true,
                ..TemporalProfilerConfig::default()
            };
            Backend::Profiler(Box::new(TemporalProfiler::new(config)))
        }
        _ => abort_with_error(&format!(
            "qdk_init() unknown backend `{backend}`; expected sparse_sim, counting_profiler, or temporal_profiler"
        )),
    });
}

/// Saves profiling data, or reports unsupported profiling on stderr and returns.
/// Invalid paths and save failures abort the process.
///
/// # Safety
///
/// A non-null `path` must point to a readable, NUL-terminated string that remains
/// valid and unmodified for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn qdk_save_profile(path: *const c_char) {
    if path.is_null() {
        abort_with_error("qdk_save_profile() path must not be null");
    }
    // SAFETY: The caller guarantees a readable, NUL-terminated string.
    let path = unsafe { CStr::from_ptr(path) }
        .to_str()
        .unwrap_or_else(|error| {
            abort_with_error(&format!("qdk_save_profile() path is not UTF-8: {error}"))
        });
    let backend = lock_backend();
    let backend = backend
        .as_ref()
        .unwrap_or_else(|| abort_with_error("qdk_init() must be called before qdk_save_profile()"));
    let Backend::Profiler(profiler) = backend else {
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
        "ccx",
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
        "cx",
        smallvec![Parameter::QubitId(control), Parameter::QubitId(target)],
    );
}

/// Applies a controlled Y gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_cy(control: usize, target: usize) {
    instruction(
        "cy",
        smallvec![Parameter::QubitId(control), Parameter::QubitId(target)],
    );
}

/// Applies a controlled Z gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_cz(control: usize, target: usize) {
    instruction(
        "cz",
        smallvec![Parameter::QubitId(control), Parameter::QubitId(target)],
    );
}

/// Rotates a qubit around the X axis by an angle in radians.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_rx(qubit_id: usize, angle: f64) {
    instruction(
        "rx",
        smallvec![Parameter::QubitId(qubit_id), Parameter::Float(angle)],
    );
}

/// Applies an XX rotation by an angle in radians.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_rxx(qubit0: usize, qubit1: usize, angle: f64) {
    instruction(
        "rxx",
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
        "ry",
        smallvec![Parameter::QubitId(qubit_id), Parameter::Float(angle)],
    );
}

/// Applies a YY rotation by an angle in radians.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_ryy(qubit0: usize, qubit1: usize, angle: f64) {
    instruction(
        "ryy",
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
        "rz",
        smallvec![Parameter::QubitId(qubit_id), Parameter::Float(angle)],
    );
}

/// Applies a ZZ rotation by an angle in radians.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_rzz(qubit0: usize, qubit1: usize, angle: f64) {
    instruction(
        "rzz",
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
    instruction("h", smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies an S phase gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_s(qubit_id: usize) {
    instruction("s", smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies the adjoint of an S phase gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_sadj(qubit_id: usize) {
    instruction("sadj", smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies a square-root X gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_sx(qubit_id: usize) {
    instruction("sx", smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies a T phase gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_t(qubit_id: usize) {
    instruction("t", smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies the adjoint of a T phase gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_tadj(qubit_id: usize) {
    instruction("tadj", smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies an X gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_x(qubit_id: usize) {
    instruction("x", smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies a Y gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_y(qubit_id: usize) {
    instruction("y", smallvec![Parameter::QubitId(qubit_id)]);
}

/// Applies a Z gate.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_z(qubit_id: usize) {
    instruction("z", smallvec![Parameter::QubitId(qubit_id)]);
}

/// Swaps two qubits.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_swap(qubit0: usize, qubit1: usize) {
    instruction(
        "swap",
        smallvec![Parameter::QubitId(qubit0), Parameter::QubitId(qubit1)],
    );
}

/// Resets a qubit to zero.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_reset(qubit_id: usize) {
    instruction("reset", smallvec![Parameter::QubitId(qubit_id)]);
}

/// Measures a qubit in Z and stores the outcome in a result register.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_mz_store(qubit_id: usize, result_id: usize) {
    instruction(
        "mz_store",
        smallvec![Parameter::QubitId(qubit_id), Parameter::ResultId(result_id)],
    );
}

/// Measures a qubit in Z, stores the outcome, and resets the qubit.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_mz_store_reset(qubit_id: usize, result_id: usize) {
    instruction(
        "mz_store_reset",
        smallvec![Parameter::QubitId(qubit_id), Parameter::ResultId(result_id)],
    );
}

/// Reads a stored result as zero, one, or the backend error value two.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_read_result(result_id: usize) -> usize {
    instruction_with_result("read_result", smallvec![Parameter::ResultId(result_id)])
}

/// Measures a qubit in Z and returns zero, one, or the backend error value two.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_mz(qubit_id: usize) -> usize {
    instruction_with_result("mz", smallvec![Parameter::QubitId(qubit_id)])
}

/// Measures a qubit in Z and resets it, returning zero, one, or the backend error value two.
#[unsafe(no_mangle)]
pub extern "C" fn qdk_mz_reset(qubit_id: usize) -> usize {
    instruction_with_result("mz_reset", smallvec![Parameter::QubitId(qubit_id)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use qep::{BatchResult, QuantumBackendResult};
    use std::{ffi::CString, fs, sync::Arc};

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    struct RecordingBackend {
        instructions: Arc<Mutex<Vec<QuantumBackendInstruction>>>,
    }

    impl QuantumBackend for RecordingBackend {
        /// Validates and records single-instruction batches with distinct test outcomes.
        fn execute_batch(
            &mut self,
            instructions: &[QuantumBackendInstruction],
        ) -> Result<BatchResult, String> {
            assert_eq!(instructions.len(), 1);
            let instruction = &instructions[0];
            qep::validate_instruction(instruction);
            self.instructions
                .lock()
                .expect("recording mutex must not be poisoned")
                .push(instruction.clone());
            let mut result = BatchResult::default();
            match instruction.instruction_name {
                "read_result" => result.results.push(QuantumBackendResult::Zero),
                "mz" => result.results.push(QuantumBackendResult::One),
                "mz_reset" => result.results.push(QuantumBackendResult::Error),
                _ => {}
            }
            Ok(result)
        }
    }

    #[test]
    /// Checks every public instruction's name, operand order, and result mapping.
    fn forwards_all_public_instructions() {
        use Parameter::{Float, QubitId, ResultId};

        let _test_lock = TEST_LOCK.lock().expect("test mutex must not be poisoned");
        let recorded = Arc::new(Mutex::new(Vec::new()));
        *lock_backend() = Some(Backend::Simulator(Box::new(RecordingBackend {
            instructions: Arc::clone(&recorded),
        })));
        qdk_ccx(1, 2, 3);
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
        assert_eq!(qdk_read_result(4), 0);
        assert_eq!(qdk_mz(1), 1);
        assert_eq!(qdk_mz_reset(1), 2);

        let expected = [
            ("ccx", smallvec![QubitId(1), QubitId(2), QubitId(3)]),
            ("cx", smallvec![QubitId(1), QubitId(2)]),
            ("cy", smallvec![QubitId(1), QubitId(2)]),
            ("cz", smallvec![QubitId(1), QubitId(2)]),
            ("rx", smallvec![QubitId(1), Float(0.25)]),
            ("rxx", smallvec![QubitId(1), QubitId(2), Float(0.25)]),
            ("ry", smallvec![QubitId(1), Float(0.25)]),
            ("ryy", smallvec![QubitId(1), QubitId(2), Float(0.25)]),
            ("rz", smallvec![QubitId(1), Float(0.25)]),
            ("rzz", smallvec![QubitId(1), QubitId(2), Float(0.25)]),
            ("h", smallvec![QubitId(1)]),
            ("s", smallvec![QubitId(1)]),
            ("sadj", smallvec![QubitId(1)]),
            ("sx", smallvec![QubitId(1)]),
            ("t", smallvec![QubitId(1)]),
            ("tadj", smallvec![QubitId(1)]),
            ("x", smallvec![QubitId(1)]),
            ("y", smallvec![QubitId(1)]),
            ("z", smallvec![QubitId(1)]),
            ("swap", smallvec![QubitId(1), QubitId(2)]),
            ("reset", smallvec![QubitId(1)]),
            ("mz_store", smallvec![QubitId(1), ResultId(4)]),
            ("mz_store_reset", smallvec![QubitId(1), ResultId(4)]),
            ("read_result", smallvec![ResultId(4)]),
            ("mz", smallvec![QubitId(1)]),
            ("mz_reset", smallvec![QubitId(1)]),
        ]
        .map(|(instruction_name, parameters)| QuantumBackendInstruction {
            instruction_name,
            parameters,
        });
        assert_eq!(
            recorded
                .lock()
                .expect("recording mutex must not be poisoned")
                .as_slice(),
            expected.as_slice()
        );
        *lock_backend() = None;
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
            if let Some(Backend::Profiler(profiler)) = lock_backend().as_ref() {
                let result = profiler.get_result();
                assert_eq!(result.totals.logical_qubits, 0);
                assert!(result.totals.total_runtime_sec.abs() < f64::EPSILON);
            }
            qdk_x(5);
            let first = qdk_mz(5);
            qdk_x(5);
            let second = qdk_mz(5);
            assert!(first <= 1 && second <= 1);

            if backend == c"sparse_sim" {
                assert_eq!((first, second), (1, 0));
                assert!(matches!(*lock_backend(), Some(Backend::Simulator(_))));
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
                    let Some(Backend::Profiler(profiler)) = stored.as_ref() else {
                        panic!("profiler selection must use the profiler variant");
                    };
                    let result = profiler.get_result();
                    assert_eq!(result.totals.logical_qubits, 6);
                    if backend == c"counting_profiler" {
                        assert_eq!(result.totals.physical_qubits, 6);
                        assert_eq!(result.totals.total_operations_count, 4);
                        assert_eq!(result.totals.calls_per_operation["x"], 2);
                        assert_eq!(result.totals.calls_per_operation["mz"], 2);
                        assert!(result.totals.total_runtime_sec.abs() < f64::EPSILON);
                        assert!(result.infos.is_empty());
                    } else {
                        assert!(result.trace.is_none());
                        let x_rate = result
                            .profiles
                            .profiles
                            .iter()
                            .find(|profile| profile.metric_name == "x rate")
                            .expect("temporal profiler must record X gates");
                        assert!((x_rate.series[0].1 - 2.0).abs() < 1e-12);
                    }
                }
                // SAFETY: The CString remains valid for the duration of the call.
                unsafe { qdk_save_profile(c_path.as_ptr()) };
                assert!(
                    fs::read_to_string(&path)
                        .expect("profiler must save JSON")
                        .contains("\"totals\"")
                );
                fs::remove_file(&path).expect("test profile must be removed");
            }
        }
        *lock_backend() = None;
    }
}
