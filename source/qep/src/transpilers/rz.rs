// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Transpiler;
use crate::{Parameter::Float, Parameter::QubitId, QuantumBackendInstruction};
use rsgridsynth::{config::config_from_theta_epsilon, gridsynth::gridsynth_gates};
use smallvec::smallvec;
use std::{
    collections::BTreeMap,
    f64::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU},
    sync::Mutex,
};

// rsgridsynth changes process-global arithmetic precision during configuration.
static SYNTHESIS_LOCK: Mutex<()> = Mutex::new(());

/// Decomposes finite Rz rotations into Clifford+T gates, up to global phase.
pub struct RzTranspiler {
    precision: f64,
    use_cache: bool,
    // Positive finite f64 bit patterns have the same ordering as their numeric values.
    cache: Mutex<BTreeMap<u64, String>>,
}

impl Default for RzTranspiler {
    /// Uses a synthesis tolerance of 1e-6 with caching disabled.
    fn default() -> Self {
        Self {
            precision: 1e-6,
            use_cache: false,
            cache: Mutex::default(),
        }
    }
}

impl RzTranspiler {
    /// Creates a synthesizer with the given tolerance in (0, 1) and optional angle caching.
    pub fn new(precision: f64, use_cache: bool) -> Result<Self, String> {
        if !precision.is_finite() || precision <= 0.0 || precision >= 1.0 {
            return Err(format!(
                "Rz synthesis precision must be finite and in (0, 1), got {precision}"
            ));
        }
        Ok(Self {
            precision,
            use_cache,
            cache: Mutex::default(),
        })
    }

    /// Borrows the nearest cached sequence when its angle is strictly within precision.
    fn lookup_cache<'a>(&self, cache: &'a BTreeMap<u64, String>, residual: f64) -> Option<&'a str> {
        let key = residual.to_bits();
        let (distance, gates) = cache
            .range(..=key)
            .next_back()
            .into_iter()
            .chain(cache.range(key..).next())
            .map(|(&bits, gates)| ((f64::from_bits(bits) - residual).abs(), gates))
            .min_by(|(left, _), (right, _)| left.total_cmp(right))?;
        (distance < self.precision).then_some(gates.as_str())
    }

    /// Decodes a compact sequence into static backend gate names without copying its string.
    fn append_sequence(sequence: &str, output: &mut Vec<&'static str>) -> Result<(), String> {
        for gate in sequence.chars() {
            output.push(match gate {
                'h' => "h",
                't' => "t",
                's' => "s",
                'x' => "x",
                _ => return Err(format!("Unsupported synthesized Rz gate: {gate}")),
            });
        }
        Ok(())
    }

    /// Appends a cached residual sequence or synthesizes and stores a new one.
    fn synthesize_residual(
        &self,
        residual: f64,
        output: &mut Vec<&'static str>,
    ) -> Result<(), String> {
        let mut cache = if self.use_cache {
            Some(
                self.cache
                    .lock()
                    .map_err(|error| format!("Rz synthesis cache lock poisoned: {error}"))?,
            )
        } else {
            None
        };
        if let Some(cache) = &cache
            && let Some(gates) = self.lookup_cache(cache, residual)
        {
            return Self::append_sequence(gates, output);
        }

        let _guard = SYNTHESIS_LOCK
            .lock()
            .map_err(|error| format!("Rz synthesis lock poisoned: {error}"))?;
        let mut config = config_from_theta_epsilon(residual, self.precision, 0, false, true)
            .with_compute_error(true);
        let result = gridsynth_gates(&mut config);
        if result.is_correct != Some(true) {
            return Err(format!(
                "Rz synthesis failed for residual {residual} at precision {}: error {:?}",
                self.precision, result.error
            ));
        }
        let mut gates = String::new();
        // The library returns matrix order; W is a scalar global phase.
        for gate in result.gates.chars().rev() {
            match gate {
                'I' | 'W' => {}
                'H' | 'T' | 'S' | 'X' => gates.push(gate.to_ascii_lowercase()),
                _ => return Err(format!("Unsupported rsgridsynth gate: {gate}")),
            }
        }
        Self::append_sequence(&gates, output)?;
        if let Some(cache) = &mut cache {
            cache.insert(residual.to_bits(), gates);
        }
        Ok(())
    }

    /// Extracts exact phase gates and synthesizes only the remaining angle in (0, pi/4).
    fn synthesize(&self, angle: f64) -> Result<Vec<&'static str>, String> {
        // Trigonometric range reduction avoids the large error of angle % TAU for huge angles.
        let angle = if angle.abs() > TAU {
            angle.sin().atan2(angle.cos())
        } else {
            angle
        };
        let mut residual = angle.abs() % TAU;
        let mut gates = Vec::new();
        for (rotation, gate) in [(PI, "z"), (FRAC_PI_2, "s"), (FRAC_PI_4, "t")] {
            if residual >= rotation {
                residual -= rotation;
                gates.push(gate);
            }
        }

        if residual > self.precision {
            self.synthesize_residual(residual, &mut gates)?;
        }
        if angle < 0.0 {
            gates.reverse();
            for gate in &mut gates {
                *gate = match *gate {
                    "s" => "sadj",
                    "t" => "tadj",
                    other => other,
                };
            }
        }
        Ok(gates)
    }
}

