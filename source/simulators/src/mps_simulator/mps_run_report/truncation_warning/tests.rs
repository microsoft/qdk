// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The warning's message: what a user reads when truncation lost more
//! weight than the tolerance.

use crate::mps_simulator::TruncationWarning;

/// The message gives the weight lost, not the weight kept, and the max
/// bond dimension that made the truncations, for the user to raise it: a
/// run that lost 1% of its weight at χ = 16.
#[test]
fn the_message_gives_the_weight_lost_and_the_max_bond_dimension() {
    let lost_weight = 0.01;
    let warning = TruncationWarning {
        lost_weight,
        max_bond_dimension: 16,
    };
    let message = warning.to_string();
    assert!(
        message.contains("max bond dimension 16"),
        "the max bond dimension: {message}"
    );
    let numbers: Vec<f64> = message
        .split(|c: char| !(c.is_ascii_digit() || "+-.eE".contains(c)))
        .filter_map(|token| {
            token
                .trim_matches(|c: char| !c.is_ascii_digit())
                .parse()
                .ok()
        })
        .collect();
    let shows = |weight: f64| {
        numbers
            .iter()
            .any(|number| (number - weight).abs() <= 0.01 * weight)
    };
    assert!(
        shows(lost_weight) && !shows(1.0 - lost_weight),
        "the message gives the weight lost, not the weight kept: {message}"
    );
}
