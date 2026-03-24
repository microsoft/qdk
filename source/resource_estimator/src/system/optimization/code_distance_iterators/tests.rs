// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    increment_code_distance_indexes, iterate_for_code_distances, search_for_code_distances,
    switch_to_non_comparable_code_distance_indexes,
};

#[test]
fn test_switch_to_non_comparable_code_distance_indexes_basic() {
    let mut current = [1, 3, 5];
    let left = [1, 2, 3];
    let right = [10, 10, 10];
    assert!(switch_to_non_comparable_code_distance_indexes(
        &mut current,
        &left,
        &right,
        3
    ));
    assert_eq!(current, [1, 4, 4]);
}

#[test]
fn test_switch_to_non_comparable_code_distance_indexes_overflow() {
    // reaching the max (right)
    let mut current = [1, 7, 9];
    let left = [1, 2, 3];
    let right = [10, 7, 10];
    assert!(switch_to_non_comparable_code_distance_indexes(
        &mut current,
        &left,
        &right,
        3
    ));
    assert_eq!(current, [2, 2, 3]);
}

#[test]
fn test_switch_to_non_comparable_code_distance_indexes_finding_left() {
    let mut current = [1, 4, 5, 5, 6];
    let left = [1, 2, 3, 3, 3];
    let right = [10, 10, 10, 10, 10];
    assert!(switch_to_non_comparable_code_distance_indexes(
        &mut current,
        &left,
        &right,
        5
    ));
    assert_eq!(current, [2, 2, 3, 3, 3]);
}

#[test]
fn test_switch_to_non_comparable_code_distance_indexes_all_max() {
    let mut current = [9, 9, 10];
    let left = [1, 2, 3];
    let right = [10, 10, 10];
    assert!(!switch_to_non_comparable_code_distance_indexes(
        &mut current,
        &left,
        &right,
        3
    ));
}

#[test]
fn test_increment_code_distance_indexes_basic() {
    let mut current = [0, 1, 2];
    let left_code_distance_indexes = [0, 0, 0];
    let right_code_distance_indexes = [3, 3, 3];
    let num_rounds = 3;
    assert!(increment_code_distance_indexes(
        &mut current,
        &left_code_distance_indexes,
        &right_code_distance_indexes,
        num_rounds,
    ));
    assert_eq!(current, [0, 1, 3]);
}

#[test]
fn test_increment_code_distance_indexes_with_overflow() {
    let mut current = [0, 3, 3];
    let left_code_distance_indexes = [0, 1, 2];
    let right_code_distance_indexes = [3, 3, 3];
    let num_rounds = 3;
    assert!(increment_code_distance_indexes(
        &mut current,
        &left_code_distance_indexes,
        &right_code_distance_indexes,
        num_rounds,
    ));
    assert_eq!(current, [1, 1, 2]);
}
#[test]
fn test_increment_code_distance_indexes_with_zero_max_index() {
    let mut current = [3, 3, 3];
    let left_code_distance_indexes = [0, 1, 2];
    let right_code_distance_indexes = [3, 3, 3];
    let num_rounds = 3;
    assert!(!increment_code_distance_indexes(
        &mut current,
        &left_code_distance_indexes,
        &right_code_distance_indexes,
        num_rounds,
    ));
}

// always allow going right
#[test]
fn test_iterate_for_code_distances_1() {
    let num_rounds = 2;
    let left_code_distance_indexes = &[1, 1];
    let right_code_distance_indexes = &[4, 5];
    let initial_code_distance_indexes = &[1, 5];
    let mut points_visited: Vec<Vec<usize>> = Vec::new();
    let mut check_if_can_improve_with_increasing_code_distances = |c: &[usize]| {
        points_visited.push(c.to_vec());
        true
    };

    iterate_for_code_distances(
        num_rounds,
        left_code_distance_indexes,
        right_code_distance_indexes,
        initial_code_distance_indexes,
        &mut check_if_can_improve_with_increasing_code_distances,
    );
    assert_eq!(
        points_visited,
        vec![
            [2, 2],
            [2, 3],
            [2, 4],
            [2, 5],
            [3, 3],
            [3, 4],
            [3, 5],
            [4, 4],
            [4, 5]
        ]
    );
}

// always allow going right but start from left because it is greater than initial
#[test]
fn test_iterate_for_code_distances_2() {
    let num_rounds = 2;
    let left_code_distance_indexes = &[2, 3];
    let right_code_distance_indexes = &[6, 6];
    let initial_code_distance_indexes = &[1, 6];
    let mut points_visited: Vec<Vec<usize>> = Vec::new();
    let mut check_if_can_improve_with_increasing_code_distances = |c: &[usize]| {
        points_visited.push(c.to_vec());
        true
    };

    iterate_for_code_distances(
        num_rounds,
        left_code_distance_indexes,
        right_code_distance_indexes,
        initial_code_distance_indexes,
        &mut check_if_can_improve_with_increasing_code_distances,
    );
    assert_eq!(
        points_visited,
        vec![
            [3, 3],
            [3, 4],
            [3, 5],
            [3, 6],
            [4, 4],
            [4, 5],
            [4, 6],
            [5, 5],
            [5, 6],
            [6, 6]
        ]
    );
}

