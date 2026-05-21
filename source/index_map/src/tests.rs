// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn insert_if_absent_into_empty_returns_true() {
    let mut map: IndexMap<usize, i32> = IndexMap::new();
    assert!(map.insert_if_absent(0, 42));
    assert_eq!(*map.get(0).expect("IndexMap::get: index out of bounds"), 42);
}

#[test]
fn insert_if_absent_occupied_returns_false_preserves_original() {
    let mut map: IndexMap<usize, i32> = IndexMap::new();
    map.insert(0, 42);
    assert!(!map.insert_if_absent(0, 99));
    assert_eq!(*map.get(0).expect("IndexMap::get: index out of bounds"), 42);
}

#[test]
fn insert_if_absent_extends_capacity_for_sparse_key() {
    let mut map: IndexMap<usize, i32> = IndexMap::new();
    assert!(map.insert_if_absent(100, 7));
    assert_eq!(
        *map.get(100).expect("IndexMap::get: index out of bounds"),
        7
    );
    assert!(!map.contains_key(0));
}
