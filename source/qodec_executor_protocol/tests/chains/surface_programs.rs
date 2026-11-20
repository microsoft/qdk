// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Programs of the surface qodec, on its two block types, that prepare a
//! block at a label used before, as both parts' tests run them.

/// A surface program preparing a block at the label of a live block of the
/// same type, which ends it: no two live blocks share the label.
pub const SURFACE_REPREPARED: &str = "prepare_z 0; prepare_x 0; measure_x 0";

/// A surface program preparing a `surface` block 0 once the `merged`
/// block 0 a merge gave has been measured out, each in an `execute` of its
/// own: the label is free again.
pub const SURFACE_LABEL_REUSED: &str =
    "prepare_z 0; prepare_z 1; merge_zz 0 1 | measure_merged_z 0 | prepare_z 0; measure_z 0";
