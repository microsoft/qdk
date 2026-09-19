use super::*;
use crate::simulation::{
    contraction::execution::{ContractionExecution, ContractionExecutionApi, WorkspaceLimits},
    ffi::Complex64Abi,
    memory_workspace::{MemorySpace, MemoryWorkspaceApi, WorkspaceKind, WorkspacePreference},
};
use num_complex::Complex64;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct NumericalState {
    allocations: BTreeMap<usize, usize>,
    uploads: BTreeMap<usize, Vec<Complex64Abi>>,
    inputs: Vec<(i64, usize)>,
    output: Option<usize>,
    workspace: Vec<(MemorySpace, WorkspaceKind, Option<usize>, i64)>,
    prepared: bool,
    pub(super) pending: bool,
}

pub(super) struct NumericalSettings {
    device_minimum: i64,
    host_minimum: i64,
    nonfinite_output: bool,
}

impl Default for NumericalSettings {
    fn default() -> Self {
        Self {
            device_minimum: 512,
            host_minimum: 128,
            nonfinite_output: false,
        }
    }
}

fn api(
    failures: Vec<(&'static str, usize)>,
    numerical: NumericalSettings,
) -> Arc<TestDoubleContractionApi> {
    Arc::new(TestDoubleContractionApi {
        state: Mutex::new(State::default()),
        failures,
        corruption: Corruption::None,
        numerical,
    })
}

impl MemoryWorkspaceApi for TestDoubleContractionApi {
    fn memory_info(&self) -> Result<(usize, usize), SimulationError> {
        self.event("memory_info")?;
        Ok((1024 * 1024, 2 * 1024 * 1024))
    }
    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, SimulationError> {
        assert!(bytes > 0);
        let id = 1000
            + self
                .state
                .lock()
                .expect("state")
                .numerical
                .allocations
                .len();
        let allocation = self.create("allocate", id)?;
        self.state
            .lock()
            .expect("state")
            .numerical
            .allocations
            .insert(id, bytes);
        Ok(allocation)
    }
    fn free(&self, allocation: OpaqueHandle) -> Result<(), SimulationError> {
        self.destroy("free", allocation)
    }
    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), SimulationError> {
        self.event("copy_to_device")?;
        let id = destination.as_ptr() as usize;
        let mut state = self.state.lock().expect("state");
        assert_eq!(state.numerical.allocations[&id], size_of_val(source));
        assert!(
            state
                .numerical
                .uploads
                .insert(id, source.to_vec())
                .is_none(),
            "upload unique buffer once"
        );
        Ok(())
    }
    fn copy_from_device(
        &self,
        source: OpaqueHandle,
        destination: &mut [Complex64Abi],
    ) -> Result<(), SimulationError> {
        self.event("copy_from_device")?;
        let state = self.state.lock().expect("state");
        assert!(!state.numerical.pending);
        assert_eq!(state.numerical.output, Some(source.as_ptr() as usize));
        assert_eq!(
            state.numerical.allocations[&(source.as_ptr() as usize)],
            size_of_val(destination)
        );
        destination.fill(Complex64Abi::new(
            if self.numerical.nonfinite_output {
                f64::NAN
            } else {
                0.25
            },
            -0.125,
        ));
        Ok(())
    }
    fn create_workspace(&self, _handle: OpaqueHandle) -> Result<OpaqueHandle, SimulationError> {
        self.create("create_workspace", 8)
    }
    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), SimulationError> {
        self.destroy("destroy_workspace", workspace)
    }
    fn workspace_memory_size(
        &self,
        _handle: OpaqueHandle,
        _workspace: OpaqueHandle,
        preference: WorkspacePreference,
        space: MemorySpace,
        kind: WorkspaceKind,
    ) -> Result<i64, SimulationError> {
        self.event("workspace_memory_size")?;
        let value = match (space, kind) {
            (_, WorkspaceKind::Cache) => 4096,
            (MemorySpace::Device, _) => self.numerical.device_minimum,
            (MemorySpace::Host, _) => self.numerical.host_minimum,
        };
        Ok(
            if preference == WorkspacePreference::Recommended
                && kind == WorkspaceKind::Scratch
                && value > 0
            {
                value * 2
            } else {
                value
            },
        )
    }
    fn set_workspace_memory(
        &self,
        _handle: OpaqueHandle,
        _workspace: OpaqueHandle,
        space: MemorySpace,
        kind: WorkspaceKind,
        allocation: Option<OpaqueHandle>,
        bytes: i64,
    ) -> Result<(), SimulationError> {
        self.event("set_workspace_memory")?;
        let address = allocation.map(|p| p.as_ptr() as usize);
        match kind {
            WorkspaceKind::Scratch => {
                assert!(allocation.is_some() && bytes > 0);
                if space == MemorySpace::Host {
                    assert_eq!(address.expect("host allocation") % 256, 0);
                }
            }
            WorkspaceKind::Cache => assert!(allocation.is_none() && bytes == 0),
        }
        self.state
            .lock()
            .expect("state")
            .numerical
            .workspace
            .push((space, kind, address, bytes));
        Ok(())
    }
}

