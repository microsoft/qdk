// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The bench's workload: each qodec it runs, with a program of Qodec's own
//! REPL tests.

/// Each qodec's model, under Qodec's examples, and its program, in Qodec's
/// REPL syntax.
pub const CASES: [(&str, &str); 3] = [
    (
        "steane/steane.qodec.yaml",
        "prepare_z 0; prepare_x 1; idle_ft 0; h 0; cnot 0 1; measure_z 0; measure_x 1",
    ),
    (
        "c4c6/qodec.yaml",
        "prepare_x_all 0; hadamard_all 0; mul_u 0; measure_z_all 0",
    ),
    (
        "surface/surface.qodec.yaml",
        "prepare_z 0; prepare_z 1; idle 0; merge_zz 0 1; measure_merged_z 0",
    ),
];

/// The most instructions per batch the split chain's simulator takes:
/// fewer than most of the layers' batches hold, so that the adapter splits
/// them (the bench checks that it sends more batches).
pub const MOST_BELOW: u32 = 2;
