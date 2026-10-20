// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Transpiler;
use crate::{Parameter::Float, Parameter::QubitId, QuantumBackendInstruction};
use smallvec::smallvec;
use std::f64::consts::PI;

/// Guarenteed upper bound on the error.
const EPSILON: f64 = 1e-10;

/// Entry k approximates Rz(pi / 2^k); Rust parses the generated array at compile time.
const ROTATION_SEQUENCES: &[&str] = include!("qft_rotations.inc");

/// Decomposes Rz rotations with angles of the form +/- pi*2^k into Clifford+T gates.
#[derive(Default)]
pub struct RzQftTranspiler;

impl RzQftTranspiler {
    /// Synthesizes a nonnegative Z rotation as a gate sequence up to global phase.
    /// Returns string of following gates only: x,z,s,h,t.
    fn synthesize_rz(angle: f64) -> Result<&'static str, String> {
        if !angle.is_finite() {
            return Err(format!("Rz angle must be finite, got {angle}"));
        }
        debug_assert!(angle >= 0.0);

        if angle <= EPSILON {
            return Ok("");
        }

        // Finite angles above EPSILON have rounded exponents that fit in i32.
        #[allow(clippy::cast_possible_truncation)]
        let power_of_2 = (angle / PI).log2().round() as i32;
        let exact_angle = PI * 2.0_f64.powi(power_of_2);
        if (angle - exact_angle).abs() > EPSILON {
            return Err(format!(
                "Rz angle {angle} is not within {EPSILON} radians of +/- 2^k * pi"
            ));
        }

        if power_of_2 >= 1 {
            Ok("")
        } else {
            let index = usize::try_from(-power_of_2)
                .expect("nonpositive power has a nonnegative sequence index");
            ROTATION_SEQUENCES.get(index).copied().ok_or_else(|| {
                format!(
                    "Unsupported Rz angle {angle}: no synthesis sequence for power {power_of_2}"
                )
            })
        }
    }

    /// Reverses a gate sequence and replaces each gate with its adjoint.
    fn make_adjoint(gates: Vec<&'static str>) -> Vec<&'static str> {
        gates
            .into_iter()
            .rev()
            .map(|gate| match gate {
                "x" => "x",
                "z" => "z",
                "h" => "h",
                "t" => "tadj",
                "tadj" => "t",
                "s" => "sadj",
                "sadj" => "s",
                _ => panic!("Unsupported gate in adjoint sequence: {gate}"),
            })
            .collect()
    }
}

