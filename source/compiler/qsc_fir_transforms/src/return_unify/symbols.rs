// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Synthesized-name constants for the `return_unify` pass.
//!
//! Centralizes magic strings that appear in multiple places across
//! normalize, transform, and simplify phases.
//!
//! `HAS_RETURNED`, `RET_VAL`, and `TRAILING_RESULT` are FIR-dump-only labels:
//! they name the synthesized `Ident.name` strings purely so emitted FIR reads
//! clearly. They MUST NOT be used for match/branch logic. Cleanup phases
//! identify these synthesized locals by `LocalVarId` identity (carried in
//! `SynthSlots`), never by comparing against these name strings.

/// FIR-dump-only label for the synthesized mutable boolean flag indicating
/// whether a return has been executed.
///
/// MUST NOT be used for match/branch logic — use `LocalVarId` identity via
/// `SynthSlots` instead.
pub(super) const HAS_RETURNED: &str = "__has_returned";

/// FIR-dump-only label for the synthesized mutable slot holding the return
/// value.
///
/// MUST NOT be used for match/branch logic — use `LocalVarId` identity via
/// `SynthSlots` instead.
pub(super) const RET_VAL: &str = "__ret_val";

/// FIR-dump-only label for the synthesized trailing result variable used for
/// block-tail synthesis.
///
/// MUST NOT be used for match/branch logic — use `LocalVarId` identity via
/// `SynthSlots` instead.
pub(super) const TRAILING_RESULT: &str = "__trailing_result";

/// The temporary variable used during normalize hoist operations.
pub(super) const RET_HOIST: &str = "__ret_hoist";
