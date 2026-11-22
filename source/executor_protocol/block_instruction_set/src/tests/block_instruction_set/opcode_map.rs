// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The map a sender receives from a fit: each of its opcodes to the
//! receiver's, and the matches resting on a mnemonic alone.

use super::fit::{gate, hadamard, set_of};
use executor_protocol_core::BoundaryCompatible;

/// What a match by mnemonic rests on is reported; a match by meaning is
/// checked, so not listed.
#[test]
fn matches_by_mnemonic_alone_are_listed() {
    let sent = set_of(
        "Gates",
        1,
        vec![gate("h").meaning(hadamard()), gate("y"), gate("z")],
    );
    let accepted = set_of(
        "Gates",
        1,
        vec![gate("z"), gate("y"), gate("h").meaning(hadamard())],
    );
    let map = sent.fits(&accepted).expect("one set");
    assert_eq!(map.by_mnemonic(), ["y", "z"]);
}

/// A sender maps only its own opcodes: one beyond its set's last
/// instruction, here an opcode of a larger set, panics.
#[test]
#[should_panic]
#[expect(
    clippy::should_panic_without_expect,
    reason = "`OpcodeMap::map` promises a panic, not its message"
)]
fn mapping_an_opcode_beyond_the_sending_set_panics() {
    let sent = set_of("Gates", 1, vec![gate("h")]);
    let larger = set_of("Gates", 1, vec![gate("h"), gate("x")]);
    let map = sent.fits(&larger).expect("`h` fits");
    let beyond = larger.opcode("x").expect("`x` is in the larger set");
    let _ = map.map(beyond);
}
