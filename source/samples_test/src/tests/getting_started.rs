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
pub const BELLPAIR_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 565"];
pub const BELLPAIR_EXPECT_QIR: Expect = expect!["generated QIR of length 2021"];
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
pub const BELLSTATES_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 5533"];
pub const BELLSTATES_EXPECT_QIR: Expect = expect!["generated QIR of length 5628"];
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
pub const CATSTATES_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 1807"];
pub const CATSTATES_EXPECT_QIR: Expect = expect!["generated QIR of length 3311"];
pub const RANDOMBITS_EXPECT: Expect = expect!["[Zero, Zero, One, One, One]"];
pub const RANDOMBITS_EXPECT_DEBUG: Expect = expect!["[Zero, Zero, One, One, One]"];
pub const RANDOMBITS_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 3486"];
pub const RANDOMBITS_EXPECT_QIR: Expect = expect!["generated QIR of length 3101"];
pub const SIMPLETELEPORTATION_EXPECT: Expect = expect![[r#"
    STATE:
    |000⟩: 1.0000+0.0000𝑖
    Teleportation successful: true.
    true"#]];
pub const SIMPLETELEPORTATION_EXPECT_DEBUG: Expect = expect![[r#"
    STATE:
    |000⟩: 1.0000+0.0000𝑖
    Teleportation successful: true.
    true"#]];
pub const SIMPLETELEPORTATION_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 2123"];
pub const SIMPLETELEPORTATION_EXPECT_QIR: Expect = expect!["generated QIR of length 3118"];
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
pub const ENTANGLEMENT_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 417"];
pub const ENTANGLEMENT_EXPECT_QIR: Expect = expect!["generated QIR of length 2021"];
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
pub const JOINTMEASUREMENT_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 959"];
pub const JOINTMEASUREMENT_EXPECT_QIR: Expect = expect!["generated QIR of length 3259"];
pub const MEASUREMENT_EXPECT: Expect = expect!["(One, [Zero, Zero])"];
pub const MEASUREMENT_EXPECT_DEBUG: Expect = expect!["(One, [Zero, Zero])"];
pub const MEASUREMENT_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 613"];
pub const MEASUREMENT_EXPECT_QIR: Expect = expect!["generated QIR of length 2399"];
pub const QUANTUMHELLOWORLD_EXPECT: Expect = expect![[r#"
    Hello world!
    Zero"#]];
pub const QUANTUMHELLOWORLD_EXPECT_DEBUG: Expect = expect![[r#"
    Hello world!
    Zero"#]];
pub const QUANTUMHELLOWORLD_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 165"];
pub const QUANTUMHELLOWORLD_EXPECT_QIR: Expect = expect!["generated QIR of length 1185"];
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
pub const SUPERPOSITION_EXPECT_CIRCUIT: Expect = expect!["generated circuit of length 187"];
pub const SUPERPOSITION_EXPECT_QIR: Expect = expect!["generated QIR of length 1307"];
