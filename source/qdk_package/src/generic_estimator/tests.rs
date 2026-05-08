// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::factory::round_based::{OrderedBFSControl, ordered_bfs};

#[test]
fn test_ordered_bfs() {
    let mut total_sum = 0;
    let mut total_visits = 0;

    let elements = vec![2, 4, 6, 8, 10];
    ordered_bfs(&elements, 5, |v| {
        let sum = v.iter().copied().sum::<i32>();
        if sum <= 10 {
            total_sum += sum;
            total_visits += 1;
            if v.len() > 3 {
                Ok(OrderedBFSControl::Terminate)
            } else {
                Ok(OrderedBFSControl::Continue)
            }
        } else {
            Ok(OrderedBFSControl::Cutoff)
        }
    })
    .expect("BFS should complete successfully");

    assert_eq!(total_sum, 118);
    assert_eq!(total_visits, 16);
}