impl ContractionExecutionApi for TestDoubleContractionApi {
    fn compute_contraction_workspace(
        &self,
        _handle: OpaqueHandle,
        _network: OpaqueHandle,
        _info: OpaqueHandle,
        _workspace: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.event("compute_contraction_workspace")
    }
    fn bind_input(
        &self,
        _handle: OpaqueHandle,
        _network: OpaqueHandle,
        tensor_id: i64,
        allocation: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.event("bind_input")?;
        let mut state = self.state.lock().expect("state");
        assert!([90, 7, 400, 12].contains(&tensor_id));
        assert!(
            state
                .numerical
                .uploads
                .contains_key(&(allocation.as_ptr() as usize))
        );
        state
            .numerical
            .inputs
            .push((tensor_id, allocation.as_ptr() as usize));
        Ok(())
    }
    fn bind_output(
        &self,
        _handle: OpaqueHandle,
        _network: OpaqueHandle,
        allocation: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.event("bind_output")?;
        self.state.lock().expect("state").numerical.output = Some(allocation.as_ptr() as usize);
        Ok(())
    }
    fn prepare_contraction(
        &self,
        _handle: OpaqueHandle,
        _network: OpaqueHandle,
        _workspace: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.event("prepare_contraction")?;
        let mut state = self.state.lock().expect("state");
        assert_eq!(state.numerical.inputs.len(), 4);
        assert!(state.numerical.output.is_some());
        assert!(!state.path.is_empty());
        state.numerical.prepared = true;
        Ok(())
    }
    fn contract(
        &self,
        _handle: OpaqueHandle,
        _network: OpaqueHandle,
        _workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(), SimulationError> {
        let mut state = self.state.lock().expect("state");
        assert!(state.numerical.prepared);
        assert!(!state.numerical.pending);
        state.numerical.pending = true;
        drop(state);
        self.event("contract")
    }
}

fn limits() -> WorkspaceLimits {
    WorkspaceLimits {
        device_scratch: 1024,
        host_scratch: 256,
    }
}

fn shared_chain() -> TensorNetwork {
    let axes: Vec<_> = [11, 23, 37, 53, 71]
        .into_iter()
        .map(|id| Index::new(id, 2).expect("axis"))
        .collect();
    TensorNetwork::new(
        axes.windows(2)
            .map(|w| Indices::new(w.to_vec()).expect("axes"))
            .collect(),
    )
    .expect("network")
}

fn shared_query(network: &TensorNetwork) -> ContractionQuery<'_> {
    ContractionQuery::new(
        network,
        Indices::new(vec![
            Index::new(11, 2).expect("axis"),
            Index::new(71, 2).expect("axis"),
        ])
        .expect("axes"),
    )
    .expect("query")
}

fn coefficients() -> Vec<Box<[Complex64]>> {
    vec![vec![Complex64::new(0.5, -0.25); 4].into_boxed_slice()]
}

