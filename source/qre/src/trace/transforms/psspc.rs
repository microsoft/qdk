// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::trace::{Gate, TraceTransform};
use crate::{Error, Trace, instruction_ids};

/// Implements the Parellel Synthesis Sequential Pauli Computation (PSSPC)
/// layout algorithm described in Appendix D in
/// [arXiv:2211.07629](https://arxiv.org/pdf/2211.07629).  This scheme combines
/// sequential Pauli-based computation (SPC) as described in
/// [arXiv:1808.02892](https://arxiv.org/pdf/1808.02892) and
/// [arXiv:2109.02746](https://arxiv.org/pdf/2109.02746) with an approach to
/// synthesize sets of diagonal non-Cliﬀord unitaries in parallel as done in
/// [arXiv:2110.11493](https://arxiv.org/pdf/2110.11493).
///
/// References:
/// - Michael E. Beverland, Prakash Murali, Matthias Troyer, Krysta M. Svore,
///   Torsten Hoefler, Vadym Kliuchnikov, Guang Hao Low, Mathias Soeken, Aarthi
///   Sundaram, Alexander Vaschillo: Assessing requirements to scale to
///   practical quantum advantage,
///   [arXiv:2211.07629](https://arxiv.org/pdf/2211.07629)
/// - Daniel Litinski: A Game of Surface Codes: Large-Scale Quantum Computing
///   with Lattice Surgery, [arXiv:1808.02892](https://arxiv.org/pdf/1808.02892)
/// - Christopher Chamberland, Earl T. Campbell: Universal quantum computing
///   with twist-free and temporally encoded lattice surgery,
///   [arXiv:2109.02746](https://arxiv.org/pdf/2109.02746)
/// - Michael Beverland, Vadym Kliuchnikov, Eddie Schoute: Surface code
///   compilation via edge-disjoint paths,
///   [arXiv:2110.11493](https://arxiv.org/pdf/2110.11493).
#[derive(Clone)]
pub struct PSSPC {
    /// Number of multi-qubit Pauli measurements to inject a synthesized
    /// rotation, defaults to 1, see [arXiv:2211.07629, (D3)]
    num_measurements_per_r: u64,
    /// Number of multi-qubit Pauli measurements to apply a Toffoli gate,
    /// defaults to 3, see [arXiv:2211.07629, (D3)]
    num_measurements_per_ccx: u64,
    /// Number of Pauli measurements to write to memory, defaults to 2, see
    /// [arXiv:2109.02746, Fig. 16a]
    num_measurements_per_wtm: u64,
    /// Number of Pauli measurements to read from memory, defaults to 1, see
    /// [arXiv:2109.02746, Fig. 16b]
    num_measurements_per_rfm: u64,

    /// Number of Ts per rotation synthesis
    num_ts_per_rotation: u64,
    /// Perform Toffoli gates using CCX magic states, if false, T gates are used
    ccx_magic_states: bool,
}

impl PSSPC {
    #[must_use]
    pub fn new(num_ts_per_rotation: u64, ccx_magic_states: bool) -> Self {
        Self {
            num_measurements_per_r: 1,
            num_measurements_per_ccx: 3,
            num_measurements_per_wtm: 2,
            num_measurements_per_rfm: 1,
            num_ts_per_rotation,
            ccx_magic_states,
        }
    }
}

impl PSSPC {
    #[allow(clippy::cast_possible_truncation)]
    fn psspc_counts(trace: &Trace) -> Result<PSSPCCounts, Error> {
        let mut counter = PSSPCCounts::default();

        let mut max_rotation_depth = vec![0; trace.compute_qubits() as usize];

        for (Gate { id, qubits, .. }, mult) in trace.deep_iter() {
            if instruction_ids::is_pauli_measurement(*id) {
                counter.measurements += mult;
            } else if instruction_ids::is_t_like(*id) {
                counter.t_like += mult;
            } else if instruction_ids::is_ccx_like(*id) {
                counter.ccx_like += mult;
            } else if instruction_ids::is_rotation_like(*id) {
                counter.rotation_like += mult;

                // Track rotation depth
                let mut current_depth = 0;
                for q in qubits {
                    if max_rotation_depth[*q as usize] > current_depth {
                        current_depth = max_rotation_depth[*q as usize];
                    }
                }
                let new_depth = current_depth + mult;
                for q in qubits {
                    max_rotation_depth[*q as usize] = new_depth;
                }
                if new_depth > counter.rotation_depth {
                    counter.rotation_depth = new_depth;
                }
            } else if *id == instruction_ids::READ_FROM_MEMORY {
                counter.read_from_memory += mult;
            } else if *id == instruction_ids::WRITE_TO_MEMORY {
                counter.write_to_memory += mult;
            } else if !instruction_ids::is_clifford(*id) {
                // Unsupported non-Clifford gate
                return Err(Error::UnsupportedInstruction {
                    id: *id,
                    name: "PSSPC",
                });
            } else {
                // For Clifford gates, synchronize depths across qubits
                if !qubits.is_empty() {
                    let mut max_depth = 0;
                    for q in qubits {
                        if max_rotation_depth[*q as usize] > max_depth {
                            max_depth = max_rotation_depth[*q as usize];
                        }
                    }
                    for q in qubits {
                        max_rotation_depth[*q as usize] = max_depth;
                    }
                }
            }
        }

        Ok(counter)
    }

