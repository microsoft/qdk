//! Fixed workload qualification through the retained production owner.

use num_complex::Complex64;
use qdk_simulators::execution::{CircuitTensorNetwork, QuantumEvolutionRegion, UnitaryOperation};
use std::{fs, path::PathBuf};

#[path = "experiment.rs"]
mod experiment;

struct Fixture {
    circuit: CircuitTensorNetwork,
    expected: Vec<Complex64>,
    limit: f64,
}

fn fixture(name: &str) -> Fixture {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../samples/python_interop/ising2d_tensor_network_demo/fixtures/i3a_numerical");
    let data = fs::read(directory.join(format!("{name}.json"))).expect("retained fixture");
    let record: serde_json::Value = serde_json::from_slice(&data).expect("fixture JSON");
    assert_eq!(record["name"], name);
    assert_eq!(
        record["angle_encoding"],
        "IEEE754 binary64 bits as unsigned integer"
    );
    let qubits = usize::try_from(record["qubits"].as_u64().expect("width")).expect("width");
    let operations: Vec<_> = record["gates"]
        .as_array()
        .expect("gates")
        .iter()
        .map(|gate| {
            let values = gate.as_array().expect("gate");
            let angle = f64::from_bits(values[1].as_u64().expect("angle bits"));
            let target = usize::try_from(values[2].as_u64().expect("qubit")).expect("qubit");
            match values[0].as_str().expect("kind") {
                "rx" => {
                    assert_eq!(values.len(), 3);
                    UnitaryOperation::Rx { angle, target }
                }
                "rzz" => {
                    assert_eq!(values.len(), 4);
                    UnitaryOperation::Rzz {
                        angle,
                        q1: target,
                        q2: usize::try_from(values[3].as_u64().expect("qubit")).expect("qubit"),
                    }
                }
                kind => panic!("unqualified gate {kind}"),
            }
        })
        .collect();
    let circuit =
        CircuitTensorNetwork::from_zero_state(qubits, &QuantumEvolutionRegion::new(operations))
            .expect("I2 builder");
    let count = 1_usize << qubits;
    let array = fs::read(directory.join(record["amplitudes"].as_str().expect("amplitude path")))
        .expect("oracle");
    // Only decode the explicitly pinned format of these retained fixtures.
    assert_eq!(&array[..8], b"\x93NUMPY\x01\x00");
    let header_size = usize::from(u16::from_le_bytes(
        array[8..10].try_into().expect("header size"),
    ));
    let header = std::str::from_utf8(&array[10..10 + header_size])
        .expect("ASCII header")
        .trim();
    assert_eq!(
        header,
        format!("{{'descr': '<c16', 'fortran_order': False, 'shape': ({count},), }}")
    );
    let payload = &array[10 + header_size..];
    assert_eq!(payload.len(), count * 16);
    let expected = payload
        .chunks_exact(16)
        .map(|value| {
            Complex64::new(
                f64::from_le_bytes(value[..8].try_into().expect("real")),
                f64::from_le_bytes(value[8..].try_into().expect("imaginary")),
            )
        })
        .collect();
    Fixture {
        circuit,
        expected,
        limit: record["limit"].as_f64().expect("limit"),
    }
}

#[derive(Debug)]
struct Comparison {
    amplitude_error: f64,
    probability_tv: f64,
    squared_norm_error: f64,
}

