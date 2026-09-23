use super::*;
use crate::simulation::Stream;
use std::{
    ffi::c_void,
    ptr::NonNull,
    sync::{Arc, Mutex},
};
use tensornet::{Index, TensorNetwork};

#[path = "adapter/tests.rs"]
mod adapter;
#[path = "execution/tests.rs"]
mod execution;

const P1: [[i32; 2]; 3] = [[1, 2], [0, 2], [0, 1]];

fn chain() -> TensorNetwork {
    let axes: Vec<_> = [11, 23, 37, 53, 71]
        .into_iter()
        .zip([2, 3, 5, 7, 11])
        .map(|(id, dim)| Index::new(id, dim).expect("valid fixture and successful test operation"))
        .collect();
    TensorNetwork::new(
        axes.windows(2)
            .map(|pair| {
                Indices::new(pair.to_vec()).expect("valid fixture and successful test operation")
            })
            .collect(),
    )
    .expect("valid fixture and successful test operation")
}

fn query(network: &TensorNetwork) -> ContractionQuery<'_> {
    ContractionQuery::new(
        network,
        Indices::new(vec![
            Index::new(11, 2).expect("valid fixture and successful test operation"),
            Index::new(71, 11).expect("valid fixture and successful test operation"),
        ])
        .expect("valid fixture and successful test operation"),
    )
    .expect("valid fixture and successful test operation")
}

fn metadata() -> NativeMetadata {
    NativeMetadata {
        path: P1.to_vec(),
        slicing: Vec::new(),
        num_slices: 1,
    }
}

fn settings() -> OptimizerSettings {
    OptimizerSettings {
        workspace_constraint: 67_108_864,
        hyper_samples: 1,
        threads: 1,
        seed: 17,
        reconfiguration_iterations: 0,
        disable_rank_simplification: true,
        disable_slicing: true,
    }
}

fn handle(id: usize) -> OpaqueHandle {
    NonNull::new(id as *mut c_void).expect("valid fixture and successful test operation")
}

#[derive(Clone, Copy, Default)]
enum Corruption {
    #[default]
    None,
    DuplicateId,
    PathCount,
    PathOperand,
    ChangedPath,
    NegativeSlicedCount,
    ExcessSlicedCount,
    ChangedSlicedCount,
    SliceCount,
    NegativeRank,
    ExcessRank,
    UnknownMode,
    DuplicateMode,
    Estimate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResourceKind {
    Stream,
    Handle,
    Network,
    Config,
    Info,
    Workspace,
    Allocation,
    SliceGroup,
}

struct Resource {
    kind: ResourceKind,
    parent: Option<usize>,
}

#[derive(Default)]
struct NetworkState {
    tensors: Vec<NativeTensor>,
    tensor_ids: Vec<i64>,
    output: Option<Vec<i32>>,
    info: Option<usize>,
    numerical: execution::NetworkNumericalState,
}

#[derive(Default)]
struct InfoState {
    path: Vec<[i32; 2]>,
    slicing: Vec<SlicedMode>,
}

#[derive(Default)]
struct CallHistory {
    tensors: Vec<NativeTensor>,
    output: Option<Vec<i32>>,
    settings: Vec<(OptimizerSetting, i32)>,
    workspace_constraints: Vec<u64>,
    numerical: execution::NumericalHistory,
}

#[derive(Default)]
struct State {
    events: Vec<&'static str>,
    next_handle: usize,
    live: BTreeMap<usize, Resource>,
    networks: BTreeMap<usize, NetworkState>,
    infos: BTreeMap<usize, InfoState>,
    configs: BTreeMap<usize, Vec<(OptimizerSetting, i32)>>,
    numerical: execution::NumericalState,
    history: CallHistory,
}

impl State {
    fn check(&self, object: OpaqueHandle, kind: ResourceKind) -> usize {
        let id = object.as_ptr() as usize;
        assert_eq!(self.live[&id].kind, kind, "resource {id}");
        id
    }

    fn child(&self, parent: OpaqueHandle, object: OpaqueHandle, kind: ResourceKind) -> usize {
        let id = self.check(object, kind);
        assert_eq!(self.live[&id].parent, Some(parent.as_ptr() as usize));
        assert!(self.live.contains_key(&(parent.as_ptr() as usize)));
        id
    }

    fn network(&self, parent: OpaqueHandle, network: OpaqueHandle) -> &NetworkState {
        &self.networks[&self.child(parent, network, ResourceKind::Network)]
    }

    fn network_mut(&mut self, parent: OpaqueHandle, network: OpaqueHandle) -> &mut NetworkState {
        let id = self.child(parent, network, ResourceKind::Network);
        self.networks.get_mut(&id).expect("live network state")
    }

