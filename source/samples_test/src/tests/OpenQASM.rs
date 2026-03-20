// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};

// Each file in the samples/OpenQASM folder is compiled and run as two tests and should
// have matching expect strings in this file. If new samples are added, this file will
// fail to compile until the new expect strings are added.
pub const BELLPAIR_EXPECT: Expect = expect!["(One, One)"];
pub const BELLPAIR_EXPECT_DEBUG: Expect = expect!["(One, One)"];
pub const BELLPAIR_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 412"];
pub const BELLPAIR_EXPECT_QIR: Expect = expect!["generated QIR of length 2214"];
pub const OPENQASMHELLOWORLD_EXPECT: Expect = expect!["Zero"];
pub const OPENQASMHELLOWORLD_EXPECT_DEBUG: Expect = expect!["Zero"];
pub const OPENQASMHELLOWORLD_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 164"];
pub const OPENQASMHELLOWORLD_EXPECT_QIR: Expect = expect!["generated QIR of length 1306"];
pub const BERNSTEINVAZIRANI_EXPECT: Expect = expect!["[One, Zero, One, Zero, One]"];
pub const BERNSTEINVAZIRANI_EXPECT_DEBUG: Expect = expect!["[One, Zero, One, Zero, One]"];
pub const BERNSTEINVAZIRANI_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 4341"];
pub const BERNSTEINVAZIRANI_EXPECT_QIR: Expect = expect!["generated QIR of length 4548"];
pub const GROVER_EXPECT: Expect = expect!["[Zero, One, Zero, One, Zero]"];
pub const GROVER_EXPECT_DEBUG: Expect = expect!["[Zero, One, Zero, One, Zero]"];
pub const GROVER_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 33215"];
pub const GROVER_EXPECT_QIR: Expect = expect!["generated QIR of length 20376"];
pub const RANDOMNUMBER_EXPECT: Expect = expect!["9"];
pub const RANDOMNUMBER_EXPECT_DEBUG: Expect = expect!["9"];
pub const RANDOMNUMBER_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 3559"];
pub const RANDOMNUMBER_EXPECT_QIR: Expect = expect!["generated QIR of length 4064"];
pub const SIMPLE1DISINGORDER1_EXPECT: Expect =
    expect!["[Zero, One, One, Zero, Zero, One, One, One, One]"];
pub const SIMPLE1DISINGORDER1_EXPECT_DEBUG: Expect =
    expect!["[Zero, One, One, Zero, Zero, One, One, One, One]"];
pub const SIMPLE1DISINGORDER1_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 12465"];
pub const SIMPLE1DISINGORDER1_EXPECT_QIR: Expect = expect!["generated QIR of length 18979"];
pub const TELEPORTATION_EXPECT: Expect = expect!["Zero"];
pub const TELEPORTATION_EXPECT_DEBUG: Expect = expect!["Zero"];
pub const TELEPORTATION_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 2086"];
pub const TELEPORTATION_EXPECT_QIR: Expect = expect!["generated QIR of length 3062"];
