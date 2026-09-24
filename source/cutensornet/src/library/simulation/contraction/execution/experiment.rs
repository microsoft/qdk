//! Parameterized experiments, separate from the fixed qualification selectors.

use crate::simulation::{
    SimulationError,
    contraction::{NativeMetadata, NativeOptimizerSettings, invalid},
};
use serde_json::Value;

const WORKSPACE_BYTES: u64 = 32 * 1024 * 1024 * 1024;

fn error_status(error: &SimulationError) -> &'static str {
    match error {
        SimulationError::WorkspaceLimitExceeded { .. } => "workspace_limit",
        SimulationError::HostScratchAllocationFailed { .. }
        | SimulationError::NativeCallFailed {
            component: "CUDA Runtime",
            operation: "cudaMalloc",
            status: 2,
            ..
        }
        | SimulationError::NativeCallFailed {
            component: "cuTensorNet",
            status: 3,
            ..
        } => "allocation_failed",
        _ => "failed",
    }
}

struct Trial {
    optimizer: Option<NativeOptimizerSettings>,
    repeats: usize,
}

impl Trial {
    fn parse(value: &Value) -> Result<Self, SimulationError> {
        let object = value
            .as_object()
            .ok_or_else(|| invalid("expected trial object"))?;
        let integer = |key| {
            value[key]
                .as_u64()
                .and_then(|n| i32::try_from(n).ok())
                .ok_or_else(|| invalid("trial settings must be nonnegative i32 integers"))
        };
        let repeats =
            usize::try_from(integer("repeats")?).map_err(|_| invalid("invalid repeat count"))?;
        if repeats == 0 {
            return Err(invalid("at least one repeated execution is required"));
        }
        let (optimizer, keys): (_, &[&str]) = match value["plan_source"].as_str() {
            Some("chronological") => (None, &["plan_source", "repeats"]),
            Some("optimizer") => {
                let settings = NativeOptimizerSettings {
                    workspace_constraint: WORKSPACE_BYTES,
                    hyper_samples: integer("hyper_samples")?,
                    reconfiguration_iterations: integer("reconfiguration_iterations")?,
                    disable_rank_simplification: value["disable_rank_simplification"]
                        .as_bool()
                        .ok_or_else(|| invalid("expected boolean rank simplification control"))?,
                    seed: integer("seed")?,
                    threads: 1,
                    disable_slicing: true,
                };
                settings.attributes()?;
                (
                    Some(settings),
                    &[
                        "plan_source",
                        "repeats",
                        "hyper_samples",
                        "reconfiguration_iterations",
                        "disable_rank_simplification",
                        "seed",
                    ],
                )
            }
            _ => return Err(invalid("unknown experiment plan source")),
        };
        if object.len() != keys.len() || !keys.iter().all(|key| object.contains_key(*key)) {
            return Err(invalid("unexpected or missing trial field"));
        }
        Ok(Self { optimizer, repeats })
    }
}