    fn info_id(&self, parent: OpaqueHandle, info: OpaqueHandle) -> usize {
        let id = self.check(info, ResourceKind::Info);
        let network = self.live[&id].parent.expect("info's network");
        self.child(parent, handle(network), ResourceKind::Network);
        id
    }

    fn info(&self, parent: OpaqueHandle, info: OpaqueHandle) -> &InfoState {
        &self.infos[&self.info_id(parent, info)]
    }

    fn info_mut(&mut self, parent: OpaqueHandle, info: OpaqueHandle) -> &mut InfoState {
        let id = self.info_id(parent, info);
        self.infos.get_mut(&id).expect("live optimizer info")
    }

    fn assert_idle(&self, network: usize) {
        assert!(
            self.numerical
                .pending
                .values()
                .all(|pending| !pending.contains(&network)),
            "synchronize before releasing network resources"
        );
    }
}

struct NativeObservations {
    selected: NativeMetadata,
    modes: Vec<Vec<i32>>,
    estimates: [f64; 2],
    memory: Vec<(usize, usize)>,
}

impl Default for NativeObservations {
    fn default() -> Self {
        Self {
            selected: metadata(),
            modes: vec![vec![23, 53], vec![11, 53], vec![11, 71]],
            estimates: [42.0, 42.0],
            memory: vec![(1024 * 1024, 2 * 1024 * 1024)],
        }
    }
}

struct TestDoubleContractionApi {
    state: Mutex<State>,
    failures: Vec<(&'static str, usize)>,
    corruption: Corruption,
    numerical: execution::NumericalSettings,
    observations: NativeObservations,
}

impl TestDoubleContractionApi {
    fn new(failures: Vec<(&'static str, usize)>, corruption: Corruption) -> Arc<Self> {
        Self::with_observations(failures, corruption, NativeObservations::default())
    }

    fn with_observations(
        failures: Vec<(&'static str, usize)>,
        corruption: Corruption,
        observations: NativeObservations,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            failures,
            corruption,
            numerical: execution::NumericalSettings::default(),
            observations,
        })
    }

    fn event(&self, event: &'static str) -> Result<(), SimulationError> {
        let mut state = self.state.lock().expect("test state lock should succeed");
        state.events.push(event);
        let count = state.events.iter().filter(|&&name| name == event).count();
        if self.failures.contains(&(event, count)) {
            return Err(SimulationError::NativeCallFailed {
                component: "fake",
                operation: event,
                status: 7,
                message: "injected failure".to_string(),
            });
        }
        Ok(())
    }

    fn create(
        &self,
        event: &'static str,
        kind: ResourceKind,
        parent: Option<OpaqueHandle>,
    ) -> Result<OpaqueHandle, SimulationError> {
        self.event(event)?;
        let mut state = self.state.lock().expect("test state lock should succeed");
        let parent = parent.map(|parent| {
            let id = parent.as_ptr() as usize;
            assert!(state.live.contains_key(&id));
            id
        });
        state.next_handle = state.next_handle.checked_add(1).expect("test handle space");
        let id = state.next_handle;
        assert!(state.live.insert(id, Resource { kind, parent }).is_none());
        Ok(handle(id))
    }

    fn destroy(&self, event: &'static str, object: OpaqueHandle) -> Result<(), SimulationError> {
        let id = object.as_ptr() as usize;
        let mut state = self.state.lock().expect("test state lock should succeed");
        let kind = state.live.get(&id).expect("no double destruction").kind;
        let expected = match event {
            "destroy_stream" => ResourceKind::Stream,
            "destroy_handle" => ResourceKind::Handle,
            "destroy_network" => ResourceKind::Network,
            "destroy_optimizer_config" => ResourceKind::Config,
            "destroy_optimizer_info" => ResourceKind::Info,
            "destroy_workspace" => ResourceKind::Workspace,
            "free" => ResourceKind::Allocation,
            "destroy_slice_group" => ResourceKind::SliceGroup,
            _ => panic!("unknown destruction operation {event}"),
        };
        assert_eq!(kind, expected, "destructor must match its resource");
        assert!(
            state.live.values().all(|child| child.parent != Some(id)),
            "children must close before parent {id}"
        );
        match kind {
            ResourceKind::Network => {
                state.assert_idle(id);
                assert!(
                    state
                        .numerical
                        .workspaces
                        .values()
                        .all(|workspace| workspace.network != Some(id)),
                    "workspace must close before network"
                );
                state.networks.remove(&id).expect("live network");
            }
            ResourceKind::Config => {
                state.configs.remove(&id).expect("live config");
            }
            ResourceKind::Info => {
                state.infos.remove(&id).expect("live info");
            }
            ResourceKind::Workspace => {
                if let Some(network) = state.numerical.workspaces[&id].network {
                    state.assert_idle(network);
                }
                state
                    .numerical
                    .workspaces
                    .remove(&id)
                    .expect("live workspace");
            }
            ResourceKind::Allocation => {
                assert!(
                    state.networks.values().all(|network| {
                        network.numerical.output != Some(id)
                            && !network.numerical.inputs.values().any(|&input| input == id)
                    }),
                    "network must close before its buffers"
                );
                assert!(
                    state.numerical.workspaces.values().all(|workspace| {
                        !workspace.bindings.iter().any(|(space, _, allocation, _)| {
                            *space == crate::simulation::memory_workspace::MemorySpace::Device
                                && *allocation == Some(id)
                        })
                    }),
                    "workspace must close before its buffers"
                );
                state
                    .numerical
                    .allocations
                    .remove(&id)
                    .expect("live allocation");
            }
            ResourceKind::Stream => {
                assert!(state.numerical.pending[&id].is_empty());
                assert!(
                    state
                        .networks
                        .values()
                        .all(|network| network.numerical.stream != Some(id)),
                    "network must close before its stream"
                );
                state.numerical.pending.remove(&id).expect("live stream");
            }
            ResourceKind::Handle | ResourceKind::SliceGroup => {}
        }
        state.live.remove(&id).expect("live resource");
        drop(state);
        self.event(event)
    }