fn run_numerical<T>(
    api: Arc<TestDoubleContractionApi>,
    buffers: &[Box<[Complex64]>],
    bindings: &[usize],
    limits: WorkspaceLimits,
    operation: impl FnOnce(
        &mut ContractionExecution<'_, TestDoubleContractionApi>,
    ) -> Result<T, SimulationError>,
) -> Result<T, SimulationError> {
    let mut session = SessionResources::new(api, 0)?;
    let network = shared_chain();
    let result = (|| {
        let mut resources = ContractionResources::new(&mut session, &shared_query(&network))?;
        if let Err(error) = resources.import(&metadata()) {
            return combine_execution_and_cleanup(Err(error), resources.close());
        }
        let mut execution = ContractionExecution::prepare(resources, buffers, bindings, limits)?;
        let result = operation(&mut execution);
        combine_execution_and_cleanup(result, execution.close())
    })();
    combine_execution_and_cleanup(result, session.close())
}

#[test]
fn shared_buffers_native_ids_repeated_readback_and_owned_results() {
    let api = api(vec![], NumericalSettings::default());
    let output = run_numerical(
        api.clone(),
        &coefficients(),
        &[0; 4],
        limits(),
        |execution| {
            assert_eq!(execution.metadata()?, metadata());
            assert_eq!(execution.memory().unique_buffers, 1);
            assert_eq!(execution.memory().coefficient_bytes, 64);
            assert_eq!(execution.memory().output_bytes, 64);
            assert_eq!(execution.memory().owned_device_bytes, 640);
            let first = execution.contract()?;
            assert_eq!(first, execution.contract()?);
            Ok(first)
        },
    )
    .expect("execution");
    api.assert_released();
    assert_eq!(output, vec![Complex64::new(0.25, -0.125); 4]);
    let state = api.state.lock().expect("state");
    assert_eq!(state.numerical.uploads.len(), 1);
    assert_eq!(
        state
            .numerical
            .inputs
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [90, 7, 400, 12]
    );
    assert!(state.numerical.inputs.windows(2).all(|w| w[0].1 == w[1].1));
    assert!(!state.events.contains(&"optimize"));
    assert!(!state.events.contains(&"create_optimizer_config"));
    assert_eq!(
        state
            .events
            .iter()
            .filter(|&&event| event == "prepare_contraction")
            .count(),
        1
    );
}

#[test]
fn validates_bindings_and_values_before_allocating_or_binding() {
    for (buffers, bindings) in [
        (coefficients(), vec![0; 3]),
        (coefficients(), vec![1; 4]),
        (
            vec![vec![Complex64::new(1.0, 0.0); 3].into_boxed_slice()],
            vec![0; 4],
        ),
        (
            vec![vec![Complex64::new(f64::NAN, 0.0); 4].into_boxed_slice()],
            vec![0; 4],
        ),
    ] {
        let api = api(vec![], NumericalSettings::default());
        assert!(run_numerical(api.clone(), &buffers, &bindings, limits(), |_| Ok(())).is_err());
        api.assert_released();
        assert!(!api.events().contains(&"allocate"));
        assert!(!api.events().contains(&"create_workspace"));
    }
}

#[test]
fn rejects_missing_metadata_and_unqualified_numerical_slicing_before_allocation() {
    for selected in [
        None,
        Some(NativeMetadata {
            slicing: vec![SlicedMode {
                mode: 23,
                extent: 1,
            }],
            num_slices: 3,
            ..metadata()
        }),
    ] {
        let api = api(vec![], NumericalSettings::default());
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let network = chain();
        let mut resources =
            ContractionResources::new(&mut session, &query(&network)).expect("network");
        if let Some(selected) = selected {
            resources
                .import(&selected)
                .expect("supported metadata subset");
        }
        assert!(matches!(
            ContractionExecution::prepare(resources, &coefficients(), &[0; 4], limits()),
            Err(SimulationError::InvalidContractionConfiguration { .. })
        ));
        session.close().expect("session cleanup");
        api.assert_released();
        assert!(!api.events().contains(&"create_workspace"));
        assert!(!api.events().contains(&"allocate"));
    }
}

