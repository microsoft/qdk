// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Synthesized-name constants for the `return_unify` pass.
//!
//! Centralizes magic strings that appear in multiple places across
//! normalize, transform, and simplify phases.
//!
//! `HAS_RETURNED`, `RET_VAL`, and `TRAILING_RESULT` are FIR-dump-only labels:
//! they name the synthesized `Ident.name` strings purely so emitted FIR reads
//! clearly. They must not be used for match/branch logic. Cleanup phases
//! identify these synthesized locals by `LocalVarId` identity (carried in
//! `SynthSlots`), never by comparing against these name strings.

/// FIR-dump-only label for the synthesized mutable boolean flag indicating
/// whether a return has been executed.
///
/// The in-memory `Ident.name` carries a `.` sentinel (`_.has_returned`), which
/// is never a valid Q# identifier character, so name-based lookups cannot
/// collide with user code. The Parseable render (`render_ident`) maps `.` back
/// to `_`, restoring the original `__has_returned` spelling in Parseable
/// snapshots.
///
/// Must not be used for match/branch logic — use `LocalVarId` identity via
/// `SynthSlots` instead.
pub(crate) const HAS_RETURNED: &str = "_.has_returned";

/// FIR-dump-only label for the synthesized mutable slot holding the return
/// value.
///
/// The in-memory `Ident.name` carries a `.` sentinel (`_.ret_val`); the
/// Parseable render restores the original `__ret_val` spelling.
///
/// Must not be used for match/branch logic — use `LocalVarId` identity via
/// `SynthSlots` instead.
pub(crate) const RET_VAL: &str = "_.ret_val";

/// FIR-dump-only label for the synthesized trailing result variable used for
/// block-tail synthesis.
///
/// The in-memory `Ident.name` carries a `.` sentinel (`_.trailing_result`); the
/// Parseable render restores the original `__trailing_result` spelling.
///
/// Must not be used for match/branch logic — use `LocalVarId` identity via
/// `SynthSlots` instead.
pub(super) const TRAILING_RESULT: &str = "_.trailing_result";

/// The temporary variable used during normalize hoist operations.
///
/// The in-memory `Ident.name` carries a `.` sentinel (`_.ret_hoist`); the
/// Parseable render restores the original `__ret_hoist` spelling.
pub(super) const RET_HOIST: &str = "_.ret_hoist";

/// FIR-dump-only label for the synthesized immutable temp that an
/// operand-position subexpression containing a `Return` is ANF-lifted into
/// (and for the eval-order pins of its earlier sibling operands).
///
/// The in-memory `Ident.name` carries a `.` sentinel (`_.operand_tmp`); the
/// Parseable render restores the original `__operand_tmp` spelling.
///
/// Must not be used for match/branch logic — synthesized temps are
/// identified by `LocalVarId` identity, never by name.
pub(super) const OPERAND_TEMP: &str = "_.operand_tmp";
