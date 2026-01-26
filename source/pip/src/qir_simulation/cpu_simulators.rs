// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir_simulation::{NoiseConfig, QirInstruction, QirInstructionId, unbind_noise_config};
use pyo3::{IntoPyObjectExt, exceptions::PyValueError, prelude::*, types::PyList};
use pyo3::{PyResult, pyfunction};
use qdk_simulators::{
    MeasurementResult, Simulator,
    cpu_full_state_simulator::{NoiselessSimulator, NoisySimulator},
    noise_config::{self, CumulativeNoiseConfig},
    stabilizer_simulator::StabilizerSimulator,
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::{fmt::Write, sync::Arc};

#[pyfunction]
pub fn run_clifford<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    num_qubits: u32,
    num_results: u32,
    shots: u32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
) -> PyResult<Py<PyAny>> {
    let make_simulator = |num_qubits, num_results, seed, noise| {
        StabilizerSimulator::new(num_qubits as usize, num_results as usize, seed, noise)
    };
    py_run(
        py,
        input,
        num_qubits,
        num_results,
        shots,
        noise_config,
        seed,
        make_simulator,
    )
}

#[pyfunction]
pub fn run_cpu_full_state<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    num_qubits: u32,
    num_results: u32,
    shots: u32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
) -> PyResult<Py<PyAny>> {
    use qdk_simulators::cpu_full_state_simulator::noise::Fault;
    if noise_config.is_some() {
        let make_simulator = |num_qubits, num_results, seed, noise| {
            NoisySimulator::new(num_qubits as usize, num_results as usize, seed, noise)
        };
        py_run(
            py,
            input,
            num_qubits,
            num_results,
            shots,
            noise_config,
            seed,
            make_simulator,
        )
    } else {
        let make_simulator =
            |num_qubits, num_results, seed, _noise: Arc<CumulativeNoiseConfig<Fault>>| {
                NoiselessSimulator::new(num_qubits as usize, num_results as usize, seed, ())
            };
        py_run(
            py,
            input,
            num_qubits,
            num_results,
            shots,
            noise_config,
            seed,
            make_simulator,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn py_run<'py, SimulatorBuilder, Noise, S>(
    py: Python<'py>,
    input: &Bound<'py, PyList>,
    num_qubits: u32,
    num_results: u32,
    shots: u32,
    noise_config: Option<&Bound<'py, NoiseConfig>>,
    seed: Option<u32>,
    make_simulator: SimulatorBuilder,
) -> PyResult<Py<PyAny>>
where
    SimulatorBuilder: Fn(u32, u32, u32, Arc<Noise>) -> S,
    SimulatorBuilder: Send + Sync,
    Noise: From<qdk_simulators::noise_config::NoiseConfig<f64, f64>> + Send + Sync,
    S: Simulator,
{
    // Convert Python list to Vec<QirInstruction>.
    let mut instructions: Vec<QirInstruction> = Vec::with_capacity(input.len());
    for item in input.iter() {
        let item: QirInstruction = item
            .extract()
            .map_err(|e| PyValueError::new_err(format!("expected QirInstruction: {e}")))?;
        instructions.push(item);
    }

    // Convert NoiseConfig to a rust NoiseConfig.
    let noise: qdk_simulators::noise_config::NoiseConfig<f64, f64> =
        if let Some(noise_config) = noise_config {
            unbind_noise_config(py, noise_config)
        } else {
            qdk_simulators::noise_config::NoiseConfig::NOISELESS
        };

    // Run the simulation.
    let output = run(
        &instructions,
        num_qubits,
        num_results,
        shots,
        seed,
        noise,
        make_simulator,
    );

    // Convert results back to Python.
    let mut array = Vec::with_capacity(shots as usize);
    for val in output {
        array.push(
            val.into_py_any(py).map_err(|e| {
                PyValueError::new_err(format!("failed to create Python string: {e}"))
            })?,
        );
    }

    PyList::new(py, array)
        .map_err(|e| PyValueError::new_err(format!("failed to create Python list: {e}")))?
        .into_py_any(py)
}

fn run<SimulatorBuilder, Noise, S>(
    instructions: &[QirInstruction],
    num_qubits: u32,
    num_results: u32,
    shots: u32,
    seed: Option<u32>,
    mut noise: noise_config::NoiseConfig<f64, f64>,
    make_simulator: SimulatorBuilder,
) -> Vec<String>
where
    SimulatorBuilder: Fn(u32, u32, u32, Arc<Noise>) -> S,
    SimulatorBuilder: Send + Sync,
    Noise: From<noise_config::NoiseConfig<f64, f64>> + Send + Sync,
    S: Simulator,
{
    if !noise.rz.is_noiseless() {
        if noise.s.is_noiseless() {
            noise.s = noise.rz.clone();
        }
        if noise.z.is_noiseless() {
            noise.z = noise.rz.clone();
        }
        if noise.s_adj.is_noiseless() {
            noise.s_adj = noise.rz.clone();
        }
    }

    let noise: Noise = noise.into();
    let noise = Arc::new(noise);

    // Create a random number generator to generate the seed for each individual shot.
    let mut rng = if let Some(seed) = seed {
        StdRng::seed_from_u64(seed.into())
    } else {
        StdRng::from_entropy()
    };

    // run the shots
    let output = (0..shots)
        .map(|_| rng.r#gen())
        .collect::<Vec<u32>>()
        .par_iter()
        .map(|shot_seed| {
            let simulator = make_simulator(num_qubits, num_results, *shot_seed, noise.clone());
            run_shot(instructions, simulator)
        })
        .collect::<Vec<_>>();

    // Convert results to a list of strings.
    let mut values = Vec::with_capacity(shots as usize);
    for shot_result in output {
        let mut buffer = String::with_capacity(shot_result.len());
        for measurement in shot_result {
            match measurement {
                MeasurementResult::Zero => write!(&mut buffer, "0").expect("write should succeed"),
                MeasurementResult::One => write!(&mut buffer, "1").expect("write should succeed"),
                MeasurementResult::Loss => write!(&mut buffer, "L").expect("write should succeed"),
            }
        }
        values.push(buffer);
    }
    values
}

fn run_shot(instructions: &[QirInstruction], mut sim: impl Simulator) -> Vec<MeasurementResult> {
    for qir_inst in instructions {
        match qir_inst {
            QirInstruction::OneQubitGate(id, qubit) => match id {
                QirInstructionId::H => sim.h(*qubit as usize),
                QirInstructionId::X => sim.x(*qubit as usize),
                QirInstructionId::Y => sim.y(*qubit as usize),
                QirInstructionId::Z => sim.z(*qubit as usize),
                QirInstructionId::S => sim.s(*qubit as usize),
                QirInstructionId::SAdj => sim.s_adj(*qubit as usize),
                QirInstructionId::SX => sim.sx(*qubit as usize),
                QirInstructionId::SXAdj => sim.sx_adj(*qubit as usize),
                QirInstructionId::T => sim.t(*qubit as usize),
                QirInstructionId::TAdj => sim.t_adj(*qubit as usize),
                QirInstructionId::Move => sim.mov(*qubit as usize),
                QirInstructionId::RESET => sim.resetz(*qubit as usize),
                _ => panic!("unsupported one-qubit gate: {id:?}"),
            },
            QirInstruction::TwoQubitGate(id, q1, q2) => match id {
                QirInstructionId::CX => sim.cx(*q1 as usize, *q2 as usize),
                QirInstructionId::CZ => sim.cz(*q1 as usize, *q2 as usize),
                QirInstructionId::MZ | QirInstructionId::M => sim.mz(*q1 as usize, *q2 as usize),
                QirInstructionId::MResetZ => sim.mresetz(*q1 as usize, *q2 as usize),
                QirInstructionId::SWAP => sim.swap(*q1 as usize, *q2 as usize),
                _ => panic!("unsupported two-qubits gate: {id:?}"),
            },
            QirInstruction::OneQubitRotationGate(id, angle, qubit) => match id {
                QirInstructionId::RX => sim.rx(*angle, *qubit as usize),
                QirInstructionId::RY => sim.ry(*angle, *qubit as usize),
                QirInstructionId::RZ => sim.rz(*angle, *qubit as usize),
                _ => {
                    panic!("unsupported one-qubit rotation gate: {id:?}");
                }
            },
            QirInstruction::TwoQubitRotationGate(id, angle, qubit1, qubit2) => match id {
                QirInstructionId::RXX => sim.rxx(*angle, *qubit1 as usize, *qubit2 as usize),
                QirInstructionId::RYY => sim.ryy(*angle, *qubit1 as usize, *qubit2 as usize),
                QirInstructionId::RZZ => sim.rzz(*angle, *qubit1 as usize, *qubit2 as usize),
                _ => panic!("unsupported two-qubit rotation gate: {id:?}"),
            },
            QirInstruction::CorrelatedNoise(_id, intrinsic_id, qubits) => {
                sim.correlated_noise_intrinsic(
                    *intrinsic_id,
                    &qubits.iter().map(|q| *q as usize).collect::<Vec<_>>(),
                );
            }
            QirInstruction::OutputRecording(_id, _s, _tag) => {
                // Ignore for now.
            }
            QirInstruction::ThreeQubitGate(..) => {
                panic!("unsupported instruction: {qir_inst:?}")
            }
        }
    }

    sim.take_measurements()
}

#[cfg(test)]
mod tests {

    mod test_utils {
        #![allow(dead_code)]

        use crate::qir_simulation::{QirInstruction, QirInstructionId};

        // ==================== Instruction Builder Functions ====================
        // These functions create QirInstruction values for use in check_sim! tests.

        // Single-qubit gates
        pub fn i(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::I, q)
        }
        pub fn h(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::H, q)
        }
        pub fn x(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::X, q)
        }
        pub fn y(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::Y, q)
        }
        pub fn z(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::Z, q)
        }
        pub fn s(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::S, q)
        }
        pub fn s_adj(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::SAdj, q)
        }
        pub fn sx(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::SX, q)
        }
        pub fn sx_adj(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::SXAdj, q)
        }
        pub fn t(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::T, q)
        }
        pub fn t_adj(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::TAdj, q)
        }
        pub fn mov(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::Move, q)
        }
        pub fn reset(q: u32) -> QirInstruction {
            QirInstruction::OneQubitGate(QirInstructionId::RESET, q)
        }

        // Two-qubit gates
        pub fn cnot(q1: u32, q2: u32) -> QirInstruction {
            QirInstruction::TwoQubitGate(QirInstructionId::CNOT, q1, q2)
        }
        pub fn cx(q1: u32, q2: u32) -> QirInstruction {
            QirInstruction::TwoQubitGate(QirInstructionId::CX, q1, q2)
        }
        pub fn cy(q1: u32, q2: u32) -> QirInstruction {
            QirInstruction::TwoQubitGate(QirInstructionId::CY, q1, q2)
        }
        pub fn cz(q1: u32, q2: u32) -> QirInstruction {
            QirInstruction::TwoQubitGate(QirInstructionId::CZ, q1, q2)
        }
        pub fn swap(q1: u32, q2: u32) -> QirInstruction {
            QirInstruction::TwoQubitGate(QirInstructionId::SWAP, q1, q2)
        }
        pub fn m(q: u32, r: u32) -> QirInstruction {
            QirInstruction::TwoQubitGate(QirInstructionId::M, q, r)
        }
        pub fn mz(q: u32, r: u32) -> QirInstruction {
            QirInstruction::TwoQubitGate(QirInstructionId::MZ, q, r)
        }
        pub fn mresetz(q: u32, r: u32) -> QirInstruction {
            QirInstruction::TwoQubitGate(QirInstructionId::MResetZ, q, r)
        }

        // Three-qubit gates
        pub fn ccx(q1: u32, q2: u32, q3: u32) -> QirInstruction {
            QirInstruction::ThreeQubitGate(QirInstructionId::CCX, q1, q2, q3)
        }

        // Single-qubit rotation gates
        pub fn rx(angle: f64, q: u32) -> QirInstruction {
            QirInstruction::OneQubitRotationGate(QirInstructionId::RX, angle, q)
        }
        pub fn ry(angle: f64, q: u32) -> QirInstruction {
            QirInstruction::OneQubitRotationGate(QirInstructionId::RY, angle, q)
        }
        pub fn rz(angle: f64, q: u32) -> QirInstruction {
            QirInstruction::OneQubitRotationGate(QirInstructionId::RZ, angle, q)
        }

        // Two-qubit rotation gates
        pub fn rxx(angle: f64, q1: u32, q2: u32) -> QirInstruction {
            QirInstruction::TwoQubitRotationGate(QirInstructionId::RXX, angle, q1, q2)
        }
        pub fn ryy(angle: f64, q1: u32, q2: u32) -> QirInstruction {
            QirInstruction::TwoQubitRotationGate(QirInstructionId::RYY, angle, q1, q2)
        }
        pub fn rzz(angle: f64, q1: u32, q2: u32) -> QirInstruction {
            QirInstruction::TwoQubitRotationGate(QirInstructionId::RZZ, angle, q1, q2)
        }

        // Correlated noise intrinsic
        pub fn noise_intrinsic(id: u32, qubits: &[u32]) -> QirInstruction {
            QirInstruction::CorrelatedNoise(QirInstructionId::CorrelatedNoise, id, qubits.to_vec())
        }

        // ==================== Macros ====================

        /// Macro to build a `NoiseConfig` for testing.
        ///
        /// # Example
        /// ```ignore
        /// noise_config! {
        ///     rx: {
        ///         x: 1e-5,
        ///         z: 1e-10,
        ///         loss: 1e-10,
        ///     },
        ///     rxx: {
        ///         ix: 1e-10,
        ///         xi: 1e-10,
        ///         xx: 1e-5,
        ///         loss: 1e-10,
        ///     },
        ///     intrinsics: {
        ///         0: {
        ///             iizz: 1e-4,
        ///             ixix: 2e-4,
        ///         },
        ///         1: {
        ///             iziz: 1e-4,
        ///             iizz: 1e-5,
        ///         },
        ///     },
        /// }
        /// ```
        macro_rules! noise_config {
            // Entry point
            ( $( $field:ident : { $($inner:tt)* } ),* $(,)? ) => {{
                #[allow(unused_mut)]
                let mut config = noise_config::NoiseConfig::<f64, f64>::NOISELESS;
                $(
                    noise_config!(@field config, $field, { $($inner)* });
                )*
                config
            }};

            // Handle intrinsics field specially
            (@field $config:ident, intrinsics, { $( $id:literal : { $($pauli:ident : $prob:expr),* $(,)? } ),* $(,)? }) => {{
                $(
                    let mut table = noise_config::NoiseTable::<f64>::noiseless(0);
                    $(
                        noise_config!(@set_pauli table, $pauli, $prob);
                    )*
                    $config.intrinsics.insert($id, table);
                )*
            }};

            // Handle regular gate fields (single-qubit gates)
            (@field $config:ident, i, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.i, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, x, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.x, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, y, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.y, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, z, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.z, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, h, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.h, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, s, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.s, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, s_adj, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.s_adj, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, t, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.t, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, t_adj, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.t_adj, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, sx, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.sx, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, sx_adj, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.sx_adj, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, rx, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.rx, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, ry, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.ry, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, rz, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.rz, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, mov, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.mov, 1, $($pauli : $prob),*);
            }};
            (@field $config:ident, mresetz, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.mresetz, 1, $($pauli : $prob),*);
            }};

            // Handle two-qubit gate fields
            (@field $config:ident, cx, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.cx, 2, $($pauli : $prob),*);
            }};
            (@field $config:ident, cz, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.cz, 2, $($pauli : $prob),*);
            }};
            (@field $config:ident, rxx, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.rxx, 2, $($pauli : $prob),*);
            }};
            (@field $config:ident, ryy, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.ryy, 2, $($pauli : $prob),*);
            }};
            (@field $config:ident, rzz, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.rzz, 2, $($pauli : $prob),*);
            }};
            (@field $config:ident, swap, { $($pauli:ident : $prob:expr),* $(,)? }) => {{
                noise_config!(@set_table $config.swap, 2, $($pauli : $prob),*);
            }};

            // Helper to set a noise table with the given number of qubits
            (@set_table $table:expr, $qubits:expr, $($pauli:ident : $prob:expr),* $(,)?) => {{
                let mut table = noise_config::NoiseTable::<f64>::noiseless($qubits);
                $(
                    noise_config!(@set_pauli table, $pauli, $prob);
                )*
                $table = table;
            }};

            // Helper to set a single pauli entry
            (@set_pauli $table:ident, loss, $prob:expr) => {{
                $table.loss = $prob;
            }};
            (@set_pauli $table:ident, $pauli:ident, $prob:expr) => {{
                let pauli_str = stringify!($pauli).to_uppercase();
                // Update qubits if needed based on pauli string length
                #[allow(clippy::cast_possible_truncation)]
                if $table.qubits == 0 {
                    $table.qubits = pauli_str.len() as u32;
                }
                $table.pauli_strings.push(pauli_str);
                $table.probabilities.push($prob);
            }};
        }

        #[cfg(test)]
        pub(crate) use noise_config;

        /// Macro to build a program (list of QIR instructions) for testing.
        ///
        /// # Example
        /// ```ignore
        /// qir! {
        ///     x(0);
        ///     cx(0, 1);
        ///     mresetz(0, 0);
        ///     mresetz(1, 1);
        /// }
        /// ```
        /// expands to `vec![x(0), cx(0, 1), mresetz(0, 0), mresetz(1, 1)]`
        macro_rules! qir {
            ( $($inst:expr);* $(;)? ) => {{
                vec![$($inst),*]
            }};
        }

        #[cfg(test)]
        pub(crate) use qir;

        /// Macro to build and run a simulation test.
        ///
        /// # Required fields:
        /// - `simulator`: One of `StabilizerSimulator`, `NoisySimulator`, or `NoiselessSimulator`
        /// - `program`: An expression that evaluates to `Vec<QirInstruction>` (use `qir!` macro)
        /// - `num_qubits`: The number of qubits in the simulation
        /// - `num_results`: The number of measurement results
        /// - `expect`: The expected output (using `expect!` macro)
        ///
        /// # Optional fields:
        /// - `shots`: Number of shots (defaults to 1)
        /// - `seed`: Random seed (defaults to None)
        /// - `noise`: A `NoiseConfig` built with `noise_config!` macro (defaults to NOISELESS)
        /// - `format`: A function to format the output (defaults to `raw`)
        ///
        /// # Available format functions:
        /// - `raw`: Joins all results with newlines (default)
        /// - `histogram`: Counts occurrences of each result
        /// - `histogram_percent`: Shows percentages for each result
        /// - `top_n(n)`: Shows only top N results by count (descending)
        /// - `top_n_percent(n)`: Shows only top N results with percentages (descending)
        /// - `count`: Shows the total number of shots
        /// - `summary`: Shows shots, unique count, and loss count
        /// - `loss_count`: Counts results with qubit loss
        ///
        /// # Example
        /// ```ignore
        /// check_sim! {
        ///     simulator: NoisySimulator,
        ///     program: qir! {
        ///         x(2);
        ///         swap(2, 7);
        ///         mresetz(2, 0);
        ///         mresetz(7, 1);
        ///     },
        ///     num_qubits: 8,
        ///     num_results: 2,
        ///     shots: 100,
        ///     seed: 42,
        ///     noise: noise_config! { ... },
        ///     format: histogram,
        ///     expect: expect![[r#"..."#]],
        /// }
        /// ```
        macro_rules! check_sim {
            // Main entry with all fields
            (
                simulator: $sim:ident,
                program: $program:expr,
                num_qubits: $num_qubits:expr,
                num_results: $num_results:expr,
                $( shots: $shots:expr, )?
                $( seed: $seed:expr, )?
                $( noise: $noise:expr, )?
                $( format: $format:expr, )?
                expect: $expected:expr $(,)?
            ) => {{
                // Get instructions from the expression
                let instructions: Vec<QirInstruction> = $program;

                // Set defaults
                let shots: u32 = check_sim!(@default_shots $( $shots )?);
                let seed: Option<u32> = check_sim!(@default_seed $( $seed )?);
                let noise: noise_config::NoiseConfig<f64, f64> = check_sim!(@default_noise $( $noise )?);
                let format_fn = check_sim!(@default_format $( $format )?);

                // Create simulator and run
                let output = check_sim!(@run $sim, &instructions, $num_qubits, $num_results, shots, seed, noise);

                // Format output using the specified format function
                let result_str = format_fn(&output);

                // Assert with expect
                $expected.assert_eq(&result_str);
            }};

            // Default shots
            (@default_shots $shots:expr) => { $shots };
            (@default_shots) => { 1 };

            // Default seed
            (@default_seed $seed:expr) => { Some($seed) };
            (@default_seed) => { None };

            // Default noise
            (@default_noise $noise:expr) => { $noise };
            (@default_noise) => { noise_config::NoiseConfig::<f64, f64>::NOISELESS };

            // Default format
            (@default_format $format:expr) => { $format };
            (@default_format) => { raw };

            // Run with StabilizerSimulator
            (@run StabilizerSimulator, $instructions:expr, $num_qubits:expr, $num_results:expr, $shots:expr, $seed:expr, $noise:expr) => {{
                let make_simulator = |num_qubits, num_results, seed, noise| {
                    StabilizerSimulator::new(num_qubits as usize, num_results as usize, seed, noise)
                };
                run($instructions, $num_qubits, $num_results, $shots, $seed, $noise, make_simulator)
            }};

            // Run with NoisySimulator
            (@run NoisySimulator, $instructions:expr, $num_qubits:expr, $num_results:expr, $shots:expr, $seed:expr, $noise:expr) => {{
                use qdk_simulators::cpu_full_state_simulator::noise::Fault;
                let make_simulator = |num_qubits, num_results, seed, noise| {
                    NoisySimulator::new(num_qubits as usize, num_results as usize, seed, noise)
                };
                run::<_, CumulativeNoiseConfig<Fault>, _>($instructions, $num_qubits, $num_results, $shots, $seed, $noise, make_simulator)
            }};

            // Run with NoiselessSimulator
            (@run NoiselessSimulator, $instructions:expr, $num_qubits:expr, $num_results:expr, $shots:expr, $seed:expr, $noise:expr) => {{
                use qdk_simulators::cpu_full_state_simulator::noise::Fault;
                let make_simulator = |num_qubits, num_results, seed, _noise: Arc<CumulativeNoiseConfig<Fault>>| {
                    NoiselessSimulator::new(num_qubits as usize, num_results as usize, seed, ())
                };
                run::<_, CumulativeNoiseConfig<Fault>, _>($instructions, $num_qubits, $num_results, $shots, $seed, $noise, make_simulator)
            }};
        }

        #[cfg(test)]
        pub(crate) use check_sim;

        // ==================== Format Functions ====================
        // These functions format the output of the simulator for testing.
        // Use them with the `format:` field in `check_sim!`.

        /// Helper function to normalize simulator output by converting 'L' (loss) to '-'.
        /// This ensures consistent loss representation across the test infrastructure.
        fn normalize_output(output: &[String]) -> Vec<String> {
            output.iter().map(|s| s.replace('L', "-")).collect()
        }

        /// Raw format: joins all shot results with newlines.
        /// This is the default format.
        /// Example: "010\n110\n001"
        pub fn raw(output: &[String]) -> String {
            let output = normalize_output(output);
            output.join("\n")
        }

        /// Histogram format: counts occurrences of each result and displays them sorted.
        /// Useful for verifying probability distributions across many shots.
        /// Example: "001: 25\n010: 50\n110: 25"
        pub fn histogram(output: &[String]) -> String {
            use std::collections::BTreeMap;
            let output = normalize_output(output);
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for result in &output {
                *counts.entry(result.as_str()).or_insert(0) += 1;
            }
            counts
                .into_iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join("\n")
        }

        /// Histogram with percentages: shows each result with its percentage.
        /// Useful for verifying probability distributions with percentages.
        /// Example: "001: 25.00%\n010: 50.00%\n110: 25.00%"
        #[allow(clippy::cast_precision_loss)]
        pub fn histogram_percent(output: &[String]) -> String {
            use std::collections::BTreeMap;
            let output = normalize_output(output);
            let total = output.len() as f64;
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for result in &output {
                *counts.entry(result.as_str()).or_insert(0) += 1;
            }
            counts
                .into_iter()
                .map(|(k, v)| format!("{k}: {:.2}%", (v as f64 / total) * 100.0))
                .collect::<Vec<_>>()
                .join("\n")
        }

        /// Top N histogram: shows only the top N results by count, sorted by frequency (descending).
        /// Useful for large quantum simulations where histograms are noisy.
        /// Example with `top_n(3)`: "010: 50\n001: 30\n110: 15"
        pub fn top_n(n: usize) -> impl Fn(&[String]) -> String {
            move |output: &[String]| {
                use std::collections::BTreeMap;
                let output = normalize_output(output);
                let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
                for result in &output {
                    *counts.entry(result.as_str()).or_insert(0) += 1;
                }
                let mut sorted: Vec<_> = counts.into_iter().collect();
                sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
                sorted
                    .into_iter()
                    .take(n)
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }

        /// Top N histogram with percentages: shows only the top N results by count with percentages.
        /// Useful for large quantum simulations where histograms are noisy.
        /// Example with `top_n_percent(3)`: "010: 50.00%\n001: 30.00%\n110: 15.00%"
        #[allow(clippy::cast_precision_loss)]
        pub fn top_n_percent(n: usize) -> impl Fn(&[String]) -> String {
            move |output: &[String]| {
                use std::collections::BTreeMap;
                let output = normalize_output(output);
                let total = output.len() as f64;
                let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
                for result in &output {
                    *counts.entry(result.as_str()).or_insert(0) += 1;
                }
                let mut sorted: Vec<_> = counts.into_iter().collect();
                sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
                sorted
                    .into_iter()
                    .take(n)
                    .map(|(k, v)| format!("{k}: {:.2}%", (v as f64 / total) * 100.0))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }

        /// Count format: shows the total number of shots.
        /// Useful for quick sanity checks on shot count.
        /// Example: "100"
        pub fn count(output: &[String]) -> String {
            output.len().to_string()
        }

        /// Summary format: shows shots, unique count, and loss count.
        /// Useful for debugging and getting a quick overview of results.
        /// Example: "shots: 100\nunique: 3\nloss: 5"
        pub fn summary(output: &[String]) -> String {
            use std::collections::BTreeSet;
            let output = normalize_output(output);
            let unique_results: BTreeSet<&str> = output.iter().map(String::as_str).collect();
            let loss_count = output.iter().filter(|s| s.contains('-')).count();
            format!(
                "shots: {}\nunique: {}\nloss: {}",
                output.len(),
                unique_results.len(),
                loss_count
            )
        }

        /// Loss count format: counts how many results contain loss ('-').
        /// Useful for testing noisy simulations with qubit loss.
        ///
        /// Example output:
        /// ```text
        /// total: 100
        /// loss: 5
        /// no_loss: 95
        /// ```
        pub fn loss_count(output: &[String]) -> String {
            let output = normalize_output(output);
            let loss_count = output.iter().filter(|s| s.contains('-')).count();
            let no_loss_count = output.len() - loss_count;
            format!(
                "total: {}\nloss: {}\nno_loss: {}",
                output.len(),
                loss_count,
                no_loss_count
            )
        }
    }

    mod full_state_noiseless {
        use super::{super::*, test_utils::*};
        use expect_test::expect;
        use std::f64::consts::PI;

        // ==================== Single-Qubit Gate Tests ====================

        #[test]
        fn x_gate_flips_qubit() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    x(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn double_x_gate_returns_to_zero() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    x(0);
                    x(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        #[test]
        fn y_gate_flips_qubit() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    y(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn z_gate_preserves_zero_state() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    z(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        #[test]
        fn z_gate_applies_phase() {
            // H·Z·H = X, which flips |0⟩ to |1⟩
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    h(0);
                    z(0);
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn h_gate_creates_superposition() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                shots: 100,
                seed: 42,
                format: histogram,
                expect: expect![[r#"
                    0: 46
                    1: 54"#]],
            }
        }

        #[test]
        fn double_h_gate_returns_to_zero() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    h(0);
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        #[test]
        fn s_gate_preserves_computational_basis() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    s(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        #[test]
        fn s_gate_applies_phase() {
            // S = sqrt(Z), so S·S = Z
            // H·Z·H = X, which flips |0⟩ to |1⟩
            // Therefore H·S·S·H|0⟩ = |1⟩
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    h(0);
                    s(0);
                    s(0);
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn s_adj_cancels_s() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    h(0);
                    s(0);
                    s_adj(0);
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        #[test]
        fn t_gate_preserves_computational_basis() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    t(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        #[test]
        fn t_gate_applies_phase() {
            // T = sqrt(S) = fourth root of Z, so T^4 = Z
            // H·Z·H = X, which flips |0⟩ to |1⟩
            // Therefore H·T·T·T·T·H|0⟩ = |1⟩
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    h(0);
                    t(0);
                    t(0);
                    t(0);
                    t(0);
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn t_adj_cancels_t() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    h(0);
                    t(0);
                    t_adj(0);
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        // ==================== Rotation Gate Tests ====================

        #[test]
        fn rx_pi_flips_qubit() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    rx(PI, 0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn ry_pi_flips_qubit() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    ry(PI, 0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn rz_preserves_zero_state() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    rz(PI, 0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        #[test]
        fn rz_pi_equivalent_to_z() {
            // RZ(π) applies a π phase, equivalent to Z (up to global phase)
            // H·RZ(π)·H should flip |0⟩ to |1⟩
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    h(0);
                    rz(PI, 0);
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn rx_half_pi_creates_superposition() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    rx(PI / 2.0, 0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                shots: 100,
                seed: 42,
                format: histogram,
                expect: expect![[r#"
                    0: 46
                    1: 54"#]],
            }
        }

        // ==================== Two-Qubit Gate Tests ====================

        #[test]
        fn cx_gate_entangles_qubits() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    x(0);
                    cx(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"11"#]],
            }
        }

        #[test]
        fn cx_gate_no_flip_when_control_is_zero() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    cx(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"00"#]],
            }
        }

        #[test]
        fn cz_gate_preserves_computational_basis() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    x(0);
                    x(1);
                    cz(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"11"#]],
            }
        }

        #[test]
        fn cz_gate_applies_phase() {
            // CZ applies a phase flip when both qubits are |1⟩
            // Start with Bell state |00⟩ + |11⟩, apply CZ to get |00⟩ - |11⟩
            // Then reverse Bell circuit: CX followed by H on control
            // |00⟩ - |11⟩ → CX → |00⟩ - |10⟩ → H⊗I → |10⟩
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    h(0);
                    cx(0, 1);
                    cz(0, 1);
                    cx(0, 1);
                    h(0);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"10"#]],
            }
        }

        #[test]
        fn swap_gate_exchanges_qubit_states() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    x(0);
                    swap(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"01"#]],
            }
        }

        #[test]
        fn double_swap_returns_to_original() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    x(0);
                    swap(0, 1);
                    swap(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"10"#]],
            }
        }

        // ==================== Two-Qubit Rotation Gate Tests ====================

        #[test]
        fn rxx_pi_creates_bell_state() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    rxx(PI / 2.0, 0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                shots: 100,
                seed: 42,
                format: histogram,
                expect: expect![[r#"
                    00: 46
                    11: 54"#]],
            }
        }

        #[test]
        fn ryy_pi_creates_bell_state() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    ryy(PI / 2.0, 0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                shots: 100,
                seed: 42,
                format: histogram,
                expect: expect![[r#"
                    00: 46
                    11: 54"#]],
            }
        }

        #[test]
        fn rzz_preserves_computational_basis() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    rzz(PI, 0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"00"#]],
            }
        }

        #[test]
        fn rzz_applies_phase() {
            // Start with |01⟩, apply H⊗H to get |+⟩|-⟩
            // RZZ(π) transforms |+⟩|-⟩ to |-⟩|+⟩ (with global phase)
            // H⊗H transforms |-⟩|+⟩ to |10⟩
            // Without RZZ, H⊗H would return |+−⟩ back to |01⟩
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    x(1);
                    h(0);
                    h(1);
                    rzz(PI, 0, 1);
                    h(0);
                    h(1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"10"#]],
            }
        }

        // ==================== Bell State Tests ====================

        #[test]
        fn bell_state_produces_correlated_measurements() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    h(0);
                    cx(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                shots: 100,
                seed: 42,
                format: histogram,
                expect: expect![[r#"
                    00: 49
                    11: 51"#]],
            }
        }

        // ==================== Reset Tests ====================

        #[test]
        fn reset_returns_qubit_to_zero() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    x(0);
                    reset(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        #[test]
        fn mresetz_resets_after_measurement() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    x(0);
                    mresetz(0, 0);
                    mresetz(0, 1);
                },
                num_qubits: 1,
                num_results: 2,
                expect: expect![[r#"10"#]],
            }
        }

        // ==================== Multi-Qubit Tests ====================

        #[test]
        fn ghz_state_three_qubits() {
            check_sim! {
                simulator: NoiselessSimulator,
                program: qir! {
                    h(0);
                    cx(0, 1);
                    cx(1, 2);
                    mresetz(0, 0);
                    mresetz(1, 1);
                    mresetz(2, 2);
                },
                num_qubits: 3,
                num_results: 3,
                shots: 100,
                seed: 42,
                format: histogram,
                expect: expect![[r#"
                    000: 51
                    111: 49"#]],
            }
        }
    }

    mod full_state_noisy {
        use super::{super::*, test_utils::*};
        use expect_test::expect;

        // ==================== Basic Noisy Tests ====================

        #[test]
        fn noiseless_config_produces_clean_results() {
            check_sim! {
                simulator: NoisySimulator,
                program: qir! {
                    x(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                shots: 100,
                noise: noise_config! {},
                format: histogram,
                expect: expect![[r#"1: 100"#]],
            }
        }

        #[test]
        fn x_noise_causes_bit_flips() {
            // High X noise on X gate should cause some results to flip back to 0
            check_sim! {
                simulator: NoisySimulator,
                program: qir! {
                    x(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                shots: 1000,
                seed: 42,
                noise: noise_config! {
                    x: {
                        x: 0.1,
                    },
                },
                format: histogram,
                expect: expect![[r#"
                    0: 97
                    1: 903"#]],
            }
        }

        #[test]
        fn z_noise_does_not_affect_computational_basis() {
            // Z noise should not change measurement outcomes in computational basis
            check_sim! {
                simulator: NoisySimulator,
                program: qir! {
                    x(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                shots: 100,
                seed: 42,
                noise: noise_config! {
                    x: {
                        z: 0.5,
                    },
                },
                format: histogram,
                expect: expect![[r#"1: 100"#]],
            }
        }

        #[test]
        fn loss_noise_produces_loss_marker() {
            check_sim! {
                simulator: NoisySimulator,
                program: qir! {
                    x(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                shots: 1000,
                seed: 42,
                noise: noise_config! {
                    x: {
                        loss: 0.1,
                    },
                },
                format: summary,
                expect: expect![[r#"
                    shots: 1000
                    unique: 2
                    loss: 119"#]],
            }
        }

        // ==================== Two-Qubit Noise Tests ====================

        #[test]
        fn cx_noise_affects_entangled_qubits() {
            check_sim! {
                simulator: NoisySimulator,
                program: qir! {
                    x(0);
                    cx(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                shots: 1000,
                seed: 42,
                noise: noise_config! {
                    cx: {
                        xi: 0.05,
                        ix: 0.05,
                    },
                },
                format: top_n(4),
                expect: expect![[r#"
                    11: 908
                    10: 56
                    01: 36"#]],
            }
        }

        // ==================== Hadamard with Noise ====================

        #[test]
        fn hadamard_with_noise_still_produces_superposition() {
            check_sim! {
                simulator: NoisySimulator,
                program: qir! {
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                shots: 100,
                seed: 42,
                noise: noise_config! {
                    h: {
                        x: 0.01,
                        z: 0.01,
                    },
                },
                format: histogram,
                expect: expect![[r#"
                    0: 46
                    1: 54"#]],
            }
        }

        // ==================== Multiple Gates with Noise ====================

        #[test]
        fn bell_state_with_noise_produces_errors() {
            check_sim! {
                simulator: NoisySimulator,
                program: qir! {
                    h(0);
                    cx(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                shots: 1000,
                seed: 42,
                noise: noise_config! {
                    h: {
                        x: 0.02,
                    },
                    cx: {
                        xi: 0.02,
                        ix: 0.02,
                    },
                },
                format: top_n(4),
                expect: expect![[r#"
                    00: 491
                    11: 481
                    01: 18
                    10: 10"#]],
            }
        }
    }

    mod clifford {
        use super::{super::*, test_utils::*};
        use expect_test::expect;

        // ==================== Single-Qubit Clifford Gate Tests ====================

        #[test]
        fn x_gate_flips_qubit() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    x(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn y_gate_flips_qubit() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    y(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn z_gate_preserves_zero() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    z(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        #[test]
        fn z_gate_applies_phase() {
            // H·Z·H = X, which flips |0⟩ to |1⟩
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    h(0);
                    z(0);
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn h_gate_creates_superposition() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                shots: 100,
                seed: 42,
                format: histogram,
                expect: expect![[r#"
                    0: 50
                    1: 50"#]],
            }
        }

        #[test]
        fn s_gate_preserves_computational_basis() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    s(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        #[test]
        fn s_gate_applies_phase() {
            // S = sqrt(Z), so S·S = Z
            // H·Z·H = X, which flips |0⟩ to |1⟩
            // Therefore H·S·S·H|0⟩ = |1⟩
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    h(0);
                    s(0);
                    s(0);
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"1"#]],
            }
        }

        #[test]
        fn s_adj_cancels_s() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    h(0);
                    s(0);
                    s_adj(0);
                    h(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        // ==================== Two-Qubit Clifford Gate Tests ====================

        #[test]
        fn cx_gate_entangles_qubits() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    x(0);
                    cx(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"11"#]],
            }
        }

        #[test]
        fn cz_gate_preserves_computational_basis() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    x(0);
                    x(1);
                    cz(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"11"#]],
            }
        }

        #[test]
        fn cz_gate_applies_phase() {
            // CZ applies a phase flip when both qubits are |1⟩
            // Start with Bell state |00⟩ + |11⟩, apply CZ to get |00⟩ - |11⟩
            // Then reverse Bell circuit: CX followed by H on control
            // |00⟩ - |11⟩ → CX → |00⟩ - |10⟩ → H⊗I → |10⟩
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    h(0);
                    cx(0, 1);
                    cz(0, 1);
                    cx(0, 1);
                    h(0);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"10"#]],
            }
        }

        #[test]
        fn swap_gate_exchanges_qubit_states() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    x(0);
                    swap(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                expect: expect![[r#"01"#]],
            }
        }

        // ==================== Bell State Tests ====================

        #[test]
        fn bell_state_produces_correlated_measurements() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    h(0);
                    cx(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                shots: 100,
                seed: 42,
                format: histogram,
                expect: expect![[r#"
                    00: 58
                    11: 42"#]],
            }
        }

        // ==================== GHZ State Test ====================

        #[test]
        fn ghz_state_three_qubits() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    h(0);
                    cx(0, 1);
                    cx(1, 2);
                    mresetz(0, 0);
                    mresetz(1, 1);
                    mresetz(2, 2);
                },
                num_qubits: 3,
                num_results: 3,
                shots: 100,
                seed: 42,
                format: histogram,
                expect: expect![[r#"
                    000: 56
                    111: 44"#]],
            }
        }

        // ==================== Reset Tests ====================

        #[test]
        fn reset_returns_qubit_to_zero() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    x(0);
                    reset(0);
                    mresetz(0, 0);
                },
                num_qubits: 1,
                num_results: 1,
                expect: expect![[r#"0"#]],
            }
        }

        #[test]
        fn mresetz_resets_after_measurement() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    x(0);
                    mresetz(0, 0);
                    mresetz(0, 1);
                },
                num_qubits: 1,
                num_results: 2,
                expect: expect![[r#"10"#]],
            }
        }

        // ==================== Noisy Stabilizer Tests ====================

        #[test]
        fn stabilizer_with_noise() {
            check_sim! {
                simulator: StabilizerSimulator,
                program: qir! {
                    x(0);
                    cx(0, 1);
                    mresetz(0, 0);
                    mresetz(1, 1);
                },
                num_qubits: 2,
                num_results: 2,
                shots: 1000,
                seed: 42,
                noise: noise_config! {
                    cx: {
                        xi: 0.05,
                        ix: 0.05,
                    },
                },
                format: top_n(4),
                expect: expect![[r#"
                    11: 908
                    10: 56
                    01: 36"#]],
            }
        }
    }
}
