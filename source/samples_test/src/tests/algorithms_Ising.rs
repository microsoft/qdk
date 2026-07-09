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
pub const SIMPLE2DISINGORDER2_EXPECT_QIR: Expect =
    expect!["QIR generation error for `Simple2dIsingOrder2.Main()`: partial evaluation error"];
pub const SIMPLE1DISINGORDER1_EXPECT: Expect =
    expect!["[Zero, Zero, Zero, One, One, Zero, Zero, Zero, Zero]"];
pub const SIMPLE1DISINGORDER1_EXPECT_DEBUG: Expect =
    expect!["[Zero, Zero, Zero, One, One, Zero, Zero, Zero, Zero]"];
pub const SIMPLE1DISINGORDER1_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 12317"];
pub const SIMPLE1DISINGORDER1_EXPECT_QIR: Expect = expect!["generated QIR of length 6539"];
pub const SIMPLE2DISINGORDER1_EXPECT: Expect =
    expect!["[Zero, Zero, Zero, One, One, Zero, One, One, Zero]"];
pub const SIMPLE2DISINGORDER1_EXPECT_DEBUG: Expect =
    expect!["[Zero, Zero, Zero, One, One, Zero, One, One, Zero]"];
pub const SIMPLE2DISINGORDER1_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 24085"];
pub const SIMPLE2DISINGORDER1_EXPECT_QIR: Expect =
    expect!["QIR generation error for `Simple2dIsingOrder1.Main()`: partial evaluation error"];
