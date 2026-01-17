use sorted_iter::{assume::AssumeSortedByItemExt, SortedIterator};
use std::borrow::Borrow;

#[must_use]
pub fn union(v1: &[usize], v2: &[usize]) -> Vec<usize> {
    as_sorted_iter(v1)
        .union(as_sorted_iter(v2))
        .copied()
        .collect()
}

#[must_use]
pub fn is_subset(subset: &[usize], superset: &[usize]) -> bool {
    as_sorted_iter(subset).is_subset(as_sorted_iter(superset))
}

#[must_use]
pub fn intersection(v1: &[usize], v2: &[usize]) -> Vec<usize> {
    as_sorted_iter(v1)
        .intersection(as_sorted_iter(v2))
        .copied()
        .collect()
}

#[must_use]
pub fn complement(v: &[usize], index_bound: usize) -> Vec<usize> {
    let values = as_sorted_iter(v).copied().assume_sorted_by_item();
    (0..index_bound).difference(values).collect()
}

#[must_use]
pub fn symmetric_difference(v1: &[usize], v2: &[usize]) -> Vec<usize> {
    as_sorted_iter(v1)
        .symmetric_difference(as_sorted_iter(v2))
        .copied()
        .collect()
}

#[test]
fn intersection_test() {
    assert_eq!(intersection(&[1, 2, 3, 6], &[3, 4, 5, 10]), vec![3]);
    assert_eq!(intersection(&[1, 2, 3, 6], &[2, 3, 4, 5, 10]), vec![2, 3]);
}

#[test]
fn symmetric_difference_test() {
    assert_eq!(
        symmetric_difference(&[1, 2, 3, 6], &[3, 4, 5, 10]),
        vec![1, 2, 4, 5, 6, 10]
    );
    assert_eq!(
        symmetric_difference(&[1, 2, 3, 6], &[2, 3, 4, 5, 10]),
        vec![1, 4, 5, 6, 10]
    );
}

fn as_sorted_iter<Value, Values>(values: Values) -> impl SortedIterator<Item = Value>
where
    Value: Borrow<usize> + Copy,
    Values: IntoIterator<Item = Value> + Copy,
{
    debug_assert!(is_ascending(values));
    values.into_iter().assume_sorted_by_item()
}

fn is_ascending<Value, Values>(values: Values) -> bool
where
    Value: Borrow<usize> + Copy,
    Values: IntoIterator<Item = Value>,
{
    values
        .into_iter()
        .is_sorted_by(|a, b| a.borrow() < b.borrow())
}

#[test]
fn union_test() {
    assert_eq!(
        union(&[1, 2, 3, 6], &[3, 4, 5, 10]),
        vec![1, 2, 3, 4, 5, 6, 10]
    );
    assert_eq!(
        union(&[1, 2, 3, 6], &[2, 3, 4, 5, 10]),
        vec![1, 2, 3, 4, 5, 6, 10]
    );
}

#[test]
fn complement_test() {
    assert_eq!(complement(&[1, 2, 3, 6], 7), vec![0, 4, 5]);
    assert_eq!(complement(&[0, 1, 2, 3, 6], 8), vec![4, 5, 7]);
}

#[test]
fn is_subset_test() {
    assert!(is_subset(&[1, 2, 3], &[0, 1, 2, 3, 5]));
    assert!(!is_subset(&[1, 2, 3], &[0, 1, 3, 5]));
}