// do not allow going right at some points
#[test]
fn test_iterate_for_code_distances_3() {
    let num_rounds = 2;
    let left_code_distance_indexes = &[1, 2];
    let right_code_distance_indexes = &[6, 6];
    let initial_code_distance_indexes = &[1, 4];
    let answers = [true, false];
    let mut points_visited: Vec<Vec<usize>> = Vec::new();
    let mut check_if_can_improve_with_increasing_code_distances = |c: &[usize]| {
        points_visited.push(c.to_vec());
        if points_visited.len() <= answers.len() {
            return answers[points_visited.len() - 1];
        }
        true
    };

    iterate_for_code_distances(
        num_rounds,
        left_code_distance_indexes,
        right_code_distance_indexes,
        initial_code_distance_indexes,
        &mut check_if_can_improve_with_increasing_code_distances,
    );

    assert_eq!(points_visited, vec![[2, 2], [2, 3]]);
}

// always allow going right
#[test]
fn test_iterate_for_code_distances_4() {
    let num_rounds = 3;
    let left_code_distance_indexes = &[0, 1, 1];
    let right_code_distance_indexes = &[0, 4, 5];
    let initial_code_distance_indexes = &[0, 1, 5];
    let mut points_visited: Vec<Vec<usize>> = Vec::new();
    let mut check_if_can_improve_with_increasing_code_distances = |c: &[usize]| {
        points_visited.push(c.to_vec());
        true
    };

    iterate_for_code_distances(
        num_rounds,
        left_code_distance_indexes,
        right_code_distance_indexes,
        initial_code_distance_indexes,
        &mut check_if_can_improve_with_increasing_code_distances,
    );
    assert_eq!(
        points_visited,
        vec![
            [0, 2, 2],
            [0, 2, 3],
            [0, 2, 4],
            [0, 2, 5],
            [0, 3, 3],
            [0, 3, 4],
            [0, 3, 5],
            [0, 4, 4],
            [0, 4, 5],
        ]
    );
}

// immediate escape because all are dominated
#[test]
fn test_iterate_for_code_distances_5() {
    let num_rounds = 2;
    let left_code_distance_indexes = &[1, 1];
    let right_code_distance_indexes = &[4, 5];
    let initial_code_distance_indexes = &[1, 1];
    let mut points_visited: Vec<Vec<usize>> = Vec::new();
    let mut check_if_can_improve_with_increasing_code_distances = |c: &[usize]| {
        points_visited.push(c.to_vec());
        true
    };

    iterate_for_code_distances(
        num_rounds,
        left_code_distance_indexes,
        right_code_distance_indexes,
        initial_code_distance_indexes,
        &mut check_if_can_improve_with_increasing_code_distances,
    );
    assert_eq!(points_visited.len(), 0);
}

#[test]
fn test_search_for_code_distances_1() {
    let num_rounds = 2;
    let left_code_distance_indexes = &[0, 0];
    let right_code_distance_indexes = &[7, 7];
    let mut points_visited: Vec<Vec<usize>> = Vec::new();
    let mut check_if_can_improve_with_increasing_code_distances = |c: &[usize]| {
        points_visited.push(c.to_vec());
        true
    };

    assert_eq!(
        search_for_code_distances(
            num_rounds,
            left_code_distance_indexes,
            right_code_distance_indexes,
            &mut check_if_can_improve_with_increasing_code_distances,
        ),
        None
    );

    assert_eq!(points_visited, vec![[7, 7]]);
}

#[test]
fn test_search_for_code_distances_2() {
    let num_rounds = 2;
    let left_code_distance_indexes = &[0, 0];
    let right_code_distance_indexes = &[7, 7];
    let mut points_visited: Vec<Vec<usize>> = Vec::new();
    let mut check_if_can_improve_with_increasing_code_distances = |c: &[usize]| {
        points_visited.push(c.to_vec());
        false
    };

    assert_eq!(
        search_for_code_distances(
            num_rounds,
            left_code_distance_indexes,
            right_code_distance_indexes,
            &mut check_if_can_improve_with_increasing_code_distances,
        ),
        None
    );

    assert_eq!(points_visited, vec![[7, 7], [0, 0]]);
}

#[test]
fn test_search_for_code_distances_3() {
    let num_rounds = 2;
    let left_code_distance_indexes = &[0, 0];
    let right_code_distance_indexes = &[7, 7];
    let mut points_visited: Vec<Vec<usize>> = Vec::new();
    let mut check_if_can_improve_with_increasing_code_distances = |c: &[usize]| {
        points_visited.push(c.to_vec());
        c[0] * 2 + c[1] < 10
    };

    assert_eq!(
        search_for_code_distances(
            num_rounds,
            left_code_distance_indexes,
            right_code_distance_indexes,
            &mut check_if_can_improve_with_increasing_code_distances,
        ),
        Some(vec![2, 6])
    );

    assert_eq!(
        points_visited,
        vec![
            [7, 7],
            [0, 0],
            [3, 7],
            [1, 7],
            [2, 7],
            [2, 4],
            [2, 6],
            [2, 5]
        ]
    );
}

#[test]
fn test_search_for_code_distances_4() {
    let num_rounds = 3;
    let left_code_distance_indexes = &[0, 0, 0];
    let right_code_distance_indexes = &[0, 7, 7];
    let mut points_visited: Vec<Vec<usize>> = Vec::new();
    let mut check_if_can_improve_with_increasing_code_distances = |c: &[usize]| {
        points_visited.push(c.to_vec());
        c[1] * 2 + c[2] < 10
    };

    assert_eq!(
        search_for_code_distances(
            num_rounds,
            left_code_distance_indexes,
            right_code_distance_indexes,
            &mut check_if_can_improve_with_increasing_code_distances,
        ),
        Some(vec![0, 2, 6])
    );

    assert_eq!(
        points_visited,
        vec![
            [0, 7, 7],
            [0, 0, 0],
            [0, 3, 7],
            [0, 1, 7],
            [0, 2, 7],
            [0, 2, 4],
            [0, 2, 6],
            [0, 2, 5]
        ]
    );
}
