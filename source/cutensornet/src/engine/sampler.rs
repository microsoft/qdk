//! The cuTensorNet sampler over a computed MPS.
//!
//! [`SamplingRequest`] validates shots, hyper-samples and seeds before any
//! native call. `PreparedSampler` owns one native sampler: created against a
//! computed state, configured and prepared within the policy's workspace
//! budget, and destroyed by `close` on every path, with Drop as a
//! non-panicking fallback. [`SamplerApi`] is the injection seam that
//! `library::engine` implements with cuTensorNet and host tests with test
//! doubles. It returns the native shot-major rows; the engine checks that
//! each is a bit.

use super::{EngineError, OpaqueHandle, Stream};

pub(crate) trait SamplerApi {
    fn create_sampler(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        modes_to_sample: &[i32],
    ) -> Result<OpaqueHandle, EngineError>;
    fn destroy_sampler(&self, sampler: OpaqueHandle) -> Result<(), EngineError>;
    fn configure_sampler_hyper_samples(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), EngineError>;
    fn configure_sampler_path_seed(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        seed: i32,
    ) -> Result<(), EngineError>;
    fn prepare_sampler(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), EngineError>;
    fn configure_sampler_sample_seed(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        seed: i32,
    ) -> Result<(), EngineError>;
    fn sample(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        shots: i64,
        workspace: OpaqueHandle,
        output: &mut [i64],
        stream: Stream,
    ) -> Result<(), EngineError>;
}