fn chronological(inputs: usize) -> Result<NativeMetadata, SimulationError> {
    let inputs = i32::try_from(inputs).map_err(|_| invalid("too many circuit tensors"))?;
    if inputs < 2 {
        return Err(invalid("chronological path requires two or more tensors"));
    }
    let mut path = vec![[0, 1]];
    // I2 orders zero boundaries first, then gates in circuit order.
    for remaining in (2..inputs).rev() {
        path.push([0, remaining - 1]);
    }
    Ok(NativeMetadata {
        path,
        slicing: vec![],
        num_slices: 1,
    })
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[path = "experiment/native.rs"]
mod native;

#[test]
fn experiment_configuration_is_explicit_and_preserves_search_settings() {
    let mut value = serde_json::json!({
        "plan_source": "optimizer", "repeats": 5, "hyper_samples": 64,
        "reconfiguration_iterations": 500, "disable_rank_simplification": false,
        "seed": 29
    });
    let trial = Trial::parse(&value).expect("trial");
    assert_eq!(trial.repeats, 5);
    let optimizer = trial.optimizer.expect("search");
    assert_eq!(optimizer.hyper_samples, 64);
    assert_eq!(optimizer.reconfiguration_iterations, 500);
    assert!(!optimizer.disable_rank_simplification);
    assert!(optimizer.disable_slicing);
    assert_eq!(optimizer.workspace_constraint, WORKSPACE_BYTES);
    assert_eq!(optimizer.threads, 1);
    value["repeats"] = serde_json::json!(0);
    assert!(Trial::parse(&value).is_err());
    value["repeats"] = serde_json::json!(5);
    value["extra"] = serde_json::json!(true);
    assert!(Trial::parse(&value).is_err());
    for value in [
        serde_json::json!({"plan_source": "unknown", "repeats": 5}),
        serde_json::json!({"plan_source": "chronological", "repeats": -1}),
        serde_json::json!({"plan_source": "chronological", "repeats": 1.5}),
    ] {
        assert!(Trial::parse(&value).is_err());
    }
    let control = Trial::parse(&serde_json::json!({
        "plan_source": "chronological", "repeats": 5
    }))
    .expect("control");
    assert!(control.optimizer.is_none());
}

#[test]
fn chronological_control_consumes_each_i2_input_in_order() {
    use std::collections::BTreeSet;
    for name in ["diagnostic", "case_a_2x2", "case_a_4x4"] {
        let fixture = super::fixture(name);
        let query = fixture.circuit.query().expect("query");
        let metadata = chronological(query.network().nodes().len()).expect("path");
        let mut operands: Vec<BTreeSet<_>> = query
            .network()
            .nodes()
            .iter()
            .map(|axes| axes.as_slice().iter().map(|axis| axis.id()).collect())
            .collect();
        let output: BTreeSet<_> = query
            .keep()
            .as_slice()
            .iter()
            .map(|axis| axis.id())
            .collect();
        for [a, b] in metadata.path {
            let a = usize::try_from(a).expect("position");
            let b = usize::try_from(b).expect("position");
            let union: BTreeSet<_> = operands[a].union(&operands[b]).copied().collect();
            let mut retained = output.clone();
            for (index, operand) in operands.iter().enumerate() {
                if index != a && index != b {
                    retained.extend(operand);
                }
            }
            let result: BTreeSet<_> = union.intersection(&retained).copied().collect();
            assert!(result.len() <= output.len(), "chronological frontier grew");
            operands.remove(a.max(b));
            operands.remove(a.min(b));
            operands.push(result);
        }
        assert_eq!(operands, vec![output]);
    }
}

#[test]
fn cleanup_errors_never_become_continuable_resource_rejections() {
    let resource = SimulationError::WorkspaceLimitExceeded {
        required: 1025,
        maximum: 1024,
    };
    assert_eq!(error_status(&resource), "workspace_limit");
    let combined = SimulationError::ExecutionAndCleanupFailed {
        execution: Box::new(resource),
        cleanup: Box::new(invalid("cleanup failed")),
    };
    assert_eq!(error_status(&combined), "failed");
    assert_eq!(error_status(&invalid("metadata changed")), "failed");
    let host_allocation = SimulationError::HostScratchAllocationFailed { bytes: 1_048_832 };
    assert_eq!(error_status(&host_allocation), "allocation_failed");
    assert_eq!(
        error_status(&SimulationError::ExecutionAndCleanupFailed {
            execution: Box::new(host_allocation),
            cleanup: Box::new(invalid("cleanup failed")),
        }),
        "failed"
    );
    for (component, operation, status, expected) in [
        ("CUDA Runtime", "cudaMalloc", 2, "allocation_failed"),
        ("CUDA Runtime", "cudaMemcpy", 2, "failed"),
        ("cuTensorNet", "optimize", 3, "allocation_failed"),
        ("cuTensorNet", "optimize", 7, "failed"),
    ] {
        assert_eq!(
            error_status(&SimulationError::NativeCallFailed {
                component,
                operation,
                status,
                message: "injected".into(),
            }),
            expected
        );
    }
}