#[test]
fn uploads_referenced_buffers_only_and_keeps_distinct_bindings() {
    let api = api(vec![], NumericalSettings::default());
    let buffers = vec![
        vec![Complex64::new(0.5, -0.25); 4].into_boxed_slice(),
        vec![Complex64::new(-0.5, 0.25); 4].into_boxed_slice(),
        vec![Complex64::new(f64::NAN, 0.0)].into_boxed_slice(),
    ];
    run_numerical(
        api.clone(),
        &buffers,
        &[0, 1, 0, 1],
        limits(),
        |execution| {
            assert_eq!(execution.memory().unique_buffers, 2);
            assert_eq!(execution.memory().coefficient_bytes, 128);
            execution.contract()?;
            Ok(())
        },
    )
    .expect("bound buffers only");
    api.assert_released();
    let state = api.state.lock().expect("state");
    assert_eq!(state.numerical.uploads.len(), 2);
    let inputs = &state.numerical.inputs;
    assert_eq!(inputs[0].1, inputs[2].1);
    assert_eq!(inputs[1].1, inputs[3].1);
    assert_ne!(inputs[0].1, inputs[1].1);
}

#[test]
fn honors_exact_workspace_limits_and_distinguishes_zero_scratch_from_disabled_cache() {
    for (device, host) in [(512, 128), (0, 0)] {
        let api = api(
            vec![],
            NumericalSettings {
                device_minimum: device,
                host_minimum: host,
                nonfinite_output: false,
            },
        );
        run_numerical(
            api.clone(),
            &coefficients(),
            &[0; 4],
            WorkspaceLimits {
                device_scratch: usize::try_from(device.max(256)).expect("size"),
                host_scratch: usize::try_from(host).expect("size"),
            },
            |execution| {
                assert_eq!(
                    execution.memory().device_scratch_minimum,
                    usize::try_from(device).expect("size")
                );
                execution.contract()
            },
        )
        .expect("exact bound");
        api.assert_released();
        let state = api.state.lock().expect("state");
        assert_eq!(
            state
                .numerical
                .workspace
                .iter()
                .filter(|(_, kind, _, _)| *kind == WorkspaceKind::Cache)
                .count(),
            2
        );
        assert_eq!(
            state
                .numerical
                .workspace
                .iter()
                .filter(|(space, kind, _, _)| *space == MemorySpace::Host
                    && *kind == WorkspaceKind::Scratch)
                .count(),
            usize::from(host > 0)
        );
    }
    for (device, host) in [(511, 128), (512, 127)] {
        let api = api(vec![], NumericalSettings::default());
        assert!(matches!(
            run_numerical(
                api.clone(),
                &coefficients(),
                &[0; 4],
                WorkspaceLimits {
                    device_scratch: device,
                    host_scratch: host
                },
                |_| Ok(())
            ),
            Err(SimulationError::WorkspaceLimitExceeded { .. })
        ));
        api.assert_released();
        assert!(!api.events().contains(&"allocate"));
    }
}

#[test]
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "method items cannot satisfy the driver's independently quantified session lifetime"
)]
fn native_negative_workspace_and_nonfinite_readback_are_errors() {
    for settings in [
        NumericalSettings {
            device_minimum: -1,
            ..NumericalSettings::default()
        },
        NumericalSettings {
            host_minimum: -1,
            ..NumericalSettings::default()
        },
        NumericalSettings {
            nonfinite_output: true,
            ..NumericalSettings::default()
        },
    ] {
        let api = api(vec![], settings);
        assert!(matches!(
            run_numerical(
                api.clone(),
                &coefficients(),
                &[0; 4],
                limits(),
                |execution| execution.contract()
            ),
            Err(SimulationError::InvalidNativeResult { .. })
        ));
        api.assert_released();
    }
}

