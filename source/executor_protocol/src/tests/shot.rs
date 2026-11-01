// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{IntoShotSet, Rejected, ShotSet};

/// A shot set names at least one shot: an empty list or range is refused.
#[test]
fn a_shot_set_names_at_least_one_shot() {
    assert_eq!(ShotSet::new(Vec::new()), Err(Rejected::NoShots));
    let (high, low) = (3, 2);
    assert_eq!(ShotSet::range(high..=low), Err(Rejected::NoShots));
}

/// A shot set names each shot once: a repeated shot is refused, naming it,
/// whether the list is in order or not.
#[test]
fn a_shot_set_names_each_shot_once() {
    assert_eq!(
        ShotSet::new(vec![5, 2, 5, 2]),
        Err(Rejected::RepeatedShot(2))
    );
    assert_eq!(
        ShotSet::new(vec![1, 2, 2, 3]),
        Err(Rejected::RepeatedShot(2))
    );
}

/// A shot set keeps the caller's order, however it is built: the same
/// shots in another order make another set.
#[test]
fn a_shot_set_keeps_the_callers_order() {
    let given = ShotSet::new(vec![4, 1, 3]).expect("4, 1 and 3 are distinct");
    assert_eq!(given.as_slice(), [4, 1, 3]);
    assert_eq!(ShotSet::try_from(&[4, 1, 3][..]), Ok(given.clone()));
    assert_ne!(
        given,
        ShotSet::new(vec![1, 3, 4]).expect("1, 3 and 4 are distinct")
    );
}

/// A subset keeps the set's order, and is `None` when no shot is kept,
/// since a shot set is never empty.
#[test]
fn a_subset_keeps_the_order_and_is_none_when_empty() {
    let all = ShotSet::new(vec![4, 1, 2, 3]).expect("1 to 4 are distinct");
    let even = all.subset(|j| j % 2 == 0).expect("2 and 4 are even");
    assert_eq!(even.as_slice(), [4, 2]);
    assert_eq!(all.subset(|j| j > 4), None);
}

/// An executor of one shot, called on a set with other shots, is refused
/// naming the first other shot in the set's order: shot 1's executor called
/// on shots 5, 1 and 2 is refused at 5, shot 5's at 1, and a set of one on
/// its own shot passes.
#[test]
fn a_set_other_than_the_held_shot_is_refused_naming_its_first_other_shot() {
    let set = ShotSet::new(vec![5, 1, 2]).expect("5, 1 and 2 are distinct");
    assert_eq!(set.check_only(1), Err(Rejected::ShotNotHeld(5)));
    assert_eq!(set.check_only(5), Err(Rejected::ShotNotHeld(1)));
    assert_eq!(ShotSet::one(1).check_only(1), Ok(()));
}

/// Two sets with the same shots in the same order are equal, however each
/// was built: a driver of one shot and a subset of one name the same set.
#[test]
fn sets_with_the_same_shots_are_equal_however_built() {
    let one = ShotSet::one(3);
    assert_eq!(ShotSet::new(vec![3]), Ok(one.clone()));
    let all = ShotSet::new(vec![4, 3]).expect("4 and 3 are distinct");
    assert_eq!(all.subset(|j| j == 3), Some(one));
}

/// A set prints as its shots, in its order, however it holds them: a test
/// that fails on a batch shows the shots it named.
#[test]
fn a_set_prints_as_its_shots_in_its_order() {
    let several = ShotSet::new(vec![4, 3]).expect("4 and 3 are distinct");
    assert_eq!(format!("{:?}", ShotSet::one(3)), "ShotSet([3])");
    assert_eq!(format!("{several:?}"), "ShotSet([4, 3])");
}

/// A set of one shot allocates nothing, since an executor that runs each
/// shot alone makes one per shot per call.
#[test]
fn a_set_of_one_shot_allocates_nothing() {
    let mut one = None;
    let made = crate::tests::allocations(|| one = Some(ShotSet::one(7)));
    assert_eq!(one.as_deref(), Some(&[7][..]), "the set holds shot 7");
    assert_eq!(made, 0);
}

/// A job's shots written as an inclusive range, `2..=4`, are the shots 2,
/// 3 and 4, its end included, as the half-open `2..5` writes them.
#[test]
fn shots_written_as_an_inclusive_range_include_its_end() {
    let three = ShotSet::new(vec![2, 3, 4]).expect("2, 3 and 4 are distinct");
    assert_eq!((2..=4).into_shot_set(), Ok(three.clone()));
    assert_eq!((2..5).into_shot_set(), Ok(three));
}