fn compare(actual: &[Complex64], expected: &[Complex64], limit: f64) -> Result<Comparison, String> {
    if !limit.is_finite()
        || limit < 0.0
        || actual.len() != expected.len()
        || actual.is_empty()
        || actual
            .iter()
            .chain(expected)
            .any(|v| !v.re.is_finite() || !v.im.is_finite())
    {
        return Err("invalid tolerance, amplitude shape or nonfinite value".into());
    }
    let norm: f64 = actual.iter().map(Complex64::norm_sqr).sum();
    let expected_norm: f64 = expected.iter().map(Complex64::norm_sqr).sum();
    if !norm.is_finite()
        || !expected_norm.is_finite()
        || (norm - 1.0).abs() > limit
        || (expected_norm - 1.0).abs() > limit
    {
        return Err(format!(
            "norm before normalization: actual={norm}, expected={expected_norm}, limit={limit}"
        ));
    }
    let report = Comparison {
        amplitude_error: actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (*a - *b).norm())
            .fold(0.0, f64::max),
        probability_tv: actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (a.norm_sqr() / norm - b.norm_sqr() / expected_norm).abs())
            .sum::<f64>()
            / 2.0,
        squared_norm_error: (norm - 1.0).abs(),
    };
    if report.amplitude_error > limit || report.probability_tv > limit {
        return Err(format!(
            "numerical mismatch: {report:?}, limit={limit}; no global-phase alignment"
        ));
    }
    Ok(report)
}

#[test]
fn retained_cases_use_the_i2_builder_and_independent_finite_oracles() {
    for (name, nodes, amplitudes, limit) in [
        ("diagnostic", 9, 8, 1e-12_f64),
        ("case_a_2x2", 92, 16, 1e-12_f64),
        ("case_a_4x4", 448, 65536, 1e-8_f64),
    ] {
        let fixture = fixture(name);
        assert_eq!(fixture.circuit.network().nodes().len(), nodes);
        assert_eq!(fixture.expected.len(), amplitudes);
        assert_eq!(
            fixture.circuit.output_axes().element_count(),
            Some(amplitudes)
        );
        assert_eq!(fixture.limit.to_bits(), limit.to_bits());
        compare(&fixture.expected, &fixture.expected, fixture.limit).expect("valid oracle");
        assert!(
            !fixture
                .circuit
                .query()
                .expect("query")
                .hyperedges()
                .as_slice()
                .is_empty()
        );
    }
}

#[test]
fn comparator_rejects_phase_shape_norm_and_nonfinite_errors() {
    let expected = [Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)];
    for actual in [
        vec![Complex64::new(0.0, 1.0), Complex64::new(0.0, 0.0)],
        vec![Complex64::new(1.0, 0.0)],
        vec![Complex64::new(2.0, 0.0), Complex64::new(0.0, 0.0)],
        vec![Complex64::new(f64::NAN, 0.0), Complex64::new(0.0, 0.0)],
    ] {
        assert!(compare(&actual, &expected, 1e-12).is_err());
    }
    for limit in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(compare(&expected, &expected, limit).is_err());
    }
}