#[test]
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "method items cannot satisfy the driver's independently quantified session lifetime"
)]
fn every_numerical_boundary_reports_failures_and_releases_resources() {
    let mut points = vec![
        ("create_workspace", 1),
        ("compute_contraction_workspace", 1),
        ("copy_to_device", 1),
        ("bind_output", 1),
        ("prepare_contraction", 1),
        ("contract", 1),
        ("synchronize_stream", 1),
        ("copy_from_device", 1),
    ];
    points.extend((1..=6).map(|n| ("workspace_memory_size", n)));
    points.extend((1..=3).map(|n| ("allocate", n)));
    points.extend((1..=4).map(|n| ("set_workspace_memory", n)));
    points.extend((1..=4).map(|n| ("bind_input", n)));
    for point in points {
        let api = api(vec![point], NumericalSettings::default());
        let error = run_numerical(
            api.clone(),
            &coefficients(),
            &[0; 4],
            limits(),
            |execution| execution.contract(),
        )
        .expect_err("injected failure");
        assert!(error.to_string().contains(point.0), "{point:?}: {error}");
        api.assert_released();
    }
}

#[test]
fn execution_failure_invalidates_reuse_and_preserves_cleanup_failures() {
    let api = api(
        vec![("contract", 1), ("destroy_workspace", 1), ("free", 1)],
        NumericalSettings::default(),
    );
    let error = run_numerical(
        api.clone(),
        &coefficients(),
        &[0; 4],
        limits(),
        |execution| {
            let result = execution.contract();
            assert!(matches!(
                execution.contract(),
                Err(SimulationError::InvalidContractionConfiguration { .. })
            ));
            assert!(matches!(
                execution.metadata(),
                Err(SimulationError::InvalidContractionConfiguration { .. })
            ));
            assert!(matches!(
                execution.intermediate_modes(),
                Err(SimulationError::InvalidContractionConfiguration { .. })
            ));
            result
        },
    )
    .expect_err("execution and cleanup");
    for operation in ["contract", "destroy_workspace", "free"] {
        assert!(error.to_string().contains(operation));
    }
    api.assert_released();
}

#[test]
fn preparation_failure_preserves_cleanup_errors() {
    let api = api(
        vec![
            ("prepare_contraction", 1),
            ("destroy_workspace", 1),
            ("free", 1),
        ],
        NumericalSettings::default(),
    );
    let error = run_numerical(api.clone(), &coefficients(), &[0; 4], limits(), |_| Ok(()))
        .expect_err("preparation and cleanup");
    for operation in ["prepare_contraction", "destroy_workspace", "free"] {
        assert!(error.to_string().contains(operation));
    }
    api.assert_released();
}

#[test]
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "method items cannot satisfy the driver's independently quantified session lifetime"
)]
fn drop_and_explicit_cleanup_failures_attempt_every_release_once() {
    for failure in [
        ("synchronize_stream", 2),
        ("destroy_workspace", 1),
        ("destroy_network", 1),
        ("free", 1),
        ("free", 2),
        ("free", 3),
    ] {
        let api = api(vec![failure], NumericalSettings::default());
        assert!(
            run_numerical(
                api.clone(),
                &coefficients(),
                &[0; 4],
                limits(),
                |execution| execution.contract()
            )
            .is_err()
        );
        api.assert_released();
        assert_eq!(
            api.events()
                .iter()
                .filter(|&&event| event == "free")
                .count(),
            3
        );
    }
    let api = api(vec![], NumericalSettings::default());
    {
        let mut session = SessionResources::new(api.clone(), 0).expect("session");
        let network = shared_chain();
        let mut resources =
            ContractionResources::new(&mut session, &shared_query(&network)).expect("network");
        resources.import(&metadata()).expect("import");
        let mut execution =
            ContractionExecution::prepare(resources, &coefficients(), &[0; 4], limits())
                .expect("prepare");
        execution.contract().expect("contract");
    }
    api.assert_released();
    assert_eq!(
        api.events()
            .iter()
            .filter(|&&event| event == "destroy_workspace")
            .count(),
        1
    );
}
