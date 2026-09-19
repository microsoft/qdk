use super::{Trial, WORKSPACE_BYTES, chronological, error_status};
use crate::simulation::{
    SimulationError,
    contraction::{
        ContractionResources, NativeMetadata, OptimizerEstimate,
        execution::{ContractionExecution, ExecutionMemory, WorkspaceLimits},
        unexpected,
    },
    error::combine_execution_and_cleanup,
    memory_workspace::MemoryWorkspaceApi,
    resources::SessionResources,
};
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use super::super::{
    Fixture, compare, fixture,
    native::{assert_mode_sets, save_output},
};

struct Journal {
    file: File,
    cleanup_failed: bool,
}

impl Journal {
    fn record(&mut self, event: &Value) -> Result<(), SimulationError> {
        serde_json::to_writer(&mut self.file, event)
            .map_err(|error| unexpected(error.to_string()))?;
        self.file
            .write_all(b"\n")
            .and_then(|()| self.file.flush())
            .map_err(|error| unexpected(error.to_string()))
    }

    fn timed<T>(
        &mut self,
        phase: &str,
        operation: impl FnOnce() -> Result<T, SimulationError>,
    ) -> Result<T, SimulationError> {
        self.record(&json!({"event": "begin", "phase": phase}))?;
        let start = Instant::now();
        let result = operation();
        let elapsed = start.elapsed().as_secs_f64();
        let result = self.operation_result(result);
        combine_execution_and_cleanup(
            result,
            self.record(&json!({
                "event": "timing", "phase": phase, "seconds": elapsed
            })),
        )
    }

    fn operation_result<T>(
        &mut self,
        result: Result<T, SimulationError>,
    ) -> Result<T, SimulationError> {
        if let Err(error) = &result {
            let report = self.record(&json!({
                "event": "operation_error", "status": error_status(error),
                "error": error.to_string()
            }));
            return combine_execution_and_cleanup(result, report);
        }
        result
    }

    fn cleanup(
        &mut self,
        owner: &str,
        result: Result<(), SimulationError>,
    ) -> Result<(), SimulationError> {
        self.cleanup_failed |= result.is_err();
        let report = self.record(&json!({
            "event": "cleanup", "owner": owner,
            "error": result.as_ref().err().map(ToString::to_string)
        }));
        combine_execution_and_cleanup(result, report)
    }
}

fn select(
    availability: &crate::Availability,
    fixture: &Fixture,
    trial: &Trial,
    journal: &mut Journal,
) -> Result<(NativeMetadata, Vec<Vec<i32>>), SimulationError> {
    let mut session = SessionResources::new(Arc::clone(&availability.libraries), 0)?;
    let result = (|| {
        let (free, total) = session.api().memory_info()?;
        journal.record(
            &json!({"event": "device_memory_before_search", "free": free, "total": total}),
        )?;
        let query = fixture.circuit.query().expect("qualified query");
        let mut resources = ContractionResources::new(&mut session, &query)?;
        let result = (|| {
            if let Some(settings) = trial.optimizer {
                journal.timed("optimize", || resources.optimize(settings))?;
                journal.record(&json!({
                    "event": "estimates",
                    "flops": resources.estimate(OptimizerEstimate::FlopCount)?,
                    "largest_intermediate_elements": resources.estimate(OptimizerEstimate::LargestTensor)?,
                    "source": "cuTensorNet; no normalized hardware FLOP convention"
                }))?;
            } else {
                let metadata = journal.timed("construct_control", || {
                    chronological(query.network().nodes().len())
                })?;
                resources.import(&metadata)?;
                journal.record(&json!({
                    "event": "estimates", "flops": null, "largest_intermediate_elements": null,
                    "source": "supplied chronological path; no optimizer estimate"
                }))?;
            }
            let metadata = journal.timed("export", || resources.export())?;
            let modes = resources.intermediate_modes()?;
            journal.record(&json!({
                "event": "plan", "path": metadata.path,
                "slicing": metadata.slicing.iter().map(|slice| json!({
                    "mode": slice.mode, "extent": slice.extent
                })).collect::<Vec<_>>(),
                "num_slices": metadata.num_slices, "intermediate_modes": modes,
            }))?;
            Ok((metadata, modes))
        })();
        let result = journal.operation_result(result);
        let cleanup = journal.cleanup("source_topology", resources.close());
        combine_execution_and_cleanup(result, cleanup)
    })();
    let cleanup = journal.cleanup("source_session", session.close());
    combine_execution_and_cleanup(result, cleanup)
}

