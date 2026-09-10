use super::{OpaqueHandle, SimulationError};
use std::sync::Arc;

pub(crate) trait ContractionApi {
    fn create_network(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_network(&self, network: OpaqueHandle) -> Result<(), SimulationError>;
    fn create_optimizer_config(
        &self,
        handle: OpaqueHandle,
    ) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_optimizer_config(&self, config: OpaqueHandle) -> Result<(), SimulationError>;
    fn create_optimizer_info(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_optimizer_info(&self, info: OpaqueHandle) -> Result<(), SimulationError>;
    fn create_slice_group_from_id_range(
        &self,
        handle: OpaqueHandle,
        start: i64,
        stop: i64,
        increment: i64,
    ) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_slice_group(&self, slice_group: OpaqueHandle) -> Result<(), SimulationError>;
}

pub(crate) struct ContractionResources<Api: ContractionApi> {
    api: Arc<Api>,
    handle: OpaqueHandle,
    network: Option<OpaqueHandle>,
    optimizer_config: Option<OpaqueHandle>,
    optimizer_info: Option<OpaqueHandle>,
}

impl<Api: ContractionApi> ContractionResources<Api> {
    pub(crate) fn new(api: Arc<Api>, handle: OpaqueHandle) -> Result<Self, SimulationError> {
        let network = api.create_network(handle)?;
        let mut resources = Self {
            api,
            handle,
            network: Some(network),
            optimizer_config: None,
            optimizer_info: None,
        };
        resources.optimizer_config = Some(resources.api.create_optimizer_config(handle)?);
        resources.optimizer_info = Some(resources.api.create_optimizer_info(handle, network)?);
        Ok(resources)
    }

    pub(crate) fn handle(&self) -> OpaqueHandle {
        self.handle
    }

    pub(crate) fn network(&self) -> OpaqueHandle {
        self.network
            .expect("live contraction resources always own their network descriptor")
    }

    pub(crate) fn optimizer_config(&self) -> OpaqueHandle {
        self.optimizer_config
            .expect("live contraction resources always own their optimizer config")
    }

    pub(crate) fn optimizer_info(&self) -> OpaqueHandle {
        self.optimizer_info
            .expect("live contraction resources always own their optimizer info")
    }

    pub(crate) fn close(&mut self) -> Result<(), SimulationError> {
        let mut first_error = None;
        if let Some(info) = self.optimizer_info.take()
            && let Err(error) = self.api.destroy_optimizer_info(info)
        {
            first_error = Some(error);
        }
        if let Some(config) = self.optimizer_config.take()
            && let Err(error) = self.api.destroy_optimizer_config(config)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(network) = self.network.take()
            && let Err(error) = self.api.destroy_network(network)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl<Api: ContractionApi> Drop for ContractionResources<Api> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

pub(crate) struct SliceGroup<Api: ContractionApi> {
    api: Arc<Api>,
    slice_group: Option<OpaqueHandle>,
}

impl<Api: ContractionApi> SliceGroup<Api> {
    pub(crate) fn from_id_range(
        api: Arc<Api>,
        handle: OpaqueHandle,
        start: i64,
        stop: i64,
        increment: i64,
    ) -> Result<Self, SimulationError> {
        if increment == 0 {
            return Err(SimulationError::InvalidContractionConfiguration {
                reason: "slice identifier increment must be non-zero",
            });
        }
        let slice_group = api.create_slice_group_from_id_range(handle, start, stop, increment)?;
        Ok(Self {
            api,
            slice_group: Some(slice_group),
        })
    }

    pub(crate) fn as_handle(&self) -> OpaqueHandle {
        self.slice_group
            .expect("a live slice group always owns its native object")
    }

    pub(crate) fn close(&mut self) -> Result<(), SimulationError> {
        self.slice_group.take().map_or(Ok(()), |slice_group| {
            self.api.destroy_slice_group(slice_group)
        })
    }
}

impl<Api: ContractionApi> Drop for SliceGroup<Api> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::{ContractionApi, ContractionResources, OpaqueHandle, SimulationError, SliceGroup};
    use std::{
        ffi::c_void,
        ptr::NonNull,
        sync::{Arc, Mutex},
    };

    const SESSION_HANDLE: usize = 0x1000;
    const NETWORK: usize = 0x2000;
    const OPTIMIZER_CONFIG: usize = 0x3000;
    const OPTIMIZER_INFO: usize = 0x4000;
    const SLICE_GROUP: usize = 0x5000;

    fn handle_at(address: usize) -> OpaqueHandle {
        NonNull::new(address as *mut c_void).expect("test addresses are non-zero")
    }

    struct FakeApi {
        events: Mutex<Vec<&'static str>>,
        released: Mutex<Vec<usize>>,
        fail_on: Option<&'static str>,
    }

    impl FakeApi {
        fn new(fail_on: Option<&'static str>) -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                released: Mutex::new(Vec::new()),
                fail_on,
            }
        }

        fn record(&self, event: &'static str) -> Result<(), SimulationError> {
            self.events
                .lock()
                .expect("event lock should succeed")
                .push(event);
            if self.fail_on == Some(event) {
                return Err(SimulationError::NativeCallFailed {
                    component: "cuTensorNet",
                    operation: event,
                    status: 7,
                    message: "simulated failure".to_string(),
                });
            }
            Ok(())
        }

        fn release(&self, handle: OpaqueHandle) {
            self.released
                .lock()
                .expect("release lock should succeed")
                .push(handle.as_ptr() as usize);
        }

        fn events(&self) -> Vec<&'static str> {
            self.events
                .lock()
                .expect("event lock should succeed")
                .clone()
        }

        fn released(&self) -> Vec<usize> {
            self.released
                .lock()
                .expect("release lock should succeed")
                .clone()
        }
    }

    impl ContractionApi for FakeApi {
        fn create_network(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, SimulationError> {
            assert_eq!(handle, handle_at(SESSION_HANDLE));
            self.record("create_network")?;
            Ok(handle_at(NETWORK))
        }

        fn destroy_network(&self, network: OpaqueHandle) -> Result<(), SimulationError> {
            self.release(network);
            self.record("destroy_network")
        }

        fn create_optimizer_config(
            &self,
            handle: OpaqueHandle,
        ) -> Result<OpaqueHandle, SimulationError> {
            assert_eq!(handle, handle_at(SESSION_HANDLE));
            self.record("create_optimizer_config")?;
            Ok(handle_at(OPTIMIZER_CONFIG))
        }

        fn destroy_optimizer_config(&self, config: OpaqueHandle) -> Result<(), SimulationError> {
            self.release(config);
            self.record("destroy_optimizer_config")
        }

        fn create_optimizer_info(
            &self,
            handle: OpaqueHandle,
            network: OpaqueHandle,
        ) -> Result<OpaqueHandle, SimulationError> {
            assert_eq!(handle, handle_at(SESSION_HANDLE));
            assert_eq!(network, handle_at(NETWORK));
            self.record("create_optimizer_info")?;
            Ok(handle_at(OPTIMIZER_INFO))
        }

        fn destroy_optimizer_info(&self, info: OpaqueHandle) -> Result<(), SimulationError> {
            self.release(info);
            self.record("destroy_optimizer_info")
        }

        fn create_slice_group_from_id_range(
            &self,
            handle: OpaqueHandle,
            start: i64,
            stop: i64,
            increment: i64,
        ) -> Result<OpaqueHandle, SimulationError> {
            assert_eq!(handle, handle_at(SESSION_HANDLE));
            assert_eq!((start, stop, increment), (0, 8, 2));
            self.record("create_slice_group_from_id_range")?;
            Ok(handle_at(SLICE_GROUP))
        }

        fn destroy_slice_group(&self, slice_group: OpaqueHandle) -> Result<(), SimulationError> {
            self.release(slice_group);
            self.record("destroy_slice_group")
        }
    }

    #[test]
    fn contraction_resources_are_created_and_released_in_dependency_order() {
        let api = Arc::new(FakeApi::new(None));
        let resources = ContractionResources::new(api.clone(), handle_at(SESSION_HANDLE))
            .expect("contraction resources should be created");

        assert_eq!(resources.handle(), handle_at(SESSION_HANDLE));
        assert_eq!(resources.network(), handle_at(NETWORK));
        assert_eq!(resources.optimizer_config(), handle_at(OPTIMIZER_CONFIG));
        assert_eq!(resources.optimizer_info(), handle_at(OPTIMIZER_INFO));
        drop(resources);

        assert_eq!(
            api.events(),
            [
                "create_network",
                "create_optimizer_config",
                "create_optimizer_info",
                "destroy_optimizer_info",
                "destroy_optimizer_config",
                "destroy_network",
            ]
        );
        assert_eq!(
            api.released(),
            [OPTIMIZER_INFO, OPTIMIZER_CONFIG, NETWORK],
            "each native object must be released exactly once"
        );
    }

    #[test]
    fn failed_optimizer_config_creation_releases_the_network() {
        let api = Arc::new(FakeApi::new(Some("create_optimizer_config")));
        let error = ContractionResources::new(api.clone(), handle_at(SESSION_HANDLE))
            .err()
            .expect("configuration failure should abort construction");

        assert!(matches!(
            error,
            SimulationError::NativeCallFailed {
                operation: "create_optimizer_config",
                ..
            }
        ));
        assert_eq!(
            api.events(),
            [
                "create_network",
                "create_optimizer_config",
                "destroy_network",
            ]
        );
        assert_eq!(api.released(), [NETWORK]);
    }

    #[test]
    fn failed_optimizer_info_creation_releases_the_config_and_network() {
        let api = Arc::new(FakeApi::new(Some("create_optimizer_info")));
        let error = ContractionResources::new(api.clone(), handle_at(SESSION_HANDLE))
            .err()
            .expect("optimizer info failure should abort construction");

        assert!(matches!(
            error,
            SimulationError::NativeCallFailed {
                operation: "create_optimizer_info",
                ..
            }
        ));
        assert_eq!(
            api.events(),
            [
                "create_network",
                "create_optimizer_config",
                "create_optimizer_info",
                "destroy_optimizer_config",
                "destroy_network",
            ]
        );
        assert_eq!(api.released(), [OPTIMIZER_CONFIG, NETWORK]);
    }

    #[test]
    fn cleanup_reports_the_first_failure_and_still_releases_every_object() {
        let api = Arc::new(FakeApi::new(Some("destroy_optimizer_info")));
        let mut resources = ContractionResources::new(api.clone(), handle_at(SESSION_HANDLE))
            .expect("contraction resources should be created");
        let error = resources
            .close()
            .expect_err("the first cleanup failure should be reported");

        assert!(matches!(
            error,
            SimulationError::NativeCallFailed {
                operation: "destroy_optimizer_info",
                ..
            }
        ));
        assert_eq!(
            api.released(),
            [OPTIMIZER_INFO, OPTIMIZER_CONFIG, NETWORK],
            "a failed release must not strand the remaining objects"
        );
    }

    #[test]
    fn closing_twice_does_not_release_any_object_again() {
        let api = Arc::new(FakeApi::new(None));
        let mut resources = ContractionResources::new(api.clone(), handle_at(SESSION_HANDLE))
            .expect("contraction resources should be created");

        resources.close().expect("the first close should succeed");
        resources.close().expect("the second close should be inert");
        drop(resources);

        assert_eq!(api.released(), [OPTIMIZER_INFO, OPTIMIZER_CONFIG, NETWORK]);
    }

    #[test]
    fn slice_group_is_released_on_drop() {
        let api = Arc::new(FakeApi::new(None));
        let slice_group =
            SliceGroup::from_id_range(api.clone(), handle_at(SESSION_HANDLE), 0, 8, 2)
                .expect("slice group should be created");

        assert_eq!(slice_group.as_handle(), handle_at(SLICE_GROUP));
        drop(slice_group);

        assert_eq!(
            api.events(),
            ["create_slice_group_from_id_range", "destroy_slice_group"]
        );
        assert_eq!(api.released(), [SLICE_GROUP]);
    }

    #[test]
    fn slice_group_rejects_a_zero_step_before_reaching_the_native_api() {
        let api = Arc::new(FakeApi::new(None));
        let error = SliceGroup::from_id_range(api.clone(), handle_at(SESSION_HANDLE), 0, 8, 0)
            .err()
            .expect("a zero increment should be rejected");

        assert!(matches!(
            error,
            SimulationError::InvalidContractionConfiguration {
                reason: "slice identifier increment must be non-zero",
            }
        ));
        assert!(
            api.events().is_empty(),
            "validation must happen before any native call"
        );
    }
}