impl Transpiler for RzTranspiler {
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
        if !angle.is_finite() {
            return Err(format!("Rz angle must be finite, got {angle}"));
        }
        let gates = self.synthesize(angle)?;
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
    use crate::{QuantumBackends, TranspilingAdapter, tests::run_qsharp_main};
    use num_complex::Complex64;
    use qsc_eval::backend::Backend;
    use std::f64::consts::FRAC_1_SQRT_2;

    /// Builds a rotation on a nonzero qubit ID to check operand preservation.
    fn rotation(angle: f64) -> QuantumBackendInstruction {
        QuantumBackendInstruction {
            instruction_name: "rz",
            parameters: smallvec![QubitId(7), Float(angle)],
        }
    }

    /// Reconstructs both columns of the emitted unitary and measures operator-norm error.
    fn synthesis_error(angle: f64, precision: f64) -> Result<f64, String> {
        let mut output = Vec::new();
        RzTranspiler::new(precision, false)?.transpile(&rotation(angle), &mut output)?;
        let zero = Complex64::new(0.0, 0.0);
        let one = Complex64::new(1.0, 0.0);
        let mut matrix = [[one, zero], [zero, one]];
        for instruction in output {
            assert_eq!(instruction.parameters.as_slice(), &[QubitId(7)]);
            let [top, bottom] = &mut matrix;
            for (top, bottom) in top.iter_mut().zip(bottom) {
                let [a, b] = [*top, *bottom];
                let state = match instruction.instruction_name {
                    "h" => [(a + b) * FRAC_1_SQRT_2, (a - b) * FRAC_1_SQRT_2],
                    "x" => [b, a],
                    "z" => [a, -b],
                    "s" => [a, b * Complex64::i()],
                    "sadj" => [a, -b * Complex64::i()],
                    "t" => [a, b * Complex64::from_polar(1.0, FRAC_PI_4)],
                    "tadj" => [a, b * Complex64::from_polar(1.0, -FRAC_PI_4)],
                    other => panic!("Unexpected synthesized gate: {other}"),
                };
                *top = state[0];
                *bottom = state[1];
            }
        }
        let expected = [
            Complex64::from_polar(1.0, -angle / 2.0),
            Complex64::from_polar(1.0, angle / 2.0),
        ];
        let overlap = matrix[0][0] * expected[0].conj() + matrix[1][1] * expected[1].conj();
        let phase = overlap / overlap.norm();
        matrix[0][0] -= phase * expected[0];
        matrix[1][1] -= phase * expected[1];
        let a = matrix[0][0].norm_sqr() + matrix[1][0].norm_sqr();
        let d = matrix[0][1].norm_sqr() + matrix[1][1].norm_sqr();
        let b = matrix[0][0].conj() * matrix[0][1] + matrix[1][0].conj() * matrix[1][1];
        Ok(((a + d + ((a - d).powi(2) + 4.0 * b.norm_sqr()).sqrt()) / 2.0).sqrt())
    }