impl Transpiler for RzQftTranspiler {
    /// Returns the Z-rotation instruction name.
    fn source_instruction_name(&self) -> &'static str {
        "rz"
    }

    /// Appends the synthesized rotation sequence, up to global phase.
    fn transpile(
        &self,
        source: &QuantumBackendInstruction,
        output: &mut Vec<QuantumBackendInstruction>,
    ) -> Result<(), String> {
        if source.instruction_name != self.source_instruction_name() {
            return Err(format!("Expected an Rz instruction, got {source:?}"));
        }
        let &[QubitId(qubit_id), Float(angle)] = source.parameters.as_slice() else {
            return Err(format!("Malformed Rz instruction: {source:?}"));
        };
        let mut gates: Vec<_> = Self::synthesize_rz(angle.abs())?
            .chars()
            .map(|gate| match gate {
                'x' => "x",
                'z' => "z",
                's' => "s",
                'h' => "h",
                't' => "t",
                _ => panic!("Unsupported gate in synthesized sequence: {gate}"),
            })
            .collect();
        if angle < 0.0 {
            gates = Self::make_adjoint(gates);
        }
        output.extend(
            gates
                .into_iter()
                .map(|instruction_name| QuantumBackendInstruction {
                    instruction_name,
                    parameters: smallvec![QubitId(qubit_id)],
                }),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BatchResult, QuantumBackend, QuantumBackends, TranspilingAdapter, run_qir,
        tests::compile_qsharp_to_qir,
    };
    use num_bigint::BigUint;
    use num_complex::Complex64;
    use qsc::target::Profile;
    use qsc_eval::backend::Backend;

    #[test]
    /// Checks synthesis for gates Rz(+/- pi*2^-k).
    /// Checks squared state-vector distance after cancelling global phase.
    fn test_rotation_synthesis_rotations() -> Result<(), String> {
        for power in -40..=3 {
            for sign in [-1.0, 1.0] {
                let angle = sign * 2.0_f64.powi(power) * PI;
                let instructions = [
                    QuantumBackendInstruction {
                        instruction_name: "h",
                        parameters: smallvec![QubitId(0)],
                    },
                    QuantumBackendInstruction {
                        instruction_name: "rz",
                        parameters: smallvec![QubitId(0), Float(angle)],
                    },
                ];
                let mut adapter = TranspilingAdapter::new(
                    QuantumBackends::sparse_sim(),
                    vec![Box::new(RzQftTranspiler)],
                )?;
                assert_eq!(
                    adapter.execute_batch(&instructions)?,
                    BatchResult::default()
                );
                let state = adapter
                    .get_backend_mut()
                    .backend_mut()
                    .capture_quantum_state()?
                    .0;
                assert_eq!(state.len(), 2, "angle {angle}: expected |0> and |1>");
                assert_eq!(state[0].0, BigUint::from(0_u32));
                assert_eq!(state[1].0, BigUint::from(1_u32));
                let magnitude = std::f64::consts::FRAC_1_SQRT_2;
                let expected = [
                    Complex64::new(magnitude, 0.0),
                    Complex64::from_polar(magnitude, angle),
                ];
                // Compare the adapter's Rz(angle)H|0> with expected (|0> + exp(i*angle)|1>)/sqrt(2).
                // The distance must be < EPSILON.
                let cancel_global_phase = Complex64::from_polar(1.0, -state[0].1.arg());
                let distance_squared: f64 = state
                    .iter()
                    .zip(expected)
                    .map(|((_, actual), expected)| {
                        (actual * cancel_global_phase - expected).norm_sqr()
                    })
                    .sum();
                let distance = distance_squared.sqrt();
                assert!(distance < EPSILON);
            }
        }
        Ok(())
    }

    #[test]
    /// Compares compiled QFT states with and without synthesis, up to global phase.
    fn test_rotation_synthesis_qft() -> Result<(), String> {
        if cfg!(debug_assertions) {
            return Ok(()); // Skip in debug mode
        }

        let num_qubits = 10;
        let source = format!(
            r#"
                    namespace Test {{
                        @EntryPoint()
                        operation Main() : Unit {{
                            use qs = Qubit[{num_qubits}];
                            X(qs[0]);
                            X(qs[3]);
                            Std.Canon.ApplyQFT(qs);
                        }}
                    }}
                "#
        );
        let file = compile_qsharp_to_qir(&source, Profile::Base);
        let mut reference = QuantumBackends::sparse_sim();
        let mut synthesized = TranspilingAdapter::new(
            QuantumBackends::sparse_sim(),
            vec![Box::new(RzQftTranspiler)],
        )?;
        // SAFETY: The QIR was compiled from the trusted Q# source above.
        unsafe {
            run_qir(&file.0, &mut reference)?;
            run_qir(&file.0, &mut synthesized)?;
        }
        let expected = reference.backend_mut().capture_quantum_state()?.0;
        let actual = synthesized
            .get_backend_mut()
            .backend_mut()
            .capture_quantum_state()?
            .0;
        assert_eq!(expected.len(), 1 << num_qubits);
        assert_eq!(actual.len(), expected.len());
        let phase_ratio = actual[0].1 / expected[0].1;
        let global_phase = phase_ratio / phase_ratio.norm();
        let distance = actual
            .iter()
            .zip(expected.iter())
            .map(|(actual, expected)| {
                assert_eq!(actual.0, expected.0);
                (actual.1 - global_phase * expected.1).norm_sqr()
            })
            .sum::<f64>()
            .sqrt();
        assert!(distance < 1e-9, "{distance:.e}"); // Actual distance is ~3e-10.
        Ok(())
    }
}