#[test]
fn comparator_enforces_inclusive_amplitude_tv_and_norm_boundaries() {
    let expected = [Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0)];
    let phase = [Complex64::new(0.0, 1.0), Complex64::new(0.0, 0.0)];
    let amplitude_limit = 2.0_f64.sqrt();
    compare(&phase, &expected, amplitude_limit).expect("inclusive amplitude boundary");
    assert!(compare(&phase, &expected, amplitude_limit.next_down()).is_err());

    let scaled = [Complex64::new(0.5, 0.0), Complex64::new(0.0, 0.0)];
    compare(&scaled, &expected, 0.75).expect("inclusive squared-norm boundary");
    assert!(compare(&scaled, &expected, 0.75_f64.next_down()).is_err());

    let uniform = [Complex64::new(1.0 / 8.0_f64.sqrt(), 0.0); 8];
    let mut concentrated = [Complex64::new(0.0, 0.0); 8];
    concentrated[0] = Complex64::new(1.0, 0.0);
    let report = compare(&concentrated, &uniform, 1.0).expect("measure TV boundary");
    compare(&concentrated, &uniform, report.probability_tv).expect("inclusive TV boundary");
    assert!(compare(&concentrated, &uniform, report.probability_tv.next_down()).is_err());

    for limit in [1e-12_f64, 1e-8_f64] {
        let below = [Complex64::from_polar(1.0, limit / 2.0), expected[1]];
        let above = [Complex64::from_polar(1.0, 2.0 * limit), expected[1]];
        compare(&below, &expected, limit).expect("approved amplitude limit");
        assert!(compare(&above, &expected, limit).is_err());
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod native {
    use super::*;
    use crate::simulation::{
        SimulationError,
        contraction::{
            ContractionResources, NativeMetadata, OptimizerEstimate, OptimizerSettings,
            execution::{ContractionExecution, WorkspaceLimits},
        },
        error::combine_execution_and_cleanup,
        memory_workspace::MemoryWorkspaceApi,
        resources::SessionResources,
    };
    use std::{io::Write, sync::Arc};

    fn execute(name: &str) -> Result<(), SimulationError> {
        let fixture = fixture(name);
        let availability = crate::discover().expect("audited libraries required");
        println!(
            "{name}: versions={:?}; CUDA_C_64F/COMPUTE_64F",
            availability.report()
        );
        let settings = OptimizerSettings {
            workspace_constraint: 67_108_864,
            hyper_samples: 1,
            threads: 1,
            seed: 17,
            reconfiguration_iterations: 0,
            disable_rank_simplification: true,
            disable_slicing: true,
        };
        let limits = WorkspaceLimits {
            device_scratch: Some(if name == "case_a_4x4" {
                3 * 1024 * 1024 * 1024
            } else {
                67_108_864
            }),
            host_scratch: Some(1_048_576),
        };
        println!(
            "{name}: settings={settings:?}; limits={limits:?}; remaining SDK defaults unchanged; cache disabled; no autotuning"
        );
        let query = fixture.circuit.query().expect("query");
        let (metadata, modes) = select_metadata(name, &availability, &query, settings)?;
        println!("{name}: owned_metadata={metadata:?}; native_intermediate_modes={modes:?}");
        let mut session = SessionResources::new(Arc::clone(&availability.libraries), 0)?;
        println!(
            "{name}: pid={}; device=0; cuda_stream={:p}",
            std::process::id(),
            session.stream().as_ptr()
        );
        let result = (|| {
            let mut resources = ContractionResources::new(&mut session, &query)?;
            if let Err(error) = resources.import(&metadata) {
                return combine_execution_and_cleanup(Err(error), resources.close());
            }
            println!(
                "{name}: fresh import, source owners closed, no search; IDs={:?}",
                resources.tensor_ids()
            );
            let mut execution = ContractionExecution::prepare(
                resources,
                fixture.circuit.buffers(),
                fixture.circuit.node_buffer_ids(),
                limits,
            )?;
            let result = (|| {
                if execution.metadata()? != metadata {
                    return Err(super::super::unexpected("metadata changed at preparation"));
                }
                let prepared_modes = execution.intermediate_modes()?;
                println!(
                    "{name}: prepared_intermediate_modes={prepared_modes:?}; memory={:?}",
                    execution.memory()
                );
                assert_mode_sets(&modes, &prepared_modes)?;
                for iteration in 0..2 {
                    println!("{name}: contract_begin iteration={iteration}");
                    let output = execution.contract()?;
                    println!("{name}: contract_synchronized_readback iteration={iteration}");
                    save_output(name, iteration, &output)?;
                    let report = compare(&output, &fixture.expected, fixture.limit)
                        .map_err(super::super::unexpected)?;
                    println!(
                        "{name}: iteration={iteration}; comparison={report:?}; limit={}; no phase alignment",
                        fixture.limit
                    );
                }
                if execution.metadata()? != metadata {
                    return Err(super::super::unexpected("metadata changed at execution"));
                }
                assert_mode_sets(&prepared_modes, &execution.intermediate_modes()?)?;
                Ok(())
            })();
            let cleanup = execution.close();
            println!("{name}: execution_cleanup={cleanup:?}");
            combine_execution_and_cleanup(result, cleanup)
        })();
        let memory = session.api().memory_info();
        println!(
            "{name}: device_wide_free_total_after_children={memory:?}; not process allocation accounting"
        );
        let result = combine_execution_and_cleanup(result, memory.map(|_| ()));
        let cleanup = session.close();
        println!("{name}: fresh_session_cleanup={cleanup:?}");
        combine_execution_and_cleanup(result, cleanup)
    }

    fn select_metadata(
        name: &str,
        availability: &crate::Availability,
        query: &tensornet::ContractionQuery<'_>,
        settings: OptimizerSettings,
    ) -> Result<(NativeMetadata, Vec<Vec<i32>>), SimulationError> {
        let mut source = SessionResources::new(Arc::clone(&availability.libraries), 0)?;
        let selected = (|| {
            let mut resources = ContractionResources::new(&mut source, query)?;
            let result = (|| {
                resources.optimize(settings)?;
                for estimate in [
                    OptimizerEstimate::FlopCount,
                    OptimizerEstimate::LargestTensor,
                ] {
                    println!(
                        "{name}: vendor_estimate={estimate:?} value={}; no inferred counting/inclusion convention",
                        resources.estimate(estimate)?
                    );
                }
                Ok((resources.export()?, resources.intermediate_modes()?))
            })();
            let cleanup = resources.close();
            println!("{name}: source_topology_cleanup={cleanup:?}");
            combine_execution_and_cleanup(result, cleanup)
        })();
        let cleanup = source.close();
        println!("{name}: source_session_cleanup={cleanup:?}");
        combine_execution_and_cleanup(selected, cleanup)
    }

    pub(super) fn assert_mode_sets(
        expected: &[Vec<i32>],
        actual: &[Vec<i32>],
    ) -> Result<(), SimulationError> {
        use std::collections::BTreeSet;
        if expected.len() != actual.len()
            || expected
                .iter()
                .zip(actual)
                .any(|(a, b)| a.iter().collect::<BTreeSet<_>>() != b.iter().collect())
        {
            return Err(super::super::unexpected(
                "native intermediate mode sets changed",
            ));
        }
        Ok(())
    }

    pub(super) fn save_output(
        name: &str,
        iteration: usize,
        output: &[Complex64],
    ) -> Result<(), SimulationError> {
        let Some(directory) = std::env::var_os("QDK_CONTRACTION_EVIDENCE_DIR") else {
            println!(
                "{name}: raw readback not retained (set QDK_CONTRACTION_EVIDENCE_DIR for qualification delivery)"
            );
            return Ok(());
        };
        let path = PathBuf::from(directory).join(format!("{name}-{iteration}.complex64le"));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| super::super::unexpected(format!("{}: {error}", path.display())))?;
        for value in output {
            file.write_all(&value.re.to_le_bytes())
                .and_then(|()| file.write_all(&value.im.to_le_bytes()))
                .map_err(|error| {
                    super::super::unexpected(format!("{}: {error}", path.display()))
                })?;
        }
        file.sync_all()
            .map_err(|error| super::super::unexpected(error.to_string()))?;
        println!(
            "{name}: readback={} (interleaved little-endian f64 real/imaginary)",
            path.display()
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires audited cuTensorNet/CUDA and GPU; separate contraction qualification"]
    fn a_asymmetric_diagnostic() {
        execute("diagnostic").expect("native execution and cleanup");
    }
    #[test]
    #[ignore = "requires audited cuTensorNet/CUDA and GPU; separate contraction qualification"]
    fn b_case_a_2x2() {
        execute("case_a_2x2").expect("native execution and cleanup");
    }
    #[test]
    #[ignore = "requires audited cuTensorNet/CUDA and GPU; separate contraction qualification"]
    fn c_case_a_4x4() {
        execute("case_a_4x4").expect("native execution and cleanup");
    }
}