    #[test]
    /// Checks exact phase gates, identity, and negative-angle adjoints without synthesis.
    fn exact_rotations() -> Result<(), String> {
        for (angle, names) in [
            (0.0, vec![]),
            (-0.0, vec![]),
            (TAU, vec![]),
            (-TAU, vec![]),
            (FRAC_PI_4, vec!["t"]),
            (-FRAC_PI_4, vec!["tadj"]),
            (FRAC_PI_2, vec!["s"]),
            (-FRAC_PI_2, vec!["sadj"]),
            (PI, vec!["z"]),
            (PI + FRAC_PI_4, vec!["z", "t"]),
            (-PI - FRAC_PI_4, vec!["tadj", "z"]),
        ] {
            let mut output = Vec::new();
            RzTranspiler::default().transpile(&rotation(angle), &mut output)?;
            assert_eq!(
                output
                    .iter()
                    .map(|gate| gate.instruction_name)
                    .collect::<Vec<_>>(),
                names,
                "angle {angle}"
            );
        }
        Ok(())
    }

    #[test]
    /// Checks arbitrary angles, range reduction, and two requested synthesis tolerances.
    fn arbitrary_rotations() -> Result<(), String> {
        for precision in [1e-4, 1e-10] {
            for angle in [
                0.123,
                0.6,
                FRAC_PI_4 - 1e-8,
                FRAC_PI_4 + 1e-8,
                PI + 0.123,
                TAU - 0.123,
                10.0,
                1e20,
                f64::MAX,
                precision / 2.0,
            ] {
                for angle in [angle, -angle] {
                    let error = synthesis_error(angle, precision)?;
                    assert!(
                        error <= precision,
                        "angle {angle}, precision {precision}, error {error}"
                    );
                }
            }
        }
        Ok(())
    }

