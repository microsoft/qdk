//! Native metadata conformance, not numerical execution. Every case uses the
//! production topology/metadata owner; no test-local FFI or optimizer fallback.

use super::*;
use crate::library::CuTensorNetApi;
use std::sync::Arc;
use tensornet::{Index, TensorNetwork};

const P1: [[i32; 2]; 3] = [[1, 2], [0, 2], [0, 1]];
const P2: [[i32; 2]; 3] = [[2, 3], [1, 2], [0, 1]];

fn fixture() -> TensorNetwork {
    let axes: Vec<_> = [11, 23, 37, 53, 71]
        .into_iter()
        .zip([2, 3, 5, 7, 11])
        .map(|(id, dim)| Index::new(id, dim).expect("positive extent"))
        .collect();
    TensorNetwork::new(
        axes.windows(2)
            .map(|pair| Indices::new(pair.to_vec()).expect("consistent axes"))
            .collect(),
    )
    .expect("consistent network")
}

fn settings() -> NativeOptimizerSettings {
    NativeOptimizerSettings {
        workspace_constraint: 67_108_864,
        hyper_samples: 1,
        threads: 1,
        seed: 17,
        reconfiguration_iterations: 0,
        disable_rank_simplification: true,
        disable_slicing: true,
    }
}

fn with_network<T>(
    label: &str,
    operation: impl FnOnce(&mut ContractionResources<'_, CuTensorNetApi>) -> Result<T, SimulationError>,
) -> Result<T, SimulationError> {
    let availability = crate::discover().expect("audited native libraries must be available");
    println!("{label}: native_versions={:?}", availability.report());
    println!(
        "{label}: fixture=A[11:2,23:3] B[23:3,37:5] C[37:5,53:7] D[53:7,71:11] -> [11,71]; CUDA_C_64F/COMPUTE_64F"
    );
    let mut session = SessionResources::new(Arc::clone(&availability.libraries), 0)?;
    let network = fixture();
    let query = ContractionQuery::new(
        &network,
        Indices::new(vec![
            Index::new(11, 2).expect("valid fixture and successful test operation"),
            Index::new(71, 11).expect("valid fixture and successful test operation"),
        ])
        .expect("valid fixture and successful test operation"),
    )
    .expect("valid fixture and successful test operation");
    let result = (|| {
        let mut resources = ContractionResources::new(&mut session, &query)?;
        println!(
            "{label}: native_tensor_ids={:?} (not path positions)",
            resources.tensor_ids()
        );
        let result = operation(&mut resources);
        let cleanup = resources.close();
        println!("{label}: topology_cleanup={cleanup:?}");
        combine_execution_and_cleanup(result, cleanup)
    })();
    let cleanup = session.close();
    println!("{label}: session_cleanup={cleanup:?}");
    combine_execution_and_cleanup(result, cleanup)
}

struct Inspection {
    metadata: NativeMetadata,
    modes: Result<Vec<Vec<i32>>, SimulationError>,
}

fn inspect(
    label: &str,
    resources: &mut ContractionResources<'_, CuTensorNetApi>,
) -> Result<Inspection, SimulationError> {
    let metadata = resources.export()?;
    println!("{label}: owned_metadata={metadata:?}");
    let modes = resources.intermediate_modes();
    println!("{label}: native_intermediate_modes={modes:?}");
    for estimate in [
        OptimizerEstimate::FlopCount,
        OptimizerEstimate::LargestTensor,
    ] {
        let value = resources.estimate(estimate);
        println!(
            "{label}: native_{estimate:?}={value:?}; vendor estimate, no inferred counting convention"
        );
    }
    Ok(Inspection { metadata, modes })
}

fn assert_mode_sets(observed: Result<Vec<Vec<i32>>, SimulationError>, expected: &[Vec<i32>]) {
    let observed = observed.expect(
        "ACCEPTANCE GAP: native structural metadata unavailable; path echo is not conformance",
    );
    assert_eq!(observed.len(), expected.len());
    for (step, (actual, expected)) in observed.iter().zip(expected).enumerate() {
        assert_eq!(
            actual.iter().copied().collect::<BTreeSet<_>>(),
            expected.iter().copied().collect::<BTreeSet<_>>(),
            "step {step}: native intermediate modes disagree with recycled-position semantics",
        );
    }
}

fn imported_path_case(label: &str, path: [[i32; 2]; 3], expected: &[Vec<i32>]) {
    let selected = NativeMetadata {
        path: path.to_vec(),
        slicing: Vec::new(),
        num_slices: 1,
    };
    let Inspection {
        metadata: returned,
        modes,
    } = with_network(label, |resources| {
        println!("{label}: import only; no optimizer invocation");
        resources.import(&selected)?;
        inspect(label, resources)
    })
    .expect("native import/export and explicit cleanup must succeed");
    // All native owners are already closed. The assertions use only owned data.
    assert_eq!(returned, selected);
    assert_mode_sets(modes, expected);
}

#[test]
#[ignore = "requires audited cuTensorNet 2.13/CUDA 12.9 and an NVIDIA GPU"]
fn supplied_path_p1_uses_appended_intermediates() {
    imported_path_case("P1", P1, &[vec![23, 53], vec![11, 53], vec![11, 71]]);
}

#[test]
#[ignore = "requires audited cuTensorNet 2.13/CUDA 12.9 and an NVIDIA GPU"]
fn supplied_path_p2_uses_distinct_intermediates() {
    imported_path_case("P2", P2, &[vec![37, 71], vec![23, 71], vec![11, 71]]);
}

#[test]
#[ignore = "requires audited cuTensorNet 2.13/CUDA 12.9 and an NVIDIA GPU"]
fn optimized_metadata_survives_close_and_import_into_fresh_network() {
    let Inspection { metadata: exported, modes: source_modes } = with_network("optimized-source", |resources| {
        println!("optimized-source: settings={:?}; workspace constraint is not allocated workspace or a total-memory cap", settings());
        println!("optimized-source: all other optimizer attributes retain pinned-SDK defaults (including FLOPS objective and CUTENSOR memory model)");
        resources.optimize(settings())?;
        inspect("optimized-source", resources)
    }).expect("native optimization/export and cleanup must succeed");
    assert_eq!(exported.num_slices, 1);
    assert!(exported.slicing.is_empty());
    let expected = expected_chain_intermediates(&exported.path);
    assert_mode_sets(source_modes, &expected);
    let Inspection { metadata: imported, modes: imported_modes } = with_network("fresh-import", |resources| {
        println!("fresh-import: import only; source native owners already closed; no optimizer invocation");
        resources.import(&exported)?;
        inspect("fresh-import", resources)
    }).expect("fresh import/export and cleanup must succeed");
    assert_eq!(imported, exported);
    assert_mode_sets(imported_modes, &expected);
}

#[test]
#[ignore = "requires audited cuTensorNet 2.13/CUDA 12.9 and an NVIDIA GPU"]
fn explicit_internal_unit_extent_slicing_preserves_slice_count() {
    let selected = NativeMetadata {
        path: P1.to_vec(),
        slicing: vec![SlicedMode {
            mode: 23,
            extent: 1,
        }],
        num_slices: 3,
    };
    let exported = with_network("internal-slicing", |resources| {
        println!("internal-slicing: import only; mode 23 has dimension 3, slice extent 1, expected full coverage 3");
        resources.import(&selected)?;
        let metadata = resources.export()?;
        println!("internal-slicing: owned_metadata={metadata:?}");
        Ok(metadata)
    }).expect("explicit slicing import/export and cleanup must succeed");
    assert_eq!(exported, selected);
}

// Independent set-algebra oracle for whichever path the optimizer selects.
// Fixed seed is not a promise of a particular selected path.
fn expected_chain_intermediates(path: &[[i32; 2]]) -> Vec<Vec<i32>> {
    let mut operands: Vec<BTreeSet<i32>> = [[11, 23], [23, 37], [37, 53], [53, 71]]
        .into_iter()
        .map(BTreeSet::from)
        .collect();
    let output = BTreeSet::from([11, 71]);
    let mut expected = Vec::new();
    for &[first, second] in path {
        let first = usize::try_from(first).expect("valid fixture and successful test operation");
        let second = usize::try_from(second).expect("valid fixture and successful test operation");
        let right = operands.remove(first.max(second));
        let left = operands.remove(first.min(second));
        let needed: BTreeSet<_> = operands
            .iter()
            .flatten()
            .copied()
            .chain(output.iter().copied())
            .collect();
        let result: BTreeSet<_> = left
            .union(&right)
            .copied()
            .filter(|mode| needed.contains(mode))
            .collect();
        expected.push(result.iter().copied().collect());
        operands.push(result);
    }
    expected
}
