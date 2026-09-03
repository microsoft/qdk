use crate::{
    library::NativeApi,
    simulation::{ExecutionPolicy, OpaqueHandle, SimulationError, Stream},
};
use std::{marker::PhantomData, rc::Rc, sync::Arc};

pub(super) trait SessionApi: Send + Sync {
    fn device_count(&self) -> Result<i32, SimulationError>;
    fn set_device(&self, ordinal: i32) -> Result<(), SimulationError>;
    fn create_stream(&self) -> Result<Stream, SimulationError>;
    fn synchronize_stream(&self, stream: Stream) -> Result<(), SimulationError>;
    fn destroy_stream(&self, stream: Stream) -> Result<(), SimulationError>;
    fn create_handle(&self) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_handle(&self, handle: OpaqueHandle) -> Result<(), SimulationError>;
}

struct SessionResources<Api: SessionApi> {
    api: Arc<Api>,
    device_ordinal: i32,
    stream: Option<Stream>,
    handle: Option<OpaqueHandle>,
}

pub struct Session {
    resources: SessionResources<NativeApi>,
    policy: ExecutionPolicy,
    _thread_marker: PhantomData<Rc<()>>,
}

impl Session {
    pub(crate) fn new(
        api: Arc<NativeApi>,
        policy: ExecutionPolicy,
    ) -> Result<Self, SimulationError> {
        let policy = policy.validate()?;
        Ok(Self {
            resources: SessionResources::new(api, policy.device_ordinal)?,
            policy,
            _thread_marker: PhantomData,
        })
    }

    #[must_use]
    pub fn device_ordinal(&self) -> i32 {
        self.resources.device_ordinal
    }

    pub(crate) fn close(mut self) -> Result<(), SimulationError> {
        self.resources.close()
    }

    pub(crate) fn api(&self) -> &NativeApi {
        self.resources.api.as_ref()
    }

    pub(crate) fn stream(&self) -> Stream {
        self.resources
            .stream
            .expect("a live session always owns its CUDA stream")
    }

    pub(crate) fn handle(&self) -> OpaqueHandle {
        self.resources
            .handle
            .expect("a live session always owns its cuTensorNet handle")
    }

    pub(crate) fn policy(&self) -> ExecutionPolicy {
        self.policy
    }
}

impl<Api: SessionApi> SessionResources<Api> {
    fn new(api: Arc<Api>, device_ordinal: i32) -> Result<Self, SimulationError> {
        let device_count = api.device_count()?;
        if device_count <= 0 {
            return Err(SimulationError::NoDevice);
        }
        if device_ordinal < 0 || device_ordinal >= device_count {
            return Err(SimulationError::InvalidExecutionPolicy {
                reason: "device ordinal is outside the available CUDA devices",
            });
        }
        api.set_device(device_ordinal)?;
        let stream = api.create_stream()?;
        let mut resources = Self {
            api,
            device_ordinal,
            stream: Some(stream),
            handle: None,
        };
        resources.handle = Some(resources.api.create_handle()?);
        Ok(resources)
    }

    fn close(&mut self) -> Result<(), SimulationError> {
        let mut first_error = None;
        if let Some(stream) = self.stream
            && let Err(error) = self.api.synchronize_stream(stream)
        {
            first_error = Some(error);
        }
        if let Some(handle) = self.handle.take()
            && let Err(error) = self.api.destroy_handle(handle)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(stream) = self.stream.take()
            && let Err(error) = self.api.destroy_stream(stream)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl<Api: SessionApi> Drop for SessionResources<Api> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::{OpaqueHandle, SessionApi, SessionResources, SimulationError, Stream};
    use std::{ptr::NonNull, sync::Arc, sync::Mutex};

    struct FakeApi {
        events: Mutex<Vec<&'static str>>,
        device_count: i32,
        fail_handle_creation: bool,
    }

    impl FakeApi {
        fn new(device_count: i32, fail_handle_creation: bool) -> Self {
            Self {
                events: Mutex::new(Vec::new()),
                device_count,
                fail_handle_creation,
            }
        }

        fn record(&self, event: &'static str) {
            self.events
                .lock()
                .expect("event lock should succeed")
                .push(event);
        }

        fn events(&self) -> Vec<&'static str> {
            self.events
                .lock()
                .expect("event lock should succeed")
                .clone()
        }
    }

    impl SessionApi for FakeApi {
        fn device_count(&self) -> Result<i32, SimulationError> {
            self.record("device_count");
            Ok(self.device_count)
        }

        fn set_device(&self, ordinal: i32) -> Result<(), SimulationError> {
            assert_eq!(ordinal, 0);
            self.record("set_device");
            Ok(())
        }

        fn create_stream(&self) -> Result<Stream, SimulationError> {
            self.record("create_stream");
            Ok(NonNull::dangling())
        }

        fn synchronize_stream(&self, _stream: Stream) -> Result<(), SimulationError> {
            self.record("synchronize_stream");
            Ok(())
        }

        fn destroy_stream(&self, _stream: Stream) -> Result<(), SimulationError> {
            self.record("destroy_stream");
            Ok(())
        }

        fn create_handle(&self) -> Result<OpaqueHandle, SimulationError> {
            self.record("create_handle");
            if self.fail_handle_creation {
                Err(SimulationError::NativeCallFailed {
                    component: "cuTensorNet",
                    operation: "cutensornetCreate",
                    status: 13,
                    message: "simulated failure".to_string(),
                })
            } else {
                Ok(NonNull::dangling())
            }
        }

        fn destroy_handle(&self, _handle: OpaqueHandle) -> Result<(), SimulationError> {
            self.record("destroy_handle");
            Ok(())
        }
    }

    #[test]
    fn session_selects_device_and_drops_in_dependency_order() {
        let api = Arc::new(FakeApi::new(1, false));
        let resources =
            SessionResources::new(api.clone(), 0).expect("session resources should be created");
        assert_eq!(resources.device_ordinal, 0);
        drop(resources);

        assert_eq!(
            api.events(),
            [
                "device_count",
                "set_device",
                "create_stream",
                "create_handle",
                "synchronize_stream",
                "destroy_handle",
                "destroy_stream",
            ]
        );
    }

    #[test]
    fn failed_handle_creation_cleans_up_the_stream() {
        let api = Arc::new(FakeApi::new(1, true));
        let error = SessionResources::new(api.clone(), 0)
            .err()
            .expect("handle creation should fail");
        assert!(matches!(
            error,
            SimulationError::NativeCallFailed {
                operation: "cutensornetCreate",
                status: 13,
                ..
            }
        ));
        assert_eq!(
            api.events(),
            [
                "device_count",
                "set_device",
                "create_stream",
                "create_handle",
                "synchronize_stream",
                "destroy_stream",
            ]
        );
    }

    #[test]
    fn session_rejects_a_host_without_a_cuda_device() {
        let api = Arc::new(FakeApi::new(0, false));
        assert!(matches!(
            SessionResources::new(api.clone(), 0),
            Err(SimulationError::NoDevice)
        ));
        assert_eq!(api.events(), ["device_count"]);
    }

    #[test]
    fn session_rejects_an_unavailable_device_ordinal_before_selection() {
        let api = Arc::new(FakeApi::new(1, false));

        assert!(matches!(
            SessionResources::new(api.clone(), 1),
            Err(SimulationError::InvalidExecutionPolicy { .. })
        ));
        assert_eq!(api.events(), ["device_count"]);
    }
}
