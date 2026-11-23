// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A shot set is never empty, names no shot twice, keeps the caller's order,
//! and holds one shot inline; `subset` splits one into sound sets.

use crate::tests::allocations;
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
/// given, that an earlier one names, whatever the order: 5, here, not the
/// least repeated, 2; and after an increasing run, where the ids stop
/// increasing at the repeat itself.
#[test]
fn a_repeated_shot_is_refused_naming_the_first_repeat() {
    for (ids, repeat) in [(vec![5, 2, 5, 2], 5), (vec![1, 2, 3, 3], 3)] {
        assert_eq!(
            ShotSet::try_ids(ids.clone()),
            Err(ShotSetFault::RepeatedShot(ShotId::new(repeat))),
            "{ids:?}"
        );
    }
}

/// `ids` panics with the fault `try_ids` returns, on a repeated shot and
/// on no shots, so a caller that does not handle the fault reads it in the
/// panic; the message of a repeated shot names that shot.
#[test]
fn ids_panics_with_the_fault_try_ids_returns() {
    for (ids, named) in [(vec![4, 4], Some("4")), (vec![], None)] {
        let fault = ShotSet::try_ids(ids.clone()).expect_err("not a set");
        let panic = std::panic::catch_unwind(|| ShotSet::ids(ids.clone()))
            .expect_err(&format!("{ids:?} panics"));
        let message = panic.downcast_ref::<String>().map(String::as_str);
        assert_eq!(message, Some(fault.to_string().as_str()), "{ids:?}");
        if let Some(shot) = named {
            assert!(fault.to_string().contains(shot), "{ids:?}: {fault}");
        }
    }
}

/// A set keeps the caller's order, since a batch's rows come back in it:
/// decreasing, interleaved, and two ranges joined.
#[test]
fn a_set_keeps_the_callers_order() {
    assert_eq!(ids_of(&ShotSet::ids([9, 2, 7, 0])), [9, 2, 7, 0]);
    let joined = ShotSet::ids((0..3).chain(8..10));
    assert_eq!(ids_of(&joined), [0, 1, 2, 8, 9]);
}

/// Two sets are equal when they hold the same shots in the same order,
/// however they were made; the same shots in another order are another
/// set.
#[test]
fn sets_are_equal_by_their_shots_in_order() {
    let all = ShotSet::ids([3, 1, 4]);
    let subset = ShotSet::ids([8, 3, 1, 4])
        .subset(&[1, 2, 3])
        .expect("three positions give three shots");
    assert_eq!(subset, all);
    assert_ne!(ShotSet::ids([1, 3, 4]), all);
}

/// `subset` takes the shots at the positions given, in the positions'
/// order, so a driver can split a batch by its readouts in any order.
#[test]
fn subset_takes_the_shots_at_the_positions_in_their_order() {
    let set = ShotSet::ids([10, 20, 30, 40]);
    let part = set
        .subset(&[3, 0, 2])
        .expect("three positions give three shots");
    assert_eq!(ids_of(&part), [40, 10, 30]);
}

/// A subset of no position is no set, since a set is never empty.
#[test]
fn subset_of_no_position_is_none() {
    assert_eq!(ShotSet::ids([1, 2]).subset(&[]), None);
}

/// A position given twice would name its shot twice, which a set refuses:
/// the panic names the position.
#[test]
#[should_panic(expected = "position 1")]
fn subset_panics_on_a_position_given_twice() {
    let _ = ShotSet::ids([1, 2, 3]).subset(&[1, 0, 1]);
}

/// A position beyond the set names no shot: the panic names it.
#[test]
#[should_panic(expected = "position 2")]
fn subset_panics_on_a_position_beyond_the_set() {
    let _ = ShotSet::ids([1, 2]).subset(&[0, 2]);
}

/// Adaptive programs send many batches of one shot: such a set, made from
/// ids or taken as a subset of a larger one, allocates nothing.
#[test]
fn a_one_shot_set_allocates_nothing() {
    let set = ShotSet::ids([1, 2, 3]);
    let count = allocations(|| {
        let one = ShotSet::ids([7]);
        let subset = set.subset(&[2]);
        drop((one, subset));
    });
    assert_eq!(count, 0);
}

/// Increasing ids, as ranges give, are checked with no allocation beyond
/// the set's own buffer.
#[test]
fn increasing_ids_allocate_only_the_set() {
    let count = allocations(|| drop(ShotSet::ids(0..1000)));
    assert!(count <= 1, "{count} allocations");
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
