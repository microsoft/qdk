// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

pub const QSHARP_LIBRARY_URI_SCHEME: &str = "qsharp-library-source";

// The core prefix on the name is needed to disambiguate from the std
// files of the same name. This comes in during debugging when we need
// to load a core/std file from the library.
pub const CORE_LIB: &[(&str, &str)] = &[
    (
        "qsharp-library-source:core/core.qs",
        include_str!("../core/core.qs"),
    ),
    (
        "qsharp-library-source:core/qir.qs",
        include_str!("../core/qir.qs"),
    ),
];

pub const STD_LIB: &[(&str, &str)] = &[
    (
        "qsharp-library-source:Std/Arrays.qs",
        include_str!("../std/src/Std/Arrays.qs"),
    ),
    (
        "qsharp-library-source:Std/Canon.qs",
        include_str!("../std/src/Std/Canon.qs"),
    ),
    (
        "qsharp-library-source:Std/Convert.qs",
        include_str!("../std/src/Std/Convert.qs"),
    ),
    (
        "qsharp-library-source:Std/Range.qs",
        include_str!("../std/src/Std/Range.qs"),
    ),
    (
        "qsharp-library-source:Std/Diagnostics.qs",
        include_str!("../std/src/Std/Diagnostics.qs"),
    ),
    (
        "qsharp-library-source:Std/InternalHelpers.qs",
        include_str!("../std/src/Std/InternalHelpers.qs"),
    ),
    (
        "qsharp-library-source:Std/Intrinsic.qs",
        include_str!("../std/src/Std/Intrinsic.qs"),
    ),
    (
        "qsharp-library-source:Std/Logical.qs",
        include_str!("../std/src/Std/Logical.qs"),
    ),
    (
        "qsharp-library-source:Std/Math.qs",
        include_str!("../std/src/Std/Math.qs"),
    ),
    (
        "qsharp-library-source:Std/Measurement.qs",
        include_str!("../std/src/Std/Measurement.qs"),
    ),
    (
        "qsharp-library-source:QIR/Intrinsic.qs",
        include_str!("../std/src/QIR/Intrinsic.qs"),
    ),
    (
        "qsharp-library-source:Std/Random.qs",
        include_str!("../std/src/Std/Random.qs"),
    ),
    (
        "qsharp-library-source:Std/ResourceEstimation.qs",
        include_str!("../std/src/Std/ResourceEstimation.qs"),
    ),
    (
        "qsharp-library-source:Std/Arithmetic.qs",
        include_str!("../std/src/Std/Arithmetic.qs"),
    ),
    (
        "qsharp-library-source:Std/ArithmeticUtils.qs",
        include_str!("../std/src/Std/ArithmeticUtils.qs"),
    ),
    (
        "qsharp-library-source:Std/TestUtils.qs",
        include_str!("../std/src/Std/TestUtils.qs"),
    ),
    (
        "qsharp-library-source:Std/ClassicalArithmetic.qs",
        include_str!("../std/src/Std/ClassicalArithmetic.qs"),
    ),
    (
        "qsharp-library-source:Std/StatePreparation.qs",
        include_str!("../std/src/Std/StatePreparation.qs"),
    ),
    (
        "qsharp-library-source:Std/TableLookup.qs",
        include_str!("../std/src/Std/TableLookup.qs"),
    ),
    (
        "qsharp-library-source:Std/legacy_api.qs",
        include_str!("../std/src/legacy_api.qs"),
    ),
    (
        "qsharp-library-source:Std/OpenQASM/Angle.qs",
        include_str!("../std/src/Std/OpenQASM/Angle.qs"),
    ),
    (
        "qsharp-library-source:Std/OpenQASM/Builtin.qs",
        include_str!("../std/src/Std/OpenQASM/Builtin.qs"),
    ),
    (
        "qsharp-library-source:Std/OpenQASM/Convert.qs",
        include_str!("../std/src/Std/OpenQASM/Convert.qs"),
    ),
    (
        "qsharp-library-source:Std/OpenQASM/Intrinsic.qs",
        include_str!("../std/src/Std/OpenQASM/Intrinsic.qs"),
    ),
];