    #[test]
    /// Compares QFT states on 6 qubits in debug and 10 in release, up to global phase.
    fn test_rotation_synthesis_qft() -> Result<(), String> {
        let num_qubits = if cfg!(debug_assertions) { 6 } else { 10 };
        let precision = 1e-10;
        let source = format!(
            r#"
                namespace Test {{
                    /// Prepares a basis state and applies the QFT.
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
        let mut reference = QuantumBackends::sparse_sim();
        let mut synthesized = TranspilingAdapter::new(
            QuantumBackends::sparse_sim(),
            vec![Box::new(RzTranspiler::new(precision, true)?)],
        )?;
        run_qsharp_main(&source, &mut reference)?;
        run_qsharp_main(&source, &mut synthesized)?;
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
            .zip(&expected)
            .map(|(actual, expected)| {
                assert_eq!(actual.0, expected.0);
                (actual.1 - global_phase * expected.1).norm_sqr()
            })
            .sum::<f64>()
            .sqrt();
        println!(
            "{num_qubits}-qubit QFT: precision={precision:.e}, state-vector distance={distance:.16e}"
        );
        assert!(distance < 1e-9, "{distance:.e}");
        Ok(())
    }

    #[test]
    /// Serializes library calls made by threads requesting different arithmetic precisions.
    fn concurrent_synthesis() -> Result<(), String> {
        std::thread::scope(|scope| {
            let workers = [1e-4, 1e-10].map(|precision| {
                scope.spawn(move || {
                    let error = synthesis_error(0.345, precision)?;
                    assert!(error <= precision, "precision {precision}, error {error}");
                    Ok::<_, String>(())
                })
            });
            for worker in workers {
                worker.join().expect("synthesis thread panicked")?;
            }
            Ok(())
        })
    }

    #[test]
    /// Reuses close residuals across exact phase gates and negative-angle adjoints.
    fn cache_reuses_normalized_residuals() -> Result<(), String> {
        let precision = 1e-6;
        let transpiler = RzTranspiler::new(precision, true)?;
        let gates = transpiler.synthesize(0.125)?;
        for angle in [0.125, 0.125 - precision / 2.0, 0.125 + precision / 2.0] {
            assert_eq!(transpiler.synthesize(angle)?, gates);
        }
        let mut shifted = vec!["t"];
        shifted.extend(&gates);
        assert_eq!(transpiler.synthesize(FRAC_PI_4 + 0.125)?, shifted);
        let adjoint: Vec<_> = gates
            .iter()
            .rev()
            .map(|&gate| match gate {
                "s" => "sadj",
                "t" => "tadj",
                other => other,
            })
            .collect();
        assert_eq!(transpiler.synthesize(-0.125)?, adjoint);
        assert_eq!(
            transpiler.cache.lock().expect("cache lock poisoned").len(),
            1
        );
        Ok(())
    }

    #[test]
    /// Returns a string slice backed by the cached string rather than an allocated copy.
    fn cache_lookup_borrows_sequence() -> Result<(), String> {
        let transpiler = RzTranspiler::new(0.015_625, true)?;
        let key = 0.125_f64.to_bits();
        let cache = BTreeMap::from([(key, String::from("ht"))]);
        let gates = transpiler
            .lookup_cache(&cache, 0.125)
            .expect("exact cache hit");
        assert!(std::ptr::eq(
            gates,
            cache.get(&key).expect("cached sequence").as_str()
        ));
        assert!(transpiler.lookup_cache(&cache, 0.140_625).is_none());
        Ok(())
    }

    #[test]
    /// Distinguishes the closest predecessor and successor, including queries outside the key range.
    fn cache_selects_nearest_key() -> Result<(), String> {
        let transpiler = RzTranspiler::new(0.03125, true)?;
        {
            let mut cache = transpiler.cache.lock().expect("cache lock poisoned");
            cache.insert(0.125_f64.to_bits(), String::from("h"));
            cache.insert(0.15625_f64.to_bits(), String::from("x"));
        }
        for (angle, expected) in [
            (0.117_187_5, vec!["h"]),
            (0.132_812_5, vec!["h"]),
            (0.148_437_5, vec!["x"]),
            (0.164_062_5, vec!["x"]),
        ] {
            assert_eq!(transpiler.synthesize(angle)?, expected);
        }
        assert_eq!(
            transpiler.cache.lock().expect("cache lock poisoned").len(),
            2
        );
        Ok(())
    }

    #[test]
    /// Inserts new entries when the nearest key is exactly the precision away or farther.
    fn cache_misses_at_precision_boundary() -> Result<(), String> {
        let transpiler = RzTranspiler::new(0.015_625, true)?;
        for angle in [0.125, 0.140_625, 0.25] {
            transpiler.synthesize(angle)?;
        }
        assert_eq!(
            transpiler.cache.lock().expect("cache lock poisoned").len(),
            3
        );
        Ok(())
    }

    #[test]
    /// Leaves the cache empty when caching is disabled.
    fn disabled_cache() -> Result<(), String> {
        let transpiler = RzTranspiler::new(1e-6, false)?;
        let first = transpiler.synthesize(0.125)?;
        assert_eq!(transpiler.synthesize(0.125)?, first);
        assert!(
            transpiler
                .cache
                .lock()
                .expect("cache lock poisoned")
                .is_empty()
        );
        assert!(!RzTranspiler::default().use_cache);
        Ok(())
    }

    #[test]
    /// Shares a single cached result between concurrent requests for the same angle.
    fn concurrent_cache_requests() -> Result<(), String> {
        let transpiler = RzTranspiler::new(1e-6, true)?;
        std::thread::scope(|scope| {
            let first = scope.spawn(|| transpiler.synthesize(0.125));
            let second = scope.spawn(|| transpiler.synthesize(0.125));
            assert_eq!(
                first.join().expect("synthesis thread panicked")?,
                second.join().expect("synthesis thread panicked")?
            );
            Ok::<_, String>(())
        })?;
        assert_eq!(
            transpiler.cache.lock().expect("cache lock poisoned").len(),
            1
        );
        Ok(())
    }

    #[test]
    /// Rejects invalid tolerances, angles, instruction names, and operands without appending output.
    fn invalid_inputs() {
        for precision in [0.0, -1.0, 1.0, f64::NAN, f64::INFINITY] {
            for use_cache in [false, true] {
                assert!(RzTranspiler::new(precision, use_cache).is_err());
            }
        }
        let mut invalid = vec![
            rotation(f64::NAN),
            rotation(f64::INFINITY),
            rotation(f64::NEG_INFINITY),
        ];
        invalid.push(QuantumBackendInstruction {
            instruction_name: "rx",
            parameters: rotation(0.1).parameters,
        });
        invalid.push(QuantumBackendInstruction {
            instruction_name: "rz",
            parameters: smallvec![Float(0.1), QubitId(7)],
        });
        invalid.push(QuantumBackendInstruction {
            instruction_name: "rz",
            parameters: smallvec![QubitId(7)],
        });
        for instruction in invalid {
            let mut output = vec![rotation(0.0)];
            assert!(
                RzTranspiler::default()
                    .transpile(&instruction, &mut output)
                    .is_err()
            );
            assert_eq!(output, vec![rotation(0.0)]);
        }
    }
}