    fn events(&self) -> Vec<&'static str> {
        self.state
            .lock()
            .expect("test state lock should succeed")
            .events
            .clone()
    }

    fn assert_released(&self) {
        let state = self.state.lock().expect("test state lock should succeed");
        assert!(state.live.is_empty());
        assert!(state.networks.is_empty());
        assert!(state.infos.is_empty());
        assert!(state.configs.is_empty());
        assert!(state.numerical.allocations.is_empty());
        assert!(state.numerical.workspaces.is_empty());
        assert!(state.numerical.pending.is_empty());
    }
}

impl SessionApi for TestDoubleContractionApi {
    fn device_count(&self) -> Result<i32, SimulationError> {
        self.event("device_count")?;
        Ok(1)
    }
    fn set_device(&self, ordinal: i32) -> Result<(), SimulationError> {
        assert_eq!(ordinal, 0);
        self.event("set_device")
    }
    fn create_stream(&self) -> Result<Stream, SimulationError> {
        let stream = self.create("create_stream", ResourceKind::Stream, None)?;
        self.state
            .lock()
            .expect("state")
            .numerical
            .pending
            .insert(stream.as_ptr() as usize, BTreeSet::new());
        Ok(stream)
    }
    fn synchronize_stream(&self, stream: Stream) -> Result<(), SimulationError> {
        let result = self.event("synchronize_stream");
        // Model a reported asynchronous error after the stream has drained.
        let mut state = self.state.lock().expect("test state");
        let id = state.check(stream, ResourceKind::Stream);
        state
            .numerical
            .pending
            .get_mut(&id)
            .expect("live stream")
            .clear();
        result
    }
    fn destroy_stream(&self, stream: Stream) -> Result<(), SimulationError> {
        self.destroy("destroy_stream", stream)
    }
    fn create_handle(&self) -> Result<OpaqueHandle, SimulationError> {
        self.create("create_handle", ResourceKind::Handle, None)
    }
    fn destroy_handle(&self, object: OpaqueHandle) -> Result<(), SimulationError> {
        self.destroy("destroy_handle", object)
    }
}

impl ContractionApi for TestDoubleContractionApi {
    fn create_network(&self, parent: OpaqueHandle) -> Result<OpaqueHandle, SimulationError> {
        self.state
            .lock()
            .expect("state")
            .check(parent, ResourceKind::Handle);
        let network = self.create("create_network", ResourceKind::Network, Some(parent))?;
        self.state
            .lock()
            .expect("state")
            .networks
            .insert(network.as_ptr() as usize, NetworkState::default());
        Ok(network)
    }
    fn destroy_network(&self, object: OpaqueHandle) -> Result<(), SimulationError> {
        self.destroy("destroy_network", object)
    }
    fn append_tensor(
        &self,
        parent: OpaqueHandle,
        network: OpaqueHandle,
        tensor: &NativeTensor,
    ) -> Result<i64, SimulationError> {
        self.event("append_tensor")?;
        let mut state = self.state.lock().expect("test state lock should succeed");
        let id = if matches!(self.corruption, Corruption::DuplicateId) {
            90
        } else {
            [90, 7, 400, 12][state.network(parent, network).tensors.len()]
        };
        state
            .network_mut(parent, network)
            .tensors
            .push(tensor.clone());
        state.network_mut(parent, network).tensor_ids.push(id);
        state.history.tensors.push(tensor.clone());
        Ok(id)
    }
    fn set_output(
        &self,
        parent: OpaqueHandle,
        network: OpaqueHandle,
        modes: &[i32],
    ) -> Result<(), SimulationError> {
        self.event("set_output")?;
        let mut state = self.state.lock().expect("state");
        state.network_mut(parent, network).output = Some(modes.to_vec());
        state.history.output = Some(modes.to_vec());
        Ok(())
    }
    fn set_compute_f64(
        &self,
        parent: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.state.lock().expect("state").network(parent, network);
        self.event("set_compute_f64")
    }
    fn create_optimizer_config(
        &self,
        parent: OpaqueHandle,
    ) -> Result<OpaqueHandle, SimulationError> {
        self.state
            .lock()
            .expect("state")
            .check(parent, ResourceKind::Handle);
        let config = self.create(
            "create_optimizer_config",
            ResourceKind::Config,
            Some(parent),
        )?;
        self.state
            .lock()
            .expect("state")
            .configs
            .insert(config.as_ptr() as usize, Vec::new());
        Ok(config)
    }
    fn destroy_optimizer_config(&self, object: OpaqueHandle) -> Result<(), SimulationError> {
        self.destroy("destroy_optimizer_config", object)
    }
    fn configure_optimizer(
        &self,
        parent: OpaqueHandle,
        config: OpaqueHandle,
        setting: OptimizerSetting,
        value: i32,
    ) -> Result<(), SimulationError> {
        self.event("configure_optimizer")?;
        let mut state = self.state.lock().expect("state");
        let id = state.child(parent, config, ResourceKind::Config);
        state
            .configs
            .get_mut(&id)
            .expect("live config")
            .push((setting, value));
        state.history.settings.push((setting, value));
        Ok(())
    }
    fn create_optimizer_info(
        &self,
        parent: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<OpaqueHandle, SimulationError> {
        {
            let state = self.state.lock().expect("test state lock should succeed");
            let network = state.network(parent, network);
            assert_eq!(
                network.tensors.len(),
                self.observations.selected.path.len() + 1,
                "topology precedes optimizer info"
            );
            assert!(network.output.is_some(), "output precedes optimizer info");
        }
        let info = self.create("create_optimizer_info", ResourceKind::Info, Some(network))?;
        self.state
            .lock()
            .expect("state")
            .infos
            .insert(info.as_ptr() as usize, InfoState::default());
        Ok(info)
    }
    fn destroy_optimizer_info(&self, object: OpaqueHandle) -> Result<(), SimulationError> {
        self.destroy("destroy_optimizer_info", object)
    }
    fn optimize(
        &self,
        parent: OpaqueHandle,
        network: OpaqueHandle,
        config: OpaqueHandle,
        workspace_constraint: u64,
        info: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.event("optimize")?;
        let mut state = self.state.lock().expect("test state lock should succeed");
        state.child(parent, config, ResourceKind::Config);
        state.child(network, info, ResourceKind::Info);
        let selected = state.info_mut(parent, info);
        selected.path.clone_from(&self.observations.selected.path);
        selected
            .slicing
            .clone_from(&self.observations.selected.slicing);
        state.network_mut(parent, network).info = Some(info.as_ptr() as usize);
        state
            .history
            .workspace_constraints
            .push(workspace_constraint);
        Ok(())
    }
    fn set_path(
        &self,
        parent: OpaqueHandle,
        info: OpaqueHandle,
        path: &[[i32; 2]],
    ) -> Result<(), SimulationError> {
        self.event("set_path")?;
        self.state
            .lock()
            .expect("test state lock should succeed")
            .info_mut(parent, info)
            .path = path.to_vec();
        Ok(())
    }
    fn set_slicing(
        &self,
        parent: OpaqueHandle,
        info: OpaqueHandle,
        slicing: &[SlicedMode],
    ) -> Result<(), SimulationError> {
        self.event("set_slicing")?;
        self.state
            .lock()
            .expect("test state lock should succeed")
            .info_mut(parent, info)
            .slicing = slicing.to_vec();
        Ok(())
    }
    fn attach_optimizer_info(
        &self,
        parent: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        self.event("attach_optimizer_info")?;
        let mut state = self.state.lock().expect("state");
        state.child(network, info, ResourceKind::Info);
        state.network_mut(parent, network).info = Some(info.as_ptr() as usize);
        Ok(())
    }
    fn read_path(
        &self,
        parent: OpaqueHandle,
        info: OpaqueHandle,
        path: &mut [[i32; 2]],
    ) -> Result<i32, SimulationError> {
        self.event("read_path")?;
        path.copy_from_slice(
            &self
                .state
                .lock()
                .expect("test state lock should succeed")
                .info(parent, info)
                .path,
        );
        if matches!(self.corruption, Corruption::PathOperand) {
            path[1] = [0, 3];
        }
        if matches!(self.corruption, Corruption::ChangedPath) {
            path[0].swap(0, 1);
        }
        Ok(if matches!(self.corruption, Corruption::PathCount) {
            -1
        } else {
            i32::try_from(path.len()).expect("fixture path length")
        })
    }
    fn num_sliced_modes(
        &self,
        parent: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<i32, SimulationError> {
        self.event("num_sliced_modes")?;
        Ok(match self.corruption {
            Corruption::NegativeSlicedCount => -1,
            Corruption::ExcessSlicedCount => i32::MAX,
            _ => i32::try_from(
                self.state
                    .lock()
                    .expect("test state lock should succeed")
                    .info(parent, info)
                    .slicing
                    .len(),
            )
            .expect("valid fixture and successful test operation"),
        })
    }
    fn read_slicing(
        &self,
        parent: OpaqueHandle,
        info: OpaqueHandle,
        slicing: &mut [SlicedMode],
    ) -> Result<u32, SimulationError> {
        self.event("read_slicing")?;
        slicing.copy_from_slice(
            &self
                .state
                .lock()
                .expect("test state lock should succeed")
                .info(parent, info)
                .slicing,
        );
        Ok(
            if matches!(self.corruption, Corruption::ChangedSlicedCount) {
                7
            } else {
                u32::try_from(slicing.len()).expect("valid fixture and successful test operation")
            },
        )
    }
    fn num_slices(&self, parent: OpaqueHandle, info: OpaqueHandle) -> Result<i64, SimulationError> {
        self.event("num_slices")?;
        if matches!(self.corruption, Corruption::SliceCount) {
            return Ok(0);
        }
        Ok(
            if self
                .state
                .lock()
                .expect("test state lock should succeed")
                .info(parent, info)
                .slicing
                .is_empty()
            {
                1
            } else {
                3
            },
        )
    }
    fn intermediate_mode_counts(
        &self,
        parent: OpaqueHandle,
        info: OpaqueHandle,
        counts: &mut [i32],
    ) -> Result<(), SimulationError> {
        self.event("intermediate_mode_counts")?;
        self.state.lock().expect("state").info(parent, info);
        for (count, modes) in counts.iter_mut().zip(&self.observations.modes) {
            *count = i32::try_from(modes.len()).expect("fixture rank");
        }
        match self.corruption {
            Corruption::NegativeRank => counts[0] = -1,
            Corruption::ExcessRank => counts[0] = i32::MAX,
            _ => {}
        }
        Ok(())
    }
    fn intermediate_modes(
        &self,
        parent: OpaqueHandle,
        info: OpaqueHandle,
        modes: &mut [i32],
    ) -> Result<(), SimulationError> {
        self.event("intermediate_modes")?;
        self.state.lock().expect("state").info(parent, info);
        modes.copy_from_slice(
            &self
                .observations
                .modes
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
        );
        match self.corruption {
            Corruption::UnknownMode => modes[0] = 999,
            Corruption::DuplicateMode => modes[0] = modes[1],
            _ => {}
        }
        Ok(())
    }
    fn estimate(
        &self,
        parent: OpaqueHandle,
        info: OpaqueHandle,
        estimate: OptimizerEstimate,
    ) -> Result<f64, SimulationError> {
        self.event("estimate")?;
        self.state.lock().expect("state").info(parent, info);
        Ok(if matches!(self.corruption, Corruption::Estimate) {
            f64::NAN
        } else {
            self.observations.estimates[match estimate {
                OptimizerEstimate::FlopCount => 0,
                OptimizerEstimate::LargestTensor => 1,
            }]
        })
    }
    fn create_slice_group_from_id_range(
        &self,
        parent: OpaqueHandle,
        start: i64,
        stop: i64,
        increment: i64,
    ) -> Result<OpaqueHandle, SimulationError> {
        assert_eq!((start, stop, increment), (0, 8, 2));
        self.create("create_slice_group", ResourceKind::SliceGroup, Some(parent))
    }
    fn destroy_slice_group(&self, object: OpaqueHandle) -> Result<(), SimulationError> {
        self.destroy("destroy_slice_group", object)
    }
}

fn run<T>(
    api: Arc<TestDoubleContractionApi>,
    operation: impl FnOnce(
        &mut ContractionResources<'_, TestDoubleContractionApi>,
    ) -> Result<T, SimulationError>,
) -> Result<T, SimulationError> {
    let mut session = SessionResources::new(api, 0)?;
    let network = chain();
    let execution = (|| {
        let mut resources = ContractionResources::new(&mut session, &query(&network))?;
        let result = operation(&mut resources);
        combine_execution_and_cleanup(result, resources.close())
    })();
    combine_execution_and_cleanup(execution, session.close())
}

#[test]
fn topology_precedes_info_and_retains_native_ids_separately_from_path_positions() {
    let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
    let (exported, modes) = run(api.clone(), |resources| {
        assert_eq!(resources.tensor_ids(), [90, 7, 400, 12]);
        resources.import(&metadata())?;
        Ok((resources.export()?, resources.intermediate_modes()?))
    })
    .expect("valid fixture and successful test operation");
    assert_eq!(exported, metadata());
    assert_eq!(modes, [vec![23, 53], vec![11, 53], vec![11, 71]]);
    let state = api.state.lock().expect("test state lock should succeed");
    assert_eq!(state.history.output.as_deref(), Some([11, 71].as_slice()));
    assert_eq!(
        state.history.tensors,
        [
            NativeTensor {
                modes: vec![11, 23],
                extents: vec![2, 3]
            },
            NativeTensor {
                modes: vec![23, 37],
                extents: vec![3, 5]
            },
            NativeTensor {
                modes: vec![37, 53],
                extents: vec![5, 7]
            },
            NativeTensor {
                modes: vec![53, 71],
                extents: vec![7, 11]
            },
        ]
    );
    assert!(!state.events.contains(&"optimize"));
    assert!(!state.events.contains(&"create_optimizer_config"));
    drop(state);
    api.assert_released();
}

#[test]
fn optimize_export_close_and_import_into_fresh_owners_preserves_owned_metadata() {
    let source = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
    let exported = run(source.clone(), |resources| {
        resources.optimize(settings())?;
        resources.export()
    })
    .expect("valid fixture and successful test operation");
    source.assert_released();
    assert_eq!(
        source
            .state
            .lock()
            .expect("test state")
            .history
            .workspace_constraints,
        [67_108_864]
    );
    assert_eq!(
        source
            .state
            .lock()
            .expect("test state lock should succeed")
            .history
            .settings,
        [
            (OptimizerSetting::HyperSamples, 1),
            (OptimizerSetting::Threads, 1),
            (OptimizerSetting::Seed, 17),
            (OptimizerSetting::ReconfigurationIterations, 0),
            (OptimizerSetting::DisableRankSimplification, 1),
            (OptimizerSetting::DisableSlicing, 1),
        ]
    );
    drop(source);
    let destination = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
    let imported = run(destination.clone(), |resources| {
        resources.import(&exported)?;
        resources.export()
    })
    .expect("valid fixture and successful test operation");
    assert_eq!(imported, exported);
    assert!(!destination.events().contains(&"optimize"));
    destination.assert_released();
}

#[test]
fn explicit_internal_unit_extent_slicing_retains_full_coverage() {
    let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
    let expected = NativeMetadata {
        slicing: vec![SlicedMode {
            mode: 23,
            extent: 1,
        }],
        num_slices: 3,
        ..metadata()
    };
    let actual = run(api.clone(), |resources| {
        resources.import(&expected)?;
        resources.export()
    })
    .expect("valid fixture and successful test operation");
    assert_eq!(actual, expected);
    api.assert_released();
}

#[test]
fn malformed_paths_and_slicing_fail_before_native_setters() {
    let mut cases: Vec<_> = [
        vec![],
        vec![[0, 1]],
        vec![[1, 2], [0, 3], [0, 1]],
        vec![[1, 1], [0, 2], [0, 1]],
        vec![[-1, 2], [0, 2], [0, 1]],
        vec![[90, 7], [0, 2], [0, 1]],
        vec![[1, 2], [0, 2], [0, 2]],
    ]
    .into_iter()
    .map(|path| NativeMetadata { path, ..metadata() })
    .collect();
    for slicing in [
        vec![SlicedMode {
            mode: 999,
            extent: 1,
        }],
        vec![SlicedMode {
            mode: 11,
            extent: 1,
        }],
        vec![SlicedMode {
            mode: 23,
            extent: 2,
        }],
        vec![SlicedMode {
            mode: 23,
            extent: 0,
        }],
        vec![SlicedMode {
            mode: 23,
            extent: -1,
        }],
        vec![
            SlicedMode {
                mode: 23,
                extent: 1
            };
            2
        ],
    ] {
        cases.push(NativeMetadata {
            slicing,
            num_slices: 3,
            ..metadata()
        });
    }
    cases.push(NativeMetadata {
        num_slices: 2,
        ..metadata()
    });
    for invalid in cases {
        let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
        assert!(run(api.clone(), |resources| resources.import(&invalid)).is_err());
        assert!(!api.events().contains(&"set_path"));
        api.assert_released();
    }
}

#[test]
fn count_and_query_narrowing_fail_without_allocating_large_buffers_or_native_topology() {
    assert!(native_count(usize::MAX, "count").is_err());
    assert!(validate_path(usize::MAX, &[]).is_err());
    let mut invalid_axes = vec![(u32::MAX, 2)];
    if let Ok(dimension) = usize::try_from(1_u64 << 63) {
        invalid_axes.push((11, dimension));
    }
    for (id, dim) in invalid_axes {
        let axis = Index::new(id, dim).expect("valid fixture and successful test operation");
        let node = Indices::new(vec![axis]).expect("valid fixture and successful test operation");
        let network = TensorNetwork::new(vec![node.clone(), node])
            .expect("valid fixture and successful test operation");
        let query = ContractionQuery::new(
            &network,
            Indices::new(Vec::new()).expect("valid fixture and successful test operation"),
        )
        .expect("valid fixture and successful test operation");
        let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
        let mut session = SessionResources::new(api.clone(), 0)
            .expect("valid fixture and successful test operation");
        assert!(ContractionResources::new(&mut session, &query).is_err());
        session
            .close()
            .expect("valid fixture and successful test operation");
        assert!(!api.events().contains(&"create_network"));
        api.assert_released();
    }
}

#[test]
fn slice_count_product_is_checked() {
    let axes = [
        Index::new(11, 2_000_000_000).expect("positive extent"),
        Index::new(23, 2_000_000_000).expect("positive extent"),
        Index::new(37, 3).expect("positive extent"),
    ];
    let node = Indices::new(axes.to_vec()).expect("valid fixture and successful test operation");
    let network = TensorNetwork::new(vec![node.clone(), node])
        .expect("valid fixture and successful test operation");
    let query = ContractionQuery::new(
        &network,
        Indices::new(Vec::new()).expect("valid fixture and successful test operation"),
    )
    .expect("valid fixture and successful test operation");
    let topology = Topology::new(&query).expect("valid fixture and successful test operation");
    let metadata = NativeMetadata {
        path: vec![[0, 1]],
        slicing: vec![
            SlicedMode {
                mode: 11,
                extent: 1,
            },
            SlicedMode {
                mode: 23,
                extent: 1,
            },
            SlicedMode {
                mode: 37,
                extent: 1,
            },
        ],
        num_slices: 1,
    };
    assert!(matches!(
        topology.validate(&metadata),
        Err(SimulationError::ResourceSizeOverflow {
            resource: "slice count"
        })
    ));
}

#[test]
fn unpopulated_or_failed_import_metadata_cannot_be_read() {
    let api = TestDoubleContractionApi::new(vec![("attach_optimizer_info", 1)], Corruption::None);
    run(api.clone(), |resources| {
        assert!(resources.export().is_err());
        assert!(resources.intermediate_modes().is_err());
        assert!(resources.import(&metadata()).is_err());
        assert!(resources.export().is_err());
        Ok(())
    })
    .expect("valid fixture and successful test operation");
    assert!(!api.events().contains(&"read_path"));
    api.assert_released();
}

#[test]
fn invalid_optimizer_settings_do_not_create_config_or_search() {
    for invalid in [
        OptimizerSettings {
            workspace_constraint: 0,
            ..settings()
        },
        OptimizerSettings {
            hyper_samples: -1,
            ..settings()
        },
        OptimizerSettings {
            threads: 0,
            ..settings()
        },
        OptimizerSettings {
            seed: -1,
            ..settings()
        },
        OptimizerSettings {
            reconfiguration_iterations: -1,
            ..settings()
        },
    ] {
        let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
        assert!(run(api.clone(), |resources| resources.optimize(invalid)).is_err());
        assert!(!api.events().contains(&"create_optimizer_config"));
        api.assert_released();
    }
}

#[test]
fn all_construction_and_import_boundaries_release_acquired_resources() {
    for (event, occurrence) in [
        ("device_count", 1),
        ("set_device", 1),
        ("create_stream", 1),
        ("create_handle", 1),
        ("set_device", 2),
        ("create_network", 1),
        ("append_tensor", 1),
        ("append_tensor", 2),
        ("append_tensor", 3),
        ("append_tensor", 4),
        ("set_output", 1),
        ("set_compute_f64", 1),
        ("create_optimizer_info", 1),
        ("set_path", 1),
        ("set_slicing", 1),
        ("attach_optimizer_info", 1),
    ] {
        let api = TestDoubleContractionApi::new(vec![(event, occurrence)], Corruption::None);
        let error = run(api.clone(), |resources| resources.import(&metadata()))
            .expect_err("injected failure must propagate");
        assert!(error.to_string().contains(event), "{event}: {error}");
        api.assert_released();
    }
}

#[test]
fn all_optimization_and_retrieval_boundaries_propagate_failure_and_release_resources() {
    for event in [
        "create_optimizer_config",
        "configure_optimizer",
        "optimize",
        "read_path",
        "num_sliced_modes",
        "read_slicing",
        "num_slices",
        "intermediate_mode_counts",
        "intermediate_modes",
        "estimate",
    ] {
        let api = TestDoubleContractionApi::new(vec![(event, 1)], Corruption::None);
        let error = run(api.clone(), |resources| {
            resources.optimize(settings())?;
            resources.export()?;
            resources.intermediate_modes()?;
            resources.estimate(OptimizerEstimate::FlopCount)
        })
        .expect_err("injected failure must propagate");
        assert!(error.to_string().contains(event), "{event}: {error}");
        api.assert_released();
    }
}

#[test]
fn native_metadata_is_validated_before_it_escapes() {
    for corruption in [
        Corruption::DuplicateId,
        Corruption::PathCount,
        Corruption::PathOperand,
        Corruption::NegativeSlicedCount,
        Corruption::ExcessSlicedCount,
        Corruption::ChangedSlicedCount,
        Corruption::SliceCount,
        Corruption::NegativeRank,
        Corruption::ExcessRank,
        Corruption::UnknownMode,
        Corruption::DuplicateMode,
        Corruption::Estimate,
    ] {
        let api = TestDoubleContractionApi::new(Vec::new(), corruption);
        assert!(matches!(
            run(api.clone(), |resources| {
                resources.import(&metadata())?;
                resources.export()?;
                resources.intermediate_modes()?;
                resources.estimate(OptimizerEstimate::LargestTensor)
            }),
            Err(SimulationError::InvalidNativeResult { .. })
        ));
        api.assert_released();
    }
}

#[test]
fn construction_error_and_cleanup_error_are_both_preserved() {
    for failures in [
        vec![("create_handle", 1), ("destroy_stream", 1)],
        vec![("create_optimizer_info", 1), ("destroy_network", 1)],
        vec![("set_output", 1), ("destroy_network", 1)],
    ] {
        let api = TestDoubleContractionApi::new(failures.clone(), Corruption::None);
        let error = run(api.clone(), |_| Ok(())).expect_err("injected failure must propagate");
        assert!(matches!(
            error,
            SimulationError::ExecutionAndCleanupFailed { .. }
        ));
        for (event, _) in failures {
            assert!(error.to_string().contains(event));
        }
        api.assert_released();
    }
}

#[test]
fn operation_and_cleanup_failures_do_not_hide_each_other_or_strand_resources() {
    let failures = vec![
        ("read_path", 1),
        ("destroy_optimizer_info", 1),
        ("destroy_optimizer_config", 1),
        ("destroy_network", 1),
        ("destroy_handle", 1),
    ];
    let api = TestDoubleContractionApi::new(failures.clone(), Corruption::None);
    let error = run(api.clone(), |resources| {
        resources.optimize(settings())?;
        resources.export()
    })
    .expect_err("injected failure must propagate");
    for (event, _) in failures {
        assert!(error.to_string().contains(event), "{error}");
    }
    api.assert_released();
}

#[test]
fn cleanup_failure_is_reported_even_after_successful_operation() {
    for event in [
        "destroy_optimizer_info",
        "destroy_optimizer_config",
        "destroy_network",
        "synchronize_stream",
        "destroy_handle",
        "destroy_stream",
    ] {
        let api = TestDoubleContractionApi::new(vec![(event, 1)], Corruption::None);
        let error = run(api.clone(), |resources| {
            resources.optimize(settings())?;
            resources.export()
        })
        .expect_err("injected failure must propagate");
        assert!(error.to_string().contains(event), "{event}: {error}");
        api.assert_released();
    }
}

#[test]
fn drop_releases_children_once_before_parent() {
    let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
    {
        let mut session = SessionResources::new(api.clone(), 0)
            .expect("valid fixture and successful test operation");
        let network = chain();
        let mut resources = ContractionResources::new(&mut session, &query(&network))
            .expect("valid fixture and successful test operation");
        resources
            .optimize(settings())
            .expect("valid fixture and successful test operation");
    }
    api.assert_released();
    for event in [
        "destroy_optimizer_info",
        "destroy_optimizer_config",
        "destroy_network",
        "destroy_handle",
        "destroy_stream",
    ] {
        assert_eq!(
            api.events().iter().filter(|&&name| name == event).count(),
            1
        );
    }
}

#[test]
fn slice_group_borrows_parent_and_rejects_zero_step_before_native_creation() {
    let api = TestDoubleContractionApi::new(Vec::new(), Corruption::None);
    let mut session =
        SessionResources::new(api.clone(), 0).expect("valid fixture and successful test operation");
    assert!(SliceGroup::from_id_range(&mut session, 0, 8, 0).is_err());
    assert!(!api.events().contains(&"create_slice_group"));
    let group = SliceGroup::from_id_range(&mut session, 0, 8, 2)
        .expect("valid fixture and successful test operation");
    api.state
        .lock()
        .expect("state")
        .check(group.as_handle(), ResourceKind::SliceGroup);
    drop(group);
    session
        .close()
        .expect("valid fixture and successful test operation");
    api.assert_released();
}
