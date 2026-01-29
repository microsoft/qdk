// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// #![allow(dead_code)]

use crate::qir_simulation::{QirInstruction, QirInstructionId, cpu_simulators::run_shot};

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
///
/// The macro also supports the `within { } apply { }` construct for
/// the conjugation pattern (apply within, then apply, then reverse within):
/// ```ignore
/// qir! {
///     x(0);
///     within {
///         x(1);
///         h(1);
///     } apply {
///         cz(0, 1);
///     }
///     mresetz(0, 0);
/// }
/// ```
/// expands to `vec![x(0), x(1), h(1), cz(0, 1), h(1), x(1), mresetz(0, 0)]`
macro_rules! qir {
    // Internal rule: base case - empty input
    (@accum [$($acc:expr),*] ) => {
        vec![$($acc),*]
    };

    // Match within { } apply { } followed by semicolon and more instructions
    (@accum [$($acc:expr),*] within { $($within_tt:tt)* } apply { $($apply_tt:tt)* } ; $($rest:tt)*) => {{
        compile_error!("semicolon after a within-apply block")
    }};

    // Match within { } apply { } at the end (no trailing semicolon or more instructions)
    (@accum [$($acc:expr),*] within { $($within_tt:tt)* } apply { $($apply_tt:tt)* } $($rest:tt)*) => {{
        let mut result: Vec<QirInstruction> = vec![$($acc),*];
        result.extend(qir!($($within_tt)*));  // forward within
        result.extend(qir!($($apply_tt)*));   // apply
        let within_rev: Vec<QirInstruction> = {
            let mut v = qir!($($within_tt)*); // expand tokens again for reverse
            v.reverse();
            v
        };
        result.extend(within_rev);
        let remaining: Vec<QirInstruction> = qir!(@accum [] $($rest)*);
        result.extend(remaining);
        result
    }};

    // Match a single instruction followed by semicolon and more
    (@accum [$($acc:expr),*] $inst:expr ; $($rest:tt)*) => {
        qir!(@accum [$($acc,)* $inst] $($rest)*)
    };

    // Match final instruction without trailing semicolon
    (@accum [$($acc:expr),*] $inst:expr) => {
        qir!(@accum [$($acc,)* $inst])
    };

    // Entry point
    ( $($tokens:tt)* ) => {
        qir!(@accum [] $($tokens)*)
    };
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
///     output: expect![[r#"..."#]],
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
        output: $expected:expr $(,)?
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

/// Macro to check that multiple QIR programs are equivalent.
///
/// This macro runs each program in the list with a fresh simulator and compares their
/// final states. The programs are considered equivalent if they produce the same state
/// (up to global phase, as supported by the simulator's `state_dump` comparison).
///
/// # Required fields:
/// - `simulator`: One of `StabilizerSimulator`, `NoisySimulator`, or `NoiselessSimulator`
/// - `programs`: An array of expressions evaluating to `Vec<QirInstruction>` (use `qir!` macro)
/// - `num_qubits`: The number of qubits in the simulation
/// - `num_results`: The number of measurement results
///
/// # Example
/// ```ignore
/// check_programs_are_eq! {
///     simulator: NoiselessSimulator,
///     programs: [
///         qir! { i(0) },
///         qir! { x(0); x(0); }
///     ],
///     num_qubits: 1,
///     num_results: 0,
/// }
/// ```
macro_rules! check_programs_are_eq {
    (
        simulator: $sim:ident,
        programs: [ $( $program:expr ),+ $(,)? ],
        num_qubits: $num_qubits:expr,
        num_results: $num_results:expr $(,)?
    ) => {{
        use qdk_simulators::Simulator;
        let programs: Vec<Vec<QirInstruction>> = vec![ $( $program ),+ ];
        let simulators: Vec<_> = programs
            .iter()
            .map(|program| {
                check_programs_are_eq!(@run_and_get_sim $sim, program, $num_qubits, $num_results)
            })
            .collect();

        // Compare all states to the first one
        for (i, sim) in simulators.iter().enumerate().skip(1) {
            assert!(
                simulators[0].state_dump() == sim.state_dump(),
                "Program 0 and program {} produce different states.\n\
                        Program 0 state dump:\n{:#?}\n\n\
                        Program {} state dump:\n{:#?}",
                i,
                simulators[0].state_dump(),
                i,
                simulators[1].state_dump(),
            );
        }
    }};

    // Run with NoiselessSimulator and return the simulator
    (@run_and_get_sim NoiselessSimulator, $program:expr, $num_qubits:expr, $num_results:expr) => {{
        run_and_get_simulator::<NoiselessSimulator, ()>(
            $program,
            $num_qubits as usize,
            $num_results as usize,
            0,
            (),
        )
    }};

    // Run with NoisySimulator and return the simulator
    (@run_and_get_sim NoisySimulator, $program:expr, $num_qubits:expr, $num_results:expr) => {{
        use qdk_simulators::cpu_full_state_simulator::noise::Fault;
        let noise: Arc<CumulativeNoiseConfig<Fault>> = Arc::new(noise_config::NoiseConfig::<f64, f64>::NOISELESS.into());
        run_and_get_simulator::<NoisySimulator, Arc<CumulativeNoiseConfig<Fault>>>(
            $program,
            $num_qubits as usize,
            $num_results as usize,
            0,
            noise,
        )
    }};

    // Run with StabilizerSimulator and return the simulator
    (@run_and_get_sim StabilizerSimulator, $program:expr, $num_qubits:expr, $num_results:expr) => {{
        use qdk_simulators::stabilizer_simulator::noise::Fault;
        let noise: Arc<CumulativeNoiseConfig<Fault>> = Arc::new(noise_config::NoiseConfig::<f64, f64>::NOISELESS.into());
        run_and_get_simulator::<StabilizerSimulator, Arc<CumulativeNoiseConfig<Fault>>>(
            $program,
            $num_qubits as usize,
            $num_results as usize,
            0,
            noise,
        )
    }};
}

/// Helper function to run a QIR program and return the simulator with its final state.
pub fn run_and_get_simulator<S, N>(
    instructions: &[QirInstruction],
    num_qubits: usize,
    num_results: usize,
    seed: u32,
    noise: N,
) -> S
where
    S: qdk_simulators::Simulator<Noise = N>,
{
    let sim = S::new(num_qubits, num_results, seed, noise);
    run_shot(instructions, sim)
}

#[cfg(test)]
pub(crate) use check_programs_are_eq;

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

/// Outcomes format: shows only the unique outcomes (sorted) without counts.
/// Useful for verifying that only valid outcomes appear in probabilistic tests.
/// Example: "00\n11" for a Bell state
pub fn outcomes(output: &[String]) -> String {
    use std::collections::BTreeSet;
    let output = normalize_output(output);
    let unique_results: BTreeSet<&str> = output.iter().map(String::as_str).collect();
    unique_results.into_iter().collect::<Vec<_>>().join("\n")
}
