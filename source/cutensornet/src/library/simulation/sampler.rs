use super::{OpaqueHandle, SimulationError, Stream};

pub(crate) trait SamplerApi {
    fn create_sampler(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        modes_to_sample: &[i32],
    ) -> Result<OpaqueHandle, SimulationError>;
    fn destroy_sampler(&self, sampler: OpaqueHandle) -> Result<(), SimulationError>;
    fn configure_sampler_hyper_samples(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), SimulationError>;
    fn configure_sampler_path_seed(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        seed: i32,
    ) -> Result<(), SimulationError>;
    fn prepare_sampler(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), SimulationError>;
    fn configure_sampler_sample_seed(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        seed: i32,
    ) -> Result<(), SimulationError>;
    fn sample(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        shots: i64,
        workspace: OpaqueHandle,
        output: &mut [i64],
        stream: Stream,
    ) -> Result<(), SimulationError>;
}

fn require_positive<T>(value: T, reason: &'static str) -> Result<(), SimulationError>
where
    T: Copy + PartialOrd + From<u8>,
{
    if value > T::from(0) {
        Ok(())
    } else {
        Err(SimulationError::InvalidSamplerConfiguration { reason })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SamplingRequest {
    shots: usize,
    hyper_samples: i32,
    path_seed: Option<i32>,
    sample_seed: i32,
}

impl SamplingRequest {
    pub(super) fn new(
        shots: usize,
        hyper_samples: i32,
        path_seed: Option<i32>,
        sample_seed: i32,
    ) -> Result<Self, SimulationError> {
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct FullBitstringSamples {
    width: usize,
    samples: Box<[i64]>,
}

impl FullBitstringSamples {
    pub(super) fn new(width: usize, samples: Box<[i64]>) -> Result<Self, SimulationError> {
        if width == 0 || !samples.len().is_multiple_of(width) {
            return Err(SimulationError::InvalidNativeResult {
                reason: "sampler output shape does not match the sampled modes".to_string(),
            });
        }
        if samples.iter().any(|sample| !matches!(sample, 0 | 1)) {
            return Err(SimulationError::InvalidNativeResult {
                reason: "sampler returned a non-bit value for a qubit mode".to_string(),
            });
        }
        Ok(Self { width, samples })
    }

    #[must_use]
    pub(super) const fn width(&self) -> usize {
        self.width
    }

    #[must_use]
    pub(super) fn shots(&self) -> usize {
        self.samples.len() / self.width
    }

    #[must_use]
    pub(super) fn shot(&self, shot: usize) -> Option<&[i64]> {
        let start = shot.checked_mul(self.width)?;
        self.samples.get(start..start + self.width)
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
    ) -> Result<Self, SimulationError> {
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
                Err(cleanup) => Err(SimulationError::ExecutionAndCleanupFailed {
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
    ) -> Result<Box<[i64]>, SimulationError> {
        let native_shots =
            i64::try_from(request.shots).map_err(|_| SimulationError::ResourceSizeOverflow {
                resource: "sampler shot count",
            })?;
        let sample_count = request.shots.checked_mul(self.mode_count).ok_or(
            SimulationError::ResourceSizeOverflow {
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

    pub(super) fn close(&mut self) -> Result<(), SimulationError> {
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
        FullBitstringSamples, PreparedSampler, SamplerApi, SamplerContext, SamplingRequest,
        require_positive,
    };
    use crate::simulation::{OpaqueHandle, SimulationError, Stream};
    use std::{ptr::NonNull, sync::Mutex};

    struct FakeApi {
        events: Mutex<Vec<&'static str>>,
        failure: Option<&'static str>,
    }

    impl FakeApi {
        fn events(&self) -> Vec<&'static str> {
            self.events
                .lock()
                .expect("event lock should succeed")
                .clone()
        }

        fn record(&self, event: &'static str) -> Result<(), SimulationError> {
            self.events
                .lock()
                .expect("event lock should succeed")
                .push(event);
            if self.failure == Some(event) {
                Err(SimulationError::NativeCallFailed {
                    component: "fake sampler API",
                    operation: event,
                    status: 17,
                    message: "injected failure".to_string(),
                })
            } else {
                Ok(())
            }
        }
    }

    impl SamplerApi for FakeApi {
        fn create_sampler(
            &self,
            _handle: OpaqueHandle,
            _state: OpaqueHandle,
            modes_to_sample: &[i32],
        ) -> Result<OpaqueHandle, SimulationError> {
            assert_eq!(modes_to_sample, [0, 1]);
            self.record("create_sampler")?;
            Ok(NonNull::dangling())
        }

        fn destroy_sampler(&self, _sampler: OpaqueHandle) -> Result<(), SimulationError> {
            self.record("destroy_sampler")
        }

        fn configure_sampler_hyper_samples(
            &self,
            _handle: OpaqueHandle,
            _sampler: OpaqueHandle,
            hyper_samples: i32,
        ) -> Result<(), SimulationError> {
            assert_eq!(hyper_samples, 8);
            self.record("configure_hyper_samples")
        }

        fn configure_sampler_path_seed(
            &self,
            _handle: OpaqueHandle,
            _sampler: OpaqueHandle,
            seed: i32,
        ) -> Result<(), SimulationError> {
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
        ) -> Result<(), SimulationError> {
            assert_eq!(maximum_workspace_bytes, 1024);
            self.record("prepare_sampler")
        }

        fn configure_sampler_sample_seed(
            &self,
            _handle: OpaqueHandle,
            _sampler: OpaqueHandle,
            seed: i32,
        ) -> Result<(), SimulationError> {
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
        ) -> Result<(), SimulationError> {
            assert_eq!(shots, 3);
            assert_eq!(output.len(), 6);
            self.record("sample")?;
            output.copy_from_slice(&[0, 0, 1, 1, 0, 0]);
            Ok(())
        }
    }

    #[test]
    fn sampler_preserves_configuration_order_and_shot_major_output() {
        let api = FakeApi {
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
        let bitstrings =
            FullBitstringSamples::new(2, output).expect("qubit samples should contain only bits");
        assert_eq!(bitstrings.width(), 2);
        assert_eq!(bitstrings.shots(), 3);
        assert_eq!(bitstrings.shot(0), Some([0, 0].as_slice()));
        assert_eq!(bitstrings.shot(1), Some([1, 1].as_slice()));
        assert_eq!(bitstrings.shot(2), Some([0, 0].as_slice()));
        assert_eq!(bitstrings.shot(3), None);
        prepared.close().expect("sampler cleanup should succeed");
        assert_eq!(
            api.events(),
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
        let api = FakeApi {
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

    #[test]
    fn qubit_sample_output_rejects_non_bit_values() {
        let error = FullBitstringSamples::new(2, vec![0, 2].into_boxed_slice())
            .expect_err("non-bit output should be rejected");
        assert!(error.to_string().contains("non-bit value"));
    }
}
