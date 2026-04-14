// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod llvm;
pub mod qir;

pub use llvm::{LlvmIrError, validate_ir};
pub use qir::{
    Capabilities, DetectedProfile, QirProfileError, QirProfileValidation, validate_qir_profile,
};