fn require_positive<T>(value: T, reason: &'static str) -> Result<(), EngineError>
where
    T: Copy + PartialOrd + From<u8>,
{
    if value > T::from(0) {
        Ok(())
    } else {
        Err(EngineError::InvalidSamplerConfiguration { reason })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SamplingRequest {
    shots: usize,
    hyper_samples: i32,
    path_seed: Option<i32>,
    sample_seed: i32,
}

impl SamplingRequest {
    pub(crate) fn new(
        shots: usize,
        hyper_samples: i32,
        path_seed: Option<i32>,
        sample_seed: i32,
    ) -> Result<Self, EngineError> {
        require_positive(shots, "sampler shot count must be positive")?;
        require_positive(hyper_samples, "sampler hyper-samples must be positive")?;
        if let Some(seed) = path_seed {
            require_positive(seed, "sampler pathfinding seed must be positive")?;
        }
        require_positive(sample_seed, "sampler sample seed must be positive")?;
        Ok(Self {
            shots,
            hyper_samples,
            path_seed,
            sample_seed,
        })
    }
}

pub(super) struct PreparedSampler<'api, Api: SamplerApi + ?Sized> {
    api: &'api Api,
    handle: OpaqueHandle,
    sampler: Option<OpaqueHandle>,
    mode_count: usize,
}

#[derive(Clone, Copy)]
pub(super) struct SamplerContext {
    pub(super) handle: OpaqueHandle,
    pub(super) state: OpaqueHandle,
    pub(super) workspace: OpaqueHandle,
    pub(super) stream: Stream,
    pub(super) maximum_workspace_bytes: usize,
}

impl<'api, Api: SamplerApi + ?Sized> PreparedSampler<'api, Api> {
    pub(super) fn new(
        api: &'api Api,
        context: SamplerContext,
        modes_to_sample: &[i32],
        request: &SamplingRequest,
    ) -> Result<Self, EngineError> {
        let sampler = api.create_sampler(context.handle, context.state, modes_to_sample)?;
        let mut prepared = Self {
            api,
            handle: context.handle,
            sampler: Some(sampler),
            mode_count: modes_to_sample.len(),
        };
        let preparation = (|| {
            api.configure_sampler_hyper_samples(context.handle, sampler, request.hyper_samples)?;
            if let Some(seed) = request.path_seed {
                api.configure_sampler_path_seed(context.handle, sampler, seed)?;
            }
            api.prepare_sampler(
                context.handle,
                sampler,
                context.maximum_workspace_bytes,
                context.workspace,
                context.stream,
            )
        })();
        if let Err(execution) = preparation {
            return match prepared.close() {
                Ok(()) => Err(execution),
                Err(cleanup) => Err(EngineError::ExecutionAndCleanupFailed {
                    execution: Box::new(execution),
                    cleanup: Box::new(cleanup),
                }),
            };
        }
        Ok(prepared)
    }

    pub(super) fn sample(
        &self,
        request: &SamplingRequest,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<Box<[i64]>, EngineError> {
        let native_shots =
            i64::try_from(request.shots).map_err(|_| EngineError::ResourceSizeOverflow {
                resource: "sampler shot count",
            })?;
        let sample_count = request.shots.checked_mul(self.mode_count).ok_or(
            EngineError::ResourceSizeOverflow {
                resource: "sampler output buffer",
            },
        )?;
        let mut output = vec![0_i64; sample_count];
        let sampler = self
            .sampler
            .expect("a prepared sampler remains live until close");
        self.api
            .configure_sampler_sample_seed(self.handle, sampler, request.sample_seed)?;
        self.api.sample(
            self.handle,
            sampler,
            native_shots,
            workspace,
            &mut output,
            stream,
        )?;
        Ok(output.into_boxed_slice())
    }

    pub(super) fn close(&mut self) -> Result<(), EngineError> {
        self.sampler
            .take()
            .map_or(Ok(()), |sampler| self.api.destroy_sampler(sampler))
    }
}

impl<Api: SamplerApi + ?Sized> Drop for PreparedSampler<'_, Api> {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EngineError, OpaqueHandle, PreparedSampler, SamplerApi, SamplerContext, SamplingRequest,
        Stream, require_positive,
    };
    use std::{ptr::NonNull, sync::Mutex};

    struct TestDoubleSamplerApi {
        events: Mutex<Vec<&'static str>>,
        failure: Option<&'static str>,
    }

    impl TestDoubleSamplerApi {
        fn events(&self) -> Vec<&'static str> {
            self.events
                .lock()
                .expect("event lock should succeed")
                .clone()
        }

        fn record(&self, event: &'static str) -> Result<(), EngineError> {
            self.events
                .lock()
                .expect("event lock should succeed")
                .push(event);
            if self.failure == Some(event) {
                Err(EngineError::NativeCallFailed {
                    component: "test-double sampler API",
                    operation: event,
                    status: 17,
                    message: "injected failure".to_string(),
                })
            } else {
                Ok(())
            }
        }
    }

    impl SamplerApi for TestDoubleSamplerApi {
        fn create_sampler(
            &self,
            _handle: OpaqueHandle,
            _state: OpaqueHandle,
            modes_to_sample: &[i32],
        ) -> Result<OpaqueHandle, EngineError> {
            assert_eq!(modes_to_sample, [0, 1]);
            self.record("create_sampler")?;
            Ok(NonNull::dangling())
        }

        fn destroy_sampler(&self, _sampler: OpaqueHandle) -> Result<(), EngineError> {
            self.record("destroy_sampler")
        }

        fn configure_sampler_hyper_samples(
            &self,
            _handle: OpaqueHandle,
            _sampler: OpaqueHandle,
            hyper_samples: i32,
        ) -> Result<(), EngineError> {
            assert_eq!(hyper_samples, 8);
            self.record("configure_hyper_samples")
        }

        fn configure_sampler_path_seed(
            &self,
            _handle: OpaqueHandle,
            _sampler: OpaqueHandle,
            seed: i32,
        ) -> Result<(), EngineError> {
            assert_eq!(seed, 11);
            self.record("configure_path_seed")
        }

        fn prepare_sampler(
            &self,
            _handle: OpaqueHandle,
            _sampler: OpaqueHandle,
            maximum_workspace_bytes: usize,
            _workspace: OpaqueHandle,
            _stream: Stream,
        ) -> Result<(), EngineError> {
            assert_eq!(maximum_workspace_bytes, 1024);
            self.record("prepare_sampler")
        }

        fn configure_sampler_sample_seed(
            &self,
            _handle: OpaqueHandle,
            _sampler: OpaqueHandle,
            seed: i32,
        ) -> Result<(), EngineError> {
            assert_eq!(seed, 29);
            self.record("configure_sample_seed")
        }

        fn sample(
            &self,
            _handle: OpaqueHandle,
            _sampler: OpaqueHandle,
            shots: i64,
            _workspace: OpaqueHandle,
            output: &mut [i64],
            _stream: Stream,
        ) -> Result<(), EngineError> {
            assert_eq!(shots, 3);
            assert_eq!(output.len(), 6);
            self.record("sample")?;
            output.copy_from_slice(&[0, 1, 1, 0, 1, 1]);
            Ok(())
        }
    }

    #[test]
    fn sampler_preserves_configuration_order_and_shot_major_output() {
        let api = TestDoubleSamplerApi {
            events: Mutex::new(Vec::new()),
            failure: None,
        };
        let handle = NonNull::dangling();
        let state = NonNull::dangling();
        let workspace = NonNull::dangling();
        let stream = NonNull::dangling();
        let request = SamplingRequest::new(3, 8, Some(11), 29).expect("request should be valid");
        let mut prepared = PreparedSampler::new(
            &api,
            SamplerContext {
                handle,
                state,
                workspace,
                stream,
                maximum_workspace_bytes: 1024,
            },
            &[0, 1],
            &request,
        )
        .expect("sampler preparation should succeed");
        let output = prepared
            .sample(&request, workspace, stream)
            .expect("sampling should succeed");
        assert_eq!(*output, [0, 1, 1, 0, 1, 1]);
        prepared.close().expect("sampler cleanup should succeed");
        let events = api.events();
        assert_eq!(events.iter().filter(|event| **event == "sample").count(), 1);
        assert_eq!(
            events,
            [
                "create_sampler",
                "configure_hyper_samples",
                "configure_path_seed",
                "prepare_sampler",
                "configure_sample_seed",
                "sample",
                "destroy_sampler",
            ]
        );
    }

    #[test]
    fn sampler_counts_and_seeds_must_be_positive() {
        assert!(require_positive(1_i32, "positive").is_ok());
        assert!(require_positive(1_i64, "positive").is_ok());
        assert!(require_positive(0_i32, "zero").is_err());
        assert!(require_positive(-1_i64, "negative").is_err());
        assert!(SamplingRequest::new(0, 8, Some(11), 29).is_err());
        assert!(SamplingRequest::new(1, 0, Some(11), 29).is_err());
        assert!(SamplingRequest::new(1, 8, Some(0), 29).is_err());
        assert!(SamplingRequest::new(1, 8, None, 0).is_err());
    }

    #[test]
    fn preparation_failure_destroys_the_created_sampler() {
        let api = TestDoubleSamplerApi {
            events: Mutex::new(Vec::new()),
            failure: Some("prepare_sampler"),
        };
        let error = PreparedSampler::new(
            &api,
            SamplerContext {
                handle: NonNull::dangling(),
                state: NonNull::dangling(),
                workspace: NonNull::dangling(),
                stream: NonNull::dangling(),
                maximum_workspace_bytes: 1024,
            },
            &[0, 1],
            &SamplingRequest::new(3, 8, Some(11), 29).expect("request should be valid"),
        )
        .err()
        .expect("injected preparation failure should propagate");
        assert!(error.to_string().contains("prepare_sampler"));
        assert_eq!(
            api.events(),
            [
                "create_sampler",
                "configure_hyper_samples",
                "configure_path_seed",
                "prepare_sampler",
                "destroy_sampler",
            ]
        );
    }
}