fn memory_report(memory: &ExecutionMemory) -> Value {
    json!({
        "event": "memory",
        "coefficient_bytes": memory.coefficient_bytes,
        "unique_buffers": memory.unique_buffers,
        "output_bytes": memory.output_bytes,
        "device_scratch_minimum": memory.device_scratch_minimum,
        "device_scratch_recommended": memory.device_scratch_recommended,
        "device_scratch_allocated": memory.device_scratch_allocated,
        "host_scratch_minimum": memory.host_scratch_minimum,
        "host_scratch_recommended": memory.host_scratch_recommended,
        "host_scratch_allocated": memory.host_scratch_allocated,
        "device_cache_recommended": memory.device_cache_recommended,
        "host_cache_recommended": memory.host_cache_recommended,
        "owned_device_bytes": memory.owned_device_bytes,
    })
}

fn execute(
    availability: &crate::Availability,
    fixture: &Fixture,
    trial: &Trial,
    metadata: &NativeMetadata,
    modes: &[Vec<i32>],
    journal: &mut Journal,
) -> Result<(), SimulationError> {
    let mut session = SessionResources::new(Arc::clone(&availability.libraries), 0)?;
    let result = (|| {
        let query = fixture.circuit.query().expect("qualified query");
        let mut resources = ContractionResources::new(&mut session, &query)?;
        if let Err(error) = journal.timed("import", || resources.import(metadata)) {
            return combine_execution_and_cleanup(
                Err(error),
                journal.cleanup("fresh_topology", resources.close()),
            );
        }
        let start = Instant::now();
        let prepared = ContractionExecution::prepare(
            resources,
            fixture.circuit.buffers(),
            fixture.circuit.node_buffer_ids(),
            WorkspaceLimits {
                device_scratch: Some(usize::try_from(WORKSPACE_BYTES).expect("64-bit host")),
                host_scratch: None,
            },
        );
        let elapsed = start.elapsed().as_secs_f64();
        let mut execution = match prepared {
            Ok(execution) => execution,
            Err(error) => {
                let report = journal.record(&json!({
                    "event": "timing", "phase": "prepare_host_call", "seconds": elapsed
                }));
                return journal.operation_result(combine_execution_and_cleanup(Err(error), report));
            }
        };
        let result = (|| {
            // Keep the prepared owner available for explicit close if journal I/O fails.
            journal.record(&json!({
                "event": "timing", "phase": "prepare_host_call", "seconds": elapsed
            }))?;
            journal.record(&memory_report(execution.memory()))?;
            if execution.metadata()? != *metadata {
                return Err(unexpected("metadata changed during preparation"));
            }
            assert_mode_sets(modes, &execution.intermediate_modes()?)?;
            contract(&mut execution, fixture, trial.repeats, journal)?;
            if execution.metadata()? != *metadata {
                return Err(unexpected("metadata changed during execution"));
            }
            assert_mode_sets(modes, &execution.intermediate_modes()?)?;
            Ok(())
        })();
        let result = journal.operation_result(result);
        let cleanup = journal.cleanup("execution", execution.close());
        combine_execution_and_cleanup(result, cleanup)
    })();
    let cleanup = journal.cleanup("fresh_session", session.close());
    combine_execution_and_cleanup(result, cleanup)
}

