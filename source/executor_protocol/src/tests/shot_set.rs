// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot set is never empty, names no shot twice, keeps the caller's order,
//! and holds one shot inline; `select` splits one into sound sets.

use super::allocations;
use crate::{ShotId, ShotSet, ShotSetFault};

/// The ids of `set`, in its order.
fn ids_of(set: &ShotSet) -> Vec<u32> {
    set.iter().map(ShotId::get).collect()
}

/// A driver given no shots learns it at once, rather than sending a batch
/// that would run nothing.
#[test]
fn no_shots_is_refused() {
    assert_eq!(ShotSet::try_ids([]), Err(ShotSetFault::NoShots));
}

/// A shot named twice is refused, naming the first shot, in the order
/// given, that an earlier one names: 5, here, not the least repeated, 2.
#[test]
fn a_repeated_shot_is_refused_naming_the_first_repeat() {
    assert_eq!(
        ShotSet::try_ids([5, 2, 5, 2]),
        Err(ShotSetFault::RepeatedShot(ShotId::new(5)))
    );
}

/// A repeat is found whatever the order: after an increasing run, where the
/// ids stop increasing at the repeat itself.
#[test]
fn a_repeat_after_increasing_ids_is_refused() {
    assert_eq!(
        ShotSet::try_ids([1, 2, 3, 3]),
        Err(ShotSetFault::RepeatedShot(ShotId::new(3)))
    );
}

/// `ids` panics with the fault `try_ids` returns.
#[test]
#[should_panic(expected = "shot 4 is named twice in a shot set")]
fn ids_panics_on_a_repeated_shot() {
    let _ = ShotSet::ids([4, 4]);
}

/// `ids` panics on no shots, with the fault `try_ids` returns.
#[test]
#[should_panic(expected = "a shot set needs at least one shot")]
fn ids_panics_on_no_shots() {
    let _ = ShotSet::ids([]);
}

/// A set keeps the caller's order, since a batch's rows come back in it:
/// decreasing, interleaved, and two ranges joined.
#[test]
fn a_set_keeps_the_callers_order() {
    assert_eq!(ids_of(&ShotSet::ids([9, 2, 7, 0])), [9, 2, 7, 0]);
    let joined = ShotSet::ids((0..3).chain(8..10));
    assert_eq!(ids_of(&joined), [0, 1, 2, 8, 9]);
}

/// A set's length is its number of shots, so a part sizes a batch's
/// readouts by it.
#[test]
fn len_counts_the_shots() {
    assert_eq!(ShotSet::ids([9, 2, 7, 0]).len(), 4);
    assert_eq!(ShotSet::ids([3]).len(), 1);
}

/// Two sets are equal when they hold the same shots in the same order,
/// however they were made; the same shots in another order are another
/// set.
#[test]
fn sets_are_equal_by_their_shots_in_order() {
    let all = ShotSet::ids([3, 1, 4]);
    let selected = ShotSet::ids([8, 3, 1, 4])
        .select(&[1, 2, 3])
        .expect("three positions select three shots");
    assert_eq!(selected, all);
    assert_ne!(ShotSet::ids([1, 3, 4]), all);
}

/// `select` takes the shots at the positions given, in the positions'
/// order, so a driver can split a batch by its readouts in any order.
#[test]
fn select_takes_the_shots_at_the_positions_in_their_order() {
    let set = ShotSet::ids([10, 20, 30, 40]);
    let part = set
        .select(&[3, 0, 2])
        .expect("three positions select three shots");
    assert_eq!(ids_of(&part), [40, 10, 30]);
}

/// Selecting no position gives no set, since a set is never empty.
#[test]
fn select_of_no_position_is_none() {
    assert_eq!(ShotSet::ids([1, 2]).select(&[]), None);
}

/// A position given twice would name its shot twice, which a set refuses.
#[test]
#[should_panic(expected = "position 1 is selected twice")]
fn select_panics_on_a_position_given_twice() {
    let _ = ShotSet::ids([1, 2, 3]).select(&[1, 0, 1]);
}

/// A position beyond the set names no shot.
#[test]
#[should_panic(expected = "position 2 is beyond the set's 2 shots")]
fn select_panics_on_a_position_beyond_the_set() {
    let _ = ShotSet::ids([1, 2]).select(&[0, 2]);
}

/// Adaptive programs send many batches of one shot: such a set, made from
/// ids or selected from a larger one, allocates nothing.
#[test]
fn a_one_shot_set_allocates_nothing() {
    let set = ShotSet::ids([1, 2, 3]);
    let count = allocations(|| {
        let one = ShotSet::ids([7]);
        let selected = set.select(&[2]);
        drop((one, selected));
    });
    assert_eq!(count, 0);
}

/// Increasing ids, as ranges give, are checked with no allocation beyond
/// the set's own buffer.
#[test]
fn increasing_ids_allocate_only_the_set() {
    let count = allocations(|| drop(ShotSet::ids(0..1000)));
    assert_eq!(count, 1);
}

/// A failing test shows the shots a set names, in order.
#[test]
fn debug_lists_the_shots_in_order() {
    assert_eq!(format!("{:?}", ShotSet::ids([2, 5])), "ShotSet([2, 5])");
}

/// A table of results lines up its shots: a width, fill and alignment pad
/// a shot as they pad its number.
#[test]
fn a_shot_is_padded_as_its_number() {
    assert_eq!(
        format!("{:>4}|{:*<3}", ShotId::new(7), ShotId::new(12)),
        "   7|12*"
    );
}
