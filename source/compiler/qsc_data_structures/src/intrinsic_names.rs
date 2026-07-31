// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// Intrinsic names that partial evaluation handles as code generation no-ops.
pub const CODEGEN_NOOP_INTRINSIC_NAMES: &[&str] = &[
    "DumpRegister",
    "DumpOperation",
    "AccountForEstimatesInternal",
    "BeginRepeatEstimatesInternal",
    "EndRepeatEstimatesInternal",
    "EnableMemoryComputeArchitecture",
    "Load",
    "Store",
    "ApplyIdleNoise",
    "GlobalPhase",
    "Message",
    "PostSelectZ",
    "Fact",
];

/// Returns whether the intrinsic is handled as a code generation no-op.
#[must_use]
pub fn is_codegen_noop_intrinsic(name: &str) -> bool {
    CODEGEN_NOOP_INTRINSIC_NAMES.contains(&name)
}

/// Returns whether downstream FIR consumers require the intrinsic's literal name.
#[must_use]
pub fn must_preserve_intrinsic_name(name: &str) -> bool {
    name == "Length" || is_codegen_noop_intrinsic(name)
}
