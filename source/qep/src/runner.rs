// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod batcher;
mod qir_runner;
pub use batcher::DEFAULT_MAX_BATCH_SIZE;
pub use qir_runner::{OutputRecord, OutputValue, RunOutput, run_qir, run_qir_with_batch_size};
