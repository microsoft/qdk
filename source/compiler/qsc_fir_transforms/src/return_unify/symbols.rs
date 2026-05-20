// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Synthesized-name constants for the `return_unify` pass.
//!
//! Centralizes magic strings that appear in multiple places across
//! normalize, transform, and simplify phases.

/// The mutable boolean flag indicating whether a return has been executed.
pub(super) const HAS_RETURNED: &str = "__has_returned";

/// The mutable slot holding the return value.
pub(super) const RET_VAL: &str = "__ret_val";

/// The trailing result variable used for block-tail synthesis.
pub(super) const TRAILING_RESULT: &str = "__trailing_result";

/// The temporary variable used during normalize hoist operations.
pub(super) const RET_HOIST: &str = "__ret_hoist";
