// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};

// Each file in the samples/algorithms/Ising folder is compiled and run as two tests and should
// have matching expect strings in this file. If new samples are added, this file will
// fail to compile until the new expect strings are added.
pub const SIMPLE2DISINGORDER2_EXPECT: Expect =
    expect!["[Zero, Zero, Zero, One, One, Zero, One, Zero, One]"];
pub const SIMPLE2DISINGORDER2_EXPECT_DEBUG: Expect =
    expect!["[Zero, Zero, Zero, One, One, Zero, One, Zero, One]"];
pub const SIMPLE2DISINGORDER2_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 24940"];
pub const SIMPLE2DISINGORDER2_EXPECT_QIR_ADAPTIVE_RIF: Expect =
    expect!["generated QIR of length 20849"];
pub const SIMPLE2DISINGORDER2_EXPECT_QIR_ADAPTIVE: Expect =
    expect!["generated QIR of length 11721"];
pub const SIMPLE1DISINGORDER1_EXPECT: Expect =
    expect!["[Zero, Zero, Zero, One, One, Zero, Zero, Zero, Zero]"];
pub const SIMPLE1DISINGORDER1_EXPECT_DEBUG: Expect =
    expect!["[Zero, Zero, Zero, One, One, Zero, Zero, Zero, Zero]"];
pub const SIMPLE1DISINGORDER1_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 12317"];
pub const SIMPLE1DISINGORDER1_EXPECT_QIR_ADAPTIVE_RIF: Expect =
    expect!["generated QIR of length 18408"];
pub const SIMPLE1DISINGORDER1_EXPECT_QIR_ADAPTIVE: Expect = expect!["generated QIR of length 6539"];
pub const SIMPLE2DISINGORDER1_EXPECT: Expect =
    expect!["[Zero, Zero, Zero, One, One, Zero, One, One, Zero]"];
pub const SIMPLE2DISINGORDER1_EXPECT_DEBUG: Expect =
    expect!["[Zero, Zero, Zero, One, One, Zero, One, One, Zero]"];
pub const SIMPLE2DISINGORDER1_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 24085"];
pub const SIMPLE2DISINGORDER1_EXPECT_QIR_ADAPTIVE_RIF: Expect =
    expect!["generated QIR of length 16214"];
pub const SIMPLE2DISINGORDER1_EXPECT_QIR_ADAPTIVE: Expect =
    expect!["generated QIR of length 11223"];