    #[allow(clippy::cast_precision_loss)]
    fn compute_only_trace(&self, trace: &Trace, counts: &PSSPCCounts) -> Trace {
        let num_qubits = trace.compute_qubits();
        let logical_qubits = Self::logical_qubit_overhead(num_qubits);

        let mut transformed = trace.clone_empty(Some(logical_qubits));

        let logical_depth = self.logical_depth_overhead(counts);
        let (t_states, ccx_states) = self.num_magic_states(counts);

        transformed.increment_resource_state(instruction_ids::T, t_states);
        transformed.increment_resource_state(instruction_ids::CCX, ccx_states);

        let block = transformed.add_block(logical_depth);
        block.add_operation(
            instruction_ids::MULTI_PAULI_MEAS,
            (0..logical_qubits).collect(),
            vec![],
        );

        // Add error due to rotation synthesis
        transformed.increment_base_error(counts.rotation_like as f64 * self.synthesis_error());

        transformed
    }

    /// Calculates the number of logical qubits required for the PSSPC layout
    /// according to Eq. (D1) in
    /// [arXiv:2211.07629](https://arxiv.org/pdf/2211.07629)
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn logical_qubit_overhead(algorithm_qubits: u64) -> u64 {
        let qubit_padding = ((8 * algorithm_qubits) as f64).sqrt().ceil() as u64 + 1;
        2 * algorithm_qubits + qubit_padding
    }

    /// Calculates the number of multi-qubit Pauli measurements executed in
    /// sequence according to Eq. (D3) in
    /// [arXiv:2211.07629](https://arxiv.org/pdf/2211.07629)
    fn logical_depth_overhead(&self, counter: &PSSPCCounts) -> u64 {
        (counter.measurements + counter.t_like + counter.rotation_like)
            * self.num_measurements_per_r
            + counter.ccx_like * self.num_measurements_per_ccx
            + counter.read_from_memory * self.num_measurements_per_rfm
            + counter.write_to_memory * self.num_measurements_per_wtm
            + (self.num_ts_per_rotation * counter.rotation_depth) * self.num_measurements_per_r
    }

    /// Calculates the number of T and CCX magic states that are consumed by
    /// multi-qubit Pauli measurements executed by PSSPC according to Eq. (D4)
    /// in [arXiv:2211.07629](https://arxiv.org/pdf/2211.07629)
    ///
    /// CCX magic states are only counted if the hyper parameter
    /// `ccx_magic_states` is set to true.
    fn num_magic_states(&self, counter: &PSSPCCounts) -> (u64, u64) {
        let t_states = counter.t_like + self.num_ts_per_rotation * counter.rotation_like;

        if self.ccx_magic_states {
            (t_states, counter.ccx_like)
        } else {
            (t_states + 4 * counter.ccx_like, 0)
        }
    }

    /// Calculates the synthesis error from the formula provided in Table 1 in
    /// [arXiv:2203.10064](https://arxiv.org/pdf/2203.10064) for Clifford+T in
    /// the mixed fallback approximation protocol.
    #[allow(clippy::cast_precision_loss)]
    fn synthesis_error(&self) -> f64 {
        2f64.powf((4.86 - self.num_ts_per_rotation as f64) / 0.53)
    }
}

impl TraceTransform for PSSPC {
    fn transform(&self, trace: &Trace) -> Result<Trace, Error> {
        let counts = Self::psspc_counts(trace)?;

        Ok(self.compute_only_trace(trace, &counts))
    }
}

#[derive(Default)]
struct PSSPCCounts {
    measurements: u64,
    t_like: u64,
    ccx_like: u64,
    rotation_like: u64,
    write_to_memory: u64,
    read_from_memory: u64,
    rotation_depth: u64,
}
