// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Within a batch a tag names one readout of each shot: `check_unique_tags`
//! finds the first readout that repeats a tag.

use crate::{Rejected, check_unique_tags};

/// The index at which `check_unique_tags` refuses `readouts`, or `None`
/// when it accepts them.
fn refused_at(readouts: &[(usize, u32)]) -> Option<usize> {
    match check_unique_tags(readouts.iter().copied()) {
        Ok(()) => None,
        Err(Rejected::Instruction { at, .. }) => Some(at),
        Err(other) => panic!("a repeated tag is refused as `Instruction`, not `{other:?}`"),
    }
}

/// A program measures qubit 0 under tag 4, then measures it again under
/// tag 4 in the same batch: the second readout is refused, at its index.
#[test]
fn a_readout_that_repeats_a_tag_is_refused_at_its_index() {
    assert_eq!(refused_at(&[(0, 4), (1, 4)]), Some(1));
}

/// A tag repeated after other readouts is found too, not only next to its
/// first use.
#[test]
fn a_tag_repeated_after_other_readouts_is_refused() {
    assert_eq!(refused_at(&[(1, 7), (2, 3), (4, 5), (6, 7)]), Some(6));
}

/// Two tags each repeat: the refusal names the readout that repeats a tag
/// first in the batch (tag 9 at 3), not the smaller tag (tag 1 at 5).
#[test]
fn the_first_repeat_in_the_batchs_order_is_refused() {
    assert_eq!(refused_at(&[(0, 1), (1, 9), (3, 9), (5, 1)]), Some(3));
}

/// Readouts under distinct tags are accepted, in any order.
#[test]
fn distinct_tags_are_accepted() {
    assert_eq!(refused_at(&[(0, 2), (2, 0), (3, 1)]), None);
}

/// An instruction that reads two qubits under one tag repeats it within
/// itself: it is refused at its own index, and the reason says so rather
/// than naming it as an earlier instruction.
#[test]
fn an_instruction_that_repeats_a_tag_among_its_own_readouts_is_refused() {
    let Err(Rejected::Instruction { at, reason }) = check_unique_tags([(0, 1), (2, 5), (2, 5)])
    else {
        panic!("a repeated tag is refused as `Instruction`");
    };
    assert_eq!(at, 2);
    assert!(
        reason.contains("another readout of this instruction"),
        "{reason}"
    );
}

/// A program measures qubit 0 under tag 4 at instruction 1, then qubit 1
/// under the same tag two instructions later: the refusal names
/// instruction 1, whose readout the tag already names.
#[test]
fn a_refusal_names_the_earlier_instruction_that_read_the_tag() {
    let Err(Rejected::Instruction { at, reason }) = check_unique_tags([(1, 4), (2, 6), (3, 4)])
    else {
        panic!("a repeated tag is refused as `Instruction`");
    };
    assert_eq!(at, 3);
    assert!(reason.contains("the readout of instruction 1"), "{reason}");
}

/// A batch with no readout, or with one, repeats no tag.
#[test]
fn a_batch_of_at_most_one_readout_is_accepted() {
    assert_eq!(refused_at(&[]), None);
    assert_eq!(refused_at(&[(0, 0)]), None);
}

/// The refusal sends the program's author to the contract and to the
/// planned readout register, which a program that repeats a tag needs.
#[test]
fn the_refusal_names_the_contract_and_the_planned_readout_register() {
    let Err(Rejected::Instruction { reason, .. }) = check_unique_tags([(0, 4), (1, 4)]) else {
        panic!("a repeated tag is refused as `Instruction`");
    };
    assert!(reason.contains("docs/replies.md"), "{reason}");
    assert!(reason.contains("planned readout register"), "{reason}");
}
