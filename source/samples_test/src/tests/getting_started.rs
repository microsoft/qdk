// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};

// Each file in the samples/getting_started folder is compiled and run as two tests and should
// have matching expect strings in this file. If new samples are added, this file will
// fail to compile until the new expect strings are added.
pub const BELLPAIR_EXPECT: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    (Zero, Zero)"#]];
pub const BELLPAIR_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    (Zero, Zero)"#]];
pub const BELLSTATES_EXPECT: Expect = expect![[r#"
    Bell state |Φ+〉:
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    Bell state |Φ-〉:
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: −0.7071+0.0000𝑖
    Bell state |Ψ+〉:
    STATE:
    |01⟩: 0.7071+0.0000𝑖
    |10⟩: 0.7071+0.0000𝑖
    Bell state |Ψ-〉:
    STATE:
    |01⟩: 0.7071+0.0000𝑖
    |10⟩: −0.7071+0.0000𝑖
    [(Zero, Zero), (One, One), (One, Zero), (One, Zero)]"#]];
pub const BELLSTATES_EXPECT_DEBUG: Expect = expect![[r#"
    Bell state |Φ+〉:
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    Bell state |Φ-〉:
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: −0.7071+0.0000𝑖
    Bell state |Ψ+〉:
    STATE:
    |01⟩: 0.7071+0.0000𝑖
    |10⟩: 0.7071+0.0000𝑖
    Bell state |Ψ-〉:
    STATE:
    |01⟩: 0.7071+0.0000𝑖
    |10⟩: −0.7071+0.0000𝑖
    [(Zero, Zero), (One, One), (One, Zero), (One, Zero)]"#]];
pub const CATSTATES_EXPECT: Expect = expect![[r#"
    STATE:
    |00000⟩: 0.7071+0.0000𝑖
    |11111⟩: 0.7071+0.0000𝑖
    [Zero, Zero, Zero, Zero, Zero]"#]];
pub const CATSTATES_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |00000⟩: 0.7071+0.0000𝑖
    |11111⟩: 0.7071+0.0000𝑖
    [Zero, Zero, Zero, Zero, Zero]"#]];
pub const RANDOMBITS_EXPECT: Expect = expect!["[Zero, Zero, One, One, One]"];
pub const RANDOMBITS_EXPECT_DEBUG: Expect = expect!["[Zero, Zero, One, One, One]"];
pub const SIMPLETELEPORTATION_EXPECT: Expect = expect![[r#"
    Teleportation successful: true.
    true"#]];
pub const SIMPLETELEPORTATION_EXPECT_DEBUG: Expect = expect![[r#"
    Teleportation successful: true.
    true"#]];
pub const ENTANGLEMENT_EXPECT: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    [Zero, Zero]"#]];
pub const ENTANGLEMENT_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    [Zero, Zero]"#]];
pub const JOINTMEASUREMENT_EXPECT: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    STATE:
    |11⟩: 1.0000+0.0000𝑖
    STATE:
    |11⟩: 1.0000+0.0000𝑖
    (Zero, [One, One])"#]];
pub const JOINTMEASUREMENT_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    STATE:
    |00⟩: 0.7071+0.0000𝑖
    |11⟩: 0.7071+0.0000𝑖
    STATE:
    |11⟩: 1.0000+0.0000𝑖
    STATE:
    |11⟩: 1.0000+0.0000𝑖
    (Zero, [One, One])"#]];
pub const MEASUREMENT_EXPECT: Expect = expect!["(One, [Zero, Zero])"];
pub const MEASUREMENT_EXPECT_DEBUG: Expect = expect!["(One, [Zero, Zero])"];
pub const QUANTUMHELLOWORLD_EXPECT: Expect = expect![[r#"
    Hello world!
    Zero"#]];
pub const QUANTUMHELLOWORLD_EXPECT_DEBUG: Expect = expect![[r#"
    Hello world!
    Zero"#]];
pub const SUPERPOSITION_EXPECT: Expect = expect![[r#"
    STATE:
    |0⟩: 0.7071+0.0000𝑖
    |1⟩: 0.7071+0.0000𝑖
    Zero"#]];
pub const SUPERPOSITION_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |0⟩: 0.7071+0.0000𝑖
    |1⟩: 0.7071+0.0000𝑖
    Zero"#]];