fn contract<Api: super::super::super::ContractionExecutionApi>(
    execution: &mut ContractionExecution<'_, Api>,
    fixture: &Fixture,
    repeats: usize,
    journal: &mut Journal,
) -> Result<(), SimulationError> {
    let mut first = Vec::new();
    for iteration in 0..=repeats {
        let output = journal.timed(&format!("contract_readback_{iteration}"), || {
            execution.contract()
        })?;
        let comparison = compare(&output, &fixture.expected, fixture.limit);
        if iteration == 0 || comparison.is_err() {
            save_output("case_a_4x4", iteration, &output)?;
        }
        let comparison = comparison.map_err(unexpected)?;
        let identical = iteration == 0
            || output
                .iter()
                .zip(&first)
                .all(|(a, b): (_, &num_complex::Complex64)| {
                    a.re.to_bits() == b.re.to_bits() && a.im.to_bits() == b.im.to_bits()
                });
        journal.record(&json!({
            "event": "comparison", "iteration": iteration,
            "amplitude_error": comparison.amplitude_error,
            "probability_tv": comparison.probability_tv,
            "squared_norm_error": comparison.squared_norm_error,
            "bitwise_equal_to_first": identical
        }))?;
        if iteration == 0 {
            first = output;
        }
    }
    Ok(())
}

fn run(trial: &Trial, journal: &mut Journal) -> Result<(), SimulationError> {
    let fixture = fixture("case_a_4x4");
    let availability = crate::discover().map_err(|error| unexpected(error.to_string()))?;
    let report = availability.report();
    journal.record(&json!({
        "event": "environment", "pid": std::process::id(),
        "versions": {
            "cutensornet": report.cutensornet_version,
            "cutensornet_cuda_runtime": report.cutensornet_cuda_runtime_version,
            "cuda_runtime": report.cuda_runtime_version,
            "cuda_driver": report.cuda_driver_version
        },
        "libraries": {
            "cutensornet": report.cutensornet_library,
            "cuda_runtime": report.cuda_runtime_library
        },
        "device_scratch_ceiling": WORKSPACE_BYTES, "host_scratch_ceiling": null,
        "optimizer_workspace_constraint": trial.optimizer.map(|s| s.workspace_constraint),
        "device": 0, "precision": "CUDA_C_64F/COMPUTE_64F", "limit": fixture.limit,
        "threads": 1, "slicing": false, "cache": false, "autotuning": false
    }))?;
    let (metadata, modes) = select(&availability, &fixture, trial, journal)?;
    execute(&availability, &fixture, trial, &metadata, &modes, journal)
}

#[test]
#[ignore = "requires audited CUDA host; run through contraction-experiments.py"]
fn parameterized_trial() {
    let directory = PathBuf::from(
        std::env::var_os("QDK_CONTRACTION_EVIDENCE_DIR").expect("evidence directory"),
    );
    let value: Value =
        serde_json::from_slice(&fs::read(directory.join("config.json")).expect("trial config"))
            .expect("JSON");
    let trial = Trial::parse(&value).expect("valid trial configuration");
    let file = File::create_new(directory.join("events.jsonl")).expect("fresh journal");
    let mut journal = Journal {
        file,
        cleanup_failed: false,
    };
    let result = run(&trial, &mut journal);
    let status = if journal.cleanup_failed {
        "failed"
    } else {
        result.as_ref().map_or_else(error_status, |()| "passed")
    };
    let requirement = match &result {
        Err(SimulationError::WorkspaceLimitExceeded { required, maximum }) => {
            Some(json!({"required": required, "maximum": maximum}))
        }
        _ => None,
    };
    journal
        .record(&json!({
            "event": "result", "status": status, "workspace_rejection": requirement,
            "error": result.as_ref().err().map(ToString::to_string)
        }))
        .expect("persist outcome");
    result.expect("experiment result");
}
