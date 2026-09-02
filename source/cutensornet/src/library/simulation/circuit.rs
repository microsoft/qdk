use super::SimulationError;
use super::policy::ExecutionPolicy;
use num_complex::Complex64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gate {
    X { target: u32 },
    H { target: u32 },
    Rx { theta: f64, target: u32 },
    Rz { theta: f64, target: u32 },
    Cnot { control: u32, target: u32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Circuit {
    qubit_count: u32,
    gates: Vec<Gate>,
}

impl Circuit {
    pub fn new(qubit_count: u32) -> Result<Self, SimulationError> {
        if qubit_count == 0 {
            return Err(SimulationError::InvalidCircuit {
                reason: "a circuit must contain at least one qubit".to_string(),
            });
        }
        Ok(Self {
            qubit_count,
            gates: Vec::new(),
        })
    }

    #[must_use]
    pub fn qubit_count(&self) -> u32 {
        self.qubit_count
    }

    #[must_use]
    pub fn gates(&self) -> &[Gate] {
        &self.gates
    }

    pub fn push(&mut self, gate: Gate) -> Result<(), SimulationError> {
        match gate {
            Gate::X { target } | Gate::H { target } => self.validate_qubit(target)?,
            Gate::Rx { theta, target } | Gate::Rz { theta, target } => {
                self.validate_qubit(target)?;
                if !theta.is_finite() {
                    return Err(SimulationError::InvalidCircuit {
                        reason: "rotation angle must be finite".to_string(),
                    });
                }
            }
            Gate::Cnot { control, target } => {
                self.validate_qubit(control)?;
                self.validate_qubit(target)?;
                if control == target {
                    return Err(SimulationError::InvalidCircuit {
                        reason: "CNOT control and target must be different qubits".to_string(),
                    });
                }
            }
        }
        self.gates.push(gate);
        Ok(())
    }

    pub(super) fn trotter_domain_wall(
        qubit_count: u32,
        steps: u32,
        angle: f64,
    ) -> Result<Self, SimulationError> {
        let mut circuit = Self::new(qubit_count)?;
        for target in qubit_count / 2..qubit_count {
            circuit.push(Gate::X { target })?;
        }
        for _ in 0..steps {
            for control in 0..qubit_count - 1 {
                let target = control + 1;
                circuit.push(Gate::Cnot { control, target })?;
                circuit.push(Gate::Rz {
                    theta: angle,
                    target,
                })?;
                circuit.push(Gate::Cnot { control, target })?;
            }
            for target in 0..qubit_count {
                circuit.push(Gate::Rx {
                    theta: angle,
                    target,
                })?;
            }
        }
        Ok(circuit)
    }

    #[must_use]
    pub(super) fn canonical_description(&self) -> String {
        let mut description = format!("width={};", self.qubit_count);
        for gate in &self.gates {
            use std::fmt::Write;
            match gate {
                Gate::X { target } => write!(description, "x:{target};"),
                Gate::H { target } => write!(description, "h:{target};"),
                Gate::Rx { theta, target } => {
                    write!(description, "rx:{target}:{:016x};", theta.to_bits())
                }
                Gate::Rz { theta, target } => {
                    write!(description, "rz:{target}:{:016x};", theta.to_bits())
                }
                Gate::Cnot { control, target } => {
                    write!(description, "cx:{control}:{target};")
                }
            }
            .expect("writing to a String should not fail");
        }
        description
    }

    fn validate_qubit(&self, qubit: u32) -> Result<(), SimulationError> {
        if qubit >= self.qubit_count {
            Err(SimulationError::InvalidCircuit {
                reason: format!(
                    "qubit {qubit} is outside a {}-qubit circuit",
                    self.qubit_count
                ),
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StateReadout {
    FullAmplitudes,
    MetadataOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "resource evidence keeps byte units explicit on every reported quantity"
)]
pub(super) struct WorkspaceReport {
    pub(super) total_bytes: usize,
    pub(super) free_before_bytes: usize,
    pub(super) requested_maximum_bytes: usize,
    pub(super) native_recommended_bytes: usize,
    pub(super) allocated_bytes: usize,
    pub(super) free_after_cleanup_bytes: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "phase evidence keeps seconds explicit on every reported duration"
)]
pub(super) struct StatePhaseTimings {
    pub(super) matrix_construction_seconds: f64,
    pub(super) upload_seconds: f64,
    pub(super) operator_registration_seconds: f64,
    pub(super) finalization_configuration_seconds: f64,
    pub(super) preparation_workspace_sizing_seconds: f64,
    pub(super) workspace_allocation_attachment_seconds: f64,
    pub(super) state_compute_call_seconds: f64,
    pub(super) synchronization_seconds: f64,
    pub(super) output_metadata_transfer_seconds: f64,
    pub(super) host_validation_seconds: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ExecutionReport {
    pub(super) policy: ExecutionPolicy,
    pub(super) target_extents: Vec<Vec<usize>>,
    pub(super) realized_extents: Vec<Vec<usize>>,
    pub(super) maximum_bond: usize,
    pub(super) workspace: WorkspaceReport,
    pub(super) timings: StatePhaseTimings,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimulationResult {
    amplitudes: Option<Vec<Complex64>>,
    pub(super) report: ExecutionReport,
}

impl SimulationResult {
    pub(super) fn new(amplitudes: Option<Vec<Complex64>>, report: ExecutionReport) -> Self {
        Self { amplitudes, report }
    }

    #[must_use]
    pub fn amplitudes(&self) -> Option<&[Complex64]> {
        self.amplitudes.as_deref()
    }
}

pub(super) fn contract_open_mps(
    tensors: &[Vec<Complex64>],
    extents: &[Vec<usize>],
    strides: &[Vec<usize>],
) -> Result<Vec<Complex64>, SimulationError> {
    if tensors.len() < 2 || tensors.len() != extents.len() || tensors.len() != strides.len() {
        return Err(SimulationError::InvalidNativeResult {
            reason: "MPS tensor, extent, and stride counts do not match".to_string(),
        });
    }
    let last = tensors.len() - 1;
    for site in 0..tensors.len() {
        let expected_rank = if site == 0 || site == last { 2 } else { 3 };
        if extents[site].len() != expected_rank || strides[site].len() != expected_rank {
            return Err(SimulationError::InvalidNativeResult {
                reason: format!("MPS site {site} has an invalid rank"),
            });
        }
        let physical_mode = usize::from(site != 0);
        if extents[site][physical_mode] != 2 {
            return Err(SimulationError::InvalidNativeResult {
                reason: format!("MPS site {site} has a non-qubit physical extent"),
            });
        }
        if site > 0 && extents[site - 1][extents[site - 1].len() - 1] != extents[site][0] {
            return Err(SimulationError::InvalidNativeResult {
                reason: format!("MPS bond before site {site} is inconsistent"),
            });
        }
        validate_storage(
            &format!("site {site}"),
            tensors[site].len(),
            &extents[site],
            &strides[site],
        )?;
    }

    let amplitude_count = u32::try_from(tensors.len())
        .ok()
        .and_then(|width| 1_usize.checked_shl(width))
        .ok_or(SimulationError::ResourceSizeOverflow {
            resource: "dense state",
        })?;
    let mut amplitudes = Vec::with_capacity(amplitude_count);
    for basis in 0..amplitude_count {
        let q0 = basis & 1;
        let mut boundary = (0..extents[0][1])
            .map(|right| tensors[0][q0 * strides[0][0] + right * strides[0][1]])
            .collect::<Vec<_>>();
        for site in 1..last {
            let physical = (basis >> site) & 1;
            let mut next = vec![Complex64::new(0.0, 0.0); extents[site][2]];
            for (left, value) in boundary.iter().enumerate() {
                for (right, next_value) in next.iter_mut().enumerate() {
                    let index = left * strides[site][0]
                        + physical * strides[site][1]
                        + right * strides[site][2];
                    *next_value += *value * tensors[site][index];
                }
            }
            boundary = next;
        }
        let physical = (basis >> last) & 1;
        let amplitude = boundary
            .iter()
            .enumerate()
            .map(|(left, value)| {
                *value * tensors[last][left * strides[last][0] + physical * strides[last][1]]
            })
            .sum();
        amplitudes.push(amplitude);
    }
    Ok(amplitudes)
}

fn validate_storage(
    label: &str,
    length: usize,
    extents: &[usize],
    strides: &[usize],
) -> Result<(), SimulationError> {
    if strides.contains(&0) {
        return Err(SimulationError::InvalidNativeResult {
            reason: format!("{label} MPS tensor has a zero stride: {strides:?}"),
        });
    }
    let maximum_index = extents
        .iter()
        .zip(strides)
        .try_fold(0_usize, |index, (extent, stride)| {
            extent
                .checked_sub(1)
                .and_then(|offset| offset.checked_mul(*stride))
                .and_then(|offset| index.checked_add(offset))
        })
        .ok_or_else(|| SimulationError::InvalidNativeResult {
            reason: format!("{label} MPS layout overflows usize"),
        })?;
    if maximum_index >= length {
        return Err(SimulationError::InvalidNativeResult {
            reason: format!(
                "{label} MPS layout requires index {maximum_index}, but capacity is {length}"
            ),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::SimulationError;
    use super::{Circuit, Gate, contract_open_mps};
    use num_complex::Complex64;
    use qdk_simulators::SparseStateSim;
    use std::f64::consts::FRAC_1_SQRT_2;

    #[test]
    fn circuit_rejects_invalid_gate_targets() {
        let mut circuit = Circuit::new(2).expect("two-qubit circuit should be valid");
        circuit
            .push(Gate::X { target: 0 })
            .expect("in-range X gate should be valid");
        assert!(matches!(
            circuit.push(Gate::H { target: 2 }),
            Err(SimulationError::InvalidCircuit { .. })
        ));
        assert!(matches!(
            circuit.push(Gate::Cnot {
                control: 0,
                target: 0,
            }),
            Err(SimulationError::InvalidCircuit { .. })
        ));
        assert_eq!(circuit.gates(), [Gate::X { target: 0 }]);
    }

    #[test]
    fn circuit_rejects_non_finite_rotation_angles() {
        let mut circuit = Circuit::new(2).expect("two-qubit circuit should be valid");

        assert!(matches!(
            circuit.push(Gate::Rx {
                theta: f64::NAN,
                target: 0,
            }),
            Err(SimulationError::InvalidCircuit { .. })
        ));
        assert!(matches!(
            circuit.push(Gate::Rz {
                theta: f64::INFINITY,
                target: 1,
            }),
            Err(SimulationError::InvalidCircuit { .. })
        ));
        assert!(circuit.gates().is_empty());
    }

    #[test]
    fn trotter_fixture_is_deterministic_and_has_the_expected_operation_order() {
        let circuit =
            Circuit::trotter_domain_wall(4, 1, 0.3).expect("Trotter fixture should be valid");
        let repeated =
            Circuit::trotter_domain_wall(4, 1, 0.3).expect("repeated fixture should be valid");

        assert_eq!(circuit, repeated);
        assert_eq!(circuit.gates().len(), 15);
        assert_eq!(
            &circuit.gates()[..5],
            &[
                Gate::X { target: 2 },
                Gate::X { target: 3 },
                Gate::Cnot {
                    control: 0,
                    target: 1,
                },
                Gate::Rz {
                    theta: 0.3,
                    target: 1,
                },
                Gate::Cnot {
                    control: 0,
                    target: 1,
                },
            ]
        );
        assert_eq!(
            circuit.canonical_description(),
            repeated.canonical_description()
        );
    }

    #[test]
    fn approved_b2_fixtures_have_expected_gate_counts() {
        for width in [12, 16, 20] {
            let circuit = Circuit::trotter_domain_wall(width, 8, 0.3)
                .expect("approved fixture should be valid");
            let expected = usize::try_from(width / 2 + 8 * (4 * width - 3))
                .expect("gate count should fit usize");
            assert_eq!(circuit.gates().len(), expected);
            let canonical = circuit.canonical_description();
            assert!(canonical.starts_with(&format!("width={width};")));
            println!("fixture_manifest_width={width}");
            println!("fixture_manifest_operation_count={expected}");
            println!("fixture_manifest_query_terms={}", width - 1);
            println!("fixture_manifest_canonical={canonical}");
        }
    }

    #[test]
    fn b3_fixture_matches_historical_n128_steps16_structure() {
        let circuit =
            Circuit::trotter_domain_wall(128, 16, 0.3).expect("B3 fixture should be valid");
        let expected_operation_count = 128 / 2 + 16 * (4 * 128 - 3);

        assert_eq!(expected_operation_count, 8_208);
        assert_eq!(circuit.gates().len(), expected_operation_count);
        assert_eq!(
            &circuit.gates()[..64],
            &(64..128)
                .map(|target| Gate::X { target })
                .collect::<Vec<_>>()
        );
        assert_eq!(
            &circuit.gates()[64..67],
            &[
                Gate::Cnot {
                    control: 0,
                    target: 1,
                },
                Gate::Rz {
                    theta: 0.3,
                    target: 1,
                },
                Gate::Cnot {
                    control: 0,
                    target: 1,
                },
            ]
        );
        assert_eq!(
            &circuit.gates()[445..448],
            &[
                Gate::Rx {
                    theta: 0.3,
                    target: 0,
                },
                Gate::Rx {
                    theta: 0.3,
                    target: 1,
                },
                Gate::Rx {
                    theta: 0.3,
                    target: 2,
                },
            ]
        );
        println!("b3_fixture_width=128");
        println!("b3_fixture_steps=16");
        println!("b3_fixture_operation_count={}", circuit.gates().len());
        println!("b3_fixture_query_terms=127");
        println!("b3_fixture_canonical={}", circuit.canonical_description());
    }

    #[test]
    fn b4_fixture_has_the_approved_n256_steps16_structure() {
        let circuit =
            Circuit::trotter_domain_wall(256, 16, 0.3).expect("B4 fixture should be valid");
        let expected_operation_count = 256 / 2 + 16 * (4 * 256 - 3);

        assert_eq!(expected_operation_count, 16_464);
        assert_eq!(circuit.gates().len(), expected_operation_count);
        println!("b4_fixture_width=256");
        println!("b4_fixture_steps=16");
        println!("b4_fixture_operation_count={}", circuit.gates().len());
        println!("b4_fixture_query_terms=255");
        println!("b4_fixture_canonical={}", circuit.canonical_description());
    }

    #[test]
    fn n12_trotter_query_matches_qdk_sparse_exact_oracle() {
        let started = std::time::Instant::now();
        let width = 12_u32;
        let circuit =
            Circuit::trotter_domain_wall(width, 8, 0.3).expect("approved fixture should be valid");
        let mut simulator = SparseStateSim::new(None);
        for expected in 0..usize::try_from(width).expect("width should fit usize") {
            assert_eq!(simulator.allocate(), expected);
        }
        for gate in circuit.gates() {
            match *gate {
                Gate::X { target } => simulator.x(target as usize),
                Gate::Rx { theta, target } => simulator.rx(theta, target as usize),
                Gate::Rz { theta, target } => simulator.rz(theta, target as usize),
                Gate::Cnot { control, target } => {
                    simulator.mcx(&[control as usize], target as usize);
                }
                Gate::H { .. } => panic!("the Trotter fixture contains no Hadamard gates"),
            }
        }
        let query = (0..usize::try_from(width - 1).expect("width should fit usize"))
            .map(|left| 1.0 - 2.0 * simulator.joint_probability(&[left, left + 1]))
            .sum::<f64>();
        let expected = 4.332_869_154_633;
        let relative_error = ((query - expected) / expected).abs();

        println!("independent_oracle=QDK SparseStateSim");
        println!("independent_oracle_width=12");
        println!("independent_oracle_query={query:.15}");
        println!("independent_oracle_expected={expected:.15}");
        println!("independent_oracle_relative_error={relative_error:.17e}");
        println!(
            "independent_oracle_elapsed_seconds={:.9}",
            started.elapsed().as_secs_f64()
        );

        assert!(
            relative_error <= 1.0e-12,
            "QDK sparse exact Query {query:.15} differed from {expected:.15} by {relative_error:e} relative"
        );
    }

    #[test]
    fn contracts_bell_mps_in_little_endian_order() {
        let zero = Complex64::new(0.0, 0.0);
        let scale = Complex64::new(FRAC_1_SQRT_2, 0.0);
        let one = Complex64::new(1.0, 0.0);
        let left = [scale, zero, zero, scale];
        let right = [one, zero, zero, one];

        let result = contract_open_mps(
            &[left.to_vec(), right.to_vec()],
            &[vec![2, 2], vec![2, 2]],
            &[vec![2, 1], vec![2, 1]],
        )
        .expect("valid Bell MPS should contract");

        assert_eq!(result, [scale, zero, zero, scale]);
    }

    #[test]
    fn contraction_honors_runtime_element_strides() {
        let zero = Complex64::new(0.0, 0.0);
        let one = Complex64::new(1.0, 0.0);
        let left = [zero, one];
        let right = [one, zero];

        let result = contract_open_mps(
            &[left.to_vec(), right.to_vec()],
            &[vec![2, 1], vec![1, 2]],
            &[vec![1, 2], vec![2, 1]],
        )
        .expect("valid product-state MPS should contract");

        assert_eq!(result, [zero, one, zero, zero]);
    }

    #[test]
    fn contracts_three_site_product_state_with_interior_tensor() {
        let zero = Complex64::new(0.0, 0.0);
        let one = Complex64::new(1.0, 0.0);
        let result = contract_open_mps(
            &[vec![zero, one], vec![one, zero], vec![zero, one]],
            &[vec![2, 1], vec![1, 2, 1], vec![1, 2]],
            &[vec![1, 2], vec![4, 1, 2], vec![2, 1]],
        )
        .expect("valid three-site product MPS should contract");

        assert_eq!(result, basis_state(3, 5));
    }

    #[test]
    fn rejects_malformed_native_mps_layouts() {
        let zero = Complex64::new(0.0, 0.0);
        let tensors = [vec![zero; 4], vec![zero; 4]];
        let invalid = [
            contract_open_mps(&tensors, &[vec![2, 2]], &[vec![2, 1], vec![2, 1]]),
            contract_open_mps(
                &tensors,
                &[vec![3, 2], vec![2, 2]],
                &[vec![2, 1], vec![2, 1]],
            ),
            contract_open_mps(
                &tensors,
                &[vec![2, 2], vec![1, 2]],
                &[vec![2, 1], vec![2, 1]],
            ),
            contract_open_mps(
                &tensors,
                &[vec![2, 2], vec![2, 2]],
                &[vec![0, 1], vec![2, 1]],
            ),
            contract_open_mps(
                &tensors,
                &[vec![2, 2], vec![2, 2]],
                &[vec![usize::MAX, 1], vec![2, 1]],
            ),
        ];

        for result in invalid {
            assert!(matches!(
                result,
                Err(SimulationError::InvalidNativeResult { .. })
            ));
        }
    }

    fn basis_state(width: usize, index: usize) -> Vec<Complex64> {
        let mut state = vec![Complex64::new(0.0, 0.0); 1 << width];
        state[index] = Complex64::new(1.0, 0.0);
        state
    }
}
